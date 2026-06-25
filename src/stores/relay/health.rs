//! Relay health tracking and quarantine
//!
//! Periodically polls SDK relay status to maintain an accurate health picture.
//! Relays with sustained failures are soft-quarantined (logged + tracked) but
//! NOT removed from the pool. The SDK's built-in auto-reconnection handles
//! retrying with incremental backoff.
//!
//! # Architecture
//!
//! - `RELAY_HEALTH` GlobalSignal stores per-relay health snapshots
//! - `poll_relay_health()` queries the SDK pool and updates the signal
//! - `quarantine_dead_relays()` marks relays with excessive failures (soft quarantine)
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

/// How many consecutive failures before soft-quarantining a relay (log only)
const QUARANTINE_THRESHOLD: u32 = 5;

/// How many consecutive failures before actively removing a relay (10 min at 30s poll).
/// Uses remove_relay() which respects GOSSIP flag (downgrades rather than kills).
const REMOVE_THRESHOLD: u32 = 20;

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

/// Two-tier relay quarantine:
///
/// Tier 1 (QUARANTINE_THRESHOLD = 5 failures / 2.5 min):
///   Soft quarantine — log only. The SDK's built-in adaptive retry
///   (10s -> 60s with jitter) continues retrying automatically.
///
/// Tier 2 (REMOVE_THRESHOLD = 20 failures / 10 min):
///   Active removal via `client.remove_relay(url)`. Uses the SDK's
///   `can_remove_relay()` logic: GOSSIP relays get downgraded (READ/WRITE
///   flags removed, stay in pool), others fully removed + disconnected.
///   NOT disconnect_relay() which permanently sets Terminated and caused
///   browser WebSocket "Insufficient resources" exhaustion.
pub async fn quarantine_dead_relays(client: &Client) -> Vec<String> {
    let mut to_soft_quarantine: Vec<String> = Vec::new();
    let mut to_remove: Vec<String> = Vec::new();

    {
        let health = RELAY_HEALTH.peek();
        for (url, entry) in &health.relays {
            if entry.connected {
                continue;
            }
            if entry.consecutive_failures >= REMOVE_THRESHOLD
                && entry.quarantine_count < 2
            {
                to_remove.push(url.clone());
            } else if entry.consecutive_failures >= QUARANTINE_THRESHOLD
                && entry.quarantine_count == 0
            {
                to_soft_quarantine.push(url.clone());
            }
        }
    }

    for url in &to_soft_quarantine {
        log::warn!(
            "Relay {} has {} consecutive failures (SDK auto-retry active)",
            url,
            QUARANTINE_THRESHOLD
        );
    }

    for url in &to_remove {
        log::warn!(
            "Relay {} has {}+ consecutive failures, removing from pool",
            url,
            REMOVE_THRESHOLD
        );
        if let Err(e) = client.remove_relay(url).await {
            log::debug!(
                "Relay {} couldn't be fully removed (likely GOSSIP-only, downgraded): {:?}",
                url,
                e
            );
        }
    }

    {
        let mut state = RELAY_HEALTH.write();
        for url in &to_soft_quarantine {
            if let Some(entry) = state.relays.get_mut(url) {
                entry.quarantine_count = 1;
            }
        }
        for url in &to_remove {
            if let Some(entry) = state.relays.get_mut(url) {
                entry.quarantine_count = 2;
            }
        }
    }

    let total = to_soft_quarantine.len() + to_remove.len();
    if total > 0 {
        log::info!(
            "Quarantined {} relays (soft: {}, removed: {})",
            total,
            to_soft_quarantine.len(),
            to_remove.len()
        );
    }

    to_soft_quarantine
        .into_iter()
        .chain(to_remove)
        .collect()
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

    // Only write RELAY_POOL if the data actually changed, to avoid
    // re-rendering every UI component subscribed to it every 30s.
    let current = super::signals::RELAY_POOL.peek().data().read().clone();
    if current != relay_infos {
        super::signals::RELAY_POOL
            .read()
            .data()
            .write()
            .clone_from(&relay_infos);
    }
}

/// Start the background health polling loop.
/// Call once after client initialization.
pub fn start_health_poll(client: Arc<Client>) {
    #[cfg(target_arch = "wasm32")]
    {
        // Single sequential loop — matches the native path below.
        // Previously this spawned a nested `spawn_local` per 30s tick, which
        // risked task accumulation and handle churn in the externref table
        // when the tab was idle (the very symptom this file is hardening against).
        crate::platform::spawn::spawn_local_catch_unwind("health_poll", async move {
            let mut tick: u32 = 0;
            loop {
                crate::stores::nostr_client::platform_sleep_ms(POLL_INTERVAL_MS).await;
                let connected = poll_relay_health(&client).await;
                let _ = connected;
                sync_ui_signals(&client).await;
                quarantine_dead_relays(&client).await;
                tick = tick.wrapping_add(1);
                if tick % 10 == 0 {
                    super::coverage::cleanup_gossip_relays(&client).await;
                }
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        spawn_forever(async move {
            let mut tick: u32 = 0;
            loop {
                crate::stores::nostr_client::platform_sleep_ms(POLL_INTERVAL_MS).await;
                let connected = poll_relay_health(&client).await;
                let _ = connected;
                sync_ui_signals(&client).await;
                quarantine_dead_relays(&client).await;
                tick = tick.wrapping_add(1);
                if tick % 10 == 0 {
                    super::coverage::cleanup_gossip_relays(&client).await;
                }
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
