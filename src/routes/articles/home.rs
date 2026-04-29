use crate::components::{ArticleCard, ArticleCardSkeleton, ClientInitializing};
use crate::hooks::use_infinite_scroll;
use crate::stores::feed_cache::FeedCacheKey;
use crate::stores::{auth_store, feed_cache, nostr_client};
use crate::utils::article_meta::get_identifier;
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
    use_effect(move || {
        let refresh = *refresh_trigger.read();
        let current_feed_type = *feed_type.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
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
                if let Some(oldest) = cached_events.iter().map(|e| e.created_at).min() {
                    oldest_timestamp.set(Some(oldest.as_secs().saturating_sub(1)));
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
                    if let Some(last_event) = feed_events.last() {
                        oldest_timestamp.set(Some(
                            last_event.created_at.as_secs().saturating_sub(1),
                        ));
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
                    has_more.set(feed_events.len() >= 20);
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
        loading.set(true);
        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => match load_following_articles(until).await {
                    Ok((events, did_fallback)) => {
                        if did_fallback {
                            log::info!(
                                    "Pagination fallback detected, returning empty to preserve feed type"
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
            match result {
                Ok(new_articles) => {
                    if let Some(last_event) = new_articles.last() {
                        oldest_timestamp.set(Some(
                            last_event.created_at.as_secs().saturating_sub(1),
                        ));
                    }
                    has_more.set(new_articles.len() >= 20);
                    let mut current = articles.read().clone();
                    let existing_ids: HashSet<_> =
                        current.iter().map(|e| e.id).collect();
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
    let sentinel_id = use_infinite_scroll(load_more, has_more, loading);
    let article_list = articles.read();
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
/// Load articles with deduplication by address (kind:pubkey:identifier)
async fn load_articles(until: Option<u64>) -> Result<Vec<Event>, String> {
    let raw_articles = nostr_client::fetch_articles(20, until).await?;
    let mut address_map: HashMap<String, Event> = HashMap::new();
    for article in raw_articles {
        if let Some(identifier) = get_identifier(&article) {
            let address = format!(
                "{}:{}:{}",
                article.kind.as_u16(),
                article.pubkey.to_hex(),
                identifier,
            );
            address_map
                .entry(address)
                .and_modify(|existing| {
                    if article.created_at > existing.created_at {
                        *existing = article.clone();
                    }
                })
                .or_insert(article);
        }
    }
    let mut deduplicated: Vec<Event> = address_map.into_values().collect();
    deduplicated.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(deduplicated)
}
/// Load articles from followed users with deduplication by address
/// Returns (articles, did_fallback) where did_fallback indicates if we fell back to global.
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
    let mut authors = Vec::new();
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            authors.push(pk);
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
        .limit(100);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    log::info!(
        "Fetching articles from {} followed accounts",
        filter.authors.as_ref().map(|a| a.len()).unwrap_or(0)
    );
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(raw_articles) => {
            log::info!(
                "Loaded {} raw articles from following feed",
                raw_articles.len()
            );
            let raw_articles_vec: Vec<Event> = raw_articles.into_iter().collect();
            log::info!(
                "Processing {} articles for deduplication",
                raw_articles_vec.len()
            );
            let mut address_map: HashMap<String, Event> = HashMap::new();
            let mut articles_without_identifier = 0;
            for article in raw_articles_vec {
                if let Some(identifier) = get_identifier(&article) {
                    let address = format!(
                        "{}:{}:{}",
                        article.kind.as_u16(),
                        article.pubkey.to_hex(),
                        identifier,
                    );
                    address_map
                        .entry(address)
                        .and_modify(|existing| {
                            if article.created_at > existing.created_at {
                                *existing = article.clone();
                            }
                        })
                        .or_insert(article);
                } else {
                    articles_without_identifier += 1;
                }
            }
            if articles_without_identifier > 0 {
                log::warn!(
                    "Filtered out {} articles without identifiers",
                    articles_without_identifier
                );
            }
            let mut deduplicated: Vec<Event> = address_map.into_values().collect();
            deduplicated.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            log::info!(
                "After deduplication: {} unique articles",
                deduplicated.len()
            );
            if deduplicated.is_empty() {
                log::info!("No articles from followed users");
                return Ok((Vec::new(), false));
            }
            Ok((deduplicated, false))
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
