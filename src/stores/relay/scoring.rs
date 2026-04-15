//! Relay Scoring and Reputation
//!
//! Persists relay performance snapshots to localStorage for cross-session scoring.
//! Uses SDK's built-in `relay.stats()` for live performance data.
//!
//! **IMPORTANT**: All functions take `client: &Client` as a parameter
//! rather than calling `nostr_client::get_client()` internally.
#[cfg(feature = "web")]
use crate::platform::storage;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(feature = "web")]
use std::collections::HashMap;

#[cfg(feature = "web")]
#[allow(dead_code)]
const RELAY_SCORES_KEY: &str = "nostr_relay_scores_v2";
#[cfg(feature = "web")]
#[allow(dead_code)]
const MAX_STORED_RELAYS: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct RelayScoreSnapshot {
    pub url: String,
    pub lifetime_success_rate: f64,
    pub last_seen_connected: Option<u64>,
    pub total_bytes_transferred: usize,
    pub connection_count: u32,
}

#[cfg(feature = "web")]
#[allow(dead_code)]
pub async fn persist_relay_stats(client: &Client) {
    let relays = client.relays().await;
    let mut snapshots: HashMap<String, RelayScoreSnapshot> =
        storage::get(RELAY_SCORES_KEY).unwrap_or_default();
    let now_ms = crate::platform::timestamp::now_millis();
    for (url, relay) in relays {
        let url_str = url.to_string();
        let stats = relay.stats();
        let mut snapshot = snapshots
            .remove(&url_str)
            .unwrap_or_else(|| RelayScoreSnapshot {
                url: url_str.clone(),
                ..Default::default()
            });
        snapshot.lifetime_success_rate = {
            let rate = stats.success_rate();
            if rate.is_finite() { rate } else { 0.0 }
        };
        snapshot.total_bytes_transferred = stats.bytes_sent() + stats.bytes_received();
        if relay.is_connected() {
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
    if snapshots.len() > MAX_STORED_RELAYS {
        let mut entries: Vec<_> = snapshots.into_iter().collect();
        entries.sort_by(|a, b| {
            b.1.last_seen_connected
                .unwrap_or(0)
                .cmp(&a.1.last_seen_connected.unwrap_or(0))
        });
        entries.truncate(MAX_STORED_RELAYS);
        snapshots = entries.into_iter().collect();
    }
    if let Err(e) = storage::set(RELAY_SCORES_KEY, &snapshots) {
        log::warn!("Failed to persist relay scores: {}", e);
    } else {
        log::debug!("Persisted {} relay scores", snapshots.len());
    }
}

#[cfg(feature = "native")]
#[allow(dead_code)]
pub async fn persist_relay_stats(_client: &Client) {}

#[cfg(feature = "web")]
#[allow(dead_code)]
pub fn clear_relay_scores() {
    let _ = storage::delete(RELAY_SCORES_KEY);
    log::info!("Cleared stored relay scores");
}

#[cfg(feature = "native")]
#[allow(dead_code)]
pub fn clear_relay_scores() {}
