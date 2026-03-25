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

    let mut events = Vec::new();
    nostr_client::stream_events_immediate(filter, Duration::from_secs(10), |event| {
        events.push(event);
    })
    .await
    .map_err(|e| format!("Failed to fetch streams: {}", e))?;

    let mut seen = HashSet::new();
    events.retain(|event| seen.insert(event.id));
    events.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let next_until = events.last().map(|event| event.created_at.as_secs());
    let hit_limit = events.len() >= hit_limit_threshold;

    Ok((events, next_until, hit_limit))
}
