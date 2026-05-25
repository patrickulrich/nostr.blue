//! Event fetching strategies
//!
//! Functions for fetching events from relays and database with various patterns:
//! - Aggregated (database first, relay background sync)
//! - Direct relay fetch (bypassing cache)
//! - Outbox routing (NIP-65 gossip)
//! - Connected-only fetch (fast, bypasses gossip discovery)
use super::signals::{HAS_SIGNER, NOSTR_CLIENT};
use crate::stores::relay;
use crate::stores::relay::USER_RELAYS_APPLIED;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;
/// Get the current client instance
pub(crate) fn get_client() -> Option<std::sync::Arc<Client>> {
    NOSTR_CLIENT.read().clone()
}
/// Get the current client instance without subscribing to changes.
/// Use in spawn/async contexts to avoid creating reactive subscriptions.
#[allow(dead_code)]
pub(crate) fn get_client_peek() -> Option<std::sync::Arc<Client>> {
    NOSTR_CLIENT.peek().clone()
}
/// Wait for at least one relay to be ready before fetching
pub(crate) async fn ensure_relays_ready(client: &Client) {
    relay::connection::ensure_relays_ready(client).await;
}
/// Ensure the video relay is connected
#[allow(dead_code)]
pub(crate) async fn ensure_video_relay_connected(client: &Client) {
    relay::connection::ensure_video_relay_connected(client).await;
}
pub(crate) async fn ensure_radio_relay_connected(client: &Client) {
    relay::connection::ensure_radio_relay_connected(client).await;
}
pub(crate) async fn ensure_chess_relays_connected(client: &Client) {
    relay::connection::ensure_chess_relays_connected(client).await;
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
                    if let Err(e) = client_clone.fetch_events(filter_clone, timeout).await {
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
#[allow(dead_code)]
pub async fn fetch_video_events(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    ensure_video_relay_connected(&client).await;
    fetch_events_aggregated_with_client(&client, filter, timeout).await
}
/// Fetch chess events: DB-first for fast paint, then always refresh from chess relays.
///
/// 1. Query IndexedDB cache → return immediately for fast UI if found
/// 2. Always fetch fresh from chess relays (wss://relay.damus.io, etc.)
/// 3. Merge new relay events with DB events
/// 4. Return combined result (new events auto-saved to DB by nostr-sdk)
pub async fn fetch_chess_events(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    ensure_chess_relays_connected(&client).await;

    let mut seen_ids: std::collections::HashSet<nostr::EventId> = std::collections::HashSet::new();
    let mut all_events: Vec<nostr::Event> = vec![];

    // 1. DB first (fast paint)
    if let Ok(db_events) = client.database().query(filter.clone()).await {
        if !db_events.is_empty() {
            log::info!("Chess DB cache: {} events", db_events.len());
            let db_vec: Vec<nostr::Event> = db_events.into_iter().collect();
            for ev in &db_vec {
                seen_ids.insert(ev.id);
            }
            all_events = db_vec;
        }
    }

    // 2. Always fetch fresh from chess relays
    let chess_urls: Vec<RelayUrl> = crate::utils::nips::chess::CHESS_RELAYS
        .iter()
        .filter_map(|u| RelayUrl::parse(u).ok())
        .collect();

    match client.fetch_events_from(chess_urls, filter, timeout).await {
        Ok(relay_events) => {
            let mut new_count = 0;
            for ev in relay_events {
                if seen_ids.insert(ev.id) {
                    new_count += 1;
                    all_events.push(ev);
                }
            }
            if new_count > 0 {
                log::info!("Chess relay fetch: {} new events merged", new_count);
            }
        }
        Err(e) => {
            log::warn!("Chess relay fetch failed: {} (returning {} DB events)", e, all_events.len());
        }
    }

    Ok(all_events)
}
/// Fetch radio events directly from relays, bypassing the aggregated cache.
///
/// The aggregated cache pattern (`fetch_events_aggregated_with_client`) returns
/// stale IndexedDB data immediately and spawns a background relay sync whose
/// results are never propagated to the UI. For radio, this means once a single
/// event is cached, only that one station shows forever.
///
/// Instead, this always fetches fresh from relays. Events are still saved to
/// IndexedDB automatically by nostr-sdk during relay message processing.
pub async fn fetch_radio_events(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    ensure_radio_relay_connected(&client).await;
    relay::connection::ensure_relays_ready(&client).await;

    let mut events: Vec<_> = client
        .fetch_events(filter.clone(), timeout)
        .await
        .map(|events| events.into_iter().collect())
        .map_err(|e| e.to_string())?;

    if events.is_empty() {
        log::info!("Radio fetch returned 0 events, waiting for relay and retrying...");
        crate::platform::timer::sleep_ms(3000).await;
        ensure_radio_relay_connected(&client).await;
        relay::connection::ensure_relays_ready(&client).await;
        events = client
            .fetch_events(filter, timeout)
            .await
            .map(|events| events.into_iter().collect())
            .map_err(|e| e.to_string())?;
    }
    Ok(events)
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
    log::info!(
        "fetch_events_from_relays: {} relays connected",
        connected_count
    );
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
pub(crate) async fn fetch_events_aggregated_outbox_with_client(
    client: &std::sync::Arc<Client>,
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    crate::stores::relay::wait_for_user_relays(
        Duration::from_millis(500),
        "fetch_events_aggregated_outbox",
    )
    .await;
    ensure_relays_ready(client).await;
    let filter_authors = filter.authors.clone();
    let events = client
        .fetch_events(filter, timeout)
        .await
        .map_err(|e| format!("Failed to fetch events: {}", e))?;
    let filtered_events: Vec<nostr::Event> = if let Some(ref authors) = filter_authors {
        let author_set: std::collections::HashSet<_> = authors.iter().collect();
        events
            .into_iter()
            .filter(|e| author_set.contains(&e.pubkey))
            .collect()
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

/// Fetch profile events from pre-resolved relay URLs, skipping relay discovery.
/// Falls back to generic relay fetch if targeted fetch returns empty.
pub async fn fetch_profile_events_from_relays_direct(
    client: &std::sync::Arc<Client>,
    filter: Filter,
    relay_urls: &[String],
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    if relay_urls.is_empty() {
        return fetch_profile_events_from_relays(filter, timeout).await;
    }
    let ephemeral = relay::coverage::connect_ephemeral_relays(client, relay_urls).await;
    if !ephemeral.connected.is_empty() {
        let result = relay::connection::fetch_events_from_relays(
            client,
            filter.clone(),
            ephemeral.connected.clone(),
            timeout,
        )
        .await;
        relay::coverage::cleanup_ephemeral_relays(client, &ephemeral.newly_added).await;
        if let Ok(events) = result {
            if !events.is_empty() {
                return Ok(events);
            }
        }
    }
    fetch_profile_events_from_relays(filter, timeout).await
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
pub(crate) async fn fetch_events_from_connected_relays_with_client(
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
        return fetch_events_aggregated_outbox_with_client(client, filter.clone(), timeout).await;
    }
    log::info!(
        "Fast fetching from {} connected relays (bypassing gossip)",
        connected_urls.len()
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
    log::info!(
        "Fast fetch completed: {} events (after filtering)",
        result.len()
    );
    Ok(result)
}

/// Fetch events from local nostrdb first (instant), then refresh from relays in background.
/// Only available on native builds. Follows the channel bridge pattern.
#[cfg(feature = "native")]
pub async fn fetch_events_ndb_first(
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let ndb_jsons =
        crate::stores::ndb::queries::sdk_filters_to_ndb_jsons(std::slice::from_ref(&filter));
    match crate::stores::ndb::queries::query_note_keys(ndb_jsons, 1000).await {
        Ok(note_keys) if !note_keys.is_empty() => {
            let mut events = Vec::new();
            for (key, _) in note_keys {
                match crate::stores::ndb::queries::get_note_data(key).await {
                    Ok(note_data) => {
                        if let Ok(event) =
                            crate::stores::ndb::queries::note_data_to_event(&note_data)
                        {
                            events.push(event);
                        }
                    }
                    Err(e) => log::warn!("Failed to get note data: {}", e),
                }
            }
            if !events.is_empty() {
                log::info!("Loaded {} events from nostrdb", events.len());
                if let Some(client) = get_client() {
                    let filter_clone = filter.clone();
                    dioxus::prelude::spawn(async move {
                        let _ = client.fetch_events(filter_clone, timeout).await;
                    });
                }
                return Ok(events);
            }
        }
        Ok(_) => {}
        Err(e) => log::warn!("nostrdb query failed: {}, falling back to relays", e),
    }

    fetch_events_from_connected_relays(filter, timeout).await
}

/// Parsed event ID with optional author pubkey and relay hints from nevent decoding.
#[derive(Clone, Debug)]
pub struct ParsedEventId {
    pub event_id: EventId,
    pub author: Option<PublicKey>,
    pub relay_hints: Vec<String>,
}

/// Parse an event ID from various formats (nevent, note, hex) with author and relay hints.
pub fn parse_event_id(id: &str) -> Option<ParsedEventId> {
    use nostr_sdk::nips::nip19::Nip19;
    let trimmed = id.trim();
    let normalized = trimmed
        .strip_prefix("nostr:")
        .or_else(|| trimmed.strip_prefix("NOSTR:"))
        .unwrap_or(trimmed);

    if let Ok(event_id) = EventId::from_hex(normalized) {
        return Some(ParsedEventId {
            event_id,
            author: None,
            relay_hints: vec![],
        });
    }

    Nip19::from_bech32(normalized).ok().and_then(|n| match n {
        Nip19::Event(e) => Some(ParsedEventId {
            event_id: e.event_id,
            author: e.author,
            relay_hints: e.relays.iter().map(|r| r.to_string()).collect(),
        }),
        Nip19::EventId(id) => Some(ParsedEventId {
            event_id: id,
            author: None,
            relay_hints: vec![],
        }),
        _ => None,
    })
}

/// Targeted event fetch with hybrid broadcast+targeted strategy.
///
/// 1. Bridge cache check (instant, native only — bridges nostrdb async ingestion gap)
/// 2. DB check (instant, direct primary key lookup)
/// 3. Broadcast to connected relays (fast, uses existing connections)
/// 4. Targeted author relays via NIP-65 resolution (if author known)
/// 5. Relay hints from nevent (if available)
/// 6. Fallback — gossip/outbox model
pub async fn fetch_event_targeted(
    parsed: ParsedEventId,
    timeout: Duration,
) -> std::result::Result<Option<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    let event_id = parsed.event_id;

    // Phase 0: Bridge cache check (instant, native only)
    #[cfg(feature = "native")]
    {
        let bridge_hit = crate::stores::ndb::get_cached_event(&event_id.to_bytes());
        log::debug!("fetch_event_targeted: Phase 0 bridge cache check for {:?} -> {}", event_id.to_hex(), if bridge_hit.is_some() { "HIT" } else { "MISS" });
        if let Some(event) = bridge_hit {
            return Ok(Some(event));
        }
    }

    // Phase 1: DB check (instant) — direct primary key lookup
    match client.database().event_by_id(&event_id).await {
        Ok(Some(event)) => {
            log::debug!("fetch_event_targeted: Phase 1 DB hit for {:?}", event_id.to_hex());
            return Ok(Some(event));
        }
        Ok(None) => {
            log::debug!("fetch_event_targeted: Phase 1 DB miss for {:?}", event_id.to_hex());
        }
        Err(e) => {
            log::debug!("fetch_event_targeted: Phase 1 DB error for {:?}: {}", event_id.to_hex(), e);
        }
    }

    let filter = Filter::new().id(event_id).limit(1);

    // Phase 2: Broadcast to connected relays (fast)
    match fetch_events_from_connected_relays_with_client(
        &client,
        filter.clone(),
        Duration::from_secs(3),
    )
    .await
    {
        Ok(events) if !events.is_empty() => {
            log::debug!("fetch_event_targeted: Phase 2 broadcast hit for {:?}", event_id.to_hex());
            let event = events.into_iter().next();
            #[cfg(feature = "native")]
            if let Some(ref e) = event {
                crate::stores::ndb::cache_event(e);
            }
            return Ok(event);
        }
        Ok(events) => {
            log::debug!("fetch_event_targeted: Phase 2 broadcast miss for {:?} ({} events)", event_id.to_hex(), events.len());
        }
        Err(e) => {
            log::debug!("fetch_event_targeted: Phase 2 broadcast error for {:?}: {}", event_id.to_hex(), e);
        }
    }

    // Phase 3: Targeted author relays (if known)
    if let Some(author) = &parsed.author {
        let relay_urls = relay::coverage::resolve_user_relays(
            &author.to_hex(),
            relay::coverage::RelayPurpose::Write,
        )
        .await;
        let ephemeral = relay::coverage::connect_ephemeral_relays(&client, &relay_urls).await;
        if !ephemeral.connected.is_empty() {
            let result = relay::connection::fetch_events_from_relays(
                &client,
                filter.clone(),
                ephemeral.connected.clone(),
                timeout,
            )
            .await;
            relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;
            if let Ok(events) = result {
                if let Some(event) = events.into_iter().next() {
                    log::debug!("fetch_event_targeted: Phase 3 author relays hit for {:?}", event_id.to_hex());
                    #[cfg(feature = "native")]
                    crate::stores::ndb::cache_event(&event);
                    return Ok(Some(event));
                }
            }
        }
    }

    // Phase 4: Relay hints from nevent
    if !parsed.relay_hints.is_empty() {
        let ephemeral =
            relay::coverage::connect_ephemeral_relays(&client, &parsed.relay_hints).await;
        if !ephemeral.connected.is_empty() {
            let result = relay::connection::fetch_events_from_relays(
                &client,
                filter.clone(),
                ephemeral.connected.clone(),
                timeout,
            )
            .await;
            relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;
            if let Ok(events) = result {
                if let Some(event) = events.into_iter().next() {
                    log::debug!("fetch_event_targeted: Phase 4 relay hints hit for {:?}", event_id.to_hex());
                    #[cfg(feature = "native")]
                    crate::stores::ndb::cache_event(&event);
                    return Ok(Some(event));
                }
            }
        }
    }

    // Phase 5: Fallback — outbox (gossip model)
    let events = fetch_events_aggregated_outbox(filter, timeout).await?;
    let event = events.into_iter().next();
    #[cfg(feature = "native")]
    if let Some(ref e) = event {
        crate::stores::ndb::cache_event(e);
    }
    Ok(event)
}

/// Fetch video events from connected relays (bypasses gossip)
///
/// Ensures video relay (relay.divine.video) is connected first,
/// then uses fast fetch (bypasses gossip) for the query.
#[allow(dead_code)]
pub async fn fetch_video_events_from_connected_relays(
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    ensure_video_relay_connected(&client).await;
    fetch_events_from_connected_relays_with_client(&client, filter, timeout).await
}

/// Fetch a user's events by targeting their NIP-65 write relays.
///
/// Resolves the user's relay list via the three-tier resolver, connects to those
/// relays (ephemerally), and fetches events. Falls back to the standard
/// `fetch_profile_events_from_relays` (SDK gossip) if no NIP-65 data is available
/// or the targeted fetch returns no results.
pub async fn fetch_profile_events_targeted(
    pubkey_hex: &str,
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    let relay_urls = relay::coverage::resolve_user_relays(
        pubkey_hex,
        relay::coverage::RelayPurpose::Write,
    )
    .await;

    if !relay_urls.is_empty() {
        let ephemeral = relay::coverage::connect_ephemeral_relays(&client, &relay_urls).await;
        if !ephemeral.connected.is_empty() {
            let result = relay::connection::fetch_events_from_relays(
                &client,
                filter.clone(),
                ephemeral.connected.clone(),
                timeout,
            )
            .await;
            relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;
            if let Ok(events) = result {
                if !events.is_empty() {
                    log::debug!(
                        "fetch_profile_events_targeted: got {} events from author relays",
                        events.len()
                    );
                    return Ok(events);
                }
            }
        }
    }

    fetch_profile_events_from_relays(filter, timeout).await
}

/// Fetch a user's metadata (kind 0) by targeting their NIP-65 write relays.
///
/// Resolves the user's relay list via the three-tier resolver, connects to those
/// relays (ephemerally), and queries for metadata. Falls back to the SDK's
/// `client.fetch_metadata()` if no NIP-65 data is available or the targeted fetch
/// returns no results.
pub async fn fetch_metadata_targeted(
    pubkey_hex: &str,
    timeout: Duration,
) -> std::result::Result<Option<nostr_sdk::Metadata>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    let public_key = PublicKey::from_hex(pubkey_hex).map_err(|e| format!("Invalid pubkey: {}", e))?;

    let relay_urls = relay::coverage::resolve_user_relays(
        pubkey_hex,
        relay::coverage::RelayPurpose::Write,
    )
    .await;

    if !relay_urls.is_empty() {
        let ephemeral = relay::coverage::connect_ephemeral_relays(&client, &relay_urls).await;
        if !ephemeral.connected.is_empty() {
            let filter = Filter::new()
                .author(public_key)
                .kind(Kind::Metadata)
                .limit(1);
            let result = relay::connection::fetch_events_from_relays(
                &client,
                filter,
                ephemeral.connected.clone(),
                timeout,
            )
            .await;
            relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;
            if let Ok(events) = result {
                if let Some(event) = events.into_iter().next() {
                    match nostr_sdk::Metadata::from_json(&event.content) {
                        Ok(metadata) => {
                            log::debug!("fetch_metadata_targeted: found via author relays");
                            return Ok(Some(metadata));
                        }
                        Err(e) => {
                            log::warn!("fetch_metadata_targeted: failed to parse metadata: {}", e);
                        }
                    }
                }
            }
        }
    }

    match client.fetch_metadata(public_key, timeout).await {
        Ok(m) => Ok(m),
        Err(e) => Err(format!("Failed to fetch metadata: {}", e)),
    }
}
