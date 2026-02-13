use crate::components::{ClientInitializing, PhotoCard};
use crate::hooks::use_infinite_scroll;
use crate::stores::feed_cache::FeedCacheKey;
use crate::stores::{auth_store, feed_cache, nostr_client};
use crate::utils::FeedItem;
use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, PublicKey, Timestamp};
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
pub fn Photos() -> Element {
    let mut events = use_signal(Vec::<Event>::new);
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
        let has_data = !events.peek().is_empty();
        let feed_type_changed = current_feed_type != last_feed;
        let refresh_changed = refresh != last_refresh;
        if has_data && !feed_type_changed && !refresh_changed {
            log::debug!(
                "Skipping photos feed re-load: data already present, no intentional change"
            );
            return;
        }
        last_loaded_trigger.set((refresh, current_feed_type));
        let current_id = *request_id.peek() + 1;
        request_id.set(current_id);
        loading.set(true);
        if feed_type_changed {
            events.set(Vec::new());
        }
        error.set(None);
        oldest_timestamp.set(None);
        has_more.set(true);
        spawn(async move {
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale photos feed request {}", current_id);
                return;
            }
            let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
            let cache_key = match current_feed_type {
                FeedType::Following => {
                    FeedCacheKey::Photos {
                        pubkey: pubkey_str,
                    }
                }
                FeedType::Global => FeedCacheKey::PhotosGlobal,
            };
            let cached_items = feed_cache::load_cached_feed(&cache_key, 100)
                .await
                .unwrap_or_default();
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale photos request after cache load");
                return;
            }
            if !cached_items.is_empty() {
                log::info!("Loaded {} photos from cache", cached_items.len());
                let cached_events: Vec<Event> = cached_items
                    .iter()
                    .map(|i| i.event().clone())
                    .collect();
                if let Some(oldest_event) = cached_events.last() {
                    oldest_timestamp.set(Some(oldest_event.created_at.as_secs()));
                }
                events.set(cached_events);
            }
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale photos request before network");
                return;
            }
            let result = match current_feed_type {
                FeedType::Following => load_following_photos(None).await,
                FeedType::Global => load_global_photos(None).await.map(|e| (e, false)),
            };
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale photos request after network");
                return;
            }
            match result {
                Ok((photo_events, did_fallback)) => {
                    let effective_cache_key = if did_fallback {
                        log::info!("No contacts, switched to Global photos feed");
                        feed_type.set(FeedType::Global);
                        FeedCacheKey::PhotosGlobal
                    } else {
                        cache_key.clone()
                    };
                    let feed_items: Vec<FeedItem> = photo_events
                        .iter()
                        .map(|e| FeedItem::OriginalPost(e.clone()))
                        .collect();
                    let cache_key_for_store = effective_cache_key;
                    spawn(async move {
                        let _ = feed_cache::store_feed_items(
                                &cache_key_for_store,
                                &feed_items,
                            )
                            .await;
                        let _ = feed_cache::run_eviction_if_needed().await;
                    });
                    if let Some(last_event) = photo_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }
                    has_more.set(true);
                    events.set(photo_events);
                    loading.set(false);
                }
                Err(e) => {
                    if cached_items.is_empty() {
                        error.set(Some(e));
                    } else {
                        log::warn!("Network error but showing cached photos: {}", e);
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
        let until = match *oldest_timestamp.read() {
            Some(ts) => ts,
            None => return,
        };
        let current_feed_type = *feed_type.read();
        loading.set(true);
        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => {
                    match load_following_photos(Some(until)).await {
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
                    }
                }
                FeedType::Global => load_global_photos(Some(until)).await,
            };
            match result {
                Ok(new_events) => {
                    if new_events.is_empty() {
                        has_more.set(false);
                        loading.set(false);
                        return;
                    }
                    let current = events.read().clone();
                    let existing_ids: std::collections::HashSet<_> = current
                        .iter()
                        .map(|e| e.id)
                        .collect();
                    let unique_events: Vec<_> = new_events
                        .iter()
                        .filter(|e| !existing_ids.contains(&e.id))
                        .cloned()
                        .collect();
                    if let Some(last_event) = new_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }
                    if !unique_events.is_empty() {
                        let mut updated = current;
                        updated.extend(unique_events);
                        events.set(updated);
                    }
                    loading.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load more photos: {}", e);
                    loading.set(false);
                }
            }
        });
    };
    let sentinel_id = use_infinite_scroll(load_more, has_more, loading);
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
                            "📷 {feed_type.read().label()}"
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
                                            "Photos from people you follow"
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
                                            "Photos from everyone"
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
                        class: "p-2 hover:bg-accent rounded-full transition disabled:opacity-50",
                        disabled: *loading.read(),
                        onclick: move |_| {
                            let current = *refresh_trigger.read();
                            refresh_trigger.set(current + 1);
                        },
                        title: "Refresh feed",
                        if *loading.read() {
                            span { class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                        } else {
                            "🔄"
                        }
                    }
                }
            }
            div { class: "p-4",
                if !*nostr_client::CLIENT_INITIALIZED.read()
                    || (*loading.read() && events.read().is_empty())
                {
                    ClientInitializing {}
                } else if let Some(err) = error.read().as_ref() {
                    if !*loading.read() && events.read().is_empty() {
                        div { class: "p-6 text-center",
                            div { class: "max-w-md mx-auto",
                                div { class: "text-4xl mb-2", "⚠️" }
                                p { class: "text-red-600 dark:text-red-400",
                                    "Error loading photos: {err}"
                                }
                            }
                        }
                    }
                } else if events.read().is_empty() {
                    div { class: "p-6 text-center text-gray-500 dark:text-gray-400",
                        div { class: "max-w-md mx-auto space-y-4",
                            div { class: "text-6xl mb-2", "📷" }
                            h3 { class: "text-xl font-semibold text-gray-700 dark:text-gray-300",
                                "No photos yet"
                            }
                            p { class: "text-sm",
                                "Photo posts from the network will appear here. NIP-68 picture events are displayed in an Instagram-style feed."
                            }
                        }
                    }
                } else {
                    div { class: "max-w-[600px] mx-auto",
                        for event in events.read().iter() {
                            PhotoCard { key: "{event.id}", event: event.clone() }
                        }
                    }
                    if *has_more.read() {
                        div {
                            id: "{sentinel_id}",
                            class: "p-8 flex justify-center",
                            if *loading.read() {
                                span { class: "flex items-center gap-2 text-muted-foreground",
                                    span { class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                    "Loading more photos..."
                                }
                            }
                        }
                    } else if !events.read().is_empty() {
                        div { class: "p-8 text-center text-muted-foreground", "You've reached the end" }
                    }
                }
            }
        }
    }
}
/// Load following photos feed (NIP-68 kind 20 events from followed users)
/// Returns (events, did_fallback) where did_fallback indicates if we fell back to global.
async fn load_following_photos(
    until: Option<u64>,
) -> Result<(Vec<Event>, bool), String> {
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    log::info!("Loading following photos feed for {} (until: {:?})", pubkey_str, until);
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            let global = load_global_photos(until).await?;
            return Ok((global, true));
        }
    };
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global photos");
        let global = load_global_photos(until).await?;
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
        let global = load_global_photos(until).await?;
        return Ok((global, true));
    }
    let mut filter = Filter::new().kind(Kind::Custom(20)).authors(authors).limit(50);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts.saturating_sub(1)));
    }
    log::info!(
        "Fetching photo events from {} followed accounts", filter.authors.as_ref().map(|
        a | a.len()).unwrap_or(0)
    );
    let mut events = Vec::new();
    let stream_result = nostr_client::stream_events_immediate(
        filter,
        Duration::from_secs(10),
        |event| {
            events.push(event);
        },
    )
    .await;
    match stream_result {
        Ok(_) => {
            log::info!("Loaded {} photo events from following", events.len());
            events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            if events.is_empty() {
                log::info!("No photos from followed users");
                return Ok((Vec::new(), false));
            }
            Ok((events, false))
        }
        Err(e) => {
            log::error!(
                "Failed to fetch following photos: {}, falling back to global", e
            );
            let global = load_global_photos(until).await?;
            Ok((global, true))
        }
    }
}
async fn load_global_photos(until: Option<u64>) -> Result<Vec<Event>, String> {
    log::info!("Loading global photos feed (until: {:?})...", until);
    let mut filter = Filter::new().kind(Kind::Custom(20)).limit(50);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts.saturating_sub(1)));
    } else {
        let since = Timestamp::now() - Duration::from_secs(86400 * 7);
        filter = filter.since(since);
    }
    log::info!("Fetching global photo events with filter: {:?}", filter);
    let mut events = Vec::new();
    nostr_client::stream_events_immediate(
        filter,
        Duration::from_secs(10),
        |event| {
            events.push(event);
        },
    )
    .await
    .map_err(|e| format!("Failed to load photos: {}", e))?;
    log::info!("Loaded {} global photo events", events.len());
    events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(events)
}
