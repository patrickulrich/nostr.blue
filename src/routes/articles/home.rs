use crate::components::{ArticleCard, ArticleCardSkeleton, ClientInitializing};
use crate::hooks::use_infinite_scroll_with_generation;
use crate::stores::feed_cache::FeedCacheKey;
use crate::stores::relay::{self, USER_RELAYS_APPLIED};
use crate::stores::{auth_store, feed_cache, nostr_client};
use crate::utils::article_meta::{get_identifier, get_published_at};
use crate::utils::pagination::is_likely_future;
use crate::utils::FeedItem;
use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, PublicKey, Timestamp};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
#[derive(Clone, Copy, PartialEq, Debug)]
enum FeedType {
    Following,
    Global,
}
impl FeedType {
    fn label(&self) -> &'static str {
        match self {
            FeedType::Following => "Following",
            FeedType::Global => "Global",
        }
    }
}
#[component]
pub fn Articles() -> Element {
    let mut articles = use_signal(Vec::<Event>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut request_id = use_signal(|| 0u32);
    let mut last_loaded_trigger = use_signal(|| (0u32, FeedType::Following));
    let mut optimistic_articles = use_signal(Vec::<Event>::new);
    let mut articles_reset_generation = use_signal(|| 0u64);
    use_effect(move || {
        let refresh = *refresh_trigger.read();
        let current_feed_type = *feed_type.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = *nostr_client::HAS_SIGNER.read();
        let relays_applied = *USER_RELAYS_APPLIED.read();
        if !client_initialized {
            return;
        }
        // Relay readiness gate (mirrors routes/dms.rs): for signed-in users,
        // wait until the NIP-65 relay list is applied so the fetch targets
        // the full pool instead of racing it with only DEFAULT_RELAYS.
        if has_signer && !relays_applied {
            return;
        }
        let (last_refresh, last_feed) = *last_loaded_trigger.peek();
        let has_data = !articles.peek().is_empty();
        let feed_type_changed = current_feed_type != last_feed;
        let refresh_changed = refresh != last_refresh;
        if has_data && !feed_type_changed && !refresh_changed {
            log::debug!(
                "Skipping articles feed re-load: data already present, no intentional change"
            );
            return;
        }
        last_loaded_trigger.set((refresh, current_feed_type));
        let current_id = *request_id.peek() + 1;
        request_id.set(current_id);
        // Intentional reload: reset pagination state. The generation bump
        // re-attaches the infinite-scroll observer (the loading branch may
        // unmount the sentinel — see `use_infinite_scroll_with_generation`).
        // `loading` is released here: an in-flight page from the previous
        // feed is invalidated by `request_id` and must not clear the flag
        // itself (mirrors the kind-1 home feed's stale-token discipline).
        articles_reset_generation += 1;
        loading.set(false);
        if !has_data {
            loading.set(true);
        }
        error.set(None);
        oldest_timestamp.set(None);
        has_more.set(true);
        spawn(async move {
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale articles feed request {}", current_id);
                return;
            }
            // No-op when logged out or already applied; the effect gate above
            // makes this a defensive second check for mid-flight transitions.
            relay::wait_for_user_relays(Duration::from_secs(10), "articles feed").await;
            let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
            let cache_key = match current_feed_type {
                FeedType::Following => FeedCacheKey::Articles { pubkey: pubkey_str },
                FeedType::Global => FeedCacheKey::ArticlesGlobal,
            };
            let cached_items = feed_cache::load_cached_feed(&cache_key, 100)
                .await
                .unwrap_or_default();
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale articles request after cache load");
                return;
            }
            if !cached_items.is_empty() {
                log::info!("Loaded {} articles from cache", cached_items.len());
                let cached_events: Vec<Event> =
                    cached_items.iter().map(|i| i.event().clone()).collect();
                // Cursor: exclusive floor of the raw page (min created_at - 1).
                if let Some(oldest) = cached_events
                    .iter()
                    .map(|e| e.created_at.as_secs())
                    .min()
                {
                    oldest_timestamp.set(Some(oldest.saturating_sub(1)));
                }
                articles.set(cached_events);
            }
            let result = match current_feed_type {
                FeedType::Following => load_following_articles(None).await,
                FeedType::Global => load_articles(None).await.map(|e| (e, false)),
            };
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale articles request after network load");
                return;
            }
            match result {
                Ok((feed_events, did_fallback)) => {
                    let effective_cache_key = if did_fallback {
                        log::info!("No contacts, switched to Global articles feed");
                        feed_type.set(FeedType::Global);
                        FeedCacheKey::ArticlesGlobal
                    } else {
                        cache_key.clone()
                    };
                    // Cursor: exclusive floor of the raw page. Pages come
                    // back newest-first from the loaders; use min for safety.
                    if !feed_events.is_empty() {
                        if let Some(oldest) = feed_events
                            .iter()
                            .map(|e| e.created_at.as_secs())
                            .min()
                        {
                            oldest_timestamp.set(Some(oldest.saturating_sub(1)));
                        }
                    }
                    let feed_items: Vec<FeedItem> = feed_events
                        .iter()
                        .map(|e| FeedItem::OriginalPost(e.clone()))
                        .collect();
                    spawn(async move {
                        let _ =
                            feed_cache::store_feed_items(&effective_cache_key, &feed_items).await;
                        let _ = feed_cache::run_eviction_if_needed().await;
                    });
                    // Progress-based: a sparse (sub-limit) page still walks
                    // back one window on the next scroll; only a truly empty
                    // page ends the feed. Pages are the union across relays,
                    // so "fewer than the limit" says nothing about depth.
                    has_more.set(!feed_events.is_empty());
                    articles.set(feed_events);
                    loading.set(false);
                }
                Err(e) => {
                    if cached_items.is_empty() {
                        error.set(Some(e));
                    } else {
                        log::warn!("Network error but showing cached articles: {}", e);
                    }
                    loading.set(false);
                }
            }
        });
    });
    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }
        let until = *oldest_timestamp.read();
        let current_feed_type = *feed_type.read();
        // Snapshot the request generation: `request_id` is bumped on every
        // intentional reload (refresh / feed switch). If that happens while
        // this page is in flight, the result belongs to the previous feed
        // and must be discarded WITHOUT touching any signal (the reset
        // block owns `loading` and the list).
        let current_id = *request_id.peek();
        loading.set(true);
        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => match load_following_articles(until).await {
                    Ok((events, did_fallback)) => {
                        if did_fallback {
                            log::info!(
                                "Pagination fallback detected, ending pagination to preserve feed type"
                            );
                            Ok(Vec::new())
                        } else {
                            Ok(events)
                        }
                    }
                    Err(e) => Err(e),
                },
                FeedType::Global => load_articles(until).await,
            };
            if *request_id.peek() != current_id {
                log::debug!(
                    "Discarding stale articles page (request {} superseded)",
                    current_id
                );
                return;
            }
            match result {
                Ok(new_articles) => {
                    if new_articles.is_empty() {
                        log::info!("No more articles from relays, reached end of feed");
                        has_more.set(false);
                        loading.set(false);
                        return;
                    }
                    if let Some(oldest) = new_articles
                        .iter()
                        .map(|e| e.created_at.as_secs())
                        .min()
                    {
                        oldest_timestamp.set(Some(oldest.saturating_sub(1)));
                    }
                    has_more.set(true);
                    // Id-level append only; address dedup and ordering are
                    // the render memo's job, so append order doesn't matter.
                    let mut current = articles.read().clone();
                    let existing_ids: HashSet<_> = current.iter().map(|e| e.id).collect();
                    for article in new_articles {
                        if !existing_ids.contains(&article.id) {
                            current.push(article);
                        }
                    }
                    articles.set(current);
                    loading.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load more articles: {}", e);
                    loading.set(false);
                }
            }
        });
    };
    let sentinel_id = use_infinite_scroll_with_generation(
        load_more,
        has_more,
        loading,
        articles_reset_generation,
    );
    use_effect(move || {
        let queued = feed_cache::OPTIMISTIC_FEED_INSERTS.read().clone();
        if queued.is_empty() {
            return;
        }
        // Take only kind 30023 originals; leave notes for the home feed's drain.
        let drained = feed_cache::drain_optimistic_feed_items_matching(|item| {
            matches!(item, FeedItem::OriginalPost(e) if e.kind == Kind::LongFormTextNote)
        });
        if drained.is_empty() {
            return;
        }
        let events: Vec<Event> = drained
            .into_iter()
            .filter_map(|item| match item {
                FeedItem::OriginalPost(e) => Some(e),
                _ => None,
            })
            .collect();
        log::info!(
            "Drained {} optimistic articles into the articles feed",
            events.len()
        );
        optimistic_articles.write().extend(events);
    });
    // Single dedup/ordering point for the feed. Always runs (even without
    // optimistic items) so paginated appends and cross-page edits of already
    // shown articles can't produce duplicates or append-order lists. A memo
    // survives `articles.set()` replacing the loaded list.
    let article_list = use_memo(move || {
        let loaded = articles.read();
        let optimistic = optimistic_articles.read();
        merge_and_sort_articles(&loaded, &optimistic)
    });
    let is_loading = *loading.read();
    let error_msg = error.read();
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center justify-between",
                    div { class: "relative",
                        button {
                            class: "text-xl font-bold flex items-center gap-2 hover:bg-accent px-3 py-1 rounded-lg transition",
                            onclick: move |_| {
                                let current = *show_dropdown.read();
                                show_dropdown.set(!current);
                            },
                            "📚 {feed_type.read().label()}"
                            span { class: "text-sm",
                                if *show_dropdown.read() {
                                    "▲"
                                } else {
                                    "▼"
                                }
                            }
                        }
                        if *show_dropdown.read() {
                            div { class: "absolute top-full left-0 mt-2 bg-card border border-border rounded-lg shadow-lg min-w-[200px] overflow-hidden z-30",
                                button {
                                    class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                    onclick: move |_| {
                                        feed_type.set(FeedType::Following);
                                        show_dropdown.set(false);
                                    },
                                    div {
                                        div { class: "font-medium", "Following" }
                                        div { class: "text-xs text-muted-foreground",
                                            "Articles from people you follow"
                                        }
                                    }
                                    if *feed_type.read() == FeedType::Following {
                                        span { "✓" }
                                    }
                                }
                                div { class: "border-t border-border" }
                                button {
                                    class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                    onclick: move |_| {
                                        feed_type.set(FeedType::Global);
                                        show_dropdown.set(false);
                                    },
                                    div {
                                        div { class: "font-medium", "Global" }
                                        div { class: "text-xs text-muted-foreground",
                                            "Articles from across the network"
                                        }
                                    }
                                    if *feed_type.read() == FeedType::Global {
                                        span { "✓" }
                                    }
                                }
                            }
                        }
                    }
                    button {
                        class: "text-sm px-3 py-1 rounded-lg hover:bg-accent transition",
                        onclick: move |_| {
                            let current = *refresh_trigger.peek();
                            refresh_trigger.set(current + 1);
                        },
                        "↻ Refresh"
                    }
                }
            }
            if let Some(err) = error_msg.as_ref() {
                div { class: "p-4 bg-destructive/10 border border-destructive text-destructive",
                    p { "Failed to load articles: {err}" }
                    button {
                        class: "mt-2 px-3 py-1 bg-destructive text-destructive-foreground rounded-lg",
                        onclick: move |_| {
                            let current = *refresh_trigger.peek();
                            refresh_trigger.set(current + 1);
                        },
                        "Try Again"
                    }
                }
            }
            div { class: "p-4",
                if !*nostr_client::CLIENT_INITIALIZED.read() || (is_loading && article_list.is_empty()) {
                    ClientInitializing {}
                } else if article_list.is_empty() {
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "📚" }
                        h3 { class: "text-xl font-semibold mb-2", "No Articles Found" }
                        p { class: "text-muted-foreground text-sm mb-4",
                            "Check back later for long-form content from the Nostr network."
                        }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90",
                            onclick: move |_| {
                                let current = *refresh_trigger.peek();
                                refresh_trigger.set(current + 1);
                            },
                            "Refresh"
                        }
                    }
                } else {
                    div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                        for article in article_list.iter() {
                            ArticleCard { key: "{article.id}", event: article.clone() }
                        }
                    }
                    if *has_more.read() {
                        div {
                            id: "{sentinel_id}",
                            class: "h-20 flex items-center justify-center",
                            if is_loading {
                                div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 w-full",
                                    for _ in 0..3 {
                                        ArticleCardSkeleton {}
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "text-center py-8 text-muted-foreground text-sm",
                            "You've reached the end"
                        }
                    }
                }
            }
        }
    }
}
/// Dedup key for an article event: address (kind:pubkey:identifier) for
/// addressable events, event id as fallback for malformed ones.
fn article_address_key(event: &Event) -> String {
    match get_identifier(event) {
        Some(identifier) => format!(
            "{}:{}:{}",
            event.kind.as_u16(),
            event.pubkey.to_hex(),
            identifier,
        ),
        None => event.id.to_hex(),
    }
}
/// Merge loaded and optimistic article events into the render list.
///
/// Dedup by address (NIP-23: clients "should take care to hide old versions
/// of the same article"), keeping the newest `created_at` version — all
/// versions of one article carry the same `published_at` tag, so the newest
/// version preserves the original publication date. Sort descending by
/// `published_at` (falling back to `created_at`), the exact date the card
/// displays, so edits never reorder the feed.
fn merge_and_sort_articles(loaded: &[Event], optimistic: &[Event]) -> Vec<Event> {
    let mut address_map: HashMap<String, Event> = HashMap::new();
    for article in loaded.iter().chain(optimistic.iter()) {
        let key = article_address_key(article);
        address_map
            .entry(key)
            .and_modify(|existing| {
                if article.created_at > existing.created_at {
                    *existing = article.clone();
                }
            })
            .or_insert_with(|| article.clone());
    }
    let mut merged: Vec<Event> = address_map.into_values().collect();
    merged.sort_by_key(|b| std::cmp::Reverse(get_published_at(b)));
    merged
}
/// Page limit for the global articles REQ (per relay; results are the union
/// across relays plus the local DB, so pages may exceed this).
const GLOBAL_PAGE_LIMIT: usize = 20;
/// Page limit for the following articles REQ. Higher than global: the
/// author set is bounded and per-author article counts are sparse.
const FOLLOWING_PAGE_LIMIT: usize = 100;
/// Load one raw page of global articles (future-filtered, newest first).
///
/// Raw window semantics: no address dedup, no truncation — either would drop
/// events from the middle of the window and corrupt the caller's pagination
/// cursor. Dedup/ordering live in `merge_and_sort_articles`.
async fn load_articles(until: Option<u64>) -> Result<Vec<Event>, String> {
    let mut events = nostr_client::fetch_articles(GLOBAL_PAGE_LIMIT, until).await?;
    events.retain(|e| !is_likely_future(e.created_at));
    Ok(events)
}
/// Load articles from followed users (plus self).
/// Returns (articles, did_fallback) where did_fallback indicates if we fell back to global.
/// Raw window semantics — see [`load_articles`].
async fn load_following_articles(until: Option<u64>) -> Result<(Vec<Event>, bool), String> {
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    log::info!(
        "Loading following articles for {} (until: {:?})",
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
            let global = load_articles(until).await?;
            return Ok((global, true));
        }
    };
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global articles");
        let global = load_articles(until).await?;
        return Ok((global, true));
    }
    log::info!("User follows {} accounts", contacts.len());
    // Include self so the user's own articles appear in their Following feed
    // (the contact list never contains one's own pubkey).
    let mut seen_authors: HashSet<PublicKey> = HashSet::new();
    let mut authors = Vec::new();
    if let Ok(pk) = PublicKey::parse(&pubkey_str) {
        seen_authors.insert(pk);
        authors.push(pk);
    }
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            if seen_authors.insert(pk) {
                authors.push(pk);
            }
        }
    }
    if authors.is_empty() {
        log::warn!("No valid contact pubkeys, falling back to global feed");
        let global = load_articles(until).await?;
        return Ok((global, true));
    }
    let mut filter = Filter::new()
        .kind(Kind::LongFormTextNote)
        .authors(authors)
        .limit(FOLLOWING_PAGE_LIMIT);
    if let Some(until_ts) = until {
        // Since-floored pagination window (mirrors the kind-1 feed's
        // `pagination_since_floor`): an `until`-only filter returns
        // unbounded-old pages from both the DB and relays.
        filter = filter
            .until(Timestamp::from(until_ts))
            .since(Timestamp::from(
                until_ts.saturating_sub(nostr_client::ARTICLES_PAGINATION_WINDOW_SECS),
            ));
    }
    log::info!(
        "Fetching articles from {} followed accounts",
        filter.authors.as_ref().map(|a| a.len()).unwrap_or(0)
    );
    // Gossip-bypassing fetch: the author-scoped filter through the gossip
    // path cold-starts with a NIP-65 negentropy sync for every author and
    // yields ~zero events on web (see fetch_events_db_merge_from_connected).
    match nostr_client::fetch_events_db_merge_from_connected(filter, Duration::from_secs(10))
        .await
    {
        Ok(mut events) => {
            events.retain(|e| !is_likely_future(e.created_at));
            events.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            log::info!("Loaded {} raw articles from following feed", events.len());
            Ok((events, false))
        }
        Err(e) => {
            log::error!(
                "Failed to fetch following articles: {}, falling back to global",
                e
            );
            let global = load_articles(until).await?;
            Ok((global, true))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::prelude::*;

    fn article(keys: &Keys, identifier: &str, created_at: u64, published_at: Option<u64>) -> Event {
        let mut tags = vec![nostr::Tag::identifier(identifier)];
        if let Some(ts) = published_at {
            tags.push(nostr::Tag::custom(
                nostr::TagKind::Custom("published_at".into()),
                vec![ts.to_string()],
            ));
        }
        EventBuilder::new(Kind::LongFormTextNote, "content")
            .tags(tags)
            .custom_created_at(Timestamp::from(created_at))
            .sign_with_keys(keys)
            .unwrap()
    }

    #[test]
    fn merge_keeps_newest_version_per_address() {
        let keys = Keys::generate();
        let old = article(&keys, "post", 1_000, Some(900));
        let new = article(&keys, "post", 2_000, Some(900));
        let other = article(&Keys::generate(), "other", 1_500, Some(1_400));
        // Old version arrives after the new one (e.g. from another relay).
        let merged = merge_and_sort_articles(&[new.clone(), other], &[old]);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|e| e.id == new.id));
        assert!(!merged.iter().any(|e| e.created_at.as_secs() == 1_000));
    }

    #[test]
    fn merge_sorts_by_published_at_descending() {
        let a = article(&Keys::generate(), "a", 5_000, Some(1_000));
        let b = article(&Keys::generate(), "b", 6_000, Some(3_000));
        let c = article(&Keys::generate(), "c", 4_000, Some(2_000));
        // Unsorted input, created_at order differs from published_at order.
        let merged = merge_and_sort_articles(&[a.clone(), b.clone(), c.clone()], &[]);
        assert_eq!(
            merged.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![b.id, c.id, a.id]
        );
    }

    #[test]
    fn merge_edit_does_not_reorder() {
        let keys = Keys::generate();
        let original = article(&keys, "post", 1_000, Some(900));
        let edited = article(&keys, "post", 9_999, Some(900));
        let newer = article(&Keys::generate(), "newer", 2_000, Some(5_000));
        let merged = merge_and_sort_articles(&[original], &[edited.clone()]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id, edited.id);
        // The edited article still sorts by its original published_at.
        let merged = merge_and_sort_articles(&[edited], &[newer]);
        assert_eq!(merged[0].created_at.as_secs(), 2_000);
    }

    #[test]
    fn merge_falls_back_to_created_at_without_published_at() {
        let a = article(&Keys::generate(), "a", 1_000, None);
        let b = article(&Keys::generate(), "b", 2_000, None);
        let merged = merge_and_sort_articles(&[a.clone(), b.clone()], &[]);
        assert_eq!(merged[0].id, b.id);
        assert_eq!(merged[1].id, a.id);
    }
}
