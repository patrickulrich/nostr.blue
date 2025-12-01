//! Recovery and sync operations
//!
//! Functions for syncing proofs with mints, cleaning up spent proofs,
//! recovering from failures, and handling pending operations.
//!
//! Implements patterns from Minibits and Harbor for robust recovery.

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
use super::types::*;

use super::events::fetch_tokens;
use super::history::fetch_history;
use super::init::is_wallet_initialized;
use super::mint_mgmt::get_mints;
use super::signals::try_acquire_mint_lock;

/// Refresh wallet data by re-fetching tokens and history from Nostr
pub async fn refresh_wallet() -> Result<(), String> {
    if !is_wallet_initialized() {
        return Err("Wallet not initialized".to_string());
    }

    log::info!("Refreshing wallet data");

    fetch_tokens().await?;
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
// NUT-07 State Sync with Batch Pagination (Minibits pattern)
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

        // Process each proof state
        for (proof, state) in batch.iter().zip(states.iter()) {
            match state.state {
                State::Spent => {
                    // Proof is spent - mark locally and clean up
                    move_proofs_to_spent(&[proof.secret.clone()]);
                    result.spent_found += 1;
                    result.sats_cleaned += proof.amount;

                    // Remove from pending-at-mint if it was there
                    if is_proof_pending_at_mint(&proof.secret) {
                        remove_from_pending_at_mint(&[proof.secret.clone()]);
                    }

                    log::debug!("Proof {} marked as spent ({} sats)", &proof.secret[..8], proof.amount);
                }
                State::Pending => {
                    // Proof is pending at mint (lightning in-flight)
                    result.pending_found += 1;
                    if !is_proof_pending_at_mint(&proof.secret) {
                        register_proofs_pending_at_mint(&[proof.secret.clone()]);
                        log::debug!("Proof {} registered as pending at mint", &proof.secret[..8]);
                    }
                }
                State::Unspent => {
                    // Proof is unspent at mint
                    if is_proof_pending_at_mint(&proof.secret) {
                        // Was pending but now unspent = payment failed, revert
                        remove_from_pending_at_mint(&[proof.secret.clone()]);
                        revert_proofs_to_spendable(&[proof.secret.clone()]);
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
                        revert_proofs_to_spendable(&[proof.secret.clone()]);
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
                        register_proofs_pending_at_mint(&[proof.secret.clone()]);
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
pub async fn sync_state_with_all_mints() -> CashuResult<SyncResult> {
    let mints = get_mints();
    let mut total_result = SyncResult::default();

    for mint_url in mints {
        match sync_state_with_mint(&mint_url).await {
            Ok(result) => {
                total_result.spent_found += result.spent_found;
                total_result.proofs_cleaned += result.proofs_cleaned;
                total_result.sats_cleaned += result.sats_cleaned;
            }
            Err(e) => {
                log::warn!("Failed to sync with mint {}: {}", mint_url, e);
                // Continue with other mints
            }
        }
    }

    Ok(total_result)
}

// =============================================================================
// Pending Operation Recovery (Harbor pattern)
// =============================================================================

/// Recover pending operations on startup
///
/// Implements Harbor's pattern of recovering all pending operations when the
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
// Quote Recovery (Minibits pattern)
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
                .map_err(|e| super::errors::CashuWalletError::Cdk(e))?;

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
        .map_err(|e| super::errors::CashuWalletError::Cdk(e))?;

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
// Quote Expiry Checking (Harbor pattern)
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

    // Call check_all_mint_quotes once (not in a loop)
    // CDK's check_all_mint_quotes processes all pending quotes and mints any paid ones
    let (paid, total_minted) = if let Some(multi_wallet) = cashu_cdk_bridge::MULTI_WALLET.read().as_ref() {
        match multi_wallet.check_all_mint_quotes(None).await {
            Ok(amount) => {
                let minted = u64::from(amount);
                if minted > 0 {
                    // CDK processed paid quotes - clear our pending tracking
                    // We don't know exactly which quotes were paid, so clear all non-expired
                    let non_expired_ids: Vec<_> = quotes.iter()
                        .filter(|q| !expired_ids.contains(&q.quote_id))
                        .map(|q| q.quote_id.clone())
                        .collect();
                    let paid_count = non_expired_ids.len();
                    for id in non_expired_ids {
                        remove_paid_mint_quote(&id);
                    }
                    (paid_count, minted)
                } else {
                    (0, 0)
                }
            }
            Err(e) => {
                log::warn!("Failed to check mint quotes: {}", e);
                (0, 0)
            }
        }
    } else {
        (0, 0)
    };

    if paid > 0 {
        log::info!("Minted {} sats from {} paid quotes", total_minted, paid);
        // Sync wallet state
        let _ = cashu_cdk_bridge::sync_wallet_state().await;
    }

    Ok((checked, paid, total_minted as u64))
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
/// Returns a summary of what was found and fixed.
pub async fn run_wallet_health_check() -> Result<WalletHealthReport, String> {
    use crate::stores::cashu_cdk_bridge;

    log::info!("Running wallet health check...");

    // Check pending proofs
    let (proofs_checked, spent_found, pending_found) = check_all_pending_proofs().await?;

    // Check mint quotes
    let (mint_quotes_checked, mint_quotes_paid, amount_minted) = check_all_mint_quotes().await?;

    // Check melt quotes
    let (melt_quotes_checked, melt_completed, melt_expired) = check_all_melt_quotes().await?;

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
}

impl WalletHealthReport {
    /// Check if any issues were found
    pub fn has_issues(&self) -> bool {
        self.spent_proofs_found > 0
            || self.mint_quotes_paid > 0
            || self.melt_quotes_expired > 0
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
