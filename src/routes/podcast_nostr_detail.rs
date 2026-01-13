//! Nostr Podcast Detail Route
//!
//! Shows a native Nostr podcast with:
//! - Podcast metadata (cover, title, description)
//! - Episode list
//! - V4V payment support
//! - Follow/subscribe functionality

use dioxus::prelude::*;
use crate::components::{
    PodcastEpisodeList, DisplayEpisode, icons,
    ContentShareModal, ContentType,
};
use crate::routes::Route;
use crate::stores::{nostr_client, auth_store, podcast_subscription};
use crate::utils::podcast::{self, PodcastMetadata};
use nostr_sdk::prelude::{Filter, Kind, PublicKey, SingleLetterTag};
use std::time::Duration;

#[derive(Props, Clone, PartialEq)]
pub struct PodcastNostrDetailProps {
    pub naddr: String,
}

/// Nostr podcast detail page
#[component]
pub fn PodcastNostrDetail(props: PodcastNostrDetailProps) -> Element {
    let naddr = props.naddr.clone();

    // State for podcast data
    let mut podcast_data = use_signal(|| None::<Result<(PodcastMetadata, Vec<DisplayEpisode>), String>>);
    let mut loading = use_signal(|| true);

    // Fetch podcast when client is initialized (signer not needed for public content)
    use_effect(move || {
        let naddr = naddr.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        loading.set(true);

        spawn(async move {
            let result = fetch_nostr_podcast(&naddr).await;
            podcast_data.set(Some(result));
            loading.set(false);
        });
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header with back button
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "p-4 flex items-center gap-4",
                    Link {
                        to: Route::PodcastHome {},
                        class: "p-2 hover:bg-muted rounded-full transition",
                        dangerous_inner_html: icons::ARROW_LEFT
                    }
                    h1 {
                        class: "text-xl font-bold",
                        "Podcast"
                    }
                }
            }

            // Content
            if !*nostr_client::CLIENT_INITIALIZED.read() || *loading.read() {
                // Loading state
                PodcastDetailSkeleton {}
            } else {
                match podcast_data.read().as_ref() {
                    Some(Ok((metadata, episodes))) => rsx! {
                        PodcastDetailContent {
                            metadata: metadata.clone(),
                            episodes: episodes.clone()
                        }
                    },
                    Some(Err(e)) => rsx! {
                        div {
                            class: "p-4 text-center",
                            div {
                                class: "text-destructive mb-2",
                                "Failed to load podcast"
                            }
                            div {
                                class: "text-sm text-muted-foreground",
                                "{e}"
                            }
                        }
                    },
                    None => rsx! {
                        PodcastDetailSkeleton {}
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PodcastDetailContentProps {
    metadata: PodcastMetadata,
    episodes: Vec<DisplayEpisode>,
}

#[component]
fn PodcastDetailContent(props: PodcastDetailContentProps) -> Element {
    let metadata = &props.metadata;
    let auth = auth_store::AUTH_STATE.read();
    let mut show_share_modal = use_signal(|| false);

    // Image URL with fallback
    let image_url = metadata.image.clone()
        .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/shapes/svg?seed={}", metadata.title));

    // Check if V4V is available
    let has_v4v = metadata.value.is_some();

    // Build coordinate for subscription
    let coordinate = format!("30078:{}:{}", metadata.pubkey, metadata.d_tag);
    // Check subscription reactively using use_memo so UI updates when SUBSCRIPTIONS changes
    let coordinate_for_memo = coordinate.clone();
    let is_subscribed = use_memo(move || {
        podcast_subscription::is_subscribed(&coordinate_for_memo)
    });
    let mut subscribing = use_signal(|| false);

    rsx! {
        div {
            // Cover section
            div {
                class: "relative",

                // Background blur
                div {
                    class: "absolute inset-0 h-48 bg-gradient-to-b from-primary/20 to-background"
                }

                // Content overlay
                div {
                    class: "relative p-6",

                    div {
                        class: "flex gap-6",

                        // Cover image
                        img {
                            src: "{image_url}",
                            alt: "{metadata.title}",
                            class: "w-32 h-32 md:w-40 md:h-40 rounded-lg object-cover shadow-lg"
                        }

                        // Info
                        div {
                            class: "flex-1 min-w-0",

                            // Title
                            h1 {
                                class: "text-2xl font-bold truncate",
                                "{metadata.title}"
                            }

                            // Author
                            if let Some(ref author) = metadata.author {
                                p {
                                    class: "text-muted-foreground mt-1",
                                    "{author}"
                                }
                            }

                            // Badges
                            div {
                                class: "flex items-center gap-2 mt-2 flex-wrap",

                                // Nostr badge
                                span {
                                    class: "px-2 py-1 text-xs bg-purple-500/20 text-purple-400 rounded-full font-medium",
                                    "Nostr"
                                }

                                // V4V badge
                                if has_v4v {
                                    span {
                                        class: "px-2 py-1 text-xs bg-amber-500/20 text-amber-400 rounded-full font-medium flex items-center gap-1",
                                        dangerous_inner_html: icons::ZAP,
                                        "V4V Enabled"
                                    }
                                }

                                // Explicit badge
                                if metadata.explicit {
                                    span {
                                        class: "px-2 py-1 text-xs bg-red-500/20 text-red-400 rounded-full font-medium",
                                        "Explicit"
                                    }
                                }
                            }

                            // Categories
                            if !metadata.categories.is_empty() {
                                div {
                                    class: "flex items-center gap-1 mt-2 flex-wrap",
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

                    // Action buttons
                    div {
                        class: "flex items-center gap-3 mt-4",

                        // Subscribe button
                        if auth.is_authenticated {
                            button {
                                class: if *is_subscribed.read() || *subscribing.read() {
                                    "px-4 py-2 text-sm font-medium border border-border rounded-full hover:bg-muted transition"
                                } else {
                                    "px-4 py-2 text-sm font-medium bg-primary text-primary-foreground rounded-full hover:bg-primary/90 transition"
                                },
                                disabled: *subscribing.read(),
                                onclick: {
                                    let coord = coordinate.clone();
                                    move |_| {
                                        if *subscribing.read() { return; }
                                        let coord = coord.clone();
                                        // Re-check subscription state from store (not stale captured value)
                                        let currently_subscribed = podcast_subscription::is_subscribed(&coord);
                                        subscribing.set(true);

                                        spawn(async move {
                                            if currently_subscribed {
                                                match podcast_subscription::remove_subscription(&coord).await {
                                                    Ok(()) => log::info!("Unsubscribed from: {}", coord),
                                                    Err(e) => log::error!("Failed to unsubscribe: {}", e),
                                                }
                                            } else {
                                                match podcast_subscription::add_nostr_subscription(&coord, None).await {
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

                        // Zap button (if V4V)
                        if has_v4v && auth.is_authenticated {
                            button {
                                class: "p-2 hover:bg-muted rounded-full transition text-amber-500",
                                title: "Send sats",
                                dangerous_inner_html: icons::ZAP
                            }
                        }

                        // Share button
                        button {
                            class: "p-2 hover:bg-muted rounded-full transition",
                            title: "Share",
                            onclick: move |_| show_share_modal.set(true),
                            dangerous_inner_html: icons::SHARE
                        }
                    }
                }
            }

            // Share modal
            if *show_share_modal.read() {
                ContentShareModal {
                    title: metadata.title.clone(),
                    url: format!("https://nostr.blue/podcast/nostr/{}", coordinate),
                    content_type: ContentType::Podcast,
                    image_url: metadata.image.clone(),
                    on_close: move |_| show_share_modal.set(false)
                }
            }

            // Description
            if let Some(ref desc) = metadata.description {
                div {
                    class: "px-6 py-4 border-b border-border",
                    div {
                        class: "text-sm text-muted-foreground",
                        "{desc}"
                    }
                }
            }

            // Funding links
            if !metadata.funding.is_empty() {
                div {
                    class: "px-6 py-4 border-b border-border",
                    h3 {
                        class: "font-semibold mb-2",
                        "Support this podcast"
                    }
                    div {
                        class: "flex flex-wrap gap-2",
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

            // Episodes section
            div {
                class: "p-6",
                div {
                    class: "flex items-center justify-between mb-4",
                    h2 {
                        class: "font-semibold text-lg",
                        "Episodes"
                    }
                    span {
                        class: "text-sm text-muted-foreground",
                        "{props.episodes.len()} episodes"
                    }
                }

                PodcastEpisodeList {
                    episodes: props.episodes.clone(),
                    show_podcast_title: false,
                    enable_playlist: true
                }
            }
        }
    }
}

/// Skeleton loader for podcast detail
#[component]
fn PodcastDetailSkeleton() -> Element {
    rsx! {
        div {
            class: "animate-pulse",

            // Cover section skeleton
            div {
                class: "p-6",
                div {
                    class: "flex gap-6",
                    div { class: "w-32 h-32 md:w-40 md:h-40 bg-muted rounded-lg" }
                    div {
                        class: "flex-1 space-y-3",
                        div { class: "h-8 bg-muted rounded w-3/4" }
                        div { class: "h-4 bg-muted rounded w-1/2" }
                        div { class: "flex gap-2",
                            div { class: "h-6 bg-muted rounded-full w-16" }
                            div { class: "h-6 bg-muted rounded-full w-20" }
                        }
                    }
                }
            }

            // Description skeleton
            div {
                class: "px-6 py-4 border-b border-border space-y-2",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-5/6" }
                div { class: "h-4 bg-muted rounded w-4/6" }
            }

            // Episodes skeleton
            div {
                class: "p-6 space-y-4",
                div { class: "h-6 bg-muted rounded w-24" }
                for i in 0..5 {
                    div {
                        key: "{i}",
                        class: "flex gap-4 p-3",
                        div { class: "w-16 h-16 bg-muted rounded-lg" }
                        div {
                            class: "flex-1 space-y-2",
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

// ============================================================================
// Data Fetching
// ============================================================================

/// Fetch Nostr podcast by naddr/coordinate
async fn fetch_nostr_podcast(naddr: &str) -> std::result::Result<(PodcastMetadata, Vec<DisplayEpisode>), String> {
    // Parse the coordinate (format: "30078:pubkey:d-tag" or naddr)
    let (pubkey, d_tag) = parse_coordinate(naddr)?;

    // Fetch podcast metadata
    let metadata_filter = Filter::new()
        .kind(Kind::from(podcast::KIND_APP_DATA))
        .author(PublicKey::from_hex(&pubkey).map_err(|e| e.to_string())?)
        .custom_tag(
            SingleLetterTag::from_char('d').unwrap(),
            d_tag.clone()
        )
        .limit(1);

    let metadata_events = nostr_client::fetch_events_aggregated(metadata_filter, Duration::from_secs(10))
        .await?;

    let metadata_event = metadata_events.first()
        .ok_or_else(|| "Podcast not found".to_string())?;

    let metadata = podcast::parse_podcast_metadata(metadata_event)?;

    // Fetch episodes for this podcast
    let episode_filter = Filter::new()
        .kind(Kind::from(podcast::KIND_PODCAST_EPISODE))
        .author(PublicKey::from_hex(&pubkey).map_err(|e| e.to_string())?)
        .limit(100);

    let episode_events = nostr_client::fetch_events_aggregated(episode_filter, Duration::from_secs(10))
        .await?;

    let mut episodes = Vec::new();
    for event in episode_events.iter() {
        if let Ok(episode) = podcast::parse_podcast_episode(event) {
            let display = DisplayEpisode::from_nostr_episode(
                &episode,
                &metadata.title,
                metadata.image.as_deref(),
            );
            episodes.push(display);
        }
    }

    // Sort episodes by created_at descending (most recent first)
    episodes.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok((metadata, episodes))
}

/// Parse coordinate string into pubkey and d-tag
fn parse_coordinate(coord: &str) -> Result<(String, String), String> {
    use nostr::prelude::*;

    // Handle naddr format
    if coord.starts_with("naddr") {
        let nip19 = Nip19::from_bech32(coord)
            .map_err(|e| format!("Invalid naddr: {}", e))?;

        match nip19 {
            Nip19::Coordinate(nip19_coord) => {
                let pubkey = nip19_coord.coordinate.public_key.to_hex();
                let d_tag = nip19_coord.coordinate.identifier;
                Ok((pubkey, d_tag))
            }
            _ => Err("Expected naddr coordinate".to_string())
        }
    } else {
        // Handle coordinate format: "KIND:PUBKEY:D-TAG"
        let parts: Vec<&str> = coord.split(':').collect();
        if parts.len() >= 3 {
            return Ok((parts[1].to_string(), parts[2].to_string()));
        }

        // Try as just "PUBKEY:D-TAG"
        if parts.len() == 2 {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }

        Err(format!("Invalid coordinate format: {}", coord))
    }
}
