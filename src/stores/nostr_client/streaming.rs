//! Progressive event streaming
//!
//! Functions for streaming events with callbacks for progressive UI updates.

use std::time::Duration;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

use super::fetching::{get_client, ensure_relays_ready, fetch_events_aggregated_outbox};
use super::platform_sleep_ms;
use super::signals::HAS_SIGNER;
use crate::stores::relay::USER_RELAYS_APPLIED;

// =============================================================================
// Callback-Based Streaming
// =============================================================================

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

    // Wait for user relays if signed in (up to 2 seconds)
    // This ensures gossip routing uses the user's configured relays
    if *HAS_SIGNER.peek() && !*USER_RELAYS_APPLIED.peek() {
        log::debug!("Streaming callback: Waiting for user relay lists...");
        let start = instant::Instant::now();

        // Use shared platform sleep helper (Dioxus pattern: no duplicated cfg blocks)
        while !*USER_RELAYS_APPLIED.peek() && start.elapsed() < Duration::from_secs(2) {
            platform_sleep_ms(50).await;
        }

        if *USER_RELAYS_APPLIED.peek() {
            log::debug!("Streaming callback: User relays applied after {}ms", start.elapsed().as_millis());
        } else {
            log::warn!("Streaming callback: User relays not applied after timeout");
        }
    }

    ensure_relays_ready(&client).await;

    let mut stream = client.stream_events(filter, timeout)
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

// =============================================================================
// Batched Streaming
// =============================================================================

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

    // Wait for user relays if signed in (up to 2 seconds)
    // This ensures gossip routing uses the user's configured relays
    if *HAS_SIGNER.peek() && !*USER_RELAYS_APPLIED.peek() {
        log::debug!("Streaming: Waiting for user relay lists to be applied...");
        let start = instant::Instant::now();

        // Use shared platform sleep helper (Dioxus pattern: no duplicated cfg blocks)
        while !*USER_RELAYS_APPLIED.peek() && start.elapsed() < Duration::from_secs(2) {
            platform_sleep_ms(50).await;
        }

        if *USER_RELAYS_APPLIED.peek() {
            log::debug!("Streaming: User relay lists applied after {}ms", start.elapsed().as_millis());
        } else {
            log::warn!("Streaming: User relay lists not applied after timeout, proceeding with defaults");
        }
    }

    // Wait for at least one relay to be ready
    ensure_relays_ready(&client).await;

    // Capture authors for client-side filtering (defense-in-depth)
    // Must be before stream_events consumes the filter
    let filter_authors = filter.authors.clone();
    let author_set: Option<std::collections::HashSet<_>> = filter_authors.as_ref()
        .map(|authors| authors.iter().collect());

    let mut stream = client.stream_events(filter, timeout)
        .await
        .map_err(|e| format!("Failed to create event stream: {}", e))?;

    let mut total_count = 0;
    let mut filtered_count = 0;
    let mut batch = Vec::with_capacity(batch_size);

    while let Some(event) = stream.next().await {
        // Client-side author filtering (defense-in-depth against misbehaving relays)
        if let Some(ref authors) = author_set {
            if !authors.contains(&event.pubkey) {
                filtered_count += 1;
                continue;  // Skip events from non-matching authors
            }
        }

        batch.push(event);
        total_count += 1;

        // Deliver batch when we reach batch_size
        if batch.len() >= batch_size {
            let items = std::mem::take(&mut batch);
            batch.reserve(batch_size);
            on_batch(items);
        }
    }

    // Deliver any remaining events
    if !batch.is_empty() {
        on_batch(batch);
    }

    if filtered_count > 0 {
        log::info!("Stream completed: {} events ({} filtered out from non-matching authors)", total_count, filtered_count);
    } else {
        log::info!("Stream completed: received {} events in batches", total_count);
    }
    Ok(total_count)
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

    // Get connected relay URLs
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

    log::info!("Fast streaming from {} connected relays (bypassing gossip)", connected_urls.len());

    // Capture authors for client-side filtering (defense-in-depth)
    // Relays may return events from any author, ignoring the filter
    let filter_authors = filter.authors.clone();
    let author_set: Option<std::collections::HashSet<_>> = filter_authors.as_ref()
        .map(|authors| authors.iter().collect());

    // Use stream_events_from which bypasses gossip entirely
    let mut stream = client
        .stream_events_from(connected_urls, filter, timeout)
        .await
        .map_err(|e| format!("Failed to create stream: {}", e))?;

    let mut total_count = 0;
    let mut filtered_count = 0;
    let mut batch = Vec::with_capacity(batch_size);

    while let Some(event) = stream.next().await {
        // Client-side author filtering (defense-in-depth against misbehaving relays)
        if let Some(ref authors) = author_set {
            if !authors.contains(&event.pubkey) {
                filtered_count += 1;
                continue;  // Skip events from non-followed authors
            }
        }

        batch.push(event);
        total_count += 1;

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
        log::info!("Fast stream completed: {} events ({} filtered out from non-followed authors)", total_count, filtered_count);
    } else {
        log::info!("Fast stream completed: {} events from connected relays", total_count);
    }
    Ok(total_count)
}

// =============================================================================
// Collected Streaming
// =============================================================================

/// Stream events and collect them into a Vec
///
/// This is a convenience wrapper that collects all streamed events
/// into a vector with deduplication and sorting.
#[allow(dead_code)]
pub async fn stream_events_collected(
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    use futures::StreamExt;

    let client = get_client().ok_or("Client not initialized")?;

    let mut stream = client.stream_events(filter, timeout)
        .await
        .map_err(|e| format!("Failed to create event stream: {}", e))?;

    let mut events = Vec::new();

    while let Some(event) = stream.next().await {
        events.push(event);
    }

    // Deduplicate by event ID (events may come from multiple relays)
    events.sort_by(|a, b| a.id.cmp(&b.id));
    events.dedup_by(|a, b| a.id == b.id);

    // Sort by created_at descending (newest first)
    events.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    log::info!("Stream completed: collected {} unique events", events.len());
    Ok(events)
}
