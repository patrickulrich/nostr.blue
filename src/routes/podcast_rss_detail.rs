//! RSS Podcast Detail Route
//!
//! Shows an RSS podcast fetched via direct RSS URL:
//! - Podcast metadata (cover, title, description)
//! - Episode list with Podcasting 2.0 features
//! - V4V payment support (if available)
//! - Chapters, transcripts, soundbites

use dioxus::prelude::*;
use crate::components::{
    PodcastEpisodeList, DisplayEpisode, icons,
};
use crate::routes::Route;
use crate::services::podcast_rss::{self, RssPodcast};
use crate::stores::auth_store;

#[derive(Props, Clone, PartialEq)]
pub struct PodcastRssFeedDetailProps {
    pub feed_url: String,
}

/// RSS podcast detail page (loaded via direct feed URL)
#[component]
pub fn PodcastRssFeedDetail(props: PodcastRssFeedDetailProps) -> Element {
    // Decode the URL-encoded feed URL
    let url = urlencoding::decode(&props.feed_url)
        .map(|s| s.to_string())
        .unwrap_or_else(|_| props.feed_url.clone());

    // Fetch podcast data
    let podcast_data = use_resource(move || {
        let url = url.clone();
        async move {
            podcast_rss::fetch_podcast_feed(&url).await
        }
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
            match &*podcast_data.read() {
                Some(Ok(podcast)) => rsx! {
                    RssPodcastDetailContent {
                        podcast: podcast.clone()
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
                    RssPodcastDetailSkeleton {}
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RssPodcastDetailContentProps {
    podcast: RssPodcast,
}

#[component]
fn RssPodcastDetailContent(props: RssPodcastDetailContentProps) -> Element {
    let podcast = &props.podcast;
    let auth = auth_store::AUTH_STATE.read();

    // Image URL with fallback
    let image_url = podcast.image.clone()
        .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/shapes/svg?seed={}", podcast.title));

    // Check if V4V is available
    let has_v4v = podcast.value.is_some();

    // Convert episodes to DisplayEpisode
    let episodes: Vec<DisplayEpisode> = podcast.episodes.iter()
        .map(|ep| DisplayEpisode::from_rss_episode(ep, podcast))
        .collect();

    rsx! {
        div {
            // Cover section
            div {
                class: "relative",

                // Background blur
                div {
                    class: "absolute inset-0 h-48 bg-gradient-to-b from-green-500/20 to-background"
                }

                // Content overlay
                div {
                    class: "relative p-6",

                    div {
                        class: "flex gap-6",

                        // Cover image
                        img {
                            src: "{image_url}",
                            alt: "{podcast.title}",
                            class: "w-32 h-32 md:w-40 md:h-40 rounded-lg object-cover shadow-lg"
                        }

                        // Info
                        div {
                            class: "flex-1 min-w-0",

                            // Title
                            h1 {
                                class: "text-2xl font-bold truncate",
                                "{podcast.title}"
                            }

                            // Author
                            if let Some(ref author) = podcast.author {
                                p {
                                    class: "text-muted-foreground mt-1",
                                    "{author}"
                                }
                            }

                            // Badges
                            div {
                                class: "flex items-center gap-2 mt-2 flex-wrap",

                                // RSS badge
                                span {
                                    class: "px-2 py-1 text-xs bg-green-500/20 text-green-400 rounded-full font-medium",
                                    "RSS"
                                }

                                // Podcasting 2.0 badge (if has V4V or other features)
                                if has_v4v || podcast.trailer.is_some() || !podcast.persons.is_empty() {
                                    span {
                                        class: "px-2 py-1 text-xs bg-blue-500/20 text-blue-400 rounded-full font-medium",
                                        "Podcasting 2.0"
                                    }
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
                                if podcast.explicit {
                                    span {
                                        class: "px-2 py-1 text-xs bg-red-500/20 text-red-400 rounded-full font-medium",
                                        "Explicit"
                                    }
                                }
                            }

                            // Categories
                            if !podcast.categories.is_empty() {
                                div {
                                    class: "flex items-center gap-1 mt-2 flex-wrap",
                                    for cat in podcast.categories.iter().take(4) {
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

                        // Zap button (if V4V)
                        if has_v4v && auth.is_authenticated {
                            button {
                                class: "px-4 py-2 text-sm font-medium bg-amber-500 text-white rounded-full hover:bg-amber-600 transition flex items-center gap-2",
                                title: "Send sats",
                                dangerous_inner_html: icons::ZAP,
                                "Boost"
                            }
                        }

                        // RSS Feed link
                        a {
                            href: "{podcast.feed_url}",
                            target: "_blank",
                            class: "px-4 py-2 text-sm font-medium border border-border rounded-full hover:bg-muted transition",
                            "RSS Feed"
                        }

                        // Website link
                        if let Some(ref link) = podcast.link {
                            a {
                                href: "{link}",
                                target: "_blank",
                                class: "p-2 hover:bg-muted rounded-full transition",
                                title: "Visit website",
                                dangerous_inner_html: icons::EXTERNAL_LINK
                            }
                        }

                        // Share button
                        button {
                            class: "p-2 hover:bg-muted rounded-full transition",
                            title: "Share",
                            dangerous_inner_html: icons::SHARE
                        }
                    }
                }
            }

            // Description
            if let Some(ref desc) = podcast.description {
                div {
                    class: "px-6 py-4 border-b border-border",
                    div {
                        class: "text-sm text-muted-foreground prose prose-sm dark:prose-invert max-w-none",
                        dangerous_inner_html: "{desc}"
                    }
                }
            }

            // Hosts/Persons
            if !podcast.persons.is_empty() {
                div {
                    class: "px-6 py-4 border-b border-border",
                    h3 {
                        class: "font-semibold mb-3",
                        "Hosts"
                    }
                    div {
                        class: "flex flex-wrap gap-4",
                        for person in &podcast.persons {
                            div {
                                key: "{person.name}",
                                class: "flex items-center gap-2",
                                if let Some(ref img) = person.img {
                                    img {
                                        src: "{img}",
                                        alt: "{person.name}",
                                        class: "w-10 h-10 rounded-full object-cover"
                                    }
                                } else {
                                    div {
                                        class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center",
                                        dangerous_inner_html: icons::USER
                                    }
                                }
                                div {
                                    div {
                                        class: "font-medium text-sm",
                                        "{person.name}"
                                    }
                                    if let Some(ref role) = person.role {
                                        div {
                                            class: "text-xs text-muted-foreground",
                                            "{role}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Funding links
            if !podcast.funding.is_empty() {
                div {
                    class: "px-6 py-4 border-b border-border",
                    h3 {
                        class: "font-semibold mb-2",
                        "Support this podcast"
                    }
                    div {
                        class: "flex flex-wrap gap-2",
                        for link in &podcast.funding {
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

            // Trailer (if available)
            if let Some(ref trailer) = podcast.trailer {
                div {
                    class: "px-6 py-4 border-b border-border",
                    h3 {
                        class: "font-semibold mb-2",
                        "Trailer"
                    }
                    div {
                        class: "flex items-center gap-4 p-3 bg-muted/50 rounded-lg",
                        button {
                            class: "p-3 bg-primary text-primary-foreground rounded-full hover:bg-primary/90 transition",
                            // TODO: Play trailer
                            dangerous_inner_html: icons::PLAY
                        }
                        div {
                            class: "flex-1",
                            div {
                                class: "font-medium text-sm",
                                "Listen to the trailer"
                            }
                            if let Some(ref season) = trailer.season {
                                div {
                                    class: "text-xs text-muted-foreground",
                                    "Season {season}"
                                }
                            }
                        }
                    }
                }
            }

            // Podroll (recommended shows)
            if !podcast.podroll.is_empty() {
                div {
                    class: "px-6 py-4 border-b border-border",
                    h3 {
                        class: "font-semibold mb-2",
                        "Recommended Shows"
                    }
                    div {
                        class: "text-sm text-muted-foreground",
                        "{podcast.podroll.len()} recommended podcasts"
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
                        "{episodes.len()} episodes"
                    }
                }

                PodcastEpisodeList {
                    episodes: episodes,
                    show_podcast_title: false,
                    enable_playlist: true
                }
            }
        }
    }
}

/// Skeleton loader for RSS podcast detail
#[component]
fn RssPodcastDetailSkeleton() -> Element {
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
                            div { class: "h-6 bg-muted rounded-full w-12" }
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

