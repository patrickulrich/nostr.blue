use crate::components::{NoteCard, NoteCardSkeleton};
use crate::hooks::use_mute_block_cache;
use crate::services::trending::{get_trending_notes, TrendingNote};
use crate::stores::nostr_client;
use dioxus::prelude::*;
use nostr::secp256k1::schnorr::Signature;
use nostr_sdk::{Event as NostrEvent, EventId, Kind, PublicKey, Tag, Timestamp};
#[component]
pub fn Trending() -> Element {
    let mut trending_notes = use_signal(Vec::<TrendingNote>::new);
    let mut events = use_signal(Vec::<NostrEvent>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);
    let (cached_muted_posts, cached_blocked_users) = use_mute_block_cache();
    use_effect(move || {
        let _ = refresh_trigger.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        loading.set(true);
        error.set(None);
        spawn(async move {
            match get_trending_notes(Some(100)).await {
                Ok(notes) => {
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
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center justify-between",
                    h2 { class: "text-xl font-bold flex items-center gap-2",
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
                            span { class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                        } else {
                            "🔄"
                        }
                    }
                }
                div { class: "px-4 pb-3",
                    p { class: "text-sm text-muted-foreground", "Top trending posts from Nostr.Band" }
                }
            }
            if let Some(err) = error.read().as_ref() {
                div { class: "p-4",
                    div { class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                        "❌ {err}"
                    }
                }
            }
            if *loading.read() && events.read().is_empty() {
                div { class: "divide-y divide-border",
                    for _ in 0..5 {
                        NoteCardSkeleton {}
                    }
                }
            }
            if !events.read().is_empty() {
                div { class: "divide-y divide-border",
                    for event in events.read().iter() {
                        NoteCard {
                            key: "{event.id}",
                            event: event.clone(),
                            collapsible: true,
                            cached_muted_posts: cached_muted_posts.read().clone(),
                            cached_blocked_users: cached_blocked_users.read().clone(),
                        }
                    }
                }
            }
            if !*loading.read() && events.read().is_empty() && error.read().is_none() {
                div { class: "text-center py-12",
                    div { class: "text-6xl mb-4", "📈" }
                    h3 { class: "text-xl font-semibold mb-2", "No trending posts" }
                    p { class: "text-muted-foreground", "Check back later for trending content" }
                }
            }
        }
    }
}
/// Convert a TrendingNote to a nostr_sdk::Event
fn convert_trending_to_event(note: &TrendingNote) -> Result<NostrEvent, String> {
    let event_id =
        EventId::from_hex(&note.event.id).map_err(|e| format!("Invalid event ID: {}", e))?;
    let pubkey =
        PublicKey::from_hex(&note.event.pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let created_at = Timestamp::from(note.event.created_at);
    let kind = Kind::from(note.event.kind);
    let tags: Vec<Tag> = note
        .event
        .tags
        .iter()
        .filter_map(|tag_vec| {
            if tag_vec.is_empty() {
                return None;
            }
            Tag::parse(tag_vec.iter().map(|s| s.as_str())).ok()
        })
        .collect();
    let sig_bytes =
        hex::decode(&note.event.sig).map_err(|e| format!("Invalid signature hex: {}", e))?;
    let sig = Signature::from_slice(&sig_bytes).map_err(|e| format!("Invalid signature: {}", e))?;
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
