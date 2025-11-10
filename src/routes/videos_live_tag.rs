use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::components::{ClientInitializing, MiniLiveStreamCard};
use crate::routes::Route;
use nostr_sdk::{Event, Filter, Kind, Timestamp};
use std::time::Duration;
use wasm_bindgen::JsCast;

#[component]
pub fn VideosLiveTag(tag: String) -> Element {
    let mut stream_events = use_signal(|| Vec::<Event>::new());
    let mut loading = use_signal(|| false);
    let mut refresh_trigger = use_signal(|| 0);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut error = use_signal(|| None::<String>);

    // Load streams with specific tag
    use_effect(use_reactive((&tag, &*refresh_trigger.read()), move |(current_tag, _)| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        loading.set(true);
        error.set(None);
        oldest_timestamp.set(None);
        has_more.set(true);

        spawn(async move {
            match load_streams_by_tag(&current_tag, None).await {
                Ok(events) => {
                    if let Some(last_event) = events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }

                    has_more.set(events.len() >= 50);
                    stream_events.set(events);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    }));

    // Clone tag for use in load more
    let tag_for_scroll = tag.clone();

    // Infinite scroll detection with inline loading logic
    let mut scroll_callback = use_signal(|| None::<wasm_bindgen::closure::Closure<dyn FnMut()>>);

    use_effect(use_reactive(&tag_for_scroll, move |current_tag| {
        // Remove old listener if it exists
        if let Some(old_callback) = scroll_callback.write().take() {
            if let Some(window) = web_sys::window() {
                window.remove_event_listener_with_callback("scroll", old_callback.as_ref().unchecked_ref()).ok();
            }
        }

        let window = web_sys::window().expect("no global window");
        let _document = window.document().expect("no document");

        // Clone tag for the callback closure
        let tag_for_callback = current_tag.clone();

        let callback = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            let window = web_sys::window().expect("no global window");
            let scroll_y = window.scroll_y().unwrap_or(0.0);
            let inner_height = window.inner_height().unwrap().as_f64().unwrap_or(0.0);
            let document = window.document().expect("no document");
            let body = document.body().expect("no body");
            let scroll_height = body.scroll_height() as f64;

            if scroll_y + inner_height >= scroll_height - 1000.0 {
                if *loading.read() || !*has_more.read() {
                    return;
                }

                let until = *oldest_timestamp.read();
                let current_tag = tag_for_callback.clone();

                loading.set(true);

                spawn(async move {
                    match load_streams_by_tag(&current_tag, until).await {
                        Ok(new_events) => {
                            let existing_ids: std::collections::HashSet<_> = {
                                let current = stream_events.read();
                                current.iter().map(|e| e.id).collect()
                            };

                            let unique_events: Vec<_> = new_events.into_iter()
                                .filter(|e| !existing_ids.contains(&e.id))
                                .collect();

                            if unique_events.is_empty() {
                                has_more.set(false);
                                loading.set(false);
                                log::info!("No new unique streams found, stopping pagination");
                            } else {
                                if let Some(last_event) = unique_events.last() {
                                    oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                                }

                                has_more.set(unique_events.len() >= 50);

                                let mut current = stream_events.read().clone();
                                current.extend(unique_events);
                                stream_events.set(current);
                                loading.set(false);
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to load more streams: {}", e);
                            loading.set(false);
                        }
                    }
                });
            }
        }) as Box<dyn FnMut()>);

        // Register the event listener (keep ownership of callback)
        window.add_event_listener_with_callback("scroll", callback.as_ref().unchecked_ref()).ok();

        // Store callback to clean up later
        scroll_callback.set(Some(callback));
    }));

    // Cleanup scroll listener on unmount
    use_drop(move || {
        if let Some(callback) = scroll_callback.write().take() {
            if let Some(window) = web_sys::window() {
                window.remove_event_listener_with_callback("scroll", callback.as_ref().unchecked_ref()).ok();
            }
        }
    });

    rsx! {
        div {
            class: "min-h-screen bg-background",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/95 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-6 py-4 flex items-center justify-between max-w-[1600px] mx-auto",

                    div {
                        class: "flex items-center gap-3",
                        Link {
                            to: Route::VideosLive {},
                            class: "hover:bg-accent p-2 rounded-full transition",
                            crate::components::icons::ArrowLeftIcon { class: "w-5 h-5" }
                        }
                        h1 {
                            class: "text-2xl font-bold",
                            "#{tag}"
                        }
                    }

                    button {
                        class: "p-2 hover:bg-accent rounded-full transition disabled:opacity-50",
                        disabled: *loading.read(),
                        onclick: move |_| {
                            let current = *refresh_trigger.read();
                            refresh_trigger.set(current + 1);
                        },
                        title: "Refresh",
                        if *loading.read() {
                            span {
                                class: "inline-block w-5 h-5 border-2 border-foreground border-t-transparent rounded-full animate-spin"
                            }
                        } else {
                            crate::components::icons::RefreshIcon { class: "w-5 h-5" }
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
                    // Streams grid
                    if *loading.read() && stream_events.read().is_empty() {
                        div {
                            class: "flex items-center justify-center py-20",
                            div {
                                class: "w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin"
                            }
                        }
                    } else if let Some(err) = error.read().as_ref() {
                        div {
                            class: "text-center py-20 text-muted-foreground",
                            "Error loading streams: {err}"
                        }
                    } else if stream_events.read().is_empty() {
                        div {
                            class: "text-center py-20 text-muted-foreground",
                            "No streams found with tag #{tag}"
                        }
                    } else {
                        div {
                            class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-4",
                            for event in stream_events.read().iter() {
                                MiniLiveStreamCard {
                                    key: "{event.id}",
                                    event: event.clone()
                                }
                            }
                        }

                        // Loading indicator for infinite scroll
                        if *loading.read() {
                            div {
                                class: "flex items-center justify-center py-8",
                                div {
                                    class: "w-6 h-6 border-4 border-blue-500 border-t-transparent rounded-full animate-spin"
                                }
                            }
                        }

                        // End of results message
                        if !*has_more.read() && !*loading.read() {
                            div {
                                class: "text-center py-8 text-muted-foreground",
                                "No more streams to load"
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn load_streams_by_tag(tag: &str, until: Option<u64>) -> Result<Vec<Event>, String> {
    let mut filter = Filter::new()
        .kind(Kind::from(30311))
        .custom_tag(
            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::T),
            tag
        )
        .limit(50);

    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch streams: {}", e))
}
