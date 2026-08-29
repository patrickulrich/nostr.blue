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

/// Collect pool handles for the user's NIP-65 READ relays.
///
/// Returns an empty Vec when the user has no relay metadata or none of the
/// configured read relays are pool members — callers treat that as "nothing
/// to wait for". URL comparison is `RelayUrl`-based because `Eq` ignores
/// trailing-slash/case differences that raw string comparison would
/// false-mismatch on.
async fn user_read_relay_handles(client: &Client) -> Vec<Relay> {
    let metadata = super::nip65::USER_RELAY_METADATA.peek();
    let Some(metadata) = metadata.as_ref() else {
        return Vec::new();
    };
    let mut handles = Vec::new();
    for config in metadata.relays.iter().filter(|r| r.read) {
        let Ok(url) = nostr::RelayUrl::parse(&config.url) else {
            log::debug!("Skipping unparseable user read relay URL: {}", config.url);
            continue;
        };
        match client.relay(url).await {
            Ok(relay) => handles.push(relay),
            Err(_) => {
                // Not a pool member (blocked / failed to add) — nothing to wait on.
            }
        }
    }
    handles
}

/// Whether at least one of the user's NIP-65 read relays has a live
/// connection. Returns `true` when the user has no relay metadata or no read
/// relays in the pool (nothing specific to gate on — DEFAULT relays carry
/// the request in that case).
pub async fn any_user_read_relay_connected(client: &Client) -> bool {
    let handles = user_read_relay_handles(client).await;
    if handles.is_empty() {
        return true;
    }
    handles.iter().any(|r| r.is_connected())
}

/// Share of the user's NIP-65 read relays that must be connected before
/// the feed-readiness gate (`USER_RELAYS_APPLIED`) flips. One connected
/// relay still races one-shot `fetch_events_from` snapshots (which silently
/// skip not-yet-connected pool members), yielding sparse first loads.
pub const USER_READ_RELAY_QUORUM_FRACTION: f32 = 0.8;

/// `ceil(total * fraction)`, clamped to `[1, total]` (0 when `total` is 0).
fn quorum_required(total: usize, fraction: f32) -> usize {
    if total == 0 {
        return 0;
    }
    (((total as f32) * fraction.clamp(0.0, 1.0)).ceil() as usize).clamp(1, total)
}

/// Wait until a quorum of the user's NIP-65 read relays is connected.
///
/// Event-driven via `Relay::wait_for_connection` (no polling): races a
/// first-of-N futures set over the user's read relays. Each future carries
/// the full `timeout` because `wait_for_connection` never resolves while a
/// relay sits in `Disconnected` (only Connected/Terminated/Banned/Sleeping
/// or timeout resolve it), so the timeout is the only bound for those.
/// `wait_for_connection` also resolves on terminal states, so each winner is
/// re-checked with `is_connected()`. Returns `(total, connected)` — when the
/// quorum is not met within `timeout`, callers proceed anyway (mirroring
/// `wait_for_user_relays`).
async fn wait_user_read_relays(
    client: &Client,
    timeout: Duration,
    fraction: f32,
    context: &str,
) -> (usize, usize) {
    let handles = user_read_relay_handles(client).await;
    let total = handles.len();
    if total == 0 {
        return (0, 0);
    }
    let required = quorum_required(total, fraction);

    // Fast path: quorum already satisfied (already-connected relays also
    // resolve immediately below, so no double counting either way).
    let connected_now = handles.iter().filter(|r| r.is_connected()).count();
    if connected_now >= required {
        return (total, connected_now);
    }

    use futures::StreamExt;
    let start = instant::Instant::now();
    let mut waits: futures::stream::FuturesUnordered<_> = handles
        .into_iter()
        .map(|relay| async move {
            relay.wait_for_connection(timeout).await;
            relay
        })
        .collect();
    let mut connected = 0usize;
    while let Some(relay) = waits.next().await {
        if relay.is_connected() {
            connected += 1;
            if connected >= required {
                log::debug!(
                    "{context}: {connected}/{total} user read relays connected after {}ms (quorum met)",
                    start.elapsed().as_millis()
                );
                return (total, connected);
            }
        }
    }
    log::warn!(
        "{context}: only {connected}/{total} user read relays connected after {}ms, proceeding anyway",
        start.elapsed().as_millis()
    );
    (total, connected)
}

/// Wait until at least one of the user's NIP-65 read relays is connected.
///
/// `USER_RELAYS_APPLIED` means "relay list applied to the pool", NOT "user
/// relays connected": relays added by `init_user_relay_lists` connect
/// asynchronously after the non-blocking `client.connect()`, and a gated
/// fetch that races ahead targets only the DEFAULT relays (targeted streams
/// silently skip not-yet-connected pool members) — yielding empty-but-
/// successful results that render as "No posts yet" on cold first logins.
///
/// Returns `true` when the user has no relay metadata or no read relays in
/// the pool (nothing specific to gate on — DEFAULT relays carry the request
/// in that case).
pub async fn wait_for_user_read_relay_connected(
    client: &Client,
    timeout: Duration,
    context: &str,
) -> bool {
    // fraction 0.0 → required 1 (first-of-N semantics).
    let (total, connected) = wait_user_read_relays(client, timeout, 0.0, context).await;
    total == 0 || connected >= 1
}

/// Wait until a `fraction` (e.g. [`USER_READ_RELAY_QUORUM_FRACTION`]) of the
/// user's NIP-65 read relays are connected; returns how many are connected
/// when the quorum was met or the timeout expired. Returns 0 when the user
/// has no read relays in the pool.
pub async fn wait_for_user_read_relays_quorum(
    client: &Client,
    timeout: Duration,
    fraction: f32,
    context: &str,
) -> usize {
    wait_user_read_relays(client, timeout, fraction, context)
        .await
        .1
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
#[allow(dead_code)]
pub async fn ensure_p2p_relays_connected(client: &Client) -> Vec<String> {
    super::specialty::ensure_p2p_relays_connected(client).await
}
/// Ensure the video relay is connected
/// Delegates to specialty::ensure_video_relay
///
/// # Arguments
/// * `client` - The Nostr client instance
pub async fn ensure_video_relay_connected(client: &Client) {
    super::specialty::ensure_video_relay(client).await;
}
pub async fn ensure_radio_relay_connected(client: &Client) -> bool {
    super::specialty::ensure_radio_relay(client).await
}
/// Ensure chess relays are in the pool and connecting.
///
/// Non-blocking: adds relays to pool (idempotent) and initiates connections.
/// Actual connection status is handled by `fetch_events_from` which will
/// wait for connections as needed.
pub async fn ensure_chess_relays_connected(client: &Client) {
    let urls = crate::stores::chess::chess_config::chess_relay_urls();
    for url in &urls {
        let Ok(relay_url) = nostr::RelayUrl::parse(url) else {
            log::warn!("Invalid chess relay URL: {}", url);
            continue;
        };
        match client.add_relay(relay_url).await {
            Ok(_) => log::info!("Added chess relay to pool: {}", url),
            Err(e) if !e.to_string().contains("already") => {
                log::warn!("Failed to add chess relay {}: {}", url, e);
            }
            _ => {}
        }
    }
    let _ = client.connect().await;
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

#[cfg(test)]
mod tests {
    use super::quorum_required;

    #[test]
    fn quorum_required_rounds_up() {
        // ceil(0.8 * 5) = 4
        assert_eq!(quorum_required(5, 0.8), 4);
        // ceil(0.8 * 10) = 8
        assert_eq!(quorum_required(10, 0.8), 8);
        // ceil(0.8 * 3) = 3 — small sets round up to near-totality
        assert_eq!(quorum_required(3, 0.8), 3);
    }

    #[test]
    fn quorum_required_clamps_to_one_and_total() {
        assert_eq!(quorum_required(0, 0.8), 0);
        assert_eq!(quorum_required(1, 0.8), 1);
        assert_eq!(quorum_required(7, 0.8), 6);
        // fraction 0.0 = first-of-N semantics → at least 1
        assert_eq!(quorum_required(10, 0.0), 1);
        assert_eq!(quorum_required(4, 1.0), 4);
    }
}
