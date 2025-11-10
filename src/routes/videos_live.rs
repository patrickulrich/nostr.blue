use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client};
use crate::components::{ClientInitializing, MiniLiveStreamCard};
use crate::routes::Route;
use nostr_sdk::{Event, Filter, Kind, Timestamp, PublicKey};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Debug)]
enum StatusFilter {
    Live,
    Upcoming,
    All,
}

#[component]
pub fn VideosLive() -> Element {
    // Following streams state
    let mut following_streams = use_signal(|| Vec::<Event>::new());
    let mut loading_following = use_signal(|| false);
    let mut has_more_following = use_signal(|| true);
    let mut oldest_timestamp_following = use_signal(|| None::<u64>);
    let mut error_following = use_signal(|| None::<String>);

    // Global streams state
    let mut global_streams = use_signal(|| Vec::<Event>::new());
    let mut loading_global = use_signal(|| false);
    let mut has_more_global = use_signal(|| true);
    let mut oldest_timestamp_global = use_signal(|| None::<u64>);
    let mut error_global = use_signal(|| None::<String>);

    let mut status_filter = use_signal(|| StatusFilter::Live);
    let mut refresh_trigger = use_signal(|| 0);

    // Load following streams
    use_effect(use_reactive((&*refresh_trigger.read(), &*status_filter.read()), move |(_, current_status)| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        loading_following.set(true);
        error_following.set(None);
        oldest_timestamp_following.set(None);
        has_more_following.set(true);

        spawn(async move {
            match load_following_streams(None, current_status).await {
                Ok(events) => {
                    if let Some(last_event) = events.last() {
                        oldest_timestamp_following.set(Some(last_event.created_at.as_secs()));
                    }

                    has_more_following.set(events.len() >= 50);
                    following_streams.set(events);
                    loading_following.set(false);
                }
                Err(e) => {
                    error_following.set(Some(e));
                    loading_following.set(false);
                }
            }
        });
    }));

    // Load global streams
    use_effect(use_reactive((&*refresh_trigger.read(), &*status_filter.read()), move |(_, current_status)| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        loading_global.set(true);
        error_global.set(None);
        oldest_timestamp_global.set(None);
        has_more_global.set(true);

        spawn(async move {
            match load_global_streams(None, current_status).await {
                Ok(events) => {
                    if let Some(last_event) = events.last() {
                        oldest_timestamp_global.set(Some(last_event.created_at.as_secs()));
                    }

                    has_more_global.set(events.len() >= 50);
                    global_streams.set(events);
                    loading_global.set(false);
                }
                Err(e) => {
                    error_global.set(Some(e));
                    loading_global.set(false);
                }
            }
        });
    }));

    // Load more following streams
    let mut load_more_following = move || {
        if *loading_following.read() || !*has_more_following.read() {
            return;
        }

        let until = *oldest_timestamp_following.read();
        let current_status = *status_filter.read();

        loading_following.set(true);

        spawn(async move {
            match load_following_streams(until, current_status).await {
                Ok(new_events) => {
                    let existing_ids: std::collections::HashSet<_> = {
                        let current = following_streams.read();
                        current.iter().map(|e| e.id).collect()
                    };

                    let unique_events: Vec<_> = new_events.into_iter()
                        .filter(|e| !existing_ids.contains(&e.id))
                        .collect();

                    if unique_events.is_empty() {
                        has_more_following.set(false);
                        loading_following.set(false);
                        log::info!("No new unique following streams found");
                    } else {
                        if let Some(last_event) = unique_events.last() {
                            oldest_timestamp_following.set(Some(last_event.created_at.as_secs()));
                        }

                        has_more_following.set(unique_events.len() >= 50);

                        let mut current = following_streams.read().clone();
                        current.extend(unique_events);
                        following_streams.set(current);
                        loading_following.set(false);
                    }
                }
                Err(e) => {
                    log::error!("Failed to load more following streams: {}", e);
                    loading_following.set(false);
                }
            }
        });
    };

    // Load more global streams
    let mut load_more_global = move || {
        if *loading_global.read() || !*has_more_global.read() {
            return;
        }

        let until = *oldest_timestamp_global.read();
        let current_status = *status_filter.read();

        loading_global.set(true);

        spawn(async move {
            match load_global_streams(until, current_status).await {
                Ok(new_events) => {
                    let existing_ids: std::collections::HashSet<_> = {
                        let current = global_streams.read();
                        current.iter().map(|e| e.id).collect()
                    };

                    let unique_events: Vec<_> = new_events.into_iter()
                        .filter(|e| !existing_ids.contains(&e.id))
                        .collect();

                    if unique_events.is_empty() {
                        has_more_global.set(false);
                        loading_global.set(false);
                        log::info!("No new unique global streams found");
                    } else {
                        if let Some(last_event) = unique_events.last() {
                            oldest_timestamp_global.set(Some(last_event.created_at.as_secs()));
                        }

                        has_more_global.set(unique_events.len() >= 50);

                        let mut current = global_streams.read().clone();
                        current.extend(unique_events);
                        global_streams.set(current);
                        loading_global.set(false);
                    }
                }
                Err(e) => {
                    log::error!("Failed to load more global streams: {}", e);
                    loading_global.set(false);
                }
            }
        });
    };

    // Note: Infinite scroll handled by individual "Load more" buttons for each section

    rsx! {
        div {
            class: "min-h-screen bg-background",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/95 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-6 py-4 flex items-center justify-between max-w-[1600px] mx-auto",

                    h1 {
                        class: "text-2xl font-bold flex items-center gap-3",
                        svg {
                            class: "w-7 h-7",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"
                            }
                        }
                        "Live Streams"
                    }

                    div {
                        class: "flex items-center gap-3",

                        // Create Stream button
                        if auth_store::AUTH_STATE.read().is_authenticated {
                            Link {
                                to: Route::LiveStreamNew {},
                                class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white font-medium rounded-lg transition",
                                "Create Stream"
                            }
                        }

                        button {
                            class: "p-2 hover:bg-accent rounded-full transition disabled:opacity-50",
                            disabled: *loading_following.read() || *loading_global.read(),
                            onclick: move |_| {
                                let current = *refresh_trigger.read();
                                refresh_trigger.set(current + 1);
                            },
                            title: "Refresh",
                            if *loading_following.read() || *loading_global.read() {
                                span {
                                    class: "inline-block w-5 h-5 border-2 border-foreground border-t-transparent rounded-full animate-spin"
                                }
                            } else {
                                crate::components::icons::RefreshIcon { class: "w-5 h-5" }
                            }
                        }
                    }
                }
            }

            // Content
            div {
                class: "max-w-[1600px] mx-auto px-6 py-6",

                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else {
                    // Status filter tabs
                    div {
                        class: "flex gap-2 mb-6",
                        button {
                            class: if *status_filter.read() == StatusFilter::Live {
                                "px-4 py-2 bg-red-600 text-white font-medium rounded-lg"
                            } else {
                                "px-4 py-2 bg-accent hover:bg-accent/80 font-medium rounded-lg transition"
                            },
                            onclick: move |_| status_filter.set(StatusFilter::Live),
                            "🔴 Live"
                        }
                        button {
                            class: if *status_filter.read() == StatusFilter::Upcoming {
                                "px-4 py-2 bg-blue-600 text-white font-medium rounded-lg"
                            } else {
                                "px-4 py-2 bg-accent hover:bg-accent/80 font-medium rounded-lg transition"
                            },
                            onclick: move |_| status_filter.set(StatusFilter::Upcoming),
                            "Upcoming"
                        }
                        button {
                            class: if *status_filter.read() == StatusFilter::All {
                                "px-4 py-2 bg-primary text-white font-medium rounded-lg"
                            } else {
                                "px-4 py-2 bg-accent hover:bg-accent/80 font-medium rounded-lg transition"
                            },
                            onclick: move |_| status_filter.set(StatusFilter::All),
                            "All"
                        }
                    }

                    // Following section - only show if loading or has streams
                    if *loading_following.read() || !following_streams.read().is_empty() || error_following.read().is_some() {
                        div {
                            class: "mb-12",
                            h2 {
                                class: "text-xl font-bold mb-4",
                                "Following"
                            }

                            if *loading_following.read() && following_streams.read().is_empty() {
                                div {
                                    class: "flex items-center justify-center py-20",
                                    div {
                                        class: "w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin"
                                    }
                                }
                            } else if let Some(err) = error_following.read().as_ref() {
                                div {
                                    class: "text-center py-20 text-muted-foreground",
                                    "Error loading streams: {err}"
                                }
                            } else {
                                div {
                                    class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-4",
                                    for event in following_streams.read().iter() {
                                        MiniLiveStreamCard {
                                            key: "{event.id}",
                                            event: event.clone()
                                        }
                                    }
                                }

                                // Loading indicator
                                if *loading_following.read() {
                                    div {
                                        class: "flex items-center justify-center py-8",
                                        div {
                                            class: "w-6 h-6 border-4 border-blue-500 border-t-transparent rounded-full animate-spin"
                                        }
                                    }
                                }

                                // Load more button
                                if *has_more_following.read() && !*loading_following.read() {
                                    div {
                                        class: "flex justify-center mt-6",
                                        button {
                                            class: "px-6 py-3 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                            onclick: move |_| load_more_following(),
                                            "Load More"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Global section
                    div {
                        h2 {
                            class: "text-xl font-bold mb-4",
                            "Global"
                        }

                        if *loading_global.read() && global_streams.read().is_empty() {
                            div {
                                class: "flex items-center justify-center py-20",
                                div {
                                    class: "w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin"
                                }
                            }
                        } else if let Some(err) = error_global.read().as_ref() {
                            div {
                                class: "text-center py-20 text-muted-foreground",
                                "Error loading streams: {err}"
                            }
                        } else if global_streams.read().is_empty() {
                            div {
                                class: "text-center py-20 text-muted-foreground",
                                "No global streams found"
                            }
                        } else {
                            div {
                                class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-4",
                                for event in global_streams.read().iter() {
                                    MiniLiveStreamCard {
                                        key: "{event.id}",
                                        event: event.clone()
                                    }
                                }
                            }

                            // Loading indicator
                            if *loading_global.read() {
                                div {
                                    class: "flex items-center justify-center py-8",
                                    div {
                                        class: "w-6 h-6 border-4 border-blue-500 border-t-transparent rounded-full animate-spin"
                                    }
                                }
                            }

                            // Load more button
                            if *has_more_global.read() && !*loading_global.read() {
                                div {
                                    class: "flex justify-center mt-6",
                                    button {
                                        class: "px-6 py-3 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                        onclick: move |_| load_more_global(),
                                        "Load More"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// Helper functions to load streams

async fn load_following_streams(until: Option<u64>, status: StatusFilter) -> Result<Vec<Event>, String> {
    let pubkey_str = auth_store::AUTH_STATE.read().pubkey.clone()
        .ok_or("Not authenticated")?;

    // Fetch contacts
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            return load_global_streams(until, status).await;
        }
    };

    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global streams");
        return load_global_streams(until, status).await;
    }

    // Parse contact pubkeys
    let mut authors = Vec::new();
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            authors.push(pk);
        }
    }

    if authors.is_empty() {
        log::warn!("No valid contact pubkeys, falling back to global feed");
        return load_global_streams(until, status).await;
    }

    // Fetch all content types (same as /videos page) to get same event pool
    let mut filter = Filter::new()
        .kinds([Kind::Custom(21), Kind::Custom(22), Kind::Custom(30311)])
        .authors(authors)
        .limit(50);

    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch streams: {}", e))?;

    // Filter to only livestreams (Kind 30311)
    let livestreams: Vec<Event> = events.into_iter()
        .filter(|e| e.kind == Kind::Custom(30311))
        .collect();

    Ok(filter_by_status(livestreams, status))
}

async fn load_global_streams(until: Option<u64>, status: StatusFilter) -> Result<Vec<Event>, String> {
    // Fetch all content types (same as /videos page) to get same event pool
    let mut filter = Filter::new()
        .kinds([Kind::Custom(21), Kind::Custom(22), Kind::Custom(30311)])
        .limit(50);

    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch streams: {}", e))?;

    // Filter to only livestreams (Kind 30311)
    let livestreams: Vec<Event> = events.into_iter()
        .filter(|e| e.kind == Kind::Custom(30311))
        .collect();

    Ok(filter_by_status(livestreams, status))
}

fn filter_by_status(events: Vec<Event>, status: StatusFilter) -> Vec<Event> {
    match status {
        StatusFilter::All => events,
        StatusFilter::Live => {
            events.into_iter().filter(|event| {
                event.tags.iter().any(|tag| {
                    let tag_vec = tag.clone().to_vec();
                    tag_vec.first().map(|s| s.as_str()) == Some("status") &&
                    tag_vec.get(1).map(|s| s.to_lowercase()) == Some("live".to_string())
                })
            }).collect()
        }
        StatusFilter::Upcoming => {
            events.into_iter().filter(|event| {
                event.tags.iter().any(|tag| {
                    let tag_vec = tag.clone().to_vec();
                    tag_vec.first().map(|s| s.as_str()) == Some("status") &&
                    tag_vec.get(1).map(|s| s.to_lowercase()) == Some("planned".to_string())
                })
            }).collect()
        }
    }
}
