use crate::error::NostrBlueError;
use crate::feeds::classify::{classify_event, classify_events, FeedRules};
use crate::hooks::UserList;
use crate::stores::relay;
use crate::stores::{auth_store, nostr_client};
use crate::utils::list_encryption::get_all_list_members;
use crate::utils::{process_events_to_feed_items, FeedItem};
use dioxus::prelude::*;
use nostr::types::url::RelayUrl;
use nostr_sdk::prelude::{Filter, Kind, PublicKey, Timestamp};
use nostr_sdk::relay::{RelayStatus as PoolRelayStatus, SyncDirection, SyncOptions};
use nostr_sdk::client::Client;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

const SINCE_BUFFER_SECS: u64 = 120;
const RELAY_FEED_LIMIT: usize = 33;
pub const FEED_LIMIT: usize = 33;
/// Per-relay page limit for backward pagination (`until`-bounded pages).
///
/// Larger than the initial-load `FEED_LIMIT` on purpose: a correctly
/// cursored page walks into fresh territory, so a bigger page cuts the
/// number of full-author REQs (and relay subscription pressure) ~3x.
/// Amethyst's home REQ uses `min(authors * 10, 500)`; NIP-67 notes relays
/// commonly cap around 500 events per filter regardless of the requested
/// limit.
pub const PAGINATION_LIMIT: usize = 100;
#[cfg(feature = "native")]
const NDB_FEED_LIMIT: usize = 200;

/// One backward-pagination page: classified feed items plus the raw floor.
///
/// `raw_floor` is the minimum `created_at` across the fetched events
/// BEFORE classification. Replies and comments the feed rules drop never
/// make it into `items`, but they still bound the next page's cursor —
/// without them a page full of classified-away events would look "empty"
/// and end pagination early (Amethyst's pager: "a relay returning fewer
/// is its own cap, NOT exhaustion — only an empty page ends a relay").
#[derive(Debug, Clone, Default)]
pub struct FeedPage {
    pub items: Vec<FeedItem>,
    pub raw_floor: Option<u64>,
}

impl FeedPage {
    /// Classify `events` under `rules`, recording the raw floor first.
    pub fn from_events(events: Vec<nostr::event::Event>, rules: FeedRules) -> Self {
        let raw_floor = events.iter().map(|e| e.created_at.as_secs()).min();
        Self {
            items: classify_events(events, rules),
            raw_floor,
        }
    }
}

/// Classify `events` into a page, then hydrate NIP-18 reposts whose `content`
/// does not embed the original event (dropped by the classifier; the `e` tag
/// is the authoritative, required reference) so they join the page as normal
/// [`FeedItem::Repost`]s. One bounded `ids` fetch for the whole page's
/// dropped reposts — see `feeds::repost_hydration`.
async fn feed_page_with_hydration(
    events: Vec<nostr::event::Event>,
    rules: FeedRules,
) -> FeedPage {
    let candidates = crate::feeds::repost_hydration::select_hydration_candidates(&events);
    let mut page = FeedPage::from_events(events, rules);
    if candidates.is_empty() {
        return page;
    }
    let hydrated = crate::feeds::repost_hydration::hydrate_repost_batch(candidates).await;
    if hydrated.is_empty() {
        return page;
    }
    log::info!(
        "Hydrated {} e-tag-only repost(s) into feed page (page now {} items)",
        hydrated.len(),
        page.items.len() + hydrated.len()
    );
    page.items.extend(hydrated);
    page.items
        .sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
    page
}

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
/// mention backfills) surface in the feed in one page. The guarded
/// cache-to-feed path (`rebuildFeedFromCache`) applies a 24h `since`.
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
    // Incremental merge via FeedList (plan wave B1): no full re-sort, and
    // the cursor comes from the merged view's gap-proof walk.
    let mut list = crate::feeds::list::FeedList::from_sorted_items(current);
    let unique_items = list.insert_batch_returning(fetched_page);
    let next_cursor = list.cursor();
    (list.into_items(), unique_items, next_cursor)
}

pub fn build_global_feed_filter(until: Option<u64>, cached_cursor: Option<u64>) -> Filter {
    // Paginated pages are cursored into fresh territory: a larger page
    // limit cuts round trips. Initial loads keep the smaller FEED_LIMIT to
    // bound boot-time work.
    let limit = if until.is_some() {
        PAGINATION_LIMIT
    } else {
        FEED_LIMIT
    };
    let mut filter = Filter::new()
        .kinds(feed_kinds())
        .limit(limit);
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
    let limit = if until.is_some() {
        PAGINATION_LIMIT
    } else {
        FEED_LIMIT
    };
    let mut filter = Filter::new()
        .kinds(feed_kinds())
        .authors(authors)
        .limit(limit);

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
    match client.sync(filter).opts(sync_opts).await {
        Ok(output) => {
            log::info!(
                "Global feed negentropy sync complete: {} received, {} sent",
                output.value.received.len(),
                output.value.sent.len()
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

/// Max relays for following-feed negentropy reconciliation. Bounds sync cost
/// (one negentropy round per target) and keeps the operation clear of the
/// SDK gossip breakdown (see `sync_following_feed_page`).
const FOLLOWING_SYNC_TARGET_BUDGET: usize = 20;

/// Pick bounded, pool-member sync targets for the following feed: the user's
/// NIP-65 read relays first (they hold propagated feed events), then the
/// relays covering the most followed authors (author outboxes, ranked by the
/// coverage map). Degrades to connected READ-capability pool relays when
/// none of the candidates are pool members.
///
/// Every explicit target must be a pool member: `pool.sync` hard-fails the
/// entire call on a non-member URL, so candidates are intersected with
/// membership before use (same lesson as the `subscribe_target` fixes).
async fn resolve_following_sync_targets(
    client: &Arc<Client>,
    authors: &[PublicKey],
) -> Vec<RelayUrl> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut candidates: Vec<String> = relay::get_read_relays()
        .into_iter()
        .filter(|url| seen.insert(url.clone()))
        .collect();
    let coverage_budget = FOLLOWING_SYNC_TARGET_BUDGET.saturating_sub(candidates.len());
    if coverage_budget > 0 {
        for url in
            relay::coverage::top_relays_by_author_coverage(authors, coverage_budget)
        {
            if seen.insert(url.clone()) {
                candidates.push(url);
            }
        }
    }

    let members: HashSet<String> = client
        .relays()
        .all()
        .await
        .keys()
        .map(|u| u.as_str().to_string())
        .collect();
    let targets: Vec<RelayUrl> = candidates
        .iter()
        .filter(|url| members.contains(url.as_str()))
        .filter_map(|url| RelayUrl::parse(url.as_str()).ok())
        .collect();
    if !targets.is_empty() {
        if targets.len() < candidates.len() {
            log::debug!(
                "Following feed sync: {}/{} candidate relays are pool members",
                targets.len(),
                candidates.len()
            );
        }
        return targets;
    }

    // No candidates in the pool: degrade to connected READ relays (bounded
    // by the pool cap; same rule as services/sync.rs::resolve_sync_relays).
    client
        .relays()
        .await
        .iter()
        .filter(|(_, pool_relay)| {
            pool_relay.status() == PoolRelayStatus::Connected
                && pool_relay.capabilities().has_read()
        })
        .filter_map(|(url, _)| RelayUrl::parse(url.as_str()).ok())
        .collect()
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
    // Bounded explicit targets. An untargeted `client.sync(filter)` with
    // hundreds of authors makes the SDK gossip-break-down every author's
    // outbox and ADD+CONNECT each relay (client/gossip/updater.rs) until our
    // `max_relays(100)` pool cap fails the whole operation with
    // "too many relays" (observed live with 406 follows). `.with()` skips
    // the breakdown and the pool mutation entirely.
    let targets = resolve_following_sync_targets(&client, &authors).await;
    if targets.is_empty() {
        log::debug!("Following feed negentropy sync skipped: no usable targets");
        return;
    }
    let sync_opts = SyncOptions::default()
        .direction(SyncDirection::Down)
        .initial_timeout(Duration::from_secs(5));
    match client.sync(filter).with(targets).opts(sync_opts).await {
        Ok(output) => {
            if !output.failed.is_empty() {
                // The INFO line only carries counts; per-target failure
                // reasons (timeout, unsupported, …) go to debug for digs.
                log::debug!(
                    "Following feed sync failed targets: {:?}",
                    output
                        .failed
                        .iter()
                        .map(|(url, e)| (url.to_string(), e.to_string()))
                        .collect::<Vec<_>>()
                );
            }
            log::info!(
                "Following feed negentropy sync complete: {} received, {} sent, {} relays succeeded, {} failed",
                output.value.received.len(),
                output.value.sent.len(),
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
) -> Result<FeedPage, NostrBlueError> {
    sync_global_feed_page(until).await;
    load_global_feed(until, None, 0).await
}

pub async fn load_following_feed(
    until: Option<u64>,
    cached_cursor: Option<u64>,
    _cached_count: usize,
) -> Result<(FeedPage, bool), NostrBlueError> {
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
            let page = feed_page_with_hydration(events, FeedRules::FOLLOWING).await;
            log::info!(
                "Loaded {} events (including reposts) from following feed via outbox",
                raw_count
            );
            log::info!(
                "After processing: {} feed items (raw: {}), page floor: {:?}",
                page.items.len(),
                raw_count,
                page.raw_floor
            );
            if page.items.is_empty() {
                log::info!("No posts from followed users, trying favorite relays");
                match try_feed_from_favorite_relays(&authors, until).await {
                    Ok(fallback) if !fallback.items.is_empty() => {
                        log::info!("Got {} items from favorite relays", fallback.items.len());
                        return Ok((fallback, false));
                    }
                    _ => {
                        return Ok((FeedPage::default(), false));
                    }
                }
            }
            Ok((page, false))
        }
        Err(e) => {
            log::error!(
                "Failed to fetch following feed: {}, trying favorite relays before global fallback",
                e
            );
            match try_feed_from_favorite_relays(&authors, until).await {
                Ok(fallback) if !fallback.items.is_empty() => {
                    log::info!("Got {} items from favorite relays", fallback.items.len());
                    Ok((fallback, false))
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
) -> Result<FeedPage, NostrBlueError> {
    let favorite_urls = {
        use dioxus::prelude::ReadableExt;
        let relays = crate::stores::relay::nip65::FAVORITE_RELAYS.peek().clone();
        if relays.is_empty() {
            crate::stores::relay::nip65::default_favorite_relays()
        } else {
            relays
        }
    };
    let relay_urls: Vec<nostr::types::url::RelayUrl> = favorite_urls
        .iter()
        .filter_map(|s| nostr::types::url::RelayUrl::parse(s).ok())
        .collect();
    if relay_urls.is_empty() || authors.is_empty() {
        return Ok(FeedPage::default());
    }
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => return Ok(FeedPage::default()),
    };
    let filter = build_following_feed_filter(authors.to_vec(), until, Timestamp::now(), None);
    let targets: Vec<_> = relay_urls
        .into_iter()
        .map(|u| (u, vec![filter.clone()]))
        .collect();
    match crate::stores::nostr_client::fetch_with_client(
        &client,
        crate::stores::nostr_client::FetchTarget::Targeted(targets),
        crate::stores::nostr_client::FetchProfile::Slow,
    )
    .await
    {
        Ok(events) => Ok(feed_page_with_hydration(events, FeedRules::FOLLOWING).await),
        Err(e) => {
            log::warn!("Favorite relay feed fetch failed: {}", e);
            Ok(FeedPage::default())
        }
    }
}

/// Per-relay pagination page: fold inputs (one per relay that carried a
/// REQ this page) plus the classified cross-relay union page.
#[derive(Debug, Default)]
pub struct PerRelayPage {
    /// `(relay, raw floor of its deliveries, settled at EOSE)` — the
    /// inputs `RelayCursorSet::fold_page` consumes. The floor is the
    /// min `created_at` of the relay's raw deliveries, pre-classification
    /// AND pre-author-filter (a relay's cursor tracks its own timeline,
    /// so misbehaving echoes still bound it — clamped to the request
    /// bound at fold time).
    pub folds: Vec<(RelayUrl, Option<u64>, bool)>,
    pub page: FeedPage,
}

/// Shared per-relay fetch: one filter per relay (its own `until`),
/// author defense filter, cross-relay dedup, single classification.
async fn fetch_feed_page_per_relay(
    authors: &[PublicKey],
    rules: FeedRules,
    per_relay_untils: &[(RelayUrl, Option<u64>)],
    build_filter: impl Fn(Option<u64>) -> Filter,
) -> Result<PerRelayPage, NostrBlueError> {
    let client = nostr_client::get_client()
        .ok_or_else(|| NostrBlueError::Other("Client not initialized".to_string()))?;
    let author_set: HashSet<PublicKey> = authors.iter().copied().collect();
    let targets: Vec<(RelayUrl, Filter)> = per_relay_untils
        .iter()
        .map(|(url, until)| (url.clone(), build_filter(*until)))
        .collect();
    let results =
        nostr_client::fetch_page_per_relay(&client, targets, Duration::from_secs(10)).await;
    let mut folds = Vec::with_capacity(results.len());
    let mut seen: HashSet<nostr::event::EventId> = HashSet::new();
    let mut union = Vec::new();
    let mut total_raw = 0usize;
    for result in results {
        total_raw += result.events.len();
        let floor = result.events.iter().map(|e| e.created_at.as_secs()).min();
        for event in result.events {
            // Defense-in-depth against relays ignoring the authors
            // filter (same rule as the Phase-1 connected fetch).
            if !author_set.contains(&event.pubkey) {
                continue;
            }
            if seen.insert(event.id) {
                union.push(event);
            }
        }
        folds.push((result.url, floor, result.settled));
    }
    log::info!(
        "Per-relay page: {} relays ({} settled), {} raw -> {} unique events",
        folds.len(),
        folds.iter().filter(|(_, _, settled)| *settled).count(),
        total_raw,
        union.len()
    );
    Ok(PerRelayPage {
        folds,
        page: feed_page_with_hydration(union, rules).await,
    })
}

/// Resolve the followed-author set for the per-relay following loaders.
/// Errors defer to the Phase-1 union loader (which owns the global-feed
/// fallback chain).
async fn following_authors() -> Result<Vec<PublicKey>, NostrBlueError> {
    let pubkey_str = auth_store::get_pubkey().ok_or(NostrBlueError::NotAuthenticated)?;
    let contacts = nostr_client::fetch_contacts(pubkey_str).await.map_err(|e| {
        log::warn!("Per-relay pagination: failed to fetch contacts: {}", e);
        NostrBlueError::Other(format!("Failed to fetch contacts: {}", e))
    })?;
    let authors: Vec<PublicKey> = contacts
        .iter()
        .filter_map(|c| PublicKey::parse(c).ok())
        .collect();
    if authors.is_empty() {
        return Err(NostrBlueError::Other("No followed accounts".to_string()));
    }
    Ok(authors)
}

/// Phase-2 per-relay variant of `load_following_feed` for pagination:
/// every relay gets its own `until` (its own cursor) instead of the
/// global floor, and the page reports per-relay fold inputs.
pub async fn load_following_feed_per_relay(
    per_relay_untils: &[(RelayUrl, Option<u64>)],
) -> Result<PerRelayPage, NostrBlueError> {
    let authors = following_authors().await?;
    log::info!(
        "Per-relay following page: {} relays, {} authors",
        per_relay_untils.len(),
        authors.len()
    );
    let now = Timestamp::now();
    fetch_feed_page_per_relay(&authors, FeedRules::FOLLOWING, per_relay_untils, |until| {
        build_following_feed_filter(authors.clone(), until, now, None)
    })
    .await
}

/// Phase-2 per-relay variant of `load_following_with_replies`.
pub async fn load_following_with_replies_per_relay(
    per_relay_untils: &[(RelayUrl, Option<u64>)],
) -> Result<PerRelayPage, NostrBlueError> {
    let authors = following_authors().await?;
    fetch_feed_page_per_relay(
        &authors,
        FeedRules::FOLLOWING_WITH_REPLIES,
        per_relay_untils,
        |until| {
            // Parity with `load_following_with_replies`: paginated pages
            // bound only `until` (replies live anywhere in the past).
            let limit = if until.is_some() {
                PAGINATION_LIMIT
            } else {
                FEED_LIMIT
            };
            let mut filter = Filter::new()
                .kinds(feed_kinds())
                .authors(authors.clone())
                .limit(limit);
            if let Some(until_ts) = until {
                filter = filter.until(Timestamp::from(until_ts));
            } else {
                let window = adaptive_since_window(authors.len());
                let adaptive_since = Timestamp::now().as_secs().saturating_sub(window);
                if let Some(since) = resolve_since(adaptive_since, None) {
                    filter = filter.since(Timestamp::from(since));
                }
            }
            filter
        },
    )
    .await
}

/// Phase-2 per-relay variant of `load_people_list_feed`. An empty member
/// list yields an empty page (no folds) — the caller settles on it.
pub async fn load_people_list_feed_per_relay(
    list: &UserList,
    per_relay_untils: &[(RelayUrl, Option<u64>)],
) -> Result<PerRelayPage, NostrBlueError> {
    let members = get_all_list_members(&list.event).await.map_err(|e| {
        log::error!("Failed to get list members: {}", e);
        NostrBlueError::Other(format!("Failed to decrypt list members: {}", e))
    })?;
    if members.is_empty() {
        log::warn!("People list '{}' has no members", list.name);
        return Ok(PerRelayPage::default());
    }
    let member_count = members.len();
    fetch_feed_page_per_relay(&members, FeedRules::BROADCAST, per_relay_untils, |until| {
        // Parity with `load_people_list_feed`'s paginated branch.
        let limit = if until.is_some() {
            PAGINATION_LIMIT
        } else {
            FEED_LIMIT
        };
        let mut filter = Filter::new()
            .kinds(feed_kinds())
            .authors(members.clone())
            .limit(limit);
        if let Some(until_ts) = until {
            filter = filter.until(Timestamp::from(until_ts));
            filter = filter.since(Timestamp::from(pagination_since_floor(
                until_ts,
                Some(member_count),
            )));
        } else {
            let window = adaptive_since_window(member_count);
            let adaptive_since = Timestamp::now().as_secs().saturating_sub(window);
            if let Some(since) = resolve_since(adaptive_since, None) {
                filter = filter.since(Timestamp::from(since));
            }
        }
        filter
    })
    .await
}

/// Map a streamed event to a top-level Following-feed item via the unified
/// classifier (replies dropped, reposts unwrapped, topic posts from comments
/// included). Borrows so the streaming closure can consult the same event
/// for the e-tag hydration fallback when classification drops it.
fn following_stream_event_to_item(event: &nostr::event::Event) -> Option<FeedItem> {
    classify_event(event, FeedRules::FOLLOWING)
}

/// Dropped-repost accumulator for one streaming pass: NIP-18 reposts whose
/// content doesn't embed the original (classifier drop), deduped by
/// referenced original id. Hydrated after the stream completes.
#[derive(Default)]
struct DroppedReposts {
    refs: std::collections::HashSet<nostr::event::EventId>,
    events: Vec<nostr::event::Event>,
}

impl DroppedReposts {
    fn push(&mut self, event: nostr::event::Event, ref_id: nostr::event::EventId) {
        if self.refs.insert(ref_id) {
            self.events.push(event);
        }
    }
}

/// Stream one following-feed batch into `all_items`, deduping via `seen_ids`
/// and forwarding each accepted item to `on_batch`. Reposts the classifier
/// drops (empty `content`, original referenced only by `e` tag) accumulate
/// into `dropped` for post-stream hydration.
#[allow(clippy::too_many_arguments)]
async fn stream_following_batch<F>(
    filter: Filter,
    timeout: Duration,
    all_items: &mut Vec<FeedItem>,
    seen_ids: &mut std::collections::HashSet<nostr::event::EventId>,
    dropped: &mut DroppedReposts,
    on_batch: &mut F,
) -> Result<(), NostrBlueError>
where
    F: FnMut(Vec<FeedItem>),
{
    nostr_client::stream_events_immediate(filter, timeout, |event| {
        if let Some(feed_item) = following_stream_event_to_item(&event) {
            if seen_ids.insert(feed_item.event().id) {
                all_items.push(feed_item.clone());
                on_batch(vec![feed_item]);
            }
        } else if let Some(ref_id) = crate::utils::repost::repost_original_ref(&event) {
            dropped.push(event, ref_id);
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
) -> Result<(FeedPage, bool), NostrBlueError>
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
    let mut seen_ids: HashSet<nostr::event::EventId> = HashSet::new();
    let mut dropped = DroppedReposts::default();

    #[cfg(feature = "native")]
    {
        if let Some(client) = nostr_client::get_client() {
            let ndb_filter = filter.clone().limit(NDB_FEED_LIMIT);
            if let Ok(db_events) = client.database().query(ndb_filter).await {
                if !db_events.is_empty() {
                    let db_events: Vec<nostr::event::Event> =
                        db_events.into_iter().collect();
                    // E-tag-only reposts in the local DB are dropped by the
                    // classifier below; capture them for hydration too.
                    for repost in crate::feeds::repost_hydration::select_hydration_candidates(
                        &db_events,
                    ) {
                        if let Some(ref_id) = crate::utils::repost::repost_original_ref(&repost) {
                            dropped.push(repost, ref_id);
                        }
                    }
                    let db_items = process_events_to_feed_items(db_events);
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
        &mut dropped,
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
                    &mut dropped,
                    &mut on_batch,
                )
                .await;
            }
        }
    }
    // NIP-18 e-tag-only reposts dropped during the NDB pre-step / stream:
    // hydrate their originals in one bounded fetch so they join the page —
    // without this the feed cursor stays pinned below them and every load
    // re-fetches and re-drops them.
    if !dropped.events.is_empty() {
        let hydrated = crate::feeds::repost_hydration::hydrate_repost_batch(std::mem::take(
            &mut dropped.events,
        ))
        .await;
        let mut hydrated_count = 0usize;
        for item in hydrated {
            if seen_ids.insert(item.event().id) {
                all_items.push(item.clone());
                on_batch(vec![item]);
                hydrated_count += 1;
            }
        }
        if hydrated_count > 0 {
            log::info!(
                "Hydrated {} e-tag-only repost(s) after following stream",
                hydrated_count
            );
        }
    }
    if all_items.is_empty() {
        log::info!("No posts from followed users via streaming");
        return Ok((FeedPage::default(), false));
    }
    all_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
    log::info!("Streaming complete: {} feed items", all_items.len());
    // Initial-load path: `raw_floor` is a pagination concern (the cursor
    // session starts from the first pagination page's floor), so it stays
    // `None` here.
    Ok((
        FeedPage {
            items: all_items,
            raw_floor: None,
        },
        false,
    ))
}

pub async fn load_following_with_replies(
    until: Option<u64>,
    cached_cursor: Option<u64>,
    _cached_count: usize,
) -> Result<(FeedPage, bool), NostrBlueError> {
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
    let limit = if until.is_some() {
        PAGINATION_LIMIT
    } else {
        FEED_LIMIT
    };
    let mut filter = Filter::new()
        .kinds(feed_kinds())
        .authors(authors.clone())
        .limit(limit);
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
            let raw_count = events.len();
            let page =
                feed_page_with_hydration(events, FeedRules::FOLLOWING_WITH_REPLIES).await;
            log::info!(
                "Loaded {} events (including replies and reposts) from following feed via outbox",
                raw_count
            );
            log::info!(
                "After processing: {} feed items (raw: {}), page floor: {:?}",
                page.items.len(),
                raw_count,
                page.raw_floor
            );
            if page.items.is_empty() {
                log::info!(
                    "No events from followed users (incl replies), trying favorite relays"
                );
                match try_feed_from_favorite_relays(&authors, until).await {
                    Ok(fallback) if !fallback.items.is_empty() => {
                        log::info!("Got {} items from favorite relays", fallback.items.len());
                        return Ok((fallback, false));
                    }
                    _ => {
                        return Ok((FeedPage::default(), false));
                    }
                }
            }
            Ok((page, false))
        }
        Err(e) => {
            log::error!(
                "Failed to fetch following feed with replies: {}, trying favorite relays before global fallback",
                e
            );
            match try_feed_from_favorite_relays(&authors, until).await {
                Ok(fallback) if !fallback.items.is_empty() => {
                    log::info!("Got {} items from favorite relays", fallback.items.len());
                    Ok((fallback, false))
                }
                _ => {
                    let global = load_global_feed(until, None, 0).await?;
                    Ok((global, true))
                }
            }
        }
    }
}

pub async fn load_global_feed(until: Option<u64>, cached_cursor: Option<u64>, _cached_count: usize) -> Result<FeedPage, NostrBlueError> {
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
            // Broadcast rules: global keeps kind-1 replies (no thread
            // context to drop them from); comments only as topic posts.
            Ok(feed_page_with_hydration(events, FeedRules::BROADCAST).await)
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
) -> Result<FeedPage, NostrBlueError> {
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
        return Ok(FeedPage::default());
    }
    log::info!("People list '{}' has {} members", list.name, members.len());
    let member_count = members.len();
    let limit = if until.is_some() {
        PAGINATION_LIMIT
    } else {
        FEED_LIMIT
    };
    let mut filter = Filter::new()
        .kinds(feed_kinds())
        .authors(members)
        .limit(limit);
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
            Ok(feed_page_with_hydration(events, FeedRules::BROADCAST).await)
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
) -> Result<FeedPage, NostrBlueError> {
    log::info!("Loading relay feed from {} relays (until: {:?}, cursor: {:?})", relay_urls.len(), until, cached_cursor);
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => return Err(NostrBlueError::Other("Client not initialized".to_string())),
    };
    for url in &relay_urls {
        let relay_url = match nostr::types::url::RelayUrl::parse(url) {
            Ok(u) => u,
            Err(_) => continue,
        };
        let relays = client.relays().await;
        if !relays.contains_key(&relay_url) {
            drop(relays);
            if let Err(e) = client
                .add_relay(url)
                .capabilities(nostr_sdk::relay::RelayCapabilities::READ)
                .await
            {
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
    let fetched = crate::stores::relay::connection::fetch_events_from_relays(
        &client,
        filter,
        relay_urls.clone(),
        Duration::from_secs(10),
    )
    .await;
    match fetched {
        Ok(events) => {
            log::info!("Relay feed: received {} events", events.len());
            Ok(feed_page_with_hydration(events, FeedRules::BROADCAST).await)
        }
        Err(e) => {
            log::error!("Failed to fetch relay feed: {}", e);
            Err(NostrBlueError::Other(format!(
                "Failed to load relay feed: {}",
                e
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::event::FinalizeEvent;
    use nostr_sdk::prelude::{EventBuilder, Keys};

    fn test_post(secs: u64, content: &str) -> FeedItem {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, content)
            .custom_created_at(Timestamp::from(secs))
            .finalize(&keys)
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
        assert_eq!(filter.limit, Some(PAGINATION_LIMIT));
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
        assert_eq!(filter.limit, Some(PAGINATION_LIMIT));
    }

    #[test]
    fn global_filter_initial_load_keeps_feed_limit() {
        let filter = build_global_feed_filter(None, None);

        assert_eq!(filter.until, None);
        assert_eq!(filter.limit, Some(FEED_LIMIT));
    }

    fn test_event(kind: Kind, secs: u64, content: &str, tags: Vec<nostr::event::Tag>) -> nostr::event::Event {
        let keys = Keys::generate();
        EventBuilder::new(kind, content)
            .tags(tags)
            .custom_created_at(Timestamp::from(secs))
            .finalize(&keys)
            .expect("signed test event")
    }

    #[test]
    fn feed_page_records_raw_floor_before_classification() {
        use nostr::event::EventId;
        use nostr::nips::nip10::{Marker, Nip10Tag};
        // A thread reply at t=100 (dropped by FOLLOWING rules) and a
        // top-level post at t=300 (kept): the raw floor must still be 100 —
        // classified-away events bound the next page's cursor too.
        let reply_tag = nostr::event::Tag::from(Nip10Tag::Event {
            id: EventId::from_slice(&[1u8; 32]).expect("valid id"),
            relay_hint: None,
            marker: Some(Marker::Root),
            public_key: None,
        });
        let events = vec![
            test_event(Kind::TextNote, 300, "post", vec![]),
            test_event(Kind::TextNote, 100, "reply", vec![reply_tag]),
        ];

        let page = FeedPage::from_events(events, FeedRules::FOLLOWING);

        assert_eq!(page.raw_floor, Some(100));
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].sort_timestamp().as_secs(), 300);
    }

    #[test]
    fn feed_page_empty_events_yield_no_floor() {
        let page = FeedPage::from_events(Vec::new(), FeedRules::FOLLOWING);
        assert_eq!(page.raw_floor, None);
        assert!(page.items.is_empty());
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
