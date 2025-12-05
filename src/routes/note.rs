use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::routes::Route;
use crate::components::{NoteCard, ThreadedComment, ClientInitializing, VoiceMessageCard};
use crate::utils::{build_thread_tree, merge_pending_into_tree, event::is_voice_message};
use crate::stores::pending_comments::get_pending_comments;
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;
use std::time::Duration;

// Helper functions for parallel loading

async fn fetch_main_note(event_id: EventId) -> std::result::Result<NostrEvent, String> {
    let filter = Filter::new().id(event_id);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;
    events.into_iter().next().ok_or("Event not found".to_string())
}

/// Extract parent event IDs from note tags (NIP-10 lowercase 'e' and NIP-22 uppercase 'E')
fn extract_parent_ids(note: &NostrEvent) -> Vec<EventId> {
    // Use SDK's event_ids() for NIP-10 lowercase 'e' tags
    let mut ids: Vec<EventId> = note.tags.event_ids().cloned().collect();

    // Also extract NIP-22 uppercase 'E' tags (for Comment kind)
    let upper_e = nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E);
    for tag in note.tags.iter() {
        if tag.kind() == nostr_sdk::TagKind::SingleLetter(upper_e) {
            if let Some(content) = tag.content() {
                if let Ok(id) = EventId::from_hex(content) {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
    }
    ids
}

/// Fetch parent events by their IDs
async fn fetch_parents_by_ids(parent_ids: Vec<EventId>) -> std::result::Result<Vec<NostrEvent>, String> {
    if parent_ids.is_empty() {
        return Ok(Vec::new());
    }

    let filter = Filter::new()
        .ids(parent_ids)
        .kinds(vec![Kind::TextNote, Kind::VoiceMessage, Kind::VoiceMessageReply, Kind::Comment]);

    nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await
}

async fn fetch_replies(event_id: EventId) -> std::result::Result<Vec<NostrEvent>, String> {
    // Fetch replies using both lowercase 'e' (NIP-10) and uppercase 'E' (NIP-22) tags
    let event_id_hex = event_id.to_hex();

    // Filter for lowercase 'e' tag references (NIP-10 standard)
    let filter_lower = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::VoiceMessage, Kind::VoiceMessageReply])
        .event(event_id)
        .limit(100);

    // Filter for uppercase 'E' tag references (NIP-22 root references)
    let upper_e_tag = nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E);
    let filter_upper = Filter::new()
        .kinds(vec![Kind::VoiceMessage, Kind::VoiceMessageReply, Kind::Comment])
        .custom_tag(upper_e_tag, event_id_hex)
        .limit(100);

    // Fetch both in parallel and combine
    let mut all_replies = Vec::new();

    let (lower_result, upper_result) = tokio::join!(
        nostr_client::fetch_events_aggregated(filter_lower, Duration::from_secs(10)),
        nostr_client::fetch_events_aggregated(filter_upper, Duration::from_secs(10))
    );

    if let Ok(lower_replies) = lower_result {
        all_replies.extend(lower_replies);
    }
    if let Ok(upper_replies) = upper_result {
        all_replies.extend(upper_replies);
    }

    // Deduplicate by event ID
    let mut seen_ids = std::collections::HashSet::new();
    let unique_replies: Vec<NostrEvent> = all_replies.into_iter()
        .filter(|event| seen_ids.insert(event.id))
        .collect();

    Ok(unique_replies)
}

#[component]
pub fn Note(note_id: String, from_voice: Option<String>) -> Element {
    // Determine initial is_voice_note from prop (for immediate correct header on deep-link)
    let initial_is_voice = from_voice.as_ref().map_or(false, |v| v == "true");
    let mut note_data = use_signal(|| None::<NostrEvent>);
    let mut parent_events = use_signal(|| Vec::<NostrEvent>::new());
    let mut replies = use_signal(|| Vec::<NostrEvent>::new());
    let mut loading = use_signal(|| true);
    let mut loading_parents = use_signal(|| false);
    let mut loading_replies = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    // PARALLEL LOADING - Fetch all data at once (10s instead of 30s)
    use_effect(use_reactive!(|note_id| {
        let note_id_str = note_id.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            log::info!("Waiting for client initialization before loading note...");
            return;
        }

        spawn(async move {
            loading.set(true);
            loading_parents.set(true);
            loading_replies.set(true);
            error.set(None);

            // Clear profile cache to prevent stale author metadata when navigating between notes
            crate::stores::profiles::PROFILE_CACHE.write().clear();

            // Parse the note ID
            let event_id = match EventId::from_bech32(&note_id_str)
                .or_else(|_| EventId::from_hex(&note_id_str)) {
                Ok(id) => id,
                Err(e) => {
                    error.set(Some(format!("Invalid note ID: {}", e)));
                    loading.set(false);
                    loading_parents.set(false);
                    loading_replies.set(false);
                    return;
                }
            };

            // Fetch main note first (needed to extract parent IDs)
            let note_result = fetch_main_note(event_id).await;

            // Process main note and extract parent IDs
            let parent_ids = match &note_result {
                Ok(event) => {
                    note_data.set(Some(event.clone()));
                    loading.set(false);
                    extract_parent_ids(event)
                }
                Err(e) => {
                    error.set(Some(e.clone()));
                    loading.set(false);
                    loading_parents.set(false);
                    loading_replies.set(false);
                    return;
                }
            };

            // Now fetch parents and replies in parallel (no duplicate main note fetch)
            let (parents_result, replies_result) = tokio::join!(
                fetch_parents_by_ids(parent_ids),
                fetch_replies(event_id)
            );

            // Process parents
            if let Ok(mut parents) = parents_result {
                parents.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                parent_events.set(parents);
            }

            // Process replies
            if let Ok(mut reply_vec) = replies_result {
                reply_vec.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                let count = reply_vec.len();
                replies.set(reply_vec);
                log::info!("Loaded {} replies", count);
            }

            // Prefetch author metadata for all loaded events
            use crate::utils::profile_prefetch;
            let mut all_events = Vec::new();
            if let Some(note) = note_data.read().as_ref() {
                all_events.push(note.clone());
            }
            all_events.extend(parent_events.read().iter().cloned());
            all_events.extend(replies.read().iter().cloned());

            if !all_events.is_empty() {
                spawn(async move {
                    profile_prefetch::prefetch_event_authors(&all_events).await;
                });
            }

            loading.set(false);
            loading_parents.set(false);
            loading_replies.set(false);
        });
    }));

    rsx! {
        div {
            class: "min-h-screen",

            // Sticky header with back button
            // Determine back route based on whether this is a voice message
            // Prefer the from_voice prop for immediate correct display, then update from loaded data
            {
                let data_is_voice = note_data.read().as_ref().map(|e| is_voice_message(e));
                let is_voice_note = data_is_voice.unwrap_or(initial_is_voice);
                let back_route = if is_voice_note { Route::VoiceMessages {} } else { Route::Home {} };
                let title = if is_voice_note { "Voice Message" } else { "Post" };

                rsx! {
                    div {
                        class: "sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border",
                        div {
                            class: "flex items-center gap-4 p-4",
                            Link {
                                to: back_route,
                                class: "hover:bg-accent rounded-full p-2 transition",
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "20",
                                    height: "20",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "m15 18-6-6 6-6" }
                                }
                            }
                            h1 {
                                class: "text-xl font-bold",
                                "{title}"
                            }
                        }
                    }
                }
            }

            if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && note_data.read().is_none()) {
                // Show client initializing animation during:
                // 1. Client initialization
                // 2. Initial note load (loading + no note, regardless of error state)
                ClientInitializing {}
            } else if let Some(err) = error.read().as_ref() {
                div {
                    class: "p-6",
                    div {
                        class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg border border-red-200 dark:border-red-800",
                        "{err}"
                    }
                }
            } else if let Some(event) = note_data.read().as_ref() {
                // Parent posts (thread context)
                if !parent_events.read().is_empty() {
                    div {
                        class: "border-b-2 border-blue-500/20",
                        for parent in parent_events.read().iter() {
                            div {
                                class: "relative",
                                // Render VoiceMessageCard for voice messages, NoteCard otherwise
                                if is_voice_message(parent) {
                                    VoiceMessageCard {
                                        event: parent.clone()
                                    }
                                } else {
                                    NoteCard {
                                        event: parent.clone(),
                                        collapsible: true
                                    }
                                }
                                // Thread line indicator
                                div {
                                    class: "absolute left-[40px] top-[60px] bottom-0 w-0.5 bg-border"
                                }
                            }
                        }
                    }
                }

                // Main post being viewed - use VoiceMessageCard for voice messages
                if is_voice_message(event) {
                    VoiceMessageCard {
                        event: event.clone()
                    }
                } else {
                    NoteCard {
                        event: event.clone(),
                        collapsible: false
                    }
                }

                div {
                    class: "border-b border-border"
                }

                // Reply Composer (TODO: Add inline reply composer)
                // div {
                //     class: "border-b border-border p-4",
                //     // ReplyComposer inline variant needed here
                // }

                // Replies (Threaded)
                if *loading_replies.read() {
                    div {
                        class: "flex items-center justify-center py-10",
                        div {
                            class: "text-center",
                            div {
                                class: "animate-spin text-4xl mb-2",
                                "⚡"
                            }
                            p {
                                class: "text-muted-foreground",
                                "Loading replies..."
                            }
                        }
                    }
                } else {
                    // Only build thread tree after loading completes to avoid caching empty results
                    {
                        let reply_vec = replies.read().clone();
                        let confirmed_tree = build_thread_tree(reply_vec, &event.id);
                        // Merge pending comments for optimistic display
                        let pending = get_pending_comments(&event.id);
                        let thread_tree = merge_pending_into_tree(confirmed_tree, pending, &event.id);

                        rsx! {
                            if thread_tree.is_empty() {
                                div {
                                    class: "flex flex-col items-center justify-center py-10 px-4 text-center text-muted-foreground",
                                    p { "No replies yet" }
                                    p {
                                        class: "text-sm",
                                        "Be the first to reply!"
                                    }
                                }
                            } else {
                                div {
                                    class: "divide-y divide-border",
                                    for node in thread_tree {
                                        ThreadedComment {
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
