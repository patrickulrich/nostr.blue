use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client};
use crate::components::{PollCard, ClientInitializing};
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
pub fn Polls() -> Element {
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
    let mut last_event_id = use_signal(|| None::<nostr_sdk::EventId>);

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
        last_event_id.set(None);
        has_more.set(true);

        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => load_following_polls(None, None).await,
                FeedType::Global => load_global_polls(None, None).await,
            };

            match result {
                Ok(poll_events) => {
                    // Check if feed type changed while we were fetching
                    if *feed_type.read() != current_feed_type {
                        loading.set(false);
                        return;
                    }

                    // Track oldest timestamp and event ID for pagination
                    if let Some(last_event) = poll_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                        last_event_id.set(Some(last_event.id));
                    }

                    // Determine if there are more events to load
                    has_more.set(poll_events.len() >= 50);

                    events.set(poll_events);
                    loading.set(false);
                }
                Err(e) => {
                    // Check if feed type changed while we were fetching
                    if *feed_type.read() != current_feed_type {
                        loading.set(false);
                        return;
                    }

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

        let until = *oldest_timestamp.read();
        let last_id = *last_event_id.read();
        let current_feed_type = *feed_type.read();

        loading.set(true);

        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => load_following_polls(until, last_id).await,
                FeedType::Global => load_global_polls(until, last_id).await,
            };

            match result {
                Ok(new_events) => {
                    // Check if feed type changed while we were fetching
                    if *feed_type.read() != current_feed_type {
                        loading.set(false);
                        return;
                    }

                    // Capture raw count before deduping to determine has_more
                    let raw_count = new_events.len();

                    // First, deduplicate to get unique new events
                    let existing_ids: std::collections::HashSet<_> = {
                        let current = events.read();
                        current.iter().map(|e| e.id).collect()
                    };

                    let unique_new: Vec<_> = new_events.into_iter()
                        .filter(|e| !existing_ids.contains(&e.id))
                        .collect();

                    // If no unique events, bail out early without updating cursors
                    if unique_new.is_empty() {
                        loading.set(false);
                        has_more.set(false);
                        return;
                    }

                    // Track oldest timestamp and event ID from unique new events
                    if let Some(last_event) = unique_new.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                        last_event_id.set(Some(last_event.id));
                    }

                    // Determine if there are more events to load based on raw count (before deduping)
                    has_more.set(raw_count >= 50);

                    // Append only unique new events
                    events.with_mut(|current| {
                        current.extend(unique_new);
                    });
                    loading.set(false);
                }
                Err(e) => {
                    // Check if feed type changed while we were fetching
                    if *feed_type.read() != current_feed_type {
                        loading.set(false);
                        return;
                    }

                    log::error!("Failed to load more polls: {}", e);
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
                            "📊 {feed_type.read().label()}"
                            span {
                                class: "text-sm",
                                "▼"
                            }
                        }

                        // Dropdown menu
                        if *show_dropdown.read() {
                            // Backdrop to close dropdown when clicking outside
                            div {
                                class: "fixed inset-0 z-20",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_dropdown.set(false);
                                }
                            }

                            div {
                                class: "absolute top-full left-0 mt-1 bg-card border border-border rounded-lg shadow-lg overflow-hidden z-30 min-w-[150px]",
                                button {
                                    class: "w-full px-4 py-2 text-left hover:bg-accent transition",
                                    onclick: move |_| {
                                        feed_type.set(FeedType::Following);
                                        show_dropdown.set(false);
                                        refresh_trigger.with_mut(|v| *v += 1);
                                    },
                                    "Following"
                                }
                                button {
                                    class: "w-full px-4 py-2 text-left hover:bg-accent transition",
                                    onclick: move |_| {
                                        feed_type.set(FeedType::Global);
                                        show_dropdown.set(false);
                                        refresh_trigger.with_mut(|v| *v += 1);
                                    },
                                    "Global"
                                }
                            }
                        }
                    }

                    // Action buttons
                    div {
                        class: "flex items-center gap-2",

                        // Refresh button
                        button {
                            class: "px-4 py-2 text-sm rounded-lg hover:bg-accent transition",
                            onclick: move |_| {
                                refresh_trigger.with_mut(|v| *v += 1);
                            },
                            "↻ Refresh"
                        }

                        // Create Poll button
                        Link {
                            to: crate::routes::Route::PollNew {},
                            class: "px-4 py-2 text-sm rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition font-medium",
                            "Create Poll"
                        }
                    }
                }
            }

            // Main content
            div {
                class: "max-w-2xl mx-auto",

                // Show client initializing
                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else if let Some(err) = error.read().as_ref() {
                    // Error state
                    div {
                        class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "⚠️" }
                        h3 { class: "text-xl font-semibold mb-2", "Error" }
                        p { class: "text-muted-foreground", "{err}" }
                        button {
                            class: "mt-4 px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            onclick: move |_| {
                                refresh_trigger.with_mut(|v| *v += 1);
                            },
                            "Try Again"
                        }
                    }
                } else if events.read().is_empty() && !*loading.read() {
                    // Empty state
                    div {
                        class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "📊" }
                        h3 { class: "text-xl font-semibold mb-2", "No polls yet" }
                        p {
                            class: "text-muted-foreground",
                            if *feed_type.read() == FeedType::Following {
                                "Polls from people you follow will appear here"
                            } else {
                                "Polls from everyone will appear here"
                            }
                        }
                        Link {
                            to: crate::routes::Route::PollNew {},
                            class: "inline-block mt-4 px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Create a Poll"
                        }
                    }
                } else {
                    // Polls feed
                    div {
                        class: "divide-y divide-border",
                        for event in events.read().iter() {
                            PollCard {
                                key: "{event.id}",
                                event: event.clone()
                            }
                        }
                    }

                    // Loading indicator or end message
                    div {
                        id: "{sentinel_id}",
                        class: "p-8 flex justify-center",
                        if *loading.read() {
                            div {
                                class: "flex items-center gap-3 text-muted-foreground",
                                span {
                                    class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin"
                                }
                                "Loading more..."
                            }
                        } else if !*has_more.read() {
                            p {
                                class: "text-muted-foreground text-sm",
                                "No more polls to load"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Load polls from followed users
async fn load_following_polls(until: Option<u64>, _last_event_id: Option<nostr_sdk::EventId>) -> Result<Vec<Event>, String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated. Please sign in to view your following feed.")?;

    // Get contacts (following list)
    let contacts = nostr_client::fetch_contacts(pubkey_str).await?;

    // Parse contacts to PublicKey
    let authors: Vec<PublicKey> = contacts
        .iter()
        .filter_map(|c| PublicKey::parse(c).ok())
        .collect();

    if authors.is_empty() {
        return Ok(Vec::new());
    }

    // Create filter for polls
    let mut filter = Filter::new()
        .kind(Kind::Poll)
        .authors(authors)
        .limit(50);

    if let Some(until_ts) = until {
        // Keep boundary inclusive
        filter = filter.until(Timestamp::from(until_ts));
    }

    // Fetch events from database and relays
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await
        .map_err(|e| format!("Failed to fetch polls: {}", e))?;

    // Convert to vector and sort by timestamp (newest first)
    let mut event_vec: Vec<Event> = events.into_iter().collect();
    event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // Return full page - deduplication is handled by the caller
    Ok(event_vec)
}

/// Load polls from everyone (global feed)
async fn load_global_polls(until: Option<u64>, _last_event_id: Option<nostr_sdk::EventId>) -> Result<Vec<Event>, String> {
    // Create filter for polls
    let mut filter = Filter::new()
        .kind(Kind::Poll)
        .limit(50);

    if let Some(until_ts) = until {
        // Keep boundary inclusive
        filter = filter.until(Timestamp::from(until_ts));
    }

    // Fetch events from database and relays
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await
        .map_err(|e| format!("Failed to fetch polls: {}", e))?;

    // Convert to vector and sort by timestamp (newest first)
    let mut event_vec: Vec<Event> = events.into_iter().collect();
    event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // Return full page - deduplication is handled by the caller
    Ok(event_vec)
}
