use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client};
use crate::components::{PhotoCard, ClientInitializing};
use crate::hooks::use_infinite_scroll;
use nostr_sdk::{Event, Filter, Kind, Timestamp, PublicKey};
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
    // State for feed events
    let mut events = use_signal(|| Vec::<Event>::new());
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);

    // Pagination state for infinite scroll
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    // Load feed on mount and when refresh is triggered or feed type changes
    use_effect(move || {
        // Watch refresh trigger and feed type
        let _ = refresh_trigger.read();
        let current_feed_type = *feed_type.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            return;
        }

        loading.set(true);
        error.set(None);
        oldest_timestamp.set(None);
        has_more.set(true);

        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => load_following_photos(None).await,
                FeedType::Global => load_global_photos(None).await,
            };

            match result {
                Ok(photo_events) => {
                    // Track oldest timestamp for pagination
                    if let Some(last_event) = photo_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }

                    // Always enable has_more on initial/refresh load
                    // Only disable when load_more gets 0 results from relay
                    has_more.set(true);

                    events.set(photo_events);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
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

        // Need a valid oldest_timestamp for pagination
        // If None, initial load hasn't completed yet
        let until = match *oldest_timestamp.read() {
            Some(ts) => ts,
            None => return,
        };
        let current_feed_type = *feed_type.read();

        loading.set(true);

        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => load_following_photos(Some(until)).await,
                FeedType::Global => load_global_photos(Some(until)).await,
            };

            match result {
                Ok(new_events) => {
                    // Stop pagination only when relay returns 0 events
                    if new_events.is_empty() {
                        has_more.set(false);
                        loading.set(false);
                        return;
                    }

                    // Deduplicate: build set of existing event IDs
                    let current = events.read().clone();
                    let existing_ids: std::collections::HashSet<_> = current.iter()
                        .map(|e| e.id)
                        .collect();

                    // Filter out duplicates
                    let unique_events: Vec<_> = new_events.iter()
                        .filter(|e| !existing_ids.contains(&e.id))
                        .cloned()
                        .collect();

                    // Update oldest timestamp from all new events (not just unique)
                    // This ensures we advance pagination even if all were duplicates
                    if let Some(last_event) = new_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }

                    // Append unique events
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

    // Set up infinite scroll
    let sentinel_id = use_infinite_scroll(
        load_more,
        has_more,
        loading
    );

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
                            "📷 {feed_type.read().label()}"
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
                                            "Photos from people you follow"
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

                    // Refresh button
                    button {
                        class: "p-2 hover:bg-accent rounded-full transition disabled:opacity-50",
                        disabled: *loading.read(),
                        onclick: move |_| {
                            let current = *refresh_trigger.read();
                            refresh_trigger.set(current + 1);
                        },
                        title: "Refresh feed",
                        if *loading.read() {
                            span {
                                class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin"
                            }
                        } else {
                            "🔄"
                        }
                    }
                }
            }

            // Feed Content
            div {
                class: "p-4",

                if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && events.read().is_empty()) {
                    // Show client initializing animation during:
                    // 1. Client initialization
                    // 2. Initial photo load (loading + no photos, regardless of error state)
                    ClientInitializing {}
                } else if let Some(err) = error.read().as_ref() {
                    // Only show error if we're not loading and have no events
                    if !*loading.read() && events.read().is_empty() {
                        div {
                            class: "p-6 text-center",
                            div {
                                class: "max-w-md mx-auto",
                                div {
                                    class: "text-4xl mb-2",
                                    "⚠️"
                                }
                                p {
                                    class: "text-red-600 dark:text-red-400",
                                    "Error loading photos: {err}"
                                }
                            }
                        }
                    }
                } else if events.read().is_empty() {
                    // Empty state
                    div {
                        class: "p-6 text-center text-gray-500 dark:text-gray-400",
                        div {
                            class: "max-w-md mx-auto space-y-4",
                            div {
                                class: "text-6xl mb-2",
                                "📷"
                            }
                            h3 {
                                class: "text-xl font-semibold text-gray-700 dark:text-gray-300",
                                "No photos yet"
                            }
                            p {
                                class: "text-sm",
                                "Photo posts from the network will appear here. NIP-68 picture events are displayed in an Instagram-style feed."
                            }
                        }
                    }
                } else {
                    // Instagram-style single-column feed
                    div {
                        class: "max-w-[600px] mx-auto",
                        for event in events.read().iter() {
                            PhotoCard {
                                key: "{event.id}",
                                event: event.clone()
                            }
                        }
                    }

                    // Infinite scroll sentinel / loading indicator
                    if *has_more.read() {
                        div {
                            id: "{sentinel_id}",
                            class: "p-8 flex justify-center",
                            if *loading.read() {
                                span {
                                    class: "flex items-center gap-2 text-muted-foreground",
                                    span {
                                        class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin"
                                    }
                                    "Loading more photos..."
                                }
                            }
                        }
                    } else if !events.read().is_empty() {
                        div {
                            class: "p-8 text-center text-muted-foreground",
                            "You've reached the end"
                        }
                    }
                }
            }
        }
    }
}

// Helper function to load following photos feed (NIP-68 kind 20 events from followed users)
async fn load_following_photos(until: Option<u64>) -> Result<Vec<Event>, String> {
    // Get current user's pubkey
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    log::info!("Loading following photos feed for {} (until: {:?})", pubkey_str, until);

    // Fetch the user's contact list (people they follow)
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            return load_global_photos(until).await;
        }
    };

    // If user doesn't follow anyone, show global feed
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global photos");
        return load_global_photos(until).await;
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
        return load_global_photos(until).await;
    }

    // Create filter for NIP-68 picture events from followed users
    let mut filter = Filter::new()
        .kind(Kind::Custom(20))
        .authors(authors)
        .limit(50);

    // Add until for pagination
    // Subtract 1 to exclude events at exactly this timestamp (avoid duplicates)
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts.saturating_sub(1)));
    }

    log::info!("Fetching photo events from {} followed accounts", filter.authors.as_ref().map(|a| a.len()).unwrap_or(0));

    // Fetch events using aggregated pattern (database-first)
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Loaded {} photo events from following", events.len());

            // Convert to Vec and sort by created_at (newest first)
            let mut event_vec: Vec<Event> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            // If no events found, fall back to global feed
            if event_vec.is_empty() {
                log::info!("No photos from followed users, showing global feed");
                return load_global_photos(until).await;
            }

            Ok(event_vec)
        }
        Err(e) => {
            log::error!("Failed to fetch following photos: {}, falling back to global", e);
            load_global_photos(until).await
        }
    }
}

// Helper function to load global photos feed (NIP-68 kind 20 events from everyone)
async fn load_global_photos(until: Option<u64>) -> Result<Vec<Event>, String> {
    log::info!("Loading global photos feed (until: {:?})...", until);

    // Create filter for NIP-68 picture events (kind 20)
    let mut filter = Filter::new()
        .kind(Kind::Custom(20))
        .limit(50);

    // Add until for pagination, or since for initial load
    // Subtract 1 to exclude events at exactly this timestamp (avoid duplicates)
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts.saturating_sub(1)));
    } else {
        let since = Timestamp::now() - Duration::from_secs(86400 * 7); // 7 days ago
        filter = filter.since(since);
    }

    log::info!("Fetching global photo events with filter: {:?}", filter);

    // Fetch events using aggregated pattern (database-first)
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Loaded {} global photo events", events.len());

            // Convert to Vec and sort by created_at (newest first)
            let mut event_vec: Vec<Event> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            Ok(event_vec)
        }
        Err(e) => {
            log::error!("Failed to fetch global photo events: {}", e);
            Err(format!("Failed to load photos: {}", e))
        }
    }
}
