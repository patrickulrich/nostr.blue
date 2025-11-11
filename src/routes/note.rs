use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::routes::Route;
use crate::components::{NoteCard, ThreadedComment, ClientInitializing};
use crate::utils::build_thread_tree;
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;
use std::time::Duration;

// Helper functions for parallel loading

async fn fetch_main_note(event_id: EventId) -> Result<NostrEvent, String> {
    let filter = Filter::new().id(event_id);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;
    events.into_iter().next().ok_or("Event not found".to_string())
}

async fn fetch_parent_notes(event_id: EventId) -> Result<Vec<NostrEvent>, String> {
    // First get the main note to extract parent IDs
    let main_note = fetch_main_note(event_id).await?;

    let parent_ids: Vec<EventId> = main_note.tags.iter()
        .filter_map(|tag| {
            if tag.kind() == nostr_sdk::TagKind::SingleLetter(
                nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::E)
            ) {
                if let Some(content) = tag.content() {
                    let parts: Vec<&str> = content.split('\t').collect();
                    EventId::from_hex(parts[0]).ok()
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    if parent_ids.is_empty() {
        return Ok(Vec::new());
    }

    let filter = Filter::new()
        .ids(parent_ids)
        .kind(Kind::TextNote);

    nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await
}

async fn fetch_replies(event_id: EventId) -> Result<Vec<NostrEvent>, String> {
    let filter = Filter::new()
        .kind(Kind::TextNote)
        .event(event_id)
        .limit(100);

    nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await
}

#[component]
pub fn Note(note_id: String) -> Element {
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

            // PARALLEL FETCHES - All three at once!
            let (note_result, parents_result, replies_result) = tokio::join!(
                fetch_main_note(event_id),
                fetch_parent_notes(event_id),
                fetch_replies(event_id)
            );

            // Process main note
            match note_result {
                Ok(event) => {
                    note_data.set(Some(event));
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }

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
            div {
                class: "sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "flex items-center gap-4 p-4",
                    Link {
                        to: Route::Home {},
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
                        "Post"
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
                                NoteCard {
                                    event: parent.clone()
                                }
                                // Thread line indicator
                                div {
                                    class: "absolute left-[40px] top-[60px] bottom-0 w-0.5 bg-border"
                                }
                            }
                        }
                    }
                }

                // Main post being viewed
                NoteCard {
                    event: event.clone()
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
                {
                    let reply_vec = replies.read().clone();
                    let thread_tree = build_thread_tree(reply_vec, &event.id);

                    rsx! {
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
                        } else if thread_tree.is_empty() {
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
