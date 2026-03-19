use crate::components::{ClientInitializing, CommentComposer, PollCard, ThreadedComment};
use crate::stores::nostr_client;
use crate::utils::build_thread_tree;
use dioxus::prelude::*;
use dioxus_core::use_drop;
use nostr_sdk::{Event as NostrEvent, EventId, Filter, Kind, RelayPoolNotification, SubscriptionId, Timestamp};
use std::time::Duration;
#[component]
pub fn PollView(noteid: String) -> Element {
    let mut poll_event = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut comments = use_signal(Vec::<NostrEvent>::new);
    let mut loading_comments = use_signal(|| false);
    let mut show_comment_composer = use_signal(|| false);
    let mut comment_sub_id: Signal<Option<SubscriptionId>> = use_signal(|| None);

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
        let event = poll_event.read().clone();
        let Some(event) = event else {
            return;
        };

        let event_id = event.id;
        spawn(async move {
            loading_comments.set(true);
            let filter = Filter::new().kind(Kind::Comment).event(event_id).limit(500);
            match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                Ok(mut comment_events) => {
                    comment_events.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                    comments.set(comment_events);
                }
                Err(e) => {
                    log::error!("Failed to fetch poll comments: {}", e);
                }
            }
            loading_comments.set(false);

            if let Some(client) = nostr_client::get_client() {
                let filter = Filter::new()
                    .kind(Kind::Comment)
                    .event(event_id)
                    .since(Timestamp::now())
                    .limit(0);

                match client.subscribe(filter, None).await {
                    Ok(output) => {
                        let subscription_id = output.val;
                        comment_sub_id.set(Some(subscription_id.clone()));

                        spawn(async move {
                            let mut notifications = client.notifications();
                            while let Ok(notification) = notifications.recv().await {
                                if let RelayPoolNotification::Event {
                                    subscription_id: sub_id,
                                    event,
                                    ..
                                } = notification
                                {
                                    if sub_id == subscription_id {
                                        let already_exists =
                                            comments.read().iter().any(|e| e.id == event.id);
                                        if !already_exists {
                                            comments.write().push((*event).clone());
                                        }
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        log::error!("Failed to subscribe for poll comments: {}", e);
                    }
                }
            }
        });
    });

    use_drop(move || {
        if let Some(sub_id) = comment_sub_id.peek().clone() {
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    client.unsubscribe(&sub_id).await;
                }
            });
        }
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
                            if thread_tree.is_empty() {
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
                                                        comments.write().push(reply_event);
                                                        crate::utils::thread_tree::invalidate_thread_tree_cache(&poll_event_id);
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
                                            if slice.first().map(|s| s.as_str()) == Some("E") {
                                                slice.get(1).and_then(|id| EventId::from_hex(id).ok())
                                            } else {
                                                None
                                            }
                                        })
                                        .unwrap_or(event.id);
                                    comments.write().push(comment_event);
                                    crate::utils::thread_tree::invalidate_thread_tree_cache(&root_event_id);
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
