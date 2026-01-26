use std::collections::HashSet;

use dioxus::prelude::*;
use crate::services::trending::{get_trending_notes, TrendingNote};
use crate::components::{NoteCard, NoteCardSkeleton};
use crate::stores::nostr_client;
use nostr_sdk::{Event as NostrEvent, EventId, PublicKey, Timestamp, Kind, Tag};
use nostr::secp256k1::schnorr::Signature;

#[component]
pub fn Trending() -> Element {
    // State for feed events
    let mut trending_notes = use_signal(Vec::<TrendingNote>::new);
    let mut events = use_signal(Vec::<NostrEvent>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);

    // Cached mute/block lists for N+1 optimization
    let mut cached_muted_posts: Signal<Option<HashSet<String>>> = use_signal(|| None);
    let mut cached_blocked_users: Signal<Option<HashSet<String>>> = use_signal(|| None);

    // Fetch mute/block lists once when client is initialized (N+1 optimization)
    // Single fetch for both muted posts and blocked users
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        // Only fetch if not already loaded
        if cached_muted_posts.peek().is_some() && cached_blocked_users.peek().is_some() {
            return;
        }

        spawn(async move {
            // Single fetch for both - avoids double fetch_mute_list() call
            let data = nostr_client::get_mute_list_data().await.unwrap_or_default();
            cached_muted_posts.set(Some(data.muted_posts));
            cached_blocked_users.set(Some(data.blocked_users));
        });
    });

    // Load trending feed - wait for client initialization
    use_effect(move || {
        let _ = refresh_trigger.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Wait for client to initialize before fetching
        if !client_initialized {
            return;
        }

        loading.set(true);
        error.set(None);

        spawn(async move {
            match get_trending_notes(Some(100)).await {
                Ok(notes) => {
                    // Convert TrendingNote to nostr_sdk::Event
                    let mut converted_events = Vec::new();
                    for note in &notes {
                        if let Ok(event) = convert_trending_to_event(note) {
                            converted_events.push(event);
                        }
                    }

                    trending_notes.set(notes);
                    events.set(converted_events);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(format!("Nostr.band API currently down: {}", e)));
                    loading.set(false);
                }
            }
        });
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center justify-between",
                    h2 {
                        class: "text-xl font-bold flex items-center gap-2",
                        span { "📈" }
                        "Trending"
                    }
                    button {
                        class: "p-2 hover:bg-accent rounded-full transition disabled:opacity-50",
                        disabled: *loading.read(),
                        onclick: move |_| {
                            let current = *refresh_trigger.read();
                            refresh_trigger.set(current + 1);
                        },
                        title: "Refresh feed",
                        if *loading.read() && events.read().is_empty() {
                            span {
                                class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin"
                            }
                        } else {
                            "🔄"
                        }
                    }
                }
                div {
                    class: "px-4 pb-3",
                    p {
                        class: "text-sm text-muted-foreground",
                        "Top trending posts from Nostr.Band"
                    }
                }
            }

            // Error state
            if let Some(err) = error.read().as_ref() {
                div {
                    class: "p-4",
                    div {
                        class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                        "❌ {err}"
                    }
                }
            }

            // Loading state (initial)
            if *loading.read() && events.read().is_empty() {
                div {
                    class: "divide-y divide-border",
                    for _ in 0..5 {
                        NoteCardSkeleton {}
                    }
                }
            }

            // Events feed
            if !events.read().is_empty() {
                div {
                    class: "divide-y divide-border",
                    for event in events.read().iter() {
                        NoteCard {
                            key: "{event.id}",
                            event: event.clone(),
                            collapsible: true,
                            cached_muted_posts: cached_muted_posts.read().clone(),
                            cached_blocked_users: cached_blocked_users.read().clone()
                        }
                    }
                }
            }

            // Empty state (no error, not loading, no events)
            if !*loading.read() && events.read().is_empty() && error.read().is_none() {
                div {
                    class: "text-center py-12",
                    div {
                        class: "text-6xl mb-4",
                        "📈"
                    }
                    h3 {
                        class: "text-xl font-semibold mb-2",
                        "No trending posts"
                    }
                    p {
                        class: "text-muted-foreground",
                        "Check back later for trending content"
                    }
                }
            }
        }
    }
}

/// Convert a TrendingNote to a nostr_sdk::Event
fn convert_trending_to_event(note: &TrendingNote) -> Result<NostrEvent, String> {
    let event_id = EventId::from_hex(&note.event.id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;

    let pubkey = PublicKey::from_hex(&note.event.pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let created_at = Timestamp::from(note.event.created_at);

    let kind = Kind::from(note.event.kind);

    // Convert tags
    let tags: Vec<Tag> = note.event.tags
        .iter()
        .filter_map(|tag_vec| {
            if tag_vec.is_empty() {
                return None;
            }
            Tag::parse(tag_vec.iter().map(|s| s.as_str())).ok()
        })
        .collect();

    // Decode signature from hex
    let sig_bytes = hex::decode(&note.event.sig)
        .map_err(|e| format!("Invalid signature hex: {}", e))?;
    let sig = Signature::from_slice(&sig_bytes)
        .map_err(|e| format!("Invalid signature: {}", e))?;

    // Build the event
    Ok(NostrEvent::new(
        event_id,
        pubkey,
        created_at,
        kind,
        tags,
        note.event.content.clone(),
        sig,
    ))
}
