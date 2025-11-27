use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::routes::Route;
use crate::components::{NoteCard, ThreadedComment, ClientInitializing};
use crate::utils::build_thread_tree;
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;
use std::time::Duration;

// Helper functions for two-phase loading (DB first, then relay)

/// Phase 1: Load from database (instant)
async fn fetch_note_from_db(event_id: EventId) -> Option<NostrEvent> {
    let client = nostr_client::get_client()?;
    let filter = Filter::new().id(event_id);

    if let Ok(events) = client.database().query(filter).await {
        events.into_iter().next()
    } else {
        None
    }
}

/// Phase 2: Fetch from relays (slower but fresh)
async fn fetch_note_from_relay(event_id: EventId) -> std::result::Result<Option<NostrEvent>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    nostr_client::ensure_relays_ready(&client).await;

    let filter = Filter::new().id(event_id);
    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => Ok(events.into_iter().next()),
        Err(e) => Err(format!("Failed to fetch note: {}", e))
    }
}

/// Fetch parent notes given the main note's tags
/// Returns events sorted chronologically (oldest first) to show the thread context
async fn fetch_parent_notes_from_tags(tags: &nostr_sdk::Tags) -> Vec<NostrEvent> {
    let mut root_id: Option<EventId> = None;
    let mut reply_id: Option<EventId> = None;
    let mut all_e_ids: Vec<EventId> = Vec::new();

    // Parse e tags according to NIP-10
    for tag in tags.iter() {
        let slice = tag.as_slice();
        if slice.first().map(|s| s.as_str()) != Some("e") {
            continue;
        }

        // Get the event ID (second element)
        let event_id = match slice.get(1).and_then(|id| EventId::from_hex(id).ok()) {
            Some(id) => id,
            None => continue,
        };

        // Check for marker (fourth element, if present)
        let marker = slice.get(3).map(|s| s.as_str());

        match marker {
            Some("root") => root_id = Some(event_id),
            Some("reply") => reply_id = Some(event_id),
            _ => {
                // Collect all e-tagged events for positional fallback
                all_e_ids.push(event_id);
            }
        }
    }

    // Build the list of parent IDs to fetch
    let mut parent_ids: Vec<EventId> = Vec::new();

    // If we have marked tags, use them
    if root_id.is_some() || reply_id.is_some() {
        if let Some(root) = root_id {
            parent_ids.push(root);
        }
        if let Some(reply) = reply_id {
            // Only add reply if it's different from root
            if Some(reply) != root_id {
                parent_ids.push(reply);
            }
        }
    } else {
        // Fallback to positional parsing (deprecated but still in use)
        // First e tag is root, last e tag is reply
        if all_e_ids.len() == 1 {
            parent_ids.push(all_e_ids[0]);
        } else if all_e_ids.len() >= 2 {
            parent_ids.push(all_e_ids[0]); // root
            let last = all_e_ids[all_e_ids.len() - 1];
            if last != all_e_ids[0] {
                parent_ids.push(last); // reply (direct parent)
            }
        }
    }

    if parent_ids.is_empty() {
        return Vec::new();
    }

    let filter = Filter::new()
        .ids(parent_ids)
        .kind(Kind::TextNote);

    nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await
        .unwrap_or_default()
}

/// Fetch replies from DB first, then relay
async fn fetch_replies_db(event_id: EventId) -> Vec<NostrEvent> {
    let client = match nostr_client::get_client() {
        Some(c) => c,
        None => return Vec::new(),
    };

    let filter = Filter::new()
        .kind(Kind::TextNote)
        .event(event_id)
        .limit(100);

    match client.database().query(filter).await {
        Ok(events) => events.into_iter().collect(),
        Err(e) => {
            log::error!("Failed to fetch replies from DB for event {}: {}", event_id, e);
            Vec::new()
        }
    }
}

async fn fetch_replies_relay(event_id: EventId) -> Vec<NostrEvent> {
    let client = match nostr_client::get_client() {
        Some(c) => c,
        None => return Vec::new(),
    };

    nostr_client::ensure_relays_ready(&client).await;

    let filter = Filter::new()
        .kind(Kind::TextNote)
        .event(event_id)
        .limit(100);

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => events.into_iter().collect(),
        Err(e) => {
            log::error!("Failed to fetch replies from relay for event {}: {}", event_id, e);
            Vec::new()
        }
    }
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

    // TWO-PHASE LOADING - DB first (instant), then relay (background)
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

            // PHASE 1: Load from DB (instant)
            let (db_note, db_replies) = tokio::join!(
                fetch_note_from_db(event_id),
                fetch_replies_db(event_id)
            );

            // Show DB results immediately
            if let Some(note) = db_note.clone() {
                note_data.set(Some(note.clone()));
                loading.set(false);

                // Fetch parents based on note tags
                let parents = fetch_parent_notes_from_tags(&note.tags).await;
                if !parents.is_empty() {
                    let mut sorted_parents = parents;
                    sorted_parents.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                    parent_events.set(sorted_parents);
                }
                loading_parents.set(false);
            }

            if !db_replies.is_empty() {
                let mut sorted_replies = db_replies;
                sorted_replies.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                log::info!("Phase 1 (DB): Loaded {} replies", sorted_replies.len());
                replies.set(sorted_replies);
                loading_replies.set(false);
            }

            // PHASE 2: Fetch from relays (background, merge new data)
            let (relay_note, relay_replies) = tokio::join!(
                fetch_note_from_relay(event_id),
                async { fetch_replies_relay(event_id).await }
            );

            // Merge relay note (if not found in DB)
            match relay_note {
                Ok(Some(note)) => {
                    if note_data.read().is_none() {
                        note_data.set(Some(note.clone()));

                        // Fetch parents if we didn't have the note before
                        let parents = fetch_parent_notes_from_tags(&note.tags).await;
                        if !parents.is_empty() {
                            let mut sorted_parents = parents;
                            sorted_parents.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                            parent_events.set(sorted_parents);
                        }
                    }
                }
                Ok(None) => {
                    if note_data.read().is_none() {
                        error.set(Some("Event not found".to_string()));
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch note from relay: {}", e);
                    if note_data.read().is_none() {
                        error.set(Some("Event not found".to_string()));
                    }
                }
            }
            loading.set(false);
            loading_parents.set(false);

            // Merge relay replies (deduplicate)
            if !relay_replies.is_empty() {
                let current_replies = replies.read().clone();
                let existing_ids: std::collections::HashSet<_> = current_replies.iter()
                    .map(|e| e.id)
                    .collect();

                let new_replies: Vec<_> = relay_replies.into_iter()
                    .filter(|e| !existing_ids.contains(&e.id))
                    .collect();

                if !new_replies.is_empty() {
                    let mut all_replies = current_replies;
                    all_replies.extend(new_replies);
                    all_replies.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                    log::info!("Phase 2 (Relay): Total {} replies after merge", all_replies.len());
                    replies.set(all_replies);
                }
            }
            loading_replies.set(false);

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
                                    event: parent.clone(),
                                    collapsible: true
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
                    event: event.clone(),
                    collapsible: false
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
