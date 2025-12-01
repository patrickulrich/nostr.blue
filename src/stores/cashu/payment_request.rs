//! Payment requests (NUT-18)
//!
//! Functions for creating and paying payment requests.
//! Supports both Nostr transport (NIP-17 gift wrap) and HTTP transport.
//!
//! Uses CDK's native PaymentRequest types for NUT-18 compliance.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use std::str::FromStr;

use dioxus::prelude::*;
use nostr_sdk::{EventId, Kind, PublicKey};

// CDK NUT-18 types
use cdk::nuts::{
    CurrencyUnit, PaymentRequest, PaymentRequestPayload as CdkPaymentRequestPayload,
    Transport, TransportType,
};
use cdk::mint_url::MintUrl;
use cdk::Amount;

use crate::utils::shorten_url;
use super::events::queue_event_for_retry;
use super::types::PendingEventType;
use super::internal::create_ephemeral_wallet;
use super::mint_mgmt::{get_mint_balance, get_mints};
use super::proofs::{cdk_proof_to_proof_data, proof_data_to_cdk_proof, register_proofs_in_event_map};
use super::signals::{
    try_acquire_mint_lock, PAYMENT_REQUEST_PROGRESS, PENDING_PAYMENT_REQUESTS, WALLET_BALANCE,
    WALLET_TOKENS,
};
use super::types::{
    ExtendedCashuProof, ExtendedTokenEvent, NostrPaymentWaitInfo,
    PaymentRequestProgress, ProofData, TokenData, WalletTokensStoreStoreExt,
};
use super::utils::mint_matches;
use crate::stores::{auth_store, nostr_client};

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
        "Creating payment request: amount={:?}, nostr={}",
        amount,
        use_nostr_transport
    );

    let mints = get_mints();
    if mints.is_empty() {
        return Err("No mints available. Add a mint first.".to_string());
    }

    let request_id = uuid::Uuid::new_v4().to_string()[..8].to_string();

    // Build transport using CDK's Transport builder
    let (transports, nostr_info): (Vec<Transport>, Option<NostrPaymentWaitInfo>) =
        if use_nostr_transport {
            // Generate ephemeral keys for receiving
            let keys = nostr_sdk::Keys::generate();

            // Get user's relays
            let relays = crate::services::profile_search::get_user_relays().await;
            if relays.is_empty() {
                return Err("No relays configured for Nostr transport".to_string());
            }

            // Create nprofile with relays
            let relay_urls: Vec<nostr_sdk::RelayUrl> = relays
                .iter()
                .filter_map(|r| nostr_sdk::RelayUrl::parse(r).ok())
                .collect();

            let nprofile =
                nostr_sdk::nips::nip19::Nip19Profile::new(keys.public_key(), relay_urls);

            let nprofile_str = nprofile
                .to_bech32()
                .map_err(|e| format!("Failed to encode nprofile: {}", e))?;

            // Use CDK's Transport builder
            let transport = Transport::builder()
                .transport_type(TransportType::Nostr)
                .target(nprofile_str)
                .tags(vec![vec!["n".to_string(), "17".to_string()]]) // NIP-17 gift wrap
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

    // Convert mint strings to MintUrl
    let mint_urls: Vec<MintUrl> = mints
        .iter()
        .filter_map(|m| MintUrl::from_str(m).ok())
        .collect();

    // Build request using CDK's PaymentRequest builder
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

    // CDK's PaymentRequest implements Display for encoding (creqA...)
    let request_string = request.to_string();

    // Store wait info for later if Nostr transport is enabled
    if let Some(ref info) = nostr_info {
        PENDING_PAYMENT_REQUESTS
            .write()
            .insert(request_id, info.clone());
    }

    log::info!(
        "Created payment request: {}",
        &request_string[..50.min(request_string.len())]
    );

    Ok((request_string, nostr_info))
}

/// Parse a payment request string (creqA...)
///
/// Uses CDK's PaymentRequest FromStr implementation for NUT-18 compliance.
pub fn parse_payment_request(request_string: &str) -> Result<PaymentRequest, String> {
    let request_string = request_string.trim();

    // CDK's PaymentRequest implements FromStr with proper prefix and CBOR handling
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

    // Parse the request using CDK's PaymentRequest
    let request = parse_payment_request(&request_string)?;

    // Determine amount (CDK uses Amount type)
    let amount: u64 = match (request.amount, custom_amount) {
        (Some(amt), _) => u64::from(amt),
        (None, Some(amt)) => amt,
        (None, None) => return Err("Amount required but not specified in request or provided".to_string()),
    };

    if amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }

    // Find a compatible mint (CDK uses MintUrl type)
    let our_mints = get_mints();
    let compatible_mint = if let Some(ref accepted_mints) = request.mints {
        let accepted_strings: Vec<String> = accepted_mints.iter().map(|m| m.to_string()).collect();
        our_mints
            .iter()
            .find(|m| accepted_strings.iter().any(|am| mint_matches(m, am)))
            .cloned()
    } else {
        // If no mints specified, use our first mint
        our_mints.first().cloned()
    };

    let mint_url = compatible_mint
        .ok_or("No compatible mint found. You don't have tokens from any of the accepted mints.")?;

    // Check balance
    let balance = get_mint_balance(&mint_url);
    if balance < amount {
        return Err(format!(
            "Insufficient balance at {}. Have: {} sats, need: {} sats",
            shorten_url(&mint_url, 30),
            balance,
            amount
        ));
    }

    // Acquire lock
    let _lock = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;

    // Get proofs and send
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

    // Prepare send
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

    // Get remaining proofs
    let keep_proofs = wallet
        .get_unspent_proofs()
        .await
        .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;

    // Get keysets to extract proofs from token
    let keysets_info = wallet
        .get_mint_keysets()
        .await
        .map_err(|e| format!("Failed to get keysets: {}", e))?;

    // Convert token proofs to our format
    let proofs = token
        .proofs(&keysets_info)
        .map_err(|e| format!("Failed to extract proofs from token: {}", e))?;

    // Build payload using CDK's PaymentRequestPayload
    let mint_url_parsed = MintUrl::from_str(&mint_url)
        .map_err(|e| format!("Invalid mint URL: {}", e))?;

    let payload = CdkPaymentRequestPayload {
        id: request.payment_id.clone(),
        memo: None,
        mint: mint_url_parsed,
        unit: CurrencyUnit::Sat,
        proofs: proofs.clone(),
    };

    // Find transport - prefer Nostr for privacy, fall back to HTTP
    let transport = request
        .transports
        .iter()
        .find(|t| t._type == TransportType::Nostr)
        .or_else(|| {
            request
                .transports
                .iter()
                .find(|t| t._type == TransportType::HttpPost)
        });

    if let Some(transport) = transport {
        match transport._type {
            TransportType::Nostr => {
                // Send via Nostr gift wrap
                log::info!("Sending payment via Nostr transport");

                // Parse nprofile
                let nprofile = Nip19Profile::from_bech32(&transport.target)
                    .map_err(|e| format!("Invalid nprofile: {}", e))?;

                // Create ephemeral client
                let ephemeral_keys = nostr_sdk::Keys::generate();
                let client = nostr_sdk::Client::new(ephemeral_keys);

                // Add relays
                for relay in &nprofile.relays {
                    if let Err(e) = client.add_write_relay(relay.clone()).await {
                        log::warn!("Failed to add relay {}: {}", relay, e);
                    }
                }

                client.connect().await;

                // Create rumor (kind 14 - gift wrap payload)
                let payload_json = serde_json::to_string(&payload)
                    .map_err(|e| format!("Failed to serialize payload: {}", e))?;

                let rumor = nostr_sdk::EventBuilder::new(nostr_sdk::Kind::from_u16(14), payload_json)
                    .build(nprofile.public_key);

                // Send gift wrap
                let result = client
                    .gift_wrap_to(nprofile.relays.clone(), &nprofile.public_key, rumor, None)
                    .await
                    .map_err(|e| format!("Failed to send gift wrap: {}", e))?;

                log::info!(
                    "Payment sent via Nostr: {} successes, {} failures",
                    result.success.len(),
                    result.failed.len()
                );

                if result.success.is_empty() {
                    return Err("Failed to deliver payment to any relay".to_string());
                }
            }
            TransportType::HttpPost => {
                // Send via HTTP POST
                log::info!("Sending payment via HTTP transport to {}", transport.target);

                let http_client = reqwest::Client::new();
                let response = http_client
                    .post(&transport.target)
                    .json(&payload)
                    .send()
                    .await
                    .map_err(|e| format!("HTTP request failed: {}", e))?;

                if !response.status().is_success() {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    return Err(format!(
                        "HTTP request failed with status {}: {}",
                        status, body
                    ));
                }

                log::info!("Payment sent via HTTP");
            }
        }
    } else {
        return Err("No transport available in payment request. Cannot deliver payment.".to_string());
    }

    // Convert CDK proofs to local ProofData for state tracking
    let _token_proofs: Vec<ProofData> = proofs.iter().map(cdk_proof_to_proof_data).collect();

    // Update local state - remove old tokens, add remaining
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey =
        PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    // Publish new token event (remaining proofs)
    let mut new_event_id: Option<String> = None;
    if !keep_proofs.is_empty() {
        let proof_data: Vec<ProofData> = keep_proofs.iter().map(cdk_proof_to_proof_data).collect();

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

        let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);

        match client.send_event_builder(builder.clone()).await {
            Ok(event_output) => {
                new_event_id = Some(event_output.id().to_hex());
            }
            Err(e) => {
                log::warn!("Failed to publish token event: {}", e);
                queue_event_for_retry(builder, PendingEventType::TokenEvent).await;
            }
        }
    } else {
        // No remaining proofs - publish deletion
        if !event_ids_to_delete.is_empty() {
            use nostr::nips::nip09::EventDeletionRequest;
            let mut deletion_request = EventDeletionRequest::new();
            for event_id_str in &event_ids_to_delete {
                if let Ok(event_id) = EventId::parse(event_id_str) {
                    deletion_request = deletion_request.id(event_id);
                }
            }

            let builder = nostr_sdk::EventBuilder::delete(deletion_request);
            if let Err(e) = client.send_event_builder(builder.clone()).await {
                log::warn!("Failed to publish deletion event: {}", e);
                queue_event_for_retry(builder, PendingEventType::DeletionEvent).await;
            }
        }
    }

    // Update local state
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens = data.write();

        // Remove old tokens
        tokens.retain(|t| !event_ids_to_delete.contains(&t.event_id));

        // Add remaining tokens
        if !keep_proofs.is_empty() {
            let proof_data: Vec<ProofData> =
                keep_proofs.iter().map(cdk_proof_to_proof_data).collect();

            let event_id =
                new_event_id.unwrap_or_else(|| format!("local-{}", chrono::Utc::now().timestamp()));
            tokens.push(TokenData {
                event_id: event_id.clone(),
                mint: mint_url,
                unit: "sat".to_string(),
                proofs: proof_data.clone(),
                created_at: chrono::Utc::now().timestamp() as u64,
            });

            // Register proofs in event map
            register_proofs_in_event_map(&event_id, &proof_data);
        }
    }

    // Update balance
    {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let new_balance: u64 = tokens
            .iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .fold(0u64, |acc, amount| acc.saturating_add(amount));
        *WALLET_BALANCE.write() = new_balance;
    }

    log::info!("Payment request paid: {} sats", amount);

    Ok(amount)
}

/// Wait for a Nostr payment for a created request
///
/// This listens for gift-wrapped events on the relays and processes
/// incoming payments.
pub async fn wait_for_nostr_payment(request_id: String, timeout_secs: u64) -> Result<u64, String> {
    use nostr_sdk::prelude::*;

    log::info!("Waiting for Nostr payment for request: {}", request_id);

    *PAYMENT_REQUEST_PROGRESS.write() = Some(PaymentRequestProgress::WaitingForPayment);

    // Get wait info
    let wait_info = PENDING_PAYMENT_REQUESTS
        .read()
        .get(&request_id)
        .cloned()
        .ok_or("No pending request found for this ID")?;

    // Create client with ephemeral keys
    let keys = nostr_sdk::Keys::new(wait_info.secret_key);
    let client = nostr_sdk::Client::new(keys);

    // Add relays
    for relay in &wait_info.relays {
        if let Err(e) = client.add_read_relay(relay.clone()).await {
            log::warn!("Failed to add relay {}: {}", relay, e);
        }
    }

    client.connect().await;

    // Subscribe to events for our pubkey
    let filter = Filter::new().pubkey(wait_info.pubkey);
    client
        .subscribe(filter, None)
        .await
        .map_err(|e| format!("Failed to subscribe: {}", e))?;

    // Wait for notifications with timeout
    let start = chrono::Utc::now().timestamp() as u64;
    let mut notifications = client.notifications();

    loop {
        // Check if cancelled (request removed from pending map by cancel_payment_request)
        if !PENDING_PAYMENT_REQUESTS.read().contains_key(&request_id) {
            return Err("Payment request cancelled".to_string());
        }

        // Check timeout
        let elapsed = chrono::Utc::now().timestamp() as u64 - start;
        if elapsed > timeout_secs {
            *PAYMENT_REQUEST_PROGRESS.write() = Some(PaymentRequestProgress::Cancelled);
            PENDING_PAYMENT_REQUESTS.write().remove(&request_id);
            return Err("Timeout waiting for payment".to_string());
        }

        // Wait for next notification with timeout
        let notification = {
            #[cfg(target_arch = "wasm32")]
            {
                use futures::future::{select, Either};
                use futures::pin_mut;
                use gloo_timers::future::TimeoutFuture;

                let timeout_fut = TimeoutFuture::new(5000); // 5 second intervals
                let recv_fut = notifications.recv();
                pin_mut!(timeout_fut);
                pin_mut!(recv_fut);

                match select(recv_fut, timeout_fut).await {
                    Either::Left((Ok(n), _)) => Some(n),
                    Either::Left((Err(_), _)) => break, // Channel closed
                    Either::Right((_, _)) => continue,  // Timeout, check again
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    notifications.recv(),
                )
                .await
                {
                    Ok(Ok(n)) => Some(n),
                    Ok(Err(_)) => break,  // Channel closed
                    Err(_) => continue,   // Timeout, check again
                }
            }
        };

        if let Some(RelayPoolNotification::Event { event, .. }) = notification {
            // Try to unwrap gift wrap
            match client.unwrap_gift_wrap(&event).await {
                Ok(unwrapped) => {
                    let rumor = unwrapped.rumor;

                    // Try to parse payload (using CDK's PaymentRequestPayload type)
                    match serde_json::from_str::<CdkPaymentRequestPayload>(&rumor.content) {
                        Ok(payload) => {
                            log::info!("Received payment payload: {} proofs", payload.proofs.len());

                            // Calculate amount with overflow protection
                            // CDK's Proof.amount is Amount type, convert via u64::from()
                            let amount: u64 = payload.proofs
                                .iter()
                                .map(|p| u64::from(p.amount))
                                .try_fold(0u64, |acc, amt| acc.checked_add(amt))
                                .unwrap_or(u64::MAX); // Cap at max if overflow

                            // Convert CDK proofs to ProofData
                            let proof_data: Vec<ProofData> = payload.proofs
                                .iter()
                                .map(cdk_proof_to_proof_data)
                                .collect();

                            // Receive the tokens (mint is MintUrl, convert to string)
                            let mint_str = payload.mint.to_string();
                            match receive_payment_proofs(&mint_str, proof_data).await {
                                Ok(_) => {
                                    *PAYMENT_REQUEST_PROGRESS.write() =
                                        Some(PaymentRequestProgress::Received { amount });
                                    PENDING_PAYMENT_REQUESTS.write().remove(&request_id);
                                    return Ok(amount);
                                }
                                Err(e) => {
                                    log::error!("Failed to receive payment proofs: {}", e);
                                    // Continue listening - might be a different payment
                                }
                            }
                        }
                        Err(e) => {
                            log::debug!("Failed to parse payment payload: {}", e);
                            // Continue listening
                        }
                    }
                }
                Err(e) => {
                    log::debug!("Failed to unwrap gift wrap: {}", e);
                    // Continue listening
                }
            }
        }
    }

    // Check if we exited due to cancellation (request already removed)
    if !PENDING_PAYMENT_REQUESTS.read().contains_key(&request_id) {
        return Err("Payment request cancelled".to_string());
    }

    *PAYMENT_REQUEST_PROGRESS.write() = Some(PaymentRequestProgress::Error {
        message: "Connection closed".to_string(),
    });
    PENDING_PAYMENT_REQUESTS.write().remove(&request_id);
    Err("Connection closed while waiting for payment".to_string())
}

/// Receive proofs from a payment request payload
async fn receive_payment_proofs(mint_url: &str, proofs: Vec<ProofData>) -> Result<u64, String> {
    use nostr_sdk::signer::NostrSigner;

    log::info!("Receiving {} proofs from {}", proofs.len(), mint_url);

    // Convert to CDK proofs
    let cdk_proofs: Vec<cdk::nuts::Proof> = proofs
        .iter()
        .map(proof_data_to_cdk_proof)
        .collect::<Result<Vec<_>, _>>()?;

    // Calculate amount with overflow protection
    let amount: u64 = cdk_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Amount overflow")?;

    // Acquire lock
    let _lock = try_acquire_mint_lock(mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;

    // Create wallet and receive
    let wallet = create_ephemeral_wallet(mint_url, cdk_proofs.clone()).await?;

    // Swap proofs to ensure they're ours (contacts mint)
    // Use swap with the proofs to get fresh ones
    let swapped = wallet
        .swap(
            None, // amount - None means all
            cdk::amount::SplitTarget::default(),
            cdk_proofs.clone(),
            None,  // spending_conditions
            true,  // include_fees
        )
        .await
        .map_err(|e| format!("Failed to swap proofs: {}", e))?;

    // Get final proofs
    let final_proofs = if let Some(swap_result) = swapped {
        swap_result
    } else {
        // Swap returned None, meaning proofs were already valid
        cdk_proofs
    };

    // Publish to Nostr
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey =
        PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;

    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    let proof_data: Vec<ProofData> = final_proofs.iter().map(cdk_proof_to_proof_data).collect();

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
        Ok(event_output) => Some(event_output.id().to_hex()),
        Err(e) => {
            log::warn!("Failed to publish token event: {}", e);
            queue_event_for_retry(builder, PendingEventType::TokenEvent).await;
            None
        }
    };

    // Update local state
    {
        let store = WALLET_TOKENS.read();
        let mut data = store.data();
        let mut tokens = data.write();

        let event_id =
            new_event_id.unwrap_or_else(|| format!("local-{}", chrono::Utc::now().timestamp()));
        tokens.push(TokenData {
            event_id: event_id.clone(),
            mint: mint_url.to_string(),
            unit: "sat".to_string(),
            proofs: proof_data.clone(),
            created_at: chrono::Utc::now().timestamp() as u64,
        });

        // Register proofs in event map
        register_proofs_in_event_map(&event_id, &proof_data);
    }

    // Update balance
    {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        let new_balance: u64 = tokens
            .iter()
            .flat_map(|t| &t.proofs)
            .map(|p| p.amount)
            .fold(0u64, |acc, amount| acc.saturating_add(amount));
        *WALLET_BALANCE.write() = new_balance;
    }

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
