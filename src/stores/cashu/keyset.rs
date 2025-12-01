//! Keyset Migration and Management
//!
//! Handles keyset rotation detection, proof migration, and keyset state tracking.
//! Implements CDK patterns for keyset lifecycle management.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use std::collections::HashMap;
use std::str::FromStr;

use cdk::nuts::Id;
use dioxus::prelude::ReadableExt;

use super::cache::{CachedKeyset, MINT_CACHE};
use super::signals::WALLET_TOKENS;
use super::types::{ProofData, WalletTokensStoreStoreExt};
use super::utils::mint_matches;

// =============================================================================
// Keyset State Types
// =============================================================================

/// Extended keyset info with local tracking
#[derive(Debug, Clone)]
pub struct KeysetState {
    /// Keyset ID
    pub id: String,
    /// Whether this keyset is currently active at the mint
    pub active: bool,
    /// Input fee in ppk
    pub input_fee_ppk: u64,
    /// Number of proofs we have from this keyset
    pub proof_count: usize,
    /// Total value of proofs from this keyset
    pub proof_value: u64,
}

/// Result of keyset refresh operation
#[derive(Debug, Clone)]
pub struct KeysetRefreshResult {
    /// Newly detected keysets
    pub new_keysets: Vec<Id>,
    /// Keysets that became inactive (rotated)
    pub rotated_keysets: Vec<Id>,
    /// Keysets that are still active
    pub active_keysets: Vec<Id>,
    /// Total proofs in inactive keysets (should be migrated)
    pub proofs_to_migrate: usize,
    /// Value of proofs in inactive keysets
    pub value_to_migrate: u64,
}

/// Migration result
#[derive(Debug, Clone)]
pub struct KeysetMigrationResult {
    /// Number of proofs migrated
    pub proofs_migrated: usize,
    /// Value migrated in sats
    pub value_migrated: u64,
    /// Fee paid for migration swap
    pub fee_paid: u64,
    /// New keyset ID proofs were migrated to
    pub target_keyset: Id,
}

// =============================================================================
// Cache Helpers
// =============================================================================

/// Get cached keysets for a mint
fn get_cached_keysets(mint_url: &str) -> Vec<CachedKeyset> {
    let cache = MINT_CACHE();
    if let Some(entry) = cache.get_mint(mint_url) {
        entry.keysets.values().cloned().collect()
    } else {
        vec![]
    }
}

// =============================================================================
// Keyset Detection
// =============================================================================

/// Get active keyset IDs for a mint from local cache
pub fn get_active_keyset_ids(mint_url: &str) -> Vec<String> {
    get_cached_keysets(mint_url)
        .iter()
        .filter(|ks| ks.active)
        .map(|ks| ks.id.clone())
        .collect()
}

/// Get all keyset IDs (active and inactive) for a mint from local cache
pub fn get_all_keyset_ids(mint_url: &str) -> Vec<String> {
    get_cached_keysets(mint_url)
        .iter()
        .map(|ks| ks.id.clone())
        .collect()
}

/// Check if a keyset is active
pub fn is_keyset_active(mint_url: &str, keyset_id: &str) -> bool {
    get_active_keyset_ids(mint_url).contains(&keyset_id.to_string())
}

/// Get proofs grouped by keyset ID
pub fn get_proofs_by_keyset(mint_url: &str) -> HashMap<String, Vec<ProofData>> {
    let store = WALLET_TOKENS();
    let data = store.data();
    let tokens = data.read();

    let mut by_keyset: HashMap<String, Vec<ProofData>> = HashMap::new();

    for token in tokens.iter().filter(|t| mint_matches(&t.mint, mint_url)) {
        for proof in &token.proofs {
            by_keyset
                .entry(proof.id.clone())
                .or_default()
                .push(proof.clone());
        }
    }

    by_keyset
}

/// Get proofs from inactive keysets that should be migrated
pub fn get_proofs_to_migrate(mint_url: &str) -> Vec<ProofData> {
    let active_keysets = get_active_keyset_ids(mint_url);
    let store = WALLET_TOKENS();
    let data = store.data();
    let tokens = data.read();

    tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url))
        .flat_map(|t| &t.proofs)
        .filter(|p| !active_keysets.contains(&p.id))
        .cloned()
        .collect()
}

/// Count proofs by keyset active status
pub fn count_proofs_by_status(mint_url: &str) -> (usize, usize, u64, u64) {
    let active_keysets = get_active_keyset_ids(mint_url);
    let store = WALLET_TOKENS();
    let data = store.data();
    let tokens = data.read();

    let mut active_count = 0usize;
    let mut inactive_count = 0usize;
    let mut active_value = 0u64;
    let mut inactive_value = 0u64;

    for token in tokens.iter().filter(|t| mint_matches(&t.mint, mint_url)) {
        for proof in &token.proofs {
            if active_keysets.contains(&proof.id) {
                active_count += 1;
                active_value += proof.amount;
            } else {
                inactive_count += 1;
                inactive_value += proof.amount;
            }
        }
    }

    (active_count, inactive_count, active_value, inactive_value)
}

// =============================================================================
// Keyset Refresh
// =============================================================================

/// Refresh keysets from mint and detect rotation
///
/// This fetches the latest keyset info from the mint and compares with
/// our local cache to detect any keysets that have been rotated.
pub async fn refresh_keysets(mint_url: &str) -> Result<KeysetRefreshResult, String> {
    use super::cache::CachedKeyset;
    use super::internal::get_or_create_wallet;
    use super::utils::now_secs;

    log::info!("Refreshing keysets for mint: {}", mint_url);

    // Get current keyset IDs from cache before refresh
    let old_keysets = get_cached_keysets(mint_url);
    let old_keyset_ids: Vec<String> = old_keysets.iter().map(|k| k.id.clone()).collect();
    let old_active_ids: Vec<String> = old_keysets.iter()
        .filter(|k| k.active)
        .map(|k| k.id.clone())
        .collect();

    // Fetch fresh keysets from mint
    let wallet = get_or_create_wallet(mint_url).await?;

    // Force refresh keysets from mint
    let fresh_keysets = wallet.get_mint_keysets().await
        .map_err(|e| format!("Failed to fetch keysets: {}", e))?;

    // Update cache with new keysets
    {
        let now = now_secs();
        let mut cache = MINT_CACHE.write();
        let entry = cache.get_or_create_mint(mint_url);

        for keyset in fresh_keysets.iter() {
            let cached = CachedKeyset {
                id: keyset.id.to_string(),
                unit: keyset.unit.to_string(),
                active: keyset.active,
                input_fee_ppk: keyset.input_fee_ppk,
                cached_at: now,
            };
            entry.keysets.insert(cached.id.clone(), cached);
        }
        entry.version += 1;
    }

    // Analyze changes
    let new_keyset_ids: Vec<String> = fresh_keysets.iter()
        .map(|k| k.id.to_string())
        .collect();

    let new_active_ids: Vec<String> = fresh_keysets.iter()
        .filter(|k| k.active)
        .map(|k| k.id.to_string())
        .collect();

    // Find newly added keysets
    let new_keysets: Vec<Id> = new_keyset_ids.iter()
        .filter(|id| !old_keyset_ids.contains(id))
        .filter_map(|id| Id::from_str(id).ok())
        .collect();

    // Find rotated keysets (were active, now inactive)
    let rotated_keysets: Vec<Id> = old_active_ids.iter()
        .filter(|id| !new_active_ids.contains(id))
        .filter_map(|id| Id::from_str(id).ok())
        .collect();

    // Get active keysets
    let active_keysets: Vec<Id> = new_active_ids.iter()
        .filter_map(|id| Id::from_str(id).ok())
        .collect();

    // Count proofs in inactive keysets
    let (_, inactive_count, _, inactive_value) = count_proofs_by_status(mint_url);

    let result = KeysetRefreshResult {
        new_keysets,
        rotated_keysets,
        active_keysets,
        proofs_to_migrate: inactive_count,
        value_to_migrate: inactive_value,
    };

    if !result.rotated_keysets.is_empty() {
        log::warn!(
            "Detected {} rotated keysets with {} proofs ({} sats) to migrate",
            result.rotated_keysets.len(),
            result.proofs_to_migrate,
            result.value_to_migrate
        );
    }

    if !result.new_keysets.is_empty() {
        log::info!("Detected {} new keysets", result.new_keysets.len());
    }

    Ok(result)
}

// =============================================================================
// Keyset Migration
// =============================================================================

/// Migrate proofs from inactive keysets to the active keyset
///
/// This swaps all proofs from rotated/inactive keysets to the current active keyset.
/// Should be called after detecting keyset rotation to prevent loss of funds.
pub async fn migrate_inactive_proofs(mint_url: &str) -> Result<KeysetMigrationResult, String> {
    use super::proofs::proof_data_to_cdk_proof;
    use super::internal::get_or_create_wallet;
    use cdk::amount::SplitTarget;

    log::info!("Migrating proofs from inactive keysets for mint: {}", mint_url);

    // Get proofs to migrate
    let proofs_to_migrate = get_proofs_to_migrate(mint_url);

    if proofs_to_migrate.is_empty() {
        log::info!("No proofs to migrate");
        return Ok(KeysetMigrationResult {
            proofs_migrated: 0,
            value_migrated: 0,
            fee_paid: 0,
            target_keyset: Id::from_str("00000000").unwrap(),
        });
    }

    let total_value: u64 = proofs_to_migrate
        .iter()
        .map(|p| p.amount)
        .fold(0u64, |acc, amt| acc.saturating_add(amt));
    let proof_count = proofs_to_migrate.len();

    log::info!(
        "Found {} proofs ({} sats) in inactive keysets to migrate",
        proof_count,
        total_value
    );

    // Convert to CDK proofs
    let cdk_proofs: Vec<cdk::nuts::Proof> = proofs_to_migrate.iter()
        .filter_map(|p| proof_data_to_cdk_proof(p).ok())
        .collect();

    if cdk_proofs.len() != proof_count {
        log::warn!(
            "Some proofs failed to convert: {} of {}",
            proof_count - cdk_proofs.len(),
            proof_count
        );
    }

    // Get wallet and swap proofs
    let wallet = get_or_create_wallet(mint_url).await?;

    // Get active keyset
    let active_keyset = wallet.get_active_keyset().await
        .map_err(|e| format!("Failed to get active keyset: {}", e))?;

    // Swap all proofs to active keyset
    // This uses CDK's internal swap which outputs to the active keyset
    let swap_result = wallet.swap(
        None, // No specific amount, swap all
        SplitTarget::default(),
        cdk_proofs.clone().into(),
        None, // No spending conditions
        true, // Include fee
    ).await
    .map_err(|e| format!("Swap failed: {}", e))?;

    // Calculate fee paid
    let output_value: u64 = swap_result.as_ref()
        .map(|proofs| proofs.iter().map(|p| u64::from(p.amount)).fold(0u64, |acc, amt| acc.saturating_add(amt)))
        .unwrap_or(0);

    let fee_paid = total_value.saturating_sub(output_value);

    log::info!(
        "Migration complete: {} proofs ({} sats) migrated to keyset {}, fee: {} sats",
        proof_count,
        output_value,
        active_keyset.id,
        fee_paid
    );

    Ok(KeysetMigrationResult {
        proofs_migrated: proof_count,
        value_migrated: output_value,
        fee_paid,
        target_keyset: active_keyset.id,
    })
}

/// Check if migration is recommended for a mint
///
/// Returns true if there are proofs in inactive keysets that should be migrated.
pub fn should_migrate(mint_url: &str) -> bool {
    let (_, inactive_count, _, _) = count_proofs_by_status(mint_url);
    inactive_count > 0
}

/// Get migration recommendation with details
pub fn get_migration_recommendation(mint_url: &str) -> Option<(usize, u64)> {
    let (_, inactive_count, _, inactive_value) = count_proofs_by_status(mint_url);

    if inactive_count > 0 {
        Some((inactive_count, inactive_value))
    } else {
        None
    }
}

// =============================================================================
// Keyset Fee Helpers
// =============================================================================

/// Get the fee per proof (ppk) for a keyset
pub fn get_keyset_fee_ppk(mint_url: &str, keyset_id: &str) -> Option<u64> {
    get_cached_keysets(mint_url)
        .iter()
        .find(|ks| ks.id == keyset_id)
        .map(|ks| ks.input_fee_ppk)
}

/// Calculate total fee for a set of proofs
pub fn calculate_proofs_fee(mint_url: &str, proofs: &[ProofData]) -> u64 {
    let mut total_fee = 0u64;

    for proof in proofs {
        if let Some(fee_ppk) = get_keyset_fee_ppk(mint_url, &proof.id) {
            // Fee = fee_ppk / 1000 per proof, rounded up
            // Use saturating arithmetic to prevent overflow
            total_fee = total_fee.saturating_add(fee_ppk.saturating_add(999) / 1000);
        }
    }

    total_fee
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_proofs_by_status_empty() {
        // This would need mock data, just verify it doesn't panic
        let (active, inactive, _, _) = count_proofs_by_status("https://nonexistent.mint");
        assert_eq!(active, 0);
        assert_eq!(inactive, 0);
    }
}
