use std::collections::HashSet;
use std::rc::Rc;

use dioxus::prelude::*;
use nostr_sdk::Event as NostrEvent;

use crate::stores::{auth_store, bookmarks, nostr_client};
use crate::components::{NoteCard, ClientInitializing};
use crate::hooks::use_infinite_scroll::use_infinite_scroll;

#[component]
pub fn Bookmarks() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut bookmarked_events = use_signal(Vec::<NostrEvent>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    // Pagination state for infinite scroll
    let mut has_more = use_signal(|| true);
    let mut loaded_count = use_signal(|| 0usize);
    const BATCH_SIZE: usize = 50;

    // Cached mute/block lists for N+1 optimization
    let mut cached_muted_posts: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);
    let mut cached_blocked_users: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);

    // Fetch mute/block lists once when authenticated (N+1 optimization)
    // Single fetch for both muted posts and blocked users
    use_effect(move || {
        let is_authenticated = auth_store::is_authenticated();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Clear caches on logout to prevent stale data
        if !is_authenticated {
            cached_muted_posts.set(None);
            cached_blocked_users.set(None);
            return;
        }

        if !client_initialized {
            return;
        }

        // Only fetch if not already loaded
        if cached_muted_posts.peek().is_some() && cached_blocked_users.peek().is_some() {
            return;
        }

        // Capture auth state before spawn to guard against logout during fetch
        let auth_snapshot = auth_store::AUTH_STATE.peek().is_authenticated;
        spawn(async move {
            // Single fetch for both - avoids double fetch_mute_list() call
            // Only set caches on success; leave as None on error so we can retry
            if let Ok(data) = nostr_client::get_mute_list_data().await {
                // Guard against logout/account switch during fetch
                // Re-check auth state matches snapshot before writing
                if auth_store::AUTH_STATE.peek().is_authenticated == auth_snapshot && auth_snapshot {
                    cached_muted_posts.set(Some(Rc::new(data.muted_posts)));
                    cached_blocked_users.set(Some(Rc::new(data.blocked_users)));
                }
            }
        });
    });

    // Load initial batch of bookmarks on mount
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !auth_store::is_authenticated() {
            return;
        }

        // Only load if client is initialized
        if !client_initialized {
            return;
        }

        loading.set(true);
        error.set(None);
        loaded_count.set(0);
        bookmarked_events.set(Vec::new());
        has_more.set(true);

        spawn(async move {
            // Initialize bookmarks from relays
            match bookmarks::init_bookmarks().await {
                Ok(_) => {
                    let total_bookmarks = bookmarks::get_bookmarks_count();

                    // Fetch first batch of events
                    match bookmarks::fetch_bookmarked_events_paginated(0, Some(BATCH_SIZE)).await {
                        Ok(events) => {
                            let fetched_count = events.len();
                            bookmarked_events.set(events);
                            loaded_count.set(fetched_count);

                            // Check if there are more bookmarks to load
                            has_more.set(fetched_count < total_bookmarks);
                            log::info!("Loaded initial batch: {} / {} bookmarks", fetched_count, total_bookmarks);
                        }
                        Err(e) => {
                            error.set(Some(e));
                        }
                    }
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    // Load more bookmarks function for infinite scroll
    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }

        loading.set(true);

        spawn(async move {
            let current_loaded = *loaded_count.read();
            let total_bookmarks = bookmarks::get_bookmarks_count();

            log::info!("Loading more bookmarks: skip={}, limit={}", current_loaded, BATCH_SIZE);

            match bookmarks::fetch_bookmarked_events_paginated(current_loaded, Some(BATCH_SIZE)).await {
                Ok(new_events) => {
                    if !new_events.is_empty() {
                        // Append new events to existing ones (deduplicated by event ID)
                        let mut current_events = bookmarked_events.read().clone();
                        let existing_ids: std::collections::HashSet<_> = current_events.iter()
                            .map(|e| e.id.to_hex())
                            .collect();
                        // Only add events that aren't already in the list
                        let unique_new: Vec<_> = new_events.iter()
                            .filter(|e| !existing_ids.contains(&e.id.to_hex()))
                            .cloned()
                            .collect();
                        let unique_count = unique_new.len();
                        current_events.extend(unique_new);
                        bookmarked_events.set(current_events);

                        // Use the original new_events count for pagination tracking
                        // (we still processed these bookmark IDs even if events were duplicates)
                        let new_loaded_count = current_loaded + new_events.len();
                        loaded_count.set(new_loaded_count);

                        // Check if there are more bookmarks to load
                        has_more.set(new_loaded_count < total_bookmarks);
                        log::info!("Loaded more bookmarks: {} / {} total ({} unique added)", new_loaded_count, total_bookmarks, unique_count);
                    } else {
                        has_more.set(false);
                    }
                }
                Err(e) => {
                    log::error!("Failed to load more bookmarks: {}", e);
                    has_more.set(false); // Stop infinite scroll retries
                    error.set(Some(format!("Failed to load more bookmarks: {}", e))); // Show error to user
                }
            }
            loading.set(false);
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
                    class: "px-4 py-3",
                    h2 {
                        class: "text-xl font-bold",
                        "🔖 Bookmarks"
                    }
                }
            }

            // Not authenticated
            if !auth.is_authenticated {
                div {
                    class: "text-center py-12",
                    div {
                        class: "text-6xl mb-4",
                        "🔐"
                    }
                    h3 {
                        class: "text-xl font-semibold mb-2",
                        "Sign in to view bookmarks"
                    }
                    p {
                        class: "text-muted-foreground",
                        "Connect your account to save and view bookmarked posts"
                    }
                }
            } else {
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

                // Loading state
                if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && bookmarked_events.read().is_empty()) {
                    // Show client initializing animation during:
                    // 1. Client initialization
                    // 2. Initial bookmarks load (loading + no bookmarks, regardless of error state)
                    ClientInitializing {}
                } else if bookmarked_events.read().is_empty() {
                    div {
                        class: "text-center py-12",
                        div {
                            class: "text-6xl mb-4",
                            "📭"
                        }
                        h3 {
                            class: "text-xl font-semibold mb-2",
                            "No bookmarks yet"
                        }
                        p {
                            class: "text-muted-foreground mb-4",
                            "Bookmark posts to save them for later"
                        }
                        p {
                            class: "text-sm text-muted-foreground",
                            "Tip: Click the bookmark button on any post to save it"
                        }
                    }
                } else {
                    div {
                        class: "space-y-4 p-4",
                        p {
                            class: "text-sm text-muted-foreground mb-4",
                            "Showing {bookmarked_events.read().len()} of {bookmarks::get_bookmarks_count()} bookmarked post(s)"
                        }
                        for event in bookmarked_events.read().iter() {
                            NoteCard {
                                key: "{event.id}",
                                event: event.clone(),
                                collapsible: true,
                                cached_muted_posts: cached_muted_posts.read().clone(),
                                cached_blocked_users: cached_blocked_users.read().clone()
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
                                        "Loading more bookmarks..."
                                    }
                                }
                            }
                        } else if !bookmarked_events.read().is_empty() {
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
}
