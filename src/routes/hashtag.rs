use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::components::{NoteCard, ClientInitializing};
use crate::hooks::use_infinite_scroll;
use nostr_sdk::{Event, Filter, Kind, Timestamp};
use std::time::Duration;

#[component]
pub fn Hashtag(tag: String) -> Element {
    // State for feed events
    let mut events = use_signal(|| Vec::<Event>::new());
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    let tag_clone = tag.clone();
    let tag_for_load = tag.clone();

    // Load initial feed
    use_effect(move || {
        let _ = refresh_trigger.read();
        let hashtag = tag_clone.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            return;
        }

        // Reset all state for fresh load/refresh
        loading.set(true);
        error.set(None);
        oldest_timestamp.set(None);
        has_more.set(true); // Reset pagination state on refresh

        spawn(async move {
            match load_hashtag_feed(&hashtag, None).await {
                Ok(feed_events) => {
                    if let Some(last_event) = feed_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }
                    // Always enable has_more on initial/refresh load
                    // Only disable when load_more gets 0 results from relay
                    has_more.set(true);
                    events.set(feed_events);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Load more function
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
        let hashtag = tag_for_load.clone();
        loading.set(true);

        spawn(async move {
            match load_hashtag_feed(&hashtag, Some(until)).await {
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
                    log::error!("Failed to load more events: {}", e);
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
                    div {
                        class: "flex items-center gap-2",
                        span {
                            class: "text-2xl",
                            "#"
                        }
                        h2 {
                            class: "text-xl font-bold",
                            "{tag}"
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
                        if *loading.read() && events.read().is_empty() {
                            span {
                                class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin"
                            }
                        } else {
                            "🔄"
                        }
                    }
                }
                div {
                    class: "px-4 pb-3",
                    p {
                        class: "text-sm text-muted-foreground",
                        if !events.read().is_empty() {
                            "{events.read().len()} posts"
                        } else if *loading.read() {
                            "Loading posts..."
                        } else {
                            "Posts tagged with #{tag}"
                        }
                    }
                }
            }

            // Error state
            if let Some(err) = error.read().as_ref() {
                div {
                    class: "p-4",
                    div {
                        class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                        "❌ {err}"
                    }
                }
            }

            // Loading state (initial)
            if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && events.read().is_empty()) {
                // Show client initializing animation during:
                // 1. Client initialization
                // 2. Initial feed load (loading + no events, regardless of error state)
                ClientInitializing {}
            }

            // Events feed
            if !events.read().is_empty() {
                div {
                    class: "divide-y divide-border",
                    for event in events.read().iter() {
                        NoteCard {
                            event: event.clone(),
                            collapsible: true
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
                                "Loading more..."
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

            // Empty state (no error, not loading, no events)
            if !*loading.read() && events.read().is_empty() && error.read().is_none() {
                div {
                    class: "text-center py-12",
                    div {
                        class: "text-6xl mb-4",
                        "#️⃣"
                    }
                    h3 {
                        class: "text-xl font-semibold mb-2",
                        "No posts found"
                    }
                    p {
                        class: "text-muted-foreground",
                        "Be the first to post with #{tag}"
                    }
                }
            }
        }
    }
}

// Helper function to load hashtag feed
async fn load_hashtag_feed(tag: &str, until: Option<u64>) -> Result<Vec<Event>, String> {
    log::info!("Loading hashtag feed for #{} (until: {:?})...", tag, until);

    // Normalize hashtag to lowercase
    let normalized_tag = tag.to_lowercase();

    // Create filter for text notes with this hashtag
    let mut filter = Filter::new()
        .kind(Kind::TextNote)
        .hashtag(normalized_tag)
        .limit(50);

    // Add until timestamp if provided for pagination
    // Subtract 1 to exclude events at exactly this timestamp (avoid duplicates)
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts.saturating_sub(1)));
    }
    // Note: No since filter on initial load to get all historical posts

    log::info!("Fetching events with filter: {:?}", filter);

    // Fetch events from relays
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Loaded {} events for #{}", events.len(), tag);

            // Convert to Vec and sort by created_at (newest first)
            let mut event_vec: Vec<Event> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            Ok(event_vec)
        }
        Err(e) => {
            log::error!("Failed to fetch hashtag events: {}", e);
            Err(format!("Failed to fetch posts: {}", e))
        }
    }
}
