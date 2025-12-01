//! Direct Swap Operations
//!
//! Exposes CDK's wallet.swap() for advanced use cases like:
//! - Keyset migration
//! - Proof consolidation
//! - Denomination optimization
//! - Atomic swaps between keysets

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use cdk::nuts::SpendingConditions;
use dioxus::prelude::ReadableExt;

use super::denomination::DenominationStrategy;
use super::internal::get_or_create_wallet;
use super::proofs::{cdk_proof_to_proof_data, proof_data_to_cdk_proof};
use super::signals::{try_acquire_mint_lock, WALLET_TOKENS};
use super::types::{ProofData, WalletTokensStoreStoreExt};
use super::utils::mint_matches;

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
            cdk_proofs.into(),
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

/// Swap all proofs for a mint to optimize denominations
///
/// Acquires mint lock and handles state updates.
pub async fn swap_optimize_denominations(
    mint_url: &str,
    strategy: DenominationStrategy,
) -> Result<SwapResult, String> {
    // Acquire lock
    let _lock = try_acquire_mint_lock(mint_url)
        .ok_or_else(|| format!("Another operation in progress for {}", mint_url))?;

    // Get all proofs for this mint
    let all_proofs = get_proofs_for_mint(mint_url)?;

    if all_proofs.is_empty() {
        return Err("No proofs found for this mint".to_string());
    }

    let options = SwapOptions::all().with_denomination(strategy);

    execute_swap(mint_url, all_proofs, options).await
}

/// Swap proofs to add spending conditions (P2PK lock)
pub async fn swap_to_locked(
    mint_url: &str,
    amount: u64,
    conditions: SpendingConditions,
) -> Result<SwapResult, String> {
    let _lock = try_acquire_mint_lock(mint_url)
        .ok_or_else(|| format!("Another operation in progress for {}", mint_url))?;

    let all_proofs = get_proofs_for_mint(mint_url)?;

    if all_proofs.is_empty() {
        return Err("No proofs found for this mint".to_string());
    }

    let total: u64 = all_proofs
        .iter()
        .map(|p| p.amount)
        .fold(0u64, |acc, amt| acc.saturating_add(amt));
    if total < amount {
        return Err(format!(
            "Insufficient funds: have {} sats, need {}",
            total, amount
        ));
    }

    let options = SwapOptions::amount(amount)
        .with_conditions(conditions)
        .with_include_fee(true);

    execute_swap(mint_url, all_proofs, options).await
}

/// Swap all proofs to fresh ones (privacy enhancement)
pub async fn swap_refresh(mint_url: &str) -> Result<SwapResult, String> {
    let _lock = try_acquire_mint_lock(mint_url)
        .ok_or_else(|| format!("Another operation in progress for {}", mint_url))?;

    let all_proofs = get_proofs_for_mint(mint_url)?;

    if all_proofs.is_empty() {
        return Err("No proofs found for this mint".to_string());
    }

    let options = SwapOptions::all().with_denomination(DenominationStrategy::PowerOfTwo);

    execute_swap(mint_url, all_proofs, options).await
}

// =============================================================================
// Helpers
// =============================================================================

/// Get all proofs for a mint from local storage
fn get_proofs_for_mint(mint_url: &str) -> Result<Vec<ProofData>, String> {
    let store = WALLET_TOKENS();
    let data = store.data();
    let tokens = data.read();

    let proofs: Vec<ProofData> = tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url))
        .flat_map(|t| t.proofs.clone())
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
