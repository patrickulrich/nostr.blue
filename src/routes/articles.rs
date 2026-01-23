use dioxus::prelude::*;
use crate::stores::{auth_store, feed_cache, nostr_client};
use crate::stores::feed_cache::FeedCacheKey;
use crate::utils::FeedItem;
use crate::components::{ArticleCard, ArticleCardSkeleton, ClientInitializing};
use crate::hooks::use_infinite_scroll;
use crate::utils::article_meta::get_identifier;
use nostr_sdk::{Event, Filter, Kind, PublicKey, Timestamp};
use std::collections::HashMap;
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
    // State for feed events
    let mut articles = use_signal(Vec::<Event>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);

    // Pagination state for infinite scroll
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    // Request ID for preventing stale results when feed type changes rapidly
    let mut request_id = use_signal(|| 0u32);

    // Track last intentional load trigger to guard against spurious re-triggers
    let mut last_loaded_trigger = use_signal(|| (0u32, FeedType::Following));

    // Load articles on mount and when refresh is triggered or feed type changes
    use_effect(move || {
        let refresh = *refresh_trigger.read();
        let current_feed_type = *feed_type.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            return;
        }

        // Guard: Only reload if intentional change or no data
        let (last_refresh, last_feed) = *last_loaded_trigger.peek();
        let has_data = !articles.peek().is_empty();

        let feed_type_changed = current_feed_type != last_feed;
        let refresh_changed = refresh != last_refresh;

        if has_data && !feed_type_changed && !refresh_changed {
            log::debug!("Skipping articles feed re-load: data already present, no intentional change");
            return;
        }

        // Update last loaded trigger
        last_loaded_trigger.set((refresh, current_feed_type));

        // Generate request ID for stale request prevention
        let current_id = *request_id.peek() + 1;
        request_id.set(current_id);

        // Only show loading if no data exists
        if !has_data {
            loading.set(true);
        }
        error.set(None);
        oldest_timestamp.set(None);
        has_more.set(true);

        spawn(async move {
            // Check if this request is still current
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale articles feed request {}", current_id);
                return;
            }

            // Determine cache key
            let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
            let cache_key = match current_feed_type {
                FeedType::Following => FeedCacheKey::Articles { pubkey: pubkey_str },
                FeedType::Global => FeedCacheKey::ArticlesGlobal,
            };

            // STEP 1: Load from cache instantly
            let cached_items = feed_cache::load_cached_feed(&cache_key, 100)
                .await
                .unwrap_or_default();

            // Check staleness after cache load
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale articles request after cache load");
                return;
            }

            if !cached_items.is_empty() {
                log::info!("Loaded {} articles from cache", cached_items.len());
                let cached_events: Vec<Event> = cached_items.iter().map(|i| i.event().clone()).collect();

                // Seed pagination from cache to enable proper pagination on cached data
                if let Some(oldest) = cached_events.iter().map(|e| e.created_at).min() {
                    oldest_timestamp.set(Some(oldest.as_secs().saturating_sub(1)));
                }

                articles.set(cached_events);
            }

            // STEP 2: Load from network
            let result = match current_feed_type {
                FeedType::Following => load_following_articles(None).await,
                FeedType::Global => load_articles(None).await.map(|e| (e, false)),
            };

            // Check staleness after network load
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale articles request after network load");
                return;
            }

            match result {
                Ok((feed_events, did_fallback)) => {
                    // Recompute cache key if fallback occurred
                    let effective_cache_key = if did_fallback {
                        log::info!("No contacts, switched to Global articles feed");
                        feed_type.set(FeedType::Global);
                        FeedCacheKey::ArticlesGlobal  // Use Global key, not Following
                    } else {
                        cache_key.clone()
                    };

                    // Track oldest timestamp for pagination
                    if let Some(last_event) = feed_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }

                    // STEP 3: Store to cache with effective key
                    let feed_items: Vec<FeedItem> = feed_events.iter()
                        .map(|e| FeedItem::OriginalPost(e.clone()))
                        .collect();
                    spawn(async move {
                        let _ = feed_cache::store_feed_items(&effective_cache_key, &feed_items).await;
                        let _ = feed_cache::run_eviction_if_needed().await;
                    });

                    // Determine if there are more events to load
                    has_more.set(feed_events.len() >= 20);

                    articles.set(feed_events);
                    loading.set(false);
                }
                Err(e) => {
                    // On error, keep cached data visible if we have it
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

    // Load more function for infinite scroll
    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }

        let until = *oldest_timestamp.read();
        let current_feed_type = *feed_type.read();

        loading.set(true);

        spawn(async move {
            // During pagination, handle fallback carefully to avoid mixing feeds
            let result = match current_feed_type {
                FeedType::Following => {
                    match load_following_articles(until).await {
                        Ok((events, did_fallback)) => {
                            // During pagination, discard fallback results to avoid mixing feeds
                            if did_fallback {
                                log::info!("Pagination fallback detected, returning empty to preserve feed type");
                                Ok(Vec::new())
                            } else {
                                Ok(events)
                            }
                        }
                        Err(e) => Err(e),
                    }
                }
                FeedType::Global => load_articles(until).await,
            };

            match result {
                Ok(mut new_articles) => {
                    // Track oldest timestamp from new events
                    if let Some(last_event) = new_articles.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }

                    // Determine if there are more events to load
                    has_more.set(new_articles.len() >= 20);

                    // Append new events to existing events
                    let mut current = articles.read().clone();
                    current.append(&mut new_articles);
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

    // Set up infinite scroll
    let sentinel_id = use_infinite_scroll(
        load_more,
        has_more,
        loading
    );

    let article_list = articles.read();
    let is_loading = *loading.read();
    let error_msg = error.read();

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center justify-between",

                    // Feed type selector (dropdown)
                    div {
                        class: "relative",
                        button {
                            class: "text-xl font-bold flex items-center gap-2 hover:bg-accent px-3 py-1 rounded-lg transition",
                            onclick: move |_| {
                                let current = *show_dropdown.read();
                                show_dropdown.set(!current);
                            },
                            "📚 {feed_type.read().label()}"
                            span {
                                class: "text-sm",
                                if *show_dropdown.read() { "▲" } else { "▼" }
                            }
                        }

                        // Dropdown menu
                        if *show_dropdown.read() {
                            div {
                                class: "absolute top-full left-0 mt-2 bg-card border border-border rounded-lg shadow-lg min-w-[200px] overflow-hidden z-30",

                                button {
                                    class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                    onclick: move |_| {
                                        feed_type.set(FeedType::Following);
                                        show_dropdown.set(false);
                                    },
                                    div {
                                        div {
                                            class: "font-medium",
                                            "Following"
                                        }
                                        div {
                                            class: "text-xs text-muted-foreground",
                                            "Articles from people you follow"
                                        }
                                    }
                                    if *feed_type.read() == FeedType::Following {
                                        span { "✓" }
                                    }
                                }

                                div {
                                    class: "border-t border-border"
                                }

                                button {
                                    class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                    onclick: move |_| {
                                        feed_type.set(FeedType::Global);
                                        show_dropdown.set(false);
                                    },
                                    div {
                                        div {
                                            class: "font-medium",
                                            "Global"
                                        }
                                        div {
                                            class: "text-xs text-muted-foreground",
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

            // Error message
            if let Some(err) = error_msg.as_ref() {
                div {
                    class: "p-4 bg-destructive/10 border border-destructive text-destructive",
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

            // Articles grid
            div {
                class: "p-4",

                // Initial loading state
                if !*nostr_client::CLIENT_INITIALIZED.read() || (is_loading && article_list.is_empty()) {
                    // Show client initializing animation during:
                    // 1. Client initialization
                    // 2. Initial articles load (loading + no articles, regardless of error state)
                    ClientInitializing {}
                } else if article_list.is_empty() {
                    // Empty state
                    div {
                        class: "text-center py-12",
                        div {
                            class: "text-6xl mb-4",
                            "📚"
                        }
                        h3 {
                            class: "text-xl font-semibold mb-2",
                            "No Articles Found"
                        }
                        p {
                            class: "text-muted-foreground text-sm mb-4",
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
                    // Article grid
                    div {
                        class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                        for article in article_list.iter() {
                            ArticleCard {
                                key: "{article.id}",
                                event: article.clone(),
                            }
                        }
                    }

                    // Infinite scroll sentinel
                    if *has_more.read() {
                        div {
                            id: "{sentinel_id}",
                            class: "h-20 flex items-center justify-center",
                            if is_loading {
                                div {
                                    class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 w-full",
                                    for _ in 0..3 {
                                        ArticleCardSkeleton {}
                                    }
                                }
                            }
                        }
                    } else {
                        // End of feed indicator
                        div {
                            class: "text-center py-8 text-muted-foreground text-sm",
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
    // Fetch articles from the client
    let raw_articles = nostr_client::fetch_articles(20, until).await?;

    // Deduplicate by address (kind:pubkey:identifier)
    // Keep only the most recent version of each article
    let mut address_map: HashMap<String, Event> = HashMap::new();

    for article in raw_articles {
        // Only include articles with valid identifiers
        if let Some(identifier) = get_identifier(&article) {
            let address = format!("{}:{}:{}",
                article.kind.as_u16(),
                article.pubkey.to_hex(),
                identifier
            );

            // Keep the newest version (replace if newer)
            address_map.entry(address)
                .and_modify(|existing| {
                    if article.created_at > existing.created_at {
                        *existing = article.clone();
                    }
                })
                .or_insert(article);
        }
    }

    // Convert back to vec and sort by created_at descending
    let mut deduplicated: Vec<Event> = address_map.into_values().collect();
    deduplicated.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(deduplicated)
}

/// Load articles from followed users with deduplication by address
/// Returns (articles, did_fallback) where did_fallback indicates if we fell back to global.
async fn load_following_articles(until: Option<u64>) -> Result<(Vec<Event>, bool), String> {
    // Get current user's pubkey
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    log::info!("Loading following articles for {} (until: {:?})", pubkey_str, until);

    // Fetch the user's contact list (people they follow)
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            let global = load_articles(until).await?;
            return Ok((global, true));
        }
    };

    // If user doesn't follow anyone, show global feed
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global articles");
        let global = load_articles(until).await?;
        return Ok((global, true));
    }

    log::info!("User follows {} accounts", contacts.len());

    // Parse contact pubkeys
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

    // Create filter for long-form articles from followed users
    // Use higher limit (100) because deduplication will reduce the count
    // Articles are less common than text notes, so we need a higher limit
    let mut filter = Filter::new()
        .kind(Kind::LongFormTextNote)
        .authors(authors)
        .limit(100);

    // Add until timestamp if provided for pagination
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    log::info!("Fetching articles from {} followed accounts", filter.authors.as_ref().map(|a| a.len()).unwrap_or(0));

    // Fetch articles using aggregated pattern (database first, then relays)
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(raw_articles) => {
            log::info!("Loaded {} raw articles from following feed", raw_articles.len());

            // Convert to Vec for deduplication
            let raw_articles_vec: Vec<Event> = raw_articles.into_iter().collect();
            log::info!("Processing {} articles for deduplication", raw_articles_vec.len());

            // Deduplicate by address (kind:pubkey:identifier)
            // Keep only the most recent version of each article
            let mut address_map: HashMap<String, Event> = HashMap::new();
            let mut articles_without_identifier = 0;

            for article in raw_articles_vec {
                // Only include articles with valid identifiers
                if let Some(identifier) = get_identifier(&article) {
                    let address = format!("{}:{}:{}",
                        article.kind.as_u16(),
                        article.pubkey.to_hex(),
                        identifier
                    );

                    // Keep the newest version (replace if newer)
                    address_map.entry(address)
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
                log::warn!("Filtered out {} articles without identifiers", articles_without_identifier);
            }

            // Convert back to vec and sort by created_at descending
            let mut deduplicated: Vec<Event> = address_map.into_values().collect();
            deduplicated.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            log::info!("After deduplication: {} unique articles", deduplicated.len());

            // If no articles found, return empty (valid result - user's contacts just haven't posted articles)
            // Don't fall back to global here - empty following is different from no contacts
            if deduplicated.is_empty() {
                log::info!("No articles from followed users");
                return Ok((Vec::new(), false));
            }

            Ok((deduplicated, false))
        }
        Err(e) => {
            log::error!("Failed to fetch following articles: {}, falling back to global", e);
            let global = load_articles(until).await?;
            Ok((global, true))
        }
    }
}
