use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::components::{NoteCard, NoteCardSkeleton};
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

        loading.set(true);
        error.set(None);
        oldest_timestamp.set(None);

        spawn(async move {
            match load_hashtag_feed(&hashtag, None).await {
                Ok(feed_events) => {
                    if let Some(last_event) = feed_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_u64()));
                    }
                    has_more.set(feed_events.len() >= 50);
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

        let until = *oldest_timestamp.read();
        let hashtag = tag_for_load.clone();
        loading.set(true);

        spawn(async move {
            match load_hashtag_feed(&hashtag, until).await {
                Ok(mut new_events) => {
                    if let Some(last_event) = new_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_u64()));
                    }
                    has_more.set(new_events.len() >= 50);

                    // Append new events
                    let mut current = events.read().clone();
                    current.append(&mut new_events);
                    events.set(current);
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
            if *loading.read() && events.read().is_empty() {
                div {
                    class: "divide-y divide-border",
                    for _ in 0..5 {
                        NoteCardSkeleton {}
                    }
                }
            }

            // Events feed
            if !events.read().is_empty() {
                div {
                    class: "divide-y divide-border",
                    for event in events.read().iter() {
                        NoteCard {
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
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    log::info!("Loading hashtag feed for #{} (until: {:?})...", tag, until);

    // Normalize hashtag to lowercase
    let normalized_tag = tag.to_lowercase();

    // Create filter for text notes with this hashtag
    let mut filter = Filter::new()
        .kind(Kind::TextNote)
        .hashtag(normalized_tag)
        .limit(50);

    // Add until timestamp if provided for pagination
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    } else {
        // For initial load, get posts from last 7 days
        let since = Timestamp::now() - Duration::from_secs(7 * 86400);
        filter = filter.since(since);
    }

    log::info!("Fetching events with filter: {:?}", filter);

    // Fetch events from relays
    match client.fetch_events(filter, Duration::from_secs(10)).await {
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
