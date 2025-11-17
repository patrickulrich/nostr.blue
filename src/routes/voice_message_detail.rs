use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::components::{VoiceMessageCard, ThreadedComment, VoiceReplyComposer, ClientInitializing};
use crate::utils::build_thread_tree;
use nostr_sdk::{Event, Filter, Kind, EventId};
use std::time::Duration;

#[component]
pub fn VoiceMessageDetail(voice_id: String) -> Element {
    let mut voice_event = use_signal(|| None::<Event>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut replies = use_signal(|| Vec::<Event>::new());
    let mut loading_replies = use_signal(|| false);
    let mut show_voice_reply_composer = use_signal(|| false);

    // Load voice message on mount - wait for client to be initialized
    use_effect(move || {
        let id = voice_id.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            log::info!("Waiting for client initialization before loading voice message...");
            return;
        }

        loading.set(true);
        error.set(None);

        // Clear profile cache to prevent stale author metadata
        crate::stores::profiles::PROFILE_CACHE.write().clear();

        spawn(async move {
            match load_voice_message_by_id(&id).await {
                Ok(event) => {
                    voice_event.set(Some(event));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Load replies when voice message is loaded
    use_effect(move || {
        let voice = voice_event.read().clone();

        if let Some(event) = voice {
            loading_replies.set(true);

            spawn(async move {
                let event_id = event.id;
                let event_id_hex = event_id.to_hex();

                log::info!("Loading replies for voice message {}", event_id_hex);

                // Fetch both voice replies and text replies
                let mut all_replies = Vec::new();

                // Fetch voice message replies (Kind::VoiceMessageReply)
                let filter_voice_replies = Filter::new()
                    .kind(Kind::VoiceMessageReply)
                    .event(event_id)
                    .limit(500);

                // Fetch text note replies (Kind::TextNote)
                let filter_text_replies = Filter::new()
                    .kind(Kind::TextNote)
                    .event(event_id)
                    .limit(500);

                log::info!("Fetching voice and text replies");

                if let Ok(voice_replies) = nostr_client::fetch_events_aggregated(filter_voice_replies, Duration::from_secs(10)).await {
                    log::info!("Loaded {} voice replies", voice_replies.len());
                    all_replies.extend(voice_replies.into_iter());
                } else {
                    log::warn!("Failed to fetch voice replies");
                }

                if let Ok(text_replies) = nostr_client::fetch_events_aggregated(filter_text_replies, Duration::from_secs(10)).await {
                    log::info!("Loaded {} text replies", text_replies.len());
                    all_replies.extend(text_replies.into_iter());
                } else {
                    log::warn!("Failed to fetch text replies");
                }

                // Deduplicate by event ID
                let mut seen_ids = std::collections::HashSet::new();
                let unique_replies: Vec<Event> = all_replies.into_iter()
                    .filter(|event| seen_ids.insert(event.id))
                    .collect();

                let mut sorted_replies = unique_replies;
                sorted_replies.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                log::info!("Total unique replies: {}", sorted_replies.len());
                replies.set(sorted_replies);

                loading_replies.set(false);
            });
        }
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center gap-3",
                    Link {
                        to: crate::routes::Route::VoiceMessages {},
                        class: "hover:bg-accent p-2 rounded-full transition",
                        "← Back"
                    }
                    h2 {
                        class: "text-xl font-bold",
                        "Voice Message"
                    }
                }
            }

            // Content
            div {
                class: "max-w-[600px] mx-auto",

                if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && voice_event.read().is_none()) {
                    // Show client initializing animation during:
                    // 1. Client initialization
                    // 2. Initial voice message load (loading + no voice message, regardless of error state)
                    ClientInitializing {}
                } else if let Some(err) = error.read().as_ref() {
                    // Error state
                    div {
                        class: "p-6 text-center",
                        div {
                            class: "max-w-md mx-auto",
                            div {
                                class: "text-4xl mb-2",
                                "⚠️"
                            }
                            p {
                                class: "text-red-600 dark:text-red-400 mb-4",
                                "Error loading voice message: {err}"
                            }
                            Link {
                                to: crate::routes::Route::VoiceMessages {},
                                class: "text-blue-500 hover:underline",
                                "← Back to Voice Messages"
                            }
                        }
                    }
                } else if let Some(event) = voice_event.read().as_ref().cloned() {
                    // Show voice message card
                    VoiceMessageCard {
                        event: event.clone()
                    }

                    // Replies section
                    div {
                        class: "border-t border-border mt-4",

                        // Replies header
                        div {
                            class: "p-4 flex items-center justify-between",
                            h3 {
                                class: "font-semibold text-lg",
                                "Replies ({replies.read().len()})"
                            }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                onclick: move |_| show_voice_reply_composer.set(true),
                                "+ Voice Reply"
                            }
                        }

                        // Replies list
                        div {
                            class: "px-4 pb-4",
                            if *loading_replies.read() {
                                div {
                                    class: "text-center py-8 text-muted-foreground",
                                    "Loading replies..."
                                }
                            } else if replies.read().is_empty() {
                                div {
                                    class: "text-center py-8 text-muted-foreground",
                                    "No replies yet. Be the first to reply!"
                                }
                            } else {
                                // Build thread tree and render - handle both voice and text replies
                                {
                                    let reply_vec = replies.read().clone();
                                    let thread_tree = build_thread_tree(reply_vec.clone(), &event.id);

                                    rsx! {
                                        div {
                                            class: "divide-y divide-border",
                                            for node in thread_tree {
                                                {
                                                    let event_kind = node.event.kind;

                                                    // Render voice replies with VoiceMessageCard
                                                    if event_kind == Kind::VoiceMessageReply {
                                                        rsx! {
                                                            div {
                                                                key: "{node.event.id}",
                                                                class: "py-4",
                                                                VoiceMessageCard {
                                                                    event: node.event.clone()
                                                                }
                                                                // Render children recursively
                                                                if !node.children.is_empty() {
                                                                    div {
                                                                        class: "ml-4 border-l-2 border-border pl-4",
                                                                        for child in &node.children {
                                                                            {render_reply_node(child)}
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        // Render text replies with ThreadedComment
                                                        rsx! {
                                                            ThreadedComment {
                                                                key: "{node.event.id}",
                                                                node: node.clone(),
                                                                depth: 0
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

                    // Voice reply composer modal
                    if *show_voice_reply_composer.read() {
                        VoiceReplyComposer {
                            reply_to: event.clone(),
                            on_close: move |_| show_voice_reply_composer.set(false),
                            on_success: move |_| {
                                show_voice_reply_composer.set(false);
                                // Reload replies
                                let event_clone = event.clone();
                                spawn(async move {
                                    loading_replies.set(true);
                                    let event_id = event_clone.id;

                                    let mut all_replies = Vec::new();

                                    let filter_voice_replies = Filter::new()
                                        .kind(Kind::VoiceMessageReply)
                                        .event(event_id)
                                        .limit(500);

                                    let filter_text_replies = Filter::new()
                                        .kind(Kind::TextNote)
                                        .event(event_id)
                                        .limit(500);

                                    if let Ok(voice_replies) = nostr_client::fetch_events_aggregated(filter_voice_replies, Duration::from_secs(10)).await {
                                        all_replies.extend(voice_replies.into_iter());
                                    }

                                    if let Ok(text_replies) = nostr_client::fetch_events_aggregated(filter_text_replies, Duration::from_secs(10)).await {
                                        all_replies.extend(text_replies.into_iter());
                                    }

                                    let mut seen_ids = std::collections::HashSet::new();
                                    let unique_replies: Vec<Event> = all_replies.into_iter()
                                        .filter(|event| seen_ids.insert(event.id))
                                        .collect();

                                    let mut sorted_replies = unique_replies;
                                    sorted_replies.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                                    replies.set(sorted_replies);
                                    loading_replies.set(false);
                                });
                            }
                        }
                    }
                } else {
                    // Empty state
                    div {
                        class: "p-6 text-center",
                        div {
                            class: "max-w-md mx-auto",
                            div {
                                class: "text-4xl mb-2",
                                "🎤"
                            }
                            p {
                                class: "text-muted-foreground mb-4",
                                "Voice message not found"
                            }
                            Link {
                                to: crate::routes::Route::VoiceMessages {},
                                class: "text-blue-500 hover:underline",
                                "← Back to Voice Messages"
                            }
                        }
                    }
                }
            }
        }
    }
}

// Helper function to render reply nodes recursively
fn render_reply_node(node: &crate::utils::thread_tree::ThreadNode) -> Element {
    let event_kind = node.event.kind;

    if event_kind == Kind::VoiceMessageReply {
        rsx! {
            div {
                key: "{node.event.id}",
                class: "py-4",
                VoiceMessageCard {
                    event: node.event.clone()
                }
                if !node.children.is_empty() {
                    div {
                        class: "ml-4 border-l-2 border-border pl-4",
                        for child in &node.children {
                            {render_reply_node(child)}
                        }
                    }
                }
            }
        }
    } else {
        rsx! {
            ThreadedComment {
                key: "{node.event.id}",
                node: node.clone(),
                depth: 0
            }
        }
    }
}

// Helper function to load a single voice message by ID
async fn load_voice_message_by_id(voice_id: &str) -> Result<Event, String> {
    log::info!("Loading voice message by ID: {}", voice_id);

    // Parse the event ID (could be hex or note1...)
    let event_id = EventId::parse(voice_id)
        .map_err(|e| format!("Invalid voice message ID: {}", e))?;

    // Create filter for voice messages (both VoiceMessage and VoiceMessageReply)
    let filter = Filter::new()
        .id(event_id)
        .kinds(vec![Kind::VoiceMessage, Kind::VoiceMessageReply])
        .limit(1);

    log::info!("Fetching voice message event with filter: {:?}", filter);

    // Fetch event from relays
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                log::info!("Loaded voice message event: {}", event.id);
                Ok(event)
            } else {
                Err("Voice message not found".to_string())
            }
        }
        Err(e) => {
            log::error!("Failed to fetch voice message: {}", e);
            Err(format!("Failed to load voice message: {}", e))
        }
    }
}
