//! Adaptive Proof Pagination
//!
//! Dynamic batch sizing based on mint's input limits.
//! Prevents errors from exceeding mint constraints.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use std::collections::HashMap;

use super::capabilities::get_mint_capabilities;
use super::types::ProofData;

// =============================================================================
// Default Limits
// =============================================================================

/// Default batch size when mint limits are unknown
pub const DEFAULT_BATCH_SIZE: usize = 200;

/// Minimum batch size
pub const MIN_BATCH_SIZE: usize = 10;

/// Maximum batch size (safety limit)
pub const MAX_BATCH_SIZE: usize = 500;

/// Default maximum inputs per request
pub const DEFAULT_MAX_INPUTS: usize = 200;

/// Default maximum outputs per request
pub const DEFAULT_MAX_OUTPUTS: usize = 200;

// =============================================================================
// Mint Limits
// =============================================================================

/// Mint operation limits
#[derive(Debug, Clone, Copy)]
pub struct MintLimits {
    /// Maximum inputs per request
    pub max_inputs: usize,
    /// Maximum outputs per request
    pub max_outputs: usize,
    /// Maximum amount per mint operation
    pub max_mint_amount: Option<u64>,
    /// Maximum amount per melt operation
    pub max_melt_amount: Option<u64>,
}

impl Default for MintLimits {
    fn default() -> Self {
        Self {
            max_inputs: DEFAULT_MAX_INPUTS,
            max_outputs: DEFAULT_MAX_OUTPUTS,
            max_mint_amount: None,
            max_melt_amount: None,
        }
    }
}

/// Cache for mint limits
static LIMITS_CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<String, MintLimits>>> =
    std::sync::OnceLock::new();

fn get_limits_cache() -> &'static std::sync::Mutex<HashMap<String, MintLimits>> {
    LIMITS_CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

// =============================================================================
// Limit Fetching
// =============================================================================

/// Fetch and cache mint limits
pub async fn fetch_mint_limits(mint_url: &str) -> Result<MintLimits, String> {
    // Check cache first
    if let Ok(cache) = get_limits_cache().lock() {
        if let Some(limits) = cache.get(mint_url) {
            return Ok(*limits);
        }
    }

    // Fetch from mint
    let caps = get_mint_capabilities(mint_url).await?;

    let limits = MintLimits {
        max_inputs: caps.max_inputs.unwrap_or(DEFAULT_MAX_INPUTS),
        max_outputs: caps.max_outputs.unwrap_or(DEFAULT_MAX_OUTPUTS),
        max_mint_amount: caps.max_mint_amount,
        max_melt_amount: caps.max_melt_amount,
    };

    // Cache the result
    if let Ok(mut cache) = get_limits_cache().lock() {
        cache.insert(mint_url.to_string(), limits);
    }

    Ok(limits)
}

/// Get cached limits or defaults
pub fn get_cached_limits(mint_url: &str) -> MintLimits {
    if let Ok(cache) = get_limits_cache().lock() {
        if let Some(limits) = cache.get(mint_url) {
            return *limits;
        }
    }
    MintLimits::default()
}

/// Clear limits cache for a mint
pub fn clear_limits_cache(mint_url: &str) {
    if let Ok(mut cache) = get_limits_cache().lock() {
        cache.remove(mint_url);
    }
}

// =============================================================================
// Batch Size Calculation
// =============================================================================

/// Calculate optimal batch size for a mint
pub async fn get_optimal_batch_size(mint_url: &str) -> usize {
    match fetch_mint_limits(mint_url).await {
        Ok(limits) => {
            // Use 90% of max inputs to leave room for edge cases
            let optimal = (limits.max_inputs * 9) / 10;
            optimal.clamp(MIN_BATCH_SIZE, MAX_BATCH_SIZE)
        }
        Err(_) => DEFAULT_BATCH_SIZE,
    }
}

/// Get batch size from cached limits (sync)
pub fn get_batch_size(mint_url: &str) -> usize {
    let limits = get_cached_limits(mint_url);
    let optimal = (limits.max_inputs * 9) / 10;
    optimal.clamp(MIN_BATCH_SIZE, MAX_BATCH_SIZE)
}

// =============================================================================
// Proof Batching
// =============================================================================

/// Batch proofs for processing
pub fn batch_proofs(proofs: Vec<ProofData>, batch_size: usize) -> Vec<Vec<ProofData>> {
    proofs.chunks(batch_size).map(|c| c.to_vec()).collect()
}

/// Batch proofs with mint-specific sizing
pub fn batch_proofs_for_mint(mint_url: &str, proofs: Vec<ProofData>) -> Vec<Vec<ProofData>> {
    let batch_size = get_batch_size(mint_url);
    batch_proofs(proofs, batch_size)
}

/// Batch proofs with adaptive sizing (async)
pub async fn batch_proofs_adaptive(
    mint_url: &str,
    proofs: Vec<ProofData>,
) -> Vec<Vec<ProofData>> {
    let batch_size = get_optimal_batch_size(mint_url).await;
    batch_proofs(proofs, batch_size)
}

// =============================================================================
// Amount Batching
// =============================================================================

/// Split amount into batches respecting mint limits
pub async fn batch_amount(mint_url: &str, total_amount: u64) -> Vec<u64> {
    let limits = fetch_mint_limits(mint_url).await.unwrap_or_default();

    let max_amount = limits.max_mint_amount.unwrap_or(u64::MAX);

    if total_amount <= max_amount {
        return vec![total_amount];
    }

    // Split into multiple operations
    let mut batches = Vec::new();
    let mut remaining = total_amount;

    while remaining > 0 {
        let batch_amount = remaining.min(max_amount);
        batches.push(batch_amount);
        remaining -= batch_amount;
    }

    batches
}

// =============================================================================
// Pagination Helpers
// =============================================================================

/// Pagination state for iterating through proofs
#[derive(Debug, Clone)]
pub struct ProofPaginator {
    /// All proofs
    proofs: Vec<ProofData>,
    /// Current position
    position: usize,
    /// Batch size
    batch_size: usize,
}

impl ProofPaginator {
    /// Create new paginator
    pub fn new(proofs: Vec<ProofData>, batch_size: usize) -> Self {
        Self {
            proofs,
            position: 0,
            batch_size,
        }
    }

    /// Create paginator with mint-specific batch size
    pub fn for_mint(mint_url: &str, proofs: Vec<ProofData>) -> Self {
        let batch_size = get_batch_size(mint_url);
        Self::new(proofs, batch_size)
    }

    /// Get next batch
    pub fn next_batch(&mut self) -> Option<Vec<ProofData>> {
        if self.position >= self.proofs.len() {
            return None;
        }

        let end = (self.position + self.batch_size).min(self.proofs.len());
        let batch = self.proofs[self.position..end].to_vec();
        self.position = end;

        Some(batch)
    }

    /// Check if more batches are available
    pub fn has_more(&self) -> bool {
        self.position < self.proofs.len()
    }

    /// Get total proof count
    pub fn total_count(&self) -> usize {
        self.proofs.len()
    }

    /// Get number of batches
    pub fn batch_count(&self) -> usize {
        (self.proofs.len() + self.batch_size - 1) / self.batch_size
    }

    /// Get current batch index
    pub fn current_batch(&self) -> usize {
        self.position / self.batch_size
    }

    /// Reset to beginning
    pub fn reset(&mut self) {
        self.position = 0;
    }
}

impl Iterator for ProofPaginator {
    type Item = Vec<ProofData>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_batch()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_proofs() {
        let proofs = vec![
            ProofData {
                id: "1".to_string(),
                amount: 1,
                secret: "s1".to_string(),
                c: "c1".to_string(),
                witness: None,
                dleq: None,
                state: Default::default(),
                transaction_id: None,
            },
            ProofData {
                id: "2".to_string(),
                amount: 2,
                secret: "s2".to_string(),
                c: "c2".to_string(),
                witness: None,
                dleq: None,
                state: Default::default(),
                transaction_id: None,
            },
            ProofData {
                id: "3".to_string(),
                amount: 4,
                secret: "s3".to_string(),
                c: "c3".to_string(),
                witness: None,
                dleq: None,
                state: Default::default(),
                transaction_id: None,
            },
        ];

        let batches = batch_proofs(proofs, 2);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 2);
        assert_eq!(batches[1].len(), 1);
    }

    #[test]
    fn test_paginator() {
        let proofs: Vec<ProofData> = (0..5)
            .map(|i| ProofData {
                id: i.to_string(),
                amount: i as u64,
                secret: format!("s{}", i),
                c: format!("c{}", i),
                witness: None,
                dleq: None,
                state: Default::default(),
                transaction_id: None,
            })
            .collect();

        let mut paginator = ProofPaginator::new(proofs, 2);

        assert_eq!(paginator.batch_count(), 3);
        assert!(paginator.has_more());

        let batch1 = paginator.next_batch().unwrap();
        assert_eq!(batch1.len(), 2);

        let batch2 = paginator.next_batch().unwrap();
        assert_eq!(batch2.len(), 2);

        let batch3 = paginator.next_batch().unwrap();
        assert_eq!(batch3.len(), 1);

        assert!(paginator.next_batch().is_none());
    }
}
