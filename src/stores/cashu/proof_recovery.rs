//! Proof State Recovery
//!
//! Handles automatic recovery of proofs stuck in transient states.
//! Implements timeout-based recovery for Reserved/PendingSpent proofs.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use std::collections::HashMap;

use dioxus::prelude::{ReadableExt, WritableExt};

use super::internal::get_or_create_wallet;
use super::proofs::proof_data_to_cdk_proof;
use super::signals::{WALLET_BALANCE, WALLET_TOKENS};
use super::types::{ProofData, ProofState, WalletTokensStoreStoreExt};
use super::utils::now_secs;

// =============================================================================
// Recovery Constants
// =============================================================================

/// Default timeout for Reserved proofs (24 hours)
pub const RESERVED_TIMEOUT_SECS: u64 = 24 * 60 * 60;

/// Default timeout for PendingSpent proofs (1 hour)
pub const PENDING_SPENT_TIMEOUT_SECS: u64 = 60 * 60;

/// Short timeout for transactions (5 minutes)
pub const TRANSACTION_TIMEOUT_SECS: u64 = 5 * 60;

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
    /// Timestamp when state was set
    pub state_set_at: u64,
    /// Associated transaction ID (if any)
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

/// Find proofs in Reserved state
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

/// Recover stuck Reserved proofs by resetting to Unspent
///
/// This should only be called when we're certain the proofs weren't used.
pub fn recover_reserved_proofs() -> ProofRecoveryResult {
    let reserved = find_reserved_proofs();

    if reserved.is_empty() {
        return ProofRecoveryResult::default();
    }

    log::info!("Recovering {} reserved proofs", reserved.len());

    let secrets: Vec<String> = reserved.iter().map(|(_, p)| p.secret.clone()).collect();
    let recovered_value: u64 = reserved
        .iter()
        .map(|(_, p)| p.amount)
        .fold(0u64, |acc, amt| acc.saturating_add(amt));

    // Reset states in storage
    {
        let store = WALLET_TOKENS();
        let mut data = store.data();
        let mut tokens = data.write();

        for token in tokens.iter_mut() {
            for proof in token.proofs.iter_mut() {
                if secrets.contains(&proof.secret) {
                    proof.state = ProofState::Unspent;
                    proof.transaction_id = None;
                }
            }
        }
    }

    // Update balance
    recalculate_balance();

    log::info!(
        "Recovered {} reserved proofs worth {} sats",
        reserved.len(),
        recovered_value
    );

    ProofRecoveryResult {
        recovered_count: reserved.len(),
        recovered_value,
        spent_count: 0,
        spent_value: 0,
        errors: Vec::new(),
    }
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
pub async fn run_full_recovery() -> ProofRecoveryResult {
    log::info!("Running full proof recovery");

    // First recover reserved proofs (local operation)
    let reserved_result = recover_reserved_proofs();

    // Then check pending spent proofs with mints
    let pending_result = recover_pending_spent_proofs().await;

    ProofRecoveryResult {
        recovered_count: reserved_result.recovered_count + pending_result.recovered_count,
        recovered_value: reserved_result.recovered_value + pending_result.recovered_value,
        spent_count: reserved_result.spent_count + pending_result.spent_count,
        spent_value: reserved_result.spent_value + pending_result.spent_value,
        errors: [reserved_result.errors, pending_result.errors].concat(),
    }
}

/// Get recovery stats without performing recovery
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
