//! Recovery and sync operations
//!
//! Functions for syncing proofs with mints, cleaning up spent proofs,
//! recovering from failures, and handling pending operations.
//!
//! Implements patterns for robust recovery.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use dioxus::prelude::*;
use cdk::nuts::State;

use super::errors::CashuResult;
use super::proofs::{
    cleanup_old_pending_at_mint, get_all_proofs_for_mint, get_pending_transactions,
    is_proof_pending_at_mint, move_proofs_to_spent, proof_data_to_cdk_proof,
    register_proofs_pending_at_mint, remove_from_pending_at_mint, revert_proofs_to_spendable,
    update_transaction_status,
};
use super::signals::MAX_SYNC_INPUT_SIZE;
use super::types::{*, InFlightMeltRequest};

use super::events::fetch_tokens;
use super::history::fetch_history;
use super::init::is_wallet_initialized;
use super::internal::inject_nip60_proofs_to_cdk;
use super::mint_mgmt::get_mints;
use super::signals::try_acquire_mint_lock;
use super::utils::now_secs;

/// Refresh wallet data by re-fetching tokens and history from Nostr
pub async fn refresh_wallet() -> Result<(), String> {
    if !is_wallet_initialized() {
        return Err("Wallet not initialized".to_string());
    }

    log::info!("Refreshing wallet data");

    // Fetch tokens from NIP-60
    fetch_tokens().await?;

    // Inject NIP-60 proofs into CDK database
    // This ensures CDK operations have access to the refreshed proofs
    if let Err(e) = inject_nip60_proofs_to_cdk().await {
        log::warn!("Failed to inject refreshed proofs to CDK: {}", e);
    }

    fetch_history().await?;

    Ok(())
}

/// Sync local proof states with all mints (NUT-07)
///
/// This function validates proof states with each mint to detect proofs that
/// were spent elsewhere. Wrapper around sync_state_with_all_mints.
pub async fn sync_proofs_with_mints() -> Result<SyncResult, String> {
    if !is_wallet_initialized() {
        return Ok(SyncResult::default());
    }

    sync_state_with_all_mints().await.map_err(|e| e.to_string())
}

/// Cleanup spent proofs for a specific mint
///
/// Checks proof states at the mint and removes spent/reserved/pending proofs.
/// The lock is acquired internally by sync_state_with_mint.
pub async fn cleanup_spent_proofs(mint_url: String) -> Result<(usize, u64), String> {
    // Sync state with mint to detect spent proofs
    // Lock is acquired internally by sync_state_with_mint
    let result = sync_state_with_mint(&mint_url).await.map_err(|e| e.to_string())?;

    Ok((result.proofs_cleaned, result.sats_cleaned))
}

// =============================================================================
// NUT-07 State Sync with Batch Pagination
// =============================================================================

/// Sync state with a specific mint using NUT-07
///
/// Implements batch pagination (MAX_SYNC_INPUT_SIZE = 200 proofs per batch)
/// to avoid mint API limits and timeouts on large wallets.
///
/// Handles three proof states:
/// - SPENT: Mark as spent, complete associated transactions
/// - PENDING: Register in PENDING_BY_MINT_SECRETS (lightning in-flight)
/// - UNSPENT: If was pending, payment failed - revert to spendable
///
/// Automatically acquires mint lock to prevent race conditions.
/// If lock is unavailable, returns early without syncing.
pub async fn sync_state_with_mint(mint_url: &str) -> CashuResult<SyncResult> {
    use crate::stores::cashu_cdk_bridge;

    // CDK best practice: Retry lock acquisition with exponential backoff
    // for critical recovery operations. This prevents silent skips when
    // transient operations are holding the lock.
    const MAX_RETRIES: u32 = 3;

    let _lock_guard = {
        let mut guard = None;
        for attempt in 0..MAX_RETRIES {
            match try_acquire_mint_lock(mint_url) {
                Some(g) => {
                    guard = Some(g);
                    break;
                }
                None => {
                    if attempt < MAX_RETRIES - 1 {
                        let delay_ms = 100 * 2_u32.pow(attempt); // 100ms, 200ms, 400ms
                        log::debug!(
                            "Lock busy for {}, retrying in {}ms (attempt {}/{})",
                            mint_url, delay_ms, attempt + 1, MAX_RETRIES
                        );
                        gloo_timers::future::TimeoutFuture::new(delay_ms).await;
                    }
                }
            }
        }

        match guard {
            Some(g) => g,
            None => {
                log::debug!(
                    "Could not acquire lock for {} after {} attempts, skipping sync",
                    mint_url, MAX_RETRIES
                );
                return Ok(SyncResult::default());
            }
        }
    };

    log::info!("Syncing state with mint: {}", mint_url);

    // CDK best practice: Cleanup stale pending-at-mint entries before sync
    // This prevents accumulation of entries from failed/abandoned lightning payments
    cleanup_old_pending_at_mint();

    let mut result = SyncResult::default();

    // Get all proofs for this mint
    let proofs = get_all_proofs_for_mint(mint_url);
    if proofs.is_empty() {
        log::debug!("No proofs to sync for mint {}", mint_url);
        return Ok(result);
    }

    log::debug!(
        "Syncing {} proofs for mint {} in batches of {}",
        proofs.len(),
        mint_url,
        MAX_SYNC_INPUT_SIZE
    );

    // Get wallet for this mint
    let wallet = cashu_cdk_bridge::get_wallet(mint_url)
        .await
        .map_err(|e| super::errors::CashuWalletError::MintConnection {
            mint_url: mint_url.to_string(),
            message: e,
        })?;

    // CDK best practice: Query mint info for batch size limits (NUT-06)
    // Use mint's reported limit if available, otherwise fall back to our default
    // Note: Most mints don't expose max_inputs, so we use our default for now
    let batch_size = match wallet.fetch_mint_info().await {
        Ok(Some(info)) => {
            // Check if NUT-07 is supported by this mint
            if info.nuts.nut07.supported {
                log::debug!("Mint {} supports NUT-07, using default batch size", mint_url);
            }
            // Most mints don't expose max_inputs limit, use our safe default
            MAX_SYNC_INPUT_SIZE
        }
        Ok(None) => {
            log::debug!("No mint info available for {}, using default batch size", mint_url);
            MAX_SYNC_INPUT_SIZE
        }
        Err(e) => {
            log::debug!("Could not fetch mint info for {}: {}, using default batch size", mint_url, e);
            MAX_SYNC_INPUT_SIZE
        }
    };

    // Process in batches
    for (batch_idx, batch) in proofs.chunks(batch_size).enumerate() {
        log::debug!(
            "Processing batch {} ({} proofs) for mint {}",
            batch_idx + 1,
            batch.len(),
            mint_url
        );

        // Convert to CDK proofs
        let cdk_proofs: Vec<cdk::nuts::Proof> = batch
            .iter()
            .filter_map(|p| proof_data_to_cdk_proof(p).ok())
            .collect();

        if cdk_proofs.is_empty() {
            continue;
        }

        // Check proof states with mint (NUT-07)
        // CDK best practice: If batch check fails, fall back to individual proofs
        // to avoid losing the entire batch due to one bad proof
        let states = match wallet.check_proofs_spent(cdk_proofs.clone()).await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Batch {} check failed: {}, trying individual proofs", batch_idx, e);

                // Fallback: check proofs individually to salvage what we can
                let mut individual_states = Vec::new();
                let mut failed_proofs = 0;

                for proof in &cdk_proofs {
                    match wallet.check_proofs_spent(vec![proof.clone()]).await {
                        Ok(mut s) => {
                            if !s.is_empty() {
                                individual_states.push(s.remove(0));
                            }
                        }
                        Err(individual_err) => {
                            log::warn!(
                                "Individual proof check failed for {}: {}",
                                &proof.secret.to_string()[..8],
                                individual_err
                            );
                            failed_proofs += 1;
                            // Skip this proof - will be checked on next sync
                        }
                    }
                }

                if individual_states.is_empty() {
                    log::warn!(
                        "Batch {}: all {} proofs failed individual check, skipping",
                        batch_idx, failed_proofs
                    );
                    continue; // Try next batch
                }

                log::info!(
                    "Batch {}: recovered {} of {} proofs via individual checks",
                    batch_idx,
                    individual_states.len(),
                    cdk_proofs.len()
                );
                individual_states
            }
        };

        // SAFETY: Warn if state count doesn't match proof count
        // CDK guarantees one state per Y-value, but network issues could cause misalignment
        if states.len() != batch.len() {
            let skipped = batch.len().saturating_sub(states.len());
            log::warn!(
                "State count mismatch: {} proofs, {} states - {} trailing proofs skipped (will retry next sync)",
                batch.len(), states.len(), skipped
            );
        }

        // Process each proof state
        for (proof, state) in batch.iter().zip(states.iter()) {
            match state.state {
                State::Spent => {
                    // Proof is spent - mark locally and clean up
                    move_proofs_to_spent(std::slice::from_ref(&proof.secret));
                    result.spent_found += 1;
                    result.sats_cleaned += proof.amount;

                    // Remove from pending-at-mint if it was there
                    if is_proof_pending_at_mint(&proof.secret) {
                        remove_from_pending_at_mint(std::slice::from_ref(&proof.secret));
                    }

                    log::debug!("Proof {} marked as spent ({} sats)", &proof.secret[..8], proof.amount);
                }
                State::Pending => {
                    // Proof is pending at mint (lightning in-flight)
                    result.pending_found += 1;
                    if !is_proof_pending_at_mint(&proof.secret) {
                        register_proofs_pending_at_mint(std::slice::from_ref(&proof.secret));
                        log::debug!("Proof {} registered as pending at mint", &proof.secret[..8]);
                    }
                }
                State::Unspent => {
                    // Proof is unspent at mint
                    if is_proof_pending_at_mint(&proof.secret) {
                        // Was pending but now unspent = payment failed, revert
                        remove_from_pending_at_mint(std::slice::from_ref(&proof.secret));
                        revert_proofs_to_spendable(std::slice::from_ref(&proof.secret));
                        log::info!(
                            "Proof {} reverted to spendable (payment failed)",
                            &proof.secret[..8]
                        );
                    }
                }
                State::Reserved => {
                    // Proof is reserved in CDK's local store (from interrupted PreparedSend)
                    // This typically means a previous operation was interrupted before confirm/cancel
                    //
                    // CDK best practice: Don't immediately revert proofs that are part of
                    // an active transaction - they may be legitimately reserved for an
                    // in-progress multi-step operation (e.g., swap, P2PK send).
                    //
                    // Only revert proofs that have no associated transaction (orphaned reserves)
                    // or proofs whose transaction has been pending for too long.
                    if proof.transaction_id.is_some() {
                        // Proof is part of an active transaction - don't revert yet.
                        // The transaction recovery logic will handle this case.
                        log::debug!(
                            "Proof {} is Reserved but has active transaction - skipping revert",
                            &proof.secret[..8]
                        );
                    } else {
                        // Orphaned reserve - no transaction ID means this was from a
                        // crashed/interrupted operation that never completed setup
                        revert_proofs_to_spendable(std::slice::from_ref(&proof.secret));
                        log::info!(
                            "Proof {} reverted from Reserved to Unspent (orphaned reserve)",
                            &proof.secret[..8]
                        );
                    }
                }
                State::PendingSpent => {
                    // Proof was sent but not yet confirmed
                    // This is similar to Pending - keep it pending until mint confirms
                    result.pending_found += 1;
                    if !is_proof_pending_at_mint(&proof.secret) {
                        register_proofs_pending_at_mint(std::slice::from_ref(&proof.secret));
                        log::debug!("Proof {} registered as pending (PendingSpent)", &proof.secret[..8]);
                    }
                }
            }
        }

        // Only count proofs that were actually checked (states.len() accounts for
        // conversion failures that reduced cdk_proofs from the original batch)
        result.proofs_cleaned += states.len();
    }

    // Sync CDK state to Dioxus signals
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync wallet state after NUT-07 check: {}", e);
    }

    log::info!(
        "Sync complete for {}: {} spent, {} sats cleaned",
        mint_url,
        result.spent_found,
        result.sats_cleaned
    );

    Ok(result)
}

/// Sync state with all mints
///
/// Uses parallel execution across mints for improved performance.
/// Each mint has its own lock via `try_acquire_mint_lock()`, so parallel ops are safe.
pub async fn sync_state_with_all_mints() -> CashuResult<SyncResult> {
    let mints = get_mints();

    if mints.is_empty() {
        return Ok(SyncResult::default());
    }

    // Parallel execution across mints - WASM compatible via futures::future::join_all
    let futures: Vec<_> = mints
        .iter()
        .map(|mint_url| sync_state_with_mint(mint_url))
        .collect();

    let results = futures::future::join_all(futures).await;

    // Aggregate results
    let mut total_result = SyncResult::default();
    for (mint_url, result) in mints.iter().zip(results.into_iter()) {
        match result {
            Ok(r) => {
                total_result.spent_found += r.spent_found;
                total_result.proofs_cleaned += r.proofs_cleaned;
                total_result.sats_cleaned += r.sats_cleaned;
                total_result.pending_found += r.pending_found;
            }
            Err(e) => {
                log::warn!("Failed to sync with mint {}: {}", mint_url, e);
            }
        }
    }

    Ok(total_result)
}

// =============================================================================
// Pending Operation Recovery
// =============================================================================

/// Recover pending operations on startup
///
/// Recovers all pending operations when the
/// app starts. This ensures no payments get stuck due to app crashes or
/// network issues.
///
/// Handles:
/// - Pending topups (mint quotes)
/// - Pending transfers (melt quotes)
/// - Pending sends
pub async fn recover_pending_operations() -> CashuResult<()> {
    log::info!("Recovering pending operations...");

    let pending_txs = get_pending_transactions();

    if pending_txs.is_empty() {
        log::debug!("No pending transactions to recover");
        return Ok(());
    }

    log::info!("Found {} pending transactions to recover", pending_txs.len());

    for tx in pending_txs {
        match tx.tx_type {
            TransactionType::Topup => {
                if let Some(ref quote_id) = tx.quote_id {
                    match recover_mint_quote(&tx.mint_url, quote_id).await {
                        Ok(result) => {
                            if result.recovered_amount > 0 {
                                log::info!(
                                    "Recovered {} sats from mint quote {}",
                                    result.recovered_amount,
                                    quote_id
                                );
                                update_transaction_status(
                                    tx.id,
                                    TransactionStatus::Recovered,
                                    result.message,
                                    None,
                                );
                            }
                        }
                        Err(e) => {
                            log::warn!("Failed to recover mint quote {}: {}", quote_id, e);
                        }
                    }
                }
            }
            TransactionType::Transfer => {
                if let Some(ref quote_id) = tx.quote_id {
                    match recover_melt_quote_change(&tx.mint_url, quote_id).await {
                        Ok(result) => {
                            if result.recovered_amount > 0 {
                                log::info!(
                                    "Recovered {} sats change from melt quote {}",
                                    result.recovered_amount,
                                    quote_id
                                );
                                update_transaction_status(
                                    tx.id,
                                    TransactionStatus::Recovered,
                                    result.message,
                                    None,
                                );
                            }
                        }
                        Err(e) => {
                            log::warn!("Failed to recover melt quote change {}: {}", quote_id, e);
                        }
                    }
                }
            }
            TransactionType::Send => {
                // For pending sends, sync with mint to check if proofs were spent
                match sync_state_with_mint(&tx.mint_url).await {
                    Ok(result) => {
                        if result.spent_found > 0 {
                            log::info!(
                                "Send tx {} completed (proofs spent at mint)",
                                tx.id
                            );
                            update_transaction_status(
                                tx.id,
                                TransactionStatus::Completed,
                                Some("Proofs confirmed spent at mint".to_string()),
                                None,
                            );
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to sync for pending send {}: {}", tx.id, e);
                    }
                }
            }
            _ => {
                // Other transaction types - just sync with mint
                let _ = sync_state_with_mint(&tx.mint_url).await;
            }
        }
    }

    log::info!("Pending operation recovery complete");
    Ok(())
}

// =============================================================================
// CDK → NIP-60 Orphan Sync (Fund Safety)
// =============================================================================

/// Maximum proofs per batch for NUT-07 state check
const ORPHAN_SYNC_BATCH_SIZE: usize = 100;

/// Maximum errors per mint before stopping orphan sync to prevent unbounded accumulation
const MAX_ERRORS_PER_MINT: usize = 5;

/// Maximum age for in-flight melt requests before forced cleanup (24 hours)
/// CDK keeps sagas indefinitely, but client-side wallets need expiry for UX.
const IN_FLIGHT_MAX_AGE_SECS: u64 = 24 * 60 * 60;

/// Sync orphaned proofs from CDK to NIP-60
///
/// Detects proofs that exist in CDK's IndexedDB but are not in WALLET_TOKENS.
/// Uses Y values (not secrets) for proof identification per CDK pattern.
/// Verifies with mint before assuming proofs are orphaned (NUT-07).
///
/// This function recovers proofs from crashed send/melt operations:
/// - CDK's confirm() commits proofs to IndexedDB
/// - If app crashes before Nostr publish, proofs exist only in CDK
/// - This function detects and publishes those orphaned proofs
///
/// CRITICAL SAFETY: Never publishes proofs without mint verification.
pub async fn sync_orphaned_cdk_proofs_to_nostr() -> CashuResult<OrphanSyncResult> {
    use crate::stores::cashu_cdk_bridge;
    use cdk::dhke::hash_to_curve;
    use super::proofs::cdk_proof_to_proof_data;
    use super::signals::WALLET_TOKENS;
    use std::collections::HashSet;

    log::info!("Checking for orphaned CDK proofs not in NIP-60...");

    let mut result = OrphanSyncResult::default();

    // Get all proof Y values currently in WALLET_TOKENS (NIP-60 source of truth)
    // CDK best practice: Use Y values for proof identification, not secrets
    // Mutable so we can update it after publishing new proofs
    let mut known_ys: HashSet<String> = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        tokens
            .iter()
            .flat_map(|t| {
                t.proofs.iter().filter_map(|p| {
                    // Compute Y = hash_to_curve(secret)
                    hash_to_curve(p.secret.as_bytes())
                        .ok()
                        .map(|y| y.to_string())
                })
            })
            .collect()
    };

    // Get MultiMintWallet - use clone to avoid holding read lock across await
    let multi_wallet = match cashu_cdk_bridge::MULTI_WALLET.read().clone() {
        Some(w) => w,
        None => {
            log::debug!("MultiMintWallet not initialized, skipping orphan sync");
            return Ok(result);
        }
    };

    // Check each wallet's proofs
    let wallets = multi_wallet.get_wallets().await;
    for wallet in wallets.iter() {
        let mint_url = wallet.mint_url.to_string();

        let cdk_proofs = match wallet.get_unspent_proofs().await {
            Ok(proofs) => proofs,
            Err(e) => {
                log::warn!("Failed to get CDK proofs for {}: {}", mint_url, e);
                continue;
            }
        };

        // Find potentially orphaned proofs (in CDK but not in WALLET_TOKENS)
        // Use Y values for comparison per CDK pattern
        // SAFETY: If Y computation fails, skip the proof to prevent balance inflation
        let potentially_orphaned: Vec<_> = cdk_proofs
            .iter()
            .filter(|p| {
                match p.y() {
                    Ok(y) => !known_ys.contains(&y.to_string()),
                    Err(e) => {
                        log::warn!(
                            "Y_VALUE_COMPUTATION_FAILED in orphan check: proof_amount={}, error='{}' - skipping",
                            u64::from(p.amount), e
                        );
                        false  // Skip this proof
                    }
                }
            })
            .cloned()
            .collect();

        if potentially_orphaned.is_empty() {
            continue;
        }

        log::info!(
            "Found {} potentially orphaned proofs in CDK for {}, verifying with mint...",
            potentially_orphaned.len(),
            mint_url
        );

        // CDK best practice: Verify with mint before assuming orphaned (NUT-07)
        // Process in batches of ORPHAN_SYNC_BATCH_SIZE
        let mut confirmed_orphaned = Vec::new();
        let mut per_mint_errors = 0;  // Track errors per-mint, not global

        for chunk in potentially_orphaned.chunks(ORPHAN_SYNC_BATCH_SIZE) {
            match wallet.check_proofs_spent(chunk.to_vec()).await {
                Ok(states) => {
                    // CRITICAL: Only consider proofs that mint confirms as UNSPENT
                    for (proof, state) in chunk.iter().zip(states.iter()) {
                        if state.state == cdk::nuts::State::Unspent {
                            confirmed_orphaned.push(proof.clone());
                        } else {
                            log::debug!(
                                "Proof Y={} is {:?} at mint, skipping (not truly orphaned)",
                                proof.y().map(|y| y.to_string()).unwrap_or_default(),
                                state.state
                            );
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to verify proofs with mint {}: {}", mint_url, e);
                    // SAFETY: Don't assume orphaned on verification failure - skip this batch
                    // This prevents publishing spent proofs on network errors
                    result
                        .errors
                        .push(format!("Mint verification failed for {}: {}", mint_url, e));

                    // Track per-mint errors (not global) to prevent one mint's issues
                    // from affecting processing of other mints
                    per_mint_errors += 1;
                    if per_mint_errors >= MAX_ERRORS_PER_MINT {
                        log::warn!(
                            "Stopping orphan sync for {} after {} errors",
                            mint_url,
                            per_mint_errors
                        );
                        break; // Exit the batch loop for this mint only
                    }
                }
            }
        }

        if confirmed_orphaned.is_empty() {
            continue;
        }

        let orphaned_amount: u64 = confirmed_orphaned
            .iter()
            .map(|p| u64::from(p.amount))
            .fold(0, |acc, a| acc.saturating_add(a));

        log::info!(
            "Confirmed {} orphaned proofs ({} sats) in CDK for {}, publishing to Nostr...",
            confirmed_orphaned.len(),
            orphaned_amount,
            mint_url
        );

        // Convert to ProofData
        let proof_data: Vec<ProofData> = confirmed_orphaned
            .iter()
            .map(cdk_proof_to_proof_data)
            .collect();

        // Create and publish token event for orphaned proofs
        // nostr-sdk will save locally before attempting relay publish
        match super::events::publish_orphaned_proofs_event(&mint_url, &proof_data).await {
            Ok(event_id) => {
                log::info!("Published orphaned proofs to Nostr: {}", event_id);

                // Normalize mint URL for consistent balance lookups
                let normalized_mint = super::utils::normalize_mint_url(&mint_url);

                // Atomically add to WALLET_TOKENS and update WALLET_BALANCES
                let new_token = TokenData {
                    event_id: event_id.clone(),
                    mint: normalized_mint,
                    unit: "sat".to_string(),
                    proofs: proof_data.clone(),
                    created_at: now_secs(),
                };

                if let Err(e) = super::signals::atomic_token_update(|tokens| {
                    tokens.push(new_token);
                    Ok(())
                }) {
                    log::error!("Failed to update tokens atomically: {}", e);
                }

                // Register proofs in event map for fast lookup
                super::proofs::register_proofs_in_event_map(&event_id, &proof_data);

                // Update known_ys with newly-published proof Y values
                // This prevents re-processing these proofs in subsequent wallet iterations
                for proof in &confirmed_orphaned {
                    if let Ok(y) = proof.y() {
                        known_ys.insert(y.to_string());
                    }
                }

                result.proofs_recovered += confirmed_orphaned.len();
                result.sats_recovered += orphaned_amount;
            }
            Err(e) => {
                log::warn!("Failed to publish orphaned proofs: {}", e);
                result.errors.push(e);
            }
        }
    }

    if result.proofs_recovered > 0 {
        log::info!(
            "Recovered {} orphaned proofs ({} sats) from CDK to NIP-60",
            result.proofs_recovered,
            result.sats_recovered
        );
    }

    Ok(result)
}

// =============================================================================
// Quote Recovery
// =============================================================================

/// Recover a mint quote (incomplete topup)
///
/// Checks the quote state at the mint and:
/// - If PAID: Mints the tokens that were paid for
/// - If ISSUED: Checks for unrecorded proofs
/// - If UNPAID + expired: Marks as failed
pub async fn recover_mint_quote(mint_url: &str, quote_id: &str) -> CashuResult<RecoveryResult> {
    use crate::stores::cashu_cdk_bridge;
    use cdk::nuts::MintQuoteState;

    log::info!("Recovering mint quote {} from {}", quote_id, mint_url);

    let wallet = cashu_cdk_bridge::get_wallet(mint_url)
        .await
        .map_err(|e| super::errors::CashuWalletError::MintConnection {
            mint_url: mint_url.to_string(),
            message: e,
        })?;

    // Check quote state at mint
    let quote_state = wallet
        .mint_quote_state(quote_id)
        .await
        .map_err(|e| super::errors::CashuWalletError::QuoteFailed {
            message: format!("Failed to check quote state: {}", e),
        })?;

    #[allow(unreachable_patterns)] // Forward compatibility for future CDK states
    match quote_state.state {
        MintQuoteState::Paid => {
            // Invoice was paid but tokens not minted - complete the mint
            log::info!("Quote {} is paid, minting tokens...", quote_id);

            let proofs = wallet
                .mint(quote_id, cdk::amount::SplitTarget::default(), None)
                .await
                .map_err(super::errors::CashuWalletError::Cdk)?;

            let amount: u64 = proofs
                .iter()
                .map(|p| u64::from(p.amount))
                .fold(0u64, |acc, amt| acc.saturating_add(amt));

            // Sync wallet state to pick up new proofs
            let _ = cashu_cdk_bridge::sync_wallet_state().await;

            Ok(RecoveryResult {
                recovered_amount: amount,
                message: Some(format!("Minted {} sats from paid quote", amount)),
            })
        }
        MintQuoteState::Issued => {
            // Already issued - might have proofs we don't know about
            // Try to recover any unrecorded proofs from CDK database
            log::info!("Quote {} already issued, checking for unrecorded proofs...", quote_id);
            recover_unrecorded_proofs(mint_url).await
        }
        MintQuoteState::Unpaid => {
            // Check if expired
            if let Some(expiry) = quote_state.expiry {
                let now = js_sys::Date::now() as u64 / 1000;
                if now >= expiry {
                    log::info!("Quote {} expired", quote_id);
                    return Ok(RecoveryResult {
                        recovered_amount: 0,
                        message: Some("Quote expired".to_string()),
                    });
                }
            }
            // Still unpaid, nothing to recover
            Ok(RecoveryResult::none())
        }
        _ => {
            log::debug!("Quote {} in state {:?}, nothing to recover", quote_id, quote_state.state);
            Ok(RecoveryResult::none())
        }
    }
}

/// Recover change from a melt quote (incomplete lightning payment)
///
/// If the melt was paid, checks for change proofs we might not have recorded.
pub async fn recover_melt_quote_change(
    mint_url: &str,
    quote_id: &str,
) -> CashuResult<RecoveryResult> {
    use crate::stores::cashu_cdk_bridge;
    use cdk::nuts::MeltQuoteState;

    log::info!("Recovering melt quote change {} from {}", quote_id, mint_url);

    let wallet = cashu_cdk_bridge::get_wallet(mint_url)
        .await
        .map_err(|e| super::errors::CashuWalletError::MintConnection {
            mint_url: mint_url.to_string(),
            message: e,
        })?;

    // Check quote state at mint
    let quote_status = wallet
        .melt_quote_status(quote_id)
        .await
        .map_err(|e| super::errors::CashuWalletError::QuoteFailed {
            message: format!("Failed to check melt quote state: {}", e),
        })?;

    #[allow(unreachable_patterns)] // Forward compatibility for future CDK states
    match quote_status.state {
        MeltQuoteState::Paid => {
            // Payment completed - check for unrecorded change proofs
            log::info!("Melt quote {} paid, checking for change proofs...", quote_id);
            recover_unrecorded_proofs(mint_url).await
        }
        MeltQuoteState::Pending => {
            // Still pending - nothing to recover yet
            log::debug!("Melt quote {} still pending", quote_id);
            Ok(RecoveryResult::none())
        }
        MeltQuoteState::Unpaid => {
            // Payment failed or expired
            log::info!("Melt quote {} unpaid/failed", quote_id);
            Ok(RecoveryResult::none())
        }
        _ => {
            log::debug!("Melt quote {} in state {:?}", quote_id, quote_status.state);
            Ok(RecoveryResult::none())
        }
    }
}

/// Recover any unrecorded proofs from CDK database
///
/// Compares CDK's stored proofs with our known proofs and adds any missing ones.
async fn recover_unrecorded_proofs(mint_url: &str) -> CashuResult<RecoveryResult> {
    use crate::stores::cashu_cdk_bridge;

    let wallet = cashu_cdk_bridge::get_wallet(mint_url)
        .await
        .map_err(|e| super::errors::CashuWalletError::MintConnection {
            mint_url: mint_url.to_string(),
            message: e,
        })?;

    // Get all unspent proofs from CDK database
    let cdk_proofs = wallet
        .get_unspent_proofs()
        .await
        .map_err(super::errors::CashuWalletError::Cdk)?;

    // Get our known proof secrets
    let known_proofs = get_all_proofs_for_mint(mint_url);
    let known_secrets: std::collections::HashSet<String> =
        known_proofs.iter().map(|p| p.secret.clone()).collect();

    // Find proofs in CDK that we don't have
    let missing: Vec<_> = cdk_proofs
        .iter()
        .filter(|p| !known_secrets.contains(&p.secret.to_string()))
        .collect();

    if missing.is_empty() {
        return Ok(RecoveryResult::none());
    }

    let recovered_amount: u64 = missing
        .iter()
        .map(|p| u64::from(p.amount))
        .fold(0u64, |acc, amt| acc.saturating_add(amt));

    log::info!(
        "Found {} unrecorded proofs ({} sats) for mint {}",
        missing.len(),
        recovered_amount,
        mint_url
    );

    // Sync wallet state to pick up these proofs
    let _ = cashu_cdk_bridge::sync_wallet_state().await;

    Ok(RecoveryResult {
        recovered_amount,
        message: Some(format!(
            "Recovered {} unrecorded proofs ({} sats)",
            missing.len(),
            recovered_amount
        )),
    })
}

// =============================================================================
// In-Flight Melt Quote Recovery (Crash Recovery)
// =============================================================================

/// Recover all pending (in-flight) melt quotes on startup
///
/// SAFETY: This function MUST verify proof states with mint (NUT-07) before
/// reverting ANY proofs. Never assume proofs are unspent based on quote state alone.
///
/// CDK MeltQuoteState values: Unpaid, Paid, Pending, Failed, Unknown
///
/// This is called during startup to recover from crashes that occurred during
/// melt operations. The in-flight melt requests contain the exact proofs used
/// and quote IDs needed for recovery.
pub async fn recover_all_pending_melt_quotes() -> CashuResult<MeltRecoveryResult> {
    use crate::stores::cashu_cdk_bridge;
    use cdk::nuts::MeltQuoteState;
    use super::signals::{
        IN_FLIGHT_MELT_REQUESTS, remove_in_flight_melt_request,
        persist_in_flight_melt_requests,
    };

    log::info!("Recovering in-flight melt requests...");

    // Snapshot the current in-flight requests
    let in_flight = IN_FLIGHT_MELT_REQUESTS.read().clone();

    if in_flight.is_empty() {
        log::debug!("No in-flight melt requests to recover");
        return Ok(MeltRecoveryResult::default());
    }

    log::info!("Found {} in-flight melt requests to recover", in_flight.len());

    let mut result = MeltRecoveryResult::default();

    for request in in_flight {
        let now = now_secs();
        let age = now.saturating_sub(request.created_at);

        // Check if request is expired (older than 24 hours)
        if age > IN_FLIGHT_MAX_AGE_SECS {
            log::warn!(
                "In-flight melt request {} is {} hours old, forcing cleanup",
                request.quote_id,
                age / 3600
            );

            // Try one final NUT-07 check
            let wallet = match cashu_cdk_bridge::get_wallet(&request.mint_url).await {
                Ok(w) => w,
                Err(_) => {
                    // Mint unreachable - remove tracking, orphan sync will handle proofs
                    log::warn!("Mint {} unreachable for expired request, removing tracking", request.mint_url);
                    remove_in_flight_melt_request(&request.transaction_id);
                    continue;
                }
            };

            // Final proof state check using paired approach to maintain alignment
            // between original ProofData and CDK proofs
            let valid_pairs: Vec<(&ProofData, cdk::nuts::Proof)> = request.proofs_used.iter()
                .filter_map(|p| proof_data_to_cdk_proof(p).ok().map(|cdk_p| (p, cdk_p)))
                .collect();

            if !valid_pairs.is_empty() {
                let cdk_proofs: Vec<_> = valid_pairs.iter().map(|(_, cdk_p)| cdk_p.clone()).collect();

                if let Ok(states) = wallet.check_proofs_spent(cdk_proofs).await {
                    // Mark spent proofs using original proof secrets for correct alignment
                    let spent_secrets: Vec<_> = valid_pairs.iter()
                        .zip(states.iter())
                        .filter(|(_, s)| s.state == State::Spent)
                        .map(|((original, _), _)| original.secret.clone())
                        .collect();

                    if !spent_secrets.is_empty() {
                        move_proofs_to_spent(&spent_secrets);
                    }
                }
            }

            // Remove expired request - proofs safe in CDK, orphan sync handles recovery
            remove_in_flight_melt_request(&request.transaction_id);
            result.quotes_checked += 1;
            continue;
        }

        // SAFETY: Acquire mint lock to prevent concurrent operations
        let _guard = match try_acquire_mint_lock(&request.mint_url) {
            Some(g) => g,
            None => {
                // Mint is busy - skip this request, will retry next startup
                log::warn!("Mint {} busy, skipping recovery for quote {}", request.mint_url, request.quote_id);
                result.errors.push(format!("Mint {} busy, skipping", request.mint_url));
                continue;
            }
        };

        // Get wallet for this mint
        let wallet = match cashu_cdk_bridge::get_wallet(&request.mint_url).await {
            Ok(w) => w,
            Err(e) => {
                log::warn!("Failed to get wallet for {}: {}", request.mint_url, e);
                result.errors.push(format!("Wallet error for {}: {}", request.mint_url, e));
                continue;
            }
        };

        // Check quote status at mint
        let quote_status = match wallet.melt_quote_status(&request.quote_id).await {
            Ok(status) => status,
            Err(e) => {
                log::warn!("Failed to check quote {} status: {}", request.quote_id, e);
                result.errors.push(format!("Quote {} check failed: {}", request.quote_id, e));
                // Keep tracking - don't remove from in-flight
                continue;
            }
        };

        log::info!("Quote {} status: {:?}", request.quote_id, quote_status.state);

        #[allow(unreachable_patterns)] // Forward compatibility for future CDK states
        match quote_status.state {
            MeltQuoteState::Paid => {
                // Payment succeeded - recover change proofs from mint
                // SAFETY: Deduplicate change proofs before adding (Risk 6)
                log::info!("Melt quote {} was paid, recovering change proofs...", request.quote_id);

                match recover_melt_change_deduplicated(&wallet, &request).await {
                    Ok(change_amount) => {
                        result.change_recovered += change_amount;
                        result.quotes_paid += 1;
                        log::info!("Recovered {} sats change from quote {}", change_amount, request.quote_id);
                        // Only remove from in-flight on success
                        remove_in_flight_melt_request(&request.transaction_id);
                    }
                    Err(e) => {
                        // Keep in-flight for retry on next restart
                        log::warn!("Failed to recover change for quote {}, keeping for retry: {}", request.quote_id, e);
                        result.errors.push(format!("Change recovery failed for {}: {}", request.quote_id, e));
                    }
                }
            }
            MeltQuoteState::Pending => {
                // Lightning still in-flight, keep tracking (don't revert proofs!)
                // CDK saga pattern: Pending can transition to Paid, Unpaid, or Failed
                log::info!("Melt quote {} still pending, keeping in tracking", request.quote_id);
                result.quotes_checked += 1;
                // DO NOT remove from in-flight - must keep tracking
            }
            MeltQuoteState::Failed | MeltQuoteState::Unpaid | MeltQuoteState::Unknown => {
                // CRITICAL SAFETY: Do NOT blindly revert proofs!
                // Quote state does NOT tell us if proofs were spent.
                // MUST verify with mint (NUT-07) before any revert.
                log::info!(
                    "Melt quote {} is {:?}, verifying proof states with mint...",
                    request.quote_id, quote_status.state
                );

                // Step 1: Convert proofs for NUT-07 check, keeping pairs aligned
                let valid_pairs: Vec<(&ProofData, cdk::nuts::Proof)> = request.proofs_used.iter()
                    .filter_map(|p| proof_data_to_cdk_proof(p).ok().map(|cdk_p| (p, cdk_p)))
                    .collect();

                if valid_pairs.is_empty() {
                    log::warn!("No valid proofs to check for quote {}", request.quote_id);
                    remove_in_flight_melt_request(&request.transaction_id);
                    continue;
                }

                let cdk_proofs: Vec<_> = valid_pairs.iter().map(|(_, cdk_p)| cdk_p.clone()).collect();

                // Step 2: Check actual proof states at mint (NUT-07)
                let proof_states = match wallet.check_proofs_spent(cdk_proofs).await {
                    Ok(states) => states,
                    Err(e) => {
                        // Network error - DO NOT revert, keep tracking for next startup
                        log::warn!("NUT-07 check failed for {}: {}", request.mint_url, e);
                        result.errors.push(format!("NUT-07 check failed for {}: {}", request.mint_url, e));
                        continue;
                    }
                };

                // Step 3: Categorize proofs by mint state (now properly aligned)
                let mut unspent_secrets = Vec::new();
                let mut spent_secrets = Vec::new();
                let mut pending_count = 0;

                for ((original_proof, _), state_info) in valid_pairs.iter().zip(proof_states.iter()) {
                    match state_info.state {
                        State::Unspent => unspent_secrets.push(original_proof.secret.clone()),
                        State::Spent => spent_secrets.push(original_proof.secret.clone()),
                        State::Pending | State::PendingSpent => pending_count += 1,
                        State::Reserved => {} // Don't touch - may be part of active operation
                    }
                }

                // Step 4: Mark spent proofs as spent locally (sync state)
                if !spent_secrets.is_empty() {
                    log::info!("Marking {} proofs as spent (confirmed by mint)", spent_secrets.len());
                    move_proofs_to_spent(&spent_secrets);
                }

                // Step 5: ONLY revert proofs confirmed UNSPENT by mint
                if !unspent_secrets.is_empty() {
                    log::info!("Reverting {} proofs to spendable (confirmed unspent by mint)", unspent_secrets.len());
                    revert_proofs_to_spendable(&unspent_secrets);
                }

                // Step 6: Leave pending proofs alone (lightning may still complete)
                if pending_count > 0 {
                    log::info!("Leaving {} proofs as pending at mint", pending_count);
                }

                // Remove from in-flight tracking
                remove_in_flight_melt_request(&request.transaction_id);
                result.quotes_checked += 1;
            }
            _ => {
                // Unknown state - keep tracking for safety
                log::debug!("Melt quote {} in unknown state {:?}", request.quote_id, quote_status.state);
                result.quotes_checked += 1;
            }
        }
    }

    // Persist the updated in-flight requests
    if let Err(e) = persist_in_flight_melt_requests().await {
        log::warn!("Failed to persist in-flight melt requests after recovery: {}", e);
    }

    // Sync wallet state to update UI
    if result.change_recovered > 0 || result.quotes_checked > 0 {
        if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
            log::warn!("Failed to sync wallet state after melt recovery: {}", e);
        }
    }

    log::info!(
        "In-flight melt recovery complete: {} checked, {} paid, {} sats recovered, {} errors",
        result.quotes_checked, result.quotes_paid, result.change_recovered, result.errors.len()
    );

    Ok(result)
}

/// Recover change proofs from a paid melt quote with deduplication
///
/// SAFETY: Deduplicates by Y value before adding to prevent phantom balance
/// from proofs that may already exist in WALLET_TOKENS.
async fn recover_melt_change_deduplicated(
    wallet: &cdk::Wallet,
    request: &InFlightMeltRequest,
) -> Result<u64, String> {
    use cdk::dhke::hash_to_curve;
    use super::signals::WALLET_TOKENS;
    use std::collections::HashSet;

    // Get all unspent proofs from CDK (these include any change from the melt)
    let cdk_proofs = wallet
        .get_unspent_proofs()
        .await
        .map_err(|e| format!("Failed to get proofs: {}", e))?;

    // Get existing Y values from WALLET_TOKENS to prevent duplicates
    let existing_ys: HashSet<String> = {
        let store = WALLET_TOKENS.read();
        let data = store.data();
        let tokens = data.read();
        tokens
            .iter()
            .flat_map(|t| {
                t.proofs.iter().filter_map(|p| {
                    hash_to_curve(p.secret.as_bytes())
                        .ok()
                        .map(|y| y.to_string())
                })
            })
            .collect()
    };

    // Find proofs in CDK that are NOT already in WALLET_TOKENS (the change proofs)
    // SAFETY: If Y computation fails, treat as duplicate to prevent balance inflation
    let new_proofs: Vec<_> = cdk_proofs
        .iter()
        .filter(|p| {
            match p.y() {
                Ok(y) => !existing_ys.contains(&y.to_string()),
                Err(e) => {
                    // SAFETY: If we can't compute Y, assume it's a duplicate
                    // This prevents balance inflation from Y computation failures
                    log::warn!(
                        "Y_VALUE_COMPUTATION_FAILED: proof_amount={}, error='{}' - treating as duplicate",
                        u64::from(p.amount), e
                    );
                    false  // Treat as duplicate = don't add
                }
            }
        })
        .collect();

    if new_proofs.is_empty() {
        log::debug!("No new change proofs to add for quote {}", request.quote_id);
        return Ok(0);
    }

    let recovered_amount: u64 = new_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .fold(0, |acc, a| acc.saturating_add(a));

    log::info!(
        "Found {} new change proofs ({} sats) for quote {}",
        new_proofs.len(),
        recovered_amount,
        request.quote_id
    );

    // Convert to ProofData and publish to Nostr
    let proof_data: Vec<ProofData> = new_proofs
        .iter()
        .map(|p| super::proofs::cdk_proof_to_proof_data(p))
        .collect();

    // Publish orphaned proofs event (uses existing event publishing logic)
    match super::events::publish_orphaned_proofs_event(&request.mint_url, &proof_data).await {
        Ok(event_id) => {
            log::info!("Published recovered change proofs to Nostr: {}", event_id);

            // Normalize mint URL for consistent balance lookups
            let normalized_mint = super::utils::normalize_mint_url(&request.mint_url);

            // Add to WALLET_TOKENS
            {
                let store = WALLET_TOKENS.read();
                let mut data = store.data();
                let mut tokens = data.write();
                tokens.push(TokenData {
                    event_id: event_id.clone(),
                    mint: normalized_mint,
                    unit: "sat".to_string(),
                    proofs: proof_data.clone(),
                    created_at: now_secs(),
                });
            }

            // Update balance from proof state
            super::signals::update_wallet_balances();

            // Register proofs in event map for fast lookup
            super::proofs::register_proofs_in_event_map(&event_id, &proof_data);

            Ok(recovered_amount)
        }
        Err(e) => {
            log::warn!("Failed to publish recovered change proofs to Nostr: {}. Proofs safe in CDK, will retry on next sync.", e);
            // Return error so caller keeps request in-flight for retry
            Err(format!("Nostr publish failed: {}", e))
        }
    }
}

// =============================================================================
// Batch Melt Quote Recovery (CDK 0.14.2+)
// =============================================================================

/// Check pending melt quotes using CDK's batch API
///
/// Uses `check_pending_melt_quotes()` for efficient batch checking across all mints.
/// Parallel execution across mints for improved performance.
pub async fn check_pending_melt_quotes_batch() -> CashuResult<MeltRecoveryResult> {
    use crate::stores::cashu_cdk_bridge;

    let mints = get_mints();
    let mut result = MeltRecoveryResult::default();

    if mints.is_empty() {
        return Ok(result);
    }

    // Parallel check across mints - WASM compatible
    let futures: Vec<_> = mints
        .iter()
        .map(|mint_url| check_melt_quotes_for_mint(mint_url))
        .collect();

    let mint_results = futures::future::join_all(futures).await;

    for (mint_url, mint_result) in mints.iter().zip(mint_results.into_iter()) {
        match mint_result {
            Ok(mr) => {
                result.quotes_checked += mr.quotes_checked;
                result.quotes_paid += mr.quotes_paid;
                result.change_recovered += mr.change_recovered;
            }
            Err(e) => {
                log::warn!("Failed to check melt quotes for {}: {}", mint_url, e);
                result.errors.push(format!("{}: {}", mint_url, e));
            }
        }
    }

    // Sync wallet state after recovery if change was recovered
    if result.change_recovered > 0 {
        let _ = cashu_cdk_bridge::sync_wallet_state().await;
    }

    log::info!(
        "Batch melt quote check: {} checked, {} paid, {} sats recovered",
        result.quotes_checked,
        result.quotes_paid,
        result.change_recovered
    );

    Ok(result)
}

/// Internal result for per-mint melt quote checking
#[derive(Clone, Debug, Default)]
struct MeltMintResult {
    quotes_checked: usize,
    quotes_paid: usize,
    change_recovered: u64,
}

/// Check melt quotes for a specific mint using CDK's batch API
async fn check_melt_quotes_for_mint(mint_url: &str) -> Result<MeltMintResult, String> {
    use crate::stores::cashu_cdk_bridge;

    let wallet = cashu_cdk_bridge::get_wallet(mint_url)
        .await
        .map_err(|e| format!("Failed to get wallet: {}", e))?;

    // CDK 0.14.2+ batch API - checks all pending melt quotes for this wallet
    wallet
        .check_pending_melt_quotes()
        .await
        .map_err(|e| format!("check_pending_melt_quotes failed: {}", e))?;

    // After CDK processes quotes, check for recovered change
    let recovered = recover_unrecorded_proofs_internal(mint_url).await.unwrap_or(0);

    // NOTE: quotes_checked/quotes_paid are batch operation markers (1 if recovery
    // occurred, 0 otherwise). CDK's check_pending_melt_quotes() returns Result<(), Error>
    // (void) and loops internally per-quote without returning per-quote statistics.
    // We use recovered > 0 as a proxy to indicate the batch operation had results.
    Ok(MeltMintResult {
        quotes_checked: if recovered > 0 { 1 } else { 0 },
        quotes_paid: if recovered > 0 { 1 } else { 0 },
        change_recovered: recovered,
    })
}

/// Internal version that recovers and publishes missing proofs to Nostr
///
/// Finds proofs in CDK that aren't in WALLET_TOKENS and publishes them to Nostr.
/// This ensures change proofs from melt operations are properly synced.
async fn recover_unrecorded_proofs_internal(mint_url: &str) -> Result<u64, String> {
    use crate::stores::cashu_cdk_bridge;
    use super::proofs::cdk_proof_to_proof_data;

    let wallet = cashu_cdk_bridge::get_wallet(mint_url)
        .await
        .map_err(|e| format!("Failed to get wallet: {}", e))?;

    let cdk_proofs = wallet
        .get_unspent_proofs()
        .await
        .map_err(|e| format!("Failed to get proofs: {}", e))?;

    let known_proofs = get_all_proofs_for_mint(mint_url);
    let known_secrets: std::collections::HashSet<String> =
        known_proofs.iter().map(|p| p.secret.clone()).collect();

    let missing: Vec<_> = cdk_proofs
        .iter()
        .filter(|p| !known_secrets.contains(&p.secret.to_string()))
        .collect();

    if missing.is_empty() {
        return Ok(0);
    }

    let recovered_amount: u64 = missing
        .iter()
        .map(|p| u64::from(p.amount))
        .fold(0u64, |acc, amt| acc.saturating_add(amt));

    // Publish missing proofs to Nostr
    let proof_data: Vec<ProofData> = missing.iter()
        .map(|p| cdk_proof_to_proof_data(p))
        .collect();

    match super::events::publish_orphaned_proofs_event(mint_url, &proof_data).await {
        Ok(event_id) => {
            log::info!(
                "Published {} recovered proofs ({} sats) to Nostr: {}",
                missing.len(), recovered_amount, event_id
            );

            let normalized_mint = super::utils::normalize_mint_url(mint_url);
            let new_token = TokenData {
                event_id: event_id.clone(),
                mint: normalized_mint,
                unit: "sat".to_string(),
                proofs: proof_data.clone(),
                created_at: now_secs(),
            };

            if let Err(e) = super::signals::atomic_token_update(|tokens| {
                tokens.push(new_token);
                Ok(())
            }) {
                log::error!("Failed to update tokens atomically: {}", e);
            }

            super::proofs::register_proofs_in_event_map(&event_id, &proof_data);
        }
        Err(e) => {
            log::warn!("Failed to publish recovered proofs: {}", e);
        }
    }

    Ok(recovered_amount)
}

/// Recover operations for a specific mint using CDK's active quote discovery
///
/// Uses `get_active_melt_quotes()` to find pending and non-expired unpaid quotes,
/// then attempts recovery for any that are paid.
async fn recover_operations_for_mint(mint_url: &str) -> Result<u64, String> {
    use crate::stores::cashu_cdk_bridge;
    use cdk::nuts::MeltQuoteState;

    let wallet = cashu_cdk_bridge::get_wallet(mint_url)
        .await
        .map_err(|e| format!("Failed to get wallet: {}", e))?;

    // CDK 0.14.2+ - finds pending + non-expired unpaid quotes
    let active_quotes = wallet
        .get_active_melt_quotes()
        .await
        .map_err(|e| format!("get_active_melt_quotes failed: {}", e))?;

    let mut total_recovered: u64 = 0;

    for quote in active_quotes {
        if matches!(quote.state, MeltQuoteState::Paid) {
            if let Ok(result) = recover_melt_quote_change(mint_url, &quote.id).await {
                if result.recovered_amount > 0 {
                    log::info!(
                        "Recovered {} sats from melt quote {}",
                        result.recovered_amount,
                        quote.id
                    );
                    total_recovered += result.recovered_amount;
                }
            }
        }
    }

    Ok(total_recovered)
}

/// Run enhanced recovery across all mints using CDK's active quote discovery
///
/// Combines `get_active_melt_quotes()` with parallel execution for comprehensive
/// melt quote recovery.
pub async fn recover_active_melt_quotes() -> CashuResult<MeltRecoveryResult> {
    use crate::stores::cashu_cdk_bridge;

    let mints = get_mints();
    let mut result = MeltRecoveryResult::default();

    if mints.is_empty() {
        return Ok(result);
    }

    // Parallel recovery across mints
    let futures: Vec<_> = mints
        .iter()
        .map(|mint_url| recover_operations_for_mint(mint_url))
        .collect();

    let mint_results = futures::future::join_all(futures).await;

    for (mint_url, mint_result) in mints.iter().zip(mint_results.into_iter()) {
        result.quotes_checked += 1;
        match mint_result {
            Ok(recovered) => {
                if recovered > 0 {
                    result.quotes_paid += 1;
                    result.change_recovered += recovered;
                }
            }
            Err(e) => {
                log::warn!("Failed to recover melt quotes for {}: {}", mint_url, e);
                result.errors.push(format!("{}: {}", mint_url, e));
            }
        }
    }

    // Sync wallet state after recovery
    if result.change_recovered > 0 {
        let _ = cashu_cdk_bridge::sync_wallet_state().await;
    }

    log::info!(
        "Active melt quote recovery: {} mints checked, {} paid, {} sats recovered",
        result.quotes_checked,
        result.quotes_paid,
        result.change_recovered
    );

    Ok(result)
}

// =============================================================================
// Quote Expiry Checking
// =============================================================================

/// Check if a quote has expired
pub fn is_quote_expired(expiry: Option<u64>) -> bool {
    if let Some(exp) = expiry {
        let now = js_sys::Date::now() as u64 / 1000;
        now >= exp
    } else {
        false
    }
}

/// Check if a quote is about to expire (with safety margin)
///
/// CDK best practice: Add a safety margin when checking quote expiry before
/// operations to avoid race conditions where the quote expires mid-operation.
/// Default margin is 30 seconds.
pub fn is_quote_about_to_expire(expiry: Option<u64>) -> bool {
    const QUOTE_SAFETY_MARGIN_SECS: u64 = 30;

    if let Some(exp) = expiry {
        let now = js_sys::Date::now() as u64 / 1000;
        now + QUOTE_SAFETY_MARGIN_SECS >= exp
    } else {
        false
    }
}

/// Check quote expiry and return error if expired
pub fn check_quote_not_expired(quote_id: &str, expiry: Option<u64>) -> CashuResult<()> {
    if is_quote_expired(expiry) {
        Err(super::errors::CashuWalletError::QuoteExpired {
            quote_id: quote_id.to_string(),
        })
    } else {
        Ok(())
    }
}

// =============================================================================
// Seed Recovery (NUT-09 / NUT-13)
// =============================================================================

/// Recover proofs from seed using NUT-09 restore protocol
///
/// This function uses CDK's deterministic proof generation to recover proofs
/// that may have been lost (e.g., due to app reinstall, database corruption).
///
/// The seed deterministically generates blinding factors, so we can regenerate
/// the same blinded messages and ask the mint which ones it has signatures for.
///
/// Returns the total amount recovered across all mints.
pub async fn recover_from_seed() -> Result<RecoverySummary, String> {
    use crate::stores::cashu_cdk_bridge;

    if !is_wallet_initialized() {
        return Err("Wallet not initialized".to_string());
    }

    log::info!("Starting seed recovery (NUT-09)...");

    let mints = get_mints();
    if mints.is_empty() {
        return Ok(RecoverySummary {
            total_recovered: 0,
            mints_checked: 0,
            message: "No mints configured".to_string(),
        });
    }

    let mut total_recovered: u64 = 0;
    let mut mints_checked: usize = 0;
    let mut errors = Vec::new();

    for mint_url in &mints {
        // Acquire lock to prevent concurrent operations
        let _lock_guard = match try_acquire_mint_lock(mint_url) {
            Some(guard) => guard,
            None => {
                log::warn!("Skipping {} - operation in progress", mint_url);
                continue;
            }
        };

        mints_checked += 1;

        match recover_from_seed_for_mint(mint_url).await {
            Ok(amount) => {
                if amount > 0 {
                    log::info!("Recovered {} sats from {}", amount, mint_url);
                    total_recovered += amount;
                } else {
                    log::debug!("No proofs to recover from {}", mint_url);
                }
            }
            Err(e) => {
                log::error!("Seed recovery failed for {}: {}", mint_url, e);
                errors.push(format!("{}: {}", mint_url, e));
            }
        }
    }

    // Sync wallet state to update UI
    if total_recovered > 0 {
        if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
            log::warn!("Failed to sync wallet state after recovery: {}", e);
        }

        // Refresh tokens from Nostr to pick up any proofs
        if let Err(e) = fetch_tokens().await {
            log::warn!("Failed to refresh tokens after recovery: {}", e);
        }

        // Inject any NIP-60 proofs into CDK database
        if let Err(e) = inject_nip60_proofs_to_cdk().await {
            log::warn!("Failed to inject proofs to CDK after recovery: {}", e);
        }
    }

    let message = if errors.is_empty() {
        if total_recovered > 0 {
            format!("Recovered {} sats from {} mints", total_recovered, mints_checked)
        } else {
            format!("Checked {} mints - no proofs to recover", mints_checked)
        }
    } else {
        format!(
            "Recovered {} sats from {} mints, {} errors: {}",
            total_recovered,
            mints_checked,
            errors.len(),
            errors.join("; ")
        )
    };

    log::info!("Seed recovery complete: {}", message);

    Ok(RecoverySummary {
        total_recovered,
        mints_checked,
        message,
    })
}

/// Recover proofs from seed for a specific mint using CDK's restore()
async fn recover_from_seed_for_mint(mint_url: &str) -> Result<u64, String> {
    use crate::stores::cashu_cdk_bridge;

    log::info!("Attempting seed recovery for mint: {}", mint_url);

    // Get or create wallet for this mint
    let wallet = cashu_cdk_bridge::get_wallet(mint_url)
        .await
        .map_err(|e| format!("Failed to get wallet: {}", e))?;

    // Use CDK's restore() which implements NUT-09
    let recovered_amount = wallet
        .restore()
        .await
        .map_err(|e| format!("Restore failed: {}", e))?;

    Ok(u64::from(recovered_amount))
}

/// Summary of seed recovery operation
#[derive(Debug, Clone)]
pub struct RecoverySummary {
    /// Total amount recovered in sats
    pub total_recovered: u64,
    /// Number of mints checked
    pub mints_checked: usize,
    /// Human-readable summary message
    pub message: String,
}

// =============================================================================
// Proof Consolidation
// =============================================================================

/// Consolidate proofs to optimize wallet performance
///
/// This swaps multiple small proofs for fewer larger ones, reducing storage
/// overhead and improving send operation efficiency.
///
/// Returns the total amount consolidated across all mints.
///
/// TODO: This is a global consolidation function intended for wallet-wide optimization.
/// A per-mint version exists in mint_mgmt.rs::consolidate_proofs(mint_url) which is
/// currently used. Wire this up to UI or remove if per-mint consolidation is sufficient.
#[allow(dead_code)]
pub async fn consolidate_proofs() -> Result<ConsolidationSummary, String> {
    use crate::stores::cashu_cdk_bridge;

    if !is_wallet_initialized() {
        return Err("Wallet not initialized".to_string());
    }

    log::info!("Starting proof consolidation...");

    // Get the MultiMintWallet
    let multi_wallet = cashu_cdk_bridge::MULTI_WALLET
        .read()
        .as_ref()
        .ok_or("MultiMintWallet not initialized")?
        .clone();

    // Count proofs before consolidation (for reporting)
    let proofs_before = match count_total_proofs(&multi_wallet).await {
        Ok(count) => count,
        Err(e) => {
            log::warn!("Failed to count proofs before consolidation: {}", e);
            0
        }
    };

    // Use CDK's consolidate() method
    let consolidated_amount = multi_wallet
        .consolidate()
        .await
        .map_err(|e| format!("Consolidation failed: {}", e))?;

    let consolidated_sats = u64::from(consolidated_amount);

    // Count proofs after consolidation
    let proofs_after = match count_total_proofs(&multi_wallet).await {
        Ok(count) => count,
        Err(e) => {
            log::warn!("Failed to count proofs after consolidation: {}", e);
            0
        }
    };

    // Sync wallet state to update UI
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!("Failed to sync wallet state after consolidation: {}", e);
    }

    // Refresh tokens from Nostr after consolidation
    if consolidated_sats > 0 {
        if let Err(e) = fetch_tokens().await {
            log::warn!("Failed to refresh tokens after consolidation: {}", e);
        }

        // Inject any NIP-60 proofs into CDK database
        if let Err(e) = inject_nip60_proofs_to_cdk().await {
            log::warn!("Failed to inject proofs to CDK after consolidation: {}", e);
        }
    }

    let proofs_reduced = proofs_before.saturating_sub(proofs_after);
    let message = if consolidated_sats > 0 {
        format!(
            "Consolidated {} sats, reduced proofs by {} ({} -> {})",
            consolidated_sats, proofs_reduced, proofs_before, proofs_after
        )
    } else {
        "No proofs needed consolidation".to_string()
    };

    log::info!("Consolidation complete: {}", message);

    Ok(ConsolidationSummary {
        consolidated_amount: consolidated_sats,
        proofs_before,
        proofs_after,
        message,
    })
}

/// Count total proofs across all wallets
async fn count_total_proofs(
    multi_wallet: &std::sync::Arc<cdk::wallet::multi_mint_wallet::MultiMintWallet>,
) -> Result<usize, String> {
    let wallets = multi_wallet.get_wallets().await;
    let mut total = 0;

    for wallet in wallets {
        let proofs = wallet
            .get_unspent_proofs()
            .await
            .map_err(|e| format!("Failed to get proofs: {}", e))?;
        total += proofs.len();
    }

    Ok(total)
}

/// Summary of proof consolidation operation
#[derive(Debug, Clone)]
pub struct ConsolidationSummary {
    /// Total amount consolidated in sats
    pub consolidated_amount: u64,
    /// Number of proofs before consolidation
    pub proofs_before: usize,
    /// Number of proofs after consolidation
    pub proofs_after: usize,
    /// Human-readable summary message
    pub message: String,
}

// =============================================================================
// Transaction Rollback (CDK pattern)
// =============================================================================

/// Revert a failed outgoing transaction
///
/// This attempts to recover proofs from a failed send/melt operation by:
/// 1. Checking the state of proofs at the mint
/// 2. Reclaiming any proofs that weren't actually spent
///
/// Returns the amount recovered in sats.
pub async fn revert_failed_transaction(
    mint_url: &str,
    proof_secrets: &[String],
) -> Result<u64, String> {
    use crate::stores::cashu_cdk_bridge;

    if proof_secrets.is_empty() {
        return Ok(0);
    }

    log::info!(
        "Attempting to revert transaction with {} proofs for mint {}",
        proof_secrets.len(),
        mint_url
    );

    // Acquire mint lock
    let _lock_guard = try_acquire_mint_lock(mint_url)
        .ok_or_else(|| format!("Operation in progress for mint: {}", mint_url))?;

    // Get proofs from our local state
    let proofs_to_check = get_all_proofs_for_mint(mint_url);
    let matching_proofs: Vec<_> = proofs_to_check
        .into_iter()
        .filter(|p| proof_secrets.contains(&p.secret))
        .collect();

    if matching_proofs.is_empty() {
        log::warn!("No matching proofs found for revert");
        return Ok(0);
    }

    // Convert to CDK proofs
    let cdk_proofs: Vec<cdk::nuts::Proof> = matching_proofs
        .iter()
        .filter_map(|p| proof_data_to_cdk_proof(p).ok())
        .collect();

    if cdk_proofs.is_empty() {
        return Ok(0);
    }

    // Get wallet and check proof states at mint
    let wallet = cashu_cdk_bridge::get_wallet(mint_url)
        .await
        .map_err(|e| format!("Failed to get wallet: {}", e))?;

    let states = wallet
        .check_proofs_spent(cdk_proofs.clone())
        .await
        .map_err(|e| format!("Failed to check proof states: {}", e))?;

    // Find proofs that are still unspent
    let mut recovered_amount: u64 = 0;
    let mut recovered_secrets = Vec::new();

    for (state, proof) in states.iter().zip(cdk_proofs.iter()) {
        if matches!(state.state, State::Unspent) {
            recovered_amount += u64::from(proof.amount);
            recovered_secrets.push(proof.secret.to_string());
        }
    }

    if recovered_secrets.is_empty() {
        log::info!("No proofs to recover - all were spent");
        return Ok(0);
    }

    // Revert local state for recovered proofs
    revert_proofs_to_spendable(&recovered_secrets);

    log::info!(
        "Reverted {} proofs worth {} sats",
        recovered_secrets.len(),
        recovered_amount
    );

    // Sync wallet state
    let _ = cashu_cdk_bridge::sync_wallet_state().await;

    Ok(recovered_amount)
}

/// Revert a pending transaction by its proofs
///
/// Use this when a send operation failed after marking proofs as pending.
pub async fn revert_pending_proofs(mint_url: &str) -> Result<u64, String> {
    // Get all pending transactions for this mint
    let pending = get_pending_transactions();

    let mint_pending: Vec<_> = pending
        .into_iter()
        .filter(|tx| tx.mint_url == mint_url)
        .collect();

    if mint_pending.is_empty() {
        return Ok(0);
    }

    // Collect all proof secrets from pending transactions
    let secrets: Vec<String> = mint_pending
        .iter()
        .flat_map(|tx| tx.proof_secrets.clone())
        .collect();

    if secrets.is_empty() {
        return Ok(0);
    }

    revert_failed_transaction(mint_url, &secrets).await
}

// =============================================================================
// Quote State Machine
// =============================================================================

use super::signals::{PENDING_MELT_QUOTES, PENDING_MINT_QUOTES};
use super::types::{MeltQuoteInfo, MintQuoteInfo};

/// Quote state for state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteState {
    /// Quote created, waiting for payment
    Pending,
    /// Payment received/sent, quote paid
    Paid,
    /// Quote has expired
    Expired,
    /// Quote failed
    Failed,
    /// Tokens minted/melted successfully
    Completed,
}

/// Check and process all pending mint quotes
///
/// Polls the mint for quote status updates and processes any paid quotes.
/// Returns tuple of (quotes_checked, quotes_paid, amount_minted).
pub async fn process_pending_mint_quotes() -> Result<(usize, usize, u64), String> {
    use crate::stores::cashu_cdk_bridge;

    let store = PENDING_MINT_QUOTES();
    let quotes: Vec<MintQuoteInfo> = store.data().read().clone();

    if quotes.is_empty() {
        return Ok((0, 0, 0));
    }

    log::info!("Processing {} pending mint quotes", quotes.len());

    let checked = quotes.len();
    let now = js_sys::Date::now() as u64 / 1000;

    // First, remove expired quotes
    let mut expired_ids = Vec::new();
    for quote in &quotes {
        if let Some(expiry) = quote.expiry {
            if now >= expiry {
                log::info!("Quote {} expired, removing", quote.quote_id);
                expired_ids.push(quote.quote_id.clone());
            }
        }
    }
    for id in &expired_ids {
        remove_expired_mint_quote(id);
    }

    // Call check_all_mint_quotes to process any paid quotes
    // Then check each quote's state individually to correctly track which were paid
    // NOTE: CDK's check_all_mint_quotes() actually MINTS tokens for paid quotes, not just checks status
    let (paid, total_minted) = if let Some(multi_wallet) = cashu_cdk_bridge::MULTI_WALLET.read().as_ref() {
        // First, let CDK process all paid quotes and mint them
        // Track whether batch mint succeeded to handle Paid quotes correctly
        let (minted, batch_mint_succeeded) = match multi_wallet.check_all_mint_quotes(None).await {
            Ok(amount) => {
                let amt = u64::from(amount);
                if amt > 0 {
                    log::info!("Recovered {} sats from paid mint quotes", amt);
                }
                (amt, true)
            }
            Err(e) => {
                log::warn!("Batch mint quote check failed: {}", e);
                (0, false)
            }
        };

        // Now check each quote's state individually to update our tracking
        // This ensures we only remove quotes that are actually paid
        let mut paid_count = 0;
        let non_expired_quotes: Vec<_> = quotes.iter()
            .filter(|q| !expired_ids.contains(&q.quote_id))
            .cloned()
            .collect();

        for quote in non_expired_quotes {
            // Try to get the quote state from CDK - need mint_url for check_mint_quote
            let mint_url: cdk::mint_url::MintUrl = match quote.mint_url.parse() {
                Ok(url) => url,
                Err(e) => {
                    log::warn!("Invalid mint URL for quote {}: {}", quote.quote_id, e);
                    continue;
                }
            };

            match multi_wallet.check_mint_quote(&mint_url, &quote.quote_id).await {
                Ok(cdk_quote) => {
                    // CDK MintQuoteState has only: Unpaid, Paid, Issued (no Unknown)
                    use cdk::nuts::MintQuoteState;
                    match cdk_quote.state {
                        MintQuoteState::Issued => {
                            // Tokens already issued - always safe to remove
                            log::debug!("Quote {} is Issued, removing from pending", quote.quote_id);
                            remove_paid_mint_quote(&quote.quote_id);
                            paid_count += 1;
                        }
                        MintQuoteState::Paid => {
                            // Only remove Paid quotes if batch mint succeeded (tokens were actually minted)
                            if batch_mint_succeeded {
                                log::debug!("Quote {} is Paid and batch mint succeeded, removing from pending", quote.quote_id);
                                remove_paid_mint_quote(&quote.quote_id);
                                paid_count += 1;
                            } else {
                                log::warn!("Quote {} paid but batch mint failed, keeping pending", quote.quote_id);
                            }
                        }
                        MintQuoteState::Unpaid => {
                            // Quote still unpaid - keep tracking it
                            log::debug!("Quote {} still unpaid", quote.quote_id);
                        }
                    }
                }
                Err(e) => {
                    // Couldn't check quote state - don't remove it
                    log::warn!("Failed to check quote {} state: {}", quote.quote_id, e);
                }
            }
        }

        (paid_count, minted)
    } else {
        (0, 0)
    };

    if paid > 0 {
        log::info!("Minted {} sats from {} paid quotes", total_minted, paid);
        // Sync wallet state
        let _ = cashu_cdk_bridge::sync_wallet_state().await;
    }

    Ok((checked, paid, total_minted))
}

/// Check and process all pending melt quotes
///
/// Returns tuple of (quotes_checked, quotes_completed, quotes_expired).
pub async fn process_pending_melt_quotes() -> Result<(usize, usize, usize), String> {
    let store = PENDING_MELT_QUOTES();
    let quotes: Vec<MeltQuoteInfo> = store.data().read().clone();

    if quotes.is_empty() {
        return Ok((0, 0, 0));
    }

    log::info!("Processing {} pending melt quotes", quotes.len());

    let mut checked = 0;
    let mut expired = 0;

    let now = js_sys::Date::now() as u64 / 1000;

    for quote in quotes {
        checked += 1;

        // Check expiry
        if let Some(expiry) = quote.expiry {
            if now >= expiry {
                log::info!("Melt quote {} expired, removing", quote.quote_id);
                remove_expired_melt_quote(&quote.quote_id);
                expired += 1;
            }
        }
    }

    Ok((checked, 0, expired))
}

/// Remove an expired mint quote from pending
fn remove_expired_mint_quote(quote_id: &str) {
    use dioxus::prelude::WritableExt;
    let store = PENDING_MINT_QUOTES();
    let mut binding = store.data();
    let mut data = binding.write();
    data.retain(|q: &MintQuoteInfo| q.quote_id != quote_id);
}

/// Remove a paid mint quote from pending
fn remove_paid_mint_quote(quote_id: &str) {
    use dioxus::prelude::WritableExt;
    let store = PENDING_MINT_QUOTES();
    let mut binding = store.data();
    let mut data = binding.write();
    data.retain(|q: &MintQuoteInfo| q.quote_id != quote_id);
}

/// Remove an expired melt quote from pending
fn remove_expired_melt_quote(quote_id: &str) {
    use dioxus::prelude::WritableExt;
    let store = PENDING_MELT_QUOTES();
    let mut binding = store.data();
    let mut data = binding.write();
    data.retain(|q: &MeltQuoteInfo| q.quote_id != quote_id);
}

// =============================================================================
// Batch State Checking (CDK pattern)
// =============================================================================

/// Check all pending proofs across all mints
///
/// Follows CDK's check_all_pending_proofs pattern for batch state verification.
/// Returns tuple of (proofs_checked, spent_count, pending_count).
pub async fn check_all_pending_proofs() -> Result<(usize, usize, usize), String> {
    log::info!("Checking all pending proofs...");

    let mints = get_mints();
    let mut total_checked = 0;
    let mut total_spent = 0;
    let mut total_pending = 0;

    for mint_url in mints {
        match sync_state_with_mint(&mint_url).await {
            Ok(result) => {
                total_checked += result.proofs_cleaned;
                total_spent += result.spent_found;
                total_pending += result.pending_found;
            }
            Err(e) => {
                log::warn!("Failed to check proofs for {}: {}", mint_url, e);
            }
        }
    }

    log::info!(
        "Checked {} proofs: {} spent, {} pending",
        total_checked, total_spent, total_pending
    );

    Ok((total_checked, total_spent, total_pending))
}

/// Check all pending mint quotes across all mints
///
/// Follows CDK's check_all_mint_quotes pattern.
/// Returns tuple of (quotes_checked, quotes_paid, amount_minted).
pub async fn check_all_mint_quotes() -> Result<(usize, usize, u64), String> {
    process_pending_mint_quotes().await
}

/// Check all pending melt quotes across all mints
///
/// Returns tuple of (quotes_checked, completed, expired).
pub async fn check_all_melt_quotes() -> Result<(usize, usize, usize), String> {
    process_pending_melt_quotes().await
}

/// Run a full wallet health check
///
/// Checks all pending proofs and quotes, recovering any stuck funds.
/// Uses CDK 0.14.2+ batch APIs for melt quote recovery.
/// Returns a summary of what was found and fixed.
pub async fn run_wallet_health_check() -> Result<WalletHealthReport, String> {
    use crate::stores::cashu_cdk_bridge;

    log::info!("Running wallet health check...");

    // Check pending proofs
    let (proofs_checked, spent_found, pending_found) = check_all_pending_proofs().await?;

    // Check mint quotes
    let (mint_quotes_checked, mint_quotes_paid, amount_minted) = check_all_mint_quotes().await?;

    // Check melt quotes (legacy tracking)
    let (melt_quotes_checked, melt_completed, melt_expired) = check_all_melt_quotes().await?;

    // CDK 0.14.2+ batch melt quote recovery
    let melt_recovery = check_pending_melt_quotes_batch()
        .await
        .unwrap_or_default();

    // Sync wallet state
    let _ = cashu_cdk_bridge::sync_wallet_state().await;

    let report = WalletHealthReport {
        proofs_checked,
        spent_proofs_found: spent_found,
        pending_proofs_found: pending_found,
        mint_quotes_checked,
        mint_quotes_paid,
        amount_minted_sats: amount_minted,
        melt_quotes_checked,
        melt_quotes_completed: melt_completed,
        melt_quotes_expired: melt_expired,
        change_recovered_sats: melt_recovery.change_recovered,
    };

    log::info!("Health check complete: {:?}", report);

    Ok(report)
}

/// Wallet health check report
#[derive(Debug, Clone)]
pub struct WalletHealthReport {
    /// Number of proofs checked
    pub proofs_checked: usize,
    /// Number of spent proofs found and cleaned
    pub spent_proofs_found: usize,
    /// Number of pending proofs found
    pub pending_proofs_found: usize,
    /// Number of mint quotes checked
    pub mint_quotes_checked: usize,
    /// Number of mint quotes that were paid
    pub mint_quotes_paid: usize,
    /// Amount minted from paid quotes (sats)
    pub amount_minted_sats: u64,
    /// Number of melt quotes checked
    pub melt_quotes_checked: usize,
    /// Number of melt quotes completed
    pub melt_quotes_completed: usize,
    /// Number of melt quotes expired
    pub melt_quotes_expired: usize,
    /// Amount of change recovered from melt quotes (sats) - CDK 0.14.2+
    pub change_recovered_sats: u64,
}

impl WalletHealthReport {
    /// Check if any issues were found
    pub fn has_issues(&self) -> bool {
        self.spent_proofs_found > 0
            || self.mint_quotes_paid > 0
            || self.melt_quotes_expired > 0
            || self.change_recovered_sats > 0
    }

    /// Get a human-readable summary
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if self.spent_proofs_found > 0 {
            parts.push(format!("{} spent proofs cleaned", self.spent_proofs_found));
        }
        if self.mint_quotes_paid > 0 {
            parts.push(format!(
                "{} quotes processed ({} sats minted)",
                self.mint_quotes_paid, self.amount_minted_sats
            ));
        }
        if self.melt_quotes_expired > 0 {
            parts.push(format!("{} expired quotes removed", self.melt_quotes_expired));
        }
        if self.change_recovered_sats > 0 {
            parts.push(format!("{} sats change recovered", self.change_recovered_sats));
        }

        if parts.is_empty() {
            "Wallet is healthy - no issues found".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Start background quote processor
///
/// Spawns a task that periodically checks pending quotes.
pub fn start_quote_processor() {
    use dioxus::prelude::spawn;

    spawn(async move {
        log::info!("Starting quote state machine processor");

        loop {
            // Wait 30 seconds between checks
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::TimeoutFuture::new(30_000).await;

            #[cfg(not(target_arch = "wasm32"))]
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;

            // Process mint quotes
            match process_pending_mint_quotes().await {
                Ok((_checked, paid, amount)) => {
                    if paid > 0 {
                        log::info!("Quote processor: minted {} sats from {} quotes", amount, paid);
                    }
                }
                Err(e) => {
                    log::warn!("Quote processor error (mint): {}", e);
                }
            }

            // Process melt quotes
            match process_pending_melt_quotes().await {
                Ok((_, _, expired)) => {
                    if expired > 0 {
                        log::debug!("Quote processor: cleaned up {} expired melt quotes", expired);
                    }
                }
                Err(e) => {
                    log::warn!("Quote processor error (melt): {}", e);
                }
            }
        }
    });
}
