//! Relay pool management
//!
//! Functions for adding, removing, and managing relays in the pool.
//! All functions take `client: &Client` as parameter to avoid circular dependencies.
use super::signals::RelayPoolStoreStoreExt;
use super::signals::{RelayInfo, RelaySource, RelayStatus, RELAY_POOL, USER_RELAYS_APPLIED};
use crate::stores::relay::nip65::USER_RELAY_METADATA;
use dioxus::prelude::WritableExt;
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::sync::Arc;
/// Check if a relay URL is in the blocked list
pub fn is_relay_blocked(url: &str) -> bool {
    let blocked = super::nip65::BLOCKED_RELAYS.peek();
    let normalized = url.trim_end_matches('/');
    blocked
        .iter()
        .any(|blocked_url| blocked_url.trim_end_matches('/') == normalized)
}
/// Remove any connected relays that are in the blocked list
/// Call this after NIP-51 blocked relay list is loaded to clean up
/// any relays that were added before the blocked list was available.
pub async fn remove_blocked_relays_from_pool(client: &Client) {
    let blocked = super::nip65::BLOCKED_RELAYS.peek().clone();
    if blocked.is_empty() {
        return;
    }
    let relays = client.relays().await;
    for (url, _) in relays {
        let url_str = url.to_string();
        let normalized = url_str.trim_end_matches('/');
        if blocked
            .iter()
            .any(|b| b.trim_end_matches('/') == normalized)
        {
            log::info!("Removing blocked relay from pool: {}", url_str);
            let _ = client.remove_relay(url).await;
        }
    }
    let store = RELAY_POOL.read();
    let mut data = store.data();
    let mut relays = data.write();
    relays.retain(|r| {
        let normalized = r.url.trim_end_matches('/');
        !blocked
            .iter()
            .any(|b| b.trim_end_matches('/') == normalized)
    });
    log::debug!(
        "Pruned blocked relays from RELAY_POOL, {} remaining",
        relays.len()
    );
}
/// Reset relay pool to default relays only, removing all user-specific relays.
/// Call this on logout to prevent relay accumulation across account switches.
pub async fn reset_pool_to_defaults(client: &Client) {
    let relays = client.relays().await;
    let mut removed_count = 0usize;
    for (url, _) in relays {
        let url_str = url.to_string();
        let normalized = url_str.trim_end_matches('/');
        if DEFAULT_RELAYS
            .iter()
            .any(|d| d.trim_end_matches('/') == normalized)
        {
            continue;
        }
        log::debug!("Removing user relay on logout: {}", url_str);
        let _ = client.force_remove_relay(url).await;
        removed_count += 1;
    }
    *USER_RELAY_METADATA.write() = None;
    let store = RELAY_POOL.read();
    let mut data = store.data();
    let mut pool = data.write();
    pool.retain(|r| {
        DEFAULT_RELAYS
            .iter()
            .any(|d| d.trim_end_matches('/') == r.url.trim_end_matches('/'))
    });
    log::info!(
        "Reset relay pool to defaults: removed {} user relays, {} defaults remaining",
        removed_count,
        pool.len()
    );
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
    let url = RelayUrl::parse(relay_url).map_err(|e| format!("Invalid relay URL: {}", e))?;
    client.add_relay(url).await.map_err(|e| e.to_string())?;
    let store = RELAY_POOL.read();
    let mut data = store.data();
    let mut relays = data.write();
    let normalized_url = relay_url.trim_end_matches('/');
    let exists = relays
        .iter()
        .any(|r| r.url.trim_end_matches('/') == normalized_url);
    if !exists {
        relays.push(RelayInfo::with_flags(
            relay_url.to_string(),
            RelayStatus::Connecting,
            true,
            true,
            RelaySource::Manual,
        ));
    }
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
    let url = RelayUrl::parse(relay_url).map_err(|e| format!("Invalid relay URL: {}", e))?;
    client
        .pool()
        .add_relay(url, opts)
        .await
        .map_err(|e| e.to_string())?;
    let store = RELAY_POOL.read();
    let mut data = store.data();
    let mut relays = data.write();
    let normalized_url = relay_url.trim_end_matches('/');
    let exists = relays
        .iter()
        .any(|r| r.url.trim_end_matches('/') == normalized_url);
    if !exists {
        relays.push(RelayInfo::with_flags(
            relay_url.to_string(),
            RelayStatus::Connecting,
            true,
            true,
            RelaySource::Manual,
        ));
    }
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
    let url = RelayUrl::parse(relay_url).map_err(|e| format!("Invalid relay URL: {}", e))?;
    client.remove_relay(url).await.map_err(|e| e.to_string())?;
    let store = RELAY_POOL.read();
    let mut data = store.data();
    let mut relays = data.write();
    let normalized_url = relay_url.trim_end_matches('/');
    relays.retain(|r| r.url.trim_end_matches('/') != normalized_url);
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
    log::info!(
        "Updating RELAY_POOL with {} connected relays for UI",
        relay_infos.len()
    );
    RELAY_POOL.read().data().write().clone_from(&relay_infos);
    super::nip65::apply_local_relays_to_client(client.clone()).await;
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
        .max_avg_latency(Some(Duration::from_secs(2)))
        .verify_subscriptions(true)
        .ban_relay_on_mismatch(true)
        .adjust_retry_interval(true)
        .reconnect(true)
}
