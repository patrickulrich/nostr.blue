//! Dust Consolidation
//!
//! Automatic consolidation of small proofs ("dust") to reduce wallet overhead.
//! Small proofs increase storage, fee costs, and transaction complexity.
#![allow(dead_code)]
use super::internal::get_or_create_wallet;
use super::proofs::proof_data_to_cdk_proof;
use super::signals::{try_acquire_mint_lock, WALLET_TOKENS};
use super::types::{ProofData, WalletTokensStoreStoreExt};
use super::utils::{mint_matches, normalize_mint_url};
use crate::stores::cashu_cdk_bridge;
use dioxus::prelude::ReadableExt;
use futures::stream::{self, StreamExt};
use std::collections::HashMap;
/// Default threshold for considering a proof as "dust" (in sats)
pub const DEFAULT_DUST_THRESHOLD: u64 = 4;
/// Minimum number of dust proofs before consolidation is recommended
pub const MIN_DUST_COUNT: usize = 10;
/// Maximum dust value to consider for consolidation (in sats)
pub const MAX_DUST_TOTAL: u64 = 100;
/// Maximum concurrent mint consolidations to prevent overwhelming WASM event loop
/// and reduce risk of mint API rate limiting
const MAX_CONCURRENT_CONSOLIDATIONS: usize = 3;
/// Dust statistics for a mint
#[derive(Debug, Clone, Default)]
pub struct DustStats {
    /// Number of dust proofs
    pub count: usize,
    /// Total value of dust proofs
    pub total_value: u64,
    /// Average dust proof size
    pub avg_value: u64,
    /// Whether consolidation is recommended
    pub should_consolidate: bool,
    /// Estimated fee for consolidation
    pub estimated_fee: u64,
}
/// Find dust proofs for a mint
pub fn find_dust_proofs(mint_url: &str, threshold: u64) -> Vec<ProofData> {
    let normalized = normalize_mint_url(mint_url);
    let store = WALLET_TOKENS();
    let data = store.data();
    let tokens = data.read();
    tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, &normalized))
        .flat_map(|t| t.proofs.iter())
        .filter(|p| p.amount <= threshold)
        .cloned()
        .collect()
}
/// Get dust statistics for a mint
pub fn get_dust_stats(mint_url: &str, threshold: u64) -> DustStats {
    let dust = find_dust_proofs(mint_url, threshold);
    if dust.is_empty() {
        return DustStats::default();
    }
    let count = dust.len();
    let total_value: u64 = dust
        .iter()
        .map(|p| p.amount)
        .fold(0u64, |acc, amt| acc.saturating_add(amt));
    let avg_value = total_value / count as u64;
    let estimated_fee = count as u64;
    let should_consolidate =
        count >= MIN_DUST_COUNT && total_value > estimated_fee && total_value <= MAX_DUST_TOTAL;
    DustStats {
        count,
        total_value,
        avg_value,
        should_consolidate,
        estimated_fee,
    }
}
/// Get dust statistics for all mints
pub fn get_all_dust_stats(threshold: u64) -> HashMap<String, DustStats> {
    let store = WALLET_TOKENS();
    let data = store.data();
    let tokens = data.read();
    let mints: Vec<String> = tokens.iter().map(|t| t.mint.clone()).collect();
    let unique_mints: Vec<String> = mints
        .into_iter()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let mut stats = HashMap::new();
    for mint in unique_mints {
        let mint_stats = get_dust_stats(&mint, threshold);
        if mint_stats.count > 0 {
            stats.insert(mint, mint_stats);
        }
    }
    stats
}
/// Consolidation result
#[derive(Debug, Clone, Default)]
pub struct DustConsolidationResult {
    /// Number of dust proofs consolidated
    pub proofs_consolidated: usize,
    /// Value before consolidation
    pub input_value: u64,
    /// Value after consolidation
    pub output_value: u64,
    /// Fee paid
    pub fee_paid: u64,
    /// Number of output proofs
    pub output_proofs: usize,
}
/// Consolidate dust proofs for a mint
///
/// Swaps small proofs into larger denominations to reduce overhead.
pub async fn consolidate_dust(
    mint_url: &str,
    threshold: u64,
) -> Result<DustConsolidationResult, String> {
    use cdk::amount::SplitTarget;
    let normalized = normalize_mint_url(mint_url);
    let _lock = try_acquire_mint_lock(&normalized)
        .ok_or_else(|| format!("Another operation in progress for {}", normalized))?;
    let dust_proofs = find_dust_proofs(&normalized, threshold);
    if dust_proofs.is_empty() {
        return Ok(DustConsolidationResult::default());
    }
    let count = dust_proofs.len();
    let input_value: u64 = dust_proofs
        .iter()
        .map(|p| p.amount)
        .fold(0u64, |acc, amt| acc.saturating_add(amt));
    log::info!(
        "Consolidating {} dust proofs ({} sats) for {}",
        count,
        input_value,
        normalized
    );
    let estimated_fee = count as u64;
    if input_value <= estimated_fee {
        return Err(format!(
            "Dust consolidation not worthwhile: {} sats value, {} sats estimated fee",
            input_value, estimated_fee,
        ));
    }
    let cdk_proofs: Vec<cdk::nuts::Proof> = dust_proofs
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            proof_data_to_cdk_proof(p)
                .map_err(|e| format!("Failed to convert proof at index {}: {}", idx, e))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if cdk_proofs.is_empty() {
        return Err("No proofs to consolidate".to_string());
    }
    let wallet = get_or_create_wallet(&normalized).await?;
    let output_proofs = wallet
        .swap(None, SplitTarget::default(), cdk_proofs, None, false, false)
        .await
        .map_err(|e| format!("Dust consolidation swap failed: {}", e))?
        .ok_or("Swap returned no proofs")?;
    let output_value: u64 = output_proofs
        .iter()
        .map(|p| u64::from(p.amount))
        .fold(0u64, |acc, amt| acc.saturating_add(amt));
    let fee_paid = input_value.saturating_sub(output_value);
    log::info!(
        "Dust consolidation complete: {} proofs -> {} proofs, fee: {} sats",
        count,
        output_proofs.len(),
        fee_paid
    );
    if let Err(e) = cashu_cdk_bridge::sync_wallet_state().await {
        log::warn!(
            "Failed to sync wallet state after dust consolidation: {}",
            e
        );
    }
    Ok(DustConsolidationResult {
        proofs_consolidated: count,
        input_value,
        output_value,
        fee_paid,
        output_proofs: output_proofs.len(),
    })
}
/// Consolidate dust across all mints (bounded parallel execution)
///
/// Uses `buffer_unordered` to cap concurrent consolidations at `MAX_CONCURRENT_CONSOLIDATIONS`.
/// This prevents overwhelming the WASM event loop and reduces mint API rate-limit risk.
pub async fn consolidate_all_dust(
    threshold: u64,
) -> HashMap<String, Result<DustConsolidationResult, String>> {
    let stats = get_all_dust_stats(threshold);
    let mints_to_consolidate: Vec<_> = stats
        .into_iter()
        .filter(|(_, s)| s.should_consolidate)
        .map(|(url, _)| url)
        .collect();
    if mints_to_consolidate.is_empty() {
        return HashMap::new();
    }
    log::info!(
        "Consolidating dust for {} mints (max {} concurrent)",
        mints_to_consolidate.len(),
        MAX_CONCURRENT_CONSOLIDATIONS
    );
    let results: Vec<_> = stream::iter(mints_to_consolidate)
        .map(|mint_url| async move {
            let result = consolidate_dust(&mint_url, threshold).await;
            (mint_url, result)
        })
        .buffer_unordered(MAX_CONCURRENT_CONSOLIDATIONS)
        .collect()
        .await;
    results.into_iter().collect()
}
/// Check if dust consolidation is recommended for any mint
pub fn should_consolidate_dust() -> Vec<(String, DustStats)> {
    get_all_dust_stats(DEFAULT_DUST_THRESHOLD)
        .into_iter()
        .filter(|(_, stats)| stats.should_consolidate)
        .collect()
}
/// Get total dust across all mints
pub fn get_total_dust_stats() -> DustStats {
    let all_stats = get_all_dust_stats(DEFAULT_DUST_THRESHOLD);
    let mut total = DustStats::default();
    for stats in all_stats.values() {
        total.count = total.count.saturating_add(stats.count);
        total.total_value = total.total_value.saturating_add(stats.total_value);
        total.estimated_fee = total.estimated_fee.saturating_add(stats.estimated_fee);
    }
    if total.count > 0 {
        total.avg_value = total.total_value / total.count as u64;
        total.should_consolidate = total.count >= MIN_DUST_COUNT
            && total.total_value > total.estimated_fee
            && total.total_value <= MAX_DUST_TOTAL;
    }
    total
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_dust_stats_default() {
        let stats = DustStats::default();
        assert_eq!(stats.count, 0);
        assert_eq!(stats.total_value, 0);
        assert!(!stats.should_consolidate);
    }
}
