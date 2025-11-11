use dioxus::prelude::*;
use dioxus::prelude::Event as DioxusEvent;
use dioxus_core::Task;
use nostr_sdk::prelude::*;

use crate::services::profile_search::{search_profiles, search_cached_profiles, get_contact_pubkeys, ProfileSearchResult};
use crate::routes::Route;

#[component]
pub fn SearchInput() -> Element {
    let mut query = use_signal(|| String::new());
    let mut show_dropdown = use_signal(|| false);
    let mut search_results = use_signal(|| Vec::<ProfileSearchResult>::new());
    let mut selected_index = use_signal(|| 0usize);
    let mut is_searching = use_signal(|| false);
    let mut relay_search_task = use_signal(|| None::<Task>);
    let mut contact_pubkeys = use_signal(|| Vec::<PublicKey>::new());
    let navigator = navigator();

    // Fetch contacts on mount
    use_effect(move || {
        spawn(async move {
            let contacts = get_contact_pubkeys().await;
            contact_pubkeys.set(contacts);
        });
    });

    let handle_input = move |evt: DioxusEvent<FormData>| {
        let new_value = evt.value().clone();
        query.set(new_value.clone());

        if new_value.is_empty() {
            show_dropdown.set(false);
            return;
        }

        // Show dropdown and reset selection
        show_dropdown.set(true);
        selected_index.set(0);

        // Search cached profiles immediately
        let contacts = contact_pubkeys.read().clone();
        let cached_results = search_cached_profiles(&new_value, 10, &contacts, &[]);
        search_results.set(cached_results.clone());

        // Query relays if query is long enough and we don't have many results
        if new_value.len() >= 2 && cached_results.len() < 5 {
            is_searching.set(true);

            // Cancel previous search task
            if let Some(task) = relay_search_task.read().as_ref() {
                task.cancel();
            }

            // Capture query for stale result verification
            let query_snapshot = new_value.clone();

            // Start new relay search with debounce
            let new_task = spawn(async move {
                // Debounce: wait 300ms
                #[cfg(target_family = "wasm")]
                {
                    gloo_timers::future::TimeoutFuture::new(300).await;
                }
                #[cfg(not(target_family = "wasm"))]
                {
                    use std::time::Duration;
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }

                // Perform relay search
                let query_relays = query_snapshot.len() >= 3;
                match search_profiles(&query_snapshot, 10, query_relays).await {
                    Ok(results) => {
                        // Only update if query hasn't changed
                        if query.read().as_str() == query_snapshot.as_str() {
                            search_results.set(results);
                            is_searching.set(false);
                        }
                    }
                    Err(e) => {
                        log::error!("Profile search failed: {}", e);
                        if query.read().as_str() == query_snapshot.as_str() {
                            is_searching.set(false);
                        }
                    }
                }
            });

            relay_search_task.set(Some(new_task));
        } else {
            is_searching.set(false);
        }
    };

    let handle_keydown = move |evt: DioxusEvent<KeyboardData>| {
        let key = evt.key();

        if *show_dropdown.read() {
            let results = search_results.read();

            match key {
                Key::ArrowDown => {
                    evt.prevent_default();
                    let current = *selected_index.read();
                    let max = results.len().saturating_sub(1);
                    if current < max {
                        selected_index.set(current + 1);
                    }
                }
                Key::ArrowUp => {
                    evt.prevent_default();
                    let current = *selected_index.read();
                    if current > 0 {
                        selected_index.set(current - 1);
                    }
                }
                Key::Enter => {
                    evt.prevent_default();
                    if !results.is_empty() {
                        // Profile selected, navigate to profile page
                        let selected = results.get(*selected_index.read());
                        if let Some(profile) = selected {
                            let pubkey_hex = profile.pubkey.to_hex();
                            navigator.push(Route::Profile { pubkey: pubkey_hex });
                            query.set(String::new());
                            show_dropdown.set(false);
                        }
                    } else {
                        // No results selected, do full search
                        let search_query = query.read().clone();
                        if !search_query.is_empty() {
                            navigator.push(Route::Search { q: search_query });
                            query.set(String::new());
                            show_dropdown.set(false);
                        }
                    }
                }
                Key::Escape => {
                    show_dropdown.set(false);
                }
                _ => {}
            }
        } else {
            // Dropdown not showing, just handle Enter for full search
            if key == Key::Enter {
                evt.prevent_default();
                let search_query = query.read().clone();
                if !search_query.is_empty() {
                    navigator.push(Route::Search { q: search_query });
                    query.set(String::new());
                }
            }
        }
    };

    // Close dropdown when clicking outside
    let close_dropdown = move |_| {
        show_dropdown.set(false);
    };

    rsx! {
        div {
            class: "relative",

            input {
                r#type: "text",
                placeholder: "Search Nostr...",
                value: "{query}",
                class: "w-full px-4 py-2 pr-10 bg-muted border border-border rounded-full text-sm focus:outline-none focus:ring-2 focus:ring-ring",
                oninput: handle_input,
                onkeydown: handle_keydown,
                onblur: close_dropdown,
            }

            div {
                class: "absolute right-2 top-1/2 -translate-y-1/2 p-1.5",
                "🔍"
            }

            // Autocomplete dropdown
            if *show_dropdown.read() {
                {render_dropdown(
                    &search_results.read(),
                    *selected_index.read(),
                    *is_searching.read(),
                    query,
                    show_dropdown,
                )}
            }
        }
    }
}

/// Render the autocomplete dropdown
fn render_dropdown(
    results: &[ProfileSearchResult],
    selected_index: usize,
    is_searching: bool,
    mut query: Signal<String>,
    mut show_dropdown: Signal<bool>,
) -> Element {
    let navigator = navigator();
    rsx! {
        div {
            class: "absolute top-full left-0 right-0 mt-2 bg-white dark:bg-gray-800 shadow-lg rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden z-50 max-h-96",

            if is_searching {
                div {
                    class: "px-4 py-3 text-sm text-gray-500 dark:text-gray-400",
                    "Searching..."
                }
            } else if results.is_empty() {
                div {
                    class: "px-4 py-3 text-sm text-gray-500 dark:text-gray-400",
                    "No profiles found"
                }
            } else {
                div {
                    class: "overflow-y-auto max-h-96",
                    for (index, profile) in results.iter().enumerate() {
                        {
                            let profile_clone = profile.clone();
                            let is_selected = index == selected_index;

                            rsx! {
                                button {
                                    key: "{profile.pubkey.to_hex()}",
                                    class: if is_selected {
                                        "w-full px-4 py-2 flex items-center gap-3 hover:bg-blue-50 dark:hover:bg-blue-900 bg-blue-50 dark:bg-blue-900 cursor-pointer transition"
                                    } else {
                                        "w-full px-4 py-2 flex items-center gap-3 hover:bg-gray-100 dark:hover:bg-gray-700 cursor-pointer transition"
                                    },
                                    onmousedown: move |evt| {
                                        evt.prevent_default(); // Prevent blur event
                                        let pubkey_hex = profile_clone.pubkey.to_hex();
                                        navigator.push(Route::Profile { pubkey: pubkey_hex });
                                        query.set(String::new());
                                        show_dropdown.set(false);
                                    },

                                    // Avatar
                                    div {
                                        class: "flex-shrink-0",
                                        if let Some(picture) = &profile.picture {
                                            img {
                                                src: "{picture}",
                                                class: "w-8 h-8 rounded-full",
                                                alt: "{profile.get_display_name()}",
                                                loading: "lazy"
                                            }
                                        } else {
                                            div {
                                                class: "w-8 h-8 rounded-full bg-gray-300 dark:bg-gray-600 flex items-center justify-center text-xs font-bold",
                                                {profile.get_display_name().chars().next().unwrap_or('?').to_string()}
                                            }
                                        }
                                    }

                                    // Profile info
                                    div {
                                        class: "flex-1 text-left min-w-0",
                                        div {
                                            class: "font-semibold text-sm text-gray-900 dark:text-gray-100 truncate",
                                            {profile.get_display_name()}
                                        }
                                        if let Some(username) = profile.get_username() {
                                            div {
                                                class: "text-xs text-gray-500 dark:text-gray-400 truncate",
                                                "@{username}"
                                            }
                                        }
                                    }

                                    // Contact badge
                                    if profile.is_contact {
                                        div {
                                            class: "flex-shrink-0 text-xs px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded-full",
                                            "Following"
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
}
