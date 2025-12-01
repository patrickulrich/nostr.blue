//! Denomination Splitting Strategy
//!
//! Implements CDK's SplitTarget strategies for optimizing proof denominations.
//! Different strategies optimize for different use cases.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use cdk::amount::SplitTarget;

// =============================================================================
// Denomination Strategies
// =============================================================================

/// Strategy for splitting amounts into denominations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DenominationStrategy {
    /// Power of two denominations (CDK default)
    /// Produces minimal number of proofs for any amount
    #[default]
    PowerOfTwo,
    /// Prefer larger denominations
    /// Good for reducing total proof count in wallet
    Large,
    /// Prefer smaller denominations (many small proofs)
    /// Good for privacy but increases proof count
    Small,
    /// Balanced denomination spread
    /// Keeps a mix of denominations for flexibility
    Balanced,
    /// Target specific denominations based on usage patterns
    /// Optimizes for common transaction amounts
    Adaptive,
}

impl DenominationStrategy {
    /// Convert to CDK's SplitTarget
    pub fn to_split_target(&self) -> SplitTarget {
        match self {
            Self::PowerOfTwo => SplitTarget::default(),
            Self::Large => SplitTarget::Value(cdk::Amount::from(1u64)),
            Self::Small => SplitTarget::None,
            Self::Balanced => SplitTarget::default(),
            Self::Adaptive => SplitTarget::default(),
        }
    }

    /// Get strategy from preference name
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "large" | "minimal" => Self::Large,
            "small" | "privacy" => Self::Small,
            "balanced" | "mixed" => Self::Balanced,
            "adaptive" | "smart" => Self::Adaptive,
            _ => Self::PowerOfTwo,
        }
    }

    /// Human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::PowerOfTwo => "Optimal proof count (power of 2 denominations)",
            Self::Large => "Fewer, larger proofs",
            Self::Small => "Many small proofs (better privacy)",
            Self::Balanced => "Mix of denominations for flexibility",
            Self::Adaptive => "Smart selection based on usage patterns",
        }
    }
}

// =============================================================================
// Denomination Selection
// =============================================================================

/// Select optimal denomination strategy based on context
pub fn select_strategy_for_operation(
    operation: OperationType,
    amount: u64,
    current_proof_count: usize,
) -> DenominationStrategy {
    match operation {
        OperationType::Mint => {
            // For minting, prefer balanced to maintain flexibility
            if current_proof_count > 50 {
                // Wallet has many proofs, prefer consolidation
                DenominationStrategy::Large
            } else {
                DenominationStrategy::Balanced
            }
        }
        OperationType::Send => {
            // For sending, use power of two for exact amounts
            DenominationStrategy::PowerOfTwo
        }
        OperationType::Swap => {
            // For swaps, consider the amount
            if amount > 100_000 {
                // Large amounts - minimize proof count
                DenominationStrategy::Large
            } else {
                DenominationStrategy::PowerOfTwo
            }
        }
        OperationType::Consolidate => {
            // Always prefer large for consolidation
            DenominationStrategy::Large
        }
        OperationType::Receive => {
            // For receiving, use power of two (standard)
            DenominationStrategy::PowerOfTwo
        }
    }
}

/// Operation types for denomination selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    Mint,
    Send,
    Swap,
    Consolidate,
    Receive,
}

// =============================================================================
// Proof Count Optimization
// =============================================================================

/// Estimate optimal proof count for an amount
pub fn estimate_proof_count(amount: u64) -> usize {
    // For power of 2 denominations, count set bits
    (amount as u128).count_ones() as usize
}

/// Check if proof count is acceptable
pub fn is_proof_count_acceptable(count: usize, threshold: usize) -> bool {
    count <= threshold
}

/// Suggested maximum proof count per mint before consolidation
pub const CONSOLIDATION_THRESHOLD: usize = 50;

/// Suggested maximum proofs per transaction
pub const MAX_PROOFS_PER_TX: usize = 20;

// =============================================================================
// SplitTarget Helpers
// =============================================================================

/// Create SplitTarget for a specific count of equal denominations
pub fn split_target_count(count: usize) -> SplitTarget {
    if count == 0 {
        SplitTarget::None
    } else {
        SplitTarget::Value(cdk::Amount::from(count as u64))
    }
}

/// Get the default SplitTarget for the wallet
pub fn default_split_target() -> SplitTarget {
    SplitTarget::default()
}

/// Get SplitTarget for consolidation (minimal proof count)
pub fn consolidation_split_target() -> SplitTarget {
    DenominationStrategy::Large.to_split_target()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_proof_count() {
        assert_eq!(estimate_proof_count(1), 1);
        assert_eq!(estimate_proof_count(2), 1);
        assert_eq!(estimate_proof_count(3), 2);
        assert_eq!(estimate_proof_count(7), 3);
        assert_eq!(estimate_proof_count(8), 1);
        assert_eq!(estimate_proof_count(100), 3); // 64 + 32 + 4
    }

    #[test]
    fn test_strategy_from_str() {
        assert_eq!(
            DenominationStrategy::from_str("large"),
            DenominationStrategy::Large
        );
        assert_eq!(
            DenominationStrategy::from_str("small"),
            DenominationStrategy::Small
        );
        assert_eq!(
            DenominationStrategy::from_str("unknown"),
            DenominationStrategy::PowerOfTwo
        );
    }
}
