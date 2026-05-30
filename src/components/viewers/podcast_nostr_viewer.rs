//! Nostr Podcast Detail Route
//!
//! Shows a native Nostr podcast with:
//! - Podcast metadata (cover, title, description)
//! - Episode list with infinite scroll
//! - V4V payment support
//! - Follow/subscribe functionality
use crate::components::{
    icons, ContentShareModal, ContentType, DisplayEpisode, PodcastEpisodeList,
};
use crate::hooks::use_infinite_scroll;
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client, podcast_subscription};
use crate::utils::pagination::{is_likely_future_secs, safe_cursor_from_timestamps};
use crate::utils::podcast::{self, PodcastMetadata};
use dioxus::prelude::*;
use nostr_sdk::prelude::{Filter, Kind, PublicKey, Timestamp};
use std::collections::HashSet;
use std::time::Duration;
#[derive(Props, Clone, PartialEq)]
pub struct PodcastNostrDetailProps {
    pub naddr: String,
}
/// Nostr podcast detail page
#[component]
pub fn PodcastNostrViewer(props: PodcastNostrDetailProps) -> Element {
    let naddr = props.naddr.clone();

    // State signals
    let mut metadata = use_signal(|| None::<(PodcastMetadata, String)>);
    let mut episodes = use_signal(Vec::<DisplayEpisode>::new);
    let mut loading = use_signal(|| true);
    let mut loading_more = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut error = use_signal(|| None::<String>);

    // Initial load - metadata then first batch of episodes
    use_effect(move || {
        let naddr = naddr.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        loading.set(true);
        spawn(async move {
            // First fetch metadata
            match fetch_nostr_podcast_metadata(&naddr).await {
                Ok((meta, pubkey)) => {
                    metadata.set(Some((meta.clone(), pubkey.clone())));

                    // Then load initial episodes
                    match fetch_nostr_episodes(&pubkey, &meta, 30, None).await {
                        Ok(eps) => {
                            log::info!("Loaded {} initial Nostr episodes", eps.len());
                            {
                                let ts: Vec<u64> = eps.iter().map(|e| e.created_at).collect();
                                oldest_timestamp.set(safe_cursor_from_timestamps(&ts));
                            }
                            has_more.set(eps.len() >= 30);
                            episodes.set(eps);
                        }
                        Err(e) => log::error!("Failed to load episodes: {}", e),
                    }
                }
                Err(e) => {
                    log::error!("Failed to load metadata: {}", e);
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    // Load more callback - only runs after initial load is complete
    let load_more = move || {
        // Don't load more until initial load is complete
        if *loading.read() || *loading_more.read() || !*has_more.read() {
            return;
        }
        let Some((ref meta, ref pubkey)) = *metadata.peek() else {
            return;
        };

        // Subtract 1 from timestamp to avoid duplicates (until filter is inclusive)
        let until = (*oldest_timestamp.peek()).map(|ts| ts.saturating_sub(1));
        let meta = meta.clone();
        let pubkey = pubkey.clone();

        loading_more.set(true);
        spawn(async move {
            log::info!("Loading more Nostr episodes, until: {:?}", until);
            match fetch_nostr_episodes(&pubkey, &meta, 30, until).await {
                Ok(new_eps) => {
                    log::info!("Loaded {} more Nostr episodes", new_eps.len());
                    // Deduplicate by checking existing episode IDs
                    let existing_ids: HashSet<_> =
                        episodes.peek().iter().map(|e| e.id.clone()).collect();
                    let unique: Vec<_> = new_eps
                        .into_iter()
                        .filter(|ep| !existing_ids.contains(&ep.id))
                        .collect();

                    if unique.is_empty() {
                        has_more.set(false);
                    } else {
                        {
                            let ts: Vec<u64> = unique.iter().map(|e| e.created_at).collect();
                            oldest_timestamp.set(safe_cursor_from_timestamps(&ts));
                        }
                        has_more.set(unique.len() >= 20);
                        episodes.write().extend(unique);
                    }
                }
                Err(e) => {
                    has_more.set(false);
                    log::error!("Load more failed: {}", e);
                }
            }
            loading_more.set(false);
        });
    };

    let sentinel_id = use_infinite_scroll(load_more, has_more, loading_more);

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-4",
                    Link {
                        to: Route::PodcastHome {},
                        class: "p-2 hover:bg-muted rounded-full transition",
                        dangerous_inner_html: icons::ARROW_LEFT,
                    }
                    h1 { class: "text-xl font-bold", "Podcast" }
                }
            }
            if !*nostr_client::CLIENT_INITIALIZED.read() || *loading.read() {
                PodcastDetailSkeleton {}
            } else if let Some(ref e) = *error.read() {
                div { class: "p-4 text-center",
                    div { class: "text-destructive mb-2", "Failed to load podcast" }
                    div { class: "text-sm text-muted-foreground", "{e}" }
                }
            } else if let Some((ref meta, _)) = *metadata.read() {
                PodcastDetailContent {
                    metadata: meta.clone(),
                    episodes: episodes.read().clone(),
                    has_more: *has_more.read(),
                    loading_more: *loading_more.read(),
                    sentinel_id: sentinel_id.clone(),
                }
            } else {
                PodcastDetailSkeleton {}
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
struct PodcastDetailContentProps {
    metadata: PodcastMetadata,
    episodes: Vec<DisplayEpisode>,
    has_more: bool,
    loading_more: bool,
    sentinel_id: String,
}
#[component]
fn PodcastDetailContent(props: PodcastDetailContentProps) -> Element {
    let metadata = &props.metadata;
    let auth = auth_store::AUTH_STATE.read();
    let mut show_share_modal = use_signal(|| false);
    let image_url = metadata.image.clone().unwrap_or_else(|| {
        format!(
            "https://api.dicebear.com/7.x/shapes/svg?seed={}",
            metadata.title
        )
    });
    let has_v4v = metadata.value.is_some();
    let coordinate = format!("30078:{}:{}", metadata.pubkey, metadata.d_tag);
    let coordinate_for_memo = coordinate.clone();
    let is_subscribed = use_memo(move || podcast_subscription::is_subscribed(&coordinate_for_memo));
    let mut subscribing = use_signal(|| false);
    let episode_count = props.episodes.len();
    rsx! {
        div {
            div { class: "relative",
                div { class: "absolute inset-0 h-48 bg-gradient-to-b from-primary/20 to-background" }
                div { class: "relative p-6",
                    div { class: "flex gap-6",
                        img {
                            src: "{image_url}",
                            alt: "{metadata.title}",
                            class: "w-32 h-32 md:w-40 md:h-40 rounded-lg object-cover shadow-lg",
                            referrerpolicy: "no-referrer",
                        }
                        div { class: "flex-1 min-w-0",
                            h1 { class: "text-2xl font-bold truncate", "{metadata.title}" }
                            if let Some(ref author) = metadata.author {
                                p { class: "text-muted-foreground mt-1", "{author}" }
                            }
                            div { class: "flex items-center gap-2 mt-2 flex-wrap",
                                span { class: "px-2 py-1 text-xs bg-purple-500/20 text-purple-400 rounded-full font-medium",
                                    "Nostr"
                                }
                                if has_v4v {
                                    span {
                                        class: "px-2 py-1 text-xs bg-amber-500/20 text-amber-400 rounded-full font-medium flex items-center gap-1",
                                        dangerous_inner_html: icons::ZAP,
                                        "V4V Enabled"
                                    }
                                }
                                if metadata.explicit {
                                    span { class: "px-2 py-1 text-xs bg-red-500/20 text-red-400 rounded-full font-medium",
                                        "Explicit"
                                    }
                                }
                            }
                            if !metadata.categories.is_empty() {
                                div { class: "flex items-center gap-1 mt-2 flex-wrap",
                                    for cat in metadata.categories.iter().take(4) {
                                        span {
                                            key: "{cat}",
                                            class: "px-2 py-0.5 text-xs bg-muted text-muted-foreground rounded",
                                            "{cat}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex items-center gap-3 mt-4",
                        if auth.is_authenticated {
                            button {
                                class: if *is_subscribed.read() || *subscribing.read() { "px-4 py-2 text-sm font-medium border border-border rounded-full hover:bg-muted transition" } else { "px-4 py-2 text-sm font-medium bg-primary text-primary-foreground rounded-full hover:bg-primary/90 transition" },
                                disabled: *subscribing.read(),
                                onclick: {
                                    let coord = coordinate.clone();
                                    move |_| {
                                        if *subscribing.read() {
                                            return;
                                        }
                                        let coord = coord.clone();
                                        let currently_subscribed = podcast_subscription::is_subscribed(&coord);
                                        subscribing.set(true);
                                        spawn(async move {
                                            if currently_subscribed {
                                                match podcast_subscription::remove_subscription(&coord).await {
                                                    Ok(()) => log::info!("Unsubscribed from: {}", coord),
                                                    Err(e) => log::error!("Failed to unsubscribe: {}", e),
                                                }
                                            } else {
                                                match podcast_subscription::add_nostr_subscription(&coord, None)
                                                    .await
                                                {
                                                    Ok(()) => log::info!("Subscribed to: {}", coord),
                                                    Err(e) => log::error!("Failed to subscribe: {}", e),
                                                }
                                            }
                                            subscribing.set(false);
                                        });
                                    }
                                },
                                if *subscribing.read() {
                                    "..."
                                } else if *is_subscribed.read() {
                                    "Subscribed"
                                } else {
                                    "Subscribe"
                                }
                            }
                        }
                        if has_v4v && auth.is_authenticated {
                            button {
                                class: "p-2 hover:bg-muted rounded-full transition text-amber-500",
                                title: "Send sats",
                                dangerous_inner_html: icons::ZAP,
                            }
                        }
                        button {
                            class: "p-2 hover:bg-muted rounded-full transition",
                            title: "Share",
                            onclick: move |_| show_share_modal.set(true),
                            dangerous_inner_html: icons::SHARE,
                        }
                    }
                }
            }
            if *show_share_modal.read() {
                ContentShareModal {
                    title: metadata.title.clone(),
                    url: format!("https://nostr.blue/podcast/nostr/{}", coordinate),
                    content_type: ContentType::Podcast,
                    image_url: metadata.image.clone(),
                    on_close: move |_| show_share_modal.set(false),
                }
            }
            if let Some(ref desc) = metadata.description {
                div { class: "px-6 py-4 border-b border-border",
                    div { class: "text-sm text-muted-foreground", "{desc}" }
                }
            }
            if !metadata.funding.is_empty() {
                div { class: "px-6 py-4 border-b border-border",
                    h3 { class: "font-semibold mb-2", "Support this podcast" }
                    div { class: "flex flex-wrap gap-2",
                        for link in &metadata.funding {
                            a {
                                key: "{link.url}",
                                href: "{link.url}",
                                target: "_blank",
                                class: "px-3 py-1.5 text-sm bg-muted hover:bg-muted/80 rounded-full transition",
                                {link.name.as_deref().unwrap_or("Support")}
                            }
                        }
                    }
                }
            }
            div { class: "p-6",
                div { class: "flex items-center justify-between mb-4",
                    h2 { class: "font-semibold text-lg", "Episodes" }
                    span { class: "text-sm text-muted-foreground",
                        "{episode_count} episodes"
                        if props.has_more { "+" } else { "" }
                    }
                }
                PodcastEpisodeList {
                    episodes: props.episodes.clone(),
                    show_podcast_title: false,
                    enable_playlist: true,
                }
                // Sentinel element for infinite scroll
                if props.has_more {
                    div {
                        id: "{props.sentinel_id}",
                        class: "h-20 flex items-center justify-center",
                        if props.loading_more {
                            div { class: "flex items-center gap-3 text-muted-foreground",
                                span { class: "w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                "Loading more episodes..."
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Skeleton loader for podcast detail
#[component]
fn PodcastDetailSkeleton() -> Element {
    rsx! {
        div { class: "animate-pulse",
            div { class: "p-6",
                div { class: "flex gap-6",
                    div { class: "w-32 h-32 md:w-40 md:h-40 bg-muted rounded-lg" }
                    div { class: "flex-1 space-y-3",
                        div { class: "h-8 bg-muted rounded w-3/4" }
                        div { class: "h-4 bg-muted rounded w-1/2" }
                        div { class: "flex gap-2",
                            div { class: "h-6 bg-muted rounded-full w-16" }
                            div { class: "h-6 bg-muted rounded-full w-20" }
                        }
                    }
                }
            }
            div { class: "px-6 py-4 border-b border-border space-y-2",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-5/6" }
                div { class: "h-4 bg-muted rounded w-4/6" }
            }
            div { class: "p-6 space-y-4",
                div { class: "h-6 bg-muted rounded w-24" }
                for i in 0..5 {
                    div { key: "{i}", class: "flex gap-4 p-3",
                        div { class: "w-16 h-16 bg-muted rounded-lg" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-4 bg-muted rounded w-3/4" }
                            div { class: "h-3 bg-muted rounded w-full" }
                            div { class: "h-3 bg-muted rounded w-1/4" }
                        }
                    }
                }
            }
        }
    }
}
/// Fetch only Nostr podcast metadata by naddr/coordinate
async fn fetch_nostr_podcast_metadata(
    naddr: &str,
) -> std::result::Result<(PodcastMetadata, String), String> {
    let parsed = crate::utils::nip19::parse_naddr(naddr)?;
    let metadata_events =
        nostr_client::fetch_event_by_coordinate_with_relays(
            parsed.kind,
            parsed.pubkey.clone(),
            parsed.identifier,
            parsed.relay_hints,
        )
        .await?;
    let metadata_event = metadata_events
        .ok_or_else(|| "Podcast not found".to_string())?;
    let metadata = podcast::parse_podcast_metadata(&metadata_event)?;
    Ok((metadata, parsed.pubkey))
}

/// Fetch Nostr podcast episodes with pagination
async fn fetch_nostr_episodes(
    pubkey_hex: &str,
    metadata: &PodcastMetadata,
    limit: usize,
    until: Option<u64>,
) -> Result<Vec<DisplayEpisode>, String> {
    let mut filter = Filter::new()
        .kind(Kind::from(podcast::KIND_PODCAST_EPISODE))
        .author(PublicKey::from_hex(pubkey_hex).map_err(|e| e.to_string())?)
        .limit(limit);

    // Timestamp::from(u64) takes seconds per rust-nostr SDK
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;

    let mut episodes = Vec::new();
    for event in events.iter() {
        if is_likely_future_secs(event.created_at.as_secs()) { continue; }
        if let Ok(episode) = podcast::parse_podcast_episode(event) {
            let display = DisplayEpisode::from_nostr_episode(
                &episode,
                &metadata.title,
                metadata.image.as_deref(),
            );
            episodes.push(display);
        }
    }
    episodes.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(episodes)
}


