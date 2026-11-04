//! Discovery rail primitives — horizontal sliders of podcast tiles.
//!
//! A rail is a heading row (title + optional "See all" link or explanatory
//! note) over a horizontally scrolling track of square-artwork tiles with
//! rank badges. Every rail renders its own loading skeleton and empty line:
//! a rail that fails never breaks the page around it.
//!
//! Personal rails show [`LockedRailCard`] to signed-out visitors (skeleton
//! tiles behind a sign-in prompt) and Podcast-Index-backed rails show
//! [`RailAuthRequired`] when no signer is attached, since PI requests
//! require NIP-98 authentication.
use crate::components::icons;
use crate::components::LoginModal;
use crate::routes::Route;
use dioxus::prelude::*;

/// Source badge rendered on a tile's artwork.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RailBadge {
    /// Podcasting 2.0 value block (amber).
    V4v,
    /// RSS feed via Podcast Index (green).
    Rss,
    /// Native nostr podcast (purple).
    Nostr,
}

impl RailBadge {
    fn class(self) -> &'static str {
        match self {
            Self::V4v => "bg-amber-500/90 text-white",
            Self::Rss => "bg-green-500/90 text-white",
            Self::Nostr => "bg-purple-500/90 text-white",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::V4v => "V4V",
            Self::Rss => "RSS",
            Self::Nostr => "Nostr",
        }
    }
}

/// What the heading row shows on the right: a route link or a plain note.
#[derive(Clone, PartialEq, Debug)]
pub enum RailFooter {
    Link { to: Route, label: String },
    Note(String),
}

/// The unified tile model every discovery rail renders.
#[derive(Clone, PartialEq, Debug)]
pub struct RailTileData {
    /// Stable list key.
    pub key: String,
    pub title: String,
    /// One line under the title (figures, author, reason…).
    pub subtitle: Option<String>,
    /// Tooltip for the subtitle.
    pub subtitle_title: Option<String>,
    pub image: Option<String>,
    /// Rank badge ("#1"); omit for unranked rails.
    pub rank: Option<usize>,
    pub badge: Option<RailBadge>,
    /// Round artwork (people) instead of square (content).
    pub person: bool,
    pub route: Option<Route>,
    /// Fallback glyph when no image resolves.
    pub glyph: &'static str,
}

impl RailTileData {
    /// Convenience constructor for RSS show tiles. The badge names the
    /// payment rail: V4V when the feed carries a value block, RSS otherwise.
    pub fn rss_show(feed: &crate::services::podcast_index::PodcastFeed) -> Self {
        Self {
            key: format!("rss-{}", feed.id),
            title: feed.title.clone(),
            subtitle: feed.author.clone(),
            subtitle_title: None,
            image: feed.get_image().map(String::from),
            rank: None,
            badge: Some(if feed.has_v4v() {
                RailBadge::V4v
            } else {
                RailBadge::Rss
            }),
            person: false,
            route: Some(Route::PodcastRssFeedDetail {
                podcast_id: feed.id.to_string(),
            }),
            glyph: "🎙",
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct DiscoveryRailProps {
    pub title: String,
    pub footer: RailFooter,
    /// `None` renders the skeleton row; `Some(vec![])` renders the empty line.
    pub tiles: Option<Vec<RailTileData>>,
    #[props(default = 12)]
    pub skeleton_count: usize,
    #[props(default = "Nothing here yet.".to_string())]
    pub empty_message: String,
}

/// One discovery rail: heading row + tile track.
#[component]
pub fn DiscoveryRail(props: DiscoveryRailProps) -> Element {
    rsx! {
        section { class: "space-y-3",
            div { class: "flex items-baseline justify-between gap-3",
                h3 { class: "font-semibold text-base shrink-0", "{props.title}" }
                match props.footer {
                    RailFooter::Link { to, label } => rsx! {
                        Link {
                            to,
                            class: "text-sm text-primary hover:underline whitespace-nowrap",
                            "{label}"
                        }
                    },
                    RailFooter::Note(note) => rsx! {
                        span { class: "text-xs text-muted-foreground text-right", "{note}" }
                    },
                }
            }
            match props.tiles {
                None => rsx! { RailSkeleton { count: props.skeleton_count } },
                Some(ref tiles) if tiles.is_empty() => rsx! {
                    p { class: "text-sm text-muted-foreground py-3", "{props.empty_message}" }
                },
                Some(ref tiles) => rsx! {
                    div {
                        class: "flex gap-3 overflow-x-auto pb-2 -mx-4 px-4 scrollbar-hide snap-x",
                        for tile in tiles.iter() {
                            RailTile { key: "{tile.key}", tile: tile.clone() }
                        }
                    }
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RailTileProps {
    tile: RailTileData,
}

/// One tile: artwork (rank + source badge), title, one-line subtitle.
#[component]
fn RailTile(props: RailTileProps) -> Element {
    let tile = &props.tile;
    let media_class = if tile.person {
        "relative aspect-square rounded-full overflow-hidden bg-muted border border-border shrink-0"
    } else {
        "relative aspect-square rounded-lg overflow-hidden bg-muted border border-border shrink-0"
    };
    let media = rsx! {
        div { class: "{media_class}",
            if let Some(ref image) = tile.image {
                img {
                    src: "{image}",
                    alt: "",
                    class: "w-full h-full object-cover",
                    loading: "lazy",
                    referrerpolicy: "no-referrer",
                }
            } else {
                div {
                    class: "w-full h-full flex items-center justify-center text-2xl text-muted-foreground",
                    "{tile.glyph}"
                }
            }
            if let Some(rank) = tile.rank {
                span {
                    class: "absolute top-1.5 left-1.5 min-w-6 h-6 px-1.5 flex items-center justify-center rounded-full bg-background/90 text-xs font-bold text-foreground",
                    "#{rank}"
                }
            }
            if let Some(badge) = tile.badge {
                span {
                    class: "absolute bottom-1.5 right-1.5 px-1.5 py-0.5 text-[10px] font-semibold rounded {badge.class()}",
                    "{badge.label()}"
                }
            }
        }
    };
    let body = rsx! {
        div { class: "flex flex-col gap-0.5 min-w-0 w-full",
            p { class: "text-xs font-medium line-clamp-2 leading-snug", "{tile.title}" }
            if let Some(ref subtitle) = tile.subtitle {
                p {
                    class: "text-[11px] text-muted-foreground truncate w-full",
                    title: tile.subtitle_title.clone().unwrap_or_default(),
                    "{subtitle}"
                }
            }
        }
    };
    let inner = rsx! {
        div { class: "flex flex-col gap-2 w-28 sm:w-32 group",
            {media}
            {body}
        }
    };
    // Person tiles keep the badge inside the circle (overflow-visible chip).
    match tile.route.clone() {
        Some(route) => rsx! {
            Link {
                to: route,
                class: "shrink-0 snap-start w-28 sm:w-32 hover:opacity-90 transition",
                {inner}
            }
        },
        None => rsx! {
            div { class: "shrink-0 snap-start w-28 sm:w-32", {inner} }
        },
    }
}

#[derive(Props, Clone, PartialEq)]
struct RailSkeletonProps {
    count: usize,
}

/// The skeleton row a rail shows while its data loads.
#[component]
fn RailSkeleton(props: RailSkeletonProps) -> Element {
    rsx! {
        div { class: "flex gap-3 overflow-hidden pb-2 -mx-4 px-4",
            for i in 0..props.count {
                div {
                    key: "{i}",
                    class: "shrink-0 w-28 sm:w-32 animate-pulse",
                    div { class: "aspect-square rounded-lg bg-muted" }
                    div { class: "h-3 bg-muted rounded w-4/5 mt-2" }
                    div { class: "h-2.5 bg-muted rounded w-3/5 mt-1" }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct LockedRailCardProps {
    pub title: &'static str,
    pub description: &'static str,
}

/// The signed-out state of a personal rail: skeleton tiles behind a sign-in
/// prompt, so the visitor sees what signing in unlocks.
#[component]
pub fn LockedRailCard(props: LockedRailCardProps) -> Element {
    let mut show_login_modal = use_signal(|| false);
    rsx! {
        div { class: "relative",
            div { class: "flex gap-3 overflow-hidden py-2 opacity-60", aria_hidden: "true",
                for i in 0..8 {
                    div {
                        key: "{i}",
                        class: "shrink-0 w-28 sm:w-32",
                        div { class: "aspect-square rounded-lg bg-muted" }
                        div { class: "h-3 bg-muted rounded w-4/5 mt-2" }
                        div { class: "h-2.5 bg-muted rounded w-3/5 mt-1" }
                    }
                }
            }
            div {
                class: "absolute inset-0 flex items-center justify-center",
                div { class: "flex items-center gap-3 bg-card border border-border rounded-xl p-3 pr-4 max-w-md shadow-sm",
                    span {
                        class: "shrink-0 text-muted-foreground",
                        dangerous_inner_html: icons::LOCK,
                    }
                    div { class: "min-w-0",
                        h4 { class: "font-semibold text-sm", "{props.title}" }
                        p { class: "text-xs text-muted-foreground mt-0.5", "{props.description}" }
                    }
                    button {
                        class: "shrink-0 px-3 py-1.5 rounded-full text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition",
                        onclick: move |_| show_login_modal.set(true),
                        "Sign In"
                    }
                }
            }
        }
        if *show_login_modal.read() {
            LoginModal {
                on_close: move |_| show_login_modal.set(false),
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct RailAuthRequiredProps {
    pub item_label: &'static str,
}

/// Compact inline sign-in prompt for Podcast-Index-backed rails when no
/// signer is attached (PI requests require NIP-98 authentication).
#[component]
pub fn RailAuthRequired(props: RailAuthRequiredProps) -> Element {
    let mut show_login_modal = use_signal(|| false);
    rsx! {
        div {
            class: "flex items-center justify-between gap-3 border border-dashed border-border rounded-lg px-4 py-3",
            span {
                class: "text-sm text-muted-foreground",
                "Sign in to load {props.item_label} — Podcast Index requests are authenticated with your signer."
            }
            button {
                class: "shrink-0 px-3 py-1.5 rounded-full text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition",
                onclick: move |_| show_login_modal.set(true),
                "Sign In"
            }
        }
        if *show_login_modal.read() {
            LoginModal {
                on_close: move |_| show_login_modal.set(false),
            }
        }
    }
}
