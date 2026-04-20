//! Relay health tracking and quarantine
//!
//! Periodically polls SDK relay status to maintain an accurate health picture.
//! Relays with sustained failures are quarantined (removed from the active pool)
//! to avoid wasting reconnection attempts.
//!
//! # Architecture
//!
//! - `RELAY_HEALTH` GlobalSignal stores per-relay health snapshots
//! - `poll_relay_health()` queries the SDK pool and updates the signal
//! - `quarantine_dead_relays()` removes relays with excessive failures
//! - `start_health_poll()` spawns the periodic background task
use super::signals::RelayPoolStoreStoreExt;
use dioxus::prelude::*;
use dioxus_core::spawn_forever;
use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct RelayHealthEntry {
    pub url: String,
    pub connected: bool,
    pub success_rate: f64,
    pub consecutive_failures: u32,
    pub last_connected_at: Option<u64>,
    pub quarantine_count: u32,
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct RelayHealthState {
    pub relays: HashMap<String, RelayHealthEntry>,
    pub last_poll_ms: u64,
}

pub static RELAY_HEALTH: GlobalSignal<RelayHealthState> = Signal::global(RelayHealthState::default);

/// How many consecutive failures before quarantining a relay
const QUARANTINE_THRESHOLD: u32 = 5;

/// Minimum time (ms) between health polls
const POLL_INTERVAL_MS: u64 = 30_000;

fn now_millis() -> u64 {
    crate::platform::timestamp::now_millis()
}

/// Query the SDK pool and update RELAY_HEALTH with current status.
/// Returns the number of connected relays.
pub async fn poll_relay_health(client: &Client) -> usize {
    let relays = client.relays().await;
    let mut connected = 0usize;
    let mut new_state = RelayHealthState {
        last_poll_ms: now_millis(),
        ..Default::default()
    };

    for (url, relay) in &relays {
        let url_str = url.to_string();
        let stats = relay.stats();
        let is_connected = relay.is_connected();
        if is_connected {
            connected += 1;
        }

        let success_rate = {
            let rate = stats.success_rate();
            if rate.is_finite() { rate } else { 0.0 }
        };

        let prev = RELAY_HEALTH
            .peek()
            .relays
            .get(&url_str)
            .cloned()
            .unwrap_or_default();

        let consecutive_failures = if is_connected {
            0
        } else {
            prev.consecutive_failures.saturating_add(1)
        };

        let last_connected_at = if is_connected {
            Some(now_millis())
        } else {
            prev.last_connected_at
        };

        new_state.relays.insert(
            url_str.clone(),
            RelayHealthEntry {
                url: url_str,
                connected: is_connected,
                success_rate,
                consecutive_failures,
                last_connected_at,
                quarantine_count: prev.quarantine_count,
            },
        );
    }

    *RELAY_HEALTH.write() = new_state;
    connected
}

/// Remove relays that have exceeded the failure threshold.
/// Returns URLs of removed relays.
pub async fn quarantine_dead_relays(client: &Client) -> Vec<String> {
    let to_remove: Vec<String> = {
        let health = RELAY_HEALTH.peek();
        health
            .relays
            .iter()
            .filter(|(_, entry)| {
                entry.consecutive_failures >= QUARANTINE_THRESHOLD && !entry.connected
            })
            .map(|(url, _)| url.clone())
            .collect()
    };

    if to_remove.is_empty() {
        return Vec::new();
    }

    for url in &to_remove {
        log::warn!(
            "Quarantining relay {} after sustained failures",
            url
        );
        if let Ok(relay_url) = RelayUrl::parse(url.as_str()) {
            let _ = client.remove_relay(relay_url).await;
        }
    }

    {
        let mut state = RELAY_HEALTH.write();
        for url in &to_remove {
            if let Some(entry) = state.relays.get_mut(url) {
                entry.quarantine_count = entry.quarantine_count.saturating_add(1);
            }
        }
    }

    {
        let pool_data = super::signals::RELAY_POOL.read();
        let mut data = pool_data.data();
        let mut relays = data.write();
        relays.retain(|r| !to_remove.contains(&r.url));
    }

    log::info!("Quarantined {} dead relays", to_remove.len());
    to_remove
}

/// Update the `RELAY_CONNECTED` and `RELAY_POOL` UI signals from SDK state.
pub async fn sync_ui_signals(client: &Client) {
    let relays = client.relays().await;
    let any_connected = relays.values().any(|r| r.is_connected());

    if any_connected && !*super::signals::RELAY_CONNECTED.peek() {
        *super::signals::RELAY_CONNECTED.write() = true;
    } else if !any_connected && *super::signals::RELAY_CONNECTED.peek() {
        *super::signals::RELAY_CONNECTED.write() = false;
    }

    let metadata = super::nip65::USER_RELAY_METADATA.read().clone();
    let mut relay_infos = Vec::new();

    for (url, relay) in &relays {
        let url_str = url.to_string();
        let status = if relay.is_connected() {
            nostr_relay_pool::RelayStatus::Connected
        } else {
            nostr_relay_pool::RelayStatus::Disconnected
        };

        let flags = relay.flags();
        let (has_read, has_write, source) = if let Some(ref m) = metadata {
            if let Some(user_relay) = m.relays.iter().find(|r| r.url == url_str) {
                (user_relay.read, user_relay.write, super::signals::RelaySource::UserNip65)
            } else if super::pool::DEFAULT_RELAYS.contains(&url_str.as_str()) {
                (true, true, super::signals::RelaySource::Default)
            } else {
                (flags.has_read(), flags.has_write(), super::signals::RelaySource::Manual)
            }
        } else if super::pool::DEFAULT_RELAYS.contains(&url_str.as_str()) {
            (true, true, super::signals::RelaySource::Default)
        } else {
            (flags.has_read(), flags.has_write(), super::signals::RelaySource::Manual)
        };

        relay_infos.push(super::signals::RelayInfo::with_flags(
            url_str, status, has_read, has_write, source,
        ));
    }

    super::signals::RELAY_POOL.read().data().write().clone_from(&relay_infos);
}

/// Start the background health polling loop.
/// Call once after client initialization.
pub fn start_health_poll(client: Arc<Client>) {
    #[cfg(target_arch = "wasm32")]
    {
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                crate::stores::nostr_client::platform_sleep_ms(POLL_INTERVAL_MS).await;
                let connected = poll_relay_health(&client).await;
                let _ = connected;
                sync_ui_signals(&client).await;
                quarantine_dead_relays(&client).await;
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        spawn_forever(async move {
            loop {
                crate::stores::nostr_client::platform_sleep_ms(POLL_INTERVAL_MS).await;
                let connected = poll_relay_health(&client).await;
                let _ = connected;
                sync_ui_signals(&client).await;
                quarantine_dead_relays(&client).await;
            }
        });
    }
    log::info!("Relay health polling started (interval: {}s)", POLL_INTERVAL_MS / 1000);
}

/// Get the number of currently connected relays from the health signal.
#[allow(dead_code)]
pub fn connected_count() -> usize {
    RELAY_HEALTH
        .peek()
        .relays
        .values()
        .filter(|r| r.connected)
        .count()
}

/// Get the number of quarantined relays.
#[allow(dead_code)]
pub fn quarantined_count() -> usize {
    RELAY_HEALTH
        .peek()
        .relays
        .values()
        .filter(|r| r.quarantine_count > 0)
        .count()
}
