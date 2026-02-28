//! Cashu wallet utility functions
#![allow(dead_code)]
use nostr_sdk::types::url::Url;
/// Normalize a mint URL to prevent duplicates like "mint.coinos.io" vs "mint.coinos.io/"
/// This should be called when storing or comparing mint URLs.
pub fn normalize_mint_url(url: &str) -> String {
    let mut normalized = url.trim().to_string();
    while normalized.ends_with('/') {
        normalized.pop();
    }
    if !normalized.starts_with("http://") && !normalized.starts_with("https://") {
        normalized = format!("https://{}", normalized);
    }
    if let Ok(parsed) = Url::parse(&normalized) {
        if let Some(host) = parsed.host_str() {
            let scheme = parsed.scheme();
            let port_str = parsed.port().map(|p| format!(":{}", p)).unwrap_or_default();
            let path = parsed.path();
            let path_str = if path == "/" { "" } else { path };
            normalized = format!("{}://{}{}{}", scheme, host, port_str, path_str);
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
    crate::platform::timestamp::now_secs()
}
/// Get current timestamp using chrono (for non-WASM contexts)
pub fn chrono_now_secs() -> u64 {
    chrono::Utc::now().timestamp() as u64
}
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
    use super::signals::MAX_SYNC_INPUT_SIZE;
    use cdk::nuts::State;
    if proofs.is_empty() {
        return Ok(ProofValidationResult::default());
    }
    log::debug!(
        "Validating {} proofs in batches of {}", proofs.len(), MAX_SYNC_INPUT_SIZE
    );
    let mut result = ProofValidationResult::default();
    let mut valid_proofs = Vec::with_capacity(proofs.len());
    for (batch_idx, batch) in proofs.chunks(MAX_SYNC_INPUT_SIZE).enumerate() {
        log::debug!("Validating batch {} ({} proofs)", batch_idx + 1, batch.len());
        let states = match wallet.check_proofs_spent(batch.to_vec()).await {
            Ok(s) => s,
            Err(e) => {
                log::warn!(
                    "Failed to check proof states for batch {}: {}", batch_idx, e
                );
                valid_proofs.extend(batch.iter().cloned());
                continue;
            }
        };
        for (proof, state_info) in batch.iter().zip(states.iter()) {
            match state_info.state {
                State::Spent => {
                    result.spent_count += 1;
                    result.spent_sats += u64::from(proof.amount);
                    log::debug!(
                        "Proof {} is spent ({} sats)", & proof.secret.to_string() [..8],
                        u64::from(proof.amount)
                    );
                }
                State::Pending => {
                    result.pending_count += 1;
                    valid_proofs.push(proof.clone());
                    log::debug!(
                        "Proof {} is pending at mint", & proof.secret.to_string() [..8]
                    );
                }
                State::Unspent => {
                    valid_proofs.push(proof.clone());
                }
                _ => {
                    valid_proofs.push(proof.clone());
                }
            }
        }
    }
    result.valid_proofs = valid_proofs;
    if result.spent_count > 0 {
        log::info!(
            "Proof validation complete: {} valid, {} spent ({} sats removed), {} pending",
            result.valid_proofs.len(), result.spent_count, result.spent_sats, result
            .pending_count
        );
    } else {
        log::debug!("All {} proofs validated as unspent", result.valid_proofs.len());
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
/// Check if error indicates counter/signature desync
///
/// These errors mean the mint has already seen these blind signatures.
/// This typically happens when:
/// - App crashed after sending blinded messages but before receiving proofs
/// - Counter got desynced between CDK's IndexedDB and mint's state
///
/// Solution: Increment counter and retry with fresh signatures.
pub fn should_heal_outputs_error(error: &str) -> bool {
    let lower = error.to_lowercase();
    lower.contains("already signed") || lower.contains("duplicate key")
        || lower.contains("outputs have already been signed")
        || lower.contains("blind signature already exists")
        || lower.contains("blinded message already used")
        || lower.contains("already exists in database")
}
/// Counter healing retry loop result
#[derive(Debug)]
pub struct CounterHealResult<T> {
    pub result: T,
    pub heal_attempts: u32,
}
/// Execute an operation with counter healing retry
///
/// If the operation fails with a signature desync error, this will:
/// 1. Increment the keyset counter by increasing amounts
/// 2. Retry the operation with fresh signatures
/// 3. Give up after MAX_COUNTER_HEAL_ATTEMPTS
///
/// SAFETY: Limited to 3 retries to prevent infinite loops (Risk 5)
pub async fn with_counter_healing<F, Fut, T, E>(
    wallet: &cdk::Wallet,
    keyset_id: &cdk::nuts::Id,
    operation: F,
) -> Result<CounterHealResult<T>, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    use super::types::{COUNTER_HEAL_INCREMENTS, MAX_COUNTER_HEAL_ATTEMPTS};
    let mut heal_attempt = 0u32;
    loop {
        match operation().await {
            Ok(result) => {
                if heal_attempt > 0 {
                    log::info!(
                        "Counter healing succeeded after {} attempt(s)", heal_attempt
                    );
                }
                return Ok(CounterHealResult {
                    result,
                    heal_attempts: heal_attempt,
                });
            }
            Err(e) => {
                let error_str = e.to_string();
                if should_heal_outputs_error(&error_str)
                    && heal_attempt < MAX_COUNTER_HEAL_ATTEMPTS
                {
                    let increment = match COUNTER_HEAL_INCREMENTS
                        .get(heal_attempt as usize)
                    {
                        Some(&inc) => inc,
                        None => {
                            log::error!(
                                "Counter heal increment not found for attempt {}",
                                heal_attempt
                            );
                            return Err(e);
                        }
                    };
                    log::warn!(
                        "Counter desync detected (attempt {}/{}), incrementing keyset counter by {}",
                        heal_attempt + 1, MAX_COUNTER_HEAL_ATTEMPTS, increment
                    );
                    if let Err(incr_err) = wallet
                        .localstore
                        .increment_keyset_counter(keyset_id, increment)
                        .await
                    {
                        log::error!(
                            "COUNTER_INCREMENT_FAILED: keyset={}, increment={}, counter_error='{}', triggering_error='{}'",
                            keyset_id, increment, incr_err, e
                        );
                        return Err(e);
                    }
                    heal_attempt += 1;
                    crate::platform::timer::sleep_ms(500).await;
                } else {
                    return Err(e);
                }
            }
        }
    }
}
/// Sanitize a Cashu token string by removing all whitespace
/// and validating the format prefix.
///
/// Returns the sanitized token string if valid, or an error message if invalid.
pub fn sanitize_and_validate_token(token_string: &str) -> Result<String, String> {
    let sanitized: String = token_string
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if sanitized.is_empty() {
        return Err("Token string is empty".to_string());
    }
    if !sanitized.starts_with("cashuA") && !sanitized.starts_with("cashuB") {
        return Err(
            format!(
                "Invalid token format. Must start with 'cashuA' or 'cashuB', got: '{}'",
                sanitized.chars().take(10).collect::<String>(),
            ),
        );
    }
    Ok(sanitized)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_normalize_mint_url() {
        assert_eq!(normalize_mint_url("mint.example.com"), "https://mint.example.com");
        assert_eq!(
            normalize_mint_url("https://mint.example.com/"),
            "https://mint.example.com",
        );
        assert_eq!(
            normalize_mint_url("https://MINT.Example.COM"),
            "https://mint.example.com",
        );
        assert_eq!(
            normalize_mint_url("  https://mint.example.com/  "),
            "https://mint.example.com",
        );
    }
    #[test]
    fn test_mint_matches() {
        assert!(mint_matches("https://mint.example.com/", "https://mint.example.com"));
        assert!(mint_matches("mint.example.com", "https://mint.example.com"));
        assert!(!mint_matches("https://other.mint.com", "https://mint.example.com"));
    }
    #[test]
    fn test_sanitize_and_validate_token() {
        assert_eq!(
            sanitize_and_validate_token("cashuAtoken123"),
            Ok("cashuAtoken123".to_string()),
        );
        assert_eq!(
            sanitize_and_validate_token("cashuBtoken456"),
            Ok("cashuBtoken456".to_string()),
        );
        assert_eq!(
            sanitize_and_validate_token("  cashuA token 123  "),
            Ok("cashuAtoken123".to_string()),
        );
        assert_eq!(
            sanitize_and_validate_token("cashuB\ntoken\t456"),
            Ok("cashuBtoken456".to_string()),
        );
        assert!(sanitize_and_validate_token("").is_err());
        assert!(sanitize_and_validate_token("   ").is_err());
        assert!(sanitize_and_validate_token("invalidtoken").is_err());
        assert!(sanitize_and_validate_token("cashu123").is_err());
    }
}
