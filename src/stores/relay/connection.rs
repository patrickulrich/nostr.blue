//! Relay connection management
//!
//! Functions for managing relay connections and fetching events from specific relays.
//! All functions take `client: &Client` as parameter to avoid circular dependencies.
use super::signals::{RELAY_CONNECTED, USER_RELAYS_APPLIED};
use dioxus::signals::ReadableExt;
use nostr_relay_pool::RelayStatus as PoolRelayStatus;
use nostr_sdk::prelude::*;
use std::time::Duration;

/// Process the result of client.try_connect(), updating RELAY_CONNECTED and logging.
/// Returns true if at least one relay connected.
fn handle_connect_output(output: &Output<()>, log_prefix: &str) -> bool {
    let success_count = output.success.len();
    let failed_count = output.failed.len();
    if success_count > 0 {
        log::info!("{log_prefix}Connected to {success_count} relay(s), {failed_count} failed");
        if !*RELAY_CONNECTED.peek() {
            *RELAY_CONNECTED.write() = true;
        }
        true
    } else {
        log::warn!("{log_prefix}No relays connected after timeout ({failed_count} failed)");
        if *RELAY_CONNECTED.peek() {
            *RELAY_CONNECTED.write() = false;
        }
        false
    }
}

/// Fast relay connection using SDK's try_connect() with timeout
///
/// Uses the SDK's `try_connect()` method which attempts connection within the
/// timeout without spawning background retry tasks on failure. This is faster
/// for initial connection attempts than the polling-based approach.
///
/// # Arguments
/// * `client` - The Nostr client instance
/// * `timeout` - Maximum time to wait for connection
///
/// # Returns
/// `true` if at least one relay connected, `false` if timeout was reached
pub async fn try_connect_relays(client: &Client, timeout: Duration) -> bool {
    // Check if already connected
    let relays = client.relays().await;
    let any_connected = relays
        .values()
        .any(|r| r.status() == PoolRelayStatus::Connected);

    if any_connected {
        log::debug!("[Fast connect] Already connected to at least one relay");
        if !*RELAY_CONNECTED.peek() {
            *RELAY_CONNECTED.write() = true;
        }
        return true;
    }

    log::info!(
        "[Fast connect] Attempting relay connection with {:?} timeout...",
        timeout
    );

    // Use SDK's try_connect which attempts connection with timeout
    // This is faster than spawning connect() and then polling
    // Returns Output<()> with success/failed relay lists
    let output = client.try_connect(timeout).await;
    handle_connect_output(&output, "[Fast connect] ")
}

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
    let relays = client.relays().await;
    let any_connected = relays
        .values()
        .any(|r| r.status() == PoolRelayStatus::Connected);
    if any_connected {
        log::debug!("At least one relay is already connected, proceeding with fetch");
        if !*RELAY_CONNECTED.peek() {
            *RELAY_CONNECTED.write() = true;
        }
        return;
    }
    log::info!("No relays connected, attempting connection with timeout...");
    let output = client.try_connect(Duration::from_secs(3)).await;
    handle_connect_output(&output, "");
}

/// Wait for USER_RELAYS_APPLIED signal, polling at 50ms intervals.
/// Only waits if HAS_SIGNER is true (NIP-46 timing issue).
/// Returns true if relays were applied within timeout, false if timed out.
pub async fn wait_for_user_relays(timeout: Duration, context: &str) -> bool {
    if !*crate::stores::nostr_client::HAS_SIGNER.peek() || *USER_RELAYS_APPLIED.peek() {
        return true;
    }
    log::debug!("{context}: waiting for user relay lists...");
    let start = instant::Instant::now();
    while !*USER_RELAYS_APPLIED.peek() && start.elapsed() < timeout {
        crate::stores::nostr_client::platform_sleep_ms(50).await;
    }
    let applied = *USER_RELAYS_APPLIED.peek();
    if applied {
        log::debug!(
            "{context}: user relay lists applied after {}ms",
            start.elapsed().as_millis()
        );
    } else {
        log::warn!("{context}: proceeding without user relay lists after timeout");
    }
    applied
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
    *RELAY_CONNECTED.write() = false;
    log::info!("Disconnected from all relays");
}
/// Reconnect to all relays
///
/// Initiates connection and polls for at least one connected relay.
///
/// # Arguments
/// * `client` - The Nostr client instance
///
/// # Returns
/// true if at least one relay connected, false otherwise
#[allow(dead_code)]
pub async fn reconnect(client: &Client) -> bool {
    client.connect().await;
    const TIMEOUT_MS: u64 = 3000;
    const POLL_INTERVAL_MS: u64 = 100;
    #[cfg(feature = "web")]
    {
        let start = instant::Instant::now();
        loop {
            let relays = client.relays().await;
            let connected = relays
                .values()
                .any(|r| r.status() == PoolRelayStatus::Connected);
            if connected {
                log::info!(
                    "Reconnected to relays successfully after {}ms",
                    start.elapsed().as_millis()
                );
                if !*RELAY_CONNECTED.peek() {
                    *RELAY_CONNECTED.write() = true;
                }
                return true;
            }
            if start.elapsed().as_millis() > TIMEOUT_MS as u128 {
                break;
            }
            crate::platform::timer::sleep_ms(POLL_INTERVAL_MS as u32).await;
        }
    }
    #[cfg(not(feature = "web"))]
    {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(TIMEOUT_MS);
        loop {
            let relays = client.relays().await;
            let connected = relays
                .values()
                .any(|r| r.status() == PoolRelayStatus::Connected);
            if connected {
                log::info!(
                    "Reconnected to relays successfully after {:?}",
                    start.elapsed()
                );
                if !*RELAY_CONNECTED.peek() {
                    *RELAY_CONNECTED.write() = true;
                }
                return true;
            }
            if start.elapsed() > timeout {
                break;
            }
            tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
        }
    }
    let relays = client.relays().await;
    let connected = relays
        .values()
        .any(|r| r.status() == PoolRelayStatus::Connected);
    if connected {
        log::info!("Reconnected to relays successfully");
        if !*RELAY_CONNECTED.peek() {
            *RELAY_CONNECTED.write() = true;
        }
    } else {
        log::warn!("Reconnect attempt: no relays connected after timeout");
    }
    connected
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
    log::info!(
        "Fetching events from {} specific relays: {:?}",
        urls.len(),
        urls
    );
    client
        .fetch_events_from(urls, filter, timeout)
        .await
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
pub async fn ensure_radio_relay_connected(client: &Client) {
    super::specialty::ensure_radio_relay(client).await;
}
pub async fn ensure_chess_relays_connected(client: &Client) {
    let urls = crate::stores::chess::chess_config::chess_relay_urls();
    for url in &urls {
        super::specialty::ensure_connected(client, url).await;
    }
}
/// Fetch addressable event by coordinate with relay hints using 5-phase targeted strategy.
///
/// 1. DB check (instant)
/// 2. Broadcast to connected relays (fast, 3s)
/// 3. Relay hints from naddr (ephemeral connections, cleaned up)
/// 4. Author relays via NIP-65 resolution (ephemeral connections, cleaned up)
/// 5. Gossip/outbox fallback (last resort)
pub async fn fetch_event_by_coordinate_with_relays(
    client: &std::sync::Arc<Client>,
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

    // Phase 1: DB check (instant)
    if let Ok(db_events) = client.database().query(filter.clone()).await {
        if let Some(event) = db_events.into_iter().next() {
            log::debug!(
                "fetch_event_by_coordinate: found kind {} in DB: {}:{}",
                kind, pubkey, identifier
            );
            return Ok(Some(event));
        }
    }

    log::info!(
        "fetch_event_by_coordinate: fetching kind {} {}:{}",
        kind, pubkey, identifier
    );

    // Phase 2: Broadcast to connected relays (fast, 3s)
    match crate::stores::nostr_client::fetching::fetch_events_from_connected_relays_with_client(
        client,
        filter.clone(),
        Duration::from_secs(3),
    )
    .await
    {
        Ok(events) if !events.is_empty() => {
            log::debug!("fetch_event_by_coordinate: found via broadcast");
            return Ok(events.into_iter().next());
        }
        _ => {}
    }

    // Phase 3: Relay hints from naddr (ephemeral)
    if !relay_hints.is_empty() {
        let ephemeral =
            super::coverage::connect_ephemeral_relays(client, &relay_hints).await;
        if !ephemeral.connected.is_empty() {
            let result = fetch_events_from_relays(
                client,
                filter.clone(),
                ephemeral.connected.clone(),
                Duration::from_secs(10),
            )
            .await;
            super::coverage::cleanup_ephemeral_relays(client, &ephemeral.newly_added).await;
            if let Ok(events) = result {
                if let Some(event) = events.into_iter().next() {
                    log::debug!("fetch_event_by_coordinate: found via relay hints");
                    return Ok(Some(event));
                }
            }
        }
    }

    // Phase 4: Author relays via NIP-65 (ephemeral)
    let author_relay_urls = super::coverage::resolve_user_relays(
        pubkey,
        super::coverage::RelayPurpose::Write,
    )
    .await;
    let ephemeral =
        super::coverage::connect_ephemeral_relays(client, &author_relay_urls).await;
    if !ephemeral.connected.is_empty() {
        let result = fetch_events_from_relays(
            client,
            filter.clone(),
            ephemeral.connected.clone(),
            Duration::from_secs(10),
        )
        .await;
        super::coverage::cleanup_ephemeral_relays(client, &ephemeral.newly_added).await;
        if let Ok(events) = result {
            if let Some(event) = events.into_iter().next() {
                log::debug!("fetch_event_by_coordinate: found via author relays");
                return Ok(Some(event));
            }
        }
    }

    // Phase 5: Gossip/outbox fallback
    match crate::stores::nostr_client::fetching::fetch_events_aggregated_outbox_with_client(
        client,
        filter,
        Duration::from_secs(10),
    )
    .await
    {
        Ok(events) => {
            if events.is_empty() {
                log::debug!("fetch_event_by_coordinate: not found (all phases exhausted)");
            } else {
                log::debug!("fetch_event_by_coordinate: found via gossip fallback");
            }
            Ok(events.into_iter().next())
        }
        Err(e) => {
            log::error!("fetch_event_by_coordinate: gossip fallback failed: {}", e);
            Err(format!("Failed to fetch event: {}", e))
        }
    }
}
