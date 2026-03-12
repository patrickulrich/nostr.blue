use crate::components::{NoteCard, NoteCardSkeleton};
use crate::hooks::use_mute_block_cache;
use crate::services::search::sidebar_discovery::{self, HotPostItem, HotPostSource};
use crate::services::trending::TrendingNote;
use crate::stores::nostr_client;
use dioxus::prelude::*;
use nostr::secp256k1::schnorr::Signature;
use nostr_sdk::{Event as NostrEvent, EventId, Kind, PublicKey, Tag, Timestamp};

#[component]
pub fn Trending(source: Option<String>) -> Element {
    let source = source
        .as_deref()
        .and_then(HotPostSource::from_query)
        .unwrap_or(HotPostSource::NostrWine);
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
            match load_trending_events(source).await {
                Ok(fetched_events) => {
                    events.set(fetched_events);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    let subtitle = match source {
        HotPostSource::NostrWine => "Top trending posts from nostr.wine",
        HotPostSource::Ditto => "Top hot posts from Ditto",
    };

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 border-b border-border bg-background/80 backdrop-blur-sm",
                div { class: "flex items-center justify-between px-4 py-3",
                    div {
                        h2 { class: "flex items-center gap-2 text-xl font-bold",
                            span { "📈" }
                            "Trending"
                        }
                        p { class: "text-sm text-muted-foreground", "{subtitle}" }
                    }
                    button {
                        class: "rounded-full p-2 transition hover:bg-accent disabled:opacity-50",
                        disabled: *loading.read(),
                        onclick: move |_| {
                            let current = *refresh_trigger.read();
                            refresh_trigger.set(current + 1);
                        },
                        title: "Refresh feed",
                        if *loading.read() && events.read().is_empty() {
                            span { class: "inline-block h-5 w-5 animate-spin rounded-full border-2 border-current border-t-transparent" }
                        } else {
                            "🔄"
                        }
                    }
                }
            }

            if let Some(err) = error.read().as_ref() {
                div { class: "p-4",
                    div { class: "rounded-lg bg-red-100 p-4 text-red-800 dark:bg-red-900 dark:text-red-200",
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
                div { class: "py-12 text-center",
                    div { class: "mb-4 text-6xl", "📈" }
                    h3 { class: "mb-2 text-xl font-semibold", "No trending posts" }
                    p { class: "text-muted-foreground", "Check back later for trending content" }
                }
            }
        }
    }
}

async fn load_trending_events(source: HotPostSource) -> Result<Vec<NostrEvent>, String> {
    let items = sidebar_discovery::get_hot_posts(source, 100).await?;
    let filtered = filter_hot_posts(items).await;
    filtered
        .into_iter()
        .map(|item| match item {
            HotPostItem::NostrWine(note) => convert_trending_to_event(&note),
            HotPostItem::Ditto(event) => Ok(event),
        })
        .collect()
}

async fn filter_hot_posts(items: Vec<HotPostItem>) -> Vec<HotPostItem> {
    let mute_data = nostr_client::get_mute_list_data().await.unwrap_or_default();
    items
        .into_iter()
        .filter(|item| match item {
            HotPostItem::NostrWine(note) => {
                !mute_data.blocked_users.contains(&note.event.pubkey)
                    && !mute_data.muted_posts.contains(&note.event.id)
            }
            HotPostItem::Ditto(event) => {
                let event_id = event.id.to_hex();
                let pubkey = event.pubkey.to_hex();
                !mute_data.blocked_users.contains(&pubkey)
                    && !mute_data.muted_posts.contains(&event_id)
            }
        })
        .collect()
}

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
