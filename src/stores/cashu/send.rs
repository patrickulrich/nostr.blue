//! Send operations
//!
//! Functions for sending ecash tokens, including P2PK sends.
use super::events::{queue_event_for_retry, queue_signed_event_for_retry};
use super::internal::{
    cleanup_spent_proofs_internal, create_ephemeral_wallet, is_insufficient_funds_error_string,
    is_token_spent_error_string, nostr_pubkey_to_cdk_pubkey, validate_proofs_with_mint,
};
use super::proofs::{cdk_proof_to_proof_data, proof_data_to_cdk_proof};
use super::signals::{try_acquire_mint_lock, WALLET_STATE, WALLET_TOKENS};
use super::types::PendingEventType;
use super::types::{
    ExtendedCashuProof, ExtendedTokenEvent, InFlightSendRequest, ProofData, TokenData,
    WalletTokensStoreStoreExt,
};
use super::utils::{mint_matches, normalize_mint_url, now_secs};
use crate::stores::{auth_store, cashu_cdk_bridge, nostr_client};
use dioxus::prelude::*;
use nostr_sdk::signer::NostrSigner;
use nostr_sdk::{EventId, Kind, PublicKey};
/// Estimate the fee for sending a given amount from a mint
///
/// Returns the estimated fee in sats, or an error if estimation fails.
/// This uses CDK's prepare_send to get the exact fee that would be charged.
pub async fn estimate_send_fee(mint_url: String, amount: u64) -> Result<u64, String> {
    use cdk::wallet::{SendKind, SendOptions};
    use cdk::Amount;
    let mint_url = normalize_mint_url(&mint_url);
    let all_proofs = get_proofs_for_mint(&mint_url)?;
    if all_proofs.is_empty() {
        return Err("No tokens found for this mint".to_string());
    }
    let total_available: u64 = all_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Balance overflow")?;
    if total_available < amount {
        return Err(format!(
            "Insufficient funds. Available: {} sats",
            total_available
        ));
    }
    let wallet = create_ephemeral_wallet(&mint_url, all_proofs).await?;
    let prepared = wallet
        .prepare_send(
            Amount::from(amount),
            SendOptions {
                conditions: None,
                include_fee: true,
                send_kind: SendKind::OnlineTolerance(Amount::from(1)),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| format!("Failed to estimate fee: {}", e))?;
    let fee = u64::from(prepared.fee());
    log::debug!(
        "Estimated fee for {} sats from {}: {} sats",
        amount,
        mint_url,
        fee
    );
    prepared
        .cancel()
        .await
        .map_err(|e| format!("Failed to cancel prepared send: {}", e))?;
    Ok(fee)
}
/// Send ecash tokens
pub async fn send_tokens(mint_url: String, amount: u64) -> Result<String, String> {
    let mint_url = normalize_mint_url(&mint_url);
    log::info!("Sending {} sats from {}", amount, mint_url);
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;
    let all_proofs = get_proofs_for_mint(&mint_url)?;
    if all_proofs.is_empty() {
        return Err("No tokens found for this mint".to_string());
    }
    let all_proofs = validate_proofs_with_mint(&mint_url, all_proofs).await?;
    let event_ids_to_delete = get_event_ids_for_mint(&mint_url);
    let total_available: u64 = all_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Balance overflow")?;
    if total_available < amount {
        return Err(format!(
            "Insufficient funds. Available: {} sats, Required: {} sats",
            total_available, amount,
        ));
    }
    let tx_id = format!("send_{}", uuid::Uuid::new_v4());
    let proof_secrets: Vec<String> = all_proofs.iter().map(|p| p.secret.to_string()).collect();
    let in_flight = InFlightSendRequest {
        transaction_id: tx_id.clone(),
        mint_url: mint_url.clone(),
        proof_secrets,
        amount,
        operation_type: super::types::OperationType::Send,
        created_at: now_secs(),
    };
    super::signals::add_in_flight_send_request(in_flight);
    let result = super::internal::try_operation_or_recover(
        &mint_url,
        all_proofs.clone(),
        execute_send_with_retry(&mint_url, amount, all_proofs, None),
    )
    .await;
    let (token_string, keep_proofs) = match result {
        Ok(r) => r,
        Err(e) => {
            super::signals::remove_in_flight_send_request(&tx_id);
            return Err(e);
        }
    };
    let pending_event_id = format!("pending_{}", uuid::Uuid::new_v4());
    if let Err(e) = update_local_state_after_send(
        &mint_url,
        &keep_proofs,
        &event_ids_to_delete,
        &Some(pending_event_id.clone()),
    ) {
        super::signals::remove_in_flight_send_request(&tx_id);
        return Err(e);
    }
    super::signals::remove_in_flight_send_request(&tx_id);
    let new_event_id = match publish_send_events(
        &mint_url,
        &keep_proofs,
        &event_ids_to_delete,
        &pending_event_id,
    )
    .await
    {
        Ok(Some(event_id)) => Some(event_id),
        Ok(None) => {
            if !keep_proofs.is_empty() {
                log::error!(
                    "publish_send_events returned None but keep_proofs has {} entries",
                    keep_proofs.len()
                );
            }
            debug_assert!(
                keep_proofs.is_empty(),
                "publish_send_events returned None but keep_proofs is non-empty",
            );
            None
        }
        Err(e) => {
            if let Err(sync_err) = cashu_cdk_bridge::sync_wallet_state().await {
                log::warn!(
                    "Failed to sync wallet state after send publish failure: {}",
                    sync_err
                );
            }
            return Err(e);
        }
    };
    let valid_created: Vec<String> = new_event_id
        .iter()
        .filter(|id| !id.starts_with("pending_"))
        .cloned()
        .collect();
    let valid_destroyed: Vec<String> = event_ids_to_delete
        .iter()
        .filter(|id| !id.starts_with("pending_") && EventId::from_hex(id).is_ok())
        .cloned()
        .collect();
    if !valid_created.is_empty() || !valid_destroyed.is_empty() {
        if let Err(e) =
            super::events::create_history_event("out", amount, valid_created, valid_destroyed).await
        {
            log::error!("Failed to create history event: {}", e);
        }
    }
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after send: {}", e);
    }
    Ok(token_string)
}
/// Send ecash tokens locked to a recipient's public key (P2PK / NUT-11)
///
/// This creates tokens that can only be spent by the holder of the corresponding
/// private key. The recipient can be specified as:
/// - A hex pubkey (64 chars)
/// - An npub (bech32 encoded)
pub async fn send_tokens_p2pk(
    mint_url: String,
    amount: u64,
    recipient_pubkey: String,
) -> Result<String, String> {
    use cdk::nuts::SpendingConditions;
    let mint_url = normalize_mint_url(&mint_url);
    log::info!(
        "Sending {} sats P2PK to {} from {}",
        amount,
        recipient_pubkey,
        mint_url
    );
    let cdk_pubkey = nostr_pubkey_to_cdk_pubkey(&recipient_pubkey)?;
    let spending_conditions = SpendingConditions::new_p2pk(cdk_pubkey, None);
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;
    let all_proofs = get_proofs_for_mint(&mint_url)?;
    if all_proofs.is_empty() {
        return Err("No tokens found for this mint".to_string());
    }
    let all_proofs = validate_proofs_with_mint(&mint_url, all_proofs).await?;
    let event_ids_to_delete = get_event_ids_for_mint(&mint_url);
    let total_available: u64 = all_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Balance overflow")?;
    if total_available < amount {
        return Err(format!(
            "Insufficient funds. Available: {} sats, Required: {} sats",
            total_available, amount,
        ));
    }
    let tx_id = format!("send_p2pk_{}", uuid::Uuid::new_v4());
    let proof_secrets: Vec<String> = all_proofs.iter().map(|p| p.secret.to_string()).collect();
    let in_flight = InFlightSendRequest {
        transaction_id: tx_id.clone(),
        mint_url: mint_url.clone(),
        proof_secrets,
        amount,
        operation_type: super::types::OperationType::SendP2pk,
        created_at: now_secs(),
    };
    super::signals::add_in_flight_send_request(in_flight);
    let result = super::internal::try_operation_or_recover(
        &mint_url,
        all_proofs.clone(),
        execute_p2pk_send_via_swap(&mint_url, amount, all_proofs, spending_conditions),
    )
    .await;
    let (token_string, keep_proofs) = match result {
        Ok(r) => r,
        Err(e) => {
            super::signals::remove_in_flight_send_request(&tx_id);
            return Err(e);
        }
    };
    let pending_event_id = format!("pending_{}", uuid::Uuid::new_v4());
    if let Err(e) = update_local_state_after_send(
        &mint_url,
        &keep_proofs,
        &event_ids_to_delete,
        &Some(pending_event_id.clone()),
    ) {
        super::signals::remove_in_flight_send_request(&tx_id);
        return Err(e);
    }
    super::signals::remove_in_flight_send_request(&tx_id);
    let new_event_id = match publish_send_events(
        &mint_url,
        &keep_proofs,
        &event_ids_to_delete,
        &pending_event_id,
    )
    .await
    {
        Ok(Some(event_id)) => Some(event_id),
        Ok(None) => {
            if !keep_proofs.is_empty() {
                log::error!(
                    "publish_send_events returned None but keep_proofs has {} entries",
                    keep_proofs.len()
                );
            }
            debug_assert!(
                keep_proofs.is_empty(),
                "publish_send_events returned None but keep_proofs is non-empty",
            );
            None
        }
        Err(e) => {
            return Err(e);
        }
    };
    let valid_created: Vec<String> = new_event_id
        .iter()
        .filter(|id| !id.starts_with("pending_"))
        .cloned()
        .collect();
    let valid_destroyed: Vec<String> = event_ids_to_delete
        .iter()
        .filter(|id| !id.starts_with("pending_") && EventId::from_hex(id).is_ok())
        .cloned()
        .collect();
    if !valid_created.is_empty() || !valid_destroyed.is_empty() {
        if let Err(e) =
            super::events::create_history_event("out", amount, valid_created, valid_destroyed).await
        {
            log::error!("Failed to create history event: {}", e);
        }
    }
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!(
            "Failed to sync MultiMintWallet state after P2PK send: {}",
            e
        );
    }
    log::info!(
        "P2PK send complete: {} sats locked to {}",
        amount,
        recipient_pubkey
    );
    Ok(token_string)
}
/// Get the wallet's P2PK public key for receiving locked tokens
///
/// Returns the hex-encoded public key derived from the wallet's private key.
pub fn get_wallet_pubkey() -> Result<String, String> {
    let wallet_state = WALLET_STATE.read();
    let state = wallet_state.as_ref().ok_or("Wallet not initialized")?;
    let privkey = state
        .privkey
        .as_ref()
        .ok_or("Wallet private key not available")?;
    let secret_key = cdk::nuts::SecretKey::from_hex(privkey)
        .map_err(|e| format!("Invalid wallet privkey: {}", e))?;
    let pubkey = secret_key.public_key();
    Ok(pubkey.to_hex())
}
/// Get CDK proofs for a specific mint
fn get_proofs_for_mint(mint_url: &str) -> Result<Vec<cdk::nuts::Proof>, String> {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();
    let mut all_proofs = Vec::new();
    for token in tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url) && !t.pending_publish)
    {
        for proof in &token.proofs {
            all_proofs.push(proof_data_to_cdk_proof(proof)?);
        }
    }
    Ok(all_proofs)
}
/// Get event IDs for a specific mint's tokens
fn get_event_ids_for_mint(mint_url: &str) -> Vec<String> {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();
    tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url) && !t.pending_publish)
        .map(|t| t.event_id.clone())
        .collect()
}
/// Execute send with auto-retry on spent proofs
async fn execute_send_with_retry(
    mint_url: &str,
    amount: u64,
    all_proofs: Vec<cdk::nuts::Proof>,
    spending_conditions: Option<cdk::nuts::SpendingConditions>,
) -> Result<(String, Vec<cdk::nuts::Proof>), String> {
    use cdk::wallet::{SendKind, SendOptions};
    use cdk::Amount;
    let result = async {
        let wallet = create_ephemeral_wallet(mint_url, all_proofs.clone()).await?;
        let prepared = wallet
            .prepare_send(
                Amount::from(amount),
                SendOptions {
                    conditions: spending_conditions.clone(),
                    include_fee: true,
                    send_kind: SendKind::OnlineTolerance(Amount::from(1)),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        let fee = u64::from(prepared.fee());
        log::info!("Send fee: {} sats", fee);
        let token = prepared.confirm(None).await.map_err(|e| e.to_string())?;
        let keep_proofs = wallet.get_unspent_proofs().await.map_err(|e| e.to_string())?;
        let initial_total: u64 = all_proofs
            .iter()
            .map(|p| u64::from(p.amount))
            .fold(0u64, |acc, amt| acc.saturating_add(amt));
        let actual_change: u64 = keep_proofs
            .iter()
            .map(|p| u64::from(p.amount))
            .fold(0u64, |acc, amt| acc.saturating_add(amt));
        let expected_change = initial_total.saturating_sub(amount).saturating_sub(fee);
        if actual_change != expected_change {
            log::warn!(
                "Change amount mismatch: expected {} sats, got {} sats (initial: {}, sent: {}, fee: {})",
                expected_change, actual_change, initial_total, amount, fee
            );
        }
        Ok::<(cdk::nuts::Token, Vec<cdk::nuts::Proof>), String>((token, keep_proofs))
    }
        .await;
    match result {
        Ok((token, proofs)) => Ok((token.to_string(), proofs)),
        Err(e) => {
            if is_token_spent_error_string(&e) || is_insufficient_funds_error_string(&e) {
                log::warn!("Send failed ({}), cleaning up and retrying...", e);
                let (cleaned_count, cleaned_amount) =
                    cleanup_spent_proofs_internal(mint_url).await?;
                log::info!(
                    "Cleaned up {} spent proofs worth {} sats, retrying send",
                    cleaned_count,
                    cleaned_amount
                );
                let fresh_proofs = get_proofs_for_mint(mint_url)?;
                let fresh_total: u64 = fresh_proofs
                    .iter()
                    .map(|p| u64::from(p.amount))
                    .try_fold(0u64, |acc, amt| acc.checked_add(amt))
                    .ok_or("Balance overflow")?;
                if fresh_total < amount {
                    return Err(format!(
                        "Insufficient funds after cleanup. Available: {} sats, Required: {} sats",
                        fresh_total, amount,
                    ));
                }
                let wallet = create_ephemeral_wallet(mint_url, fresh_proofs).await?;
                let prepared = wallet
                    .prepare_send(
                        Amount::from(amount),
                        SendOptions {
                            conditions: spending_conditions,
                            include_fee: true,
                            send_kind: SendKind::OnlineTolerance(Amount::from(1)),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| format!("Retry failed: {}", e))?;
                let fee = u64::from(prepared.fee());
                let token = prepared
                    .confirm(None)
                    .await
                    .map_err(|e| format!("Retry confirm failed: {}", e))?;
                let keep_proofs = wallet
                    .get_unspent_proofs()
                    .await
                    .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;
                let actual_change: u64 = keep_proofs
                    .iter()
                    .map(|p| u64::from(p.amount))
                    .fold(0u64, |acc, amt| acc.saturating_add(amt));
                let expected_change = fresh_total.saturating_sub(amount).saturating_sub(fee);
                if actual_change != expected_change {
                    log::warn!(
                        "Change amount mismatch (retry): expected {} sats, got {} sats (initial: {}, sent: {}, fee: {})",
                        expected_change, actual_change, fresh_total, amount, fee
                    );
                }
                log::info!("Send succeeded after cleanup and retry");
                Ok((token.to_string(), keep_proofs))
            } else {
                Err(format!("Failed to send: {}", e))
            }
        }
    }
}
/// Execute P2PK send using wallet.swap() directly
///
/// This bypasses CDK's prepare_send() which has a bug in proof selection for P2PK.
/// By using swap() directly with our own proof selection, we avoid the issue where
/// CDK only looks for bearer proofs when creating P2PK tokens.
async fn execute_p2pk_send_via_swap(
    mint_url: &str,
    amount: u64,
    all_proofs: Vec<cdk::nuts::Proof>,
    spending_conditions: cdk::nuts::SpendingConditions,
) -> Result<(String, Vec<cdk::nuts::Proof>), String> {
    use cdk::amount::SplitTarget;
    use cdk::nuts::{CurrencyUnit, Token};
    use cdk::Amount;
    let result = async {
        let wallet = create_ephemeral_wallet(mint_url, all_proofs.clone()).await?;
        let fee = wallet
            .get_proofs_fee(&all_proofs)
            .await
            .map_err(|e| format!("Failed to calculate fee: {}", e))?;
        let fee_u64 = u64::from(fee);
        log::info!("P2PK send fee: {} sats", fee_u64);
        let send_proofs = wallet
            .swap(
                Some(Amount::from(amount)),
                SplitTarget::default(),
                all_proofs.clone(),
                Some(spending_conditions.clone()),
                true,
            )
            .await
            .map_err(|e| format!("Swap failed: {}", e))?
            .ok_or("Swap returned no proofs")?;
        let keep_proofs = wallet
            .get_unspent_proofs()
            .await
            .map_err(|e| format!("Failed to get change proofs: {}", e))?;
        let mint_url_parsed: cdk::mint_url::MintUrl = mint_url
            .parse()
            .map_err(|e| format!("Invalid mint URL: {}", e))?;
        let token = Token::new(mint_url_parsed, send_proofs, None, CurrencyUnit::Sat);
        let initial_total: u64 = all_proofs
            .iter()
            .map(|p| u64::from(p.amount))
            .fold(0u64, |acc, amt| acc.saturating_add(amt));
        let actual_change: u64 = keep_proofs
            .iter()
            .map(|p| u64::from(p.amount))
            .fold(0u64, |acc, amt| acc.saturating_add(amt));
        let expected_change = initial_total
            .saturating_sub(amount)
            .saturating_sub(fee_u64);
        if actual_change != expected_change {
            log::warn!(
                "P2PK change amount mismatch: expected {} sats, got {} sats (initial: {}, sent: {}, fee: {})",
                expected_change, actual_change, initial_total, amount, fee_u64
            );
        }
        Ok::<(Token, Vec<cdk::nuts::Proof>), String>((token, keep_proofs))
    }
        .await;
    match result {
        Ok((token, proofs)) => Ok((token.to_string(), proofs)),
        Err(e) => {
            if is_token_spent_error_string(&e) || is_insufficient_funds_error_string(&e) {
                log::warn!("P2PK send failed ({}), cleaning up and retrying...", e);
                let (cleaned_count, cleaned_amount) =
                    cleanup_spent_proofs_internal(mint_url).await?;
                log::info!(
                    "Cleaned up {} spent proofs worth {} sats, retrying P2PK send",
                    cleaned_count,
                    cleaned_amount
                );
                let fresh_proofs = get_proofs_for_mint(mint_url)?;
                let fresh_total: u64 = fresh_proofs
                    .iter()
                    .map(|p| u64::from(p.amount))
                    .try_fold(0u64, |acc, amt| acc.checked_add(amt))
                    .ok_or("Balance overflow")?;
                if fresh_total < amount {
                    return Err(format!(
                        "Insufficient funds after cleanup. Available: {} sats, Required: {} sats",
                        fresh_total, amount,
                    ));
                }
                let wallet = create_ephemeral_wallet(mint_url, fresh_proofs.clone()).await?;
                let send_proofs = wallet
                    .swap(
                        Some(Amount::from(amount)),
                        SplitTarget::default(),
                        fresh_proofs.clone(),
                        Some(spending_conditions),
                        true,
                    )
                    .await
                    .map_err(|e| format!("Retry swap failed: {}", e))?
                    .ok_or("Retry swap returned no proofs")?;
                let keep_proofs = wallet
                    .get_unspent_proofs()
                    .await
                    .map_err(|e| format!("Failed to get change proofs after retry: {}", e))?;
                let mint_url_parsed: cdk::mint_url::MintUrl = mint_url
                    .parse()
                    .map_err(|e| format!("Invalid mint URL: {}", e))?;
                let token = Token::new(
                    mint_url_parsed,
                    send_proofs,
                    None,
                    cdk::nuts::CurrencyUnit::Sat,
                );
                log::info!("P2PK send succeeded after cleanup and retry");
                Ok((token.to_string(), keep_proofs))
            } else {
                Err(format!("Failed to send P2PK: {}", e))
            }
        }
    }
}
/// Publish token and deletion events after send
async fn publish_send_events(
    mint_url: &str,
    keep_proofs: &[cdk::nuts::Proof],
    event_ids_to_delete: &[String],
    pending_event_id: &str,
) -> Result<Option<String>, String> {
    let signer = crate::stores::signer::get_signer()
        .ok_or("No signer available")?
        .as_nostr_signer();
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let mut new_event_id: Option<String> = None;
    if !keep_proofs.is_empty() {
        let proof_data: Vec<ProofData> = keep_proofs.iter().map(cdk_proof_to_proof_data).collect();
        let extended_proofs: Vec<ExtendedCashuProof> = proof_data
            .iter()
            .map(|p| ExtendedCashuProof::from(p.clone()))
            .collect();
        let token_event_data = ExtendedTokenEvent {
            mint: mint_url.to_string(),
            unit: "sat".to_string(),
            proofs: extended_proofs,
            del: event_ids_to_delete.to_vec(),
        };
        let json_content = serde_json::to_string(&token_event_data)
            .map_err(|e| format!("Failed to serialize token event: {}", e))?;
        let encrypted = signer
            .nip44_encrypt(&pubkey, &json_content)
            .await
            .map_err(|e| format!("Failed to encrypt token event: {}", e))?;
        let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);
        let mut unsigned =
            crate::utils::nips::nip89::tag_event_builder(builder.clone()).build(pubkey);
        let event_id_hex = unsigned.id().to_hex();
        let signed_event = unsigned
            .sign(&signer)
            .await
            .map_err(|e| format!("Failed to sign token event: {}", e))?;
        match client.send_event(&signed_event).await {
            Ok(output) => {
                if !output.success.is_empty() {
                    super::events::update_token_event_id(pending_event_id, &event_id_hex);
                    log::info!(
                        "Published new token event: {} (to {} relays)",
                        event_id_hex,
                        output.success.len()
                    );
                    new_event_id = Some(event_id_hex);
                } else {
                    log::warn!(
                        "No relays accepted token event (failed: {}), queuing for retry",
                        output.failed.len()
                    );
                    queue_signed_event_for_retry(
                        signed_event,
                        PendingEventType::TokenEvent,
                        Some(pending_event_id.to_string()),
                        Some(mint_url.to_string()),
                    )
                    .await
                    .map_err(|queue_err| {
                        format!("Failed to queue send token event for retry: {}", queue_err)
                    })?;
                    new_event_id = Some(pending_event_id.to_string());
                }
            }
            Err(e) => {
                log::warn!("Failed to publish token event, queuing for retry: {}", e);
                queue_signed_event_for_retry(
                    signed_event,
                    PendingEventType::TokenEvent,
                    Some(pending_event_id.to_string()),
                    Some(mint_url.to_string()),
                )
                .await
                .map_err(|queue_err| {
                    format!("Failed to queue send token event for retry: {}", queue_err)
                })?;
                new_event_id = Some(pending_event_id.to_string());
            }
        }
    }
    if !event_ids_to_delete.is_empty() {
        let valid_event_ids: Vec<_> = event_ids_to_delete
            .iter()
            .filter(|id| EventId::from_hex(id).is_ok())
            .collect();
        if !valid_event_ids.is_empty() {
            let mut tags = Vec::new();
            for event_id in &valid_event_ids {
                tags.push(nostr_sdk::Tag::event(EventId::from_hex(event_id).unwrap()));
            }
            tags.push(nostr_sdk::Tag::custom(
                nostr_sdk::TagKind::custom("k"),
                ["7375"],
            ));
            let deletion_builder =
                nostr_sdk::EventBuilder::new(Kind::from(5), "Spent token").tags(tags);
            match client
                .send_event_builder(crate::utils::nips::nip89::tag_event_builder(
                    deletion_builder.clone(),
                ))
                .await
            {
                Ok(response) if !response.success.is_empty() => {
                    log::info!(
                        "Published deletion events for {} token events",
                        valid_event_ids.len()
                    );
                }
                Ok(_) => {
                    log::warn!("No relays accepted deletion event, queuing for retry");
                    queue_event_for_retry(
                        deletion_builder,
                        PendingEventType::DeletionEvent,
                        None,
                        None,
                    )
                    .await
                    .map_err(|queue_err| {
                        format!(
                            "Failed to queue send deletion event for retry: {}",
                            queue_err
                        )
                    })?;
                }
                Err(e) => {
                    log::warn!("Failed to publish deletion event, queuing for retry: {}", e);
                    queue_event_for_retry(
                        deletion_builder,
                        PendingEventType::DeletionEvent,
                        None,
                        None,
                    )
                    .await
                    .map_err(|queue_err| {
                        format!(
                            "Failed to queue send deletion event for retry: {}",
                            queue_err
                        )
                    })?;
                }
            }
        }
        let invalid_count = event_ids_to_delete.len() - valid_event_ids.len();
        if invalid_count > 0 {
            log::warn!("Skipped {} invalid event IDs in deletion", invalid_count);
        }
    }
    Ok(new_event_id)
}
/// Update local state after a successful send using atomic token replacement
///
/// Uses crash-safe atomic replacement: add new tokens BEFORE deleting old ones.
/// This ensures worst case on crash is duplicate tokens (recoverable), never lost tokens.
fn update_local_state_after_send(
    mint_url: &str,
    keep_proofs: &[cdk::nuts::Proof],
    event_ids_to_delete: &[String],
    new_event_id: &Option<String>,
) -> Result<(), String> {
    let tokens_to_add = if let Some(ref event_id) = new_event_id {
        if keep_proofs.is_empty() {
            vec![]
        } else {
            let keep_proof_data: Vec<ProofData> =
                keep_proofs.iter().map(cdk_proof_to_proof_data).collect();
            let token = TokenData {
                event_id: event_id.clone(),
                pending_publish: super::types::token_publish_pending(event_id),
                mint: mint_url.to_string(),
                unit: "sat".to_string(),
                proofs: keep_proof_data,
                created_at: chrono::Utc::now().timestamp() as u64,
            };
            vec![token]
        }
    } else {
        vec![]
    };
    let new_balance = super::signals::atomic_token_replace(tokens_to_add, event_ids_to_delete)?;
    super::proofs::rebuild_proof_event_map();
    log::info!(
        "Local state updated. Balance after send: {} sats",
        new_balance
    );
    Ok(())
}
/// Watch proof states to detect when sent tokens are claimed by recipient
///
/// This uses NUT-17 WebSocket subscriptions (or polling fallback) to monitor
/// when the proofs in a sent token are spent by the recipient.
///
/// # Arguments
/// * `mint_url` - The mint URL where tokens were sent from
/// * `y_values` - The Y values (proof secrets) of the sent proofs
/// * `on_claimed` - Callback function invoked when tokens are claimed
///
/// # Example
/// ```ignore
/// // After sending tokens, extract Y values and watch for claims
/// let y_values = extract_y_values_from_token(&token);
/// watch_sent_token_claims(mint_url, y_values, || {
///     log::info!("Tokens were claimed by recipient!");
/// });
/// ```
pub fn watch_sent_token_claims<F>(mint_url: String, y_values: Vec<String>, mut on_claimed: F)
where
    F: FnMut() + 'static,
{
    use super::ws as cashu_ws;
    use dioxus::prelude::spawn;
    if y_values.is_empty() {
        log::warn!("watch_sent_token_claims called with empty Y values");
        return;
    }
    spawn(async move {
        match cashu_ws::subscribe_to_proof_states(mint_url.clone(), y_values.clone()).await {
            Ok(mut rx) => {
                log::info!("Watching {} proof state(s) via WebSocket", y_values.len());
                while let Some(notification) = rx.recv().await {
                    if notification.state == cashu_ws::ProofState::Spent {
                        log::info!("Sent token claimed! Proof {} is now spent", notification.y);
                        on_claimed();
                        break;
                    }
                }
            }
            Err(e) => {
                log::warn!("WebSocket subscription failed for proof states: {}", e);
                poll_for_token_claims(mint_url, y_values, on_claimed).await;
            }
        }
    });
}
#[allow(dead_code)]
/// Poll proof states to detect when tokens are claimed (fallback for no WebSocket)
async fn poll_for_token_claims<F>(mint_url: String, y_values: Vec<String>, mut on_claimed: F)
where
    F: FnMut() + 'static,
{
    use super::ws as cashu_ws;
    let poll_interval_ms = 5000u32;
    let max_polls = 60;
    for poll_count in 0..max_polls {
        if poll_count > 0 {
            crate::platform::timer::sleep_ms(poll_interval_ms).await;
        }
        match cashu_ws::poll_proof_states(&mint_url, y_values.clone()).await {
            Ok(states) => {
                for state in states {
                    if state.state == cashu_ws::ProofState::Spent {
                        log::info!(
                            "Sent token claimed (via polling)! Proof {} is now spent",
                            state.y
                        );
                        on_claimed();
                        return;
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to poll proof states: {}", e);
            }
        }
    }
    log::info!(
        "Stopped watching for token claims after {} minutes",
        (max_polls * poll_interval_ms / 60000)
    );
}
/// Extract Y values from a token string for proof state tracking
///
/// Y values are computed as hash_to_curve(secret) for each proof.
/// These can be used to check proof state via NUT-07 or subscribe via NUT-17.
pub fn extract_y_values_from_token(token_str: &str) -> Result<Vec<String>, String> {
    use cdk::dhke::hash_to_curve;
    use cdk::nuts::Token;
    let token: Token = token_str
        .parse()
        .map_err(|e| format!("Failed to parse token: {}", e))?;
    let secrets: Vec<cdk::secret::Secret> = match &token {
        Token::TokenV3(v3) => v3
            .token
            .iter()
            .flat_map(|t| t.proofs.iter().map(|p| p.secret.clone()))
            .collect(),
        Token::TokenV4(v4) => v4
            .token
            .iter()
            .flat_map(|t| t.proofs.iter().map(|p| p.secret.clone()))
            .collect(),
    };
    if secrets.is_empty() {
        return Err("Token contains no proofs".to_string());
    }
    let y_values: Result<Vec<String>, String> = secrets
        .iter()
        .map(|secret| {
            hash_to_curve(secret.as_bytes())
                .map(|y| y.to_string())
                .map_err(|e| format!("Failed to compute Y for secret: {}", e))
        })
        .collect();
    y_values
}
