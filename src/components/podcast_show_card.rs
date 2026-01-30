//! Podcast Show Card Component
//!
//! Displays a podcast show in a card format, supporting both:
//! - Native Nostr podcasts (kind 30078 metadata)
//! - RSS podcasts (Podcasting 2.0)

use crate::components::icons;
use crate::routes::Route;
use crate::utils::podcast::{PodcastMetadata, PodcastSource, ValueBlock};
use dioxus::prelude::*;

/// Unified podcast show representation for display
#[derive(Clone, Debug, PartialEq)]
pub struct PodcastShow {
    /// Unique identifier
    pub id: String,
    /// Show title
    pub title: String,
    /// Author/creator name
    pub author: Option<String>,
    /// Show description
    pub description: Option<String>,
    /// Cover image URL
    pub image: Option<String>,
    /// Episode count (if known)
    pub episode_count: Option<usize>,
    /// Source information for routing
    pub source: PodcastSource,
    /// V4V payment configuration (for RSS podcasts)
    pub value: Option<ValueBlock>,
    /// Categories/topics
    pub categories: Vec<String>,
    /// Explicit content flag
    pub explicit: bool,
}

impl PodcastShow {
    /// Create from Nostr podcast metadata
    pub fn from_nostr_metadata(metadata: &PodcastMetadata) -> Self {
        use nostr::prelude::*;

        // Generate proper naddr for linking
        let naddr = PublicKey::from_hex(&metadata.pubkey)
            .ok()
            .map(|pk| {
                let coord = Coordinate::new(Kind::from(30078), pk).identifier(&metadata.d_tag);
                let nip19_coord = Nip19Coordinate::new(coord, vec![]);
                nip19_coord
                    .to_bech32()
                    .unwrap_or_else(|_| format!("30078:{}:{}", metadata.pubkey, metadata.d_tag))
            })
            .unwrap_or_else(|| format!("30078:{}:{}", metadata.pubkey, metadata.d_tag));

        Self {
            id: format!("{}:{}", metadata.pubkey, metadata.d_tag),
            title: metadata.title.clone(),
            author: metadata.author.clone(),
            description: metadata.description.clone(),
            image: metadata.image.clone(),
            episode_count: None,
            source: PodcastSource::Nostr {
                pubkey: metadata.pubkey.clone(),
                d_tag: metadata.d_tag.clone(),
                coordinate: naddr,
            },
            value: metadata.value.clone(),
            categories: metadata.categories.clone(),
            explicit: metadata.explicit,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct PodcastShowCardProps {
    /// The podcast show to display
    pub show: PodcastShow,
    /// Show episode count badge
    #[props(default = true)]
    pub show_episode_count: bool,
    /// Compact mode (smaller image, less detail)
    #[props(default = false)]
    pub compact: bool,
}

/// Podcast show card component
#[component]
pub fn PodcastShowCard(props: PodcastShowCardProps) -> Element {
    let show = &props.show;

    // Build route based on source
    let route = match &show.source {
        PodcastSource::Nostr { coordinate, .. } => {
            // naddr encoding would be done here - for now use coordinate
            Route::PodcastNostrDetail {
                naddr: coordinate.clone(),
            }
        }
        PodcastSource::Rss { podcast_id, .. } => {
            // Use podcast ID if available, otherwise fallback to URL search
            if let Some(id) = podcast_id {
                Route::PodcastRssFeedDetail {
                    podcast_id: id.to_string(),
                }
            } else {
                // TODO: Add a search-by-URL route or lookup
                Route::PodcastHome {}
            }
        }
    };

    // Source badge info
    let source_badge = match &show.source {
        PodcastSource::Nostr { .. } => ("N", "Nostr Podcast", "bg-purple-500/20 text-purple-400"),
        PodcastSource::Rss { .. } => ("R", "RSS Feed", "bg-green-500/20 text-green-400"),
    };

    // Image size based on compact mode
    let image_class = if props.compact {
        "w-12 h-12 rounded-lg object-cover"
    } else {
        "w-16 h-16 rounded-lg object-cover"
    };

    // Fallback image
    let image_url = show
        .image
        .clone()
        .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/shapes/svg?seed={}", show.id));

    // Has V4V support
    let has_v4v = show.value.is_some();

    rsx! {
        Link {
            to: route,
            class: "flex items-center gap-3 p-3 hover:bg-muted/50 rounded-lg transition group",

            // Podcast cover image
            div {
                class: "relative shrink-0",
                img {
                    src: "{image_url}",
                    alt: "{show.title}",
                    class: "{image_class}",
                    loading: "lazy"
                }

                // Source badge
                div {
                    class: "absolute -top-1 -right-1 w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold {source_badge.2}",
                    title: "{source_badge.1}",
                    "{source_badge.0}"
                }
            }

            // Show info
            div {
                class: "flex-1 min-w-0",

                // Title row
                div {
                    class: "flex items-center gap-2",
                    span {
                        class: "font-medium text-sm truncate",
                        "{show.title}"
                    }
                    if show.explicit {
                        span {
                            class: "px-1.5 py-0.5 text-[10px] bg-red-500/20 text-red-400 rounded font-bold",
                            "E"
                        }
                    }
                }

                // Author
                if let Some(ref author) = show.author {
                    div {
                        class: "text-xs text-muted-foreground truncate",
                        "{author}"
                    }
                }

                // Categories (compact view)
                if !props.compact && !show.categories.is_empty() {
                    div {
                        class: "flex items-center gap-1 mt-1 flex-wrap",
                        for category in show.categories.iter().take(3) {
                            span {
                                key: "{category}",
                                class: "px-1.5 py-0.5 text-[10px] bg-muted text-muted-foreground rounded",
                                "{category}"
                            }
                        }
                    }
                }
            }

            // Right side badges
            div {
                class: "flex items-center gap-2 shrink-0",

                // V4V indicator
                if has_v4v {
                    div {
                        class: "flex items-center text-amber-500",
                        title: "Supports Value4Value",
                        dangerous_inner_html: icons::ZAP
                    }
                }

                // Episode count
                if props.show_episode_count {
                    if let Some(count) = show.episode_count {
                        div {
                            class: "text-xs text-muted-foreground",
                            "{count} episodes"
                        }
                    }
                }
            }
        }
    }
}

/// Skeleton loader for podcast show card
#[component]
pub fn PodcastShowCardSkeleton() -> Element {
    rsx! {
        div {
            class: "flex items-center gap-3 p-3 rounded-lg animate-pulse",

            // Image skeleton
            div {
                class: "w-16 h-16 bg-muted rounded-lg shrink-0"
            }

            // Info skeleton
            div {
                class: "flex-1 min-w-0 space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
                div {
                    class: "flex gap-1",
                    div { class: "h-4 bg-muted rounded w-12" }
                    div { class: "h-4 bg-muted rounded w-16" }
                }
            }

            // Badge skeleton
            div {
                class: "w-20 h-4 bg-muted rounded shrink-0"
            }
        }
    }
}

/// Grid layout for podcast show cards
#[component]
pub fn PodcastShowGrid(shows: Vec<PodcastShow>) -> Element {
    rsx! {
        div {
            class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2",
            for show in shows {
                PodcastShowCard {
                    key: "{show.id}",
                    show: show.clone()
                }
            }
        }
    }
}
