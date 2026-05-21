use super::types::{LoadOutcome, MediaSubTab, ProfileTab, ZapSubTab, dedupe_articles_by_address};
use crate::stores::{edit_cache, nostr_client};
use crate::utils::article_meta::get_published_at;
use crate::utils::video_kinds::{horizontal_kinds, vertical_kinds};
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;
use std::collections::HashMap;
use std::time::Duration;

/// Build a filter for the given tab type
pub fn build_tab_filter(
    public_key: PublicKey,
    tab: &ProfileTab,
    until: Option<u64>,
    limit: usize,
) -> Filter {
    let mut filter = match tab {
        ProfileTab::Posts => Filter::new()
            .author(public_key)
            .kinds(vec![Kind::TextNote, Kind::Repost, Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT)])
            .limit(limit),
        ProfileTab::Replies => Filter::new()
            .author(public_key)
            .kind(Kind::TextNote)
            .limit(limit),
        ProfileTab::Articles => Filter::new()
            .author(public_key)
            .kind(Kind::LongFormTextNote)
            .limit(limit),
        ProfileTab::Media(MediaSubTab::Photos) => Filter::new()
            .author(public_key)
            .kind(Kind::Custom(20))
            .limit(limit),
        ProfileTab::Media(MediaSubTab::Videos) => Filter::new()
            .author(public_key)
            .kinds(horizontal_kinds())
            .limit(limit),
        ProfileTab::Media(MediaSubTab::Verts) => Filter::new()
            .author(public_key)
            .kinds(vertical_kinds())
            .limit(limit),
        ProfileTab::Likes => Filter::new()
            .author(public_key)
            .kind(Kind::Reaction)
            .limit(limit),
        ProfileTab::Zaps(ZapSubTab::Received) => Filter::new()
            .kind(Kind::ZapReceipt)
            .pubkey(public_key)
            .limit(limit),
        ProfileTab::Zaps(ZapSubTab::Sent) => Filter::new()
            .kind(Kind::ZapReceipt)
            .custom_tag(SingleLetterTag::uppercase(Alphabet::P), public_key)
            .limit(limit),
    };
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    filter
}
/// Filter and process events based on tab type
pub fn process_tab_events(events: Vec<NostrEvent>, tab: &ProfileTab) -> Vec<NostrEvent> {
    match tab {
        ProfileTab::Posts => {
            let edit_kind = Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT);
            let (edits, display): (Vec<_>, Vec<_>) = events.into_iter().partition(|e| e.kind == edit_kind);
            if !edits.is_empty() {
                let event_map: HashMap<String, NostrEvent> = display.iter().map(|e| (e.id.to_hex(), e.clone())).collect();
                edit_cache::apply_edits_to_event_map(&edits, &event_map);
            }
            display.into_iter().filter(|e| {
                if e.kind == Kind::Repost {
                    return true;
                }
                e.tags.event_ids().next().is_none()
            }).collect()
        }
        ProfileTab::Replies => events
            .into_iter()
            .filter(|e| e.kind != Kind::Repost && e.tags.event_ids().next().is_some())
            .collect(),
        ProfileTab::Articles => dedupe_articles_by_address(events),
        ProfileTab::Zaps(_) => events,
        _ => events,
    }
}
pub async fn load_tab_events_db(
    pubkey: &str,
    tab: &ProfileTab,
    until: Option<u64>,
) -> std::result::Result<LoadOutcome, String> {
    let public_key = crate::utils::nip19_urls::parse_profile_id(pubkey)
        .ok_or_else(|| format!("Invalid public key: {}", pubkey))?;
    if matches!(tab, ProfileTab::Likes) {
        return load_likes_db(public_key, until).await;
    }
    let filter = build_tab_filter(public_key, tab, until, 100);
    let events = nostr_client::fetch_profile_events_db(filter).await?;
    let mut processed = process_tab_events(events, tab);
    if matches!(tab, ProfileTab::Articles) {
        processed.sort_by_key(|e| std::cmp::Reverse(get_published_at(e)));
    } else {
        processed.sort_by_key(|e| std::cmp::Reverse(e.created_at));
    }
    let mut seen_ids = std::collections::HashSet::new();
    processed.retain(|e| seen_ids.insert(e.id));
    let oldest_cursor = if matches!(tab, ProfileTab::Articles) {
        processed.last().map(get_published_at)
    } else {
        processed.last().map(|e| e.created_at.as_secs())
    };
    log::info!("DB Phase: loaded {} {:?} events", processed.len(), tab);
    Ok(LoadOutcome {
        events: processed,
        oldest_cursor,
        relay_count: 0,
    })
}
#[allow(dead_code)]
pub async fn load_tab_events_relays(
    pubkey: &str,
    tab: &ProfileTab,
    until: Option<u64>,
) -> std::result::Result<LoadOutcome, String> {
    let public_key = crate::utils::nip19_urls::parse_profile_id(pubkey)
        .ok_or_else(|| format!("Invalid public key: {}", pubkey))?;
    if matches!(tab, ProfileTab::Likes) {
        return load_likes_relays(public_key, until).await;
    }
    if matches!(tab, ProfileTab::Zaps(_)) {
        return load_tab_events(pubkey, tab, until).await;
    }
    let filter = build_tab_filter(public_key, tab, until, 100);
    let events =
        nostr_client::fetch_profile_events_targeted(pubkey, filter, Duration::from_secs(7))
            .await?;
    let relay_count = events.len();
    let mut processed = process_tab_events(events, tab);
    if matches!(tab, ProfileTab::Articles) {
        processed.sort_by_key(|e| std::cmp::Reverse(get_published_at(e)));
    } else {
        processed.sort_by_key(|e| std::cmp::Reverse(e.created_at));
    }
    let mut seen_ids = std::collections::HashSet::new();
    processed.retain(|e| seen_ids.insert(e.id));
    let oldest_cursor = if matches!(tab, ProfileTab::Articles) {
        processed.last().map(get_published_at)
    } else {
        processed.last().map(|e| e.created_at.as_secs())
    };
    log::info!(
        "Relay Phase: fetched {} {:?} events (raw: {})",
        processed.len(),
        tab,
        relay_count
    );
    Ok(LoadOutcome {
        events: processed,
        oldest_cursor,
        relay_count,
    })
}
/// Common logic for loading liked events
/// Takes a fetch function that retrieves events given a filter
pub async fn load_likes_common<F, Fut>(
    public_key: PublicKey,
    until: Option<u64>,
    fetch_events: F,
) -> std::result::Result<LoadOutcome, String>
where
    F: Fn(Filter) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<Vec<NostrEvent>, String>>,
{
    let mut filter = Filter::new()
        .author(public_key)
        .kind(Kind::Reaction)
        .limit(50);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    let reactions = fetch_events(filter).await?;
    let relay_count = reactions.len();
    if reactions.is_empty() {
        return Ok(LoadOutcome {
            events: Vec::new(),
            oldest_cursor: None,
            relay_count: 0,
        });
    }
    let mut liked_event_ids = Vec::new();
    let mut reaction_times: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    for reaction in reactions.iter() {
        for event_id in reaction.tags.event_ids() {
            liked_event_ids.push(*event_id);
            reaction_times.insert(event_id.to_hex(), reaction.created_at.as_secs());
        }
    }
    if liked_event_ids.is_empty() {
        return Ok(LoadOutcome {
            events: Vec::new(),
            oldest_cursor: None,
            relay_count,
        });
    }
    let liked_filter = Filter::new().ids(liked_event_ids).limit(500);
    let liked_events = fetch_events(liked_filter).await?;
    let mut event_vec: Vec<NostrEvent> = liked_events;
    event_vec.sort_by(|a, b| {
        let time_a = reaction_times.get(&a.id.to_hex()).copied().unwrap_or(0);
        let time_b = reaction_times.get(&b.id.to_hex()).copied().unwrap_or(0);
        time_b.cmp(&time_a)
    });
    let oldest_cursor = event_vec
        .last()
        .and_then(|e| reaction_times.get(&e.id.to_hex()).copied());
    Ok(LoadOutcome {
        events: event_vec,
        oldest_cursor,
        relay_count,
    })
}
pub async fn load_likes_db(
    public_key: PublicKey,
    until: Option<u64>,
) -> std::result::Result<LoadOutcome, String> {
    load_likes_common(public_key, until, |filter| {
        nostr_client::fetch_profile_events_db(filter)
    })
    .await
}
pub async fn load_likes_relays(
    public_key: PublicKey,
    until: Option<u64>,
) -> std::result::Result<LoadOutcome, String> {
    const REACTIONS_LIMIT: usize = 50;
    let mut filter = Filter::new()
        .author(public_key)
        .kind(Kind::Reaction)
        .limit(REACTIONS_LIMIT);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    let hex_pubkey = public_key.to_hex();
    let reactions =
        nostr_client::fetch_profile_events_targeted(&hex_pubkey, filter, Duration::from_secs(10))
            .await?;
    let relay_count = if reactions.len() >= REACTIONS_LIMIT {
        REACTIONS_LIMIT
    } else {
        reactions.len()
    };
    if reactions.is_empty() {
        return Ok(LoadOutcome {
            events: Vec::new(),
            oldest_cursor: None,
            relay_count: 0,
        });
    }
    let mut liked_event_ids = Vec::new();
    let mut reaction_times: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    for reaction in reactions.iter() {
        for event_id in reaction.tags.event_ids() {
            liked_event_ids.push(*event_id);
            reaction_times.insert(event_id.to_hex(), reaction.created_at.as_secs());
        }
    }
    if liked_event_ids.is_empty() {
        return Ok(LoadOutcome {
            events: Vec::new(),
            oldest_cursor: None,
            relay_count: 0,
        });
    }
    let liked_filter = Filter::new().ids(liked_event_ids.clone()).limit(500);
    let mut event_vec: Vec<NostrEvent> =
        nostr_client::fetch_events_from_connected_relays(liked_filter, Duration::from_secs(10))
            .await?;
    let found_ids: std::collections::HashSet<_> = event_vec.iter().map(|e| e.id).collect();
    let missing_ids: Vec<_> = liked_event_ids
        .iter()
        .filter(|id| !found_ids.contains(id))
        .cloned()
        .collect();
    if !missing_ids.is_empty() {
        log::info!(
            "Fetching {} missing liked events via gossip fallback",
            missing_ids.len()
        );
        let gossip_filter = Filter::new().ids(missing_ids).limit(100);
        match nostr_client::fetch_profile_events_from_relays(gossip_filter, Duration::from_secs(10))
            .await
        {
            Ok(gossip_events) => {
                let mut found_via_gossip = 0;
                for event in gossip_events {
                    if !found_ids.contains(&event.id) {
                        event_vec.push(event);
                        found_via_gossip += 1;
                    }
                }
                log::info!("Recovered {} events via gossip", found_via_gossip);
            }
            Err(e) => {
                log::warn!("Gossip fallback failed for liked events: {}", e);
            }
        }
    }
    event_vec.sort_by(|a, b| {
        let time_a = reaction_times.get(&a.id.to_hex()).copied().unwrap_or(0);
        let time_b = reaction_times.get(&b.id.to_hex()).copied().unwrap_or(0);
        time_b.cmp(&time_a)
    });
    let oldest_cursor = event_vec
        .last()
        .and_then(|e| reaction_times.get(&e.id.to_hex()).copied());
    Ok(LoadOutcome {
        events: event_vec,
        oldest_cursor,
        relay_count,
    })
}
pub async fn load_tab_events(
    pubkey: &str,
    tab: &ProfileTab,
    until: Option<u64>,
) -> std::result::Result<LoadOutcome, String> {
    let public_key = crate::utils::nip19_urls::parse_profile_id(pubkey)
        .ok_or_else(|| format!("Invalid public key: {}", pubkey))?;
    const TARGET_COUNT: usize = 50;
    const MAX_FETCH_LIMIT: usize = 500;
    match tab {
        ProfileTab::Posts => {
            let mut all_posts = Vec::new();
            let mut current_until = until;
            let mut total_fetched = 0;
            let mut hit_end = false;
            while all_posts.len() < TARGET_COUNT && total_fetched < MAX_FETCH_LIMIT {
                let mut filter = Filter::new()
                    .author(public_key)
                    .kinds(vec![Kind::TextNote, Kind::Repost, Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT)])
                    .limit(100);
                if let Some(until_ts) = current_until {
                    filter = filter.until(Timestamp::from(until_ts));
                }
                let events =
                    nostr_client::fetch_profile_events_targeted(pubkey, filter, Duration::from_secs(10))
                        .await
                        .map_err(|e| format!("Failed to fetch events: {}", e))?;
                let events_len = events.len();
                if events_len == 0 {
                    hit_end = true;
                    break;
                }
                total_fetched += events_len;
                let oldest_event_ts = events.last().map(|e| e.created_at.as_secs());
                let edit_kind = Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT);
                let (edits, display): (Vec<NostrEvent>, Vec<NostrEvent>) = events.into_iter().partition(|e| e.kind == edit_kind);
                if !edits.is_empty() {
                    let event_map: HashMap<String, NostrEvent> = display.iter().map(|e| (e.id.to_hex(), e.clone())).collect();
                    edit_cache::apply_edits_to_event_map(&edits, &event_map);
                }
                let posts: Vec<NostrEvent> = display
                    .into_iter()
                    .filter(|e| e.kind == Kind::Repost || e.tags.event_ids().next().is_none())
                    .collect();
                all_posts.extend(posts);
                if let Some(ts) = oldest_event_ts {
                    current_until = Some(ts - 1);
                }
                if events_len < 100 {
                    hit_end = true;
                    break;
                }
            }
            all_posts.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            let mut seen_ids = std::collections::HashSet::new();
            all_posts.retain(|e| seen_ids.insert(e.id));
            log::info!(
                "Loaded {} posts (fetched {} total events, hit_end={})",
                all_posts.len(),
                total_fetched,
                hit_end
            );
            let oldest_cursor = all_posts.last().map(|e| e.created_at.as_secs());
            Ok(LoadOutcome {
                events: all_posts,
                oldest_cursor,
                relay_count: if hit_end { 0 } else { total_fetched },
            })
        }
        ProfileTab::Replies => {
            let mut all_replies = Vec::new();
            let mut current_until = until;
            let mut total_fetched = 0;
            let mut hit_end = false;
            while all_replies.len() < TARGET_COUNT && total_fetched < MAX_FETCH_LIMIT {
                let mut filter = Filter::new()
                    .author(public_key)
                    .kind(Kind::TextNote)
                    .limit(100);
                if let Some(until_ts) = current_until {
                    filter = filter.until(Timestamp::from(until_ts));
                }
                let events =
                    nostr_client::fetch_profile_events_targeted(pubkey, filter, Duration::from_secs(10))
                        .await
                        .map_err(|e| format!("Failed to fetch events: {}", e))?;
                let events_len = events.len();
                if events_len == 0 {
                    hit_end = true;
                    break;
                }
                total_fetched += events_len;
                let oldest_event_ts = events.last().map(|e| e.created_at.as_secs());
                let replies: Vec<NostrEvent> = events
                    .into_iter()
                    .filter(|e| e.tags.event_ids().next().is_some())
                    .collect();
                all_replies.extend(replies);
                if let Some(ts) = oldest_event_ts {
                    current_until = Some(ts - 1);
                }
                if events_len < 100 {
                    hit_end = true;
                    break;
                }
            }
            all_replies.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            let mut seen_ids = std::collections::HashSet::new();
            all_replies.retain(|e| seen_ids.insert(e.id));
            log::info!(
                "Loaded {} replies (fetched {} total events, hit_end={})",
                all_replies.len(),
                total_fetched,
                hit_end
            );
            let oldest_cursor = all_replies.last().map(|e| e.created_at.as_secs());
            Ok(LoadOutcome {
                events: all_replies,
                oldest_cursor,
                relay_count: if hit_end { 0 } else { total_fetched },
            })
        }
        ProfileTab::Articles => {
            let mut filter = Filter::new()
                .author(public_key)
                .kind(Kind::LongFormTextNote)
                .limit(TARGET_COUNT);
            if let Some(until_ts) = until {
                filter = filter.until(Timestamp::from(until_ts));
            }
            let events =
                nostr_client::fetch_profile_events_targeted(pubkey, filter, Duration::from_secs(10))
                    .await
                    .map_err(|e| format!("Failed to fetch events: {}", e))?;
            let relay_count = events.len();
            let event_vec: Vec<NostrEvent> = events.into_iter().collect();
            let mut deduplicated = dedupe_articles_by_address(event_vec);
            deduplicated.sort_by_key(|e| std::cmp::Reverse(get_published_at(e)));
            log::info!("Loaded {} articles (after dedup)", deduplicated.len());
            let oldest_cursor = deduplicated.last().map(get_published_at);
            Ok(LoadOutcome {
                events: deduplicated,
                oldest_cursor,
                relay_count,
            })
        }
        ProfileTab::Media(MediaSubTab::Photos) => {
            let mut filter = Filter::new()
                .author(public_key)
                .kind(Kind::Custom(20))
                .limit(TARGET_COUNT);
            if let Some(until_ts) = until {
                filter = filter.until(Timestamp::from(until_ts));
            }
            let events =
                nostr_client::fetch_profile_events_targeted(pubkey, filter, Duration::from_secs(10))
                    .await
                    .map_err(|e| format!("Failed to fetch events: {}", e))?;
            let relay_count = events.len();
            let mut event_vec: Vec<NostrEvent> = events.into_iter().collect();
            event_vec.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            log::info!("Loaded {} photos", event_vec.len());
            let oldest_cursor = event_vec.last().map(|e| e.created_at.as_secs());
            Ok(LoadOutcome {
                events: event_vec,
                oldest_cursor,
                relay_count,
            })
        }
        ProfileTab::Media(MediaSubTab::Videos) => {
            let mut filter = Filter::new()
                .author(public_key)
                .kinds(horizontal_kinds())
                .limit(TARGET_COUNT);
            if let Some(until_ts) = until {
                filter = filter.until(Timestamp::from(until_ts));
            }
            let events =
                nostr_client::fetch_profile_events_targeted(pubkey, filter, Duration::from_secs(10))
                    .await
                    .map_err(|e| format!("Failed to fetch events: {}", e))?;
            let relay_count = events.len();
            let mut event_vec: Vec<NostrEvent> = events.into_iter().collect();
            event_vec.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            log::info!("Loaded {} videos", event_vec.len());
            let oldest_cursor = event_vec.last().map(|e| e.created_at.as_secs());
            Ok(LoadOutcome {
                events: event_vec,
                oldest_cursor,
                relay_count,
            })
        }
        ProfileTab::Media(MediaSubTab::Verts) => {
            let mut filter = Filter::new()
                .author(public_key)
                .kinds(vertical_kinds())
                .limit(TARGET_COUNT);
            if let Some(until_ts) = until {
                filter = filter.until(Timestamp::from(until_ts));
            }
            let events =
                nostr_client::fetch_profile_events_targeted(pubkey, filter, Duration::from_secs(10))
                    .await
                    .map_err(|e| format!("Failed to fetch events: {}", e))?;
            let relay_count = events.len();
            let mut event_vec: Vec<NostrEvent> = events.into_iter().collect();
            event_vec.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            log::info!("Loaded {} verts", event_vec.len());
            let oldest_cursor = event_vec.last().map(|e| e.created_at.as_secs());
            Ok(LoadOutcome {
                events: event_vec,
                oldest_cursor,
                relay_count,
            })
        }
        ProfileTab::Likes => {
            let mut filter = Filter::new()
                .author(public_key)
                .kind(Kind::Reaction)
                .limit(TARGET_COUNT);
            if let Some(until_ts) = until {
                filter = filter.until(Timestamp::from(until_ts));
            }
            let reactions =
                nostr_client::fetch_profile_events_targeted(pubkey, filter, Duration::from_secs(10))
                    .await
                    .map_err(|e| format!("Failed to fetch reactions: {}", e))?;
            let relay_count = reactions.len();
            if reactions.is_empty() {
                return Ok(LoadOutcome {
                    events: Vec::new(),
                    oldest_cursor: None,
                    relay_count: 0,
                });
            }
            let mut liked_event_ids = Vec::new();
            for reaction in reactions.iter() {
                for event_id in reaction.tags.event_ids() {
                    liked_event_ids.push(*event_id);
                }
            }
            if liked_event_ids.is_empty() {
                log::info!("No event IDs found in reactions");
                return Ok(LoadOutcome {
                    events: Vec::new(),
                    oldest_cursor: None,
                    relay_count,
                });
            }
            let liked_filter = Filter::new().ids(liked_event_ids).limit(500);
            let liked_events = nostr_client::fetch_profile_events_from_relays(
                liked_filter,
                Duration::from_secs(10),
            )
            .await
            .map_err(|e| format!("Failed to fetch liked events: {}", e))?;
            let mut reaction_times: std::collections::HashMap<String, u64> =
                std::collections::HashMap::new();
            for reaction in reactions.iter() {
                for event_id in reaction.tags.event_ids() {
                    reaction_times.insert(event_id.to_hex(), reaction.created_at.as_secs());
                }
            }
            let mut event_vec: Vec<NostrEvent> = liked_events.into_iter().collect();
            event_vec.sort_by(|a, b| {
                let time_a = reaction_times.get(&a.id.to_hex()).copied().unwrap_or(0);
                let time_b = reaction_times.get(&b.id.to_hex()).copied().unwrap_or(0);
                time_b.cmp(&time_a)
            });
            log::info!("Loaded {} liked events", event_vec.len());
            let oldest_cursor = event_vec
                .last()
                .and_then(|e| reaction_times.get(&e.id.to_hex()).copied());
            Ok(LoadOutcome {
                events: event_vec,
                oldest_cursor,
                relay_count,
            })
        }
        ProfileTab::Zaps(_) => {
            let hex_pk = public_key.to_hex();
            let mut filter = build_tab_filter(public_key, tab, until, TARGET_COUNT);
            if let Some(until_ts) = until {
                filter = filter.until(Timestamp::from(until_ts));
            }
            let events = nostr_client::fetch_profile_events_targeted(
                &hex_pk, filter, Duration::from_secs(10),
            )
                .await
                .map_err(|e| format!("Failed to fetch zaps: {}", e))?;
            let relay_count = events.len();
            let mut event_vec: Vec<NostrEvent> = events.into_iter().collect();
            event_vec.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            log::info!("Loaded {} zaps", event_vec.len());
            let oldest_cursor = event_vec.last().map(|e| e.created_at.as_secs());
            Ok(LoadOutcome {
                events: event_vec,
                oldest_cursor,
                relay_count,
            })
        }
    }
}

/// Batch prefetch author metadata for all events
pub async fn prefetch_author_metadata(events: &[NostrEvent]) {
    use crate::utils::profile_prefetch;
    profile_prefetch::prefetch_event_authors(events).await;
}
