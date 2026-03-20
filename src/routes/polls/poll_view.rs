use crate::components::{ClientInitializing, CommentComposer, PollCard, ThreadedComment};
use crate::stores::subscription_manager;
use crate::stores::nostr_client;
use crate::utils::build_thread_tree;
use crate::utils::thread_tree::invalidate_thread_tree_cache;
use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, EventId, Filter, Kind, RelayPoolNotification, SubscriptionId, Timestamp};
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

const LIVE_UPDATES_MAX_RETRIES: u32 = 4;

fn merge_comments(existing: Vec<NostrEvent>, fetched: Vec<NostrEvent>) -> Vec<NostrEvent> {
    let mut by_id: HashMap<EventId, NostrEvent> =
        existing.into_iter().map(|event| (event.id, event)).collect();
    for event in fetched {
        by_id.insert(event.id, event);
    }
    let mut merged: Vec<NostrEvent> = by_id.into_values().collect();
    merged.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    merged
}

#[component]
pub fn PollView(noteid: String) -> Element {
    let mut poll_event = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut comments = use_signal(Vec::<NostrEvent>::new);
    let mut comments_error = use_signal(|| None::<String>);
    let mut live_updates_warning = use_signal(|| None::<String>);
    let mut loading_comments = use_signal(|| false);
    let mut show_comment_composer = use_signal(|| false);
    let mut comments_refresh = use_signal(|| 0u64);
    let mut live_updates_retry_count = use_signal(|| 0u32);
    let mut comment_sub_id: Signal<Option<SubscriptionId>> = use_signal(|| None);
    let comments_subscription_generation = use_hook(|| Arc::new(AtomicU64::new(0)));

    use_effect(move || {
        let noteid_str = noteid.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            log::info!("Waiting for client initialization before loading poll...");
            return;
        }
        spawn(async move {
            loading.set(true);
            error.set(None);
            match decode_event_id(&noteid_str) {
                Ok(event_id) => match fetch_poll_by_id(event_id).await {
                    Ok(Some(event)) => {
                        poll_event.set(Some(event));
                        loading.set(false);
                    }
                    Ok(None) => {
                        error.set(Some("Poll not found".to_string()));
                        loading.set(false);
                    }
                    Err(e) => {
                        error.set(Some(e));
                        loading.set(false);
                    }
                },
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    use_effect(move || {
        let _ = *comments_refresh.read();
        let event = poll_event.read().clone();
        let Some(event) = event else {
            return;
        };

        let event_id = event.id;
        let generation = comments_subscription_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation_counter = comments_subscription_generation.clone();
        spawn(async move {
            if comments.read().is_empty() {
                loading_comments.set(true);
            }
            comments_error.set(None);
            live_updates_warning.set(None);
            let subscription_handoff = Timestamp::now();
            let mut live_since = subscription_handoff;
            let filter = Filter::new().kind(Kind::Comment).event(event_id).limit(500);
            match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                Ok(comment_events) => {
                    if generation_counter.load(Ordering::SeqCst) == generation {
                        live_since = comment_events
                            .iter()
                            .map(|event| event.created_at)
                            .max()
                            .unwrap_or(subscription_handoff);
                        let merged = merge_comments(comments.read().clone(), comment_events);
                        invalidate_thread_tree_cache(&event_id);
                        comments.set(merged);
                    } else {
                        loading_comments.set(false);
                        return;
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch poll comments: {}", e);
                    if generation_counter.load(Ordering::SeqCst) == generation {
                        if comments.read().is_empty() {
                            comments_error.set(Some(format!("Failed to load comments: {}", e)));
                        } else {
                            log::warn!(
                                "Failed to refresh poll comments, keeping cached comments visible: {}",
                                e
                            );
                            live_updates_warning.set(Some(format!(
                                "Live updates unavailable. Showing cached comments. Use Retry to try again. ({})",
                                e
                            )));
                        }
                    } else {
                        loading_comments.set(false);
                        return;
                    }
                }
            }

            if let Some(client) = nostr_client::get_client() {
                let filter = Filter::new()
                    .kind(Kind::Comment)
                    .event(event_id)
                    .since(live_since)
                    .limit(0);

                match subscription_manager::subscribe_realtime(&client, filter, Some(600)).await {
                    Ok(subscription_id) => {
                        if generation_counter.load(Ordering::SeqCst) != generation {
                            subscription_manager::unsubscribe(&client, &subscription_id).await;
                            loading_comments.set(false);
                            return;
                        }

                        if let Some(old_sub_id) = comment_sub_id.replace(Some(subscription_id.clone())) {
                            let client = client.clone();
                            spawn(async move {
                                subscription_manager::unsubscribe(&client, &old_sub_id).await;
                            });
                        }

                        let generation_counter = generation_counter.clone();
                        spawn(async move {
                            let mut notifications = client.notifications();
                            while let Ok(notification) = notifications.recv().await {
                                if generation_counter.load(Ordering::SeqCst) != generation {
                                    break;
                                }
                                if let RelayPoolNotification::Event {
                                    subscription_id: sub_id,
                                    event,
                                    ..
                                } = notification
                                {
                                    if sub_id == subscription_id
                                        && generation_counter.load(Ordering::SeqCst) == generation
                                    {
                                        let already_exists =
                                            comments.read().iter().any(|e| e.id == event.id);
                                        if !already_exists {
                                            invalidate_thread_tree_cache(&event_id);
                                            comments_error.set(None);
                                            live_updates_warning.set(None);
                                            comments.write().push((*event).clone());
                                        }
                                    }
                                }
                            }
                        });
                        live_updates_retry_count.set(0);
                        live_updates_warning.set(None);
                    }
                    Err(e) => {
                        log::error!("Failed to subscribe for poll comments: {}", e);
                        if generation_counter.load(Ordering::SeqCst) != generation {
                            return;
                        }
                        let retry_count = *live_updates_retry_count.read();
                        let next_retry = retry_count.saturating_add(1);
                        let retry_delay_secs = 2u64.saturating_pow(retry_count);
                        if next_retry <= LIVE_UPDATES_MAX_RETRIES {
                            live_updates_retry_count.set(next_retry);
                            live_updates_warning.set(Some(format!(
                                "Live updates unavailable. New comments may not appear automatically. Retrying in {}s, or use Retry. ({})",
                                retry_delay_secs, e
                            )));
                            let generation_counter = generation_counter.clone();
                            spawn(async move {
                                crate::platform::timer::sleep(Duration::from_secs(retry_delay_secs)).await;
                                if generation_counter.load(Ordering::SeqCst) != generation {
                                    return;
                                }
                                comments_refresh
                                    .with_mut(|value| *value = value.wrapping_add(1));
                            });
                        } else {
                            live_updates_warning.set(Some(format!(
                                "Live updates unavailable. New comments may not appear automatically. Use Retry to try again. ({})",
                                e
                            )));
                        }
                    }
                }
            }
            if generation_counter.load(Ordering::SeqCst) == generation {
                loading_comments.set(false);
            }
        });
    });

    let current_poll_event = poll_event.read().clone();

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
                    Link {
                        to: crate::routes::Route::Polls {},
                        class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition",
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 19l-7-7 7-7",
                            }
                        }
                        "Back to Polls"
                    }
                    h1 { class: "text-xl font-bold", "📊 Poll" }
                }
            }
            div { class: "max-w-2xl mx-auto",
                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else if *loading.read() {
                    div { class: "flex items-center justify-center py-12",
                        div { class: "flex flex-col items-center gap-3 text-muted-foreground",
                            span { class: "inline-block w-8 h-8 border-4 border-current border-t-transparent rounded-full animate-spin" }
                            "Loading poll..."
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "⚠️" }
                        h3 { class: "text-xl font-semibold mb-2", "Error" }
                        p { class: "text-muted-foreground mb-4", "{err}" }
                        Link {
                            to: crate::routes::Route::Polls {},
                            class: "inline-block px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Back to Polls"
                        }
                    }
                } else if let Some(event) = current_poll_event.clone() {
                    div { class: "border-b border-border",
                        PollCard { event: event.clone() }
                    }
                    div { class: "pt-6 px-4",
                        div { class: "flex items-center justify-between mb-6",
                            h3 { class: "text-2xl font-bold", "Comments" }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition flex items-center gap-2",
                                onclick: move |_| show_comment_composer.set(true),
                                span { "Add Comment" }
                            }
                        }
                        if let Some(warning) = live_updates_warning.read().as_ref() {
                            div { class: "mb-4 rounded-lg border border-yellow-300 bg-yellow-50 dark:bg-yellow-950/30 dark:border-yellow-800 p-3 text-sm text-yellow-800 dark:text-yellow-200",
                                p { "{warning}" }
                                if *live_updates_retry_count.read() >= LIVE_UPDATES_MAX_RETRIES {
                                    button {
                                        class: "mt-3 px-3 py-1.5 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                        onclick: move |_| {
                                            live_updates_retry_count.set(0);
                                            live_updates_warning.set(None);
                                            comments_refresh.with_mut(|value| *value = value.wrapping_add(1));
                                        },
                                        "Retry now"
                                    }
                                }
                            }
                        }
                        if *loading_comments.read() {
                            div { class: "flex items-center justify-center py-10",
                                div { class: "text-center",
                                    div { class: "animate-spin text-4xl mb-2", "⚡" }
                                    p { class: "text-muted-foreground", "Loading comments..." }
                                }
                            }
                        } else {{
                            let comment_vec = comments.read().clone();
                            let thread_tree = build_thread_tree(comment_vec, &event.id);
                            let poll_event_id = event.id;
                            if let Some(err) = comments_error.read().as_ref() {
                                rsx! {
                                    div { class: "flex flex-col items-center justify-center py-10 px-4 text-center",
                                        p { class: "text-destructive font-medium", "Could not load comments" }
                                        p { class: "text-sm text-muted-foreground mt-1 mb-4", "{err}" }
                                        button {
                                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                            onclick: move |_| comments_refresh.with_mut(|value| *value = value.wrapping_add(1)),
                                            "Retry"
                                        }
                                    }
                                }
                            } else if thread_tree.is_empty() {
                                rsx! {
                                    div { class: "flex flex-col items-center justify-center py-10 px-4 text-center text-muted-foreground",
                                        p { "No comments yet" }
                                        p { class: "text-sm", "Be the first to comment!" }
                                    }
                                }
                            } else {
                                rsx! {
                                    div { class: "divide-y divide-border",
                                        for node in thread_tree {
                                            ThreadedComment {
                                                key: "{node.event.id}",
                                                node: node.clone(),
                                                depth: 0,
                                                on_reply: move |reply_event: NostrEvent| {
                                                    let already_exists = comments.read().iter().any(|e| e.id == reply_event.id);
                                                    if !already_exists {
                                                        invalidate_thread_tree_cache(&poll_event_id);
                                                        comments_error.set(None);
                                                        comments.write().push(reply_event);
                                                    }
                                                },
                                            }
                                        }
                                    }
                                }
                            }
                        }}
                    }
                    div { class: "p-4 text-sm text-muted-foreground",
                        p {
                            "Poll ID: "
                            code { class: "text-xs bg-muted px-2 py-1 rounded", "{event.id.to_hex()}" }
                        }
                    }
                    if *show_comment_composer.read() {
                        CommentComposer {
                            comment_on: event.clone(),
                            parent_comment: None,
                            on_close: move |_| show_comment_composer.set(false),
                            on_success: move |comment_event: NostrEvent| {
                                show_comment_composer.set(false);
                                let already_exists = comments.read().iter().any(|e| e.id == comment_event.id);
                                if !already_exists {
                                    let root_event_id = comment_event
                                        .tags
                                        .iter()
                                        .find_map(|tag| {
                                            let slice = tag.as_slice();
                                            if slice.first().map(|s| s.as_str()) == Some("e") {
                                                slice.get(1).and_then(|id| EventId::from_hex(id).ok())
                                            } else {
                                                None
                                            }
                                        })
                                        .unwrap_or(event.id);
                                    invalidate_thread_tree_cache(&root_event_id);
                                    comments_error.set(None);
                                    comments.write().push(comment_event);
                                }
                            },
                        }
                    }
                } else {
                    div { class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "📊" }
                        h3 { class: "text-xl font-semibold mb-2", "Poll not found" }
                        Link {
                            to: crate::routes::Route::Polls {},
                            class: "inline-block mt-4 px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Back to Polls"
                        }
                    }
                }
            }
        }
    }
}
/// Decode event ID from bech32 (note1...) or hex format
fn decode_event_id(noteid: &str) -> Result<EventId, String> {
    if noteid.starts_with("note1") {
        EventId::parse(noteid).map_err(|e| format!("Invalid note ID (bech32): {}", e))
    } else {
        EventId::from_hex(noteid).map_err(|e| format!("Invalid note ID (hex): {}", e))
    }
}
/// Fetch a poll event by ID
async fn fetch_poll_by_id(event_id: EventId) -> Result<Option<NostrEvent>, String> {
    let filter = Filter::new().id(event_id).kind(Kind::Poll).limit(1);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch poll: {}", e))?;
    Ok(events.into_iter().next())
}
