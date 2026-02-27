//! Specialty relay management
//!
//! Provides unified handling for specialty relays (video, GIF, DVM, etc.)
//! using nostr-sdk's built-in relay management APIs.
//!
//! # Design
//!
//! Specialty relays are relays that host specific content types (video, GIFs, etc.)
//! and need to be added to the pool temporarily or for the session. This module
//! provides consistent patterns for:
//!
//! - Adding relays temporarily and tracking which were newly added
//! - Ensuring relays are connected before querying
//! - Cleaning up temporary relays after use
use nostr_sdk::prelude::*;
use std::time::Duration;
/// Well-known specialty relay URLs
pub mod urls {
    pub const VIDEO: &str = "wss://relay.divine.video";
    pub const GIF: &str = "wss://relay.gifbuddy.lol";
}
/// Default options for specialty relays
fn specialty_relay_options() -> RelayOptions {
    RelayOptions::new().max_avg_latency(Some(Duration::from_secs(2))).reconnect(true)
}
/// Add relays temporarily, returning which ones were newly added.
/// Uses SDK's add_relay() which returns Ok if added successfully.
pub async fn add_relays(client: &Client, relay_urls: &[RelayUrl]) -> Vec<RelayUrl> {
    let mut added = Vec::new();
    for relay_url in relay_urls {
        match client.add_relay(relay_url.clone()).await {
            Ok(_) => {
                log::debug!("Added temporary relay: {}", relay_url);
                added.push(relay_url.clone());
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("already") {
                    log::debug!("Relay already existed: {}", relay_url);
                } else {
                    log::debug!("Could not add relay {}: {}", relay_url, e);
                }
            }
        }
    }
    added
}
/// Add relays from string URLs, returning which ones were newly added.
pub async fn add_relays_from_strings(client: &Client, urls: &[String]) -> Vec<RelayUrl> {
    let relay_urls: Vec<RelayUrl> = urls
        .iter()
        .filter_map(|u| RelayUrl::parse(u).ok())
        .collect();
    add_relays(client, &relay_urls).await
}
/// Remove specified relays from the pool.
pub async fn remove_relays(client: &Client, relays: &[RelayUrl]) {
    for relay_url in relays {
        if let Err(e) = client.remove_relay(relay_url.clone()).await {
            log::debug!("Could not remove relay {}: {}", relay_url, e);
        }
    }
}
/// Check which of the given relays are actually connected.
#[allow(dead_code)]
pub async fn get_connected(client: &Client, relay_urls: &[RelayUrl]) -> Vec<RelayUrl> {
    let relays = client.relays().await;
    relay_urls
        .iter()
        .filter(|r| {
            relays.get(*r).map(|relay| relay.is_connected()).unwrap_or(false)
        })
        .cloned()
        .collect()
}
/// Ensure a specialty relay is connected (session-persistent).
/// Adds the relay if not present and waits for connection.
pub async fn ensure_connected(client: &Client, relay_url: &str) -> bool {
    let Ok(url) = nostr::Url::parse(relay_url) else {
        log::warn!("Invalid relay URL: {}", relay_url);
        return false;
    };
    let relays = client.relays().await;
    if let Some((_, relay)) = relays.iter().find(|(u, _)| u.as_str() == relay_url) {
        if relay.status() == nostr_relay_pool::RelayStatus::Connected {
            return true;
        }
        log::info!(
            "Specialty relay exists but not connected, connecting: {}", relay_url
        );
    } else {
        let opts = specialty_relay_options();
        if let Err(e) = client.pool().add_relay(url.clone(), opts).await {
            if !e.to_string().contains("already") {
                log::warn!("Failed to add specialty relay {}: {}", relay_url, e);
                return false;
            }
        }
        log::info!("Added specialty relay: {}", relay_url);
    }
    if let Err(e) = client.pool().connect_relay(url.clone()).await {
        log::warn!("Failed to connect to specialty relay {}: {}", relay_url, e);
        return false;
    }
    for _ in 0..20 {
        let relays = client.relays().await;
        if let Some((_, relay)) = relays.iter().find(|(u, _)| u.as_str() == relay_url) {
            if relay.status() == nostr_relay_pool::RelayStatus::Connected {
                log::info!("Specialty relay connected: {}", relay_url);
                return true;
            }
        }
        #[cfg(feature = "web")]
        crate::platform::timer::sleep_ms(100).await;
        #[cfg(not(feature = "web"))]
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    log::warn!("Specialty relay connection timeout: {}", relay_url);
    false
}
/// Ensure video relay is connected (session-persistent).
pub async fn ensure_video_relay(client: &Client) -> bool {
    ensure_connected(client, urls::VIDEO).await
}
/// Ensure GIF relay is connected (session-persistent).
pub async fn ensure_gif_relay(client: &Client) -> bool {
    ensure_connected(client, urls::GIF).await
}
/// Ensure DM inbox relays are connected with privacy-respecting fallback.
///
/// Fallback behavior:
/// - If user configured kind 10050 relays: ONLY use those (privacy-first)
/// - If no 10050 configured: fallback to 10002 → defaults (UX-first)
///
/// This follows NIP-17 privacy requirements: users choose 10050 relays specifically
/// for DM privacy, so we shouldn't leak DM subscriptions to other relays.
///
/// Returns the list of relay URLs that successfully connected.
pub async fn ensure_dm_relays_connected(client: &Client) -> Vec<String> {
    // Tier 1: Try kind 10050 DM relays
    let dm_relays_10050 = super::nip65::get_dm_relays_10050_only();
    let user_configured_dm_relays = !dm_relays_10050.is_empty();

    if user_configured_dm_relays {
        log::info!("Trying {} kind 10050 DM relays...", dm_relays_10050.len());
        let connected = try_connect_relay_list(client, &dm_relays_10050).await;
        if !connected.is_empty() {
            log::info!(
                "Connected to {} kind 10050 DM relays: {:?}",
                connected.len(),
                connected
            );
            return connected;
        }
        // User explicitly configured DM relays - don't fallback (privacy-first per NIP-17)
        log::warn!(
            "No kind 10050 relays connected. Skipping fallback to preserve DM privacy. \
             User configured {} DM relays but none are reachable.",
            dm_relays_10050.len()
        );
        return Vec::new();
    }

    // No 10050 configured - user hasn't set DM relay preferences, fallback is OK
    log::info!("No kind 10050 relays configured, trying kind 10002...");

    // Tier 2: Fall back to kind 10002 write relays
    let write_relays = super::nip65::get_write_relays();
    if !write_relays.is_empty() {
        log::info!("Trying {} kind 10002 write relays...", write_relays.len());
        let connected = try_connect_relay_list(client, &write_relays).await;
        if !connected.is_empty() {
            log::info!(
                "Connected to {} kind 10002 write relays for DMs: {:?}",
                connected.len(),
                connected
            );
            return connected;
        }
        log::warn!("No kind 10002 relays connected, falling back to defaults...");
    } else {
        log::info!("No kind 10002 relays configured, trying defaults...");
    }

    // Tier 3: Fall back to hardcoded defaults
    let defaults = super::nip65::default_dm_relays();
    log::info!("Trying {} default DM relays...", defaults.len());
    let connected = try_connect_relay_list(client, &defaults).await;
    if !connected.is_empty() {
        log::info!(
            "Connected to {} default DM relays: {:?}",
            connected.len(),
            connected
        );
        return connected;
    }

    log::error!("No DM relays could be connected from any tier!");
    Vec::new()
}

/// Maximum concurrent relay connection attempts to prevent overwhelming WASM event loop
const MAX_CONCURRENT_RELAY_CONNECTIONS: usize = 5;

/// Helper to force connection to a list of relays in parallel with bounded concurrency.
/// Uses ensure_connected which adds to pool + waits for connection.
async fn try_connect_relay_list(client: &Client, relays: &[String]) -> Vec<String> {
    use futures::stream::{self, StreamExt};

    stream::iter(relays.iter().cloned())
        .map(|relay_url| {
            let client = client.clone();
            async move {
                if ensure_connected(&client, &relay_url).await {
                    Some(relay_url)
                } else {
                    None
                }
            }
        })
        .buffer_unordered(MAX_CONCURRENT_RELAY_CONNECTIONS)
        .filter_map(|result| async { result })
        .collect()
        .await
}
/// Ensure search relays are connected (session-persistent).
/// Adds user's search relays (kind 10007) or defaults to the pool and connects them.
/// Returns the list of relay URLs that successfully connected.
pub async fn ensure_search_relays_connected(client: &Client) -> Vec<String> {
    use dioxus::prelude::ReadableExt;
    let search_relays = {
        let relays = super::nip65::SEARCH_RELAYS.peek().clone();
        if relays.is_empty() { super::nip65::default_search_relays() } else { relays }
    };
    let mut connected = Vec::new();
    for relay_url in &search_relays {
        if ensure_connected(client, relay_url).await {
            connected.push(relay_url.clone());
        } else {
            log::warn!("Could not connect to search relay: {}", relay_url);
        }
    }
    if connected.is_empty() {
        log::error!("No search relays could be connected!");
    } else {
        log::info!(
            "Search relays connected: {}/{} - {:?}", connected.len(), search_relays
            .len(), connected
        );
    }
    connected
}
