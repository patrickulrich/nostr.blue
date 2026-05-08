use crate::components::{ClientInitializing, ReplyComposer, PhotoCard, ThreadedComment};
use crate::hooks::{use_mute_block_cache, use_relay_subscription};
use crate::stores::nostr_client;
use crate::utils::build_thread_tree;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use nostr_sdk::{Event, EventId};
use std::time::Duration;
#[component]
pub fn PhotoDetail(photo_id: String) -> Element {
    let mut photo_event = use_signal(|| None::<Event>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut comments = use_signal(Vec::<Event>::new);
    let mut loading_comments = use_signal(|| false);
    let mut show_comment_composer = use_signal(|| false);
    let (cached_muted_posts, cached_blocked_users) = use_mute_block_cache();
    use_effect(move || {
        let id = photo_id.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            log::info!("Waiting for client initialization before loading photo...");
            return;
        }
        loading.set(true);
        error.set(None);
        spawn(async move {
            match load_photo_by_id(&id).await {
                Ok(event) => {
                    photo_event.set(Some(event));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });
    use_effect(move || {
        let photo = photo_event.read().clone();
        if let Some(event) = photo {
            loading_comments.set(true);
            spawn(async move {
                let event_id = event.id;
                let event_id_hex = event_id.to_hex();
                log::info!("Loading comments for photo {}", event_id_hex);
                let upper_e_tag = nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E);
                let filter_upper = Filter::new()
                    .kind(Kind::Comment)
                    .custom_tag(upper_e_tag, event_id_hex.clone())
                    .limit(500);
                let filter_lower = Filter::new()
                    .kinds(vec![Kind::TextNote, Kind::Comment])
                    .event(event_id)
                    .limit(500);
                log::info!("Fetching comments with uppercase E and lowercase e tag filters");
                let mut all_comments = Vec::new();
                if let Ok(upper_comments) =
                    nostr_client::fetch_events_aggregated(filter_upper, Duration::from_secs(10))
                        .await
                {
                    log::info!(
                        "Loaded {} comments with uppercase E tags",
                        upper_comments.len()
                    );
                    all_comments.extend(upper_comments.into_iter());
                } else {
                    log::warn!("Failed to fetch comments with uppercase E tags");
                }
                if let Ok(lower_comments) =
                    nostr_client::fetch_events_aggregated(filter_lower, Duration::from_secs(10))
                        .await
                {
                    log::info!(
                        "Loaded {} comments with lowercase e tags",
                        lower_comments.len()
                    );
                    all_comments.extend(lower_comments.into_iter());
                } else {
                    log::warn!("Failed to fetch comments with lowercase e tags");
                }
                let mut seen_ids = std::collections::HashSet::new();
                let unique_comments: Vec<Event> = all_comments
                    .into_iter()
                    .filter(|event| seen_ids.insert(event.id))
                    .collect();
                let mut sorted_comments = unique_comments;
                sorted_comments.sort_by_key(|a| a.created_at);
                log::info!("Total unique comments: {}", sorted_comments.len());
                comments.set(sorted_comments);
                loading_comments.set(false);
            });
        }
    });

    {
        let comment_filter = photo_event.read().as_ref().map(|event| {
            Filter::new()
                .kinds(vec![Kind::TextNote, Kind::Comment])
                .event(event.id)
                .since(Timestamp::now())
                .limit(0)
        });
        use_relay_subscription(comment_filter, move |event: &nostr::Event| {
            let already_exists = comments.read().iter().any(|e| e.id == event.id);
            if !already_exists {
                log::info!(
                    "New comment received via streaming: {}",
                    event.id.to_hex()
                );
                comments.write().push(event.clone());
            }
        });
    }

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-3",
                    Link {
                        to: crate::routes::Route::Photos {},
                        class: "hover:bg-accent p-2 rounded-full transition",
                        "← Back"
                    }
                    h2 { class: "text-xl font-bold", "Photo" }
                }
            }
            div { class: "max-w-[600px] mx-auto",
                if !*nostr_client::CLIENT_INITIALIZED.read()
                    || (*loading.read() && photo_event.read().is_none())
                {
                    ClientInitializing {}
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "p-6 text-center",
                        div { class: "max-w-md mx-auto",
                            div { class: "text-4xl mb-2", "⚠️" }
                            p { class: "text-red-600 dark:text-red-400 mb-4",
                                "Error loading photo: {err}"
                            }
                            Link {
                                to: crate::routes::Route::Photos {},
                                class: "text-blue-500 hover:underline",
                                "← Back to Photos"
                            }
                        }
                    }
                } else if let Some(event) = photo_event.read().as_ref().cloned() {
                    PhotoCard { event: event.clone() }
                    div { class: "border-t border-border mt-4",
                        div { class: "p-4 flex items-center justify-between",
                            h3 { class: "font-semibold text-lg", "Comments ({comments.read().len()})" }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                onclick: move |_| show_comment_composer.set(true),
                                "+ Add Comment"
                            }
                        }
                        div { class: "px-4 pb-4",
                            if *loading_comments.read() {
                                div { class: "text-center py-8 text-muted-foreground",
                                    "Loading comments..."
                                }
                            } else if comments.read().is_empty() {
                                div { class: "text-center py-8 text-muted-foreground",
                                    "No comments yet. Be the first to comment!"
                                }
                            } else {{
                                let comment_vec = comments.read().clone();
                                let thread_tree = build_thread_tree(comment_vec, &event.id);
                                rsx! {
                                    div { class: "divide-y divide-border",
                                        for node in thread_tree {
                                            ThreadedComment { key: "{node.event.id}", node: node.clone(), depth: 0, cached_muted_posts: cached_muted_posts.read().clone(), cached_blocked_users: cached_blocked_users.read().clone() }
                                        }
                                    }
                                }
                            }}
                        }
                    }
                    if *show_comment_composer.read() {
                        ReplyComposer {
                            target: event.clone(),
                            root_event: None,
                            on_close: move |_| show_comment_composer.set(false),
                            on_success: move |_| {
                                show_comment_composer.set(false);
                                let event_clone = event.clone();
                                spawn(async move {
                                    loading_comments.set(true);
                                    let event_id = event_clone.id;
                                    let event_id_hex = event_id.to_hex();
                                    let upper_e_tag = nostr_sdk::SingleLetterTag::uppercase(
                                        nostr_sdk::Alphabet::E,
                                    );
                                    let filter_upper = Filter::new()
                                        .kind(Kind::Comment)
                                        .custom_tag(upper_e_tag, event_id_hex.clone())
                                        .limit(500);
                                    let filter_lower = Filter::new()
                                        .kinds(vec![Kind::TextNote, Kind::Comment])
                                        .event(event_id)
                                        .limit(500);
                                    let mut all_comments = Vec::new();
                                    if let Ok(upper_comments) = nostr_client::fetch_events_aggregated(
                                            filter_upper,
                                            Duration::from_secs(10),
                                        )
                                        .await
                                    {
                                        all_comments.extend(upper_comments.into_iter());
                                    }
                                    if let Ok(lower_comments) = nostr_client::fetch_events_aggregated(
                                            filter_lower,
                                            Duration::from_secs(10),
                                        )
                                        .await
                                    {
                                        all_comments.extend(lower_comments.into_iter());
                                    }
                                    let mut seen_ids = std::collections::HashSet::new();
                                    let unique_comments: Vec<Event> = all_comments
                                        .into_iter()
                                        .filter(|event| seen_ids.insert(event.id))
                                        .collect();
                                    let mut sorted_comments = unique_comments;
                                    sorted_comments.sort_by_key(|a| a.created_at);
                                    comments.set(sorted_comments);
                                    loading_comments.set(false);
                                });
                            },
                        }
                    }
                } else {
                    div { class: "p-6 text-center",
                        div { class: "max-w-md mx-auto",
                            div { class: "text-4xl mb-2", "📷" }
                            p { class: "text-muted-foreground mb-4", "Photo not found" }
                            Link {
                                to: crate::routes::Route::Photos {},
                                class: "text-blue-500 hover:underline",
                                "← Back to Photos"
                            }
                        }
                    }
                }
            }
        }
    }
}
async fn load_photo_by_id(photo_id: &str) -> std::result::Result<Event, String> {
    log::info!("Loading photo by ID: {}", photo_id);
    let event_id = EventId::parse(photo_id).map_err(|e| format!("Invalid photo ID: {}", e))?;
    let filter = Filter::new().id(event_id).kind(Kind::Custom(20)).limit(1);
    log::info!("Fetching photo event with filter: {:?}", filter);
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                log::info!("Loaded photo event: {}", event.id);
                Ok(event)
            } else {
                Err("Photo not found".to_string())
            }
        }
        Err(e) => {
            log::error!("Failed to fetch photo: {}", e);
            Err(format!("Failed to load photo: {}", e))
        }
    }
}
