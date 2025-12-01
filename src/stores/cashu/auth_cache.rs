//! Blind Auth Token Caching (NUT-22)
//!
//! Caches blind auth tokens per mint to avoid re-authentication.
//! Implements TTL-based expiry and automatic refresh.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use std::collections::HashMap;
use dioxus::prelude::*;

// Use CDK's BlindAuthToken type directly
use cdk_common::BlindAuthToken;

// =============================================================================
// Auth Token Cache
// =============================================================================

/// Cached auth token with metadata
#[derive(Debug, Clone)]
pub struct CachedAuthToken {
    /// The auth token
    pub token: BlindAuthToken,
    /// When the token was cached (Unix timestamp)
    pub cached_at: u64,
    /// Token expiry time (if known)
    pub expires_at: Option<u64>,
    /// Number of times this token has been used
    pub use_count: u32,
}

impl CachedAuthToken {
    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        if let Some(exp) = self.expires_at {
            now_secs() >= exp
        } else {
            false
        }
    }

    /// Check if token is still valid
    pub fn is_valid(&self) -> bool {
        !self.is_expired()
    }
}

/// Cache for blind auth tokens per mint
#[derive(Debug, Clone, Default)]
pub struct BlindAuthCache {
    /// Tokens indexed by mint URL
    tokens: HashMap<String, Vec<CachedAuthToken>>,
    /// Default TTL for cached tokens (seconds)
    pub default_ttl: u64,
}

impl BlindAuthCache {
    /// Create new cache with default TTL
    pub fn new() -> Self {
        Self {
            tokens: HashMap::new(),
            default_ttl: DEFAULT_TOKEN_TTL,
        }
    }

    /// Add a token for a mint
    pub fn add_token(&mut self, mint_url: &str, token: BlindAuthToken, expires_at: Option<u64>) {
        let cached = CachedAuthToken {
            token,
            cached_at: now_secs(),
            expires_at,
            use_count: 0,
        };

        self.tokens
            .entry(mint_url.to_string())
            .or_default()
            .push(cached);
    }

    /// Add multiple tokens for a mint
    pub fn add_tokens(&mut self, mint_url: &str, tokens: Vec<BlindAuthToken>, expires_at: Option<u64>) {
        for token in tokens {
            self.add_token(mint_url, token, expires_at);
        }
    }

    /// Get and consume a token for a mint
    ///
    /// Returns the first valid token and removes it from cache.
    pub fn consume_token(&mut self, mint_url: &str) -> Option<BlindAuthToken> {
        let tokens = self.tokens.get_mut(mint_url)?;

        // Find first valid token
        let pos = tokens.iter().position(|t| t.is_valid())?;

        // Remove and return
        Some(tokens.remove(pos).token)
    }

    /// Peek at available tokens for a mint (doesn't consume)
    pub fn peek_tokens(&self, mint_url: &str) -> Vec<&CachedAuthToken> {
        self.tokens
            .get(mint_url)
            .map(|v| v.iter().filter(|t| t.is_valid()).collect())
            .unwrap_or_default()
    }

    /// Get count of available tokens for a mint
    pub fn available_count(&self, mint_url: &str) -> usize {
        self.tokens
            .get(mint_url)
            .map(|v| v.iter().filter(|t| t.is_valid()).count())
            .unwrap_or(0)
    }

    /// Check if any tokens are available for a mint
    pub fn has_tokens(&self, mint_url: &str) -> bool {
        self.available_count(mint_url) > 0
    }

    /// Remove expired tokens
    pub fn cleanup_expired(&mut self) -> usize {
        let mut removed = 0;

        for tokens in self.tokens.values_mut() {
            let before = tokens.len();
            tokens.retain(|t| t.is_valid());
            removed += before - tokens.len();
        }

        // Remove empty entries
        self.tokens.retain(|_, v| !v.is_empty());

        removed
    }

    /// Clear all tokens for a mint
    pub fn clear_mint(&mut self, mint_url: &str) {
        self.tokens.remove(mint_url);
    }

    /// Clear all cached tokens
    pub fn clear_all(&mut self) {
        self.tokens.clear();
    }

    /// Get total cached token count
    pub fn total_count(&self) -> usize {
        self.tokens.values().map(|v| v.len()).sum()
    }
}

// =============================================================================
// Global Cache
// =============================================================================

/// Default token TTL (1 hour)
pub const DEFAULT_TOKEN_TTL: u64 = 3600;

/// Minimum tokens to request when replenishing
pub const MIN_TOKEN_REQUEST: u32 = 5;

/// Global blind auth token cache
pub static BLIND_AUTH_CACHE: GlobalSignal<BlindAuthCache> =
    GlobalSignal::new(BlindAuthCache::new);

/// Get current timestamp
fn now_secs() -> u64 {
    js_sys::Date::now() as u64 / 1000
}

// =============================================================================
// Public API
// =============================================================================

/// Add tokens to cache for a mint
pub fn cache_tokens(mint_url: &str, tokens: Vec<BlindAuthToken>, expires_at: Option<u64>) {
    let token_count = tokens.len();
    let mut cache = BLIND_AUTH_CACHE.write();
    cache.add_tokens(mint_url, tokens, expires_at);
    log::debug!("Cached {} blind auth tokens for {}", token_count, mint_url);
}

/// Get a token for a mint (consumes from cache)
pub fn get_cached_token(mint_url: &str) -> Option<BlindAuthToken> {
    let mut cache = BLIND_AUTH_CACHE.write();
    cache.consume_token(mint_url)
}

/// Check if tokens are available for a mint
pub fn has_cached_tokens(mint_url: &str) -> bool {
    let cache = BLIND_AUTH_CACHE.read();
    cache.has_tokens(mint_url)
}

/// Get count of available tokens for a mint
pub fn cached_token_count(mint_url: &str) -> usize {
    let cache = BLIND_AUTH_CACHE.read();
    cache.available_count(mint_url)
}

/// Check if we need to request more tokens
pub fn needs_token_replenishment(mint_url: &str) -> bool {
    cached_token_count(mint_url) < MIN_TOKEN_REQUEST as usize
}

/// Cleanup expired tokens
pub fn cleanup_auth_cache() -> usize {
    let mut cache = BLIND_AUTH_CACHE.write();
    let removed = cache.cleanup_expired();
    if removed > 0 {
        log::debug!("Cleaned up {} expired auth tokens", removed);
    }
    removed
}

/// Clear all tokens for a mint
pub fn clear_mint_tokens(mint_url: &str) {
    let mut cache = BLIND_AUTH_CACHE.write();
    cache.clear_mint(mint_url);
}

// =============================================================================
// Auth Token Request (NUT-22)
// =============================================================================

/// Request new blind auth tokens from mint
///
/// This mints blind auth tokens that can be used for protected endpoints.
pub async fn request_blind_auth_tokens(
    mint_url: &str,
    count: u32,
) -> Result<Vec<BlindAuthToken>, String> {
    use super::internal::get_or_create_wallet;

    log::info!("Requesting {} blind auth tokens from {}", count, mint_url);

    // Get wallet for this mint
    let wallet = get_or_create_wallet(mint_url).await?;

    // Get mint info to check NUT-22 support
    let _mint_info = wallet
        .fetch_mint_info()
        .await
        .map_err(|e| format!("Failed to fetch mint info: {}", e))?
        .ok_or("Mint info not available")?;

    // Check NUT-22 support and get max tokens
    // Note: This would require CDK NUT-22 support which may not be available yet
    // For now, return an error indicating the feature needs implementation

    Err("Blind auth token minting not yet implemented in CDK".to_string())
}

/// Ensure we have enough tokens for a mint, requesting more if needed
pub async fn ensure_tokens_available(mint_url: &str) -> Result<(), String> {
    if !needs_token_replenishment(mint_url) {
        return Ok(());
    }

    let tokens = request_blind_auth_tokens(mint_url, MIN_TOKEN_REQUEST).await?;

    // Calculate expiry (1 hour from now)
    let expires_at = Some(now_secs() + DEFAULT_TOKEN_TTL);

    cache_tokens(mint_url, tokens, expires_at);

    Ok(())
}

// =============================================================================
// Auth Stats
// =============================================================================

/// Auth cache statistics
#[derive(Debug, Clone, Default)]
pub struct AuthCacheStats {
    pub total_tokens: usize,
    pub mints_with_tokens: usize,
    pub tokens_per_mint: HashMap<String, usize>,
}

/// Get auth cache statistics
pub fn get_auth_cache_stats() -> AuthCacheStats {
    let cache = BLIND_AUTH_CACHE.read();

    let mut tokens_per_mint = HashMap::new();
    for (mint, tokens) in cache.tokens.iter() {
        tokens_per_mint.insert(mint.clone(), tokens.iter().filter(|t| t.is_valid()).count());
    }

    AuthCacheStats {
        total_tokens: cache.total_count(),
        mints_with_tokens: tokens_per_mint.values().filter(|&c| *c > 0).count(),
        tokens_per_mint,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic() {
        let mut cache = BlindAuthCache::new();

        assert_eq!(cache.available_count("https://test.mint"), 0);
        assert!(!cache.has_tokens("https://test.mint"));

        // Would need actual BlindAuthToken to test further
    }
}
