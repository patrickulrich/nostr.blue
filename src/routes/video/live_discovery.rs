use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::ReadableExt;
use nostr_sdk::{Event, Filter, Kind, PublicKey, Timestamp};
use std::collections::HashSet;
use std::time::Duration;

const FOLLOWING_FETCH_LIMIT: usize = 100;
const GLOBAL_FETCH_LIMIT: usize = 50;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LiveStreamStatusFilter {
    Live,
    Upcoming,
    All,
}

/// Returns (events, next_until, hit_limit, did_fallback, followed_pubkeys)
pub async fn load_following_live_streams(
    until: Option<u64>,
    status: LiveStreamStatusFilter,
) -> Result<(Vec<Event>, Option<u64>, bool, bool, Option<HashSet<String>>), String> {
    let pubkey_str = auth_store::AUTH_STATE
        .read()
        .pubkey
        .clone()
        .ok_or("Not authenticated")?;

    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!(
                "Failed to fetch contacts: {}, falling back to global feed",
                e
            );
            let (events, next_until, hit_limit) = load_global_live_streams(until, status).await?;
            return Ok((events, next_until, hit_limit, true, None));
        }
    };

    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global streams");
        let (events, next_until, hit_limit) = load_global_live_streams(until, status).await?;
        return Ok((events, next_until, hit_limit, true, None));
    }

    let followed_pubkeys = parse_followed_pubkeys(&contacts);
    if followed_pubkeys.is_empty() {
        log::warn!("No valid contact pubkeys, falling back to global feed");
        let (events, next_until, hit_limit) = load_global_live_streams(until, status).await?;
        return Ok((events, next_until, hit_limit, true, None));
    }

    let (events, next_until, hit_limit) =
        stream_live_events(until, FOLLOWING_FETCH_LIMIT, FOLLOWING_FETCH_LIMIT).await?;
    let following_events = events
        .into_iter()
        .filter(|event| stream_matches_following(event, &followed_pubkeys))
        .collect();

    Ok((
        filter_live_streams_by_status(following_events, status),
        next_until,
        hit_limit,
        false,
        Some(followed_pubkeys),
    ))
}

pub async fn load_global_live_streams(
    until: Option<u64>,
    status: LiveStreamStatusFilter,
) -> Result<(Vec<Event>, Option<u64>, bool), String> {
    let (events, next_until, hit_limit) =
        stream_live_events(until, GLOBAL_FETCH_LIMIT, GLOBAL_FETCH_LIMIT).await?;
    Ok((
        filter_live_streams_by_status(events, status),
        next_until,
        hit_limit,
    ))
}

pub fn filter_live_streams_by_status(
    events: Vec<Event>,
    status: LiveStreamStatusFilter,
) -> Vec<Event> {
    match status {
        LiveStreamStatusFilter::All => events,
        LiveStreamStatusFilter::Live => events
            .into_iter()
            .filter(|event| event_has_status(event, "live"))
            .collect(),
        LiveStreamStatusFilter::Upcoming => events
            .into_iter()
            .filter(|event| event_has_status(event, "planned"))
            .collect(),
    }
}

pub fn stream_matches_following(event: &Event, followed_pubkeys: &HashSet<String>) -> bool {
    if followed_pubkeys.contains(&event.pubkey.to_string()) {
        return true;
    }

    event.tags.iter().any(|tag| {
        let tag_vec = tag.as_slice();
        tag_vec.first().map(|s| s.as_str()) == Some("p")
            && tag_vec
                .get(1)
                .map(|creator_pk| followed_pubkeys.contains(creator_pk))
                == Some(true)
    })
}

fn event_has_status(event: &Event, target_status: &str) -> bool {
    event.tags.iter().any(|tag| {
        let tag_vec = tag.as_slice();
        tag_vec.first().map(|s| s.as_str()) == Some("status")
            && tag_vec
                .get(1)
                .map(|status| status.eq_ignore_ascii_case(target_status))
                == Some(true)
    })
}

fn parse_followed_pubkeys(contacts: &[String]) -> HashSet<String> {
    contacts
        .iter()
        .filter_map(|contact| PublicKey::parse(contact).ok().map(|pk| pk.to_string()))
        .collect()
}

async fn stream_live_events(
    until: Option<u64>,
    limit: usize,
    hit_limit_threshold: usize,
) -> Result<(Vec<Event>, Option<u64>, bool), String> {
    let mut filter = Filter::new().kind(Kind::Custom(30311)).limit(limit);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    // Ensure the Livelier bridge relay is in the pool and connecting
    // (non-blocking; failure to connect must not stall the Live tab).
    crate::stores::relay::specialty::ensure_livestream_relays_connected(&client).await;

    // Run both fetch arms concurrently so a slow/down Livelier relay can't
    // extend the worst-case page load beyond the shared 10s timeout.
    let livelier_url = nostr_sdk::RelayUrl::parse(
        crate::stores::relay::specialty::urls::LIVELIER,
    )
    .map_err(|e| format!("Invalid Livelier relay URL: {}", e))?;

    let mut pool_events = Vec::new();
    let pool_arm = nostr_client::stream_events_immediate(filter.clone(), Duration::from_secs(10), |event| {
        pool_events.push(event);
    });

    let livelier_arm = client.fetch_events_from(
        vec![livelier_url],
        filter,
        Duration::from_secs(10),
    );

    let (pool_result, livelier_result) = futures::join!(pool_arm, livelier_arm);

    pool_result.map_err(|e| format!("Failed to fetch streams: {}", e))?;

    // A failed Livelier fetch (relay down, cold connect timeout) degrades to
    // pool-only results rather than failing the page.
    let livelier_events = match livelier_result {
        Ok(events) => {
            let events: Vec<Event> = events.into_iter().collect();
            log::info!("Livelier bridge fetch: {} live events", events.len());
            events
        }
        Err(e) => {
            log::warn!("Livelier bridge fetch failed (continuing with pool events): {}", e);
            Vec::new()
        }
    };

    Ok(merge_live_events(pool_events, livelier_events, hit_limit_threshold))
}

/// Merge pool-streamed and Livelier-bridged 30311 events.
///
/// Dedup stages (in order):
/// 1. By event id — the same event may arrive from both arms.
/// 2. Sort newest-first, snapshot the pagination cursor (`next_until`) from
///    the oldest *pre-dedup* event so coordinate dedup can't move the cursor
///    up and force the next page to refetch overlap.
/// 3. By coordinate — 30311 is addressable and the bridge revises events
///    frequently; keep only the newest revision per pubkey+d-tag (matching
///    the realtime path's `upsert_stream_event` semantics).
///
/// `hit_limit` is anchored to the id-deduped count for the same reason: a
/// page full of revisions must not falsely end pagination.
pub(crate) fn merge_live_events(
    pool_events: Vec<Event>,
    livelier_events: Vec<Event>,
    hit_limit_threshold: usize,
) -> (Vec<Event>, Option<u64>, bool) {
    let mut events = pool_events;
    events.extend(livelier_events);

    let mut seen_ids = HashSet::new();
    events.retain(|event| seen_ids.insert(event.id));
    events.sort_by_key(|b| std::cmp::Reverse(b.created_at));

    let next_until = events.last().map(|event| event.created_at.as_secs());
    let hit_limit = events.len() >= hit_limit_threshold;

    let mut seen_coords = HashSet::new();
    events.retain(|event| seen_coords.insert(event.coordinate().map(|c| c.into_owned())));

    (events, next_until, hit_limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::*;

    fn live_event(keys: &Keys, d_tag: &str, created_at: u64) -> Event {
        EventBuilder::new(Kind::Custom(30311), "")
            .tags(vec![Tag::identifier(d_tag)])
            .custom_created_at(Timestamp::from(created_at))
            .sign_with_keys(keys)
            .unwrap()
    }

    #[test]
    fn test_merge_dedups_by_coordinate_keeping_newest() {
        let keys = Keys::generate();
        let old = live_event(&keys, "stream-1", 1000);
        let new = live_event(&keys, "stream-1", 2000);
        let other_author = live_event(&Keys::generate(), "stream-1", 1500);

        let (merged, next_until, _) =
            merge_live_events(vec![old.clone(), other_author.clone()], vec![new.clone()], 50);

        assert_eq!(merged.len(), 2);
        assert!(merged.contains(&new));
        assert!(merged.contains(&other_author));
        // Superseded revision of the same coordinate is dropped
        assert!(!merged.contains(&old));
        assert_eq!(next_until, Some(1000));
    }

    #[test]
    fn test_merge_next_until_from_pre_dedup_oldest() {
        let keys = Keys::generate();
        // Oldest event overall is a superseded revision — the pagination
        // cursor must still come from it so the next page doesn't refetch
        // the coordinate-deduped overlap.
        let oldest = live_event(&keys, "stream-1", 500);
        let newest = live_event(&keys, "stream-1", 900);
        let middle = live_event(&Keys::generate(), "stream-2", 700);

        let (merged, next_until, _) =
            merge_live_events(vec![oldest, middle.clone()], vec![newest], 50);

        assert_eq!(merged.len(), 2);
        assert_eq!(next_until, Some(500));
        assert!(merged.contains(&middle));
    }

    #[test]
    fn test_merge_dedups_by_id_across_arms() {
        let keys = Keys::generate();
        let event = live_event(&keys, "stream-1", 1000);
        let unique = live_event(&Keys::generate(), "stream-2", 1100);

        let (merged, _, _) =
            merge_live_events(vec![event.clone(), unique.clone()], vec![event.clone()], 50);

        assert_eq!(merged.len(), 2);
        assert!(merged.contains(&event));
        assert!(merged.contains(&unique));
    }

    #[test]
    fn test_merge_hit_limit_before_coordinate_dedup() {
        let keys = Keys::generate();
        let old = live_event(&keys, "stream-1", 1000);
        let new = live_event(&keys, "stream-1", 2000);
        let unique = live_event(&Keys::generate(), "stream-2", 1500);

        // 3 id-deduped events >= threshold 3 → pagination continues even
        // though coordinate dedup leaves only 2.
        let (merged, _, hit_limit) =
            merge_live_events(vec![old, unique], vec![new], 3);

        assert_eq!(merged.len(), 2);
        assert!(hit_limit);
    }

    #[test]
    fn test_merge_empty_inputs() {
        let (merged, next_until, hit_limit) = merge_live_events(Vec::new(), Vec::new(), 50);
        assert!(merged.is_empty());
        assert_eq!(next_until, None);
        assert!(!hit_limit);
    }
}
