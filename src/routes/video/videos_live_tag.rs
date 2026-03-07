use crate::components::{ClientInitializing, MiniLiveStreamCard};
use crate::routes::Route;
use crate::stores::nostr_client;
use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, Timestamp};
use std::time::Duration;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
#[component]
pub fn VideosLiveTag(tag: String) -> Element {
    let mut stream_events = use_signal(Vec::<Event>::new);
    let mut loading = use_signal(|| false);
    let mut refresh_trigger = use_signal(|| 0);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut error = use_signal(|| None::<String>);
    let mut fetch_gen = use_signal(|| 0u32);
    use_effect(
        use_reactive(
            (&tag, &*refresh_trigger.read()),
            move |(current_tag, _)| {
                let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
                if !client_initialized {
                    return;
                }
                loading.set(true);
                error.set(None);
                oldest_timestamp.set(None);
                has_more.set(true);
                let gen = fetch_gen.with_mut(|g| { *g = g.wrapping_add(1); *g });
                spawn(async move {
                    match load_streams_by_tag(&current_tag, None).await {
                        Ok((events, _hit_limit)) => {
                            // Verify generation before updating state
                            if *fetch_gen.read() != gen {
                                log::debug!("Stale fetch detected, discarding results");
                                return;
                            }
                            if let Some(last_event) = events.last() {
                                oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                            }
                            #[cfg(feature = "web")]
                            has_more.set(_hit_limit);
                            #[cfg(not(feature = "web"))]
                            has_more.set(false);
                            stream_events.set(events);
                            loading.set(false);
                        }
                        Err(e) => {
                            // Verify generation before updating state
                            if *fetch_gen.read() != gen {
                                log::debug!("Stale fetch detected, discarding results");
                                return;
                            }
                            error.set(Some(e));
                            has_more.set(false);
                            loading.set(false);
                        }
                    }
                });
            },
        ),
    );
    #[cfg(feature = "web")]
    {
        let tag_for_scroll = tag.clone();
        let mut scroll_callback = use_signal(|| {
            None::<wasm_bindgen::closure::Closure<dyn FnMut()>>
        });
        use_effect(
            use_reactive(
                &tag_for_scroll,
                move |current_tag| {
                    if let Some(old_callback) = scroll_callback.write().take() {
                        if let Some(window) = web_sys::window() {
                            window
                                .remove_event_listener_with_callback(
                                    "scroll",
                                    old_callback.as_ref().unchecked_ref(),
                                )
                                .ok();
                        }
                    }
                    let Some(window) = web_sys::window() else { return; };
                    let tag_for_callback = current_tag.clone();
                    let callback = wasm_bindgen::closure::Closure::wrap(
                        Box::new(move || {
                            let Some(window) = web_sys::window() else { return; };
                            let scroll_y = window.scroll_y().unwrap_or(0.0);
                            let inner_height = window
                                .inner_height()
                                .ok()
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0);
                            let Some(document) = window.document() else { return; };
                            let Some(body) = document.body() else { return; };
                            let scroll_height = body.scroll_height() as f64;
                                if scroll_y + inner_height >= scroll_height - 1000.0 {
                                if *loading.read() || !*has_more.read() {
                                    return;
                                }
                                let until = *oldest_timestamp.read();
                                let current_tag = tag_for_callback.clone();
                                loading.set(true);
                                let this_gen = fetch_gen.with_mut(|g| {
                                    *g = g.wrapping_add(1);
                                    *g
                                });
                                spawn(async move {
                                    trigger_load_more_for_tag(
                                        current_tag,
                                        until,
                                        this_gen,
                                        &fetch_gen,
                                        &mut loading,
                                        &mut has_more,
                                        &mut oldest_timestamp,
                                        &mut stream_events,
                                    ).await;
                                });
                            }
                        }) as Box<dyn FnMut()>,
                    );
                    window
                        .add_event_listener_with_callback(
                            "scroll",
                            callback.as_ref().unchecked_ref(),
                        )
                        .ok();
                    scroll_callback.set(Some(callback));
                },
            ),
        );
        use_drop(move || {
            if let Some(callback) = scroll_callback.write().take() {
                if let Some(window) = web_sys::window() {
                    let _ = window.remove_event_listener_with_callback(
                        "scroll",
                        callback.as_ref().unchecked_ref(),
                    );
                }
            }
        });
    }
    rsx! {
        div { class: "min-h-screen bg-background",
            div { class: "sticky top-0 z-20 bg-background/95 backdrop-blur-sm border-b border-border",
                div { class: "px-6 py-4 flex items-center justify-between max-w-[1600px] mx-auto",
                    div { class: "flex items-center gap-3",
                        Link {
                            to: Route::VideosLive {},
                            class: "hover:bg-accent p-2 rounded-full transition",
                            crate::components::icons::ArrowLeftIcon { class: "w-5 h-5" }
                        }
                        h1 { class: "text-2xl font-bold", "#{tag}" }
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
                            span { class: "inline-block w-5 h-5 border-2 border-foreground border-t-transparent rounded-full animate-spin" }
                        } else {
                            crate::components::icons::RefreshIcon { class: "w-5 h-5" }
                        }
                    }
                }
            }
            div { class: "max-w-[1600px] mx-auto px-6 py-6",
                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else {
                    if *loading.read() && stream_events.read().is_empty() {
                        div { class: "flex items-center justify-center py-20",
                            div { class: "w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" }
                        }
                    } else if let Some(err) = error.read().as_ref() {
                        div { class: "text-center py-20 text-muted-foreground",
                            "Error loading streams: {err}"
                        }
                    } else if stream_events.read().is_empty() {
                        div { class: "text-center py-20 text-muted-foreground",
                            "No streams found with tag #{tag}"
                        }
                    } else {
                        div { class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-4",
                            for event in stream_events.read().iter() {
                                MiniLiveStreamCard { key: "{event.id}", event: event.clone() }
                            }
                        }
                        if *loading.read() {
                            div { class: "flex items-center justify-center py-8",
                                div { class: "w-6 h-6 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" }
                            }
                        }
                        if !*has_more.read() && !*loading.read() {
                            div { class: "text-center py-8 text-muted-foreground",
                                "No more streams to load"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
async fn trigger_load_more_for_tag(
    tag: String,
    until: Option<u64>,
    expected_gen: u32,
    fetch_gen: &Signal<u32>,
    loading: &mut Signal<bool>,
    has_more: &mut Signal<bool>,
    oldest_timestamp: &mut Signal<Option<u64>>,
    stream_events: &mut Signal<Vec<Event>>,
) {
    match load_streams_by_tag(&tag, until).await {
        Ok((new_events, _hit_limit)) => {
            if *fetch_gen.read() != expected_gen {
                log::debug!("Stale fetch detected, discarding results");
                return;
            }
            let existing_ids: std::collections::HashSet<_> = {
                let current = stream_events.read();
                current.iter().map(|e| e.id).collect()
            };
            let unique_events: Vec<_> = new_events
                .into_iter()
                .filter(|e| !existing_ids.contains(&e.id))
                .collect();
            if unique_events.is_empty() {
                has_more.set(false);
                loading.set(false);
                return;
            }
            if let Some(last_event) = unique_events.last() {
                oldest_timestamp.set(Some(last_event.created_at.as_secs()));
            }
            #[cfg(feature = "web")]
            has_more.set(_hit_limit);
            #[cfg(not(feature = "web"))]
            has_more.set(false);
            stream_events.write().extend(unique_events);
            loading.set(false);
        }
        Err(e) => {
            if *fetch_gen.read() != expected_gen {
                log::debug!("Stale fetch detected, discarding results");
                return;
            }
            log::error!("Failed to load more streams: {}", e);
            has_more.set(false);
            loading.set(false);
        }
    }
}

#[cfg(not(feature = "web"))]
#[allow(dead_code)]
fn trigger_load_more_for_platform(
    tag: String,
    until: Option<u64>,
    fetch_gen: &Signal<u32>,
    loading: &Signal<bool>,
    has_more: &Signal<bool>,
    oldest_timestamp: &Signal<Option<u64>>,
    stream_events: &Signal<Vec<Event>>,
) {
    if *loading.read() || !*has_more.read() {
        return;
    }
    let mut loading_sig = *loading;
    let mut has_more_sig = *has_more;
    let mut oldest_timestamp_sig = *oldest_timestamp;
    let mut stream_events_sig = *stream_events;
    let mut fetch_gen_sig = *fetch_gen;
    loading_sig.set(true);
    let this_gen = fetch_gen_sig.with_mut(|g| {
        *g = g.wrapping_add(1);
        *g
    });
    spawn(async move {
        trigger_load_more_for_tag(
            tag,
            until,
            this_gen,
            &fetch_gen_sig,
            &mut loading_sig,
            &mut has_more_sig,
            &mut oldest_timestamp_sig,
            &mut stream_events_sig,
        )
        .await;
    });
}

async fn load_streams_by_tag(
    tag: &str,
    until: Option<u64>,
) -> Result<(Vec<Event>, bool), String> {
    let mut filter = Filter::new()
        .kind(Kind::from(30311))
        .custom_tag(nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::T), tag)
        .limit(50);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch streams: {}", e))?;
    let mut sorted_events: Vec<Event> = events.into_iter().collect();
    sorted_events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let hit_limit = sorted_events.len() >= 50;
    Ok((sorted_events, hit_limit))
}
