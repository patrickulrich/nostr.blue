use super::*;

/// Fetch calendar events
pub async fn fetch_calendar_events(limit: usize) -> StdResult<Vec<CalendarEvent>, String> {
    let count = *LOADING_EVENTS.read();
    *LOADING_EVENTS.write() = count + 1;
    let filter = calendar_events_filter(limit);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await;
    let count = *LOADING_EVENTS.read();
    *LOADING_EVENTS.write() = count.saturating_sub(1);
    match result {
        Ok(events) => {
            cache_calendar_events(&events);
            let cal_events: Vec<CalendarEvent> = events
                .iter()
                .filter_map(|e| parse_calendar_event(e).ok())
                .collect();
            Ok(cal_events)
        }
        Err(e) => Err(e),
    }
}
/// Fetch meetings (spaces and rooms)
pub async fn fetch_meetings(limit: usize) -> StdResult<Vec<LiveActivityEvent>, String> {
    let filter = meetings_filter(limit);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15))
            .await?;
    cache_live_events(&events);
    let activities: Vec<LiveActivityEvent> = events
        .iter()
        .filter_map(|e| {
            let kind = e.kind.as_u16();
            match kind {
                KIND_MEETING_ROOM => parse_meeting_room_event(e)
                    .ok()
                    .map(LiveActivityEvent::Meeting),
                KIND_MEETING_SPACE => parse_meeting_space(e).ok().map(LiveActivityEvent::Space),
                _ => None,
            }
        })
        .collect();
    Ok(activities)
}
/// Fetch all events (calendar + meetings) for the events page
pub async fn fetch_all_events(limit: usize) -> StdResult<Vec<UnifiedEvent>, String> {
    let count = *LOADING_EVENTS.read();
    *LOADING_EVENTS.write() = count + 1;
    let cal_filter = calendar_events_filter(limit);
    let meetings_filter = meetings_filter(limit);
    let (cal_result, meetings_result) = futures::join!(
        crate::stores::nostr_client::fetch_events_aggregated(cal_filter, Duration::from_secs(15)),
        crate::stores::nostr_client::fetch_events_aggregated(
            meetings_filter,
            Duration::from_secs(15)
        )
    );
    let count = *LOADING_EVENTS.read();
    *LOADING_EVENTS.write() = count.saturating_sub(1);
    let mut all_events = Vec::new();
    if let Ok(events) = cal_result {
        cache_calendar_events(&events);
        for event in events {
            if let Ok(cal_event) = parse_calendar_event(&event) {
                all_events.push(UnifiedEvent::Calendar(cal_event));
            }
        }
    }
    if let Ok(events) = meetings_result {
        cache_live_events(&events);
        for event in events {
            let kind = event.kind.as_u16();
            let activity = match kind {
                KIND_MEETING_ROOM => parse_meeting_room_event(&event)
                    .ok()
                    .map(LiveActivityEvent::Meeting),
                KIND_MEETING_SPACE => parse_meeting_space(&event)
                    .ok()
                    .map(LiveActivityEvent::Space),
                _ => None,
            };
            if let Some(activity) = activity {
                all_events.push(UnifiedEvent::Live(activity));
            }
        }
    }
    Ok(all_events)
}
/// Fetch all events (calendar + meetings) with pagination support
/// Returns (events, oldest_timestamp) where oldest_timestamp can be used for next page
pub async fn fetch_all_events_paginated(
    limit: usize,
    until: Option<u64>,
) -> StdResult<(Vec<UnifiedEvent>, Option<u64>), String> {
    let (cal_filter, mtg_filter) = match until {
        Some(ts) => (
            calendar_events_filter_until(ts, limit),
            meetings_filter_until(ts, limit),
        ),
        None => (calendar_events_filter(limit), meetings_filter(limit)),
    };
    let (cal_result, mtg_result) = futures::join!(
        crate::stores::nostr_client::fetch_events_aggregated(cal_filter, Duration::from_secs(15)),
        crate::stores::nostr_client::fetch_events_aggregated(mtg_filter, Duration::from_secs(15))
    );
    let cal_ok = cal_result.is_ok();
    let mtg_ok = mtg_result.is_ok();
    let mut all_events = Vec::new();
    let mut oldest_ts: Option<u64> = None;
    if let Ok(events) = cal_result {
        cache_calendar_events(&events);
        for event in &events {
            let ts = event.created_at.as_secs();
            oldest_ts = Some(oldest_ts.map_or(ts, |o| o.min(ts)));
            if let Ok(cal_event) = parse_calendar_event(event) {
                all_events.push(UnifiedEvent::Calendar(cal_event));
            }
        }
    }
    if let Ok(events) = mtg_result {
        cache_live_events(&events);
        for event in &events {
            let ts = event.created_at.as_secs();
            oldest_ts = Some(oldest_ts.map_or(ts, |o| o.min(ts)));
            let kind = event.kind.as_u16();
            let activity = match kind {
                KIND_MEETING_ROOM => parse_meeting_room_event(event)
                    .ok()
                    .map(LiveActivityEvent::Meeting),
                KIND_MEETING_SPACE => parse_meeting_space(event)
                    .ok()
                    .map(LiveActivityEvent::Space),
                _ => None,
            };
            if let Some(activity) = activity {
                all_events.push(UnifiedEvent::Live(activity));
            }
        }
    }
    if !cal_ok && !mtg_ok {
        return Err("Both calendar and meeting fetches failed".to_string());
    }
    all_events = super::dedupe_events_by_coordinate(all_events);
    Ok((all_events, oldest_ts))
}
/// Fetch personal calendar events for the signed-in user
/// Includes:
/// - Events created by user
/// - Events where user is invited (p tag)
/// - Events user has RSVPed to (accepted)
/// - User's availability blocks
pub async fn fetch_personal_calendar_events() -> StdResult<Vec<UnifiedEvent>, String> {
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pk = PublicKey::from_hex(&pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    log::info!(
        "[calendar_store] Fetching personal calendar events for {}",
        pubkey
    );
    let authored_filter = Filter::new()
        .kinds([
            Kind::Custom(KIND_DATE_CALENDAR_EVENT),
            Kind::Custom(KIND_TIME_CALENDAR_EVENT),
        ])
        .author(pk)
        .limit(200);
    let invited_filter = Filter::new()
        .kinds([
            Kind::Custom(KIND_DATE_CALENDAR_EVENT),
            Kind::Custom(KIND_TIME_CALENDAR_EVENT),
        ])
        .pubkey(pk)
        .limit(200);
    let blocks_filter = Filter::new()
        .kind(Kind::Custom(KIND_AVAILABILITY_BLOCK))
        .author(pk)
        .limit(100);
    let rsvps_filter = Filter::new()
        .kind(Kind::Custom(KIND_CALENDAR_RSVP))
        .author(pk)
        .limit(200);
    let (authored_result, invited_result, blocks_result, rsvps_result) = futures::join!(
        crate::stores::nostr_client::fetch_events_aggregated(
            authored_filter,
            Duration::from_secs(15)
        ),
        crate::stores::nostr_client::fetch_events_aggregated(
            invited_filter,
            Duration::from_secs(15)
        ),
        crate::stores::nostr_client::fetch_events_aggregated(
            blocks_filter,
            Duration::from_secs(15)
        ),
        crate::stores::nostr_client::fetch_events_aggregated(rsvps_filter, Duration::from_secs(15))
    );
    use crate::utils::nip52::RsvpStatus;
    let mut all_events = Vec::new();
    let mut seen_coords = std::collections::HashSet::new();
    if let Ok(events) = authored_result {
        log::info!("[calendar_store] Found {} authored events", events.len());
        cache_calendar_events(&events);
        for event in events {
            if let Ok(cal_event) = parse_calendar_event(&event) {
                let coord = cal_event.coordinate.clone();
                seen_coords.insert(coord);
                all_events.push(UnifiedEvent::Calendar(cal_event));
            }
        }
    }
    if let Ok(events) = invited_result {
        log::info!("[calendar_store] Found {} invited events", events.len());
        cache_calendar_events(&events);
        for event in events {
            if let Ok(cal_event) = parse_calendar_event(&event) {
                let coord = cal_event.coordinate.clone();
                seen_coords.insert(coord);
                all_events.push(UnifiedEvent::Calendar(cal_event));
            }
        }
    }
    if let Ok(events) = blocks_result {
        log::info!(
            "[calendar_store] Found {} availability blocks (not displayed yet)",
            events.len()
        );
    }
    if let Ok(rsvp_events) = rsvps_result {
        log::info!("[calendar_store] Found {} RSVPs", rsvp_events.len());
        let rsvp_coords: Vec<String> = rsvp_events
            .iter()
            .filter_map(|e| parse_calendar_rsvp(e).ok())
            .filter(|rsvp| matches!(rsvp.status, RsvpStatus::Accepted))
            .filter_map(|rsvp| {
                let coord = rsvp.event_coordinate.clone();
                if !coord.is_empty() && seen_coords.insert(coord.clone()) {
                    Some(coord)
                } else {
                    None
                }
            })
            .collect();
        use futures::stream::{self, StreamExt};
        let rsvp_results: Vec<_> = stream::iter(rsvp_coords.iter())
            .map(|coord| fetch_event_by_coordinate(coord))
            .buffer_unordered(3)
            .collect()
            .await;
        for result in rsvp_results {
            if let Ok(Some(event)) = result {
                all_events.push(event);
            }
        }
    }
    log::info!(
        "[calendar_store] Total personal calendar events: {}",
        all_events.len()
    );
    Ok(super::dedupe_events_by_coordinate(all_events))
}
/// Helper to fetch a single event by its coordinate (kind:pubkey:d-tag)
async fn fetch_event_by_coordinate(coordinate: &str) -> StdResult<Option<UnifiedEvent>, String> {
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() < 3 {
        return Ok(None);
    }
    let kind: u16 = parts[0].parse().map_err(|_| "Invalid kind")?;
    let pubkey = parts[1];
    let d_tag = parts[2..].join(":");
    let pk = PublicKey::from_hex(pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(kind))
        .author(pk)
        .identifier(&d_tag);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;
    if let Some(event) = events.first() {
        if let Ok(cal_event) = parse_calendar_event(event) {
            return Ok(Some(UnifiedEvent::Calendar(cal_event)));
        }
    }
    Ok(None)
}
/// Fetch RSVPs for a specific event
pub async fn fetch_event_rsvps(event_coordinate: &str) -> StdResult<Vec<CalendarRsvp>, String> {
    let filter = rsvps_filter(event_coordinate);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;
    let rsvps: Vec<CalendarRsvp> = events
        .iter()
        .filter_map(|e| parse_calendar_rsvp(e).ok())
        .collect();
    cache_rsvps(event_coordinate, rsvps.clone());
    Ok(rsvps)
}
/// A comment on a calendar event
#[derive(Clone, Debug)]
pub struct CalendarEventComment {
    pub event_id: String,
    pub pubkey: String,
    pub content: String,
    pub created_at: u64,
}
/// Fetch comments for a calendar event by coordinate
/// Comments reference the event via 'A' tag (root scope per NIP-22)
/// Uses fetch_events_from_relays to bypass cache and get fresh data
pub async fn fetch_event_comments(
    coordinate: &str,
) -> StdResult<Vec<CalendarEventComment>, String> {
    let filter = Filter::new()
        .kind(Kind::Custom(1111))
        .custom_tag(
            SingleLetterTag::uppercase(Alphabet::A),
            coordinate.to_string(),
        )
        .limit(100);
    let events =
        crate::stores::nostr_client::fetch_events_from_relays(filter, Duration::from_secs(10))
            .await?;
    let mut comments: Vec<CalendarEventComment> = Vec::new();
    for e in events.iter() {
        let has_tag_value = |tag_name: &str, expected: &str| -> bool {
            e.tags.iter().any(|t| {
                t.as_slice().first().map(|s| s.as_str()) == Some(tag_name)
                    && t.as_slice().get(1).map(|s| s.as_str()) == Some(expected)
            })
        };
        // Parse expected kind from coordinate (format: "kind:pubkey:dtag")
        let expected_kind: Option<u32> = coordinate.split(':').next().and_then(|k| k.parse().ok());

        let has_valid_kind_tag = |tag_name: &str| -> bool {
            e.tags.iter().any(|t| {
                t.as_slice().first().map(|s| s.as_str()) == Some(tag_name)
                    && t.as_slice()
                        .get(1)
                        .and_then(|s| s.parse::<u32>().ok())
                        .is_some_and(|v| expected_kind.is_none_or(|ek| v == ek))
            })
        };
        if !has_tag_value("A", coordinate) {
            log::debug!(
                "Skipping event {} - no 'A' tag matching coordinate '{}'",
                e.id,
                coordinate
            );
            continue;
        }
        if !has_valid_kind_tag("K") {
            log::debug!("Skipping event {} - missing or invalid 'K' tag", e.id);
            continue;
        }
        if !has_valid_kind_tag("k") {
            log::debug!(
                "Skipping event {} - missing or invalid parent kind tag 'k'",
                e.id
            );
            continue;
        }
        comments.push(CalendarEventComment {
            event_id: e.id.to_hex(),
            pubkey: e.pubkey.to_hex(),
            content: ammonia::clean(&e.content),
            created_at: e.created_at.as_secs(),
        });
    }
    comments.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(comments)
}

/// Maximum search result limit to prevent excessive relay load
const MAX_SEARCH_LIMIT: usize = 500;
/// Maximum query length to prevent abuse
const MAX_QUERY_LEN: usize = 256;

/// Search calendar events using NIP-50 relay search
/// Searches across title, description, and content fields
/// Uses fetch_events_from_relays to bypass cache for fresh search results
pub async fn search_calendar_events(
    query: &str,
    limit: usize,
) -> StdResult<Vec<UnifiedEvent>, String> {
    let query: String = query.chars().take(MAX_QUERY_LEN).collect();
    let query = query.as_str();
    let limit = limit.min(MAX_SEARCH_LIMIT);
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let filter = Filter::new()
        .kinds([
            Kind::Custom(KIND_DATE_CALENDAR_EVENT),
            Kind::Custom(KIND_TIME_CALENDAR_EVENT),
        ])
        .search(query)
        .limit(limit);
    let events =
        crate::stores::nostr_client::fetch_events_from_relays(filter, Duration::from_secs(10))
            .await?;
    let mut results: Vec<UnifiedEvent> = Vec::new();
    for e in events.iter() {
        if let Ok(cal_event) = parse_calendar_event(e) {
            results.push(UnifiedEvent::Calendar(cal_event));
        }
    }
    cache_calendar_events(&events);
    results = super::dedupe_events_by_coordinate(results);
    results.sort_by_key(|a| a.start_timestamp());
    results.truncate(limit);
    Ok(results)
}
/// Search all events (calendar + meetings) using NIP-50 relay search
/// Searches across title, description, and content fields for both calendar events and meetings
/// Uses fetch_events_from_relays to bypass cache for fresh search results
pub async fn search_all_events(query: &str, limit: usize) -> StdResult<Vec<UnifiedEvent>, String> {
    use crate::utils::nip53::{
        parse_meeting_room_event, parse_meeting_space, LiveActivityEvent, KIND_MEETING_ROOM,
        KIND_MEETING_SPACE,
    };
    let query: String = query.chars().take(MAX_QUERY_LEN).collect();
    let query = query.as_str();
    let limit = limit.min(MAX_SEARCH_LIMIT);
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let cal_filter = Filter::new()
        .kinds([
            Kind::Custom(KIND_DATE_CALENDAR_EVENT),
            Kind::Custom(KIND_TIME_CALENDAR_EVENT),
        ])
        .search(query)
        .limit(limit);
    let meeting_filter = Filter::new()
        .kinds([
            Kind::Custom(KIND_MEETING_SPACE),
            Kind::Custom(KIND_MEETING_ROOM),
        ])
        .search(query)
        .limit(limit);
    let (cal_result, meeting_result) = futures::join!(
        crate::stores::nostr_client::fetch_events_from_relays(cal_filter, Duration::from_secs(10)),
        crate::stores::nostr_client::fetch_events_from_relays(
            meeting_filter,
            Duration::from_secs(10)
        )
    );
    if let (Err(cal_err), Err(meeting_err)) = (&cal_result, &meeting_result) {
        return Err(format!(
            "Search failed - calendar: {}, meetings: {}",
            cal_err, meeting_err
        ));
    }
    let mut results = Vec::new();
    if let Ok(events) = cal_result {
        cache_calendar_events(&events);
        for event in &events {
            if let Ok(cal_event) = parse_calendar_event(event) {
                results.push(UnifiedEvent::Calendar(cal_event));
            }
        }
    }
    if let Ok(events) = meeting_result {
        cache_live_events(&events);
        for event in &events {
            let kind = event.kind.as_u16();
            let activity = match kind {
                KIND_MEETING_ROOM => parse_meeting_room_event(event)
                    .ok()
                    .map(LiveActivityEvent::Meeting),
                KIND_MEETING_SPACE => parse_meeting_space(event)
                    .ok()
                    .map(LiveActivityEvent::Space),
                _ => None,
            };
            if let Some(activity) = activity {
                results.push(UnifiedEvent::Live(activity));
            }
        }
    }
    results = super::dedupe_events_by_coordinate(results);
    results.sort_by_key(|e| e.start_timestamp());
    Ok(results)
}
/// Fetch user's RSVPs
pub async fn fetch_my_rsvps(pubkey: &str) -> StdResult<Vec<CalendarRsvp>, String> {
    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = my_rsvps_filter(pk);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;
    let rsvps: Vec<CalendarRsvp> = events
        .iter()
        .filter_map(|e| parse_calendar_rsvp(e).ok())
        .collect();
    for rsvp in &rsvps {
        cache_my_rsvp(rsvp.clone());
    }
    Ok(rsvps)
}
/// Fetch specific event by naddr
pub async fn fetch_event_by_naddr(naddr: &str) -> StdResult<Option<CalendarEvent>, String> {
    log::info!(
        "[calendar_store] fetch_event_by_naddr called with: {}",
        naddr
    );
    if let Some(cached) = get_cached_event_by_naddr(naddr) {
        log::info!("[calendar_store] Found in cache: {}", cached.title);
        return Ok(Some(cached));
    }
    let nip19 = Nip19Coordinate::from_bech32(naddr).map_err(|e| {
        log::error!("[calendar_store] Invalid naddr '{}': {}", naddr, e);
        format!("Invalid naddr: {}", e)
    })?;
    let coord = nip19.coordinate;
    let pk = coord.public_key;
    let kind = coord.kind.as_u16();
    let identifier = coord.identifier.clone();
    log::info!(
        "[calendar_store] Parsed naddr - kind: {}, pubkey: {}, identifier: {}",
        kind,
        pk,
        identifier
    );
    let filter = event_by_coordinate_filter(pk, kind, &identifier);
    log::info!("[calendar_store] Fetching from relays with 10s timeout...");
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;
    log::info!(
        "[calendar_store] Received {} events from relays",
        events.len()
    );
    if let Some(event) = events.first() {
        log::info!(
            "[calendar_store] Attempting to parse event kind {}",
            event.kind.as_u16()
        );
        if let Ok(cal_event) = parse_calendar_event(event) {
            log::info!(
                "[calendar_store] Successfully parsed event: {}",
                cal_event.title
            );
            cache_event(cal_event.clone());
            return Ok(Some(cal_event));
        } else {
            log::warn!("[calendar_store] Failed to parse event as CalendarEvent");
        }
    }
    log::warn!(
        "[calendar_store] No matching event found for naddr: {}",
        naddr
    );
    Ok(None)
}
/// Fetch unified event by naddr (handles both calendar events and meetings)
pub async fn fetch_unified_event_by_naddr(naddr: &str) -> StdResult<Option<UnifiedEvent>, String> {
    log::info!(
        "[calendar_store] fetch_unified_event_by_naddr called with: {}",
        naddr
    );
    if let Some(cached) = get_cached_event_by_naddr(naddr) {
        log::info!(
            "[calendar_store] Found calendar event in cache: {}",
            cached.title
        );
        return Ok(Some(UnifiedEvent::Calendar(cached)));
    }
    {
        let cache = LIVE_EVENTS_CACHE.read();
        if let Some((_, activity)) = cache.iter().find(|(_, e)| e.naddr() == naddr) {
            log::info!(
                "[calendar_store] Found meeting in cache: {}",
                activity.title()
            );
            return Ok(Some(UnifiedEvent::Live(activity.clone())));
        }
    }
    let nip19 = Nip19Coordinate::from_bech32(naddr).map_err(|e| {
        log::error!("[calendar_store] Invalid naddr '{}': {}", naddr, e);
        format!("Invalid naddr: {}", e)
    })?;
    let coord = nip19.coordinate;
    let kind = coord.kind.as_u16();
    let pk = coord.public_key;
    let identifier = coord.identifier.clone();
    let relay_hints: Vec<String> = nip19.relays.iter().map(|r| r.to_string()).collect();
    log::info!(
        "[calendar_store] Parsed naddr - kind: {}, pubkey: {}, identifier: {}, hints: {}",
        kind, pk, identifier, relay_hints.len()
    );
    log::info!("[calendar_store] Fetching from relays...");
    let event =
        crate::stores::nostr_client::fetch_event_by_coordinate_with_relays(
            kind,
            pk.to_hex(),
            identifier,
            relay_hints,
        )
        .await?;
    log::info!(
        "[calendar_store] Fetch result: {}",
        if event.is_some() { "found" } else { "not found" }
    );
    if let Some(event) = event {
        let event_kind = event.kind.as_u16();
        log::info!(
            "[calendar_store] Attempting to parse event kind {}",
            event_kind
        );
        match event_kind {
            KIND_DATE_CALENDAR_EVENT | KIND_TIME_CALENDAR_EVENT => {
                if let Ok(cal_event) = parse_calendar_event(&event) {
                    log::info!(
                        "[calendar_store] Successfully parsed calendar event: {}",
                        cal_event.title
                    );
                    cache_event(cal_event.clone());
                    return Ok(Some(UnifiedEvent::Calendar(cal_event)));
                }
            }
            KIND_MEETING_SPACE => {
                if let Ok(space) = parse_meeting_space(&event) {
                    log::info!(
                        "[calendar_store] Successfully parsed meeting space: {}",
                        space.room_name
                    );
                    let activity = LiveActivityEvent::Space(space);
                    LIVE_EVENTS_CACHE
                        .write()
                        .put(activity.coordinate().to_string(), activity.clone());
                    return Ok(Some(UnifiedEvent::Live(activity)));
                }
            }
            KIND_MEETING_ROOM => {
                if let Ok(meeting) = parse_meeting_room_event(&event) {
                    log::info!(
                        "[calendar_store] Successfully parsed meeting room: {}",
                        meeting.title
                    );
                    let activity = LiveActivityEvent::Meeting(meeting);
                    LIVE_EVENTS_CACHE
                        .write()
                        .put(activity.coordinate().to_string(), activity.clone());
                    return Ok(Some(UnifiedEvent::Live(activity)));
                }
            }
            _ => {
                log::warn!("[calendar_store] Unknown event kind: {}", event_kind);
            }
        }
    }
    log::warn!(
        "[calendar_store] No matching event found for naddr: {}",
        naddr
    );
    Ok(None)
}
/// Initialize calendar store
pub async fn init_calendar_store() -> StdResult<(), String> {
    if *CALENDAR_INITIALIZED.read() {
        return Ok(());
    }
    // Set immediately to prevent concurrent callers from entering
    *CALENDAR_INITIALIZED.write() = true;
    if let Err(e) = fetch_all_events(200).await {
        // Reset on failure so retry is possible
        *CALENDAR_INITIALIZED.write() = false;
        return Err(e);
    }
    if let Some(pubkey) = crate::stores::auth_store::get_pubkey() {
        if let Err(e) = fetch_my_rsvps(&pubkey).await {
            log::warn!("Failed to fetch user RSVPs: {}", e);
        }
    }
    Ok(())
}
/// Fetch availability templates for a user (kind 31926)
pub async fn fetch_availability_templates(
    pubkey: &str,
) -> StdResult<Vec<AvailabilityTemplate>, String> {
    use crate::utils::nip52::parse_availability_template;
    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = availability_templates_filter(pk);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;
    let templates: Vec<AvailabilityTemplate> = events
        .iter()
        .filter_map(|e| parse_availability_template(e).ok())
        .collect();
    {
        let mut cache = AVAILABILITY_TEMPLATES_CACHE.write();
        for template in &templates {
            cache.put(template.coordinate.clone(), template.clone());
        }
    }
    Ok(templates)
}
/// Fetch availability blocks for a user (kind 31927)
pub async fn fetch_availability_blocks(pubkey: &str) -> StdResult<Vec<AvailabilityBlock>, String> {
    use crate::utils::nip52::parse_availability_block;
    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = availability_blocks_filter(pk);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;
    let blocks: Vec<AvailabilityBlock> = events
        .iter()
        .filter_map(|e| parse_availability_block(e).ok())
        .collect();
    {
        let mut cache = AVAILABILITY_BLOCKS_CACHE.write();
        for block in &blocks {
            cache.put(block.coordinate.clone(), block.clone());
        }
    }
    Ok(blocks)
}
/// Fetch all booking data (templates + blocks) for a user
pub async fn fetch_booking_data(
    pubkey: &str,
) -> StdResult<(Vec<AvailabilityTemplate>, Vec<AvailabilityBlock>), String> {
    let (templates, blocks) = futures::join!(
        fetch_availability_templates(pubkey),
        fetch_availability_blocks(pubkey)
    );
    Ok((templates?, blocks?))
}
/// Fetch calendars for a user (kind 31924)
pub async fn fetch_calendars(
    pubkey: &str,
) -> StdResult<Vec<crate::utils::nip52::Calendar>, String> {
    use crate::utils::nip52::parse_calendar;
    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = calendars_filter(pk);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;
    let calendars: Vec<crate::utils::nip52::Calendar> = events
        .iter()
        .filter_map(|e| parse_calendar(e).ok())
        .collect();
    {
        let mut cache = CALENDARS_CACHE.write();
        for calendar in &calendars {
            cache.put(calendar.coordinate.clone(), calendar.clone());
        }
    }
    Ok(calendars)
}
/// Fetch room presence for a meeting/space coordinate
/// Returns list of users present in the room within the last `max_age_secs` seconds
pub async fn fetch_room_presence(
    room_coordinate: &str,
    max_age_secs: u64,
) -> StdResult<Vec<RoomPresence>, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    let now_secs = crate::platform::timestamp::now_secs();
    let since_ts = now_secs.saturating_sub(max_age_secs);
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_ROOM_PRESENCE))
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::A),
            room_coordinate.to_string(),
        )
        .since(Timestamp::from(since_ts));
    log::info!(
        "[calendar_store] Fetching room presence for: {}",
        room_coordinate
    );
    let events = client
        .fetch_events(filter, Duration::from_secs(5))
        .await
        .map_err(|e| format!("Failed to fetch presence: {}", e))?;
    log::info!("[calendar_store] Found {} presence events", events.len());
    let mut presence_by_user: std::collections::HashMap<String, RoomPresence> =
        std::collections::HashMap::new();
    for event in events.iter() {
        if let Ok(presence) = parse_room_presence(event) {
            presence_by_user
                .entry(presence.pubkey.clone())
                .and_modify(|existing| {
                    if presence.created_at > existing.created_at {
                        *existing = presence.clone();
                    }
                })
                .or_insert(presence);
        }
    }
    let mut result: Vec<RoomPresence> = presence_by_user.into_values().collect();
    result.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(result)
}
