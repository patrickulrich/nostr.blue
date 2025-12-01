//! Send operations
//!
//! Functions for sending ecash tokens, including P2PK sends.

use dioxus::prelude::*;
use nostr_sdk::signer::NostrSigner;
use nostr_sdk::{EventId, Kind, PublicKey};

use super::events::{queue_event_for_retry, queue_signed_event_for_retry};
use super::internal::{
    cleanup_spent_proofs_internal, create_ephemeral_wallet, is_insufficient_funds_error_string,
    is_token_spent_error_string, nostr_pubkey_to_cdk_pubkey, validate_proofs_with_mint,
};
use super::proofs::{cdk_proof_to_proof_data, proof_data_to_cdk_proof, register_proofs_in_event_map};
use super::signals::{try_acquire_mint_lock, WALLET_BALANCE, WALLET_STATE, WALLET_TOKENS};
use super::types::{
    ExtendedCashuProof, ExtendedTokenEvent, ProofData, TokenData, WalletTokensStoreStoreExt,
};
use super::utils::{mint_matches, normalize_mint_url};
use crate::stores::{auth_store, cashu_cdk_bridge, nostr_client};
use super::types::PendingEventType;

// =============================================================================
// Public API
// =============================================================================

/// Estimate the fee for sending a given amount from a mint
///
/// Returns the estimated fee in sats, or an error if estimation fails.
/// This uses CDK's prepare_send to get the exact fee that would be charged.
pub async fn estimate_send_fee(mint_url: String, amount: u64) -> Result<u64, String> {
    use cdk::wallet::{SendKind, SendOptions};
    use cdk::Amount;

    let mint_url = normalize_mint_url(&mint_url);

    // Get available proofs
    let all_proofs = get_proofs_for_mint(&mint_url)?;

    if all_proofs.is_empty() {
        return Err("No tokens found for this mint".to_string());
    }

    // Check we have enough balance
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

    // Create ephemeral wallet and prepare send to get fee estimate
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
    log::debug!("Estimated fee for {} sats from {}: {} sats", amount, mint_url, fee);

    // Cancel the prepared send to release reserved proofs back to Unspent state
    // This is required per CDK best practices - dropping without cancel/confirm
    // leaves proofs stuck in Reserved state in IndexedDB
    prepared.cancel().await
        .map_err(|e| format!("Failed to cancel prepared send: {}", e))?;

    Ok(fee)
}

/// Send ecash tokens
pub async fn send_tokens(mint_url: String, amount: u64) -> Result<String, String> {
    // Normalize mint URL for consistent comparison
    let mint_url = normalize_mint_url(&mint_url);

    log::info!("Sending {} sats from {}", amount, mint_url);

    // Acquire mint operation lock
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;

    // Get available proofs
    let all_proofs = get_proofs_for_mint(&mint_url)?;

    if all_proofs.is_empty() {
        return Err("No tokens found for this mint".to_string());
    }

    // NUT-07: Validate proofs with mint before sending
    let all_proofs = validate_proofs_with_mint(&mint_url, all_proofs).await?;

    // Re-fetch event_ids after potential cleanup
    let event_ids_to_delete = get_event_ids_for_mint(&mint_url);

    // Check balance
    let total_available: u64 = all_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Balance overflow")?;

    if total_available < amount {
        return Err(format!(
            "Insufficient funds. Available: {} sats, Required: {} sats",
            total_available, amount
        ));
    }

    // Prepare and confirm send with auto-retry on spent proofs
    let (token_string, keep_proofs) = execute_send_with_retry(
        &mint_url,
        amount,
        all_proofs,
        None, // No P2PK conditions
    )
    .await?;

    // Publish events and update state
    let new_event_id =
        publish_send_events(&mint_url, &keep_proofs, &event_ids_to_delete).await?;

    // Update local state
    update_local_state_after_send(&mint_url, &keep_proofs, &event_ids_to_delete, &new_event_id)?;

    // Create history event
    let valid_created: Vec<String> = new_event_id.iter().cloned().collect();
    let valid_destroyed: Vec<String> = event_ids_to_delete
        .iter()
        .filter(|id| EventId::from_hex(id).is_ok())
        .cloned()
        .collect();

    if let Err(e) =
        super::events::create_history_event("out", amount, valid_created, valid_destroyed).await
    {
        log::error!("Failed to create history event: {}", e);
    }

    // Sync MultiMintWallet state (non-critical)
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

    // Normalize mint URL
    let mint_url = normalize_mint_url(&mint_url);

    log::info!(
        "Sending {} sats P2PK to {} from {}",
        amount,
        recipient_pubkey,
        mint_url
    );

    // Convert Nostr pubkey to CDK pubkey for P2PK
    let cdk_pubkey = nostr_pubkey_to_cdk_pubkey(&recipient_pubkey)?;

    // Create P2PK spending conditions
    let spending_conditions = SpendingConditions::new_p2pk(cdk_pubkey, None);

    // Acquire mint operation lock
    let _lock_guard = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation is in progress for mint: {}", mint_url))?;

    // Get available proofs
    let all_proofs = get_proofs_for_mint(&mint_url)?;

    if all_proofs.is_empty() {
        return Err("No tokens found for this mint".to_string());
    }

    // NUT-07: Validate proofs with mint
    let all_proofs = validate_proofs_with_mint(&mint_url, all_proofs).await?;

    // Re-fetch event_ids after potential cleanup
    let event_ids_to_delete = get_event_ids_for_mint(&mint_url);

    // Check balance
    let total_available: u64 = all_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Balance overflow")?;

    if total_available < amount {
        return Err(format!(
            "Insufficient funds. Available: {} sats, Required: {} sats",
            total_available, amount
        ));
    }

    // Execute send with P2PK conditions
    let (token_string, keep_proofs) = execute_send_with_retry(
        &mint_url,
        amount,
        all_proofs,
        Some(spending_conditions),
    )
    .await?;

    // Publish events and update state
    let new_event_id =
        publish_send_events(&mint_url, &keep_proofs, &event_ids_to_delete).await?;

    // Update local state
    update_local_state_after_send(&mint_url, &keep_proofs, &event_ids_to_delete, &new_event_id)?;

    // Create history event
    let valid_created: Vec<String> = new_event_id.iter().cloned().collect();
    let valid_destroyed: Vec<String> = event_ids_to_delete
        .iter()
        .filter(|id| EventId::from_hex(id).is_ok())
        .cloned()
        .collect();

    if let Err(e) =
        super::events::create_history_event("out", amount, valid_created, valid_destroyed).await
    {
        log::error!("Failed to create history event: {}", e);
    }

    // Sync state
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync MultiMintWallet state after P2PK send: {}", e);
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
    let privkey = state.privkey.as_ref().ok_or("Wallet private key not available")?;

    let secret_key = cdk::nuts::SecretKey::from_hex(privkey)
        .map_err(|e| format!("Invalid wallet privkey: {}", e))?;

    let pubkey = secret_key.public_key();
    Ok(pubkey.to_hex())
}

// =============================================================================
// Internal Helpers
// =============================================================================

/// Get CDK proofs for a specific mint
fn get_proofs_for_mint(mint_url: &str) -> Result<Vec<cdk::nuts::Proof>, String> {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();

    let mut all_proofs = Vec::new();

    for token in tokens.iter().filter(|t| mint_matches(&t.mint, mint_url)) {
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
        .filter(|t| mint_matches(&t.mint, mint_url))
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

    // Try sending with current proofs
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

        // Confirm the prepared send
        // Note: CDK's confirm() takes ownership. If it fails, CDK internally handles
        // releasing reserved proofs, so we don't need explicit cancel() here.
        let token = prepared.confirm(None).await.map_err(|e| e.to_string())?;
        let keep_proofs = wallet.get_unspent_proofs().await.map_err(|e| e.to_string())?;

        // Validate change amount matches expectations
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
            // Continue anyway - CDK is authoritative, but log the discrepancy
        }

        Ok::<(cdk::nuts::Token, Vec<cdk::nuts::Proof>), String>((token, keep_proofs))
    }
    .await;

    match result {
        Ok((token, proofs)) => Ok((token.to_string(), proofs)),
        Err(e) => {
            // Auto-retry if proofs are already spent
            if is_token_spent_error_string(&e) || is_insufficient_funds_error_string(&e) {
                log::warn!("Send failed ({}), cleaning up and retrying...", e);

                // Cleanup spent proofs
                let (cleaned_count, cleaned_amount) =
                    cleanup_spent_proofs_internal(mint_url).await?;

                log::info!(
                    "Cleaned up {} spent proofs worth {} sats, retrying send",
                    cleaned_count,
                    cleaned_amount
                );

                // Get fresh proofs after cleanup
                let fresh_proofs = get_proofs_for_mint(mint_url)?;

                // Check we still have enough after cleanup
                let fresh_total: u64 = fresh_proofs
                    .iter()
                    .map(|p| u64::from(p.amount))
                    .try_fold(0u64, |acc, amt| acc.checked_add(amt))
                    .ok_or("Balance overflow")?;
                if fresh_total < amount {
                    return Err(format!(
                        "Insufficient funds after cleanup. Available: {} sats, Required: {} sats",
                        fresh_total, amount
                    ));
                }

                // Retry send with fresh proofs
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

                // Confirm the prepared send
                // Note: CDK's confirm() takes ownership. If it fails, CDK internally handles
                // releasing reserved proofs, so we don't need explicit cancel() here.
                let token = prepared
                    .confirm(None)
                    .await
                    .map_err(|e| format!("Retry confirm failed: {}", e))?;

                let keep_proofs = wallet
                    .get_unspent_proofs()
                    .await
                    .map_err(|e| format!("Failed to get remaining proofs: {}", e))?;

                // Validate change amount matches expectations
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

/// Publish token and deletion events after send
async fn publish_send_events(
    mint_url: &str,
    keep_proofs: &[cdk::nuts::Proof],
    event_ids_to_delete: &[String],
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

    // Publish token event with remaining proofs
    if !keep_proofs.is_empty() {
        let proof_data: Vec<ProofData> = keep_proofs.iter().map(|p| cdk_proof_to_proof_data(p)).collect();

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

        // Pre-compute event ID
        let mut unsigned = builder.clone().build(pubkey);
        let event_id_hex = unsigned.id().to_hex();

        // Sign the event
        let signed_event = unsigned
            .sign(&signer)
            .await
            .map_err(|e| format!("Failed to sign token event: {}", e))?;

        // Try to publish
        match client.send_event(&signed_event).await {
            Ok(_) => {
                log::info!("Published new token event: {}", event_id_hex);
            }
            Err(e) => {
                log::warn!("Failed to publish token event, queuing for retry: {}", e);
                queue_signed_event_for_retry(signed_event, PendingEventType::TokenEvent).await;
            }
        }

        new_event_id = Some(event_id_hex);
    }

    // Publish deletion event for old token events
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

            match client.send_event_builder(deletion_builder.clone()).await {
                Ok(_) => {
                    log::info!(
                        "Published deletion events for {} token events",
                        valid_event_ids.len()
                    );
                }
                Err(e) => {
                    log::warn!("Failed to publish deletion event, queuing for retry: {}", e);
                    queue_event_for_retry(deletion_builder, PendingEventType::DeletionEvent).await;
                }
            }
        }

        let invalid_count = event_ids_to_delete.len() - valid_event_ids.len();
        if invalid_count > 0 {
            log::warn!(
                "Skipped {} invalid event IDs in deletion",
                invalid_count
            );
        }
    }

    Ok(new_event_id)
}

/// Update local state after a successful send
fn update_local_state_after_send(
    mint_url: &str,
    keep_proofs: &[cdk::nuts::Proof],
    event_ids_to_delete: &[String],
    new_event_id: &Option<String>,
) -> Result<(), String> {
    let store = WALLET_TOKENS.read();
    let mut data = store.data();
    let mut tokens_write = data.write();

    // Remove old token events
    tokens_write.retain(|t| !event_ids_to_delete.contains(&t.event_id));

    // Add new token with remaining proofs
    if let Some(ref event_id) = new_event_id {
        let keep_proof_data: Vec<ProofData> =
            keep_proofs.iter().map(|p| cdk_proof_to_proof_data(p)).collect();

        tokens_write.push(TokenData {
            event_id: event_id.clone(),
            mint: mint_url.to_string(),
            unit: "sat".to_string(),
            proofs: keep_proof_data.clone(),
            created_at: chrono::Utc::now().timestamp() as u64,
        });

        // Register proofs in event map
        register_proofs_in_event_map(event_id, &keep_proof_data);
    }

    // Update balance
    let new_balance: u64 = tokens_write
        .iter()
        .flat_map(|t| &t.proofs)
        .map(|p| p.amount)
        .try_fold(0u64, |acc, amount| acc.checked_add(amount))
        .ok_or_else(|| "Balance calculation overflow".to_string())?;

    *WALLET_BALANCE.write() = new_balance;

    log::info!("Local state updated. Balance after send: {} sats", new_balance);

    Ok(())
}

// =============================================================================
// Token Claim Tracking (NUT-17)
// =============================================================================

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
pub fn watch_sent_token_claims<F>(
    mint_url: String,
    y_values: Vec<String>,
    mut on_claimed: F,
) where
    F: FnMut() + 'static,
{
    use dioxus::prelude::spawn;
    use super::ws as cashu_ws;

    if y_values.is_empty() {
        log::warn!("watch_sent_token_claims called with empty Y values");
        return;
    }

    spawn(async move {
        // Try WebSocket subscription first
        match cashu_ws::subscribe_to_proof_states(mint_url.clone(), y_values.clone()).await {
            Ok(mut rx) => {
                log::info!("Watching {} proof state(s) via WebSocket", y_values.len());

                while let Some(notification) = rx.recv().await {
                    // Check if any proof is now spent
                    if notification.state == cashu_ws::ProofState::Spent {
                        log::info!("Sent token claimed! Proof {} is now spent", notification.y);
                        on_claimed();
                        break;
                    }
                }
            }
            Err(e) => {
                log::warn!("WebSocket subscription failed for proof states: {}", e);
                // Fall back to polling
                poll_for_token_claims(mint_url, y_values, on_claimed).await;
            }
        }
    });
}

#[allow(dead_code)] // Called by watch_sent_token_claims
/// Poll proof states to detect when tokens are claimed (fallback for no WebSocket)
async fn poll_for_token_claims<F>(
    mint_url: String,
    y_values: Vec<String>,
    mut on_claimed: F,
) where
    F: FnMut() + 'static,
{
    use super::ws as cashu_ws;

    let poll_interval_ms = 5000u32; // 5 seconds
    let max_polls = 60; // 5 minutes max

    for poll_count in 0..max_polls {
        // Wait between polls
        if poll_count > 0 {
            gloo_timers::future::TimeoutFuture::new(poll_interval_ms).await;
        }

        // Check proof states
        match cashu_ws::poll_proof_states(&mint_url, y_values.clone()).await {
            Ok(states) => {
                for state in states {
                    if state.state == cashu_ws::ProofState::Spent {
                        log::info!("Sent token claimed (via polling)! Proof {} is now spent", state.y);
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

    log::info!("Stopped watching for token claims after {} minutes", (max_polls * poll_interval_ms / 60000));
}

/// Extract Y values from a token string for proof state tracking
///
/// Y values are computed as hash_to_curve(secret) for each proof.
/// These can be used to check proof state via NUT-07 or subscribe via NUT-17.
pub fn extract_y_values_from_token(token_str: &str) -> Result<Vec<String>, String> {
    use cdk::dhke::hash_to_curve;
    use cdk::nuts::Token;

    // Parse the token
    let token: Token = token_str.parse()
        .map_err(|e| format!("Failed to parse token: {}", e))?;

    // Extract secrets from proofs based on token version
    // We access internal structure directly to avoid needing keyset info
    let secrets: Vec<cdk::secret::Secret> = match &token {
        Token::TokenV3(v3) => {
            v3.token.iter()
                .flat_map(|t| t.proofs.iter().map(|p| p.secret.clone()))
                .collect()
        }
        Token::TokenV4(v4) => {
            v4.token.iter()
                .flat_map(|t| t.proofs.iter().map(|p| p.secret.clone()))
                .collect()
        }
    };

    if secrets.is_empty() {
        return Err("Token contains no proofs".to_string());
    }

    // Compute Y = hash_to_curve(secret) for each proof
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
