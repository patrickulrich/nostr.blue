//! Direct Swap Operations
//!
//! Exposes CDK's wallet.swap() for advanced use cases like:
//! - Keyset migration
//! - Proof consolidation
//! - Denomination optimization
//! - Atomic swaps between keysets
//!
//! # State Management
//!
//! This module provides two levels of swap functions:
//!
//! 1. `execute_swap()` - Low-level CDK wrapper, no NIP-60 sync (for internal use)
//! 2. `execute_swap_with_nip60()` - Full NIP-60 state management (for UI operations)
//!
//! The NIP-60 wrapper follows the pattern from send.rs:
//! - Acquire mint lock to prevent concurrent operations
//! - Execute CDK swap (has built-in try_proof_operation_or_reclaim)
//! - IMMEDIATELY update WALLET_TOKENS with pending event ID
//! - Attempt Nostr publish (safe to fail - local state already updated)
//! - Create history event (kind 7376)
//! - Sync CDK bridge state
//!
//! # Crash Recovery
//!
//! If the app crashes between CDK success and Nostr publish,
//! `sync_orphaned_cdk_proofs_to_nostr()` runs on startup and recovers
//! any proofs that exist in CDK but not in WALLET_TOKENS.
//!
//! Currently marked as dead_code - functions not yet wired to UI.
#![allow(dead_code)]
use super::denomination::DenominationStrategy;
use super::events::{queue_signed_event_for_retry, update_token_event_id};
use super::internal::get_or_create_wallet;
use super::proofs::{
    cdk_proof_to_proof_data, get_event_ids_for_proofs, proof_data_to_cdk_proof,
    register_proofs_in_event_map,
};
use super::signals::InFlightGuard;
use super::signals::{try_acquire_mint_lock, WALLET_TOKENS};
use super::types::{
    ExtendedCashuProof, ExtendedTokenEvent, InFlightSendRequest, PendingEventType, ProofData,
    TokenData, WalletTokensStoreStoreExt,
};
use super::utils::{mint_matches, normalize_mint_url, now_secs};
use crate::stores::{auth_store, cashu_cdk_bridge, nostr_client};
use cdk::nuts::SpendingConditions;
use dioxus::prelude::ReadableExt;
use nostr_sdk::signer::NostrSigner;
use nostr_sdk::{EventId, Kind, PublicKey};
/// Options for swap operations
#[derive(Debug, Clone, Default)]
pub struct SwapOptions {
    /// Target amount (None = swap all)
    pub amount: Option<u64>,
    /// Denomination strategy
    pub denomination: DenominationStrategy,
    /// Spending conditions for output proofs
    pub conditions: Option<SpendingConditions>,
    /// Include fee in output (true = fee taken from input)
    pub include_fee: bool,
}
impl SwapOptions {
    /// Create options for swapping all proofs
    pub fn all() -> Self {
        Self::default()
    }
    /// Create options for specific amount
    pub fn amount(amount: u64) -> Self {
        Self {
            amount: Some(amount),
            ..Default::default()
        }
    }
    /// Set denomination strategy
    pub fn with_denomination(mut self, strategy: DenominationStrategy) -> Self {
        self.denomination = strategy;
        self
    }
    /// Set spending conditions
    pub fn with_conditions(mut self, conditions: SpendingConditions) -> Self {
        self.conditions = Some(conditions);
        self
    }
    /// Set include_fee flag
    pub fn with_include_fee(mut self, include: bool) -> Self {
        self.include_fee = include;
        self
    }
}
/// Result of a swap operation
#[derive(Debug, Clone)]
pub struct SwapResult {
    /// Output proofs from swap
    pub proofs: Vec<ProofData>,
    /// Total value of output proofs
    pub output_amount: u64,
    /// Fee paid for the swap
    pub fee_paid: u64,
    /// Number of input proofs consumed
    pub inputs_consumed: usize,
    /// Number of output proofs received
    pub outputs_received: usize,
}
/// Execute a direct swap with the mint
///
/// This is a low-level operation that directly calls CDK's wallet.swap().
/// Use for keyset migration, consolidation, or custom denomination strategies.
pub async fn execute_swap(
    mint_url: &str,
    input_proofs: Vec<ProofData>,
    options: SwapOptions,
) -> Result<SwapResult, String> {
    if input_proofs.is_empty() {
        return Err("No input proofs provided".to_string());
    }
    let input_count = input_proofs.len();
    let input_value: u64 = input_proofs
        .iter()
        .map(|p| p.amount)
        .fold(0u64, |acc, amt| acc.saturating_add(amt));
    log::info!(
        "Executing swap: {} proofs ({} sats) at {}",
        input_count,
        input_value,
        mint_url
    );
    let cdk_proofs: Vec<cdk::nuts::Proof> = input_proofs
        .iter()
        .map(proof_data_to_cdk_proof)
        .collect::<Result<Vec<_>, _>>()?;
    let wallet = get_or_create_wallet(mint_url).await?;
    let amount = options.amount.map(cdk::Amount::from);
    let split_target = options.denomination.to_split_target();
    let output_proofs = wallet
        .swap(
            amount,
            split_target,
            cdk_proofs,
            options.conditions,
            options.include_fee,
        )
        .await
        .map_err(|e| format!("Swap failed: {}", e))?;
    let output_proofs = output_proofs.ok_or("Swap returned no proofs")?;
    let output_value: u64 = output_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .fold(0u64, |acc, amt| acc.saturating_add(amt));
    let fee_paid = input_value.saturating_sub(output_value);
    let proof_data: Vec<ProofData> = output_proofs.iter().map(cdk_proof_to_proof_data).collect();
    log::info!(
        "Swap complete: {} -> {} proofs, fee {} sats",
        input_count,
        proof_data.len(),
        fee_paid
    );
    Ok(SwapResult {
        proofs: proof_data,
        output_amount: output_value,
        fee_paid,
        inputs_consumed: input_count,
        outputs_received: output_proofs.len(),
    })
}
/// Execute a swap with full NIP-60 state management
///
/// This is the safe version that updates both CDK and Nostr state.
/// Use this for UI-facing operations.
///
/// # Pattern (from send.rs + minibits)
///
/// 1. Acquire mint lock
/// 2. Get event IDs for input proofs (for del tags)
/// 3. Call CDK swap() directly (has built-in try_proof_operation_or_reclaim)
/// 4. IMMEDIATELY update WALLET_TOKENS with pending_event_id
/// 5. Attempt Nostr publish (safe to fail - local state already updated)
/// 6. If publish fails, queue for retry
/// 7. Create history event
///
/// # NOTE
///
/// For amount=None swaps (refresh/consolidate), CDK stores all proofs
/// internally and returns None. We must fetch the new proofs from CDK's
/// localstore to publish to NIP-60.
pub async fn execute_swap_with_nip60(
    mint_url: &str,
    input_proofs: Vec<ProofData>,
    options: SwapOptions,
) -> Result<SwapResult, String> {
    if input_proofs.is_empty() {
        return Err("No input proofs provided".to_string());
    }
    let mint_url = normalize_mint_url(mint_url);
    let _lock = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation in progress for {}", mint_url))?;
    let all_spendable = get_proofs_for_mint(&mint_url)?;
    if input_proofs.len() != all_spendable.len() {
        return Err(format!(
            "Incomplete proof set: got {} proofs, wallet has {} spendable proofs for {}. \
             Use get_proofs_for_mint() or a high-level function like swap_refresh().",
            input_proofs.len(),
            all_spendable.len(),
            mint_url,
        ));
    }
    let input_secrets_set: std::collections::HashSet<_> =
        input_proofs.iter().map(|p| &p.secret).collect();
    let wallet_secrets: std::collections::HashSet<_> =
        all_spendable.iter().map(|p| &p.secret).collect();
    if input_secrets_set != wallet_secrets {
        return Err(
            "Proof set mismatch: input proofs don't match wallet's spendable proofs. \
             Ensure you're passing the complete set from get_proofs_for_mint()."
                .to_string(),
        );
    }
    let input_secrets: Vec<String> = input_proofs.iter().map(|p| p.secret.clone()).collect();
    let event_ids_to_delete = get_event_ids_for_proofs(&input_secrets);
    let cdk_proofs: Vec<cdk::nuts::Proof> = input_proofs
        .iter()
        .map(proof_data_to_cdk_proof)
        .collect::<Result<Vec<_>, _>>()?;
    let input_value: u64 = cdk_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Input value overflow")?;
    let input_count = cdk_proofs.len();
    use std::collections::HashSet;
    let input_ys: HashSet<String> = cdk_proofs
        .iter()
        .map(|p| p.y().map(|y| y.to_string()))
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|e| format!("Failed to compute Y value for input proof: {}", e))?;
    log::info!(
        "Executing NIP-60 swap: {} proofs ({} sats) at {}",
        input_count,
        input_value,
        mint_url
    );
    let tx_id = format!("swap_{}", uuid::Uuid::new_v4());
    let proof_secrets: Vec<String> = cdk_proofs.iter().map(|p| p.secret.to_string()).collect();
    let in_flight = InFlightSendRequest {
        transaction_id: tx_id.clone(),
        mint_url: mint_url.clone(),
        proof_secrets,
        amount: input_value,
        operation_type: super::types::OperationType::Swap,
        created_at: now_secs(),
    };
    super::signals::add_in_flight_send_request(in_flight);
    let mut in_flight_guard = InFlightGuard::new(tx_id.clone());
    let wallet = get_or_create_wallet(&mint_url).await?;
    let pre_swap_proofs = wallet
        .get_unspent_proofs()
        .await
        .map_err(|e| format!("Failed to get pre-swap proofs: {}", e))?;
    let pre_swap_ys: HashSet<String> = pre_swap_proofs
        .iter()
        .map(|p| p.y().map(|y| y.to_string()))
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|e| format!("Failed to compute Y for pre-swap proof: {}", e))?;
    log::debug!(
        "Pre-swap snapshot: {} existing unspent proofs",
        pre_swap_ys.len()
    );
    let amount = options.amount.map(cdk::Amount::from);
    let split_target = options.denomination.to_split_target();
    let swap_result = wallet
        .swap(
            amount,
            split_target,
            cdk_proofs,
            options.conditions.clone(),
            options.include_fee,
        )
        .await;
    let swap_result = swap_result.map_err(|e| format!("Swap failed: {}", e))?;
    let output_proofs = match swap_result {
        Some(send_proofs) => {
            let all_unspent = wallet
                .get_unspent_proofs()
                .await
                .map_err(|e| format!("Failed to get proofs after swap: {}", e))?;
            let mut seen_ys: HashSet<String> = HashSet::new();
            let merged: Vec<cdk::nuts::Proof> = send_proofs
                .iter()
                .chain(all_unspent.iter())
                .map(|p| {
                    let y = p
                        .y()
                        .map_err(|e| {
                            format!(
                                "Failed to compute Y for proof (amount={} sats): {} - this indicates a critical error",
                                u64::from(p.amount),
                                e,
                            )
                        })?;
                    Ok((p.clone(), y.to_string()))
                })
                .collect::<Result<Vec<_>, String>>()?
                .into_iter()
                .filter(|(_, y_str)| {
                    if input_ys.contains(y_str) || pre_swap_ys.contains(y_str)
                        || seen_ys.contains(y_str)
                    {
                        false
                    } else {
                        seen_ys.insert(y_str.clone());
                        true
                    }
                })
                .map(|(p, _)| p)
                .collect();
            if merged.len() > send_proofs.len() {
                log::info!(
                    "Swap produced {} send proofs + {} change proofs",
                    send_proofs.len(),
                    merged.len() - send_proofs.len()
                );
            }
            merged
        }
        None => {
            let all_unspent = wallet
                .get_unspent_proofs()
                .await
                .map_err(|e| format!("Failed to get swapped proofs: {}", e))?;
            let mut new_proofs = Vec::new();
            for p in all_unspent {
                let y = p
                    .y()
                    .map_err(|e| {
                        format!(
                            "CRITICAL: Y_VALUE_COMPUTATION_FAILED - proof_amount={} sats, error='{}' - aborting to prevent fund loss",
                            u64::from(p.amount),
                            e,
                        )
                    })?;
                if !input_ys.contains(&y.to_string()) && !pre_swap_ys.contains(&y.to_string()) {
                    new_proofs.push(p);
                }
            }
            new_proofs
        }
    };
    let output_value: u64 = output_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Output value overflow")?;
    let fee_paid = input_value.saturating_sub(output_value);
    let pending_event_id = format!("pending_{}", uuid::Uuid::new_v4());
    update_local_state_after_swap(
        &mint_url,
        &output_proofs,
        &event_ids_to_delete,
        &pending_event_id,
    )?;
    in_flight_guard.dismiss();
    super::signals::remove_in_flight_send_request(&tx_id);
    let final_event_id = match publish_swap_events(
        &mint_url,
        &output_proofs,
        &event_ids_to_delete,
        &pending_event_id,
    )
    .await
    {
        Ok(real_id) => {
            update_token_event_id(&pending_event_id, &real_id);
            real_id
        }
        Err(e) => {
            log::warn!("Nostr publish failed, queued for retry: {}", e);
            pending_event_id.clone()
        }
    };
    let valid_created: Vec<String> = if final_event_id.starts_with("pending_") {
        vec![]
    } else {
        vec![final_event_id]
    };
    let valid_destroyed: Vec<String> = event_ids_to_delete
        .iter()
        .filter(|id| !id.starts_with("pending_") && EventId::from_hex(id).is_ok())
        .cloned()
        .collect();
    if !valid_created.is_empty() {
        if let Err(e) =
            super::events::create_history_event("in", output_value, valid_created, valid_destroyed)
                .await
        {
            log::error!("Failed to create history event: {}", e);
        }
    }
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync wallet state: {}", e);
    }
    let proof_data: Vec<ProofData> = output_proofs.iter().map(cdk_proof_to_proof_data).collect();
    log::info!(
        "NIP-60 swap complete: {} -> {} proofs, fee {} sats",
        input_count,
        proof_data.len(),
        fee_paid
    );
    Ok(SwapResult {
        proofs: proof_data,
        output_amount: output_value,
        fee_paid,
        inputs_consumed: input_count,
        outputs_received: output_proofs.len(),
    })
}
/// Update local state after swap using crash-safe atomic replacement
///
/// Uses add-before-delete pattern via atomic_token_replace: worst case on crash
/// is duplicate tokens (recoverable), never lost tokens.
fn update_local_state_after_swap(
    mint_url: &str,
    output_proofs: &[cdk::nuts::Proof],
    event_ids_to_delete: &[String],
    new_event_id: &str,
) -> Result<(), String> {
    if output_proofs.is_empty() {
        return Err("Cannot update state with empty output proofs".to_string());
    }
    let proof_data: Vec<ProofData> = output_proofs.iter().map(cdk_proof_to_proof_data).collect();
    let new_token = TokenData {
        event_id: new_event_id.to_string(),
        mint: normalize_mint_url(mint_url),
        unit: "sat".to_string(),
        proofs: proof_data.clone(),
        created_at: now_secs(),
    };
    let new_balance = super::signals::atomic_token_replace(vec![new_token], event_ids_to_delete)?;
    super::proofs::rebuild_proof_event_map();
    register_proofs_in_event_map(new_event_id, &proof_data);
    log::info!(
        "Local state updated after swap. Balance: {} sats",
        new_balance
    );
    Ok(())
}
/// Publish swap events to Nostr using nostr-sdk NIP-60 types
async fn publish_swap_events(
    mint_url: &str,
    output_proofs: &[cdk::nuts::Proof],
    event_ids_to_delete: &[String],
    pending_event_id: &str,
) -> Result<String, String> {
    if output_proofs.is_empty() {
        return Err("No proofs to publish".to_string());
    }
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
    let proof_data: Vec<ProofData> = output_proofs.iter().map(cdk_proof_to_proof_data).collect();
    let extended_proofs: Vec<ExtendedCashuProof> = proof_data
        .iter()
        .map(|p| ExtendedCashuProof::from(p.clone()))
        .collect();
    let valid_del_ids: Vec<String> = event_ids_to_delete
        .iter()
        .filter(|id| {
            if id.starts_with("pending_") {
                return false;
            }
            if EventId::from_hex(id).is_err() {
                log::warn!("Skipping invalid hex event ID in del list: {}", id);
                return false;
            }
            true
        })
        .cloned()
        .collect();
    let token_event_data = ExtendedTokenEvent {
        mint: mint_url.to_string(),
        unit: "sat".to_string(),
        proofs: extended_proofs,
        del: valid_del_ids.clone(),
    };
    let json_content = serde_json::to_string(&token_event_data)
        .map_err(|e| format!("Failed to serialize token event: {}", e))?;
    let encrypted = signer
        .nip44_encrypt(&pubkey, &json_content)
        .await
        .map_err(|e| format!("Failed to encrypt token event: {}", e))?;
    let builder = nostr_sdk::EventBuilder::new(Kind::CashuWalletUnspentProof, encrypted);
    let signed_event = builder
        .build(pubkey)
        .sign(&signer)
        .await
        .map_err(|e| format!("Failed to sign token event: {}", e))?;
    let event_id_hex = signed_event.id.to_hex();
    match client.send_event(&signed_event).await {
        Ok(output) => {
            if output.success.is_empty() {
                let all_duplicates = !output.failed.is_empty()
                    && output
                        .failed
                        .values()
                        .all(|err| err.to_lowercase().starts_with("duplicate:"));
                if all_duplicates {
                    log::debug!(
                        "Swap token event {} already exists on all relays (duplicate)",
                        event_id_hex
                    );
                    publish_deletion_events(&client, &valid_del_ids).await;
                } else {
                    log::warn!(
                        "No relays accepted swap token event (failed: {:?}), queuing for retry",
                        output.failed.keys().collect::<Vec<_>>()
                    );
                    queue_signed_event_for_retry(
                        signed_event,
                        PendingEventType::TokenEvent,
                        Some(pending_event_id.to_string()),
                        Some(mint_url.to_string()),
                    )
                    .await;
                    return Err("No relays accepted swap token event".to_string());
                }
            } else {
                log::info!(
                    "Published swap token event: {} (to {}/{} relays)",
                    event_id_hex,
                    output.success.len(),
                    output.success.len() + output.failed.len()
                );
                publish_deletion_events(&client, &valid_del_ids).await;
            }
        }
        Err(e) => {
            log::warn!(
                "Failed to publish swap token event, queuing for retry: {}",
                e
            );
            queue_signed_event_for_retry(
                signed_event,
                PendingEventType::TokenEvent,
                Some(pending_event_id.to_string()),
                Some(mint_url.to_string()),
            )
            .await;
            return Err(format!("Failed to publish swap token event: {}", e));
        }
    }
    Ok(event_id_hex)
}
/// Build deletion event tags for token events
///
/// CDK pattern: centralize tag building for publish_deletion_events
fn build_deletion_tags(event_ids: &[EventId]) -> Vec<nostr_sdk::Tag> {
    let mut tags: Vec<_> = event_ids
        .iter()
        .map(|eid| nostr_sdk::Tag::event(*eid))
        .collect();
    tags.push(nostr_sdk::Tag::custom(
        nostr_sdk::TagKind::custom("k"),
        ["7375"],
    ));
    tags
}
/// Publish deletion events for consumed token events
///
/// Note: Deletion events are only published AFTER the new token event is confirmed
/// published to at least one relay. This prevents the race condition where deletions
/// could be replayed before the token is accepted. If publishing fails, this function
/// handles its own retry logic via queue_event_for_retry.
async fn publish_deletion_events(client: &nostr_sdk::Client, event_ids_to_delete: &[String]) {
    if event_ids_to_delete.is_empty() {
        return;
    }
    let valid_event_ids: Vec<EventId> = event_ids_to_delete
        .iter()
        .filter_map(|id| EventId::from_hex(id).ok())
        .collect();
    if valid_event_ids.is_empty() {
        return;
    }
    let tags = build_deletion_tags(&valid_event_ids);
    let deletion_builder = nostr_sdk::EventBuilder::new(Kind::from(5), "Swapped token").tags(tags);
    match client.send_event_builder(deletion_builder.clone()).await {
        Ok(output) => {
            if output.success.is_empty() {
                log::warn!("No relays accepted deletion event, queuing for retry");
                super::events::queue_event_for_retry(
                    deletion_builder,
                    PendingEventType::DeletionEvent,
                    None,
                    None,
                )
                .await;
            } else {
                log::info!(
                    "Published deletion events for {} token events (to {}/{} relays)",
                    valid_event_ids.len(),
                    output.success.len(),
                    output.success.len() + output.failed.len()
                );
            }
        }
        Err(e) => {
            log::warn!("Failed to publish deletion event, queuing for retry: {}", e);
            super::events::queue_event_for_retry(
                deletion_builder,
                PendingEventType::DeletionEvent,
                None,
                None,
            )
            .await;
        }
    }
    let invalid_count = event_ids_to_delete.len() - valid_event_ids.len();
    if invalid_count > 0 {
        log::warn!("Skipped {} invalid event IDs in deletion", invalid_count);
    }
}
/// Swap all proofs for a mint to optimize denominations
///
/// Uses full NIP-60 state management - updates WALLET_TOKENS and publishes to Nostr.
pub async fn swap_optimize_denominations(
    mint_url: &str,
    strategy: DenominationStrategy,
) -> Result<SwapResult, String> {
    let all_proofs = get_proofs_for_mint(mint_url)?;
    if all_proofs.is_empty() {
        return Err("No proofs found for this mint".to_string());
    }
    let options = SwapOptions::all().with_denomination(strategy);
    execute_swap_with_nip60(mint_url, all_proofs, options).await
}
/// Swap proofs to add spending conditions (P2PK lock)
///
/// Uses full NIP-60 state management - updates WALLET_TOKENS and publishes to Nostr.
pub async fn swap_to_locked(
    mint_url: &str,
    amount: u64,
    conditions: SpendingConditions,
) -> Result<SwapResult, String> {
    let all_proofs = get_proofs_for_mint(mint_url)?;
    if all_proofs.is_empty() {
        return Err("No proofs found for this mint".to_string());
    }
    let total: u64 = all_proofs
        .iter()
        .map(|p| p.amount)
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Balance overflow")?;
    if total < amount {
        return Err(format!(
            "Insufficient funds: have {} sats, need {}",
            total, amount
        ));
    }
    let options = SwapOptions::amount(amount)
        .with_conditions(conditions)
        .with_include_fee(true);
    execute_swap_with_nip60(mint_url, all_proofs, options).await
}
/// Swap all proofs to fresh ones (privacy enhancement)
///
/// Uses full NIP-60 state management - updates WALLET_TOKENS and publishes to Nostr.
pub async fn swap_refresh(mint_url: &str) -> Result<SwapResult, String> {
    let all_proofs = get_proofs_for_mint(mint_url)?;
    if all_proofs.is_empty() {
        return Err("No proofs found for this mint".to_string());
    }
    let options = SwapOptions::all().with_denomination(DenominationStrategy::PowerOfTwo);
    execute_swap_with_nip60(mint_url, all_proofs, options).await
}
/// Get all proofs for a mint from local storage
fn get_proofs_for_mint(mint_url: &str) -> Result<Vec<ProofData>, String> {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();
    let normalized_url = normalize_mint_url(mint_url);
    let proofs: Vec<ProofData> = tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, &normalized_url))
        .flat_map(|t| t.proofs.clone())
        .filter(|p| p.state.is_spendable())
        .collect();
    Ok(proofs)
}
/// Estimate fee for a swap operation based on proof count
/// Note: For full swap fee estimation with FeeEstimate struct, use fees::estimate_swap_fee
pub async fn estimate_swap_proof_fee(mint_url: &str, proof_count: usize) -> Result<u64, String> {
    let wallet = get_or_create_wallet(mint_url).await?;
    let active_keyset = wallet
        .get_active_keyset()
        .await
        .map_err(|e| format!("Failed to get active keyset: {}", e))?;
    let fee_per_proof = active_keyset.input_fee_ppk.saturating_add(999) / 1000;
    let total_fee = fee_per_proof.saturating_mul(proof_count as u64);
    Ok(total_fee)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_swap_options() {
        let opts = SwapOptions::amount(100)
            .with_denomination(DenominationStrategy::Large)
            .with_include_fee(true);
        assert_eq!(opts.amount, Some(100));
        assert_eq!(opts.denomination, DenominationStrategy::Large);
        assert!(opts.include_fee);
    }
}
