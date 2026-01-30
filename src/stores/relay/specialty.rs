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
        #[cfg(target_arch = "wasm32")]
        gloo_timers::future::TimeoutFuture::new(100).await;
        #[cfg(not(target_arch = "wasm32"))]
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
/// Ensure DM inbox relays are connected (session-persistent).
/// Adds user's DM relays (kind 10050) or defaults to the pool and connects them.
/// Returns the list of relay URLs that successfully connected.
pub async fn ensure_dm_relays_connected(client: &Client) -> Vec<String> {
    let dm_relays = super::nip65::get_dm_relays();
    let mut connected = Vec::new();
    for relay_url in &dm_relays {
        if ensure_connected(client, relay_url).await {
            connected.push(relay_url.clone());
        } else {
            log::warn!("Could not connect to DM relay: {}", relay_url);
        }
    }
    if connected.is_empty() {
        log::error!("No DM relays could be connected!");
    } else {
        log::info!(
            "DM relays connected: {}/{} - {:?}", connected.len(), dm_relays.len(),
            connected
        );
    }
    connected
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
