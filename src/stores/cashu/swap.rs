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

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use cdk::nuts::SpendingConditions;
use dioxus::prelude::ReadableExt;
use nostr_sdk::signer::NostrSigner;
use nostr_sdk::{EventId, Kind, PublicKey};

use super::denomination::DenominationStrategy;
use super::events::{
    queue_signed_event_for_retry, update_token_event_id,
};
use super::internal::get_or_create_wallet;
use super::proofs::{
    cdk_proof_to_proof_data, get_event_ids_for_proofs, proof_data_to_cdk_proof,
    register_proofs_in_event_map,
};
use super::signals::{try_acquire_mint_lock, WALLET_TOKENS};
use super::types::{
    ExtendedCashuProof, ExtendedTokenEvent, InFlightSendRequest, PendingEventType, ProofData,
    TokenData, WalletTokensStoreStoreExt,
};
use super::utils::{mint_matches, normalize_mint_url, now_secs};
use crate::stores::{auth_store, cashu_cdk_bridge, nostr_client};

use super::signals::InFlightGuard;

// =============================================================================
// Swap Options
// =============================================================================

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

// =============================================================================
// Direct Swap Operations
// =============================================================================

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

    // Convert to CDK proofs
    let cdk_proofs: Vec<cdk::nuts::Proof> = input_proofs
        .iter()
        .map(proof_data_to_cdk_proof)
        .collect::<Result<Vec<_>, _>>()?;

    // Get wallet
    let wallet = get_or_create_wallet(mint_url).await?;

    // Determine amount and split target
    let amount = options.amount.map(cdk::Amount::from);
    let split_target = options.denomination.to_split_target();

    // Execute swap
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

    // Handle result
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

// =============================================================================
// NIP-60 Integrated Swap Operations
// =============================================================================

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

    // Normalize mint URL for consistent comparison
    let mint_url = normalize_mint_url(mint_url);

    // 1. Acquire mint lock
    let _lock = try_acquire_mint_lock(&mint_url)
        .ok_or_else(|| format!("Another operation in progress for {}", mint_url))?;

    // Validate that input_proofs is the COMPLETE spendable set for this mint
    // This prevents orphaned events from partial proof sets
    let all_spendable = get_proofs_for_mint(&mint_url)?;
    if input_proofs.len() != all_spendable.len() {
        // Quick check: if counts differ, definitely incomplete
        return Err(format!(
            "Incomplete proof set: got {} proofs, wallet has {} spendable proofs for {}. \
             Use get_proofs_for_mint() or a high-level function like swap_refresh().",
            input_proofs.len(),
            all_spendable.len(),
            mint_url
        ));
    }

    // Full validation: compare proof secrets to ensure exact match
    let input_secrets_set: std::collections::HashSet<_> = input_proofs
        .iter()
        .map(|p| &p.secret)
        .collect();
    let wallet_secrets: std::collections::HashSet<_> = all_spendable
        .iter()
        .map(|p| &p.secret)
        .collect();

    if input_secrets_set != wallet_secrets {
        return Err(
            "Proof set mismatch: input proofs don't match wallet's spendable proofs. \
             Ensure you're passing the complete set from get_proofs_for_mint().".to_string()
        );
    }

    // 2. Get event IDs for input proofs (for del tags)
    let input_secrets: Vec<String> = input_proofs.iter().map(|p| p.secret.clone()).collect();
    let event_ids_to_delete = get_event_ids_for_proofs(&input_secrets);

    // 3. Convert to CDK proofs and calculate input value
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

    // Track input Y values for filtering change proofs later
    // CDK stores change proofs internally, we need to identify them
    // Use fail-fast error handling - Y computation almost never fails for valid proofs
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

    // Generate transaction ID for tracking
    let tx_id = format!("swap_{}", uuid::Uuid::new_v4());

    // Collect proof secrets for in-flight tracking
    let proof_secrets: Vec<String> = cdk_proofs.iter().map(|p| p.secret.to_string()).collect();

    // SAFETY: Add in-flight tracking BEFORE the swap operation
    // This protects proofs from being reclaimed by proof_recovery
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

    // 4. Get wallet and execute swap
    // NOTE: CDK's swap() has built-in try_proof_operation_or_reclaim - don't wrap again
    let wallet = get_or_create_wallet(&mint_url).await?;

    // Snapshot pre-swap unspent Y values (CDK saga pattern - defensive filter data)
    // Ensures only THIS swap's proofs are returned, not pre-existing ones
    // Dioxus fail-fast pattern: use collect::<Result<_, _>>() instead of filter_map
    let pre_swap_proofs = wallet
        .get_unspent_proofs()
        .await
        .map_err(|e| format!("Failed to get pre-swap proofs: {}", e))?;

    let pre_swap_ys: HashSet<String> = pre_swap_proofs
        .iter()
        .map(|p| p.y().map(|y| y.to_string()))
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|e| format!("Failed to compute Y for pre-swap proof: {}", e))?;

    log::debug!("Pre-swap snapshot: {} existing unspent proofs", pre_swap_ys.len());

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

    // Dismiss guard and handle cleanup manually now that swap completed
    in_flight_guard.dismiss();
    super::signals::remove_in_flight_send_request(&tx_id);

    let swap_result = swap_result.map_err(|e| format!("Swap failed: {}", e))?;

    // 5. Handle Option<Proofs> return type
    // - amount=Some(x): Returns Some(proofs) to send, change stored internally
    // - amount=None: Returns None, ALL new proofs stored internally
    //
    // CRITICAL: For amount=Some(x), we must store BOTH the returned proofs AND
    // any change proofs that CDK stored internally. The returned proofs may be
    // for ourselves (e.g., swap_to_locked) or for sending to others.
    //
    // Per CDK pattern: send_proofs from swap() ARE the reserved proofs, so we
    // chain them with get_unspent_proofs() for change - no get_reserved_proofs().
    let output_proofs = match swap_result {
        Some(send_proofs) => {
            // For amount=Some(x) swaps, CDK returns proofs totaling x (send_proofs)
            // AND stores any change proofs internally as Unspent.
            //
            // Per CDK pattern (swap.rs:105-115), send_proofs IS the set that gets
            // marked as Reserved - no need for get_reserved_proofs() which may
            // include unrelated reserved proofs from previous operations.
            //
            // We merge send_proofs (from swap result) with change proofs (unspent).
            let all_unspent = wallet
                .get_unspent_proofs()
                .await
                .map_err(|e| format!("Failed to get proofs after swap: {}", e))?;

            // Merge send_proofs (from swap result) with change proofs (unspent)
            // No need for get_reserved_proofs() - send_proofs IS the reserved set
            //
            // FAIL-FAST PATTERN: Y() errors should fail the operation, not silently drop proofs
            // Silent filtering could cause fund loss if proofs are valid but Y() computation fails
            let mut seen_ys: HashSet<String> = HashSet::new();
            let merged: Vec<cdk::nuts::Proof> = send_proofs.iter()
                .chain(all_unspent.iter())
                .map(|p| {
                    let y = p.y().map_err(|e| format!(
                        "Failed to compute Y for proof (amount={} sats): {} - this indicates a critical error",
                        u64::from(p.amount), e
                    ))?;
                    Ok((p.clone(), y.to_string()))
                })
                .collect::<Result<Vec<_>, String>>()? // Fail-fast on first Y() error
                .into_iter()
                .filter(|(_, y_str)| {
                    // Skip if it's an input proof, pre-existing proof, or already seen
                    if input_ys.contains(y_str) || pre_swap_ys.contains(y_str) || seen_ys.contains(y_str) {
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
            // For amount=None swaps, ALL new proofs stored internally
            // Get all unspent and filter to only new proofs
            let all_unspent = wallet
                .get_unspent_proofs()
                .await
                .map_err(|e| format!("Failed to get swapped proofs: {}", e))?;

            // Filter to only NEW proofs (those with Y values not in input_ys)
            // Fail-fast on y() error to avoid silent fund loss (nostr-sdk pattern)
            let mut new_proofs = Vec::new();
            for p in all_unspent {
                let y = p.y().map_err(|e| {
                    format!(
                        "CRITICAL: Y_VALUE_COMPUTATION_FAILED - proof_amount={} sats, error='{}' - aborting to prevent fund loss",
                        u64::from(p.amount), e
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

    // 6. IMMEDIATELY update local state with pending event ID
    // This uses a pending event ID that we'll update after Nostr publish
    // If app crashes here, sync_orphaned_cdk_proofs_to_nostr() will recover on restart
    // Use UUID to prevent collision when multiple operations occur within same second
    let pending_event_id = format!("pending_{}", uuid::Uuid::new_v4());
    update_local_state_after_swap(
        &mint_url,
        &output_proofs,
        &event_ids_to_delete,
        &pending_event_id,
    )?;

    // 7. Attempt Nostr publish (safe to fail - local state already updated)
    // nostr-sdk saves to local database before relay transmission
    let final_event_id =
        match publish_swap_events(&mint_url, &output_proofs, &event_ids_to_delete, &pending_event_id).await {
            Ok(real_id) => {
                // Update token with real Nostr event ID
                update_token_event_id(&pending_event_id, &real_id);
                real_id
            }
            Err(e) => {
                // Event already queued for retry by publish_swap_events
                log::warn!("Nostr publish failed, queued for retry: {}", e);
                pending_event_id.clone()
            }
        };

    // 8. Create history event using nostr-sdk SpendingHistory
    // Use "in" direction - we're receiving new proofs (swap creates new tokens)
    // Filter out pending_* IDs - they're not valid hex event IDs
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

    // Skip history event if both are empty (no valid event IDs to reference)
    if !valid_created.is_empty() || !valid_destroyed.is_empty() {
        if let Err(e) =
            super::events::create_history_event("in", output_value, valid_created, valid_destroyed)
                .await
        {
            log::error!("Failed to create history event: {}", e);
        }
    }

    // 9. Sync CDK bridge state (non-critical)
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync wallet state: {}", e);
    }

    // Build result
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
    // CRITICAL: Validate output_proofs is non-empty before any destructive operation
    if output_proofs.is_empty() {
        return Err("Cannot update state with empty output proofs".to_string());
    }

    // Convert proofs first (outside the lock)
    let proof_data: Vec<ProofData> = output_proofs.iter().map(cdk_proof_to_proof_data).collect();

    let new_token = TokenData {
        event_id: new_event_id.to_string(),
        mint: normalize_mint_url(mint_url),
        unit: "sat".to_string(),
        proofs: proof_data.clone(),
        created_at: now_secs(),
    };

    // Crash-safe: ADD FIRST, then DELETE (via atomic_token_replace)
    // Note: atomic_token_replace already updates wallet balances internally
    let new_balance = super::signals::atomic_token_replace(vec![new_token], event_ids_to_delete)?;

    // Register proofs in event map
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

    // Convert CDK proofs to extended proof format
    let proof_data: Vec<ProofData> = output_proofs.iter().map(cdk_proof_to_proof_data).collect();
    let extended_proofs: Vec<ExtendedCashuProof> = proof_data
        .iter()
        .map(|p| ExtendedCashuProof::from(p.clone()))
        .collect();

    // Build ExtendedTokenEvent with del tags for consumed events
    // Filter out pending_* IDs - they're not valid hex event IDs for relay transmission
    let valid_del_ids: Vec<String> = event_ids_to_delete
        .iter()
        .filter(|id| !id.starts_with("pending_"))
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

    // Build and sign the event, then get ID from signed event directly
    let signed_event = builder
        .build(pubkey)
        .sign(&signer)
        .await
        .map_err(|e| format!("Failed to sign token event: {}", e))?;
    let event_id_hex = signed_event.id.to_hex();

    // Try to publish
    match client.send_event(&signed_event).await {
        Ok(output) => {
            // Check if at least one relay accepted (nostr-sdk Output<T> pattern)
            if output.success.is_empty() {
                log::warn!(
                    "No relays accepted swap token event (failed: {:?}), queuing for retry",
                    output.failed.keys().collect::<Vec<_>>()
                );
                queue_signed_event_for_retry(
                    signed_event,
                    PendingEventType::TokenEvent,
                    Some(pending_event_id.to_string()),
                    Some(mint_url.to_string()),
                ).await;

                // Queue deletion events for retry too
                queue_deletion_event_retry(&valid_del_ids).await;
                return Err("No relays accepted swap token event".to_string());
            }

            log::info!(
                "Published swap token event: {} (to {}/{} relays)",
                event_id_hex,
                output.success.len(),
                output.success.len() + output.failed.len()
            );

            // Publish deletion events for old tokens
            publish_deletion_events(&client, &valid_del_ids).await;
        }
        Err(e) => {
            log::warn!(
                "Failed to publish swap token event, queuing for retry: {}",
                e
            );
            // Use pending_event_id (not event_id_hex) so retry can find the token in WALLET_TOKENS
            queue_signed_event_for_retry(
                signed_event,
                PendingEventType::TokenEvent,
                Some(pending_event_id.to_string()),
                Some(mint_url.to_string()),
            ).await;

            // Also queue deletion events for retry if there are any
            queue_deletion_event_retry(&valid_del_ids).await;
            return Err(format!("Failed to publish swap token event: {}", e));
        }
    }

    Ok(event_id_hex)
}

/// Build deletion event tags for token events
///
/// CDK pattern: centralize tag building to avoid duplication between
/// queue_deletion_event_retry and publish_deletion_events
fn build_deletion_tags(event_ids: &[EventId]) -> Vec<nostr_sdk::Tag> {
    let mut tags: Vec<_> = event_ids.iter().map(|eid| nostr_sdk::Tag::event(*eid)).collect();
    tags.push(nostr_sdk::Tag::custom(nostr_sdk::TagKind::custom("k"), ["7375"]));
    tags
}

/// Queue deletion event for retry (extracted helper to reduce duplication)
///
/// CDK pattern: centralize deletion event queueing logic
async fn queue_deletion_event_retry(event_ids_to_delete: &[String]) {
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
    let deletion_builder =
        nostr_sdk::EventBuilder::new(Kind::from(5), "Swapped token").tags(tags);

    super::events::queue_event_for_retry(
        deletion_builder,
        PendingEventType::DeletionEvent,
        None,
        None,
    )
    .await;
}

/// Publish deletion events for consumed token events
async fn publish_deletion_events(client: &nostr_sdk::Client, event_ids_to_delete: &[String]) {
    if event_ids_to_delete.is_empty() {
        return;
    }

    // Parse once, store Vec<EventId> (avoid double parsing)
    let valid_event_ids: Vec<EventId> = event_ids_to_delete
        .iter()
        .filter_map(|id| EventId::from_hex(id).ok())
        .collect();

    if valid_event_ids.is_empty() {
        return;
    }

    let tags = build_deletion_tags(&valid_event_ids);
    let deletion_builder =
        nostr_sdk::EventBuilder::new(Kind::from(5), "Swapped token").tags(tags);

    match client.send_event_builder(deletion_builder.clone()).await {
        Ok(output) => {
            if output.success.is_empty() {
                log::warn!("No relays accepted deletion event, queuing for retry");
                super::events::queue_event_for_retry(deletion_builder, PendingEventType::DeletionEvent, None, None)
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
            super::events::queue_event_for_retry(deletion_builder, PendingEventType::DeletionEvent, None, None)
                .await;
        }
    }

    let invalid_count = event_ids_to_delete.len() - valid_event_ids.len();
    if invalid_count > 0 {
        log::warn!("Skipped {} invalid event IDs in deletion", invalid_count);
    }
}

// =============================================================================
// High-Level Swap Operations (NIP-60 integrated)
// =============================================================================

/// Swap all proofs for a mint to optimize denominations
///
/// Uses full NIP-60 state management - updates WALLET_TOKENS and publishes to Nostr.
pub async fn swap_optimize_denominations(
    mint_url: &str,
    strategy: DenominationStrategy,
) -> Result<SwapResult, String> {
    // Get all proofs for this mint (lock acquired inside execute_swap_with_nip60)
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
    // Get all proofs for this mint (lock acquired inside execute_swap_with_nip60)
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
    // Get all proofs for this mint (lock acquired inside execute_swap_with_nip60)
    let all_proofs = get_proofs_for_mint(mint_url)?;

    if all_proofs.is_empty() {
        return Err("No proofs found for this mint".to_string());
    }

    let options = SwapOptions::all().with_denomination(DenominationStrategy::PowerOfTwo);

    execute_swap_with_nip60(mint_url, all_proofs, options).await
}

// =============================================================================
// Helpers
// =============================================================================

/// Get all proofs for a mint from local storage
fn get_proofs_for_mint(mint_url: &str) -> Result<Vec<ProofData>, String> {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();

    // Normalize URL for comparison
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
    // Get keyset fee from cache or fetch
    let wallet = get_or_create_wallet(mint_url).await?;

    let active_keyset = wallet
        .get_active_keyset()
        .await
        .map_err(|e| format!("Failed to get active keyset: {}", e))?;

    // Fee is per proof: fee_ppk / 1000 (rounded up)
    // Use saturating arithmetic to prevent overflow
    let fee_per_proof = active_keyset.input_fee_ppk.saturating_add(999) / 1000;
    let total_fee = fee_per_proof.saturating_mul(proof_count as u64);

    Ok(total_fee)
}

// =============================================================================
// Tests
// =============================================================================

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
