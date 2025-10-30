use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::routes::Route;
use crate::components::{NoteCard, ThreadedComment};
use crate::utils::build_thread_tree;
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;

#[component]
pub fn Note(note_id: String) -> Element {
    let note_id_for_replies = note_id.clone();
    let note_id_for_parents = note_id.clone();
    let mut note_data = use_signal(|| None::<NostrEvent>);
    let mut parent_events = use_signal(|| Vec::<NostrEvent>::new());
    let mut replies = use_signal(|| Vec::<NostrEvent>::new());
    let mut loading = use_signal(|| true);
    let mut loading_parents = use_signal(|| false);
    let mut loading_replies = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    // Fetch note on mount and when note_id changes
    use_effect(use_reactive!(|note_id| {
        let note_id_str = note_id.clone();
        spawn(async move {
            loading.set(true);
            error.set(None);

            // Parse the note ID (can be note1... bech32 or hex)
            let event_id = match EventId::from_bech32(&note_id_str)
                .or_else(|_| EventId::from_hex(&note_id_str)) {
                Ok(id) => id,
                Err(e) => {
                    error.set(Some(format!("Invalid note ID: {}", e)));
                    loading.set(false);
                    return;
                }
            };

            // Get the client
            let client = match nostr_client::NOSTR_CLIENT.read().as_ref() {
                Some(c) => c.clone(),
                None => {
                    error.set(Some("Nostr client not initialized".to_string()));
                    loading.set(false);
                    return;
                }
            };

            // Create filter for this specific event
            let filter = Filter::new().id(event_id);

            // Query relays
            match client.fetch_events(filter, std::time::Duration::from_secs(10)).await {
                Ok(events) => {
                    if let Some(event) = events.into_iter().next() {
                        note_data.set(Some(event.clone()));
                    } else {
                        error.set(Some("Note not found".to_string()));
                    }
                }
                Err(e) => {
                    error.set(Some(format!("Failed to fetch note: {}", e)));
                }
            }

            loading.set(false);
        });
    }));

    // Fetch parent events for thread context
    use_effect(use_reactive!(|note_id_for_parents| {
        let note_id_str = note_id_for_parents.clone();
        spawn(async move {
            loading_parents.set(true);

            // Parse the note ID
            let event_id = match EventId::from_bech32(&note_id_str)
                .or_else(|_| EventId::from_hex(&note_id_str)) {
                Ok(id) => id,
                Err(_) => {
                    loading_parents.set(false);
                    return;
                }
            };

            // Get the client
            let client = match nostr_client::NOSTR_CLIENT.read().as_ref() {
                Some(c) => c.clone(),
                None => {
                    loading_parents.set(false);
                    return;
                }
            };

            // First get the main event to extract parent IDs
            let filter = Filter::new().id(event_id);
            if let Ok(events) = client.fetch_events(filter, std::time::Duration::from_secs(10)).await {
                if let Some(event) = events.into_iter().next() {
                    // Extract parent event IDs from 'e' tags
                    let parent_ids: Vec<EventId> = event.tags.iter()
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

                    if !parent_ids.is_empty() {
                        // Fetch parent events
                        let parent_filter = Filter::new()
                            .ids(parent_ids)
                            .kind(Kind::TextNote);

                        if let Ok(parent_evts) = client.fetch_events(
                            parent_filter,
                            std::time::Duration::from_secs(10)
                        ).await {
                            let mut parents: Vec<NostrEvent> = parent_evts.into_iter().collect();
                            // Sort by created_at ascending (oldest first)
                            parents.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                            parent_events.set(parents);
                        }
                    }
                }
            }

            loading_parents.set(false);
        });
    }));

    // Fetch replies to this note
    use_effect(use_reactive!(|note_id_for_replies| {
        let note_id_str = note_id_for_replies.clone();
        spawn(async move {
            loading_replies.set(true);

            // Parse the note ID
            let event_id = match EventId::from_bech32(&note_id_str)
                .or_else(|_| EventId::from_hex(&note_id_str)) {
                Ok(id) => id,
                Err(_) => {
                    loading_replies.set(false);
                    return;
                }
            };

            // Get the client
            let client = match nostr_client::NOSTR_CLIENT.read().as_ref() {
                Some(c) => c.clone(),
                None => {
                    loading_replies.set(false);
                    return;
                }
            };

            // Create filter for replies (events with 'e' tag pointing to this event)
            let filter = Filter::new()
                .kind(Kind::TextNote)
                .event(event_id)
                .limit(100);

            // Query relays
            match client.fetch_events(filter, std::time::Duration::from_secs(10)).await {
                Ok(events) => {
                    let mut reply_vec: Vec<NostrEvent> = events.into_iter().collect();
                    reply_vec.sort_by(|a, b| a.created_at.cmp(&b.created_at)); // Chronological order
                    replies.set(reply_vec);
                    log::info!("Loaded {} replies", replies.read().len());
                }
                Err(e) => {
                    log::error!("Failed to fetch replies: {}", e);
                }
            }

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

            if *loading.read() {
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
                            "Loading note..."
                        }
                    }
                }
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
