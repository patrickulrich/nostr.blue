//! Proof state management
//!
//! Functions for managing proof states, proof-to-event mapping,
//! and transaction lifecycle.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use dioxus::prelude::*;
use std::sync::atomic::Ordering;

use super::signals::*;
use super::types::*;
use super::utils::mint_matches;

// =============================================================================
// Cross-Platform Time Helper
// =============================================================================

/// Cross-platform timestamp helper (aligned with nostr SDK pattern)
/// Returns current time in seconds since Unix epoch
pub fn now_secs() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() / 1000.0) as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

// =============================================================================
// Proof-to-Event Mapping
// =============================================================================

/// Register proofs in the event map (called when adding new token events)
pub fn register_proofs_in_event_map(event_id: &str, proofs: &[ProofData]) {
    let mut map = PROOF_EVENT_MAP.write();
    for proof in proofs {
        map.insert(proof.secret.clone(), event_id.to_string());
    }
}

/// Unregister proofs from the event map (called when proofs are spent)
pub fn unregister_proofs_from_event_map(proof_secrets: &[String]) {
    let mut map = PROOF_EVENT_MAP.write();
    for secret in proof_secrets {
        map.remove(secret);
    }
}

/// Get unique event IDs for a list of proof secrets (for building deletion events)
pub fn get_event_ids_for_proofs(proof_secrets: &[String]) -> Vec<String> {
    let map = PROOF_EVENT_MAP.read();
    let mut event_ids: Vec<String> = proof_secrets
        .iter()
        .filter_map(|secret| map.get(secret).cloned())
        .collect();
    // Deduplicate
    event_ids.sort();
    event_ids.dedup();
    event_ids
}

/// Rebuild the proof-to-event map from WALLET_TOKENS (called on initialization)
pub fn rebuild_proof_event_map() {
    let mut map = PROOF_EVENT_MAP.write();
    map.clear();

    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();

    for token in tokens.iter() {
        for proof in &token.proofs {
            map.insert(proof.secret.clone(), token.event_id.clone());
        }
    }

    log::debug!("Rebuilt proof-event map with {} entries", map.len());
}

// =============================================================================
// Proof State Transitions
// =============================================================================

/// Move proofs to Reserved state (before sending ecash via PreparedSend)
pub fn move_proofs_to_reserved(proof_secrets: &[String], tx_id: u64) {
    let store = WALLET_TOKENS.read();
    let mut data = store.data();
    let mut tokens = data.write();
    let now = now_secs();

    for token in tokens.iter_mut() {
        for proof in &mut token.proofs {
            if proof_secrets.contains(&proof.secret) {
                proof.state = ProofState::Reserved;
                proof.transaction_id = Some(tx_id);
                proof.state_set_at = Some(now);
            }
        }
    }

    log::debug!(
        "Marked {} proofs as Reserved for tx {}",
        proof_secrets.len(),
        tx_id
    );
}

/// Move proofs to Spent state
pub fn move_proofs_to_spent(proof_secrets: &[String]) {
    let store = WALLET_TOKENS.read();
    let mut data = store.data();
    let mut tokens = data.write();

    for token in tokens.iter_mut() {
        for proof in &mut token.proofs {
            if proof_secrets.contains(&proof.secret) {
                proof.state = ProofState::Spent;
                proof.transaction_id = None;
                proof.state_set_at = None; // Clear timestamp for terminal state
            }
        }
    }

    // Also unregister from proof-event map
    unregister_proofs_from_event_map(proof_secrets);

    log::debug!("Marked {} proofs as Spent", proof_secrets.len());
}

/// Revert proofs to Unspent state (failed transaction recovery)
pub fn revert_proofs_to_spendable(proof_secrets: &[String]) {
    let store = WALLET_TOKENS.read();
    let mut data = store.data();
    let mut tokens = data.write();

    for token in tokens.iter_mut() {
        for proof in &mut token.proofs {
            if proof_secrets.contains(&proof.secret) {
                proof.state = ProofState::Unspent;
                proof.transaction_id = None;
                proof.state_set_at = None; // Clear timestamp when reverting
            }
        }
    }

    log::debug!("Reverted {} proofs to Unspent", proof_secrets.len());
}

// =============================================================================
// Proof Queries
// =============================================================================

/// Get proofs by secret for a specific mint
pub fn get_proofs_by_secrets(mint_url: &str, proof_secrets: &[String]) -> Vec<ProofData> {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();

    tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url))
        .flat_map(|t| &t.proofs)
        .filter(|p| proof_secrets.contains(&p.secret))
        .cloned()
        .collect()
}

/// Get all spendable proofs for a mint (not pending, not spent)
pub fn get_spendable_proofs_for_mint(mint_url: &str) -> Vec<ProofData> {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();

    tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url))
        .flat_map(|t| &t.proofs)
        .filter(|p| p.state.is_spendable())
        .cloned()
        .collect()
}

/// Get all pending proofs for a mint
pub fn get_pending_proofs_for_mint(mint_url: &str) -> Vec<ProofData> {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();

    tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url))
        .flat_map(|t| &t.proofs)
        .filter(|p| p.state.is_pending())
        .cloned()
        .collect()
}

/// Get all proofs for a mint (regardless of state)
pub fn get_all_proofs_for_mint(mint_url: &str) -> Vec<ProofData> {
    let store = WALLET_TOKENS.read();
    let data = store.data();
    let tokens = data.read();

    tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url))
        .flat_map(|t| &t.proofs)
        .cloned()
        .collect()
}

// =============================================================================
// Pending-at-Mint Tracking (Dual Pending State)
// =============================================================================

/// Register proofs as pending at the mint level (called when mint reports PENDING state)
/// This happens during lightning payments when the mint is processing the payment
///
/// CDK best practice: Store timestamp for TTL-based cleanup
pub fn register_proofs_pending_at_mint(proof_secrets: &[String]) {
    let now = now_secs();
    let mut pending = PENDING_BY_MINT_SECRETS.write();
    for secret in proof_secrets {
        pending.insert(secret.clone(), now);
    }
    log::debug!(
        "Registered {} proofs as pending at mint (ts={})",
        proof_secrets.len(),
        now
    );
}

/// Remove proofs from pending-at-mint state (called when mint state changes to SPENT or UNSPENT)
pub fn remove_from_pending_at_mint(proof_secrets: &[String]) {
    let mut pending = PENDING_BY_MINT_SECRETS.write();
    for secret in proof_secrets {
        pending.remove(secret);
    }
    log::debug!("Removed {} proofs from pending at mint", proof_secrets.len());
}

/// Check if a proof is pending at the mint level
pub fn is_proof_pending_at_mint(proof_secret: &str) -> bool {
    PENDING_BY_MINT_SECRETS.read().contains_key(proof_secret)
}

/// Get all proofs that are pending at the mint level
pub fn get_proofs_pending_at_mint() -> Vec<String> {
    PENDING_BY_MINT_SECRETS.read().keys().cloned().collect()
}

/// Clear all pending-at-mint state (called during wallet reset/reinit)
pub fn clear_pending_at_mint() {
    PENDING_BY_MINT_SECRETS.write().clear();
    log::debug!("Cleared all pending-at-mint proofs");
}

/// Cleanup stale pending-at-mint entries (TTL-based garbage collection)
///
/// CDK best practice: Lightning payments should complete within a reasonable time.
/// If a proof has been pending for too long, it's likely the payment failed silently.
/// This function removes entries older than MAX_PENDING_AGE_SECS.
///
/// Should be called periodically (e.g., at the start of sync_state_with_mint).
pub fn cleanup_old_pending_at_mint() {
    const MAX_PENDING_AGE_SECS: u64 = 600; // 10 minutes

    let now = now_secs();
    let before_count = PENDING_BY_MINT_SECRETS.read().len();

    PENDING_BY_MINT_SECRETS.write().retain(|_secret, timestamp| {
        let age = now.saturating_sub(*timestamp);
        if age > MAX_PENDING_AGE_SECS {
            log::debug!("Cleaning up stale pending proof (age={}s)", age);
            false
        } else {
            true
        }
    });

    let after_count = PENDING_BY_MINT_SECRETS.read().len();
    if before_count != after_count {
        log::info!(
            "Cleaned up {} stale pending-at-mint proofs",
            before_count - after_count
        );
    }
}

// =============================================================================
// Transaction Lifecycle
// =============================================================================

/// Create a new active transaction and return its ID
pub fn create_transaction(
    tx_type: TransactionType,
    amount: u64,
    mint_url: &str,
    proof_secrets: Vec<String>,
    quote_id: Option<String>,
    memo: Option<String>,
) -> u64 {
    let id = TRANSACTION_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let now = chrono::Utc::now().timestamp() as u64;

    let tx = ActiveTransaction {
        id,
        tx_type,
        amount,
        unit: "sat".to_string(),
        mint_url: mint_url.to_string(),
        status: TransactionStatus::Draft,
        proof_secrets,
        quote_id,
        memo,
        expires_at: None,
        created_at: now,
        updated_at: now,
        history: vec![TransactionStatusUpdate {
            status: TransactionStatus::Draft,
            timestamp: now,
            message: None,
            fee_paid: None,
        }],
    };

    ACTIVE_TRANSACTIONS.write().push(tx);
    log::debug!("Created transaction {} with status Draft", id);
    id
}

/// Update transaction status
pub fn update_transaction_status(
    tx_id: u64,
    status: TransactionStatus,
    message: Option<String>,
    fee_paid: Option<u64>,
) {
    let mut transactions = ACTIVE_TRANSACTIONS.write();
    if let Some(tx) = transactions.iter_mut().find(|t| t.id == tx_id) {
        let now = chrono::Utc::now().timestamp() as u64;
        tx.status = status.clone();
        tx.updated_at = now;
        tx.history.push(TransactionStatusUpdate {
            status,
            timestamp: now,
            message,
            fee_paid,
        });
        log::debug!("Updated transaction {} status to {:?}", tx_id, tx.status);
    }
}

/// Get transaction by ID
pub fn get_transaction(tx_id: u64) -> Option<ActiveTransaction> {
    ACTIVE_TRANSACTIONS
        .read()
        .iter()
        .find(|t| t.id == tx_id)
        .cloned()
}

/// Get all pending transactions
pub fn get_pending_transactions() -> Vec<ActiveTransaction> {
    ACTIVE_TRANSACTIONS
        .read()
        .iter()
        .filter(|t| {
            matches!(
                t.status,
                TransactionStatus::Draft
                    | TransactionStatus::Prepared
                    | TransactionStatus::Pending
            )
        })
        .cloned()
        .collect()
}

/// Remove completed/reverted transactions older than 1 hour (cleanup)
pub fn cleanup_old_transactions() {
    let one_hour_ago = chrono::Utc::now().timestamp() as u64 - 3600;
    ACTIVE_TRANSACTIONS.write().retain(|tx| {
        // Keep pending transactions and recent ones
        matches!(
            tx.status,
            TransactionStatus::Draft | TransactionStatus::Prepared | TransactionStatus::Pending
        ) || tx.updated_at > one_hour_ago
    });
}

// =============================================================================
// Proof Conversion
// =============================================================================

/// Convert ProofData to CDK Proof
pub fn proof_data_to_cdk_proof(data: &ProofData) -> Result<cdk::nuts::Proof, String> {
    use cdk::nuts::{Proof, PublicKey, SecretKey};
    use cdk::secret::Secret;
    use std::str::FromStr;

    let keyset_id = cdk::nuts::Id::from_str(&data.id)
        .map_err(|e| format!("Invalid keyset ID '{}': {}", data.id, e))?;

    let secret = Secret::from_str(&data.secret)
        .map_err(|e| format!("Invalid secret: {}", e))?;

    let c = PublicKey::from_hex(&data.c).map_err(|e| format!("Invalid C point: {}", e))?;

    // Handle witness (P2PK)
    let witness = if let Some(ref w) = data.witness {
        Some(
            serde_json::from_str(w)
                .map_err(|e| format!("Invalid witness JSON: {}", e))?,
        )
    } else {
        None
    };

    // Handle DLEQ
    let dleq = if let Some(ref d) = data.dleq {
        Some(cdk::nuts::ProofDleq {
            e: SecretKey::from_hex(&d.e).map_err(|e| format!("Invalid DLEQ e: {}", e))?,
            s: SecretKey::from_hex(&d.s).map_err(|e| format!("Invalid DLEQ s: {}", e))?,
            r: SecretKey::from_hex(&d.r).map_err(|e| format!("Invalid DLEQ r: {}", e))?,
        })
    } else {
        None
    };

    Ok(Proof {
        keyset_id,
        amount: cdk::Amount::from(data.amount),
        secret,
        c,
        witness,
        dleq,
    })
}

/// Convert CDK Proof to ProofData
pub fn cdk_proof_to_proof_data(proof: &cdk::nuts::Proof) -> ProofData {
    // Serialize witness if present
    let witness = proof.witness.as_ref()
        .and_then(|w| serde_json::to_string(w).ok());

    // Convert DLEQ if present - serialize as strings
    let dleq = proof.dleq.as_ref()
        .map(|d| DleqData {
            e: d.e.to_string(),
            s: d.s.to_string(),
            r: d.r.to_string(),
        });

    ProofData {
        id: proof.keyset_id.to_string(),
        amount: u64::from(proof.amount),
        secret: proof.secret.to_string(),
        c: proof.c.to_hex(),
        witness,
        dleq,
        state: ProofState::Unspent,
        transaction_id: None,
        state_set_at: None,
    }
}

// =============================================================================
// Fee-Aware Proof Selection (CDK pattern)
// =============================================================================

/// Select proofs for a given amount, optimized for fees
///
/// This implements a simplified version of CDK's fee-aware proof selection:
/// 1. Prioritize proofs from inactive keysets (to rotate them out)
/// 2. Consider keyset fees when selecting proofs
/// 3. Try to select exact amounts to avoid unnecessary swaps
///
/// Returns selected proofs and estimated fee.
pub fn select_proofs_for_amount(
    proofs: &[ProofData],
    target_amount: u64,
    include_fee: bool,
    fee_ppk: u64, // Fee per proof in ppk (parts per 1000)
) -> Result<(Vec<ProofData>, u64), String> {
    if proofs.is_empty() {
        return Err("No proofs available".to_string());
    }

    // Calculate total available with overflow protection
    // CDK best practice: Use checked arithmetic for financial calculations
    let total_available: u64 = proofs.iter()
        .filter(|p| p.state.is_spendable())
        .map(|p| p.amount)
        .try_fold(0u64, |acc, amt| acc.checked_add(amt))
        .ok_or("Total available balance overflow")?;

    if total_available < target_amount {
        return Err(format!(
            "Insufficient funds: available={}, required={}",
            total_available, target_amount
        ));
    }

    // Sort proofs by amount (descending) for greedy selection
    let mut sorted_proofs: Vec<ProofData> = proofs.iter()
        .filter(|p| p.state.is_spendable())
        .cloned()
        .collect();
    sorted_proofs.sort_by(|a, b| b.amount.cmp(&a.amount));

    let mut selected = Vec::new();
    let mut selected_amount: u64 = 0;

    // Calculate fee estimate function
    let estimate_fee = |num_proofs: usize| -> u64 {
        if !include_fee || fee_ppk == 0 {
            return 0;
        }
        // Fee = (num_proofs * fee_ppk) / 1000, rounded up
        // Use saturating arithmetic to prevent overflow
        let base = (num_proofs as u64).saturating_mul(fee_ppk);
        base.saturating_add(999) / 1000
    };

    // Phase 1: Try to find exact match
    for proof in &sorted_proofs {
        if proof.amount == target_amount && selected.is_empty() {
            selected.push(proof.clone());
            let fee = estimate_fee(1);
            return Ok((selected, fee));
        }
    }

    // Phase 2: Greedy selection
    for proof in &sorted_proofs {
        let current_fee = estimate_fee(selected.len() + 1);
        let amount_needed = if include_fee {
            target_amount + current_fee
        } else {
            target_amount
        };

        if selected_amount >= amount_needed {
            break;
        }

        selected.push(proof.clone());
        selected_amount += proof.amount;
    }

    // Verify we have enough
    let final_fee = estimate_fee(selected.len());
    let total_needed = if include_fee {
        target_amount + final_fee
    } else {
        target_amount
    };

    if selected_amount < total_needed {
        return Err(format!(
            "Could not select enough proofs: selected={}, needed={}",
            selected_amount, total_needed
        ));
    }

    Ok((selected, final_fee))
}

/// Select proofs with keyset preference (prefer inactive keysets)
///
/// Active keysets are ones currently in use by the mint.
/// We prefer to spend proofs from inactive keysets first to consolidate.
pub fn select_proofs_prefer_inactive(
    proofs: &[ProofData],
    target_amount: u64,
    active_keyset_ids: &[String],
) -> Result<Vec<ProofData>, String> {
    // Separate into inactive and active keyset proofs
    let mut inactive_proofs: Vec<ProofData> = proofs.iter()
        .filter(|p| p.state.is_spendable() && !active_keyset_ids.contains(&p.id))
        .cloned()
        .collect();

    let mut active_proofs: Vec<ProofData> = proofs.iter()
        .filter(|p| p.state.is_spendable() && active_keyset_ids.contains(&p.id))
        .cloned()
        .collect();

    // Sort both by amount (descending)
    inactive_proofs.sort_by(|a, b| b.amount.cmp(&a.amount));
    active_proofs.sort_by(|a, b| b.amount.cmp(&a.amount));

    let mut selected = Vec::new();
    let mut selected_amount: u64 = 0;

    // First, select from inactive keysets
    for proof in &inactive_proofs {
        if selected_amount >= target_amount {
            break;
        }
        selected.push(proof.clone());
        selected_amount += proof.amount;
    }

    // Then, if needed, select from active keysets
    for proof in &active_proofs {
        if selected_amount >= target_amount {
            break;
        }
        selected.push(proof.clone());
        selected_amount += proof.amount;
    }

    if selected_amount < target_amount {
        return Err(format!(
            "Insufficient funds: selected={}, required={}",
            selected_amount, target_amount
        ));
    }

    Ok(selected)
}
