//! Shared search relay connection caching
//!
//! Used by both content_search and profile_search to avoid duplicate connections.

use nostr_sdk::prelude::*;

use crate::stores::relay;

/// Cached search relays after connection (avoids reconnecting on every search)
static SEARCH_RELAYS_CONNECTED: std::sync::OnceLock<tokio::sync::RwLock<Vec<String>>> =
    std::sync::OnceLock::new();

/// Get connected search relay URLs, ensuring they're in the pool first.
/// Returns empty vec if no search relays could be connected (will fallback to all relays).
pub async fn get_connected_search_relays(client: &Client) -> Vec<String> {
    let lock = SEARCH_RELAYS_CONNECTED.get_or_init(|| tokio::sync::RwLock::new(Vec::new()));

    // Check if already connected
    {
        let cached = lock.read().await;
        if !cached.is_empty() {
            return cached.clone();
        }
    }

    // Connect search relays and cache the result
    let connected = relay::ensure_search_relays_connected(client).await;

    // Cache if successful
    if !connected.is_empty() {
        let mut cached = lock.write().await;
        *cached = connected.clone();
    }

    connected
}
