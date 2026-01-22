//! Relay connection management
//!
//! Functions for managing relay connections and fetching events from specific relays.
//! All functions take `client: &Client` as parameter to avoid circular dependencies.

use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;

use super::signals::RELAY_CONNECTED;

/// Wait for at least one relay to be ready before fetching
/// This is needed because connect() is non-blocking and spawns background tasks
/// Ensure at least one relay is connected before fetching
/// Call this before any direct client.fetch_events() calls
///
/// IMPORTANT: Uses a timeout to prevent blocking the WASM event loop indefinitely.
/// In WASM, blocking connect() calls can freeze the entire UI.
///
/// # Arguments
/// * `client` - The Nostr client instance
///
/// IMPORTANT: Takes client as parameter to avoid circular dependency
pub async fn ensure_relays_ready(client: &Client) {
    use nostr_relay_pool::RelayStatus as PoolRelayStatus;

    // First, check if any relay is already connected
    let relays = client.relays().await;
    let any_connected = relays.values().any(|r| r.status() == PoolRelayStatus::Connected);

    if any_connected {
        log::debug!("At least one relay is already connected, proceeding with fetch");
        // Ensure signal is set for any code checking relay readiness
        if !*RELAY_CONNECTED.peek() {
            *RELAY_CONNECTED.write() = true;
        }
        return;
    }

    // No relays connected yet - initiate connection and poll for status
    log::info!("No relays connected, attempting connection with timeout...");

    // Initiate connection (non-blocking, spawns background tasks)
    client.connect().await;

    // Poll for at least one connected relay with timeout
    const TIMEOUT_MS: u64 = 3000;
    const POLL_INTERVAL_MS: u64 = 100;

    #[cfg(target_arch = "wasm32")]
    {
        use gloo_timers::future::TimeoutFuture;
        let start = instant::Instant::now();

        loop {
            let relays_now = client.relays().await;
            let connected = relays_now.values().any(|r| r.status() == PoolRelayStatus::Connected);

            if connected {
                log::info!("Relay connected after {}ms", start.elapsed().as_millis());
                // Signal relay connection for reactive retry triggers (one-time)
                if !*RELAY_CONNECTED.peek() {
                    *RELAY_CONNECTED.write() = true;
                }
                return;
            }

            if start.elapsed().as_millis() > TIMEOUT_MS as u128 {
                break;
            }

            TimeoutFuture::new(POLL_INTERVAL_MS as u32).await;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(TIMEOUT_MS);

        loop {
            let relays_now = client.relays().await;
            let connected = relays_now.values().any(|r| r.status() == PoolRelayStatus::Connected);

            if connected {
                log::info!("Relay connected after {:?}", start.elapsed());
                // Signal relay connection for reactive retry triggers (one-time)
                if !*RELAY_CONNECTED.peek() {
                    *RELAY_CONNECTED.write() = true;
                }
                return;
            }

            if start.elapsed() > timeout {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
        }
    }

    // Final status check
    let relays_after = client.relays().await;
    let connected_count = relays_after.values().filter(|r| r.status() == PoolRelayStatus::Connected).count();
    if connected_count == 0 {
        log::warn!("After timeout: no relays connected - fetches may fail or use cached data");
    } else {
        log::info!("After connection attempt: {} relay(s) connected", connected_count);
        // Signal relay connection for reactive retry triggers (one-time)
        if !*RELAY_CONNECTED.peek() {
            *RELAY_CONNECTED.write() = true;
        }
    }
}

/// Reset the RELAY_CONNECTED signal to false
///
/// Call this when disconnecting from relays to ensure components
/// that depend on relay connectivity state are notified.
#[allow(dead_code)]
pub fn reset_relay_connected() {
    *RELAY_CONNECTED.write() = false;
}

/// Disconnect from all relays
///
/// Resets RELAY_CONNECTED signal to false so components can react
/// to the disconnection and retry when relays reconnect.
///
/// # Arguments
/// * `client` - The Nostr client instance
#[allow(dead_code)]
pub async fn disconnect(client: &Client) {
    client.disconnect().await;
    // Reset signal so components can react when relays reconnect
    *RELAY_CONNECTED.write() = false;
    log::info!("Disconnected from all relays");
}

/// Reconnect to all relays
///
/// # Arguments
/// * `client` - The Nostr client instance
#[allow(dead_code)]
pub async fn reconnect(client: &Client) {
    client.connect().await;
    log::info!("Reconnected to relays");
}

/// Fetch events from specific relays (for privacy-sensitive queries like DMs)
/// Uses SDK's fetch_events_from() which supports targeted relay queries
///
/// This function is designed for scenarios where you want to limit which relays
/// see your query, such as:
/// - DM fetching (only query DM inbox relays)
/// - Private content (only query specific relays)
///
/// # Arguments
/// * `client` - The Nostr client instance
/// * `filter` - Filter describing events to fetch
/// * `relay_urls` - List of relay URLs to query (only these relays will see the query)
/// * `timeout` - Maximum time to wait for responses
///
/// # Returns
/// List of events matching the filter from the specified relays
///
/// IMPORTANT: Takes client as parameter to avoid circular dependency
pub async fn fetch_events_from_relays(
    client: &Client,
    filter: Filter,
    relay_urls: Vec<String>,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let urls: Vec<nostr::RelayUrl> = relay_urls
        .iter()
        .filter_map(|r| nostr::RelayUrl::parse(r).ok())
        .collect();

    if urls.is_empty() {
        return Err("No valid relay URLs provided for targeted fetch".to_string());
    }

    log::info!("Fetching events from {} specific relays: {:?}", urls.len(), urls);

    // SDK's fetch_events_from(urls, filters, timeout) - fetches only from specified relays
    // Internally uses ReqExitPolicy::ExitOnEOSE
    client.fetch_events_from(urls, filter, timeout).await
        .map(|events| {
            let events: Vec<_> = events.into_iter().collect();
            log::info!("Received {} events from targeted relay fetch", events.len());
            events
        })
        .map_err(|e| format!("Failed to fetch from specific relays: {}", e))
}

/// Ensure the video relay is connected
/// Delegates to specialty::ensure_video_relay
///
/// # Arguments
/// * `client` - The Nostr client instance
pub async fn ensure_video_relay_connected(client: &Client) {
    super::specialty::ensure_video_relay(client).await;
}

/// Fetch addressable event by coordinate with relay hints
/// Two-phase loading: DB first (instant), then relay (if not found)
pub async fn fetch_event_by_coordinate_with_relays(
    client: &Client,
    kind: u16,
    pubkey: &str,
    identifier: &str,
    relay_hints: Vec<String>,
) -> Result<Option<nostr::Event>, String> {
    use nostr::{Filter, Kind, PublicKey};

    let author = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::parse(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::from(kind))
        .author(author)
        .identifier(identifier.to_string())
        .limit(1);

    // PHASE 1: Check database first (instant)
    if let Ok(db_events) = client.database().query(filter.clone()).await {
        if let Some(event) = db_events.into_iter().next() {
            log::debug!("Found event kind {} in DB: {}:{}", kind, pubkey, identifier);
            return Ok(Some(event));
        }
    }

    log::info!("Fetching event kind {} from relay: {}:{}", kind, pubkey, identifier);

    // PHASE 2: Fetch from relays with hints
    if !relay_hints.is_empty() {
        let added = super::specialty::add_relays_from_strings(client, &relay_hints).await;

        // Try fetching with shorter timeout
        let fetch_result = client.fetch_events(filter.clone(), Duration::from_secs(5)).await;

        // Clean up temporary hint relays to avoid polluting the relay pool
        if !added.is_empty() {
            super::specialty::remove_relays(client, &added).await;
        }

        if let Ok(events) = fetch_result {
            if let Some(event) = events.into_iter().next() {
                return Ok(Some(event));
            }
        }
    }

    // Fallback: standard relay fetch
    ensure_relays_ready(client).await;

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => Ok(events.into_iter().next()),
        Err(e) => {
            log::error!("Failed to fetch event: {}", e);
            Err(format!("Failed to fetch event: {}", e))
        }
    }
}
