//! Shared search relay connection caching
//!
//! Used by both content_search and profile_search to avoid duplicate connections.
//! Cache is keyed by pubkey to handle account switches properly.

use std::collections::HashMap;
use nostr_sdk::prelude::*;

use crate::stores::relay;

/// Cached search relays by pubkey (keyed to handle account switches)
/// Key is pubkey hex string, value is connected relay URLs
static SEARCH_RELAYS_CACHE: std::sync::OnceLock<tokio::sync::RwLock<HashMap<String, Vec<String>>>> =
    std::sync::OnceLock::new();

/// Invalidate the search relay cache (call on logout/relay-list updates)
pub async fn invalidate_search_relay_cache() {
    let lock = SEARCH_RELAYS_CACHE.get_or_init(|| tokio::sync::RwLock::new(HashMap::new()));
    let mut cache = lock.write().await;
    cache.clear();
    log::debug!("Search relay cache invalidated");
}

/// Get connected search relay URLs, ensuring they're in the pool first.
/// Returns empty vec if no search relays could be connected (will fallback to all relays).
/// Cache is keyed by current user's pubkey to handle account switches.
pub async fn get_connected_search_relays(client: &Client) -> Vec<String> {
    let lock = SEARCH_RELAYS_CACHE.get_or_init(|| tokio::sync::RwLock::new(HashMap::new()));

    // Get current pubkey for cache key (use "anonymous" if not authenticated)
    let cache_key = crate::stores::auth_store::get_pubkey()
        .unwrap_or_else(|| "anonymous".to_string());

    // Check if already connected for this user
    {
        let cached = lock.read().await;
        if let Some(relays) = cached.get(&cache_key) {
            if !relays.is_empty() {
                return relays.clone();
            }
        }
    }

    // Connect search relays and cache the result
    let connected = relay::ensure_search_relays_connected(client).await;

    // Cache if successful (double-check pattern: re-check cache after acquiring write lock)
    if !connected.is_empty() {
        let mut cached = lock.write().await;
        // Double-check: another task may have populated cache while we waited for write lock
        if let Some(existing) = cached.get(&cache_key) {
            if !existing.is_empty() {
                return existing.clone();
            }
        }
        cached.insert(cache_key, connected.clone());
    }

    connected
}
