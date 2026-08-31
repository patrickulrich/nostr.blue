//! Workout detail viewer for kind-1301 events (PollViewer pattern).
use crate::components::{ClientInitializing, ThreadedComment, WorkoutCard};
use crate::hooks::{use_mute_block_cache, use_relay_subscription};
use crate::services::aggregation::get_counts_with_count_fallback;
use crate::stores::nostr_client;
use crate::utils::nips::nip101e::KIND_WORKOUT;
use crate::utils::thread_tree::invalidate_thread_tree_cache;
use crate::utils::{build_thread_tree, extract_root_event_id};
use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, EventId, Filter, Kind, Timestamp};
use std::collections::HashMap;
use std::time::Duration;

fn merge_comments(existing: Vec<NostrEvent>, fetched: Vec<NostrEvent>) -> Vec<NostrEvent> {
    let mut by_id: HashMap<EventId, NostrEvent> =
        existing.into_iter().map(|event| (event.id, event)).collect();
    for event in fetched {
        by_id.insert(event.id, event);
    }
    let mut merged: Vec<NostrEvent> = by_id.into_values().collect();
    merged.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    merged
}

fn insert_comment(existing: Vec<NostrEvent>, event: NostrEvent) -> Vec<NostrEvent> {
    merge_comments(existing, vec![event])
}

async fn fetch_workout_by_id(event_id: EventId) -> Result<Option<NostrEvent>, String> {
    let filter = Filter::new()
        .id(event_id)
        .kind(Kind::from(KIND_WORKOUT))
        .limit(1);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch workout: {}", e))?;
    Ok(events.into_iter().next())
}

#[component]
pub fn WorkoutViewer(note_id: String) -> Element {
    let mut workout_event = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut comments = use_signal(Vec::<NostrEvent>::new);
    let mut comments_error = use_signal(|| None::<String>);
    let mut loading_comments = use_signal(|| false);
    let mut reply_total = use_signal(|| 0usize);
    let mut comments_refresh = use_signal(|| 0u64);
    let (cached_muted_posts, cached_blocked_users, cached_muted_words) = use_mute_block_cache();

    use_effect(use_reactive!(|note_id| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            log::info!("Waiting for client initialization before loading workout...");
            return;
        }
        spawn(async move {
            loading.set(true);
            error.set(None);
            match nostr_client::parse_event_id(&note_id) {
                Some(parsed) => {
                    let event_id = parsed.event_id;
                    match fetch_workout_by_id(event_id).await {
                        Ok(Some(event)) => {
                            workout_event.set(Some(event));
                            loading.set(false);
                        }
                        Ok(None) => {
                            error.set(Some("Workout not found".to_string()));
                            loading.set(false);
                        }
                        Err(e) => {
                            error.set(Some(e));
                            loading.set(false);
                        }
                    }
                }
                None => {
                    error.set(Some("Invalid workout ID".to_string()));
                    loading.set(false);
                }
            }
        });
    }));

    use_effect(move || {
        let _ = *comments_refresh.read();
        let event = workout_event.read().clone();
        let Some(event) = event else {
            return;
        };
        let event_id = event.id;
        spawn(async move {
            if comments.read().is_empty() {
                loading_comments.set(true);
            }
            comments_error.set(None);
            let counts = get_counts_with_count_fallback(&event_id, Duration::from_secs(10)).await;
            reply_total.with_mut(|total| *total = (*total).max(counts.replies));
            let filter = Filter::new()
                .kinds([Kind::Comment, Kind::TextNote])
                .event(event_id)
                .limit(500);
            match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                Ok(comment_events) => {
                    invalidate_thread_tree_cache(&event_id);
                    let merged = merge_comments(comments.read().clone(), comment_events);
                    comments.set(merged);
                }
                Err(e) => {
                    log::error!("Failed to fetch workout comments: {}", e);
                    if comments.read().is_empty() {
                        comments_error.set(Some(format!("Failed to load comments: {}", e)));
                    }
                }
            }
            loading_comments.set(false);
        });
    });

    {
        let event_id = workout_event.read().clone().map(|e| e.id);
        let comment_filter = event_id.map(|eid| {
            Filter::new()
                .kinds([Kind::Comment, Kind::TextNote])
                .event(eid)
                .since(Timestamp::now())
                .limit(0)
        });
        let mut comments_mut = comments;
        let mut comments_error_mut = comments_error;
        let mut reply_total_mut = reply_total;
        use_relay_subscription(comment_filter, move |event: &nostr::Event| {
            let already_exists = comments_mut.read().iter().any(|e| e.id == event.id);
            if !already_exists {
                if let Some(eid) = event_id {
                    invalidate_thread_tree_cache(&eid);
                }
                comments_error_mut.set(None);
                let next_comments = insert_comment(comments_mut.read().clone(), event.clone());
                comments_mut.set(next_comments);
                reply_total_mut.with_mut(|count| *count = count.saturating_add(1));
            }
        });
    }

    let current_workout_event = workout_event.read().clone();
    let route_loaded_comments = comments.read().len();
    let route_replies_count = std::cmp::max(*reply_total.read(), route_loaded_comments);

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
                    Link {
                        to: crate::routes::Route::Workouts {},
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
                        "Back to Workouts"
                    }
                    h1 { class: "text-xl font-bold", "Workout" }
                }
            }
            div { class: "max-w-2xl mx-auto",
                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else if *loading.read() {
                    div { class: "flex items-center justify-center py-12",
                        div { class: "flex flex-col items-center gap-3 text-muted-foreground",
                            span { class: "inline-block w-8 h-8 border-4 border-current border-t-transparent rounded-full animate-spin" }
                            "Loading workout..."
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "\u{26A0}\u{FE0F}" }
                        h3 { class: "text-xl font-semibold mb-2", "Error" }
                        p { class: "text-muted-foreground mb-4", "{err}" }
                        Link {
                            to: crate::routes::Route::Workouts {},
                            class: "inline-block px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Back to Workouts"
                        }
                    }
                } else if let Some(event) = current_workout_event.clone() {
                    div { class: "border-b border-border",
                        WorkoutCard {
                            event: event.clone(),
                            replies_count: Some(route_replies_count),
                            on_comment_created: move |comment_event: NostrEvent| {
                                let already_exists = comments.read().iter().any(|e| e.id == comment_event.id);
                                if !already_exists {
                                    let root_event_id =
                                        extract_root_event_id(&comment_event).unwrap_or(event.id);
                                    invalidate_thread_tree_cache(&root_event_id);
                                    comments_error.set(None);
                                    let next_comments =
                                        insert_comment(comments.read().clone(), comment_event);
                                    comments.set(next_comments);
                                    reply_total.with_mut(|count| *count = count.saturating_add(1));
                                }
                            },
                        }
                    }
                    div { class: "pt-6 px-4",
                        h3 { class: "text-2xl font-bold mb-6", "Comments" }
                        if *loading_comments.read() {
                            div { class: "flex items-center justify-center py-10",
                                div { class: "text-center",
                                    div { class: "animate-spin text-4xl mb-2", "\u{26A1}" }
                                    p { class: "text-muted-foreground", "Loading comments..." }
                                }
                            }
                        } else {{
                            let comment_vec = comments.read().clone();
                            let thread_tree = build_thread_tree(comment_vec, &event.id);
                            let workout_event_id = event.id;
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
                                                root_event: Some(event.clone()),
                                                cached_muted_posts: cached_muted_posts.read().clone(),
                                                cached_blocked_users: cached_blocked_users.read().clone(),
                                                cached_muted_words: cached_muted_words.read().clone(),
                                                on_reply: move |reply_event: NostrEvent| {
                                                    let already_exists = comments.read().iter().any(|e| e.id == reply_event.id);
                                                    if !already_exists {
                                                        invalidate_thread_tree_cache(&workout_event_id);
                                                        comments_error.set(None);
                                                        let next_comments = insert_comment(
                                                            comments.read().clone(),
                                                            reply_event,
                                                        );
                                                        comments.set(next_comments);
                                                        reply_total.with_mut(|count| *count = count.saturating_add(1));
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
                        div { class: "text-6xl mb-4", "\u{1F3CB}\u{FE0F}" }
                        h3 { class: "text-xl font-semibold mb-2", "Workout not found" }
                        Link {
                            to: crate::routes::Route::Workouts {},
                            class: "inline-block mt-4 px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Back to Workouts"
                        }
                    }
                }
            }
        }
    }
}
