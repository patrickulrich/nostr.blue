//! Progressive event streaming
//!
//! Functions for streaming events with callbacks for progressive UI updates.
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;
use super::fetching::{ensure_relays_ready, fetch_events_aggregated_outbox, get_client};
use super::platform_sleep_ms;
use super::signals::HAS_SIGNER;
use crate::stores::relay::USER_RELAYS_APPLIED;
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
    if *HAS_SIGNER.peek() && !*USER_RELAYS_APPLIED.peek() {
        log::debug!("Streaming callback: Waiting for user relay lists...");
        let start = instant::Instant::now();
        while !*USER_RELAYS_APPLIED.peek() && start.elapsed() < Duration::from_secs(2) {
            platform_sleep_ms(50).await;
        }
        if *USER_RELAYS_APPLIED.peek() {
            log::debug!(
                "Streaming callback: User relays applied after {}ms", start.elapsed()
                .as_millis()
            );
        } else {
            log::warn!("Streaming callback: User relays not applied after timeout");
        }
    }
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
    if *HAS_SIGNER.peek() && !*USER_RELAYS_APPLIED.peek() {
        log::debug!("Streaming: Waiting for user relay lists to be applied...");
        let start = instant::Instant::now();
        while !*USER_RELAYS_APPLIED.peek() && start.elapsed() < Duration::from_secs(2) {
            platform_sleep_ms(50).await;
        }
        if *USER_RELAYS_APPLIED.peek() {
            log::debug!(
                "Streaming: User relay lists applied after {}ms", start.elapsed()
                .as_millis()
            );
        } else {
            log::warn!(
                "Streaming: User relay lists not applied after timeout, proceeding with defaults"
            );
        }
    }
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
            accepted_count, filtered_count
        );
    } else {
        log::info!("Stream completed: received {} events in batches", accepted_count);
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
#[allow(dead_code)]
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
        "Fast streaming from {} connected relays (bypassing gossip)", connected_urls
        .len()
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
            accepted_count, filtered_count
        );
    } else {
        log::info!(
            "Fast stream completed: {} events from connected relays", accepted_count
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
) -> std::result::Result<usize, String>
where
    F: FnMut(nostr::Event),
{
    use futures::StreamExt;
    use nostr_relay_pool::RelayStatus as PoolRelayStatus;

    let client = get_client().ok_or("Client not initialized")?;
    ensure_relays_ready(&client).await;

    let relays = client.relays().await;
    let connected_urls: Vec<nostr::RelayUrl> = relays
        .iter()
        .filter(|(_, r)| r.status() == PoolRelayStatus::Connected)
        .filter_map(|(url, _)| nostr::RelayUrl::parse(url.as_str()).ok())
        .collect();

    if connected_urls.is_empty() {
        log::warn!("No connected relays for immediate stream");
        return Err("No connected relays".to_string());
    }

    log::info!(
        "Immediate streaming from {} connected relays",
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
/// Stream events and collect them into a Vec
///
/// This is a convenience wrapper that collects all streamed events
/// into a vector with deduplication and sorting.
/// Uses HashSet for O(1) deduplication during collection (nostr-sdk pattern).
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
    events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    log::info!("Stream completed: collected {} unique events", events.len());
    Ok(events)
}
