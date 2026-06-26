use crate::components::{NoteCard, NoteCardSkeleton};
use crate::hooks::use_mute_block_cache;
use crate::hooks::{use_nostr_resource_public, NostrResourceState};
use crate::services::search::sidebar_discovery::{self, HotPostItem, HotPostSource, NostrarchivesNote};
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
    let (cached_muted_posts, cached_blocked_users, cached_muted_words) = use_mute_block_cache();

    let mut events_resource = use_nostr_resource_public(move || async move {
        load_trending_events(source).await
    });
    let events = events_resource.state();

    let subtitle = match source {
        HotPostSource::NostrWine => "Top trending posts from nostr.wine",
        HotPostSource::Ditto => "Top hot posts from Ditto",
        HotPostSource::Nostrarchives => "Top trending posts from Nostrarchives",
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
                        disabled: events_resource.is_loading(),
                        onclick: move |_| {
                            events_resource.restart();
                        },
                        title: "Refresh feed",
                        if events_resource.is_loading() {
                            span { class: "inline-block h-5 w-5 animate-spin rounded-full border-2 border-current border-t-transparent" }
                        } else {
                            "🔄"
                        }
                    }
                }
            }

            match &*events.read() {
                NostrResourceState::Error(e) => rsx! {
                    div { class: "p-4",
                        div { class: "rounded-lg bg-red-100 p-4 text-red-800 dark:bg-red-900 dark:text-red-200",
                            "❌ {e}"
                        }
                    }
                },
                NostrResourceState::Loading | NostrResourceState::Initializing => rsx! {
                    div { class: "divide-y divide-border",
                        for _ in 0..5 {
                            NoteCardSkeleton {}
                        }
                    }
                },
                NostrResourceState::Loaded(data) if !data.is_empty() => rsx! {
                    div { class: "divide-y divide-border",
                        for event in data.iter() {
                            NoteCard {
                                key: "{event.id}",
                                event: event.clone(),
                                collapsible: true,
                                cached_muted_posts: cached_muted_posts.read().clone(),
                                cached_blocked_users: cached_blocked_users.read().clone(),
                                cached_muted_words: cached_muted_words.read().clone(),
                            }
                        }
                    }
                },
                NostrResourceState::Loaded(_) => rsx! {
                    div { class: "py-12 text-center",
                        div { class: "mb-4 text-6xl", "📈" }
                        h3 { class: "mb-2 text-xl font-semibold", "No trending posts" }
                        p { class: "text-muted-foreground", "Check back later for trending content" }
                    }
                },
                NostrResourceState::AuthRequired => rsx! {},
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
            HotPostItem::Nostrarchives(note) => convert_nostrarchives_to_event(&note),
        })
        .collect()
}

async fn filter_hot_posts(items: Vec<HotPostItem>) -> Vec<HotPostItem> {
    let mute_data = nostr_client::get_mute_list_data().await.unwrap_or_default();
    items
        .into_iter()
        .filter(|item| {
            let (pubkey, event_id) = match item {
                HotPostItem::NostrWine(note) => (note.event.pubkey.clone(), note.event.id.clone()),
                HotPostItem::Ditto(event) => (event.pubkey.to_hex(), event.id.to_hex()),
                HotPostItem::Nostrarchives(note) => (note.pubkey.clone(), note.id.clone()),
            };
            if mute_data.blocked_users.contains(&pubkey)
                || mute_data.muted_posts.contains(&event_id)
            {
                return false;
            }
            if !mute_data.muted_words.is_empty() {
                let (content, hashtags) = match item {
                    HotPostItem::NostrWine(note) => (
                        note.event.content.clone(),
                        note.event.tags.iter()
                            .filter(|t| t.len() > 1 && t[0] == "t")
                            .filter_map(|t| t.get(1).cloned())
                            .collect::<Vec<String>>(),
                    ),
                    HotPostItem::Ditto(event) => (
                        event.content.clone(),
                        event.tags.iter()
                            .filter(|tag| tag.kind() == nostr_sdk::prelude::TagKind::t())
                            .filter_map(|tag| tag.content().map(|s| s.to_string()))
                            .collect::<Vec<String>>(),
                    ),
                    HotPostItem::Nostrarchives(note) => (
                        note.content.clone(),
                        note.tags.iter()
                            .filter(|t| t.len() > 1 && t[0] == "t")
                            .filter_map(|t| t.get(1).cloned())
                            .collect::<Vec<String>>(),
                    ),
                };
                if crate::utils::content_filter::contains_muted_word(
                    &content,
                    &hashtags,
                    &mute_data.muted_words,
                ) {
                    return false;
                }
            }
            true
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

fn convert_nostrarchives_to_event(note: &NostrarchivesNote) -> Result<NostrEvent, String> {
    let event_id =
        EventId::from_hex(&note.id).map_err(|e| format!("Invalid event ID: {}", e))?;
    let pubkey =
        PublicKey::from_hex(&note.pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let created_at = Timestamp::from(note.created_at as u64);
    let kind = Kind::from(note.kind as u16);
    let tags: Vec<Tag> = note
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
        hex::decode(&note.sig).map_err(|e| format!("Invalid signature hex: {}", e))?;
    let sig = Signature::from_slice(&sig_bytes).map_err(|e| format!("Invalid signature: {}", e))?;
    Ok(NostrEvent::new(
        event_id,
        pubkey,
        created_at,
        kind,
        tags,
        note.content.clone(),
        sig,
    ))
}
