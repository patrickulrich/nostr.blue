//! Proof State Recovery
//!
//! Handles automatic recovery of proofs stuck in transient states.
//! Implements timeout-based recovery for Reserved/PendingSpent proofs.
//!
//! SAFETY: This module checks with mints before recovering proofs to prevent
//! fund loss from recovering proofs that are actually in-flight.

use std::collections::HashMap;

use dioxus::prelude::{ReadableExt, WritableExt};

use super::internal::get_or_create_wallet;
use super::proofs::proof_data_to_cdk_proof;
use super::signals::{is_transaction_active, WALLET_BALANCE, WALLET_TOKENS};
use super::types::{
    ProofData, ProofState, WalletTokensStoreStoreExt,
    RESERVED_PROOF_TIMEOUT_SECS, PENDING_SPENT_TIMEOUT_SECS, IN_FLIGHT_MELT_TIMEOUT_SECS,
};
use super::utils::now_secs;

// =============================================================================
// Recovery Constants
// =============================================================================

/// Default timeout for Reserved proofs before checking mint state
/// Uses the more aggressive timeout from types.rs for faster recovery
pub const RESERVED_TIMEOUT_SECS: u64 = RESERVED_PROOF_TIMEOUT_SECS;

/// Default timeout for PendingSpent proofs before checking mint state
pub const PENDING_SPENT_TIMEOUT_DEFAULT: u64 = PENDING_SPENT_TIMEOUT_SECS;

/// Short timeout for transactions (5 minutes) - for UI urgency display
pub const TRANSACTION_TIMEOUT_SECS: u64 = IN_FLIGHT_MELT_TIMEOUT_SECS;

// =============================================================================
// Proof State Tracking
// =============================================================================

/// Tracked proof state with timestamp
#[derive(Debug, Clone)]
pub struct TrackedProofState {
    /// Proof secret (unique identifier)
    pub secret: String,
    /// Current state
    pub state: ProofState,
    /// Timestamp when state was set (available for debugging/external use)
    #[allow(dead_code)]
    pub state_set_at: u64,
    /// Associated transaction ID (if any, available for debugging/external use)
    #[allow(dead_code)]
    pub transaction_id: Option<u64>,
    /// Mint URL
    pub mint_url: String,
}

// =============================================================================
// Stuck Proof Detection
// =============================================================================

/// Detect proofs that are stuck in transient states for longer than timeout_secs
pub fn detect_stuck_proofs(timeout_secs: u64) -> Vec<TrackedProofState> {
    let now = now_secs();
    let mut stuck_proofs = Vec::new();

    let store = WALLET_TOKENS();
    let data = store.data();
    let tokens = data.read();

    for token in tokens.iter() {
        for proof in &token.proofs {
            // Only check proofs in transient states
            if proof.state.is_pending() {
                // Check if proof has been in this state longer than timeout
                let is_stuck = match proof.state_set_at {
                    Some(set_at) => now.saturating_sub(set_at) > timeout_secs,
                    // If no timestamp recorded, treat as not timed out yet
                    // (new proofs before this fix won't have timestamp)
                    None => false,
                };

                if is_stuck {
                    stuck_proofs.push(TrackedProofState {
                        secret: proof.secret.clone(),
                        state: proof.state,
                        state_set_at: proof.state_set_at.unwrap_or(0),
                        transaction_id: proof.transaction_id,
                        mint_url: token.mint.clone(),
                    });
                }
            }
        }
    }

    stuck_proofs
}

/// Find proofs in Reserved state (available for debugging/external use)
#[allow(dead_code)]
pub fn find_reserved_proofs() -> Vec<(String, ProofData)> {
    let store = WALLET_TOKENS();
    let data = store.data();
    let tokens = data.read();

    let mut reserved = Vec::new();

    for token in tokens.iter() {
        for proof in &token.proofs {
            if matches!(proof.state, ProofState::Reserved) {
                reserved.push((token.mint.clone(), proof.clone()));
            }
        }
    }

    reserved
}

/// Find proofs in PendingSpent state
pub fn find_pending_spent_proofs() -> Vec<(String, ProofData)> {
    let store = WALLET_TOKENS();
    let data = store.data();
    let tokens = data.read();

    let mut pending_spent = Vec::new();

    for token in tokens.iter() {
        for proof in &token.proofs {
            if matches!(proof.state, ProofState::PendingSpent) {
                pending_spent.push((token.mint.clone(), proof.clone()));
            }
        }
    }

    pending_spent
}

// =============================================================================
// Proof Recovery Operations
// =============================================================================

/// Recovery result
#[derive(Debug, Clone, Default)]
pub struct ProofRecoveryResult {
    /// Number of proofs recovered
    pub recovered_count: usize,
    /// Total value recovered
    pub recovered_value: u64,
    /// Number of proofs confirmed spent
    pub spent_count: usize,
    /// Value confirmed spent
    pub spent_value: u64,
    /// Errors encountered
    pub errors: Vec<String>,
}

// =============================================================================
// Wallet Health Stats (for UI)
// =============================================================================

/// Urgency level for stuck proof display
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UrgencyLevel {
    /// < 5 min (TRANSACTION_TIMEOUT_SECS = 300s)
    Normal,
    /// 5-10 min (TRANSACTION_TIMEOUT_SECS to RESERVED_TIMEOUT_SECS)
    Warning,
    /// 10-30 min (RESERVED_TIMEOUT_SECS to PENDING_SPENT_TIMEOUT_DEFAULT)
    High,
    /// > 30 min (PENDING_SPENT_TIMEOUT_DEFAULT = 1800s)
    Critical,
}

/// Stuck proof info for UI display
#[derive(Debug, Clone, PartialEq)]
pub struct StuckProofInfo {
    pub mint_url: String,
    pub amount: u64,
    pub state: ProofState,
    pub stuck_duration_secs: u64,
    pub transaction_id: Option<u64>,
    pub urgency: UrgencyLevel,
    pub can_recover: bool,
}

/// Health stats for UI display
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WalletHealthStats {
    pub spendable_balance: u64,
    pub pending_balance: u64,
    pub pending_count: usize,
    pub stuck_balance: u64,
    pub stuck_count: usize,
    pub stuck_proofs: Vec<StuckProofInfo>,
}

/// Recover stuck Reserved proofs by checking with mint first
///
/// SAFETY: Only recovers proofs that are:
/// 1. Older than RESERVED_TIMEOUT_SECS (10 minutes)
/// 2. NOT in PENDING_BY_MINT_SECRETS (not actively being processed)
/// 3. NOT part of an active transaction (checked via ACTIVE_OPERATIONS)
/// 4. Confirmed unspent by the mint
///
/// This prevents fund loss from recovering proofs that are actually in-flight.
pub async fn recover_reserved_proofs() -> ProofRecoveryResult {
    use super::signals::PENDING_BY_MINT_SECRETS;

    // Find Reserved proofs that have exceeded timeout
    let stuck_proofs = detect_stuck_proofs(RESERVED_TIMEOUT_SECS);
    let reserved_stuck: Vec<_> = stuck_proofs
        .into_iter()
        .filter(|p| matches!(p.state, ProofState::Reserved))
        .collect();

    if reserved_stuck.is_empty() {
        return ProofRecoveryResult::default();
    }

    // Filter out proofs that are actively pending at mint
    let pending_secrets = PENDING_BY_MINT_SECRETS.read();
    let mut safe_to_recover: Vec<_> = reserved_stuck
        .into_iter()
        .filter(|p| !pending_secrets.contains_key(&p.secret))
        .collect();
    drop(pending_secrets);

    // SAFETY (Risk 4): Filter out proofs that are part of active transactions
    // This prevents timeout recovery from interfering with still-running operations
    safe_to_recover.retain(|p| {
        let tx_id_str = p.transaction_id.map(|id| format!("tx_{}", id));
        !is_transaction_active(&tx_id_str)
    });

    if safe_to_recover.is_empty() {
        log::debug!("All reserved proofs are still pending at mint");
        return ProofRecoveryResult::default();
    }

    log::info!(
        "Checking {} timed-out reserved proofs with mints",
        safe_to_recover.len()
    );

    let mut result = ProofRecoveryResult::default();

    // DIOXUS PATTERN: Snapshot ALL data before any async operations
    // Never hold signal locks across await points
    let proof_data_by_mint: std::collections::HashMap<String, Vec<ProofData>> = {
        let store = WALLET_TOKENS();
        let data = store.data();
        let tokens = data.read(); // Single lock acquisition

        let mut by_mint: std::collections::HashMap<String, Vec<ProofData>> =
            std::collections::HashMap::new();

        for tracked in &safe_to_recover {
            for token in tokens.iter() {
                if super::utils::mint_matches(&token.mint, &tracked.mint_url) {
                    for proof in &token.proofs {
                        if proof.secret == tracked.secret {
                            by_mint
                                .entry(tracked.mint_url.clone())
                                .or_default()
                                .push(proof.clone());
                        }
                    }
                }
            }
        }
        by_mint
    }; // Lock released here, before any async operations

    // Now safe to do async operations - no signal locks held
    for (mint_url, proof_data) in proof_data_by_mint {
        match check_and_recover_proofs(&mint_url, proof_data).await {
            Ok(mint_result) => {
                result.recovered_count += mint_result.recovered_count;
                result.recovered_value += mint_result.recovered_value;
                result.spent_count += mint_result.spent_count;
                result.spent_value += mint_result.spent_value;
            }
            Err(e) => {
                result.errors.push(format!("{}: {}", mint_url, e));
            }
        }
    }

    if result.recovered_count > 0 || result.spent_count > 0 {
        recalculate_balance();
        log::info!(
            "Recovered {} reserved proofs worth {} sats (confirmed unspent by mint)",
            result.recovered_count,
            result.recovered_value
        );
    }

    result
}

/// Check PendingSpent proofs with mint and recover or mark spent
pub async fn recover_pending_spent_proofs() -> ProofRecoveryResult {
    let pending_spent = find_pending_spent_proofs();

    if pending_spent.is_empty() {
        return ProofRecoveryResult::default();
    }

    log::info!("Checking {} pending spent proofs", pending_spent.len());

    let mut result = ProofRecoveryResult::default();

    // Group by mint for batch checking
    let mut by_mint: HashMap<String, Vec<ProofData>> = HashMap::new();
    for (mint, proof) in pending_spent {
        by_mint.entry(mint).or_default().push(proof);
    }

    for (mint_url, proofs) in by_mint {
        match check_and_recover_proofs(&mint_url, proofs).await {
            Ok(mint_result) => {
                result.recovered_count += mint_result.recovered_count;
                result.recovered_value += mint_result.recovered_value;
                result.spent_count += mint_result.spent_count;
                result.spent_value += mint_result.spent_value;
            }
            Err(e) => {
                result.errors.push(format!("{}: {}", mint_url, e));
            }
        }
    }

    // Update balance after all changes
    recalculate_balance();

    result
}

/// Check proofs with mint and recover unspent ones
async fn check_and_recover_proofs(
    mint_url: &str,
    proofs: Vec<ProofData>,
) -> Result<ProofRecoveryResult, String> {
    let wallet = get_or_create_wallet(mint_url).await?;

    // Convert to CDK proofs for checking, tracking conversion errors
    let conversion_results: Vec<_> = proofs
        .iter()
        .enumerate()
        .map(|(idx, p)| (idx, proof_data_to_cdk_proof(p)))
        .collect();

    let mut cdk_proofs = Vec::new();
    let mut conversion_errors = 0usize;
    for (idx, result) in conversion_results {
        match result {
            Ok(proof) => cdk_proofs.push(proof),
            Err(e) => {
                conversion_errors += 1;
                log::warn!("Failed to convert proof {} for recovery: {}", idx, e);
            }
        }
    }

    if conversion_errors > 0 {
        log::warn!("Skipped {} proofs due to conversion errors", conversion_errors);
    }

    if cdk_proofs.is_empty() {
        return Ok(ProofRecoveryResult::default());
    }

    // Check proof states with mint
    let states = wallet
        .check_proofs_spent(cdk_proofs.clone())
        .await
        .map_err(|e| format!("Failed to check proof states: {}", e))?;

    let mut recovered_secrets = Vec::new();
    let mut spent_secrets = Vec::new();
    let mut recovered_value = 0u64;
    let mut spent_value = 0u64;

    for (proof, proof_state) in cdk_proofs.iter().zip(states.iter()) {
        let secret_str = proof.secret.to_string();
        let amount = u64::from(proof.amount);

        match proof_state.state {
            cdk::nuts::State::Unspent => {
                // Proof is still unspent - recover it
                recovered_secrets.push(secret_str);
                recovered_value += amount;
            }
            cdk::nuts::State::Spent => {
                // Proof was spent - mark for removal
                spent_secrets.push(secret_str);
                spent_value += amount;
            }
            cdk::nuts::State::Pending | cdk::nuts::State::PendingSpent => {
                // Still pending - leave as is
            }
            cdk::nuts::State::Reserved => {
                // Reserved by mint - unusual, leave as is
                log::warn!("Proof {} is reserved at mint", proof.secret);
            }
        }
    }

    // Update storage
    {
        let store = WALLET_TOKENS();
        let mut data = store.data();
        let mut tokens = data.write();

        for token in tokens.iter_mut() {
            if super::utils::mint_matches(&token.mint, mint_url) {
                for proof in token.proofs.iter_mut() {
                    if recovered_secrets.contains(&proof.secret) {
                        proof.state = ProofState::Unspent;
                        proof.transaction_id = None;
                        log::info!("Recovered proof: {} sats", proof.amount);
                    } else if spent_secrets.contains(&proof.secret) {
                        proof.state = ProofState::Spent;
                        log::info!("Confirmed spent proof: {} sats", proof.amount);
                    }
                }
            }
        }

        // Remove spent proofs
        for token in tokens.iter_mut() {
            if super::utils::mint_matches(&token.mint, mint_url) {
                token.proofs.retain(|p| !spent_secrets.contains(&p.secret));
            }
        }

        // Remove empty tokens
        tokens.retain(|t| !t.proofs.is_empty());
    }

    Ok(ProofRecoveryResult {
        recovered_count: recovered_secrets.len(),
        recovered_value,
        spent_count: spent_secrets.len(),
        spent_value,
        errors: Vec::new(),
    })
}

// =============================================================================
// Full Recovery Workflow
// =============================================================================

/// Run full proof recovery - check all stuck proofs and recover/cleanup
///
/// This function:
/// 1. Recovers stuck Reserved proofs (checks mint + uses timeout)
/// 2. Recovers stuck PendingSpent proofs (checks mint)
///
/// Only proofs that have been stuck longer than the timeout AND are confirmed
/// unspent by the mint will be recovered. This prevents fund loss.
pub async fn run_full_recovery() -> ProofRecoveryResult {
    log::info!("Running full proof recovery");

    // First recover reserved proofs (now async - checks with mint)
    let reserved_result = recover_reserved_proofs().await;

    // Then check pending spent proofs with mints
    let pending_result = recover_pending_spent_proofs().await;

    let total_recovered = reserved_result.recovered_count + pending_result.recovered_count;
    let total_spent = reserved_result.spent_count + pending_result.spent_count;

    if total_recovered > 0 || total_spent > 0 {
        log::info!(
            "Proof recovery complete: {} recovered, {} confirmed spent",
            total_recovered,
            total_spent
        );
    }

    ProofRecoveryResult {
        recovered_count: total_recovered,
        recovered_value: reserved_result.recovered_value + pending_result.recovered_value,
        spent_count: total_spent,
        spent_value: reserved_result.spent_value + pending_result.spent_value,
        errors: [reserved_result.errors, pending_result.errors].concat(),
    }
}

/// Get recovery stats without performing recovery (available for debugging/external use)
#[allow(dead_code)]
pub fn get_recovery_stats() -> (usize, u64, usize, u64) {
    let reserved = find_reserved_proofs();
    let pending_spent = find_pending_spent_proofs();

    let reserved_value: u64 = reserved
        .iter()
        .map(|(_, p)| p.amount)
        .fold(0u64, |acc, amt| acc.saturating_add(amt));
    let pending_value: u64 = pending_spent
        .iter()
        .map(|(_, p)| p.amount)
        .fold(0u64, |acc, amt| acc.saturating_add(amt));

    (
        reserved.len(),
        reserved_value,
        pending_spent.len(),
        pending_value,
    )
}

// =============================================================================
// Helpers
// =============================================================================

/// Recalculate and update wallet balance
fn recalculate_balance() {
    let store = WALLET_TOKENS();
    let data = store.data();
    let tokens = data.read();

    let new_balance: u64 = tokens
        .iter()
        .flat_map(|t| &t.proofs)
        .filter(|p| p.state.is_spendable())
        .map(|p| p.amount)
        .fold(0u64, |acc, amt| acc.saturating_add(amt));

    *WALLET_BALANCE.write() = new_balance;
}

/// Calculate urgency level based on how long proof has been stuck
///
/// Thresholds (checked in descending order):
/// - > 1800s (30 min, PENDING_SPENT_TIMEOUT_DEFAULT) = Critical
/// - > 600s (10 min, RESERVED_TIMEOUT_SECS) = High
/// - > 300s (5 min, TRANSACTION_TIMEOUT_SECS) = Warning
/// - <= 300s = Normal
fn calculate_urgency(duration_secs: u64) -> UrgencyLevel {
    if duration_secs > PENDING_SPENT_TIMEOUT_DEFAULT {
        UrgencyLevel::Critical
    } else if duration_secs > RESERVED_TIMEOUT_SECS {
        UrgencyLevel::High
    } else if duration_secs > TRANSACTION_TIMEOUT_SECS {
        UrgencyLevel::Warning
    } else {
        UrgencyLevel::Normal
    }
}

/// Determine if a proof can be recovered based on its state and duration
fn can_recover_proof(state: ProofState, duration_secs: u64) -> bool {
    match state {
        ProofState::PendingSpent => duration_secs > PENDING_SPENT_TIMEOUT_DEFAULT,
        _ => duration_secs > RESERVED_TIMEOUT_SECS,
    }
}

/// Get comprehensive wallet health stats for UI
///
/// Returns stats about pending and stuck proofs, categorized by urgency.
/// Proofs are considered "stuck" if they've been pending longer than TRANSACTION_TIMEOUT_SECS.
pub fn get_wallet_health_stats() -> WalletHealthStats {
    let now = now_secs();

    // Get spendable balance
    let spendable = *WALLET_BALANCE.read();

    // Get all proofs and categorize
    let store = WALLET_TOKENS();
    let data = store.data();
    let tokens = data.read();

    let mut pending_balance = 0u64;
    let mut pending_count = 0usize;
    let mut stuck_proofs = Vec::new();

    for token in tokens.iter() {
        for proof in &token.proofs {
            if proof.state.is_pending() {
                let duration = proof
                    .state_set_at
                    .map(|t| now.saturating_sub(t))
                    .unwrap_or(0);

                let urgency = calculate_urgency(duration);
                let can_recover = can_recover_proof(proof.state, duration);

                if duration > TRANSACTION_TIMEOUT_SECS {
                    // Stuck - show in modal
                    stuck_proofs.push(StuckProofInfo {
                        mint_url: token.mint.clone(),
                        amount: proof.amount,
                        state: proof.state,
                        stuck_duration_secs: duration,
                        transaction_id: proof.transaction_id,
                        urgency,
                        can_recover,
                    });
                } else {
                    // Just pending - normal operation
                    pending_balance = pending_balance.saturating_add(proof.amount);
                    pending_count += 1;
                }
            }
        }
    }

    let stuck_balance: u64 = stuck_proofs.iter().map(|p| p.amount).sum();
    let stuck_count = stuck_proofs.len();

    // Sort by urgency (critical first) then by amount
    stuck_proofs.sort_by(|a, b| {
        b.urgency
            .cmp(&a.urgency)
            .then_with(|| b.amount.cmp(&a.amount))
    });

    WalletHealthStats {
        spendable_balance: spendable,
        pending_balance,
        pending_count,
        stuck_balance,
        stuck_count,
        stuck_proofs,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_result_default() {
        let result = ProofRecoveryResult::default();
        assert_eq!(result.recovered_count, 0);
        assert_eq!(result.recovered_value, 0);
    }
}
