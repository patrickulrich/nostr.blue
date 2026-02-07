//! Event fetching strategies
//!
//! Functions for fetching events from relays and database with various patterns:
//! - Aggregated (database first, relay background sync)
//! - Direct relay fetch (bypassing cache)
//! - Outbox routing (NIP-65 gossip)
//! - Connected-only fetch (fast, bypasses gossip discovery)
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;
use super::signals::{HAS_SIGNER, NOSTR_CLIENT};
use crate::stores::relay;
use crate::stores::relay::USER_RELAYS_APPLIED;
/// Get the current client instance
pub(crate) fn get_client() -> Option<std::sync::Arc<Client>> {
    NOSTR_CLIENT.read().clone()
}
/// Wait for at least one relay to be ready before fetching
pub(crate) async fn ensure_relays_ready(client: &Client) {
    relay::connection::ensure_relays_ready(client).await;
}
/// Ensure the video relay is connected
pub(crate) async fn ensure_video_relay_connected(client: &Client) {
    relay::connection::ensure_video_relay_connected(client).await;
}
/// Fetch events using aggregated pattern: database first, then relays
///
/// This function:
/// 1. Queries local IndexedDB cache first (instant)
/// 2. If cache hit, returns immediately and syncs in background
/// 3. If cache miss, fetches from relays
pub async fn fetch_events_aggregated(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    fetch_events_aggregated_with_client(&client, filter, timeout).await
}
/// Internal: aggregated fetch using provided client (avoids re-reading NOSTR_CLIENT)
///
/// Dioxus pattern: Get client once via OnceLock/get_or_init, pass same instance
/// through all async operations. No locks held across await points.
async fn fetch_events_aggregated_with_client(
    client: &std::sync::Arc<Client>,
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    match client.database().query(filter.clone()).await {
        Ok(db_events) => {
            let db_count = db_events.len();
            if db_count > 0 {
                log::info!("Loaded {} events from IndexedDB cache", db_count);
                let client_clone = client.clone();
                let filter_clone = filter.clone();
                dioxus::prelude::spawn(async move {
                    relay::connection::ensure_relays_ready(&client_clone).await;
                    if let Err(e) = client_clone
                        .fetch_events(filter_clone, timeout)
                        .await
                    {
                        log::warn!("Background relay sync failed: {}", e);
                    }
                });
                return Ok(db_events.into_iter().collect());
            }
        }
        Err(e) => {
            log::warn!("Database query failed: {}, falling back to relays", e);
        }
    }
    log::info!("Fetching from relays (database empty or failed)");
    relay::connection::ensure_relays_ready(client).await;
    client
        .fetch_events(filter, timeout)
        .await
        .map(|events| events.into_iter().collect())
        .map_err(|e| e.to_string())
}
/// Fetch video events, ensuring relay.divine.video is included
///
/// This function adds the video-specific relay to the pool before fetching,
/// ensuring video content is discovered from the Divine relay in addition
/// to relays selected via the outbox model.
pub async fn fetch_video_events(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    ensure_video_relay_connected(&client).await;
    fetch_events_aggregated_with_client(&client, filter, timeout).await
}
/// Fetch events directly from relays, bypassing cache
///
/// Use this for discovery features where fresh data from the network is needed.
/// Results are still stored in the database for future caching.
pub async fn fetch_events_from_relays(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    ensure_relays_ready(&client).await;
    let relays = client.relays().await;
    let connected_count = relays
        .iter()
        .filter(|(_, r)| r.status() == nostr_relay_pool::RelayStatus::Connected)
        .count();
    log::info!("fetch_events_from_relays: {} relays connected", connected_count);
    let result = client
        .fetch_events(filter.clone(), timeout)
        .await
        .map(|events| {
            let events: Vec<_> = events.into_iter().collect();
            log::info!("fetch_events_from_relays: received {} events", events.len());
            events
        })
        .map_err(|e| {
            log::error!("fetch_events_from_relays error: {}", e);
            e.to_string()
        });
    result
}
/// Fetch events using gossip (automatic relay routing)
///
/// This function waits for user relay lists (kind 10002) to be applied before
/// fetching, ensuring gossip routing uses the correct relays for signed-in users.
pub async fn fetch_events_aggregated_outbox(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    fetch_events_aggregated_outbox_with_client(&client, filter, timeout).await
}
/// Internal: Fetch events using gossip with provided client (avoids re-reading NOSTR_CLIENT)
///
/// Dioxus pattern: Get client once, pass same instance through all async operations.
async fn fetch_events_aggregated_outbox_with_client(
    client: &std::sync::Arc<Client>,
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    crate::stores::relay::wait_for_user_relays(
        Duration::from_millis(500), "fetch_events_aggregated_outbox"
    ).await;
    ensure_relays_ready(client).await;
    let filter_authors = filter.authors.clone();
    let events = client
        .fetch_events(filter, timeout)
        .await
        .map_err(|e| format!("Failed to fetch events: {}", e))?;
    let filtered_events: Vec<nostr::Event> = if let Some(ref authors) = filter_authors {
        let author_set: std::collections::HashSet<_> = authors.iter().collect();
        events.into_iter().filter(|e| author_set.contains(&e.pubkey)).collect()
    } else {
        events.into_iter().collect()
    };
    Ok(filtered_events)
}
/// Fetch events from database only (instant, for initial display)
///
/// This is Phase 1 of profile loading - shows cached data immediately.
/// Call `fetch_profile_events_from_relays` afterward for fresh data.
pub async fn fetch_profile_events_db(
    filter: Filter,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    match client.database().query(filter).await {
        Ok(events) => {
            let count = events.len();
            log::info!("Profile DB: loaded {} events instantly", count);
            Ok(events.into_iter().collect())
        }
        Err(e) => {
            log::warn!("Profile DB query failed: {}", e);
            Ok(Vec::new())
        }
    }
}
/// Fetch events from relays only (for background refresh)
///
/// This is Phase 2 of profile loading - fetches fresh data from relays.
/// Uses gossip/outbox routing for efficient relay selection.
pub async fn fetch_profile_events_from_relays(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    ensure_relays_ready(&client).await;
    match client.fetch_events(filter, timeout).await {
        Ok(events) => {
            let count = events.len();
            log::info!("Profile relays: fetched {} events", count);
            Ok(events.into_iter().collect())
        }
        Err(e) => {
            log::warn!("Profile relay fetch failed: {}", e);
            Err(format!("Relay fetch failed: {}", e))
        }
    }
}
/// Fetch events from connected relays only (bypasses gossip discovery)
///
/// FAST alternative to fetch_events_aggregated_outbox for pagination:
/// 1. Only queries already-connected relays (no relay discovery)
/// 2. Bypasses the gossip model - no NIP-65 lookups per author
/// 3. Returns results much faster but may miss events from unconnected relays
///
/// Includes client-side author filtering for defense-in-depth against
/// misbehaving relays that ignore filter authors.
pub async fn fetch_events_from_connected_relays(
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    fetch_events_from_connected_relays_with_client(&client, filter, timeout).await
}
/// Internal: connected relays fetch using provided client (avoids re-reading NOSTR_CLIENT)
///
/// Dioxus pattern: Get client once, pass same instance through all async operations.
/// No locks held across await points.
async fn fetch_events_from_connected_relays_with_client(
    client: &std::sync::Arc<Client>,
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    use nostr_relay_pool::RelayStatus as PoolRelayStatus;
    relay::connection::ensure_relays_ready(client).await;
    let relays = client.relays().await;
    let connected_urls: Vec<nostr::RelayUrl> = relays
        .iter()
        .filter(|(_, r)| r.status() == PoolRelayStatus::Connected)
        .filter_map(|(url, _)| nostr::RelayUrl::parse(url.as_str()).ok())
        .collect();
    if connected_urls.is_empty() {
        log::warn!("No connected relays, falling back to gossip fetch");
        return fetch_events_aggregated_outbox_with_client(
                client,
                filter.clone(),
                timeout,
            )
            .await;
    }
    log::info!(
        "Fast fetching from {} connected relays (bypassing gossip)", connected_urls.len()
    );
    let filter_authors = filter.authors.clone();
    let author_set: Option<std::collections::HashSet<_>> = filter_authors
        .as_ref()
        .map(|authors| authors.iter().collect());
    let events = client
        .fetch_events_from(connected_urls, filter, timeout)
        .await
        .map_err(|e| format!("Failed to fetch events: {}", e))?;
    let result: Vec<nostr::Event> = events
        .into_iter()
        .filter(|event| {
            if let Some(ref authors) = author_set {
                authors.contains(&event.pubkey)
            } else {
                true
            }
        })
        .collect();
    log::info!("Fast fetch completed: {} events (after filtering)", result.len());
    Ok(result)
}
/// Fetch video events from connected relays (bypasses gossip)
///
/// Ensures video relay (relay.divine.video) is connected first,
/// then uses fast fetch (bypasses gossip) for the query.
pub async fn fetch_video_events_from_connected_relays(
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    ensure_video_relay_connected(&client).await;
    fetch_events_from_connected_relays_with_client(&client, filter, timeout).await
}
