use crate::error::NostrBlueError;
use crate::hooks::UserList;
use crate::stores::{auth_store, nostr_client};
use crate::utils::list_encryption::get_all_list_members;
use crate::utils::{extract_reposted_event, process_events_to_feed_items, FeedItem};
use dioxus::prelude::*;
use nostr_relay_pool::{SyncDirection, SyncOptions};
use nostr_sdk::{Filter, Kind, PublicKey, Timestamp};
use std::collections::HashSet;
use std::time::Duration;

const SINCE_BUFFER_SECS: u64 = 120;
const RELAY_FEED_LIMIT: usize = 33;
pub const FEED_LIMIT: usize = 33;
#[cfg(feature = "native")]
const NDB_FEED_LIMIT: usize = 200;

fn resolve_since(adaptive_since: u64, cached_cursor: Option<u64>) -> Option<u64> {
    let eose_since = crate::stores::eose_tracker::EoseTracker::get_min_since();
    match (eose_since, cached_cursor) {
        (Some(eose), _) => Some(std::cmp::min(eose, adaptive_since)),
        (None, Some(cursor)) => {
            let cursor_since = cursor.saturating_sub(SINCE_BUFFER_SECS);
            Some(std::cmp::min(cursor_since, adaptive_since))
        }
        (None, None) => Some(adaptive_since),
    }
}

pub fn feed_kinds() -> Vec<Kind> {
    vec![
        Kind::TextNote,
        Kind::Repost,
        Kind::Comment,
    ]
}

const FOLLOWING_INITIAL_WINDOW_SECS: u64 = 86400;

/// Bounds each *paginated* slice (`until = Some`) to a window below the
/// cursor. Without this, paginated DB-first reads (`fetch_events_aggregated`,
/// `fetch_events_ndb_first`) and negentropy syncs are unbounded into the past,
/// so old events deposited by unrelated unbounded fetches (e.g. notification
/// mention backfills) surface in the feed in one page. Mirrors wisp's guarded
/// cache-to-feed path (`rebuildFeedFromCache`), which applies a 24h `since`.
/// Each page still walks back one window at a time, so infinite scroll can
/// paginate arbitrarily deep — just not in a single leap.
const PAGINATION_WINDOW_SECS: u64 = 24 * 3600;

/// The `since` floor for a paginated query at cursor `until_ts`.
/// Author-scoped feeds (following, people lists) use the larger of the
/// pagination window and their adaptive window so quiet feeds (few posts in
/// any given 24h slice) don't stall pagination early.
pub fn pagination_since_floor(until_ts: u64, author_count: Option<usize>) -> u64 {
    let window = match author_count {
        Some(count) => PAGINATION_WINDOW_SECS.max(adaptive_since_window(count)),
        None => PAGINATION_WINDOW_SECS,
    };
    until_ts.saturating_sub(window)
}

pub fn adaptive_since_window(author_count: usize) -> u64 {
    match author_count {
        0..=10 => 7 * 86400,
        11..=30 => 5 * 86400,
        31..=150 => 2 * 86400,
        _ => FOLLOWING_INITIAL_WINDOW_SECS,
    }
}

#[allow(dead_code)]
pub fn exclusive_pagination_cursor(item: Option<&FeedItem>) -> Option<u64> {
    item.map(|entry| entry.sort_timestamp().as_secs())
}

pub fn merge_paginated_feed_items(
    current: Vec<FeedItem>,
    fetched_page: Vec<FeedItem>,
) -> (Vec<FeedItem>, Vec<FeedItem>, Option<u64>) {
    let existing_ids: HashSet<_> = current.iter().map(|item| item.event().id).collect();
    let unique_items: Vec<FeedItem> = fetched_page
        .into_iter()
        .filter(|item| !existing_ids.contains(&item.event().id))
        .collect();
    let mut updated = current;
    updated.extend(unique_items.iter().cloned());
    updated.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
    let timestamps: Vec<u64> = updated.iter().map(|i| i.sort_timestamp().as_secs()).collect();
    let next_cursor = crate::utils::pagination::safe_cursor_from_timestamps(&timestamps);
    (updated, unique_items, next_cursor)
}

pub fn build_global_feed_filter(until: Option<u64>, cached_cursor: Option<u64>) -> Filter {
    let mut filter = Filter::new()
        .kinds(feed_kinds())
        .limit(FEED_LIMIT);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
        filter = filter.since(Timestamp::from(pagination_since_floor(until_ts, None)));
    } else {
        let adaptive_since = Timestamp::now().as_secs().saturating_sub(86400);
        if let Some(since) = resolve_since(adaptive_since, cached_cursor) {
            filter = filter.since(Timestamp::from(since));
        }
    }
    filter
}

pub fn build_following_feed_filter(
    authors: Vec<PublicKey>,
    until: Option<u64>,
    now: Timestamp,
    cached_cursor: Option<u64>,
) -> Filter {
    let author_count = authors.len();
    let mut filter = Filter::new()
        .kinds(feed_kinds())
        .authors(authors)
        .limit(FEED_LIMIT);

    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
        filter = filter.since(Timestamp::from(pagination_since_floor(
            until_ts,
            Some(author_count),
        )));
    } else {
        let window = adaptive_since_window(author_count);
        let adaptive_since = now.as_secs().saturating_sub(window);
        if let Some(since) = resolve_since(adaptive_since, cached_cursor) {
            filter = filter.since(Timestamp::from(since));
        }
    }

    filter
}

pub async fn sync_global_feed_page(until: Option<u64>) {
    let Some(client) = nostr_client::get_client() else {
        log::debug!("Skipping global feed negentropy sync: client unavailable");
        return;
    };
    let filter = build_global_feed_filter(until, None);
    let sync_opts = SyncOptions::default()
        .direction(SyncDirection::Down)
        .initial_timeout(Duration::from_secs(5));
    match client.sync(filter, &sync_opts).await {
        Ok(output) => {
            log::info!(
                "Global feed negentropy sync complete: {} received, {} sent",
                output.val.received.len(),
                output.val.sent.len()
            );
        }
        Err(e) => {
            log::warn!(
                "Global feed negentropy sync failed, continuing with DB read: {}",
                e
            );
        }
    }
}

pub async fn sync_following_feed_page(authors: Vec<PublicKey>, until: Option<u64>) {
    let Some(client) = nostr_client::get_client() else {
        log::debug!("Skipping following feed negentropy sync: client unavailable");
        return;
    };
    if authors.is_empty() {
        return;
    }
    let mut filter = Filter::new()
        .kinds(feed_kinds())
        .authors(authors.clone())
        .limit(FEED_LIMIT);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
        filter = filter.since(Timestamp::from(pagination_since_floor(
            until_ts,
            Some(authors.len()),
        )));
    }
    let sync_opts = SyncOptions::default()
        .direction(SyncDirection::Down)
        .initial_timeout(Duration::from_secs(5));
    match client.sync(filter, &sync_opts).await {
        Ok(output) => {
            log::info!(
                "Following feed negentropy sync complete: {} received, {} sent, {} relays succeeded, {} failed",
                output.val.received.len(),
                output.val.sent.len(),
                output.success.len(),
                output.failed.len()
            );
        }
        Err(e) => {
            log::warn!("Following feed negentropy sync failed: {}", e);
        }
    }
}

pub async fn load_paginated_global_feed(
    until: Option<u64>,
) -> Result<Vec<FeedItem>, NostrBlueError> {
    sync_global_feed_page(until).await;
    load_global_feed(until, None, 0).await
}

pub async fn load_following_feed(
    until: Option<u64>,
    cached_cursor: Option<u64>,
    _cached_count: usize,
) -> Result<(Vec<FeedItem>, bool), NostrBlueError> {
    let pubkey_str = auth_store::get_pubkey().ok_or(NostrBlueError::NotAuthenticated)?;
    log::info!(
        "Loading following feed for {} (until: {:?})",
        pubkey_str,
        until
    );
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(c) => c,
        Err(e) => {
            log::warn!(
                "Failed to fetch contacts: {}, falling back to global feed",
                e
            );
            let global = load_global_feed(until, None, 0).await?;
            return Ok((global, true));
        }
    };
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global feed");
        let global = load_global_feed(until, None, 0).await?;
        return Ok((global, true));
    }
    log::info!("User follows {} accounts", contacts.len());
    let mut authors = Vec::new();
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            authors.push(pk);
        }
    }
    if authors.is_empty() {
        log::warn!("No valid contact pubkeys, falling back to global feed");
        let global = load_global_feed(until, None, 0).await?;
        return Ok((global, true));
    }
    let filter = build_following_feed_filter(authors.clone(), until, Timestamp::now(), cached_cursor);
    log::info!(
        "Fetching events from {} followed accounts",
        filter.authors.as_ref().map(|a| a.len()).unwrap_or(0)
    );
    let fetch_result = {
        #[cfg(feature = "native")]
        {
            nostr_client::fetch_events_ndb_first(filter, Duration::from_secs(10)).await
        }
        #[cfg(not(feature = "native"))]
        {
            nostr_client::fetch_events_from_connected_relays(filter, Duration::from_secs(10)).await
        }
    };
    match fetch_result {
        Ok(events) => {
            let raw_count = events.len();
            log::info!(
                "Loaded {} events (including reposts) from following feed via outbox",
                raw_count
            );
            let mut feed_items: Vec<FeedItem> = Vec::new();
            for event in events.into_iter() {
                if event.kind == Kind::Repost {
                    match extract_reposted_event(&event) {
                        Ok(original) => {
                            feed_items.push(FeedItem::Repost {
                                original,
                                reposted_by: event.pubkey,
                                repost_timestamp: event.created_at,
                            });
                        }
                        Err(e) => {
                            log::warn!("Failed to parse repost event {}: {}", event.id, e);
                        }
                    }
                } else if event.kind == Kind::TextNote {
                    let is_reply = event.tags.iter().any(|tag| tag.is_reply() || tag.is_root());
                    if !is_reply {
                        feed_items.push(FeedItem::OriginalPost(event));
                    }
                } else if event.kind == Kind::Comment
                    && crate::stores::topic_store::is_topic_post(&event)
                {
                    let is_reply = event.tags.iter().any(|tag| tag.is_reply() || tag.is_root());
                    if !is_reply {
                        feed_items.push(FeedItem::OriginalPost(event));
                    }
                }
            }
            feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
            log::info!(
                "After processing: {} feed items (raw: {})",
                feed_items.len(),
                raw_count
            );
            if feed_items.is_empty() {
                log::info!("No posts from followed users, trying favorite relays");
                match try_feed_from_favorite_relays(&authors, until).await {
                    Ok(extra_items) if !extra_items.is_empty() => {
                        log::info!("Got {} items from favorite relays", extra_items.len());
                        return Ok((extra_items, false));
                    }
                    _ => {
                        return Ok((Vec::new(), false));
                    }
                }
            }
            Ok((feed_items, false))
        }
        Err(e) => {
            log::error!(
                "Failed to fetch following feed: {}, trying favorite relays before global fallback",
                e
            );
            match try_feed_from_favorite_relays(&authors, until).await {
                Ok(extra_items) if !extra_items.is_empty() => {
                    log::info!("Got {} items from favorite relays", extra_items.len());
                    Ok((extra_items, false))
                }
                _ => {
                    let global = load_global_feed(until, None, 0).await?;
                    Ok((global, true))
                }
            }
        }
    }
}
async fn try_feed_from_favorite_relays(
    authors: &[PublicKey],
    until: Option<u64>,
) -> Result<Vec<FeedItem>, NostrBlueError> {
    let favorite_urls = {
        use dioxus::prelude::ReadableExt;
        let relays = crate::stores::relay::nip65::FAVORITE_RELAYS.peek().clone();
        if relays.is_empty() {
            crate::stores::relay::nip65::default_favorite_relays()
        } else {
            relays
        }
    };
    let relay_urls: Vec<nostr_sdk::RelayUrl> = favorite_urls
        .iter()
        .filter_map(|s| nostr_sdk::RelayUrl::parse(s).ok())
        .collect();
    if relay_urls.is_empty() || authors.is_empty() {
        return Ok(Vec::new());
    }
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => return Ok(Vec::new()),
    };
    let filter = build_following_feed_filter(authors.to_vec(), until, Timestamp::now(), None);
    match client
        .fetch_events_from(relay_urls, filter, std::time::Duration::from_secs(8))
        .await
    {
        Ok(events) => {
            let mut feed_items: Vec<FeedItem> = Vec::new();
            for event in events.into_iter() {
                if event.kind == Kind::Repost {
                    if let Ok(original) = extract_reposted_event(&event) {
                        feed_items.push(FeedItem::Repost {
                            original,
                            reposted_by: event.pubkey,
                            repost_timestamp: event.created_at,
                        });
                    }
                } else if event.kind == Kind::TextNote {
                    let is_reply = event.tags.iter().any(|tag| tag.is_reply() || tag.is_root());
                    if !is_reply {
                        feed_items.push(FeedItem::OriginalPost(event));
                    }
                } else if event.kind == Kind::Comment
                    && crate::stores::topic_store::is_topic_post(&event)
                {
                    let is_reply = event.tags.iter().any(|tag| tag.is_reply() || tag.is_root());
                    if !is_reply {
                        feed_items.push(FeedItem::OriginalPost(event));
                    }
                }
            }
            feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
            Ok(feed_items)
        }
        Err(e) => {
            log::warn!("Favorite relay feed fetch failed: {}", e);
            Ok(Vec::new())
        }
    }
}

/// Map a streamed event to a top-level Following-feed item (replies dropped,
/// reposts unwrapped, topic posts from comments included).
fn following_stream_event_to_item(event: nostr_sdk::Event) -> Option<FeedItem> {
    if event.kind == Kind::Repost {
        match extract_reposted_event(&event) {
            Ok(original) => Some(FeedItem::Repost {
                original,
                reposted_by: event.pubkey,
                repost_timestamp: event.created_at,
            }),
            Err(_) => None,
        }
    } else if event.kind == Kind::TextNote {
        let is_reply = event.tags.iter().any(|tag| tag.is_reply() || tag.is_root());
        if !is_reply {
            Some(FeedItem::OriginalPost(event))
        } else {
            None
        }
    } else if event.kind == Kind::Comment {
        if crate::stores::topic_store::is_topic_post(&event) {
            let is_reply = event.tags.iter().any(|tag| tag.is_reply() || tag.is_root());
            if !is_reply {
                Some(FeedItem::OriginalPost(event))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }
}

/// Stream one following-feed batch into `all_items`, deduping via `seen_ids`
/// and forwarding each accepted item to `on_batch`.
async fn stream_following_batch<F>(
    filter: Filter,
    timeout: Duration,
    all_items: &mut Vec<FeedItem>,
    seen_ids: &mut std::collections::HashSet<nostr_sdk::EventId>,
    on_batch: &mut F,
) -> Result<(), NostrBlueError>
where
    F: FnMut(Vec<FeedItem>),
{
    nostr_client::stream_events_immediate(filter, timeout, |event| {
        if let Some(feed_item) = following_stream_event_to_item(event) {
            if seen_ids.insert(feed_item.event().id) {
                all_items.push(feed_item.clone());
                on_batch(vec![feed_item]);
            }
        }
    })
    .await
    .map(|_| ())
}

pub async fn load_following_feed_streaming<F>(
    until: Option<u64>,
    cached_cursor: Option<u64>,
    _cached_count: usize,
    mut on_batch: F,
) -> Result<(Vec<FeedItem>, bool), NostrBlueError>
where
    F: FnMut(Vec<FeedItem>),
{
    use std::collections::HashSet;
    let pubkey_str = auth_store::get_pubkey().ok_or(NostrBlueError::NotAuthenticated)?;
    log::info!(
        "Loading following feed (streaming) for {} (until: {:?})",
        pubkey_str,
        until
    );
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!(
                "Failed to fetch contacts: {}, falling back to global feed",
                e
            );
            let global = load_global_feed(until, None, 0).await?;
            return Ok((global, true));
        }
    };
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global feed");
        let global = load_global_feed(until, None, 0).await?;
        return Ok((global, true));
    }
    log::info!("User follows {} accounts, streaming posts", contacts.len());
    let mut authors = Vec::new();
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            authors.push(pk);
        }
    }
    if authors.is_empty() {
        log::warn!("No valid contact pubkeys, falling back to global feed");
        let global = load_global_feed(until, None, 0).await?;
        return Ok((global, true));
    }
    let filter = build_following_feed_filter(authors, until, Timestamp::now(), cached_cursor);
    let mut all_items: Vec<FeedItem> = Vec::new();
    let mut seen_ids: HashSet<nostr_sdk::EventId> = HashSet::new();

    #[cfg(feature = "native")]
    {
        if let Some(client) = nostr_client::get_client() {
            let ndb_filter = filter.clone().limit(NDB_FEED_LIMIT);
            if let Ok(db_events) = client.database().query(ndb_filter).await {
                if !db_events.is_empty() {
                    let db_items = process_events_to_feed_items(db_events.into_iter().collect());
                    log::info!("nostrdb pre-step: {} items for feed", db_items.len());
                    for item in &db_items {
                        seen_ids.insert(item.event().id);
                    }
                    on_batch(db_items.clone());
                    all_items = db_items;
                }
            }
        }
    }

    let stream_result = stream_following_batch(
        filter.clone(),
        Duration::from_secs(10),
        &mut all_items,
        &mut seen_ids,
        &mut on_batch,
    )
    .await;
    if let Err(e) = stream_result {
        log::error!(
            "Failed to stream following feed: {}, falling back to global",
            e
        );
        let global = load_global_feed(until, None, 0).await?;
        return Ok((global, true));
    }
    // First-login race guard: an empty stream is a *success* state and
    // renders as "No posts yet". USER_RELAYS_APPLIED only means the NIP-65
    // list is in the pool — on a cold first login the user's relays may
    // still be handshaking, and the stream above then silently skipped them
    // (only DEFAULT relays served the REQ, and followed authors mostly post
    // on their own write relays). Wait for a user read relay to connect
    // and retry once.
    if all_items.is_empty() {
        if let Some(client) = nostr_client::get_client() {
            if !crate::stores::relay::any_user_read_relay_connected(&client).await {
                log::info!(
                    "Following stream empty with no user NIP-65 relays connected; waiting and retrying once"
                );
                crate::stores::relay::wait_for_user_read_relay_connected(
                    &client,
                    Duration::from_secs(5),
                    "following_feed_retry",
                )
                .await;
                let _ = stream_following_batch(
                    filter,
                    Duration::from_secs(10),
                    &mut all_items,
                    &mut seen_ids,
                    &mut on_batch,
                )
                .await;
            }
        }
    }
    if all_items.is_empty() {
        log::info!("No posts from followed users via streaming");
        return Ok((Vec::new(), false));
    }
    all_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
    log::info!("Streaming complete: {} feed items", all_items.len());
    Ok((all_items, false))
}

pub async fn load_following_with_replies(
    until: Option<u64>,
    cached_cursor: Option<u64>,
    _cached_count: usize,
) -> Result<(Vec<FeedItem>, bool), NostrBlueError> {
    let pubkey_str = auth_store::get_pubkey().ok_or(NostrBlueError::NotAuthenticated)?;
    log::info!(
        "Loading following feed with replies for {} (until: {:?})",
        pubkey_str,
        until
    );
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!(
                "Failed to fetch contacts: {}, falling back to global feed",
                e
            );
            let global = load_global_feed(until, None, 0).await?;
            return Ok((global, true));
        }
    };
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global feed");
        let global = load_global_feed(until, None, 0).await?;
        return Ok((global, true));
    }
    log::info!("User follows {} accounts", contacts.len());
    let mut authors = Vec::new();
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            authors.push(pk);
        }
    }
    if authors.is_empty() {
        log::warn!("No valid contact pubkeys, falling back to global feed");
        let global = load_global_feed(until, None, 0).await?;
        return Ok((global, true));
    }
    let mut filter = Filter::new()
        .kinds(feed_kinds())
        .authors(authors.clone())
        .limit(FEED_LIMIT);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    } else {
        let window = adaptive_since_window(authors.len());
        let adaptive_since = Timestamp::now().as_secs().saturating_sub(window);
        if let Some(since) = resolve_since(adaptive_since, cached_cursor) {
            filter = filter.since(Timestamp::from(since));
        }
    }
    log::info!(
        "Fetching all events (including replies and reposts) from {} followed accounts",
        filter.authors.as_ref().map(|a| a.len()).unwrap_or(0)
    );
    let fetch_result = {
        #[cfg(feature = "native")]
        {
            nostr_client::fetch_events_ndb_first(filter, Duration::from_secs(10)).await
        }
        #[cfg(not(feature = "native"))]
        {
            nostr_client::fetch_events_from_connected_relays(filter, Duration::from_secs(10)).await
        }
    };
    match fetch_result {
        Ok(events) => {
            log::info!(
                "Loaded {} events (including replies and reposts) from following feed via outbox",
                events.len()
            );
            let mut feed_items: Vec<FeedItem> = Vec::new();
            for event in events.into_iter() {
                if event.kind == Kind::Repost {
                    match extract_reposted_event(&event) {
                        Ok(original) => {
                            feed_items.push(FeedItem::Repost {
                                original,
                                reposted_by: event.pubkey,
                                repost_timestamp: event.created_at,
                            });
                        }
                        Err(e) => {
                            log::warn!("Failed to parse repost event {}: {}", event.id, e);
                        }
                    }
                } else if event.kind == Kind::TextNote || event.kind == Kind::Comment {
                    // Following+replies shows every conversation event from
                    // followed authors — kind-1 replies AND kind-1111
                    // comments (Amethyst "Conversations" parity). The
                    // `is_topic_post` gate stays on the plain Following tab.
                    feed_items.push(FeedItem::OriginalPost(event));
                }
            }
            feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
            if feed_items.is_empty() {
                log::info!(
                    "No events from followed users (incl replies), trying favorite relays"
                );
                match try_feed_from_favorite_relays(&authors, until).await {
                    Ok(extra_items) if !extra_items.is_empty() => {
                        log::info!("Got {} items from favorite relays", extra_items.len());
                        return Ok((extra_items, false));
                    }
                    _ => {
                        return Ok((Vec::new(), false));
                    }
                }
            }
            Ok((feed_items, false))
        }
        Err(e) => {
            log::error!(
                "Failed to fetch following feed with replies: {}, trying favorite relays before global fallback",
                e
            );
            match try_feed_from_favorite_relays(&authors, until).await {
                Ok(extra_items) if !extra_items.is_empty() => {
                    log::info!("Got {} items from favorite relays", extra_items.len());
                    Ok((extra_items, false))
                }
                _ => {
                    let global = load_global_feed(until, None, 0).await?;
                    Ok((global, true))
                }
            }
        }
    }
}

pub async fn load_global_feed(until: Option<u64>, cached_cursor: Option<u64>, _cached_count: usize) -> Result<Vec<FeedItem>, NostrBlueError> {
    log::info!("Loading global feed (until: {:?})...", until);
    let filter = build_global_feed_filter(until, cached_cursor);
    log::info!("Fetching events with filter: {:?}", filter);
    // db-merge (not `fetch_events_aggregated`): the aggregated pattern
    // returns the stale SDK-DB snapshot whenever the DB is non-empty and
    // discards the relay refresh in a fire-and-forget spawn, so brand-new
    // posts need a second refresh to surface. `fetch_events_db_merge`
    // paints from the DB but merges the network result into the returned
    // set (deduped by id) — one refresh shows fresh posts.
    match nostr_client::fetch_events_db_merge(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Loaded {} events", events.len());
            let mut feed_items: Vec<FeedItem> = Vec::new();
            for event in events.into_iter() {
                if event.kind == Kind::Repost {
                    match extract_reposted_event(&event) {
                        Ok(original) => {
                            feed_items.push(FeedItem::Repost {
                                original,
                                reposted_by: event.pubkey,
                                repost_timestamp: event.created_at,
                            });
                        }
                        Err(e) => {
                            log::warn!("Failed to parse repost event {}: {}", event.id, e);
                        }
                    }
                } else if event.kind == Kind::TextNote
                    || (event.kind == Kind::Comment
                        && crate::stores::topic_store::is_topic_post(&event))
                {
                    feed_items.push(FeedItem::OriginalPost(event));
                }
            }
            feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
            Ok(feed_items)
        }
        Err(e) => {
            log::error!("Failed to fetch events: {}", e);
            Err(NostrBlueError::Other(format!("Failed to load feed: {}", e)))
        }
    }
}

#[allow(dead_code)]
pub async fn fetch_quick_global_posts(
    limit: usize,
) -> Result<Vec<FeedItem>, crate::error::NostrBlueError> {
    log::info!("Fetching {} quick global posts...", limit);
    let filter = Filter::new()
        .kinds(feed_kinds())
        .limit(limit);

    let events = nostr_client::fetch_events_from_connected_relays(
        filter,
        Duration::from_secs(3),
    )
    .await?;

    let feed_items = process_events_to_feed_items(events);

    log::info!("Got {} quick global posts", feed_items.len());
    Ok(feed_items)
}

pub async fn prefetch_author_metadata(feed_items: &[FeedItem]) {
    use crate::utils::profile_prefetch;
    let mut pubkeys = Vec::new();
    for item in feed_items {
        match item {
            FeedItem::OriginalPost(event) => {
                pubkeys.push(event.pubkey);
            }
            FeedItem::Repost {
                original,
                reposted_by,
                ..
            } => {
                pubkeys.push(original.pubkey);
                pubkeys.push(*reposted_by);
            }
            FeedItem::Composite {
                underlying,
                reposts,
                ..
            } => {
                pubkeys.push(underlying.pubkey);
                for r in reposts {
                    pubkeys.push(r.by);
                }
            }
        }
    }
    pubkeys.sort();
    pubkeys.dedup();
    // Update the global feed-pubkey set for the periodic sweep safety net.
    {
        let pk_set: HashSet<String> = pubkeys.iter().map(|pk| pk.to_hex()).collect();
        *crate::stores::profiles::RECENT_FEED_PUBKEYS.write() = pk_set;
    }
    profile_prefetch::prefetch_pubkeys(pubkeys).await;
}

pub async fn prefetch_author_metadata_with_relays(feed_items: &[FeedItem]) {
    use crate::stores::relay::coverage;
    use crate::utils::profile_prefetch;
    let mut pubkeys = Vec::new();
    for item in feed_items {
        match item {
            FeedItem::OriginalPost(event) => {
                pubkeys.push(event.pubkey);
            }
            FeedItem::Repost {
                original,
                reposted_by,
                ..
            } => {
                pubkeys.push(original.pubkey);
                pubkeys.push(*reposted_by);
            }
            FeedItem::Composite {
                underlying,
                reposts,
                ..
            } => {
                pubkeys.push(underlying.pubkey);
                for r in reposts {
                    pubkeys.push(r.by);
                }
            }
        }
    }
    pubkeys.sort();
    pubkeys.dedup();
    profile_prefetch::prefetch_pubkeys(pubkeys.clone()).await;
    let pk_hexes: Vec<String> = pubkeys.iter().map(|pk| pk.to_hex()).collect();
    dioxus::prelude::spawn(async move {
        for pk_hex in pk_hexes {
            let _ = coverage::resolve_user_relays(
                &pk_hex,
                coverage::RelayPurpose::Write,
            )
            .await;
        }
    });
}

pub async fn load_people_list_feed(
    list: &UserList,
    until: Option<u64>,
    cached_cursor: Option<u64>,
    _cached_count: usize,
) -> Result<Vec<FeedItem>, NostrBlueError> {
    log::info!(
        "Loading people list feed for '{}' (until: {:?}, cursor: {:?})",
        list.name,
        until,
        cached_cursor
    );
    let members = get_all_list_members(&list.event).await.map_err(|e| {
        log::error!("Failed to get list members: {}", e);
        NostrBlueError::Other(format!("Failed to decrypt list members: {}", e))
    })?;
    if members.is_empty() {
        log::warn!(
            "People list '{}' has no members - check if private decryption failed",
            list.name
        );
        return Ok(Vec::new());
    }
    log::info!("People list '{}' has {} members", list.name, members.len());
    let member_count = members.len();
    let mut filter = Filter::new()
        .kinds(feed_kinds())
        .authors(members)
        .limit(FEED_LIMIT);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
        filter = filter.since(Timestamp::from(pagination_since_floor(
            until_ts,
            Some(member_count),
        )));
    } else {
        let window = adaptive_since_window(member_count);
        let adaptive_since = Timestamp::now().as_secs().saturating_sub(window);
        if let Some(since) = resolve_since(adaptive_since, cached_cursor) {
            filter = filter.since(Timestamp::from(since));
        }
    }
    match nostr_client::fetch_events_from_connected_relays(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!(
                "Loaded {} events from people list '{}'",
                events.len(),
                list.name
            );
            let mut feed_items: Vec<FeedItem> = Vec::new();
            for event in events.into_iter() {
                if event.kind == Kind::Repost {
                    match extract_reposted_event(&event) {
                        Ok(original) => {
                            feed_items.push(FeedItem::Repost {
                                original,
                                reposted_by: event.pubkey,
                                repost_timestamp: event.created_at,
                            });
                        }
                        Err(e) => {
                            log::warn!("Failed to parse repost event {}: {}", event.id, e);
                        }
                    }
                } else if event.kind == Kind::TextNote
                    || (event.kind == Kind::Comment
                        && crate::stores::topic_store::is_topic_post(&event))
                {
                    feed_items.push(FeedItem::OriginalPost(event));
                }
            }
            feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
            Ok(feed_items)
        }
        Err(e) => {
            log::error!(
                "Failed to fetch events for people list '{}': {}",
                list.name,
                e
            );
            Err(NostrBlueError::Other(format!("Failed to load feed: {}", e)))
        }
    }
}

pub async fn load_relay_feed(
    relay_urls: Vec<String>,
    until: Option<u64>,
    cached_cursor: Option<u64>,
    _cached_count: usize,
) -> Result<Vec<FeedItem>, NostrBlueError> {
    log::info!("Loading relay feed from {} relays (until: {:?}, cursor: {:?})", relay_urls.len(), until, cached_cursor);
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => return Err(NostrBlueError::Other("Client not initialized".to_string())),
    };
    for url in &relay_urls {
        let relay_url = match nostr_sdk::RelayUrl::parse(url) {
            Ok(u) => u,
            Err(_) => continue,
        };
        let relays = client.relays().await;
        if !relays.contains_key(&relay_url) {
            drop(relays);
            if let Err(e) = client.add_read_relay(url).await {
                log::warn!("Failed to add read relay {}: {}", url, e);
                continue;
            }
        }
        if let Err(e) = client.connect_relay(url).await {
            log::warn!("Failed to connect relay {}: {}", url, e);
        }
    }
    let mut filter = Filter::new().kinds(feed_kinds()).limit(RELAY_FEED_LIMIT);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
        filter = filter.since(Timestamp::from(pagination_since_floor(until_ts, None)));
    } else {
        let adaptive_since = Timestamp::now().as_secs().saturating_sub(86400);
        if let Some(since) = resolve_since(adaptive_since, cached_cursor) {
            filter = filter.since(Timestamp::from(since));
        }
    }
    crate::stores::relay::connection::fetch_events_from_relays(
        &client,
        filter,
        relay_urls.clone(),
        Duration::from_secs(10),
    )
    .await
    .map(|events| {
        log::info!("Relay feed: received {} events", events.len());
        let mut feed_items: Vec<FeedItem> = Vec::new();
        for event in events.into_iter() {
            if event.kind == Kind::Repost {
                match extract_reposted_event(&event) {
                    Ok(original) => {
                        feed_items.push(FeedItem::Repost {
                            original,
                            reposted_by: event.pubkey,
                            repost_timestamp: event.created_at,
                        });
                    }
                    Err(e) => {
                        log::warn!("Failed to parse repost event {}: {}", event.id, e);
                    }
                }
            } else if event.kind == Kind::TextNote
                || (event.kind == Kind::Comment
                    && crate::stores::topic_store::is_topic_post(&event))
            {
                feed_items.push(FeedItem::OriginalPost(event));
            }
        }
        feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
        feed_items
    })
    .map_err(|e| {
        log::error!("Failed to fetch relay feed: {}", e);
        NostrBlueError::Other(format!("Failed to load relay feed: {}", e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::{EventBuilder, Keys};

    fn test_post(secs: u64, content: &str) -> FeedItem {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, content)
            .custom_created_at(Timestamp::from(secs))
            .sign_with_keys(&keys)
            .expect("signed test note");
        FeedItem::OriginalPost(event)
    }

    fn test_author() -> PublicKey {
        Keys::generate().public_key()
    }

    #[test]
    fn exclusive_cursor_returns_timestamp() {
        let item = test_post(10, "hello");
        assert_eq!(exclusive_pagination_cursor(Some(&item)), Some(10));
    }

    #[test]
    fn following_filter_initial_load_uses_adaptive_window() {
        let now = Timestamp::from(200_000);
        let author = test_author();

        let filter = build_following_feed_filter(vec![author], None, now, None);

        assert_eq!(filter.authors.as_ref().map(|a| a.len()), Some(1));
        assert_eq!(filter.limit, Some(FEED_LIMIT));
        assert_eq!(
            filter.since,
            Some(now - Duration::from_secs(adaptive_since_window(1)))
        );
        assert_eq!(filter.until, None);
    }

    #[test]
    fn following_filter_paginated_load_uses_until_with_since_floor() {
        let now = Timestamp::from(200_000);
        let author = test_author();

        let filter = build_following_feed_filter(vec![author], Some(10_000_000), now, None);

        assert_eq!(filter.authors.as_ref().map(|a| a.len()), Some(1));
        assert_eq!(filter.limit, Some(FEED_LIMIT));
        assert_eq!(filter.until, Some(Timestamp::from(10_000_000)));
        // Single author: adaptive window (7d) exceeds the 24h pagination
        // window, so the floor is the adaptive window.
        assert_eq!(filter.since, Some(Timestamp::from(10_000_000 - 7 * 86400)));
    }

    #[test]
    fn following_filter_paginated_floor_widens_with_author_count() {
        let now = Timestamp::from(200_000);
        let authors: Vec<PublicKey> = (0..200).map(|_| test_author()).collect();

        let filter = build_following_feed_filter(authors, Some(1_000_000), now, None);

        // 200 authors: adaptive window (1d) loses to the 24h pagination
        // window, so the floor is the pagination window.
        assert_eq!(filter.since, Some(Timestamp::from(1_000_000 - 24 * 3600)));
    }

    #[test]
    fn global_filter_paginated_load_uses_since_floor() {
        let filter = build_global_feed_filter(Some(500_000), None);

        assert_eq!(filter.until, Some(Timestamp::from(500_000)));
        assert_eq!(filter.since, Some(Timestamp::from(500_000 - 24 * 3600)));
    }

    #[test]
    fn pagination_since_floor_saturates_at_zero() {
        assert_eq!(pagination_since_floor(1_000, None), 0);
        assert_eq!(pagination_since_floor(1_000, Some(0)), 0);
    }

    #[test]
    fn pagination_since_floor_uses_max_of_window_and_adaptive() {
        // Author-scoped: max(24h, adaptive)
        assert_eq!(pagination_since_floor(10 * 86400, Some(5)), 3 * 86400);
        assert_eq!(pagination_since_floor(10 * 86400, Some(500)), 9 * 86400);
        // Unscoped: fixed 24h
        assert_eq!(pagination_since_floor(10 * 86400, None), 9 * 86400);
    }

    #[test]
    fn merge_paginated_feed_items_dedupes_and_sorts_descending() {
        let newest = test_post(200, "newest");
        let middle = test_post(150, "middle");
        let existing = vec![newest.clone(), middle.clone()];
        let fetched = vec![middle, test_post(125, "older"), test_post(175, "between")];

        let (merged, unique, next_cursor) = merge_paginated_feed_items(existing, fetched);

        assert_eq!(unique.len(), 2);
        let timestamps: Vec<u64> = merged
            .iter()
            .map(|item| item.sort_timestamp().as_secs())
            .collect();
        assert_eq!(timestamps, vec![200, 175, 150, 125]);
        assert_eq!(next_cursor, Some(124));
    }

    #[test]
    fn duplicate_boundary_item_advances_cursor() {
        let boundary = test_post(100, "boundary");
        let existing = vec![test_post(120, "fresh"), boundary.clone()];
        let fetched = vec![boundary, test_post(90, "older")];

        let (merged, unique, next_cursor) = merge_paginated_feed_items(existing, fetched);

        assert_eq!(unique.len(), 1);
        let timestamps: Vec<u64> = merged
            .iter()
            .map(|item| item.sort_timestamp().as_secs())
            .collect();
        assert_eq!(timestamps, vec![120, 100, 90]);
        assert_eq!(next_cursor, Some(89));
    }

    #[test]
    fn adaptive_since_window_scales_with_author_count() {
        assert_eq!(adaptive_since_window(0), 7 * 86400);
        assert_eq!(adaptive_since_window(10), 7 * 86400);
        assert_eq!(adaptive_since_window(11), 5 * 86400);
        assert_eq!(adaptive_since_window(30), 5 * 86400);
        assert_eq!(adaptive_since_window(31), 2 * 86400);
        assert_eq!(adaptive_since_window(150), 2 * 86400);
        assert_eq!(adaptive_since_window(151), 86400);
        assert_eq!(adaptive_since_window(1000), 86400);
    }
}
