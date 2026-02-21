use crate::components::{ClientInitializing, PollCard};
use crate::hooks::use_infinite_scroll;
use crate::services::aggregation::{
    fetch_interaction_counts_batch, stream_interaction_counts, InteractionCounts,
    InteractionStreamHandle,
};
use crate::stores::{auth_store, nostr_client};
use crate::stores::nostr_client::stream_events_immediate;
use dioxus::prelude::*;
use nostr_sdk::{Event, EventId, Filter, Kind, PublicKey, Timestamp};
use std::collections::{HashMap, HashSet};
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
    let mut events = use_signal(Vec::<Event>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut last_event_id = use_signal(|| None::<nostr_sdk::EventId>);
    let mut interaction_counts = use_signal(HashMap::<String, InteractionCounts>::new);
    let mut interaction_stream_handle: Signal<Option<InteractionStreamHandle>> = use_signal(|| None);
    let mut all_streamed_ids = use_signal(Vec::<EventId>::new);
    let mut request_id = use_signal(|| 0u64);
    use_drop(move || {
        if let Some(handle) = interaction_stream_handle.peek().clone() {
            spawn(async move {
                handle.unsubscribe().await;
            });
        }
    });
    use_effect(move || {
        let _ = refresh_trigger.read();
        let current_feed_type = *feed_type.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        loading.set(true);
        error.set(None);
        events.set(Vec::new());
        oldest_timestamp.set(None);
        last_event_id.set(None);
        has_more.set(true);
        // Clean up previous interaction stream explicitly before assigning new one
        let old_handle = interaction_stream_handle.peek().clone();
        interaction_stream_handle.set(None);
        interaction_counts.set(HashMap::new());
        all_streamed_ids.set(Vec::new());
        // Increment request_id for stale detection
        request_id.with_mut(|v| *v += 1);
        let current_id = *request_id.peek();
        spawn(async move {
            // Unsubscribe old handle at the start of the async context
            if let Some(handle) = old_handle {
                handle.unsubscribe().await;
            }
            // Build the filter based on feed type
            let filter = match current_feed_type {
                FeedType::Following => {
                    let pubkey_str = match auth_store::get_pubkey() {
                        Some(pk) => pk,
                        None => {
                            error.set(Some("Not authenticated. Please sign in to view your following feed.".to_string()));
                            loading.set(false);
                            return;
                        }
                    };
                    let contacts = match nostr_client::fetch_contacts(pubkey_str).await {
                        Ok(c) => c,
                        Err(e) => {
                            error.set(Some(e));
                            loading.set(false);
                            return;
                        }
                    };
                    let authors: Vec<PublicKey> = contacts
                        .iter()
                        .filter_map(|c| PublicKey::parse(c).ok())
                        .collect();
                    if authors.is_empty() {
                        loading.set(false);
                        return;
                    }
                    Filter::new().kind(Kind::Poll).authors(authors).limit(50)
                }
                FeedType::Global => {
                    Filter::new().kind(Kind::Poll).limit(50)
                }
            };
            // Stream events for fast time-to-first-post
            let mut seen_ids = HashSet::new();
            let result = stream_events_immediate(
                filter,
                Duration::from_secs(10),
                |event| {
                    if *request_id.peek() != current_id {
                        return;
                    }
                    if seen_ids.insert(event.id) {
                        events.with_mut(|current| {
                            // Insert in sorted order (newest first)
                            let pos = current.iter().position(|e| e.created_at < event.created_at)
                                .unwrap_or(current.len());
                            current.insert(pos, event);
                        });
                    }
                },
            ).await;
            if *request_id.peek() != current_id {
                loading.set(false);
                return;
            }
            match result {
                Ok(count) => {
                    if *feed_type.read() != current_feed_type {
                        loading.set(false);
                        return;
                    }
                    // Sort and update pagination state
                    events.with_mut(|current| {
                        current.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                    });
                    let current_events = events.read();
                    if let Some(last_event) = current_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                        last_event_id.set(Some(last_event.id));
                    }
                    has_more.set(count >= 50);
                    // Fetch interaction counts
                    let event_ids: Vec<EventId> = current_events.iter().map(|e| e.id).collect();
                    drop(current_events);
                    all_streamed_ids.set(event_ids.clone());
                    if !event_ids.is_empty() {
                        match fetch_interaction_counts_batch(
                            event_ids.clone(),
                            Duration::from_secs(5),
                        ).await {
                            Ok(counts) => {
                                if *request_id.peek() != current_id { loading.set(false); return; }
                                interaction_counts.set(counts);
                            }
                            Err(e) => {
                                log::warn!("Failed to fetch interaction counts: {e}");
                            }
                        }
                        if *request_id.peek() != current_id { loading.set(false); return; }
                        match stream_interaction_counts(
                            event_ids,
                            interaction_counts,
                            Some(600),
                        ).await {
                            Ok(handle) => {
                                if *request_id.peek() == current_id && *feed_type.read() == current_feed_type {
                                    interaction_stream_handle.set(Some(handle));
                                } else {
                                    handle.unsubscribe().await;
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to start interaction stream for polls: {}", e);
                            }
                        }
                    }
                    loading.set(false);
                }
                Err(e) => {
                    if *feed_type.read() != current_feed_type {
                        loading.set(false);
                        return;
                    }
                    error.set(Some(format!("Failed to fetch polls: {}", e)));
                    loading.set(false);
                }
            }
        });
    });
    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }
        let until = *oldest_timestamp.read();
        let last_id = *last_event_id.read();
        let current_feed_type = *feed_type.read();
        loading.set(true);
        let rid = *request_id.peek();
        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => load_following_polls(until, last_id).await,
                FeedType::Global => load_global_polls(until, last_id).await,
            };
            match result {
                Ok(new_events) => {
                    if *request_id.peek() != rid {
                        loading.set(false);
                        return;
                    }
                    if *feed_type.read() != current_feed_type {
                        loading.set(false);
                        return;
                    }
                    let raw_count = new_events.len();
                    let existing_ids: std::collections::HashSet<_> = {
                        let current = events.read();
                        current.iter().map(|e| e.id).collect()
                    };
                    let unique_new: Vec<_> = new_events
                        .into_iter()
                        .filter(|e| !existing_ids.contains(&e.id))
                        .collect();
                    if unique_new.is_empty() {
                        loading.set(false);
                        has_more.set(false);
                        return;
                    }
                    if let Some(last_event) = unique_new.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                        last_event_id.set(Some(last_event.id));
                    }
                    has_more.set(raw_count >= 50);
                    let new_event_ids: Vec<EventId> = unique_new.iter().map(|e| e.id).collect();
                    events
                        .with_mut(|current| {
                            current.extend(unique_new);
                        });
                    loading.set(false);
                    // Fetch interaction counts for new page and extend streaming
                    if !new_event_ids.is_empty() {
                        if let Ok(counts) = fetch_interaction_counts_batch(
                            new_event_ids.clone(),
                            Duration::from_secs(5),
                        ).await {
                            if *request_id.peek() != rid { return; }
                            interaction_counts.with_mut(|existing| {
                                existing.extend(counts);
                            });
                        } else {
                            log::warn!(
                                "Failed to fetch interaction counts for load_more (rid={}, events={})",
                                rid,
                                new_event_ids.len()
                            );
                        }
                        if *request_id.peek() != rid { return; }
                        // Extend streaming subscription to cover new poll IDs
                        all_streamed_ids.with_mut(|ids| {
                            ids.extend(new_event_ids);
                        });
                        // Restart interaction stream with all IDs
                        let old_handle = interaction_stream_handle.peek().clone();
                        if let Some(handle) = old_handle {
                            interaction_stream_handle.set(None);
                            handle.unsubscribe().await;
                        }
                        let full_ids = all_streamed_ids.read().clone();
                        match stream_interaction_counts(
                            full_ids,
                            interaction_counts,
                            Some(600),
                        ).await {
                            Ok(handle) => {
                                if *feed_type.read() == current_feed_type {
                                    interaction_stream_handle.set(Some(handle));
                                } else {
                                    handle.unsubscribe().await;
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to restart interaction stream for polls: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
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
    let sentinel_id = use_infinite_scroll(load_more, has_more, loading);
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center justify-between",
                    div { class: "relative",
                        button {
                            class: "text-xl font-bold flex items-center gap-2 p-2 hover:bg-accent rounded-lg transition",
                            onclick: move |_| {
                                let current = *show_dropdown.read();
                                show_dropdown.set(!current);
                            },
                            "📊 {feed_type.read().label()}"
                            span { class: "text-sm", "▼" }
                        }
                        if *show_dropdown.read() {
                            div {
                                class: "fixed inset-0 z-40",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_dropdown.set(false);
                                },
                            }
                            div { class: "absolute top-full left-0 mt-1 bg-card border border-border rounded-lg shadow-lg overflow-hidden z-40 min-w-[150px]",
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
                    div { class: "flex items-center gap-2",
                        button {
                            class: "px-4 py-2 text-sm rounded-lg hover:bg-accent transition",
                            onclick: move |_| {
                                refresh_trigger.with_mut(|v| *v += 1);
                            },
                            "↻ Refresh"
                        }
                        Link {
                            to: crate::routes::Route::PollNew {},
                            class: "px-4 py-2 text-sm rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition font-medium",
                            "Create Poll"
                        }
                    }
                }
            }
            div { class: "max-w-2xl mx-auto",
                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "text-center py-12 px-4",
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
                    div { class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "📊" }
                        h3 { class: "text-xl font-semibold mb-2", "No polls yet" }
                        p { class: "text-muted-foreground",
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
                    div { class: "divide-y divide-border",
                        for event in events.read().iter() {
                            PollCard {
                            key: "{event.id}",
                            event: event.clone(),
                            precomputed_counts: interaction_counts.read().get(&event.id.to_hex()).cloned(),
                        }
                        }
                    }
                    div { id: "{sentinel_id}", class: "p-8 flex justify-center",
                        if *loading.read() {
                            div { class: "flex items-center gap-3 text-muted-foreground",
                                span { class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                "Loading more..."
                            }
                        } else if !*has_more.read() {
                            p { class: "text-muted-foreground text-sm", "No more polls to load" }
                        }
                    }
                }
            }
        }
    }
}
/// Load polls from followed users
async fn load_following_polls(
    until: Option<u64>,
    _last_event_id: Option<nostr_sdk::EventId>,
) -> Result<Vec<Event>, String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated. Please sign in to view your following feed.")?;
    let contacts = nostr_client::fetch_contacts(pubkey_str).await?;
    let authors: Vec<PublicKey> = contacts
        .iter()
        .filter_map(|c| PublicKey::parse(c).ok())
        .collect();
    if authors.is_empty() {
        return Ok(Vec::new());
    }
    let mut filter = Filter::new().kind(Kind::Poll).authors(authors).limit(50);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch polls: {}", e))?;
    let mut event_vec: Vec<Event> = events.into_iter().collect();
    event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(event_vec)
}
/// Load polls from everyone (global feed)
async fn load_global_polls(
    until: Option<u64>,
    _last_event_id: Option<nostr_sdk::EventId>,
) -> Result<Vec<Event>, String> {
    let mut filter = Filter::new().kind(Kind::Poll).limit(50);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch polls: {}", e))?;
    let mut event_vec: Vec<Event> = events.into_iter().collect();
    event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(event_vec)
}
