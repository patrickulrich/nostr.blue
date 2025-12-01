//! Metadata Cache for Mint Information
//!
//! Provides caching for mint keysets and info to avoid redundant network calls.
//! Follows CDK's MintMetadataCache pattern adapted for WASM environment.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use std::collections::HashMap;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

// =============================================================================
// Cache Configuration
// =============================================================================

/// Default cache TTL in seconds (5 minutes)
const DEFAULT_TTL_SECS: u64 = 300;

/// Get current timestamp in seconds
fn now_secs() -> u64 {
    instant::Instant::now().elapsed().as_secs()
}

// =============================================================================
// Cached Types
// =============================================================================

/// Cached mint information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMintInfo {
    /// Mint name
    pub name: Option<String>,
    /// Mint description
    pub description: Option<String>,
    /// Mint pubkey (hex)
    pub pubkey: Option<String>,
    /// Supported NUTs
    pub nuts: HashMap<String, serde_json::Value>,
    /// Contact info
    pub contact: Option<Vec<ContactInfo>>,
    /// Message of the day
    pub motd: Option<String>,
    /// Icon URL
    pub icon_url: Option<String>,
    /// When this was cached (unix timestamp)
    pub cached_at: u64,
}

/// Contact info from mint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactInfo {
    pub method: String,
    pub info: String,
}

/// Cached keyset information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedKeyset {
    /// Keyset ID
    pub id: String,
    /// Currency unit
    pub unit: String,
    /// Whether this keyset is active
    pub active: bool,
    /// Input fee per proof in parts per thousand (ppk)
    pub input_fee_ppk: u64,
    /// When this was cached
    pub cached_at: u64,
}

/// Cached keyset keys (amount -> pubkey mapping)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedKeys {
    /// Keyset ID
    pub keyset_id: String,
    /// Amount to public key mapping
    pub keys: HashMap<u64, String>,
    /// When this was cached
    pub cached_at: u64,
}

// =============================================================================
// Per-Mint Cache Entry
// =============================================================================

/// Cache entry for a single mint
#[derive(Debug, Clone, Default)]
pub struct MintCacheEntry {
    /// Cached mint info
    pub info: Option<CachedMintInfo>,
    /// Cached keysets (keyset_id -> keyset)
    pub keysets: HashMap<String, CachedKeyset>,
    /// Cached keys (keyset_id -> keys)
    pub keys: HashMap<String, CachedKeys>,
    /// Cache version (incremented on updates)
    pub version: u64,
}

impl MintCacheEntry {
    /// Check if mint info is still valid
    pub fn is_info_valid(&self, ttl_secs: u64) -> bool {
        if let Some(ref info) = self.info {
            now_secs().saturating_sub(info.cached_at) < ttl_secs
        } else {
            false
        }
    }

    /// Check if keysets are still valid
    pub fn are_keysets_valid(&self, ttl_secs: u64) -> bool {
        if self.keysets.is_empty() {
            return false;
        }
        // Check if any keyset is still within TTL
        self.keysets.values().any(|ks| {
            now_secs().saturating_sub(ks.cached_at) < ttl_secs
        })
    }

    /// Get active keyset IDs
    pub fn active_keyset_ids(&self) -> Vec<String> {
        self.keysets
            .iter()
            .filter(|(_, ks)| ks.active)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Get keyset by ID if cached and valid
    pub fn get_keyset(&self, keyset_id: &str, ttl_secs: u64) -> Option<&CachedKeyset> {
        self.keysets.get(keyset_id).filter(|ks| {
            now_secs().saturating_sub(ks.cached_at) < ttl_secs
        })
    }

    /// Get keys for keyset if cached and valid
    pub fn get_keys(&self, keyset_id: &str, ttl_secs: u64) -> Option<&CachedKeys> {
        self.keys.get(keyset_id).filter(|k| {
            now_secs().saturating_sub(k.cached_at) < ttl_secs
        })
    }
}

// =============================================================================
// Global Cache
// =============================================================================

/// Global metadata cache for all mints
pub static MINT_CACHE: GlobalSignal<MintMetadataCache> = GlobalSignal::new(MintMetadataCache::new);

/// Metadata cache managing multiple mints
#[derive(Debug, Clone, Default)]
pub struct MintMetadataCache {
    /// Per-mint cache entries
    pub mints: HashMap<String, MintCacheEntry>,
    /// Cache TTL in seconds
    pub ttl_secs: u64,
    /// Global version counter
    version: u64,
}

impl MintMetadataCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            mints: HashMap::new(),
            ttl_secs: DEFAULT_TTL_SECS,
            version: 0,
        }
    }

    /// Set cache TTL
    pub fn set_ttl(&mut self, ttl_secs: u64) {
        self.ttl_secs = ttl_secs;
    }

    /// Get or create cache entry for a mint
    pub fn get_or_create_mint(&mut self, mint_url: &str) -> &mut MintCacheEntry {
        let normalized = normalize_mint_url(mint_url);
        self.mints.entry(normalized).or_default()
    }

    /// Get cache entry for a mint (read-only)
    pub fn get_mint(&self, mint_url: &str) -> Option<&MintCacheEntry> {
        let normalized = normalize_mint_url(mint_url);
        self.mints.get(&normalized)
    }

    /// Cache mint info
    pub fn cache_mint_info(&mut self, mint_url: &str, info: CachedMintInfo) {
        let entry = self.get_or_create_mint(mint_url);
        entry.info = Some(info);
        entry.version += 1;
        self.version += 1;
    }

    /// Cache keysets for a mint
    pub fn cache_keysets(&mut self, mint_url: &str, keysets: Vec<CachedKeyset>) {
        let entry = self.get_or_create_mint(mint_url);
        for keyset in keysets {
            entry.keysets.insert(keyset.id.clone(), keyset);
        }
        entry.version += 1;
        self.version += 1;
    }

    /// Cache keys for a keyset
    pub fn cache_keys(&mut self, mint_url: &str, keys: CachedKeys) {
        let entry = self.get_or_create_mint(mint_url);
        entry.keys.insert(keys.keyset_id.clone(), keys);
        entry.version += 1;
        self.version += 1;
    }

    /// Check if mint info is cached and valid
    pub fn has_valid_info(&self, mint_url: &str) -> bool {
        self.get_mint(mint_url)
            .map(|e| e.is_info_valid(self.ttl_secs))
            .unwrap_or(false)
    }

    /// Check if keysets are cached and valid
    pub fn has_valid_keysets(&self, mint_url: &str) -> bool {
        self.get_mint(mint_url)
            .map(|e| e.are_keysets_valid(self.ttl_secs))
            .unwrap_or(false)
    }

    /// Get cached mint info if valid
    pub fn get_info(&self, mint_url: &str) -> Option<&CachedMintInfo> {
        self.get_mint(mint_url)
            .and_then(|e| {
                if e.is_info_valid(self.ttl_secs) {
                    e.info.as_ref()
                } else {
                    None
                }
            })
    }

    /// Invalidate cache for a mint
    pub fn invalidate_mint(&mut self, mint_url: &str) {
        let normalized = normalize_mint_url(mint_url);
        self.mints.remove(&normalized);
        self.version += 1;
    }

    /// Clear all cached data
    pub fn clear(&mut self) {
        self.mints.clear();
        self.version += 1;
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let mut total_keysets = 0;
        let mut total_keys = 0;

        for entry in self.mints.values() {
            total_keysets += entry.keysets.len();
            total_keys += entry.keys.len();
        }

        CacheStats {
            mint_count: self.mints.len(),
            total_keysets,
            total_keys,
            version: self.version,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub mint_count: usize,
    pub total_keysets: usize,
    pub total_keys: usize,
    pub version: u64,
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Normalize mint URL for cache key
fn normalize_mint_url(url: &str) -> String {
    url.trim_end_matches('/').to_lowercase()
}

/// Parse mint info from JSON response into cached format
pub fn parse_mint_info_to_cache(json: &serde_json::Value) -> CachedMintInfo {
    CachedMintInfo {
        name: json.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()),
        description: json.get("description").and_then(|v| v.as_str()).map(|s| s.to_string()),
        pubkey: json.get("pubkey").and_then(|v| v.as_str()).map(|s| s.to_string()),
        nuts: json.get("nuts")
            .and_then(|v| v.as_object())
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default(),
        contact: json.get("contact")
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
        motd: json.get("motd").and_then(|v| v.as_str()).map(|s| s.to_string()),
        icon_url: json.get("icon_url").and_then(|v| v.as_str()).map(|s| s.to_string()),
        cached_at: now_secs(),
    }
}

/// Parse keysets from JSON response into cached format
pub fn parse_keysets_to_cache(json: &serde_json::Value) -> Vec<CachedKeyset> {
    let keysets = json.get("keysets")
        .and_then(|v| v.as_array())
        .map(|arr| arr.to_vec())
        .unwrap_or_default();

    let now = now_secs();
    keysets.into_iter().filter_map(|ks| {
        Some(CachedKeyset {
            id: ks.get("id")?.as_str()?.to_string(),
            unit: ks.get("unit").and_then(|v| v.as_str()).unwrap_or("sat").to_string(),
            active: ks.get("active").and_then(|v| v.as_bool()).unwrap_or(true),
            input_fee_ppk: ks.get("input_fee_ppk").and_then(|v| v.as_u64()).unwrap_or(0),
            cached_at: now,
        })
    }).collect()
}

/// Parse keys from JSON response into cached format
pub fn parse_keys_to_cache(keyset_id: &str, json: &serde_json::Value) -> CachedKeys {
    let keys_obj = json.get("keys")
        .or_else(|| json.get("keysets").and_then(|ks| ks.get(0)).and_then(|k| k.get("keys")))
        .and_then(|v| v.as_object());

    let keys = keys_obj
        .map(|obj| {
            obj.iter()
                .filter_map(|(amount_str, pubkey)| {
                    let amount = amount_str.parse::<u64>().ok()?;
                    let pk = pubkey.as_str()?.to_string();
                    Some((amount, pk))
                })
                .collect()
        })
        .unwrap_or_default();

    CachedKeys {
        keyset_id: keyset_id.to_string(),
        keys,
        cached_at: now_secs(),
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Get cached mint info, or None if not cached/expired
pub fn get_cached_mint_info(mint_url: &str) -> Option<CachedMintInfo> {
    MINT_CACHE.read().get_info(mint_url).cloned()
}

/// Check if we have valid cached data for a mint
pub fn is_mint_cached(mint_url: &str) -> bool {
    let cache = MINT_CACHE.read();
    cache.has_valid_info(mint_url) && cache.has_valid_keysets(mint_url)
}

/// Cache mint info after fetching
pub fn cache_mint_info(mint_url: &str, info: CachedMintInfo) {
    MINT_CACHE.write().cache_mint_info(mint_url, info);
}

/// Cache keysets after fetching
pub fn cache_keysets(mint_url: &str, keysets: Vec<CachedKeyset>) {
    MINT_CACHE.write().cache_keysets(mint_url, keysets);
}

/// Cache keys after fetching
pub fn cache_keys(mint_url: &str, keys: CachedKeys) {
    MINT_CACHE.write().cache_keys(mint_url, keys);
}

/// Invalidate cache for a specific mint
pub fn invalidate_cache(mint_url: &str) {
    MINT_CACHE.write().invalidate_mint(mint_url);
}

/// Get cache statistics
pub fn get_cache_stats() -> CacheStats {
    MINT_CACHE.read().stats()
}

/// Get active keyset IDs for a mint from cache
pub fn get_cached_active_keysets(mint_url: &str) -> Vec<String> {
    MINT_CACHE.read()
        .get_mint(mint_url)
        .map(|e| e.active_keyset_ids())
        .unwrap_or_default()
}

/// Get input fee for a keyset from cache
pub fn get_cached_keyset_fee(mint_url: &str, keyset_id: &str) -> Option<u64> {
    let cache = MINT_CACHE.read();
    cache.get_mint(mint_url)
        .and_then(|e| e.get_keyset(keyset_id, cache.ttl_secs))
        .map(|ks| ks.input_fee_ppk)
}

// =============================================================================
// NUT-19 Response Caching
// =============================================================================

/// Response cache for mint API endpoints (NUT-19)
///
/// Caches responses from various mint endpoints to reduce network calls
/// and improve performance. Respects Cache-Control headers from mint.
pub static RESPONSE_CACHE: GlobalSignal<ResponseCache> = GlobalSignal::new(ResponseCache::new);

/// Cached API response
#[derive(Debug, Clone)]
pub struct CachedResponse {
    /// Response body
    pub body: String,
    /// When this was cached
    pub cached_at: u64,
    /// TTL in seconds (from Cache-Control or default)
    pub ttl_secs: u64,
    /// ETag for conditional requests
    pub etag: Option<String>,
}

impl CachedResponse {
    /// Check if this cached response is still valid
    pub fn is_valid(&self) -> bool {
        now_secs().saturating_sub(self.cached_at) < self.ttl_secs
    }
}

/// Response cache for NUT-19 optimization
#[derive(Debug, Clone, Default)]
pub struct ResponseCache {
    /// Cached responses keyed by URL
    responses: HashMap<String, CachedResponse>,
}

impl ResponseCache {
    /// Create a new empty response cache
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
        }
    }

    /// Get a cached response if valid
    pub fn get(&self, url: &str) -> Option<&CachedResponse> {
        self.responses.get(url).filter(|r| r.is_valid())
    }

    /// Store a response in the cache
    pub fn store(&mut self, url: String, body: String, ttl_secs: u64, etag: Option<String>) {
        self.responses.insert(url, CachedResponse {
            body,
            cached_at: now_secs(),
            ttl_secs,
            etag,
        });
    }

    /// Remove a cached response
    pub fn invalidate(&mut self, url: &str) {
        self.responses.remove(url);
    }

    /// Clear all cached responses
    pub fn clear(&mut self) {
        self.responses.clear();
    }

    /// Clean up expired entries
    pub fn cleanup_expired(&mut self) {
        self.responses.retain(|_, r| r.is_valid());
    }
}

/// Default TTLs for different endpoint types (in seconds)
pub mod ttls {
    /// Mint info endpoint (changes rarely)
    pub const MINT_INFO: u64 = 300; // 5 minutes
    /// Keysets endpoint (changes rarely)
    pub const KEYSETS: u64 = 300; // 5 minutes
    /// Keys endpoint (rarely changes)
    pub const KEYS: u64 = 600; // 10 minutes
    /// Quote endpoints (short-lived)
    pub const QUOTE: u64 = 30; // 30 seconds
    /// State check endpoints (very short)
    pub const STATE: u64 = 5; // 5 seconds
}

/// Parse Cache-Control header to extract max-age
pub fn parse_cache_control(header: Option<&str>) -> Option<u64> {
    header.and_then(|h| {
        for part in h.split(',') {
            let part = part.trim();
            if part.starts_with("max-age=") {
                return part[8..].parse().ok();
            }
        }
        None
    })
}

/// Cached fetch with NUT-19 support
///
/// Fetches a URL with caching support. Returns cached response if valid,
/// otherwise makes a new request.
pub async fn cached_fetch(url: &str, default_ttl: u64) -> Result<String, String> {
    // Check cache first
    {
        let cache = RESPONSE_CACHE.read();
        if let Some(cached) = cache.get(url) {
            log::debug!("Cache hit for {}", url);
            return Ok(cached.body.clone());
        }
    }

    log::debug!("Cache miss for {}, fetching...", url);

    // Make request
    let response = gloo_net::http::Request::get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    // Extract cache control headers
    let cache_control = response.headers().get("cache-control");
    let etag = response.headers().get("etag");
    let ttl = parse_cache_control(cache_control.as_deref()).unwrap_or(default_ttl);

    let body = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Store in cache
    {
        let mut cache = RESPONSE_CACHE.write();
        cache.store(url.to_string(), body.clone(), ttl, etag);
    }

    Ok(body)
}

/// Fetch mint info with caching
pub async fn cached_fetch_mint_info(mint_url: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}/v1/info", mint_url.trim_end_matches('/'));
    let body = cached_fetch(&url, ttls::MINT_INFO).await?;
    serde_json::from_str(&body).map_err(|e| format!("Invalid JSON: {}", e))
}

/// Fetch keysets with caching
pub async fn cached_fetch_keysets(mint_url: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}/v1/keysets", mint_url.trim_end_matches('/'));
    let body = cached_fetch(&url, ttls::KEYSETS).await?;
    serde_json::from_str(&body).map_err(|e| format!("Invalid JSON: {}", e))
}

/// Fetch keys for a keyset with caching
pub async fn cached_fetch_keys(mint_url: &str, keyset_id: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}/v1/keys/{}", mint_url.trim_end_matches('/'), keyset_id);
    let body = cached_fetch(&url, ttls::KEYS).await?;
    serde_json::from_str(&body).map_err(|e| format!("Invalid JSON: {}", e))
}
