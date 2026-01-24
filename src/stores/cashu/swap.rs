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
use dioxus::prelude::*;
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
use super::signals::{try_acquire_mint_lock, WALLET_BALANCE, WALLET_TOKENS};
use super::types::{
    ExtendedCashuProof, ExtendedTokenEvent, PendingEventType, ProofData, TokenData,
    WalletTokensStoreStoreExt,
};
use super::utils::{mint_matches, normalize_mint_url, now_secs};
use crate::stores::{auth_store, cashu_cdk_bridge, nostr_client};

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

    log::info!(
        "Executing NIP-60 swap: {} proofs ({} sats) at {}",
        input_count,
        input_value,
        mint_url
    );

    // 4. Get wallet and execute swap
    // NOTE: CDK's swap() has built-in try_proof_operation_or_reclaim - don't wrap again
    let wallet = get_or_create_wallet(&mint_url).await?;
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
        .await
        .map_err(|e| format!("Swap failed: {}", e))?;

    // 5. Handle Option<Proofs> return type
    // - amount=Some(x): Returns Some(proofs) to send, change stored internally
    // - amount=None: Returns None, ALL new proofs stored internally
    let output_proofs = match swap_result {
        Some(proofs) => proofs,
        None => {
            // For amount=None swaps, fetch new proofs from CDK localstore
            // They were stored as Unspent during the swap
            wallet
                .get_unspent_proofs()
                .await
                .map_err(|e| format!("Failed to get swapped proofs: {}", e))?
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
    let pending_event_id = format!("pending_{}", now_secs());
    update_local_state_after_swap(
        &mint_url,
        &output_proofs,
        &event_ids_to_delete,
        &pending_event_id,
    )?;

    // 7. Attempt Nostr publish (safe to fail - local state already updated)
    // nostr-sdk saves to local database before relay transmission
    let final_event_id =
        match publish_swap_events(&mint_url, &output_proofs, &event_ids_to_delete).await {
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
    let valid_created: Vec<String> = vec![final_event_id];
    let valid_destroyed: Vec<String> = event_ids_to_delete
        .iter()
        .filter(|id| EventId::from_hex(id).is_ok())
        .cloned()
        .collect();

    if let Err(e) =
        super::events::create_history_event("in", output_value, valid_created, valid_destroyed)
            .await
    {
        log::error!("Failed to create history event: {}", e);
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

/// Update local state after swap using Dioxus write pattern
fn update_local_state_after_swap(
    mint_url: &str,
    output_proofs: &[cdk::nuts::Proof],
    event_ids_to_delete: &[String],
    new_event_id: &str,
) -> Result<(), String> {
    // Convert proofs first (outside the lock)
    let proof_data: Vec<ProofData> = output_proofs.iter().map(cdk_proof_to_proof_data).collect();

    let new_token = TokenData {
        event_id: new_event_id.to_string(),
        mint: normalize_mint_url(mint_url),
        unit: "sat".to_string(),
        proofs: proof_data.clone(),
        created_at: now_secs(),
    };

    // Update WALLET_TOKENS
    let store = WALLET_TOKENS.read();
    let mut data = store.data();
    let mut tokens_write = data.write();

    // Remove old token events
    tokens_write.retain(|t| !event_ids_to_delete.contains(&t.event_id));

    // Add new token with output proofs
    tokens_write.push(new_token);

    // Calculate new balance while we have the lock
    let new_balance: u64 = tokens_write
        .iter()
        .flat_map(|t| &t.proofs)
        .filter(|p| p.state.is_spendable())
        .map(|p| p.amount)
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Balance overflow")?;

    // Drop write guard before updating balance
    drop(tokens_write);

    // Register proofs in event map
    register_proofs_in_event_map(new_event_id, &proof_data);

    // Update balance
    *WALLET_BALANCE.write() = new_balance;

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

    // Pre-compute event ID from unsigned event
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
            log::info!("Published swap token event: {}", event_id_hex);

            // Publish deletion events for old tokens
            publish_deletion_events(&client, event_ids_to_delete).await;
        }
        Err(e) => {
            log::warn!(
                "Failed to publish swap token event, queuing for retry: {}",
                e
            );
            queue_signed_event_for_retry(signed_event, PendingEventType::TokenEvent).await;
            // Still return the event ID - it will be published on retry
        }
    }

    Ok(event_id_hex)
}

/// Publish deletion events for consumed token events
async fn publish_deletion_events(client: &nostr_sdk::Client, event_ids_to_delete: &[String]) {
    if event_ids_to_delete.is_empty() {
        return;
    }

    let valid_event_ids: Vec<_> = event_ids_to_delete
        .iter()
        .filter(|id| EventId::from_hex(id).is_ok())
        .collect();

    if valid_event_ids.is_empty() {
        return;
    }

    let mut tags = Vec::new();
    for event_id in &valid_event_ids {
        if let Ok(eid) = EventId::from_hex(event_id) {
            tags.push(nostr_sdk::Tag::event(eid));
        }
    }
    tags.push(nostr_sdk::Tag::custom(
        nostr_sdk::TagKind::custom("k"),
        ["7375"],
    ));

    let deletion_builder =
        nostr_sdk::EventBuilder::new(Kind::from(5), "Swapped token").tags(tags);

    match client.send_event_builder(deletion_builder.clone()).await {
        Ok(_) => {
            log::info!(
                "Published deletion events for {} token events",
                valid_event_ids.len()
            );
        }
        Err(e) => {
            log::warn!("Failed to publish deletion event, queuing for retry: {}", e);
            super::events::queue_event_for_retry(deletion_builder, PendingEventType::DeletionEvent)
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
