//! Cashu wallet utility functions

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use nostr_sdk::types::url::Url;

/// Normalize a mint URL to prevent duplicates like "mint.coinos.io" vs "mint.coinos.io/"
/// This should be called when storing or comparing mint URLs.
pub fn normalize_mint_url(url: &str) -> String {
    let mut normalized = url.trim().to_string();

    // Remove trailing slashes
    while normalized.ends_with('/') {
        normalized.pop();
    }

    // Ensure https:// prefix if no scheme
    if !normalized.starts_with("http://") && !normalized.starts_with("https://") {
        normalized = format!("https://{}", normalized);
    }

    // Lowercase the host portion for consistency
    if let Ok(parsed) = Url::parse(&normalized) {
        if let Some(host) = parsed.host_str() {
            let lowercase_host = host.to_lowercase();
            normalized = normalized.replacen(host, &lowercase_host, 1);
        }
    }

    normalized
}

/// Check if a mint URL matches a normalized mint URL
/// Used for filtering tokens where stored URLs might not be normalized
#[inline]
pub fn mint_matches(stored_mint: &str, normalized_mint: &str) -> bool {
    normalize_mint_url(stored_mint) == normalized_mint
}

/// Get current timestamp in seconds
pub fn now_secs() -> u64 {
    js_sys::Date::now() as u64 / 1000
}

/// Get current timestamp using chrono (for non-WASM contexts)
pub fn chrono_now_secs() -> u64 {
    chrono::Utc::now().timestamp() as u64
}

// =============================================================================
// Batched Proof Validation
// =============================================================================

/// Result of validating proofs with a mint
#[derive(Clone, Debug, Default)]
pub struct ProofValidationResult {
    /// Proofs that are still unspent and valid
    pub valid_proofs: Vec<cdk::nuts::Proof>,
    /// Number of spent proofs removed
    pub spent_count: usize,
    /// Number of pending proofs found
    pub pending_count: usize,
    /// Total sats removed (spent proofs)
    pub spent_sats: u64,
}

/// Validate proofs with mint using batch pagination
///
/// Chunks proofs into batches of MAX_SYNC_INPUT_SIZE to avoid mint API limits
/// and timeouts with large wallets. Returns only the proofs that are still valid.
pub async fn validate_proofs_batched(
    wallet: &cdk::Wallet,
    proofs: Vec<cdk::nuts::Proof>,
) -> Result<ProofValidationResult, String> {
    use cdk::nuts::State;
    use super::signals::MAX_SYNC_INPUT_SIZE;

    if proofs.is_empty() {
        return Ok(ProofValidationResult::default());
    }

    log::debug!(
        "Validating {} proofs in batches of {}",
        proofs.len(),
        MAX_SYNC_INPUT_SIZE
    );

    let mut result = ProofValidationResult::default();
    let mut valid_proofs = Vec::with_capacity(proofs.len());

    // Process in batches
    for (batch_idx, batch) in proofs.chunks(MAX_SYNC_INPUT_SIZE).enumerate() {
        log::debug!(
            "Validating batch {} ({} proofs)",
            batch_idx + 1,
            batch.len()
        );

        // Check proof states with mint (NUT-07)
        let states = match wallet.check_proofs_spent(batch.to_vec().into()).await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to check proof states for batch {}: {}", batch_idx, e);
                // On error, assume all proofs in batch are valid (fail-safe)
                valid_proofs.extend(batch.iter().cloned());
                continue;
            }
        };

        // Filter proofs by state
        for (proof, state_info) in batch.iter().zip(states.iter()) {
            match state_info.state {
                State::Spent => {
                    result.spent_count += 1;
                    result.spent_sats += u64::from(proof.amount);
                    log::debug!(
                        "Proof {} is spent ({} sats)",
                        &proof.secret.to_string()[..8],
                        u64::from(proof.amount)
                    );
                }
                State::Pending => {
                    result.pending_count += 1;
                    // Keep pending proofs - they might still complete
                    valid_proofs.push(proof.clone());
                    log::debug!(
                        "Proof {} is pending at mint",
                        &proof.secret.to_string()[..8]
                    );
                }
                State::Unspent => {
                    valid_proofs.push(proof.clone());
                }
                _ => {
                    // Unknown state - keep the proof
                    valid_proofs.push(proof.clone());
                }
            }
        }
    }

    result.valid_proofs = valid_proofs;

    if result.spent_count > 0 {
        log::info!(
            "Proof validation complete: {} valid, {} spent ({} sats removed), {} pending",
            result.valid_proofs.len(),
            result.spent_count,
            result.spent_sats,
            result.pending_count
        );
    } else {
        log::debug!(
            "All {} proofs validated as unspent",
            result.valid_proofs.len()
        );
    }

    Ok(result)
}

/// Validate and filter proofs, returning only spendable ones
///
/// Convenience wrapper that just returns valid proofs without the stats.
pub async fn validate_and_filter_proofs(
    wallet: &cdk::Wallet,
    proofs: Vec<cdk::nuts::Proof>,
) -> Result<Vec<cdk::nuts::Proof>, String> {
    let result = validate_proofs_batched(wallet, proofs).await?;
    Ok(result.valid_proofs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_mint_url() {
        assert_eq!(
            normalize_mint_url("mint.example.com"),
            "https://mint.example.com"
        );
        assert_eq!(
            normalize_mint_url("https://mint.example.com/"),
            "https://mint.example.com"
        );
        assert_eq!(
            normalize_mint_url("https://MINT.Example.COM"),
            "https://mint.example.com"
        );
        assert_eq!(
            normalize_mint_url("  https://mint.example.com/  "),
            "https://mint.example.com"
        );
    }

    #[test]
    fn test_mint_matches() {
        assert!(mint_matches("https://mint.example.com/", "https://mint.example.com"));
        assert!(mint_matches("mint.example.com", "https://mint.example.com"));
        assert!(!mint_matches("https://other.mint.com", "https://mint.example.com"));
    }
}
