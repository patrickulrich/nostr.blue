//! Relay pool management
//!
//! Functions for adding, removing, and managing relays in the pool.
//! All functions take `client: &Client` as parameter to avoid circular dependencies.

use dioxus::prelude::WritableExt;
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use nostr::Url;
use std::sync::Arc;

use super::signals::{RelayInfo, RelaySource, RelayStatus, RELAY_POOL, USER_RELAYS_APPLIED};
// Import the auto-generated Store trait for accessing RELAY_POOL.data()
use super::signals::RelayPoolStoreStoreExt;
use crate::stores::relay::nip65::USER_RELAY_METADATA;

/// Check if a relay URL is in the blocked list
fn is_relay_blocked(url: &str) -> bool {
    let blocked = super::nip65::BLOCKED_RELAYS.peek();
    let normalized = url.trim_end_matches('/');
    blocked.iter().any(|blocked_url| {
        blocked_url.trim_end_matches('/') == normalized
    })
}

/// Default relays to connect to
pub const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.snort.social",
    "wss://nostr.wine",
];

/// Add a custom relay to the pool
///
/// # Arguments
/// * `client` - The Nostr client instance
/// * `relay_url` - URL of the relay to add
///
/// IMPORTANT: Takes client as parameter to avoid circular dependency
#[allow(dead_code)]
pub async fn add_relay(client: &Client, relay_url: &str) -> std::result::Result<(), String> {
    if is_relay_blocked(relay_url) {
        log::info!("Skipping blocked relay: {}", relay_url);
        return Ok(());
    }

    let url = Url::parse(relay_url).map_err(|e| format!("Invalid URL: {}", e))?;

    client.add_relay(url).await.map_err(|e| e.to_string())?;

    // Update relay pool state
    let store = RELAY_POOL.read();
    let mut data = store.data();
    let mut relays = data.write();
    relays.push(RelayInfo::with_flags(
        relay_url.to_string(),
        RelayStatus::Connecting,
        true,
        true,
        RelaySource::Manual,
    ));

    log::info!("Added relay: {}", relay_url);
    Ok(())
}

/// Add a relay with specific options
///
/// # Arguments
/// * `client` - The Nostr client instance
/// * `relay_url` - URL of the relay to add
/// * `opts` - Relay configuration options
#[allow(dead_code)]
pub async fn add_relay_with_opts(
    client: &Client,
    relay_url: &str,
    opts: RelayOptions,
) -> std::result::Result<(), String> {
    if is_relay_blocked(relay_url) {
        log::info!("Skipping blocked relay: {}", relay_url);
        return Ok(());
    }

    let url = Url::parse(relay_url).map_err(|e| format!("Invalid URL: {}", e))?;

    client.pool().add_relay(url, opts).await.map_err(|e| e.to_string())?;

    // Update relay pool state
    let store = RELAY_POOL.read();
    let mut data = store.data();
    let mut relays = data.write();
    relays.push(RelayInfo::with_flags(
        relay_url.to_string(),
        RelayStatus::Connecting,
        true,
        true,
        RelaySource::Manual,
    ));

    log::info!("Added relay with opts: {}", relay_url);
    Ok(())
}

/// Remove a relay from the pool
///
/// # Arguments
/// * `client` - The Nostr client instance
/// * `relay_url` - URL of the relay to remove
///
/// IMPORTANT: Takes client as parameter to avoid circular dependency
#[allow(dead_code)]
pub async fn remove_relay(client: &Client, relay_url: &str) -> std::result::Result<(), String> {
    let url = Url::parse(relay_url).map_err(|e| format!("Invalid URL: {}", e))?;

    client.remove_relay(url).await.map_err(|e| e.to_string())?;

    // Update relay pool state
    let store = RELAY_POOL.read();
    let mut data = store.data();
    let mut relays = data.write();
    relays.retain(|r| r.url != relay_url);

    log::info!("Removed relay: {}", relay_url);
    Ok(())
}

/// Apply user's relay lists to the client connections
///
/// NOTE: With SDK gossip enabled, this function is largely redundant.
/// The SDK dynamically adds relays during routing based on NIP-65 data.
/// This function is kept for backwards compatibility and to update RELAY_POOL
/// for UI display purposes.
///
/// # Arguments
/// * `client` - The Nostr client instance
#[allow(dead_code)]
pub async fn apply_relay_lists_to_client(client: Arc<Client>) -> std::result::Result<(), String> {
    // With SDK gossip, relay routing is handled automatically.
    // This function now just updates RELAY_POOL for UI display.

    // Update RELAY_POOL to reflect connected relays
    let pool_relays = client.pool().relays().await;
    let mut relay_infos = Vec::new();

    let metadata = USER_RELAY_METADATA.read().clone();

    for (url, _relay) in pool_relays {
        let url_str = url.to_string();
        let (has_read, has_write, source) = if let Some(ref m) = metadata {
            if let Some(user_relay) = m.relays.iter().find(|r| r.url == url_str) {
                (user_relay.read, user_relay.write, RelaySource::UserNip65)
            } else if DEFAULT_RELAYS.contains(&url_str.as_str()) {
                (true, true, RelaySource::Default)
            } else {
                (true, true, RelaySource::Manual)
            }
        } else if DEFAULT_RELAYS.contains(&url_str.as_str()) {
            (true, true, RelaySource::Default)
        } else {
            (true, true, RelaySource::Manual)
        };

        relay_infos.push(RelayInfo::with_flags(
            url_str,
            RelayStatus::Connected,
            has_read,
            has_write,
            source,
        ));
    }

    log::info!("Updating RELAY_POOL with {} connected relays for UI", relay_infos.len());
    RELAY_POOL.read().data().write().clone_from(&relay_infos);

    // Apply local relays (browser-only storage)
    super::nip65::apply_local_relays_to_client(client.clone()).await;

    // Signal that user relays have been applied
    *USER_RELAYS_APPLIED.write() = true;

    log::info!("Relay pool updated for UI display");
    Ok(())
}

/// Get the default relay options
/// Reserved for future relay configuration UI
#[allow(dead_code)]
pub fn default_relay_options() -> RelayOptions {
    use std::time::Duration;

    RelayOptions::new()
        // Skip relays with average latency > 2 seconds
        .max_avg_latency(Some(Duration::from_secs(2)))
        // Verify that events match subscription filters
        .verify_subscriptions(true)
        // Ban relays that send mismatched events
        .ban_relay_on_mismatch(true)
        // Adjust retry interval based on success rate
        .adjust_retry_interval(true)
        // Initial retry interval: 10 seconds
        .retry_interval(Duration::from_secs(10))
        // Enable automatic reconnection
        .reconnect(true)
}
