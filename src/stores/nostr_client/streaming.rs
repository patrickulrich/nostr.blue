//! Progressive event streaming
//!
//! Functions for streaming events with callbacks for progressive UI updates.
use super::fetching::{ensure_relays_ready, fetch_events_aggregated_outbox, get_client};
use super::platform_sleep_ms;
use super::signals::HAS_SIGNER;
use crate::error::NostrBlueError;
use crate::stores::relay::USER_RELAYS_APPLIED;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;
/// Stream events progressively with a callback for each event
///
/// Unlike fetch_events which waits for all events, this function calls the
/// provided callback as each event arrives, enabling progressive UI updates.
///
/// # Arguments
/// * `filter` - The filter to use for the subscription
/// * `timeout` - Maximum duration to wait for events
/// * `on_event` - Callback invoked for each event received
///
/// # Returns
/// Total count of events received
// TODO: remove dead_code allow when video/article feeds use per-event streaming
#[allow(dead_code)]
pub async fn stream_events_with_callback<F>(
    filter: Filter,
    timeout: std::time::Duration,
    mut on_event: F,
) -> std::result::Result<usize, String>
where
    F: FnMut(nostr::Event),
{
    use futures::StreamExt;
    let client = get_client().ok_or("Client not initialized")?;
    crate::stores::relay::wait_for_user_relays(
        Duration::from_millis(500),
        "stream_events_with_callback",
    )
    .await;
    ensure_relays_ready(&client).await;
    let mut stream = client
        .stream_events(filter, timeout)
        .await
        .map_err(|e| format!("Failed to create event stream: {}", e))?;
    let mut count = 0;
    while let Some(event) = stream.next().await {
        on_event(event);
        count += 1;
    }
    log::info!("Stream completed: received {} events", count);
    Ok(count)
}
/// Stream events with gossip routing, calling a callback for each batch
///
/// This function is optimized for progressive UI updates. It:
/// 1. Waits for user relay lists to be applied (like fetch_events_aggregated_outbox)
/// 2. Streams events as they arrive
/// 3. Calls the callback with batches of events for efficient UI updates
///
/// # Arguments
/// * `filter` - The filter to use for the subscription
/// * `timeout` - Maximum duration to wait for events
/// * `batch_size` - Number of events to collect before calling on_batch
/// * `on_batch` - Callback invoked with each batch of events
///
/// # Returns
/// Total count of events received
// TODO: remove dead_code allow when batch streaming is used for feed loading
#[allow(dead_code)]
pub async fn stream_events_batched<F>(
    filter: Filter,
    timeout: std::time::Duration,
    batch_size: usize,
    mut on_batch: F,
) -> std::result::Result<usize, String>
where
    F: FnMut(Vec<nostr::Event>),
{
    use futures::StreamExt;
    if batch_size == 0 {
        return Err("batch_size must be greater than 0".to_string());
    }
    let client = get_client().ok_or("Client not initialized")?;
    crate::stores::relay::wait_for_user_relays(
        Duration::from_millis(500),
        "stream_events_with_batches",
    )
    .await;
    ensure_relays_ready(&client).await;
    let filter_authors = filter.authors.clone();
    let author_set: Option<std::collections::HashSet<_>> = filter_authors
        .as_ref()
        .map(|authors| authors.iter().collect());
    let mut stream = client
        .stream_events(filter, timeout)
        .await
        .map_err(|e| format!("Failed to create event stream: {}", e))?;
    let mut accepted_count = 0;
    let mut filtered_count = 0;
    let mut batch = Vec::with_capacity(batch_size);
    while let Some(event) = stream.next().await {
        if let Some(ref authors) = author_set {
            if !authors.contains(&event.pubkey) {
                filtered_count += 1;
                continue;
            }
        }
        batch.push(event);
        accepted_count += 1;
        if batch.len() >= batch_size {
            let items = std::mem::take(&mut batch);
            batch.reserve(batch_size);
            on_batch(items);
        }
    }
    if !batch.is_empty() {
        on_batch(batch);
    }
    if filtered_count > 0 {
        log::info!(
            "Stream completed: {} accepted events ({} filtered out from non-matching authors)",
            accepted_count,
            filtered_count
        );
    } else {
        log::info!(
            "Stream completed: received {} events in batches",
            accepted_count
        );
    }
    Ok(accepted_count)
}
/// Stream events from connected relays only (bypasses gossip discovery)
///
/// FAST alternative to stream_events_batched that:
/// 1. Only queries already-connected relays (no relay discovery)
/// 2. Bypasses the gossip model - no NIP-65 lookups per author
/// 3. Returns results much faster but may miss events from unconnected relays
///
/// Use for initial feed load where speed is critical.
pub async fn stream_events_from_connected_relays_batched<F>(
    filter: Filter,
    timeout: std::time::Duration,
    batch_size: usize,
    mut on_batch: F,
) -> std::result::Result<usize, String>
where
    F: FnMut(Vec<nostr::Event>),
{
    use futures::StreamExt;
    use nostr_relay_pool::RelayStatus as PoolRelayStatus;
    if batch_size == 0 {
        return Err("batch_size must be greater than 0".to_string());
    }
    let client = get_client().ok_or("Client not initialized")?;
    ensure_relays_ready(&client).await;
    let relays = client.relays().await;
    let connected_urls: Vec<nostr::RelayUrl> = relays
        .iter()
        .filter(|(_, r)| r.status() == PoolRelayStatus::Connected)
        .filter_map(|(url, _)| nostr::RelayUrl::parse(url.as_str()).ok())
        .collect();
    if connected_urls.is_empty() {
        log::warn!("No connected relays, falling back to gossip stream");
        return stream_events_batched(filter, timeout, batch_size, on_batch).await;
    }
    log::info!(
        "Fast streaming from {} connected relays (bypassing gossip)",
        connected_urls.len()
    );
    let filter_authors = filter.authors.clone();
    let author_set: Option<std::collections::HashSet<_>> = filter_authors
        .as_ref()
        .map(|authors| authors.iter().collect());
    let mut stream = client
        .stream_events_from(connected_urls, filter, timeout)
        .await
        .map_err(|e| format!("Failed to create stream: {}", e))?;
    let mut accepted_count = 0;
    let mut filtered_count = 0;
    let mut batch = Vec::with_capacity(batch_size);
    while let Some(event) = stream.next().await {
        if let Some(ref authors) = author_set {
            if !authors.contains(&event.pubkey) {
                filtered_count += 1;
                continue;
            }
        }
        batch.push(event);
        accepted_count += 1;
        if batch.len() >= batch_size {
            let items = std::mem::take(&mut batch);
            batch.reserve(batch_size);
            on_batch(items);
        }
    }
    if !batch.is_empty() {
        on_batch(batch);
    }
    if filtered_count > 0 {
        log::info!(
            "Fast stream completed: {} accepted events ({} filtered out from non-followed authors)",
            accepted_count,
            filtered_count
        );
    } else {
        log::info!(
            "Fast stream completed: {} events from connected relays",
            accepted_count
        );
    }
    Ok(accepted_count)
}
/// Stream events with immediate callback per event (no batching)
///
/// Optimized for time-to-first-post: calls on_event immediately for each event
/// as it arrives from relays, enabling instant UI updates.
///
/// Use for initial feed load where displaying the first post ASAP is critical.
pub async fn stream_events_immediate<F>(
    filter: Filter,
    timeout: std::time::Duration,
    mut on_event: F,
) -> std::result::Result<usize, NostrBlueError>
where
    F: FnMut(nostr::Event),
{
    use futures::StreamExt;
    use nostr_relay_pool::RelayStatus as PoolRelayStatus;

    let client = get_client().ok_or(NostrBlueError::Other("Client not initialized".into()))?;

    // Wait for user relay lists if signer is present
    crate::stores::relay::wait_for_user_relays(
        Duration::from_millis(500),
        "stream_events_immediate",
    )
    .await;
    ensure_relays_ready(&client).await;

    let filter_authors = filter.authors.clone();
    let author_set: Option<std::collections::HashSet<_>> = filter_authors
        .as_ref()
        .map(|authors| authors.iter().collect());

    // Targeted streaming from connected relays, with recovery for the
    // snapshot/removal race: the SDK's `stream_events_targeted` re-acquires
    // the pool lock after our `client.relays()` snapshot (an await point),
    // and if ANY snapshotted relay was removed from the pool in that window
    // (e.g. ephemeral-rescue cleanup force-removing its relays) the ENTIRE
    // call fails with `RelayNotFound` — one removed relay poisons all
    // targets. On that error, retry once with a fresh snapshot (the
    // surviving relays); if the retry fails or nothing is connected, fall
    // back to the gossip stream instead of failing the page load.
    let mut stream = None;
    for attempt in 1..=2 {
        let relays = client.relays().await;
        let connected_urls: Vec<nostr::RelayUrl> = relays
            .iter()
            .filter(|(_, r)| r.status() == PoolRelayStatus::Connected)
            .filter_map(|(url, _)| nostr::RelayUrl::parse(url.as_str()).ok())
            .collect();

        if connected_urls.is_empty() {
            if attempt == 1 {
                log::warn!("No connected relays for immediate stream, falling back to gossip stream");
            }
            break;
        }

        log::info!(
            "Immediate streaming from {} connected relays",
            connected_urls.len()
        );

        match client
            .stream_events_from(connected_urls, filter.clone(), timeout)
            .await
        {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) if is_relay_not_found(&e) && attempt == 1 => {
                log::warn!(
                    "Immediate stream raced a pool removal (RelayNotFound); retrying with surviving relays"
                );
            }
            Err(e) if is_relay_not_found(&e) => {
                log::warn!(
                    "Immediate stream RelayNotFound persisted; falling back to gossip stream"
                );
                break;
            }
            Err(e) => return Err(e.into()),
        }
    }

    // Gossip fallback (or the won targeted stream).
    let mut stream = match stream {
        Some(s) => s,
        None => client.stream_events(filter, timeout).await?,
    };

    let mut count = 0;
    while let Some(event) = stream.next().await {
        if let Some(ref authors) = author_set {
            if !authors.contains(&event.pubkey) {
                continue;
            }
        }
        on_event(event);
        count += 1;
    }

    log::info!("Immediate stream completed: {} events", count);
    Ok(count)
}

/// Whether a client error is the pool's `RelayNotFound` — the signature of
/// the snapshot/removal race in `stream_events_immediate` (a relay removed
/// between the `client.relays()` snapshot and the SDK's internal lock
/// re-acquisition). Matched typed first; the string form is a defensive
/// secondary in case an intermediate layer re-wraps the error.
fn is_relay_not_found(e: &nostr_sdk::client::Error) -> bool {
    matches!(
        e,
        nostr_sdk::client::Error::RelayPool(nostr_relay_pool::pool::Error::RelayNotFound)
    ) || e.to_string().contains("relay not found")
}

/// Stream profile events by merging multiple relay sources simultaneously.
///
/// Fires queries to ALL of these sources in parallel and merges results with
/// EventId deduplication (mirrors the SDK's own merge pattern at
/// `pool/mod.rs:1267-1313` and wisp's multi-source fan-out):
///
/// 1. **Connected pool relays** (`stream_events_from`): immediate events from
///    damus.io, nos.lol, the user's read relays, etc. Starts streaming within
///    milliseconds — finds the author's content if it happens to be on any
///    general relay.
/// 2. **SDK gossip** (`stream_events`): discovers the author's NIP-65 write
///    relays automatically using our DISCOVERY-flagged indexers for relay-list
///    lookup. Takes 3–10s on cold start (relay discovery); near-instant if the
///    gossip store already has the relay list (from prior kind 10002 ingestion
///    within the 60-min freshness window). Finds content that connected relays
///    don't have.
/// 3. **Targeted outbox relays** (when `write_relay_urls` is non-empty):
///    connects ephemerally to the author's resolved write relays and streams
///    directly. Fastest when relays are already known (from Effect 3's
///    background resolution).
///
/// Events from whichever source delivers first paint immediately; other sources
/// fill in additional events as they arrive. This is the same pattern amethyst
/// and wisp use: fire to everything available, merge with dedup.
///
/// Returns the total number of unique events delivered to `on_event`.
pub async fn stream_profile_events_from_relays<F>(
    filter: Filter,
    write_relay_urls: &[String],
    timeout: std::time::Duration,
    mut on_event: F,
) -> std::result::Result<usize, NostrBlueError>
where
    F: FnMut(nostr::Event),
{
    use futures::StreamExt;
    use nostr_relay_pool::RelayStatus as PoolRelayStatus;

    let client = get_client().ok_or(NostrBlueError::Other("Client not initialized".into()))?;

    // Gates: ensure relays are connected and user relay lists are applied.
    crate::stores::relay::wait_for_user_relays(
        std::time::Duration::from_millis(500),
        "stream_profile_events",
    )
    .await;
    ensure_relays_ready(&client).await;

    // Extract author set for client-side filtering (defense-in-depth against
    // misbehaving relays that ignore filter authors).
    let filter_authors = filter.authors.clone();
    let author_set: Option<std::collections::HashSet<nostr::PublicKey>> = filter_authors
        .as_ref()
        .map(|authors| authors.iter().copied().collect());

    // Multi-source merge channel (matches SDK's pool/mod.rs:1269 capacity).
    let (tx, mut rx) = tokio::sync::mpsc::channel::<nostr::Event>(512);

    // ── Source 1: Connected pool relays (immediate) ──
    {
        let relays = client.relays().await;
        let connected_urls: Vec<nostr::RelayUrl> = relays
            .iter()
            .filter(|(_, r)| r.status() == PoolRelayStatus::Connected)
            .filter_map(|(url, _)| nostr::RelayUrl::parse(url.as_str()).ok())
            .collect();
        if !connected_urls.is_empty() {
            let tx = tx.clone();
            let client = client.clone();
            let filter = filter.clone();
            crate::platform::spawn::spawn_catch_unwind(
                "profile_connected_stream",
                async move {
                    log::info!(
                        "Profile connected-relays stream: {} relays",
                        connected_urls.len()
                    );
                    match client
                        .stream_events_from(connected_urls, filter, timeout)
                        .await
                    {
                        Ok(mut stream) => {
                            while let Some(event) = stream.next().await {
                                if tx.send(event).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(e) => log::warn!("Connected-relays stream failed: {}", e),
                    }
                },
            );
        }
    }

    // ── Source 2: SDK gossip (discovers author's write relays via NIP-65) ──
    {
        let tx = tx.clone();
        let client = client.clone();
        let filter = filter.clone();
        crate::platform::spawn::spawn_catch_unwind("profile_gossip_stream", async move {
            log::info!("Profile gossip stream starting");
            match client.stream_events(filter, timeout).await {
                Ok(mut stream) => {
                    while let Some(event) = stream.next().await {
                        if tx.send(event).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => log::warn!("Gossip stream failed: {}", e),
            }
        });
    }

    // ── Source 3 (optional): Targeted outbox relays if known ──
    if !write_relay_urls.is_empty() {
        let tx = tx.clone();
        let client = client.clone();
        let filter = filter.clone();
        let urls = write_relay_urls.to_vec();
        crate::platform::spawn::spawn_catch_unwind("profile_outbox_stream", async move {
            let ephemeral =
                crate::stores::relay::coverage::connect_ephemeral_relays(&client, &urls).await;
            if ephemeral.connected.is_empty() {
                return;
            }
            let connected: Vec<nostr::RelayUrl> = ephemeral
                .connected
                .iter()
                .filter_map(|u| nostr::RelayUrl::parse(u.as_str()).ok())
                .collect();
            log::info!("Profile outbox stream: {} relays", connected.len());
            match client.stream_events_from(connected, filter, timeout).await {
                Ok(mut stream) => {
                    while let Some(event) = stream.next().await {
                        if tx.send(event).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => log::warn!("Outbox stream failed: {}", e),
            }
        });
    }

    drop(tx); // All senders spawned; drop original so rx ends when all finish.

    // ── Consumer: merge with EventId dedup + author filter ──
    let mut seen: std::collections::HashSet<nostr::EventId> = std::collections::HashSet::new();
    let mut count = 0;
    while let Some(event) = rx.recv().await {
        if let Some(ref authors) = author_set {
            if !authors.contains(&event.pubkey) {
                continue;
            }
        }
        if seen.insert(event.id) {
            on_event(event);
            count += 1;
        }
    }

    log::info!("Profile multi-source stream completed: {} unique events", count);
    Ok(count)
}
/// Stream video events from connected relays with divine relay awareness
///
/// Ensures the video relay (relay.divine.video) is connected first,
/// then delegates to `stream_events_from_connected_relays_batched`.
pub async fn stream_video_events_from_connected_relays_batched<F>(
    filter: Filter,
    timeout: std::time::Duration,
    batch_size: usize,
    on_batch: F,
) -> std::result::Result<usize, String>
where
    F: FnMut(Vec<nostr::Event>),
{
    let client = get_client().ok_or("Client not initialized")?;
    crate::stores::relay::ensure_video_relay_connected(&client).await;
    stream_events_from_connected_relays_batched(filter, timeout, batch_size, on_batch).await
}
/// Stream events and collect them into a Vec
///
/// This is a convenience wrapper that collects all streamed events
/// into a vector with deduplication and sorting.
/// Uses HashSet for O(1) deduplication during collection (nostr-sdk pattern).
// TODO: remove dead_code allow when collected stream is used for prefetching
#[allow(dead_code)]
pub async fn stream_events_collected(
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    use futures::StreamExt;
    use std::collections::HashSet;
    let client = get_client().ok_or("Client not initialized")?;
    let mut stream = client
        .stream_events(filter, timeout)
        .await
        .map_err(|e| format!("Failed to create event stream: {}", e))?;
    let mut seen_ids: HashSet<nostr::EventId> = HashSet::new();
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        if seen_ids.insert(event.id) {
            events.push(event);
        }
    }
    events.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    log::info!("Stream completed: collected {} unique events", events.len());
    Ok(events)
}
