//! Payment requests (NUT-18)
//!
//! Functions for creating and paying payment requests.
//! Supports both Nostr transport (NIP-17 gift wrap) and HTTP transport.
//!
//! Uses CDK's native PaymentRequest types for NUT-18 compliance.
#![allow(dead_code)]

#[cfg(all(feature = "web", feature = "native"))]
compile_error!("Cannot enable both 'web' and 'native' features simultaneously");

#[cfg(not(any(feature = "web", feature = "native")))]
compile_error!("Must enable either 'web' or 'native' feature");

use crate::platform::http::http_client;
/// Error message returned when a payment request is cancelled by the user.
/// This is NOT an error condition - it indicates a clean shutdown.
pub const PAYMENT_CANCELLED_MSG: &str = "Payment request cancelled";
use std::str::FromStr;
use dioxus::prelude::*;
use nostr_sdk::{EventId, Kind, PublicKey};
use cdk::mint_url::MintUrl;
use cdk::nuts::{
    CurrencyUnit, PaymentRequest, PaymentRequestPayload as CdkPaymentRequestPayload,
    Transport, TransportType,
};
use cdk::Amount;
use super::events::queue_event_for_retry;
use super::internal::create_ephemeral_wallet;
use super::mint_mgmt::{get_mint_balance, get_mints};
use super::proofs::{
    cdk_proof_to_proof_data, proof_data_to_cdk_proof, register_proofs_in_event_map,
};
use super::signals::{
    try_acquire_mint_lock, PAYMENT_REQUEST_PROGRESS, PENDING_PAYMENT_REQUESTS,
    WALLET_TOKENS,
};
use super::types::PendingEventType;
use super::types::{
    ExtendedCashuProof, ExtendedTokenEvent, NostrPaymentWaitInfo, PaymentRequestProgress,
    ProofData, TokenData, WalletTokensStoreStoreExt,
};
use super::utils::{mint_matches, normalize_mint_url};
use crate::stores::{auth_store, nostr_client};
use crate::utils::shorten_url;
/// Create a payment request (NUT-18)
///
/// Returns the request string (creqA...) and optionally NostrPaymentWaitInfo
/// if Nostr transport is enabled.
///
/// Uses CDK's PaymentRequest builder for NUT-18 compliance.
pub async fn create_payment_request(
    amount: Option<u64>,
    description: Option<String>,
    use_nostr_transport: bool,
) -> Result<(String, Option<NostrPaymentWaitInfo>), String> {
    use nostr_sdk::ToBech32;
    log::info!(
        "Creating payment request: amount={:?}, nostr={}", amount, use_nostr_transport
    );
    let mints = get_mints();
    if mints.is_empty() {
        return Err("No mints available. Add a mint first.".to_string());
    }
    let request_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let (transports, nostr_info): (Vec<Transport>, Option<NostrPaymentWaitInfo>) = if use_nostr_transport {
        let keys = nostr_sdk::Keys::generate();
        let relays = crate::services::profile_search::get_user_relays().await;
        if relays.is_empty() {
            return Err("No relays configured for Nostr transport".to_string());
        }
        let relay_urls: Vec<nostr_sdk::RelayUrl> = relays
            .iter()
            .filter_map(|r| nostr_sdk::RelayUrl::parse(r).ok())
            .collect();
        let nprofile = nostr_sdk::nips::nip19::Nip19Profile::new(
            keys.public_key(),
            relay_urls,
        );
        let nprofile_str = nprofile
            .to_bech32()
            .map_err(|e| format!("Failed to encode nprofile: {}", e))?;
        let transport = Transport::builder()
            .transport_type(TransportType::Nostr)
            .target(nprofile_str)
            .tags(vec![vec!["n".to_string(), "17".to_string()]])
            .build()
            .map_err(|e| format!("Failed to build transport: {}", e))?;
        let wait_info = NostrPaymentWaitInfo {
            request_id: request_id.clone(),
            secret_key: keys.secret_key().clone(),
            relays,
            pubkey: keys.public_key(),
        };
        (vec![transport], Some(wait_info))
    } else {
        (vec![], None)
    };
    let mint_urls: Vec<MintUrl> = mints
        .iter()
        .filter_map(|m| MintUrl::from_str(m).ok())
        .collect();
    let mut builder = PaymentRequest::builder()
        .payment_id(&request_id)
        .unit(CurrencyUnit::Sat)
        .single_use(true)
        .mints(mint_urls);
    if let Some(amt) = amount {
        builder = builder.amount(Amount::from(amt));
    }
    if let Some(desc) = description {
        builder = builder.description(desc);
    }
    if !transports.is_empty() {
        builder = builder.transports(transports);
    }
    let request = builder.build();
    let request_string = request.to_string();
    if let Some(ref info) = nostr_info {
        PENDING_PAYMENT_REQUESTS.write().insert(request_id, info.clone());
    }
    log::info!(
        "Created payment request: {}", & request_string[..50.min(request_string.len())]
    );
    Ok((request_string, nostr_info))
}
/// Parse a payment request string (creqA...)
///
/// Uses CDK's PaymentRequest FromStr implementation for NUT-18 compliance.
pub fn parse_payment_request(request_string: &str) -> Result<PaymentRequest, String> {
    let request_string = request_string.trim();
    PaymentRequest::from_str(request_string)
        .map_err(|e| format!("Failed to parse payment request: {}", e))
}
/// Pay a payment request
///
/// Parses the request, prepares tokens, and sends via the appropriate transport.
/// Uses CDK's PaymentRequest type for NUT-18 compliance.
pub async fn pay_payment_request(
    request_string: String,
    custom_amount: Option<u64>,
) -> Result<u64, String> {
    use nostr_sdk::nips::nip19::Nip19Profile;
    use nostr_sdk::signer::NostrSigner;
    use nostr_sdk::FromBech32;
    log::info!("Paying payment request");
    let request = parse_payment_request(&request_string)?;
    let amount: u64 = match (request.amount, custom_amount) {
        (Some(amt), _) => u64::from(amt),
        (None, Some(amt)) => amt,
        (None, None) => {
            return Err(
                "Amount required but not specified in request or provided".to_string(),
            );
        }
    };
    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }
    let our_mints = get_mints();
    let compatible_mint = if let Some(ref accepted_mints) = request.mints {
        let accepted_strings: Vec<String> = accepted_mints
            .iter()
            .map(|m| m.to_string())
            .collect();
        our_mints
            .iter()
            .find(|m| accepted_strings.iter().any(|am| mint_matches(m, am)))
            .cloned()
    } else {
        our_mints.first().cloned()
    };
    let mint_url = compatible_mint
        .ok_or(
            "No compatible mint found. You don't have tokens from any of the accepted mints.",
        )?;
    let balance = get_mint_balance(&mint_url);
    if balance < amount {
        return Err(
            format!(
                "Insufficient balance at {}. Have: {} sats, need: {} sats",
                shorten_url(&mint_url, 30),
                balance,
                amount,
            ),
        );
    }
    let _lock = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| {
            format!("Another operation is in progress for mint: {}", mint_url)
        })?;
    let (all_proofs, event_ids_to_delete) = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let mint_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| mint_matches(&t.mint, &mint_url))
            .collect();
        let mut all_proofs = Vec::new();
        let mut event_ids = Vec::new();
        for token in &mint_tokens {
            event_ids.push(token.event_id.clone());
            for proof in &token.proofs {
                all_proofs.push(proof_data_to_cdk_proof(proof)?);
            }
        }
        (all_proofs, event_ids)
    };
    let wallet = create_ephemeral_wallet(&mint_url, all_proofs).await?;
    let prepared = wallet
        .prepare_send(
            cdk::Amount::from(amount),
            cdk::wallet::SendOptions {
                include_fee: true,
                ..Default::default()
            },
        )
        .await
        .map_err(|e| format!("Failed to prepare send: {}", e))?;
    let token = prepared
        .confirm(None)
        .await
        .map_err(|e| format!("Failed to confirm send: {}", e))?;
    let keep_proofs = wallet
        .get_unspent_proofs()
        .await
        .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;
    let keysets_info = wallet
        .get_mint_keysets()
        .await
        .map_err(|e| format!("Failed to get keysets: {}", e))?;
    let proofs = token
        .proofs(&keysets_info)
        .map_err(|e| format!("Failed to extract proofs from token: {}", e))?;
    let mint_url_parsed = MintUrl::from_str(&mint_url)
        .map_err(|e| format!("Invalid mint URL: {}", e))?;
    let payload = CdkPaymentRequestPayload {
        id: request.payment_id.clone(),
        memo: None,
        mint: mint_url_parsed,
        unit: CurrencyUnit::Sat,
        proofs: proofs.clone(),
    };
    let transport = request
        .transports
        .iter()
        .find(|t| t._type == TransportType::Nostr)
        .or_else(|| {
            request.transports.iter().find(|t| t._type == TransportType::HttpPost)
        });
    let token_proof_secrets: Vec<String> = proofs
        .iter()
        .map(|p| p.secret.to_string())
        .collect();
    super::proofs::register_proofs_pending_at_mint(&token_proof_secrets);
    if let Some(transport) = transport {
        match transport._type {
            TransportType::Nostr => {
                log::info!("Sending payment via Nostr transport");
                let nprofile = Nip19Profile::from_bech32(&transport.target)
                    .map_err(|e| format!("Invalid nprofile: {}", e))?;
                let ephemeral_keys = nostr_sdk::Keys::generate();
                let client = nostr_sdk::Client::new(ephemeral_keys);
                for relay in &nprofile.relays {
                    if let Err(e) = client.add_write_relay(relay.clone()).await {
                        log::warn!("Failed to add relay {}: {}", relay, e);
                    }
                }
                client.connect().await;
                let payload_json = serde_json::to_string(&payload)
                    .map_err(|e| format!("Failed to serialize payload: {}", e))?;
                let rumor = nostr_sdk::EventBuilder::new(
                        nostr_sdk::Kind::from_u16(14),
                        payload_json,
                    )
                    .build(nprofile.public_key);
                let result = client
                    .gift_wrap_to(
                        nprofile.relays.clone(),
                        &nprofile.public_key,
                        rumor,
                        None,
                    )
                    .await
                    .map_err(|e| format!("Failed to send gift wrap: {}", e))?;
                log::info!(
                    "Payment sent via Nostr: {} successes, {} failures", result.success
                    .len(), result.failed.len()
                );
                if result.success.is_empty() {
                    log::warn!("Nostr transport failed, syncing proof states with mint");
                    let _ = super::recovery::sync_proofs_with_mints().await;
                    super::proofs::revert_proofs_to_spendable(&token_proof_secrets);
                    return Err("Failed to deliver payment to any relay".to_string());
                }
            }
            TransportType::HttpPost => {
                log::info!("Sending payment via HTTP transport to {}", transport.target);
                let response = http_client()
                    .post(&transport.target)
                    .json(&payload)
                    .send()
                    .await
                    .map_err(|e| format!("HTTP request failed: {}", e))?;
                if !response.status().is_success() {
                    log::warn!("HTTP transport failed, syncing proof states with mint");
                    let _ = super::recovery::sync_proofs_with_mints().await;
                    super::proofs::revert_proofs_to_spendable(&token_proof_secrets);
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    return Err(
                        format!("HTTP request failed with status {}: {}", status, body),
                    );
                }
                log::info!("Payment sent via HTTP");
            }
        }
    } else {
        super::proofs::revert_proofs_to_spendable(&token_proof_secrets);
        return Err(
            "No transport available in payment request. Cannot deliver payment."
                .to_string(),
        );
    }
    super::proofs::move_proofs_to_spent(&token_proof_secrets);
    let _token_proofs: Vec<ProofData> = proofs
        .iter()
        .map(cdk_proof_to_proof_data)
        .collect();
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let mut new_event_id: Option<String> = None;
    if !keep_proofs.is_empty() {
        let proof_data: Vec<ProofData> = keep_proofs
            .iter()
            .map(cdk_proof_to_proof_data)
            .collect();
        let extended_proofs: Vec<ExtendedCashuProof> = proof_data
            .iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();
        let token_event_data = ExtendedTokenEvent {
            mint: mint_url.clone(),
            unit: "sat".to_string(),
            proofs: extended_proofs,
            del: event_ids_to_delete.clone(),
        };
        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize token event: {}", e))?;
        let encrypted = signer
            .nip44_encrypt(&pubkey, &json_content)
            .await
            .map_err(|e| format!("Failed to encrypt token event: {}", e))?;
        let builder = nostr_sdk::EventBuilder::new(
            Kind::CashuWalletUnspentProof,
            encrypted,
        );
        new_event_id = Some(
            match client.send_event_builder(builder.clone()).await {
                Ok(event_output) => {
                    if event_output.success.is_empty() {
                        log::warn!("No relays accepted token event, queuing for retry");
                        let pending_id = format!("pending_{}", uuid::Uuid::new_v4());
                        queue_event_for_retry(
                                builder,
                                PendingEventType::TokenEvent,
                                Some(pending_id.clone()),
                                Some(mint_url.clone()),
                            )
                            .await;
                        pending_id
                    } else {
                        event_output.id().to_hex()
                    }
                }
                Err(e) => {
                    log::warn!("Failed to publish token event: {}", e);
                    let pending_id = format!("pending_{}", uuid::Uuid::new_v4());
                    queue_event_for_retry(
                            builder,
                            PendingEventType::TokenEvent,
                            Some(pending_id.clone()),
                            Some(mint_url.clone()),
                        )
                        .await;
                    pending_id
                }
            },
        );
    } else if !event_ids_to_delete.is_empty() {
        use nostr::nips::nip09::EventDeletionRequest;
        let mut deletion_request = EventDeletionRequest::new();
        for event_id_str in &event_ids_to_delete {
            if let Ok(event_id) = EventId::parse(event_id_str) {
                deletion_request = deletion_request.id(event_id);
            }
        }
        let builder = nostr_sdk::EventBuilder::delete(deletion_request);
        match client.send_event_builder(builder.clone()).await {
            Ok(output) => {
                if output.success.is_empty() {
                    log::warn!(
                        "No relays accepted deletion event, queuing for retry"
                    );
                    queue_event_for_retry(
                            builder,
                            PendingEventType::DeletionEvent,
                            None,
                            None,
                        )
                        .await;
                }
            }
            Err(e) => {
                log::warn!("Failed to publish deletion event: {}", e);
                queue_event_for_retry(
                        builder,
                        PendingEventType::DeletionEvent,
                        None,
                        None,
                    )
                    .await;
            }
        }
    }
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens = data.write();
        tokens.retain(|t| !event_ids_to_delete.contains(&t.event_id));
        if !keep_proofs.is_empty() {
            let proof_data: Vec<ProofData> = keep_proofs
                .iter()
                .map(cdk_proof_to_proof_data)
                .collect();
            let event_id = new_event_id
                .unwrap_or_else(|| format!("local-{}", chrono::Utc::now().timestamp()));
            tokens
                .push(TokenData {
                    event_id: event_id.clone(),
                    mint: mint_url,
                    unit: "sat".to_string(),
                    proofs: proof_data.clone(),
                    created_at: chrono::Utc::now().timestamp() as u64,
                });
            register_proofs_in_event_map(&event_id, &proof_data);
        }
    }
    super::signals::update_wallet_balances();
    log::info!("Payment request paid: {} sats", amount);
    Ok(amount)
}
/// Wait for a Nostr payment for a created request
///
/// This listens for gift-wrapped events on the relays and processes
/// incoming payments.
pub async fn wait_for_nostr_payment(
    request_id: String,
    timeout_secs: u64,
) -> Result<u64, String> {
    use nostr_sdk::prelude::*;
    log::info!("Waiting for Nostr payment for request: {}", request_id);
    *PAYMENT_REQUEST_PROGRESS.write() = Some(PaymentRequestProgress::WaitingForPayment);
    let wait_info = PENDING_PAYMENT_REQUESTS
        .read()
        .get(&request_id)
        .cloned()
        .ok_or("No pending request found for this ID")?;
    let keys = nostr_sdk::Keys::new(wait_info.secret_key);
    let client = nostr_sdk::Client::new(keys);
    for relay in &wait_info.relays {
        if let Err(e) = client.add_read_relay(relay.clone()).await {
            log::warn!("Failed to add relay {}: {}", relay, e);
        }
    }
    client.connect().await;
    let filter = Filter::new().pubkey(wait_info.pubkey);
    client
        .subscribe(filter, None)
        .await
        .map_err(|e| format!("Failed to subscribe: {}", e))?;
    let start = chrono::Utc::now().timestamp() as u64;
    let mut notifications = client.notifications();
    loop {
        if !PENDING_PAYMENT_REQUESTS.read().contains_key(&request_id) {
            return Err(PAYMENT_CANCELLED_MSG.to_string());
        }
        let elapsed = chrono::Utc::now().timestamp() as u64 - start;
        if elapsed > timeout_secs {
            *PAYMENT_REQUEST_PROGRESS.write() = Some(PaymentRequestProgress::Cancelled);
            PENDING_PAYMENT_REQUESTS.write().remove(&request_id);
            return Err("Timeout waiting for payment".to_string());
        }
        let notification = {
            #[cfg(feature = "web")]
            {
                use futures::future::{select, Either};
                use futures::pin_mut;
                let timeout_fut = crate::platform::timer::sleep_ms(5000);
                let recv_fut = notifications.recv();
                pin_mut!(timeout_fut);
                pin_mut!(recv_fut);
                match select(recv_fut, timeout_fut).await {
                    Either::Left((Ok(n), _)) => Some(n),
                    Either::Left((Err(_), _)) => break,
                    Either::Right((_, _)) => continue,
                }
            }
            #[cfg(feature = "native")]
            {
                use futures::future::{select, Either};
                use futures::pin_mut;
                use std::time::Duration;
                let timeout_fut = crate::platform::timer::sleep(Duration::from_secs(5));
                let recv_fut = notifications.recv();
                pin_mut!(timeout_fut);
                pin_mut!(recv_fut);
                match select(recv_fut, timeout_fut).await {
                    Either::Left((Ok(n), _)) => Some(n),
                    Either::Left((Err(_), _)) => break,
                    Either::Right((_, _)) => continue,
                }
            }
        };
        if let Some(RelayPoolNotification::Event { event, .. }) = notification {
            match client.unwrap_gift_wrap(&event).await {
                Ok(unwrapped) => {
                    let rumor = unwrapped.rumor;
                    match serde_json::from_str::<
                        CdkPaymentRequestPayload,
                    >(&rumor.content) {
                        Ok(payload) => {
                            log::info!(
                                "Received payment payload: {} proofs", payload.proofs.len()
                            );
                            let amount: u64 = payload
                                .proofs
                                .iter()
                                .map(|p| u64::from(p.amount))
                                .try_fold(0u64, |acc, amt| acc.checked_add(amt))
                                .unwrap_or(u64::MAX);
                            let proof_data: Vec<ProofData> = payload
                                .proofs
                                .iter()
                                .map(cdk_proof_to_proof_data)
                                .collect();
                            let mint_str = payload.mint.to_string();
                            match receive_payment_proofs(&mint_str, proof_data).await {
                                Ok(_) => {
                                    *PAYMENT_REQUEST_PROGRESS.write() = Some(PaymentRequestProgress::Received {
                                        amount,
                                    });
                                    PENDING_PAYMENT_REQUESTS.write().remove(&request_id);
                                    return Ok(amount);
                                }
                                Err(e) => {
                                    log::error!("Failed to receive payment proofs: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            log::debug!("Failed to parse payment payload: {}", e);
                        }
                    }
                }
                Err(e) => {
                    log::debug!("Failed to unwrap gift wrap: {}", e);
                }
            }
        }
    }
    if !PENDING_PAYMENT_REQUESTS.read().contains_key(&request_id) {
        return Err(PAYMENT_CANCELLED_MSG.to_string());
    }
    *PAYMENT_REQUEST_PROGRESS.write() = Some(PaymentRequestProgress::Error {
        message: "Connection closed".to_string(),
    });
    PENDING_PAYMENT_REQUESTS.write().remove(&request_id);
    Err("Connection closed while waiting for payment".to_string())
}
/// Receive proofs from a payment request payload
async fn receive_payment_proofs(
    mint_url: &str,
    proofs: Vec<ProofData>,
) -> Result<u64, String> {
    use nostr_sdk::signer::NostrSigner;
    let mint_url = normalize_mint_url(mint_url);
    log::info!("Receiving {} proofs from {}", proofs.len(), mint_url);
    let cdk_proofs: Vec<cdk::nuts::Proof> = proofs
        .iter()
        .map(proof_data_to_cdk_proof)
        .collect::<Result<Vec<_>, _>>()?;
    let amount: u64 = cdk_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Amount overflow")?;
    let _lock = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| {
            format!("Another operation is in progress for mint: {}", mint_url)
        })?;
    let wallet = create_ephemeral_wallet(&mint_url, cdk_proofs.clone()).await?;
    let swapped = wallet
        .swap(None, cdk::amount::SplitTarget::default(), cdk_proofs.clone(), None, true)
        .await
        .map_err(|e| format!("Failed to swap proofs: {}", e))?;
    let final_proofs = swapped
        .ok_or("Swap validation failed - proofs rejected by mint")?;
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let proof_data: Vec<ProofData> = final_proofs
        .iter()
        .map(cdk_proof_to_proof_data)
        .collect();
    let extended_proofs: Vec<ExtendedCashuProof> = proof_data
        .iter()
        .map(|p| ExtendedCashuProof::from(p.clone()))
        .collect();
    let token_event_data = ExtendedTokenEvent {
        mint: mint_url.to_string(),
        unit: "sat".to_string(),
        proofs: extended_proofs,
        del: vec![],
    };
    let json_content = serde_json::to_string(&token_event_data)
        .map_err(|e| format!("Failed to serialize token event: {}", e))?;
    let encrypted = signer
        .nip44_encrypt(&pubkey, &json_content)
        .await
        .map_err(|e| format!("Failed to encrypt token event: {}", e))?;
    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);
    let new_event_id = match client.send_event_builder(builder.clone()).await {
        Ok(event_output) => {
            if event_output.success.is_empty() {
                log::warn!("No relays accepted token event, queuing for retry");
                let pending_id = format!("pending_{}", uuid::Uuid::new_v4());
                queue_event_for_retry(
                        builder,
                        PendingEventType::TokenEvent,
                        Some(pending_id.clone()),
                        Some(mint_url.to_string()),
                    )
                    .await;
                pending_id
            } else {
                event_output.id().to_hex()
            }
        }
        Err(e) => {
            log::warn!("Failed to publish token event: {}", e);
            let pending_id = format!("pending_{}", uuid::Uuid::new_v4());
            queue_event_for_retry(
                    builder,
                    PendingEventType::TokenEvent,
                    Some(pending_id.clone()),
                    Some(mint_url.to_string()),
                )
                .await;
            pending_id
        }
    };
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens = data.write();
        let event_id = new_event_id;
        tokens
            .push(TokenData {
                event_id: event_id.clone(),
                mint: mint_url.to_string(),
                unit: "sat".to_string(),
                proofs: proof_data.clone(),
                created_at: chrono::Utc::now().timestamp() as u64,
            });
        register_proofs_in_event_map(&event_id, &proof_data);
    }
    super::signals::update_wallet_balances();
    log::info!("Received {} sats from payment request", amount);
    Ok(amount)
}
/// Cancel waiting for a payment request
pub fn cancel_payment_request(request_id: &str) {
    PENDING_PAYMENT_REQUESTS.write().remove(request_id);
    *PAYMENT_REQUEST_PROGRESS.write() = Some(PaymentRequestProgress::Cancelled);
}
/// Alias for API compatibility
pub fn cancel_payment_request_wait(request_id: &str) {
    cancel_payment_request(request_id);
}
