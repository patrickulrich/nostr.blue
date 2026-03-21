use crate::components::{ClientInitializing, PollCard, ThreadedComment};
use crate::stores::nostr_client;
use crate::stores::subscription_manager;
use crate::utils::thread_tree::invalidate_thread_tree_cache;
use crate::utils::{build_thread_tree, extract_root_event_id};
use dioxus::prelude::*;
use dioxus_core::{spawn_forever, Task};
use nostr_sdk::{
    Event as NostrEvent, EventId, Filter, Kind, RelayPoolNotification, SubscriptionId, Timestamp,
};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use std::time::Duration;

const LIVE_UPDATES_MAX_RETRIES: u32 = 4;

fn merge_comments(existing: Vec<NostrEvent>, fetched: Vec<NostrEvent>) -> Vec<NostrEvent> {
    let mut by_id: HashMap<EventId, NostrEvent> = existing
        .into_iter()
        .map(|event| (event.id, event))
        .collect();
    for event in fetched {
        by_id.insert(event.id, event);
    }
    let mut merged: Vec<NostrEvent> = by_id.into_values().collect();
    merged.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    merged
}

fn schedule_live_updates_retry(
    generation_counter: Arc<AtomicU32>,
    generation: u32,
    reason: String,
    mut loading_comments: Signal<bool>,
    mut live_updates_retry_count: Signal<u32>,
    mut live_updates_warning: Signal<Option<String>>,
    mut comments_refresh: Signal<u64>,
) {
    if generation_counter.load(Ordering::SeqCst) != generation {
        return;
    }

    loading_comments.set(false);
    let retry_count = *live_updates_retry_count.read();
    let next_retry = retry_count.saturating_add(1);
    let retry_delay_secs = 2u64.saturating_pow(retry_count);
    if next_retry <= LIVE_UPDATES_MAX_RETRIES {
        live_updates_retry_count.set(next_retry);
        live_updates_warning.set(Some(format!(
            "Live updates unavailable. New comments may not appear automatically. Retrying in {}s. ({})",
            retry_delay_secs, reason
        )));
        spawn(async move {
            crate::platform::timer::sleep(Duration::from_secs(retry_delay_secs)).await;
            if generation_counter.load(Ordering::SeqCst) != generation {
                return;
            }
            comments_refresh.with_mut(|value| *value = value.wrapping_add(1));
        });
    } else {
        live_updates_warning.set(Some(format!(
            "Live updates unavailable. New comments may not appear automatically. Retry manually from the comments section. ({})",
            reason
        )));
    }
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
    let mut comments_refresh = use_signal(|| 0u64);
    let mut live_updates_retry_count = use_signal(|| 0u32);
    let mut comment_sub_id: Signal<Option<SubscriptionId>> = use_signal(|| None);
    let mut comment_listener_task: Signal<Option<Task>> = use_signal(|| None);
    let comments_subscription_generation = use_hook(|| Arc::new(AtomicU32::new(0)));
    use_drop(move || {
        if let Some(task) = comment_listener_task.replace(None) {
            task.cancel();
        }
        if let Some(sub_id) = comment_sub_id.replace(None) {
            if let Some(client) = nostr_client::get_client() {
                spawn_forever(async move {
                    subscription_manager::unsubscribe(&client, &sub_id).await;
                });
            }
        }
    });

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
        if let Some(task) = comment_listener_task.replace(None) {
            task.cancel();
        }
        if let Some(old_sub_id) = comment_sub_id.replace(None) {
            if let Some(client) = nostr_client::get_client() {
                spawn(async move {
                    subscription_manager::unsubscribe(&client, &old_sub_id).await;
                });
            }
        }
        spawn(async move {
            if comments.read().is_empty() {
                loading_comments.set(true);
            }
            comments_error.set(None);
            live_updates_warning.set(None);
            let subscription_handoff = Timestamp::now();
            let cached_max = comments
                .read()
                .iter()
                .map(|comment| comment.created_at)
                .max();
            let mut live_since = cached_max
                .map(|timestamp| std::cmp::min(subscription_handoff, timestamp))
                .unwrap_or(subscription_handoff);
            let filter = Filter::new().kind(Kind::Comment).event(event_id).limit(500);
            match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                Ok(comment_events) => {
                    if generation_counter.load(Ordering::SeqCst) == generation {
                        let fetched_max = comment_events.iter().map(|event| event.created_at).max();
                        live_since = fetched_max
                            .map(|timestamp| std::cmp::min(subscription_handoff, timestamp))
                            .unwrap_or(subscription_handoff);
                        let merged = merge_comments(comments.read().clone(), comment_events);
                        invalidate_thread_tree_cache(&event_id);
                        comments.set(merged);
                    } else {
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
                                "Live updates unavailable. Showing cached comments. ({})",
                                e
                            )));
                        }
                    } else {
                        return;
                    }
                }
            }

            if let Some(client) = nostr_client::get_client() {
                let filter = Filter::new()
                    .kind(Kind::Comment)
                    .event(event_id)
                    .since(live_since);

                match subscription_manager::subscribe_realtime(&client, filter, Some(600)).await {
                    Ok(subscription_id) => {
                        if generation_counter.load(Ordering::SeqCst) != generation {
                            subscription_manager::unsubscribe(&client, &subscription_id).await;
                            return;
                        }

                        comment_sub_id.set(Some(subscription_id.clone()));

                        let generation_counter = generation_counter.clone();
                        let mut listener_comment_sub_id = comment_sub_id;
                        let mut listener_comment_listener_task = comment_listener_task;
                        let listener_subscription_id = subscription_id.clone();
                        let listener_task = spawn(async move {
                            let mut notifications = client.notifications();
                            loop {
                                let notification = match notifications.recv().await {
                                    Ok(notification) => notification,
                                    Err(e) => {
                                        if generation_counter.load(Ordering::SeqCst) == generation {
                                            if listener_comment_sub_id.read().as_ref()
                                                == Some(&listener_subscription_id)
                                            {
                                                listener_comment_sub_id.set(None);
                                            }
                                            listener_comment_listener_task.set(None);
                                            schedule_live_updates_retry(
                                                generation_counter.clone(),
                                                generation,
                                                e.to_string(),
                                                loading_comments,
                                                live_updates_retry_count,
                                                live_updates_warning,
                                                comments_refresh,
                                            );
                                        }
                                        break;
                                    }
                                };
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
                        comment_listener_task.set(Some(listener_task));
                        live_updates_retry_count.set(0);
                        live_updates_warning.set(None);
                    }
                    Err(e) => {
                        log::error!("Failed to subscribe for poll comments: {}", e);
                        if generation_counter.load(Ordering::SeqCst) != generation {
                            return;
                        }
                        if let Some(old_sub_id) = comment_sub_id.replace(None) {
                            subscription_manager::unsubscribe(&client, &old_sub_id).await;
                        }
                        comment_listener_task.set(None);
                        schedule_live_updates_retry(
                            generation_counter.clone(),
                            generation,
                            e.to_string(),
                            loading_comments,
                            live_updates_retry_count,
                            live_updates_warning,
                            comments_refresh,
                        );
                    }
                }
            }
            if generation_counter.load(Ordering::SeqCst) == generation {
                loading_comments.set(false);
            }
        });
    });

    let current_poll_event = poll_event.read().clone();
    let route_replies_count = comments.read().len();

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
                        PollCard {
                            event: event.clone(),
                            replies_count: Some(route_replies_count),
                            on_comment_created: move |comment_event: NostrEvent| {
                                let already_exists = comments.read().iter().any(|e| e.id == comment_event.id);
                                if !already_exists {
                                    let root_event_id =
                                        extract_root_event_id(&comment_event).unwrap_or(event.id);
                                    invalidate_thread_tree_cache(&root_event_id);
                                    comments_error.set(None);
                                    comments.write().push(comment_event);
                                }
                            },
                        }
                    }
                    div { class: "pt-6 px-4",
                        div { class: "mb-6",
                            h3 { class: "text-2xl font-bold", "Comments" }
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
