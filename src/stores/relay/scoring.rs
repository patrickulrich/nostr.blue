//! Relay Scoring and Reputation
//!
//! This module provides relay scoring based on SDK stats and persisted history.
//! Scores are used for relay selection prioritization.
//!
//! # Design
//!
//! - Uses SDK's built-in `relay.stats()` for live performance data
//! - Persists snapshots to localStorage for cross-session scoring
//! - Combines live stats with persisted history for comprehensive scoring
//!
//! **IMPORTANT**: All functions take `client: &Client` as a parameter
//! rather than calling `nostr_client::get_client()` internally. This avoids
//! circular dependencies and follows the relay module design principle.

#[cfg(target_arch = "wasm32")]
use gloo_storage::{LocalStorage, Storage};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use std::collections::HashMap;

/// Storage key for relay scores
#[cfg(target_arch = "wasm32")]
const RELAY_SCORES_KEY: &str = "nostr_relay_scores_v2";

/// Maximum number of relay scores to store (prevent localStorage bloat)
#[cfg(target_arch = "wasm32")]
const MAX_STORED_RELAYS: usize = 100;

/// Persisted relay score snapshot (localStorage)
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct RelayScoreSnapshot {
    pub url: String,
    /// Lifetime success rate from SDK stats
    pub lifetime_success_rate: f64,
    /// Last time this relay was seen connected (Unix timestamp ms)
    pub last_seen_connected: Option<u64>,
    /// Total bytes transferred through this relay
    pub total_bytes_transferred: usize,
    /// Number of times we've connected to this relay
    pub connection_count: u32,
}

/// Get current relay score combining SDK stats + persisted history
///
/// Returns a score from 0.0 to 1.0 where higher is better.
/// Factors considered:
/// - Live success rate from SDK stats (primary)
/// - Connected status (boost)
/// - Historical reliability from persisted data
///
/// # Arguments
/// * `client` - The Nostr client instance
/// * `url` - The relay URL to get score for
#[allow(dead_code)]
pub async fn get_relay_score(client: &Client, url: &str) -> f64 {
    let relays = client.relays().await;

    if let Ok(relay_url) = RelayUrl::parse(url) {
        if let Some(relay) = relays.get(&relay_url) {
            let stats = relay.stats();

            // Primary score from SDK's live stats
            // Guard against NaN - fall back to 0.0 if not finite
            let live_score = {
                let rate = stats.success_rate();
                if rate.is_finite() {
                    rate
                } else {
                    0.0
                }
            };

            // Boost for currently connected relays
            let connected_boost = if relay.is_connected() { 0.1 } else { 0.0 };

            return (live_score + connected_boost).min(1.0);
        }
    }

    // Fall back to persisted score for unknown relays
    get_persisted_score(url).unwrap_or(0.5)
}

/// Get persisted score from localStorage (WASM only)
#[cfg(target_arch = "wasm32")]
fn get_persisted_score(url: &str) -> Option<f64> {
    let snapshots: HashMap<String, RelayScoreSnapshot> =
        LocalStorage::get(RELAY_SCORES_KEY).ok()?;

    snapshots.get(url).map(|s| s.lifetime_success_rate)
}

/// Get persisted score - returns None on non-WASM platforms
#[cfg(not(target_arch = "wasm32"))]
fn get_persisted_score(_url: &str) -> Option<f64> {
    None
}

/// Sort relays by score (best first)
///
/// # Arguments
/// * `client` - The Nostr client instance
/// * `relays` - List of relay URLs to sort
///
/// # Returns
/// Sorted list with highest-scoring relays first
#[allow(dead_code)]
pub async fn sort_relays_by_score(client: &Client, relays: Vec<String>) -> Vec<String> {
    let mut scored = Vec::new();
    for url in relays {
        let score = get_relay_score(client, &url).await;
        scored.push((url, score));
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(url, _)| url).collect()
}

/// Snapshot current SDK stats to localStorage (WASM only)
///
/// Call this periodically to persist relay performance data:
/// - On app backgrounding
/// - Every 5 minutes while active
/// - Before logout
///
/// # Arguments
/// * `client` - The Nostr client instance
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
pub async fn persist_relay_stats(client: &Client) {
    let relays = client.relays().await;
    let mut snapshots: HashMap<String, RelayScoreSnapshot> =
        LocalStorage::get(RELAY_SCORES_KEY).unwrap_or_default();

    let now_ms = js_sys::Date::now() as u64;

    for (url, relay) in relays {
        let url_str = url.to_string();
        let stats = relay.stats();

        // Get or create snapshot
        let mut snapshot = snapshots
            .remove(&url_str)
            .unwrap_or_else(|| RelayScoreSnapshot {
                url: url_str.clone(),
                ..Default::default()
            });

        // Update with current stats
        // Guard against NaN/Inf - fall back to 0.0 if not finite
        snapshot.lifetime_success_rate = {
            let rate = stats.success_rate();
            if rate.is_finite() {
                rate
            } else {
                0.0
            }
        };
        snapshot.total_bytes_transferred = stats.bytes_sent() + stats.bytes_received();

        // Update connected time if currently connected
        if relay.is_connected() {
            // Only count as new connection if there was a gap (>60s since last seen)
            let is_new_connection = snapshot
                .last_seen_connected
                .map(|last| now_ms.saturating_sub(last) > 60_000)
                .unwrap_or(true);

            if is_new_connection {
                snapshot.connection_count = snapshot.connection_count.saturating_add(1);
            }
            snapshot.last_seen_connected = Some(now_ms);
        }

        snapshots.insert(url_str, snapshot);
    }

    // Prune old entries if we have too many
    if snapshots.len() > MAX_STORED_RELAYS {
        // Sort by last_seen_connected and keep most recent
        let mut entries: Vec<_> = snapshots.into_iter().collect();
        entries.sort_by(|a, b| {
            b.1.last_seen_connected
                .unwrap_or(0)
                .cmp(&a.1.last_seen_connected.unwrap_or(0))
        });
        entries.truncate(MAX_STORED_RELAYS);
        snapshots = entries.into_iter().collect();
    }

    if let Err(e) = LocalStorage::set(RELAY_SCORES_KEY, &snapshots) {
        log::warn!("Failed to persist relay scores: {}", e);
    } else {
        log::debug!("Persisted {} relay scores to localStorage", snapshots.len());
    }
}

/// Snapshot current SDK stats - no-op on non-WASM platforms
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub async fn persist_relay_stats(_client: &Client) {
    // No-op: localStorage not available on native platforms
}

/// Get statistics about stored relay scores (WASM only)
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
pub fn get_score_stats() -> Option<(usize, f64)> {
    let snapshots: HashMap<String, RelayScoreSnapshot> =
        LocalStorage::get(RELAY_SCORES_KEY).ok()?;

    if snapshots.is_empty() {
        return None;
    }

    let avg_score: f64 = snapshots
        .values()
        .map(|s| s.lifetime_success_rate)
        .sum::<f64>()
        / snapshots.len() as f64;

    Some((snapshots.len(), avg_score))
}

/// Get statistics about stored relay scores - returns None on non-WASM platforms
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn get_score_stats() -> Option<(usize, f64)> {
    None
}

/// Clear all stored relay scores (WASM only)
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
pub fn clear_relay_scores() {
    LocalStorage::delete(RELAY_SCORES_KEY);
    log::info!("Cleared stored relay scores");
}

/// Clear all stored relay scores - no-op on non-WASM platforms
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn clear_relay_scores() {
    // No-op: localStorage not available on native platforms
}
