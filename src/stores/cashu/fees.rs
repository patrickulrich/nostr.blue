//! Fee Estimation
//!
//! Comprehensive fee estimation including P2PK witness overhead.
//! Provides accurate fee quotes for all transaction types.
#![allow(dead_code)]
use super::internal::get_or_create_wallet;
use super::signals::WALLET_TOKENS;
use super::types::{ProofData, WalletTokensStoreStoreExt};
use super::utils::mint_matches;
use dioxus::prelude::ReadableExt;
/// Estimated witness overhead for P2PK proofs (bytes)
/// Includes signature + public key serialization
pub const P2PK_WITNESS_OVERHEAD: usize = 128;
/// Estimated witness overhead for HTLC proofs (bytes)
/// Includes preimage + signature
pub const HTLC_WITNESS_OVERHEAD: usize = 160;
/// Estimated witness overhead for multisig (per signature)
pub const MULTISIG_SIGNATURE_OVERHEAD: usize = 64;
/// Base proof size (without witness)
pub const BASE_PROOF_SIZE: usize = 200;
/// Fee estimation result
#[derive(Debug, Clone, Default)]
pub struct FeeEstimate {
    /// Base fee from mint (input fee per proof)
    pub base_fee: u64,
    /// P2PK witness overhead fee
    pub witness_fee: u64,
    /// Total estimated fee
    pub total_fee: u64,
    /// Number of proofs involved
    pub proof_count: usize,
    /// Fee per proof (ppk)
    pub fee_ppk: u64,
}
impl FeeEstimate {
    /// Create from components
    pub fn new(base_fee: u64, witness_fee: u64, proof_count: usize, fee_ppk: u64) -> Self {
        Self {
            base_fee,
            witness_fee,
            total_fee: base_fee.saturating_add(witness_fee),
            proof_count,
            fee_ppk,
        }
    }
}
/// P2PK proof complexity for fee estimation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum P2pkComplexity {
    /// No P2PK (standard proof)
    #[default]
    None,
    /// Single signature P2PK
    SingleSig,
    /// Multi-signature P2PK
    MultiSig { required: u8, total: u8 },
    /// HTLC (hash time locked)
    Htlc,
    /// HTLC with P2PK
    HtlcP2pk,
}
impl P2pkComplexity {
    /// Get estimated witness size in bytes
    pub fn witness_size(&self) -> usize {
        match self {
            Self::None => 0,
            Self::SingleSig => P2PK_WITNESS_OVERHEAD,
            Self::MultiSig { required, .. } => {
                let extra_sigs = (*required as usize).saturating_sub(1);
                P2PK_WITNESS_OVERHEAD + extra_sigs * MULTISIG_SIGNATURE_OVERHEAD
            }
            Self::Htlc => HTLC_WITNESS_OVERHEAD,
            Self::HtlcP2pk => HTLC_WITNESS_OVERHEAD + P2PK_WITNESS_OVERHEAD,
        }
    }
}
/// Get fee per proof (ppk) for a mint's active keyset
pub async fn get_mint_fee_ppk(mint_url: &str) -> Result<u64, String> {
    let wallet = get_or_create_wallet(mint_url).await?;
    let keyset = wallet
        .get_active_keyset()
        .await
        .map_err(|e| format!("Failed to get active keyset: {}", e))?;
    Ok(keyset.input_fee_ppk)
}
/// Calculate fee for a number of proofs
pub fn calculate_proof_fee(proof_count: usize, fee_ppk: u64) -> u64 {
    let base = (proof_count as u64).saturating_mul(fee_ppk);
    base.saturating_add(999) / 1000
}
/// Estimate fee for a simple send (no P2PK)
pub async fn estimate_simple_send_fee(mint_url: &str, amount: u64) -> Result<FeeEstimate, String> {
    let fee_ppk = get_mint_fee_ppk(mint_url).await?;
    let proof_count = (amount as u128).count_ones() as usize;
    let base_fee = calculate_proof_fee(proof_count, fee_ppk);
    Ok(FeeEstimate::new(base_fee, 0, proof_count, fee_ppk))
}
/// Estimate fee for P2PK send with witness overhead
pub async fn estimate_p2pk_send_fee(
    mint_url: &str,
    amount: u64,
    complexity: P2pkComplexity,
) -> Result<FeeEstimate, String> {
    let fee_ppk = get_mint_fee_ppk(mint_url).await?;
    let proof_count = (amount as u128).count_ones() as usize;
    let base_fee = calculate_proof_fee(proof_count, fee_ppk);
    let witness_size = complexity.witness_size();
    let witness_fee = if witness_size > 0 {
        let fee_per_proof = witness_size.div_ceil(100).max(1) as u64;
        fee_per_proof * proof_count as u64
    } else {
        0
    };
    Ok(FeeEstimate::new(
        base_fee,
        witness_fee,
        proof_count,
        fee_ppk,
    ))
}
/// Estimate fee for multisig P2PK
pub async fn estimate_multisig_fee(
    mint_url: &str,
    amount: u64,
    required_sigs: u8,
    total_signers: u8,
) -> Result<FeeEstimate, String> {
    let complexity = P2pkComplexity::MultiSig {
        required: required_sigs,
        total: total_signers,
    };
    estimate_p2pk_send_fee(mint_url, amount, complexity).await
}
/// Estimate fee for HTLC send
pub async fn estimate_htlc_fee(mint_url: &str, amount: u64) -> Result<FeeEstimate, String> {
    estimate_p2pk_send_fee(mint_url, amount, P2pkComplexity::Htlc).await
}
/// Estimate fee for receiving P2PK tokens
///
/// When receiving P2PK tokens, we need to swap them to unlock.
/// This incurs fees based on the locked proof count.
pub async fn estimate_p2pk_receive_fee(
    mint_url: &str,
    proofs: &[ProofData],
) -> Result<FeeEstimate, String> {
    let fee_ppk = get_mint_fee_ppk(mint_url).await?;
    let proof_count = proofs.len();
    let base_fee = calculate_proof_fee(proof_count, fee_ppk);
    let has_witness = proofs.iter().any(|p| p.witness.is_some());
    let witness_fee = if has_witness {
        let witness_size = P2PK_WITNESS_OVERHEAD;
        witness_size.div_ceil(100).max(1) as u64 * proof_count as u64
    } else {
        0
    };
    Ok(FeeEstimate::new(
        base_fee,
        witness_fee,
        proof_count,
        fee_ppk,
    ))
}
/// Estimate fee for a swap operation
pub async fn estimate_swap_fee(
    mint_url: &str,
    input_proof_count: usize,
    output_amount: u64,
) -> Result<FeeEstimate, String> {
    let fee_ppk = get_mint_fee_ppk(mint_url).await?;
    let base_fee = calculate_proof_fee(input_proof_count, fee_ppk);
    let _output_count = (output_amount as u128).count_ones() as usize;
    Ok(FeeEstimate {
        base_fee,
        witness_fee: 0,
        total_fee: base_fee,
        proof_count: input_proof_count,
        fee_ppk,
    })
}
/// Compare fees between mints for an operation
pub async fn compare_mint_fees(
    mint_urls: &[String],
    amount: u64,
) -> Result<Vec<(String, FeeEstimate)>, String> {
    let mut results = Vec::new();
    for mint_url in mint_urls {
        match estimate_simple_send_fee(mint_url, amount).await {
            Ok(estimate) => results.push((mint_url.clone(), estimate)),
            Err(e) => {
                log::warn!("Failed to estimate fee for {}: {}", mint_url, e);
            }
        }
    }
    results.sort_by_key(|(_, est)| est.total_fee);
    Ok(results)
}
/// Find mint with lowest fee for an amount
pub async fn find_cheapest_mint(
    mint_urls: &[String],
    amount: u64,
) -> Result<Option<(String, FeeEstimate)>, String> {
    let comparisons = compare_mint_fees(mint_urls, amount).await?;
    Ok(comparisons.into_iter().next())
}
/// Get fee summary for a mint
pub async fn get_mint_fee_summary(mint_url: &str) -> Result<MintFeeSummary, String> {
    let fee_ppk = get_mint_fee_ppk(mint_url).await?;
    let store = WALLET_TOKENS();
    let data = store.data();
    let tokens = data.read();
    let proof_count: usize = tokens
        .iter()
        .filter(|t| mint_matches(&t.mint, mint_url))
        .map(|t| t.proofs.len())
        .fold(0usize, |acc, count| acc.saturating_add(count));
    Ok(MintFeeSummary {
        mint_url: mint_url.to_string(),
        fee_ppk,
        current_proofs: proof_count,
        estimated_spend_all_fee: calculate_proof_fee(proof_count, fee_ppk),
    })
}
/// Mint fee summary
#[derive(Debug, Clone)]
pub struct MintFeeSummary {
    pub mint_url: String,
    pub fee_ppk: u64,
    pub current_proofs: usize,
    pub estimated_spend_all_fee: u64,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_calculate_proof_fee() {
        assert_eq!(calculate_proof_fee(10, 100), 1);
        assert_eq!(calculate_proof_fee(5, 1000), 5);
        assert_eq!(calculate_proof_fee(1, 100), 1);
    }
    #[test]
    fn test_p2pk_witness_size() {
        assert_eq!(P2pkComplexity::None.witness_size(), 0);
        assert_eq!(
            P2pkComplexity::SingleSig.witness_size(),
            P2PK_WITNESS_OVERHEAD
        );
        assert_eq!(
            P2pkComplexity::MultiSig {
                required: 2,
                total: 3,
            }
            .witness_size(),
            P2PK_WITNESS_OVERHEAD + MULTISIG_SIGNATURE_OVERHEAD,
        );
    }
}
