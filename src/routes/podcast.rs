//! Podcast Home Route
//!
//! Discovery page for podcasts featuring:
//! - Platform filtering (All, RSS/Podcast 2.0, Nostr Native)
//! - Search for Nostr and RSS podcasts
//! - Trending/recent podcasts from both sources
//! - NIP-51 subscription management

use dioxus::prelude::*;
use crate::components::{
    PodcastShowCard, PodcastShowCardSkeleton, PodcastShow,
    PodcastEpisodeCard, PodcastEpisodeCardSkeleton, DisplayEpisode,
    PodcastAddFeedModal,
    icons,
};
use crate::routes::Route;
use crate::services::podcast_index;
use crate::stores::{nostr_client, auth_store, podcast_subscription};
use crate::utils::podcast;
use crate::utils::truncate_pubkey;
use nostr_sdk::prelude::{Filter, Kind};
use std::time::Duration;

/// Podcast home page
#[component]
pub fn PodcastHome() -> Element {
    let mut search_query = use_signal(String::new);  // Display value in input
    let mut submitted_query = use_signal(String::new);  // Actual search query (set on Enter)
    let mut active_tab = use_signal(|| PodcastTab::Discover);
    let mut selected_platform = use_signal(|| String::from("all")); // "all", "rss", "nostr"
    let mut show_add_feed_modal = use_signal(|| false);

    // Check authentication for subscription features
    let is_authenticated = auth_store::is_authenticated();

    // Load subscriptions on mount (authenticated users only)
    // Always fetch to ensure data is current after navigation
    use_effect(move || {
        if auth_store::is_authenticated() {
            spawn(async move {
                if let Err(e) = podcast_subscription::fetch_subscriptions().await {
                    log::error!("Failed to fetch podcast subscriptions: {}", e);
                }
            });
        }
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "p-4",
                    h1 {
                        class: "text-xl font-bold",
                        "Podcasts"
                    }
                }

                // Search bar
                div {
                    class: "px-4 pb-4",
                    div {
                        class: "relative",
                        input {
                            class: "w-full px-4 py-2 pl-10 bg-muted rounded-full text-sm focus:outline-none focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Search podcasts... (Enter to search)",
                            value: "{search_query}",
                            oninput: move |e| search_query.set(e.value()),
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter {
                                    evt.prevent_default();
                                    let q = search_query.read().clone();
                                    submitted_query.set(q);
                                }
                            }
                        }
                        div {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
                            dangerous_inner_html: icons::SEARCH
                        }
                    }
                }

                // Platform filter
                div {
                    class: "px-4 pb-3",
                    div {
                        class: "flex items-center justify-between",
                        div {
                            class: "flex gap-1.5",
                            for (value, label) in [("all", "All"), ("rss", "RSS/Podcast 2.0"), ("nostr", "Nostr Native")] {
                                button {
                                    class: if *selected_platform.read() == value {
                                        "px-3 py-1.5 rounded-full text-xs font-medium bg-primary text-primary-foreground"
                                    } else {
                                        "px-3 py-1.5 rounded-full text-xs font-medium bg-muted/50 hover:bg-muted text-muted-foreground"
                                    },
                                    onclick: move |_| selected_platform.set(value.to_string()),
                                    "{label}"
                                }
                            }
                        }
                        // Add Feed button (authenticated only, not on nostr-only filter)
                        if is_authenticated && *selected_platform.read() != "nostr" {
                            button {
                                class: "px-3 py-1.5 rounded-full text-xs font-medium bg-primary/10 text-primary hover:bg-primary/20 transition flex items-center gap-1.5",
                                onclick: move |_| show_add_feed_modal.set(true),
                                span { class: "text-sm", "+" }
                                "Add Feed"
                            }
                        }
                    }
                }

                // Tab navigation
                div {
                    class: "flex border-b border-border",
                    TabButton {
                        label: "Discover",
                        active: *active_tab.read() == PodcastTab::Discover,
                        onclick: move |_| active_tab.set(PodcastTab::Discover)
                    }
                    TabButton {
                        label: "Recent",
                        active: *active_tab.read() == PodcastTab::Recent,
                        onclick: move |_| active_tab.set(PodcastTab::Recent)
                    }
                    TabButton {
                        label: "Library",
                        active: *active_tab.read() == PodcastTab::Library,
                        onclick: move |_| active_tab.set(PodcastTab::Library)
                    }
                }
            }

            // Content
            div {
                class: "p-4",

                // Search results if submitted query is not empty
                if !submitted_query.read().is_empty() {
                    PodcastSearchResults {
                        query: submitted_query.read().clone(),
                        platform: selected_platform.read().clone()
                    }
                } else {
                    // Tab content
                    match *active_tab.read() {
                        PodcastTab::Discover => rsx! {
                            DiscoverTab {
                                platform: selected_platform.read().clone()
                            }
                        },
                        PodcastTab::Recent => rsx! {
                            RecentFromSubscriptions {
                                platform: selected_platform.read().clone()
                            }
                        },
                        PodcastTab::Library => rsx! {
                            LibraryTab {
                                platform: selected_platform.read().clone()
                            }
                        },
                    }
                }
            }
        }

        // Add Feed Modal
        if *show_add_feed_modal.read() {
            PodcastAddFeedModal {
                on_close: move |_| show_add_feed_modal.set(false),
                on_added: move |_url| {
                    show_add_feed_modal.set(false);
                    // Subscriptions will auto-refresh on next use
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum PodcastTab {
    Discover,
    Recent,
    Library,
}

#[derive(Props, Clone, PartialEq)]
struct TabButtonProps {
    label: &'static str,
    active: bool,
    onclick: EventHandler<MouseEvent>,
}

#[component]
fn TabButton(props: TabButtonProps) -> Element {
    let class = if props.active {
        "flex-1 py-3 text-sm font-medium text-primary border-b-2 border-primary"
    } else {
        "flex-1 py-3 text-sm font-medium text-muted-foreground hover:text-foreground border-b-2 border-transparent"
    };

    rsx! {
        button {
            class: "{class}",
            onclick: move |e| props.onclick.call(e),
            "{props.label}"
        }
    }
}

/// Discover tab - featured content
#[derive(Props, Clone, PartialEq)]
struct DiscoverTabProps {
    platform: String,
}

#[component]
fn DiscoverTab(props: DiscoverTabProps) -> Element {
    let mut selected_category = use_signal(|| None::<String>);

    rsx! {
        div {
            class: "space-y-6",

            // About section - contextual based on platform
            div {
                class: "bg-muted/50 rounded-lg p-4",
                h2 {
                    class: "font-semibold text-lg mb-2",
                    if props.platform == "nostr" {
                        "Nostr Native Podcasts"
                    } else if props.platform == "rss" {
                        "RSS/Podcast 2.0 Feeds"
                    } else {
                        "Podcast Discovery"
                    }
                }
                p {
                    class: "text-sm text-muted-foreground",
                    if props.platform == "nostr" {
                        "Podcasts published natively on Nostr (Kind 30054). These shows support Value4Value (V4V) payments, allowing you to directly support creators with sats."
                    } else if props.platform == "rss" {
                        "Your subscribed RSS/Podcast 2.0 feeds. Many support V4V payments through Podcasting 2.0 value blocks."
                    } else {
                        "Discover podcasts from both Nostr and RSS feeds. Subscribe to shows and stream episodes with V4V support."
                    }
                }
            }

            // Trending podcasts (single row, horizontal scroll)
            if props.platform != "nostr" {
                TrendingPodcasts {
                    category: None // Trending ignores category filter
                }
            }

            // Category filter tiles
            ApiCategoryTiles {
                selected: selected_category.read().clone(),
                on_select: move |cat: Option<String>| selected_category.set(cat)
            }

            // Browse podcasts by category (both RSS and Nostr)
            BrowsePodcastsByCategory {
                platform: props.platform.clone(),
                category: selected_category.read().clone()
            }
        }
    }
}

/// Category tiles for discovery (podverse-inspired)
#[derive(Props, Clone, PartialEq)]
struct CategoryTilesProps {
    selected: Option<String>,
    on_select: EventHandler<Option<String>>,
}

#[component]
fn CategoryTiles(props: CategoryTilesProps) -> Element {
    let categories = podcast_subscription::get_categories();

    rsx! {
        div {
            class: "space-y-2",
            h3 {
                class: "font-semibold text-sm",
                "Browse by Category"
            }
            div {
                class: "grid grid-cols-5 gap-2",
                // "All" tile
                button {
                    class: if props.selected.is_none() {
                        "p-2 rounded-lg text-center bg-primary text-primary-foreground text-xs font-medium transition"
                    } else {
                        "p-2 rounded-lg text-center bg-muted hover:bg-muted/80 text-foreground text-xs font-medium transition"
                    },
                    onclick: move |_| props.on_select.call(None),
                    "All"
                }
                // Category tiles
                for cat in categories {
                    {
                        let cat_name = cat.name.to_string();
                        let is_selected = props.selected.as_ref().map(|s| s == cat.name).unwrap_or(false);
                        rsx! {
                            button {
                                key: "{cat.name}",
                                class: if is_selected {
                                    "p-2 rounded-lg text-center bg-primary text-primary-foreground text-xs font-medium transition"
                                } else {
                                    "p-2 rounded-lg text-center bg-muted hover:bg-muted/80 text-foreground text-xs font-medium transition"
                                },
                                title: "{cat.description}",
                                onclick: move |_| props.on_select.call(Some(cat_name.clone())),
                                "{cat.name}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Trending podcasts from Podcast Index API
#[derive(Props, Clone, PartialEq)]
struct TrendingPodcastsProps {
    #[props(default)]
    category: Option<String>,
}

#[component]
fn TrendingPodcasts(props: TrendingPodcastsProps) -> Element {
    let mut podcasts = use_signal(|| None::<Vec<podcast_index::PodcastFeed>>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Use signal for category to enable reactive re-fetching when category changes
    let mut category_signal = use_signal(|| props.category.clone());

    // Sync props to signal when category changes
    if *category_signal.read() != props.category {
        category_signal.set(props.category.clone());
    }

    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = nostr_client::has_signer();

        // Wait for nostr client AND signer - NIP-98 auth requires a signer
        if !client_initialized || !has_signer {
            loading.set(false); // Don't show loading if not authenticated
            return;
        }

        // Read category signal inside effect to establish reactive dependency
        let cat = category_signal.read().clone();
        loading.set(true);
        error.set(None);

        spawn(async move {
            // Fetch enough to fill screen (12 gives good coverage)
            match podcast_index::get_trending(Some(12), cat.as_deref()).await {
                Ok(feeds) => {
                    log::info!("Fetched {} trending podcasts", feeds.len());
                    podcasts.set(Some(feeds));
                    loading.set(false);
                }
                Err(e) => {
                    log::error!("Failed to fetch trending podcasts: {}", e);
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    rsx! {
        div {
            class: "space-y-3",
            // Header with "See All" link
            div {
                class: "flex items-center justify-between",
                h3 {
                    class: "font-semibold",
                    "Trending Podcasts"
                    if props.category.is_some() {
                        span {
                            class: "text-xs text-muted-foreground font-normal ml-2",
                            "(filtered)"
                        }
                    }
                }
                Link {
                    to: Route::PodcastTrending {},
                    class: "text-sm text-primary hover:underline",
                    "See All →"
                }
            }

            if *loading.read() {
                // Single row skeleton with horizontal scroll
                div {
                    class: "flex gap-3 overflow-x-auto pb-2 -mx-4 px-4 scrollbar-hide",
                    for i in 0..12 {
                        div {
                            key: "{i}",
                            class: "flex-shrink-0 w-28 sm:w-32",
                            TrendingPodcastCardSkeleton {}
                        }
                    }
                }
            } else if let Some(ref err) = *error.read() {
                div {
                    class: "text-sm text-muted-foreground text-center py-4",
                    "Failed to load trending: {err}"
                }
            } else if let Some(ref feeds) = *podcasts.read() {
                if feeds.is_empty() {
                    div {
                        class: "text-sm text-muted-foreground text-center py-4",
                        "No trending podcasts found"
                    }
                } else {
                    // Single row with horizontal scroll
                    div {
                        class: "flex gap-3 overflow-x-auto pb-2 -mx-4 px-4 scrollbar-hide",
                        for feed in feeds.iter() {
                            div {
                                key: "{feed.id}",
                                class: "flex-shrink-0 w-28 sm:w-32",
                                TrendingPodcastCard {
                                    feed: feed.clone()
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Browse podcasts by category - shows both RSS and Nostr podcasts
#[derive(Props, Clone, PartialEq)]
struct BrowsePodcastsByCategoryProps {
    platform: String,
    category: Option<String>,
}

#[component]
fn BrowsePodcastsByCategory(props: BrowsePodcastsByCategoryProps) -> Element {
    let show_rss = props.platform == "all" || props.platform == "rss";
    let show_nostr = props.platform == "all" || props.platform == "nostr";

    // Use signals to track props - these will trigger resource re-fetches when they change
    let mut category_signal = use_signal(|| props.category.clone());
    let mut platform_signal = use_signal(|| props.platform.clone());

    // Update signals when props change
    if *category_signal.read() != props.category {
        category_signal.set(props.category.clone());
    }
    if *platform_signal.read() != props.platform {
        platform_signal.set(props.platform.clone());
    }

    // For display label
    let category_for_label = props.category.clone();

    // Fetch RSS podcasts from Podcast Index
    let rss_podcasts = use_resource(move || {
        let cat = category_signal.read().clone();
        let plat = platform_signal.read().clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = nostr_client::has_signer();
        async move {
            let should_fetch = plat == "all" || plat == "rss";
            if !should_fetch || !client_initialized || !has_signer {
                return Ok::<_, String>(Vec::new());
            }

            log::info!("Fetching trending podcasts with category: {:?}", cat);

            // Use trending endpoint with category filter
            if let Some(ref category) = cat {
                podcast_index::get_trending(Some(25), Some(category)).await
            } else {
                podcast_index::get_trending(Some(25), None).await
            }
        }
    });

    // Fetch Nostr podcasts
    let nostr_podcasts = use_resource(move || {
        let cat = category_signal.read().clone();
        let plat = platform_signal.read().clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        async move {
            let should_fetch = plat == "all" || plat == "nostr";
            if !should_fetch || !client_initialized {
                return Ok::<_, String>(Vec::new());
            }

            // Fetch all nostr podcast shows
            match fetch_nostr_podcast_shows().await {
                Ok(shows) => {
                    // Filter by category if specified
                    if let Some(ref category) = cat {
                        let cat_lower = category.to_lowercase();
                        let filtered: Vec<_> = shows.into_iter()
                            .filter(|s| s.categories.iter().any(|c| c.to_lowercase().contains(&cat_lower)))
                            .collect();
                        Ok(filtered)
                    } else {
                        Ok(shows)
                    }
                }
                Err(e) => Err(e)
            }
        }
    });

    // Read results
    let rss_result = rss_podcasts.read();
    let rss_feeds = if show_rss {
        rss_result.as_ref().and_then(|r| r.as_ref().ok())
    } else {
        None
    };

    let nostr_result = nostr_podcasts.read();
    let nostr_shows = if show_nostr {
        nostr_result.as_ref().and_then(|r| r.as_ref().ok())
    } else {
        None
    };

    let rss_loading = show_rss && rss_result.is_none();
    let nostr_loading = show_nostr && nostr_result.is_none();
    let is_loading = rss_loading || nostr_loading;

    let rss_count = rss_feeds.map(|f| f.len()).unwrap_or(0);
    let nostr_count = nostr_shows.map(|s| s.len()).unwrap_or(0);
    let total_count = rss_count + nostr_count;

    let category_label = category_for_label.unwrap_or_else(|| "All".to_string());

    rsx! {
        div {
            class: "space-y-3",

            // Header
            div {
                class: "flex items-center justify-between",
                h3 {
                    class: "font-semibold",
                    "Browse Podcasts"
                    span {
                        class: "text-xs text-muted-foreground font-normal ml-2",
                        "({category_label})"
                    }
                }
                if !is_loading && total_count > 0 {
                    span {
                        class: "text-xs text-muted-foreground",
                        "{total_count} podcast(s)"
                    }
                }
            }

            if is_loading {
                // Loading skeletons
                div {
                    class: "space-y-2",
                    for i in 0..8 {
                        BrowsePodcastRowSkeleton { key: "{i}" }
                    }
                }
            } else if total_count == 0 {
                div {
                    class: "text-center py-8 text-muted-foreground",
                    "No podcasts found for this category."
                }
            } else {
                div {
                    class: "space-y-2",

                    // Show RSS podcasts first
                    if let Some(feeds) = rss_feeds {
                        for feed in feeds.iter() {
                            BrowsePodcastRow {
                                key: "rss-{feed.id}",
                                title: feed.title.clone(),
                                author: feed.author.clone(),
                                image: feed.get_image().map(String::from),
                                is_nostr: false,
                                has_v4v: feed.has_v4v(),
                                route: Route::PodcastRssFeedDetail { podcast_id: feed.id.to_string() }
                            }
                        }
                    }

                    // Then show Nostr podcasts
                    if let Some(shows) = nostr_shows {
                        for show in shows.iter() {
                            BrowsePodcastRow {
                                key: "nostr-{show.id}",
                                title: show.title.clone(),
                                author: show.author.clone(),
                                image: show.image.clone(),
                                is_nostr: true,
                                has_v4v: true, // Nostr podcasts support V4V
                                route: Route::PodcastNostrDetail { naddr: show.id.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Row for a podcast in the browse list
#[derive(Props, Clone, PartialEq)]
struct BrowsePodcastRowProps {
    title: String,
    author: Option<String>,
    image: Option<String>,
    is_nostr: bool,
    has_v4v: bool,
    route: Route,
}

#[component]
fn BrowsePodcastRow(props: BrowsePodcastRowProps) -> Element {
    let image = props.image.clone()
        .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/shapes/svg?seed={}", props.title));

    rsx! {
        Link {
            to: props.route.clone(),
            class: "flex items-center gap-3 p-2 rounded-lg hover:bg-muted transition",

            // Podcast image
            img {
                src: "{image}",
                alt: "{props.title}",
                class: "w-14 h-14 rounded-lg object-cover flex-shrink-0"
            }

            // Info
            div {
                class: "flex-1 min-w-0",
                div {
                    class: "font-medium truncate",
                    "{props.title}"
                }
                div {
                    class: "text-sm text-muted-foreground truncate",
                    if let Some(ref author) = props.author {
                        "{author}"
                    } else if props.is_nostr {
                        "Nostr Podcast"
                    } else {
                        "RSS Podcast"
                    }
                }
            }

            // Badges
            div {
                class: "flex items-center gap-1.5 flex-shrink-0",
                if props.has_v4v {
                    span {
                        class: "px-1.5 py-0.5 text-[10px] font-medium bg-amber-500/20 text-amber-500 rounded",
                        "V4V"
                    }
                }
                if props.is_nostr {
                    span {
                        class: "px-1.5 py-0.5 text-[10px] font-medium bg-purple-500/20 text-purple-400 rounded",
                        "Nostr"
                    }
                } else {
                    span {
                        class: "px-1.5 py-0.5 text-[10px] font-medium bg-green-500/20 text-green-400 rounded",
                        "RSS"
                    }
                }
            }
        }
    }
}

#[component]
fn BrowsePodcastRowSkeleton() -> Element {
    rsx! {
        div {
            class: "flex items-center gap-3 p-2 animate-pulse",
            div { class: "w-14 h-14 rounded-lg bg-muted flex-shrink-0" }
            div {
                class: "flex-1 min-w-0 space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
            }
            div {
                class: "flex gap-1.5",
                div { class: "w-8 h-4 bg-muted rounded" }
                div { class: "w-8 h-4 bg-muted rounded" }
            }
        }
    }
}

/// Card for a trending podcast from Podcast Index
#[derive(Props, Clone, PartialEq)]
struct TrendingPodcastCardProps {
    feed: podcast_index::PodcastFeed,
}

#[component]
fn TrendingPodcastCard(props: TrendingPodcastCardProps) -> Element {
    let feed = &props.feed;
    let image = feed.get_image();
    let podcast_id = feed.id.to_string();

    rsx! {
        Link {
            to: Route::PodcastRssFeedDetail { podcast_id },
            class: "group block bg-card border border-border rounded-lg overflow-hidden hover:border-primary/50 transition",

            // Image
            div {
                class: "relative aspect-square bg-muted",
                if let Some(img) = image {
                    img {
                        src: "{img}",
                        alt: "{feed.title}",
                        class: "w-full h-full object-cover",
                        loading: "lazy"
                    }
                } else {
                    div {
                        class: "w-full h-full flex items-center justify-center text-muted-foreground",
                        dangerous_inner_html: icons::PODCAST
                    }
                }

                // V4V badge
                if feed.has_v4v() {
                    div {
                        class: "absolute top-2 left-2 px-1.5 py-0.5 text-[10px] font-semibold bg-amber-500/90 text-white rounded",
                        "V4V"
                    }
                }
            }

            // Title
            div {
                class: "p-2",
                p {
                    class: "text-xs font-medium line-clamp-2",
                    "{feed.title}"
                }
                if let Some(ref author) = feed.author {
                    p {
                        class: "text-[10px] text-muted-foreground truncate mt-0.5",
                        "{author}"
                    }
                }
            }
        }
    }
}

#[component]
fn TrendingPodcastCardSkeleton() -> Element {
    rsx! {
        div {
            class: "bg-card border border-border rounded-lg overflow-hidden animate-pulse",
            div { class: "aspect-square bg-muted" }
            div {
                class: "p-2 space-y-1",
                div { class: "h-3 bg-muted rounded w-full" }
                div { class: "h-2 bg-muted rounded w-2/3" }
            }
        }
    }
}

/// Category tiles from Podcast Index API
#[derive(Props, Clone, PartialEq)]
struct ApiCategoryTilesProps {
    selected: Option<String>,
    on_select: EventHandler<Option<String>>,
}

#[component]
fn ApiCategoryTiles(props: ApiCategoryTilesProps) -> Element {
    let mut categories = use_signal(|| None::<Vec<podcast_index::Category>>);
    let mut loading = use_signal(|| true);

    // Fetch categories (waits for client AND signer - NIP-98 auth required)
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = nostr_client::has_signer();

        // Wait for nostr client AND signer - NIP-98 auth requires a signer
        if !client_initialized || !has_signer {
            loading.set(false); // Don't show loading if not authenticated
            return;
        }

        spawn(async move {
            match podcast_index::get_categories().await {
                Ok(cats) => {
                    log::info!("Fetched {} categories from Podcast Index", cats.len());
                    categories.set(Some(cats));
                }
                Err(e) => {
                    log::error!("Failed to fetch categories: {}", e);
                }
            }
            loading.set(false);
        });
    });

    // Popular categories to show first (subset of all)
    let popular_cats = ["Technology", "Business", "Comedy", "News", "Science", "Education", "Sports", "Health & Fitness", "True Crime", "Music"];

    // Pre-compute category buttons outside rsx! to avoid let-in-rsx issues
    let is_loading = *loading.read();
    let cats_data = categories.read();

    let category_buttons: Vec<(String, String, bool)> = if let Some(ref cats) = *cats_data {
        popular_cats.iter()
            .filter_map(|display_name| {
                cats.iter()
                    .find(|c| c.name.to_lowercase().contains(&display_name.to_lowercase()))
                    .map(|api_cat| {
                        let api_name = api_cat.name.clone();
                        let is_selected = props.selected.as_ref().map(|s| s == &api_name).unwrap_or(false);
                        (display_name.to_string(), api_name, is_selected)
                    })
            })
            .collect()
    } else {
        Vec::new()
    };

    let has_categories = !category_buttons.is_empty();

    rsx! {
        div {
            class: "space-y-2",
            h3 {
                class: "font-semibold text-sm",
                "Browse by Category"
            }

            if is_loading {
                div {
                    class: "flex gap-2 overflow-x-auto pb-2",
                    for i in 0..8 {
                        div {
                            key: "{i}",
                            class: "px-3 py-1.5 rounded-full bg-muted animate-pulse w-20 h-7"
                        }
                    }
                }
            } else if has_categories {
                div {
                    class: "flex gap-2 overflow-x-auto pb-2 scrollbar-thin scrollbar-thumb-muted",
                    // All button
                    button {
                        class: if props.selected.is_none() {
                            "px-3 py-1.5 rounded-full text-xs font-medium bg-primary text-primary-foreground whitespace-nowrap flex-shrink-0"
                        } else {
                            "px-3 py-1.5 rounded-full text-xs font-medium bg-muted hover:bg-muted/80 text-foreground whitespace-nowrap flex-shrink-0"
                        },
                        onclick: move |_| props.on_select.call(None),
                        "All"
                    }

                    // Render pre-computed category buttons
                    for (display_name, api_name, is_selected) in category_buttons.iter() {
                        button {
                            key: "{api_name}",
                            class: if *is_selected {
                                "px-3 py-1.5 rounded-full text-xs font-medium bg-primary text-primary-foreground whitespace-nowrap flex-shrink-0"
                            } else {
                                "px-3 py-1.5 rounded-full text-xs font-medium bg-muted hover:bg-muted/80 text-foreground whitespace-nowrap flex-shrink-0"
                            },
                            onclick: {
                                let api_name = api_name.clone();
                                move |_| props.on_select.call(Some(api_name.clone()))
                            },
                            "{display_name}"
                        }
                    }
                }
            }
        }
    }
}

/// Nostr shows tab
#[component]
fn NostrShowsTab() -> Element {
    rsx! {
        NostrPodcastShows {}
    }
}

/// Recent episodes tab (legacy - used in Discover)
#[derive(Props, Clone, PartialEq)]
struct RecentEpisodesTabProps {
    platform: String,
}

#[component]
fn RecentEpisodesTab(props: RecentEpisodesTabProps) -> Element {
    rsx! {
        RecentEpisodesMerged {
            platform: props.platform
        }
    }
}

/// Recent tab props
#[derive(Props, Clone, PartialEq)]
struct RecentFromSubscriptionsProps {
    platform: String,
}

/// Recent tab - shows recent episodes from subscribed podcasts only
#[component]
fn RecentFromSubscriptions(props: RecentFromSubscriptionsProps) -> Element {
    let is_authenticated = auth_store::is_authenticated();
    let show_rss = props.platform == "all" || props.platform == "rss";
    let show_nostr = props.platform == "all" || props.platform == "nostr";

    if !is_authenticated {
        return rsx! {
            div {
                class: "text-center py-16 space-y-4",
                div {
                    class: "text-4xl",
                    "🎧"
                }
                h3 {
                    class: "text-lg font-semibold",
                    "Sign in to see recent episodes"
                }
                p {
                    class: "text-muted-foreground text-sm max-w-sm mx-auto",
                    "Subscribe to podcasts and see their latest episodes here."
                }
            }
        };
    }

    // Separate state for RSS and Nostr episodes
    let mut rss_episodes = use_signal(|| None::<Vec<DisplayEpisode>>);
    let mut nostr_episodes = use_signal(|| None::<Vec<DisplayEpisode>>);
    let mut rss_loading = use_signal(|| true);
    let mut nostr_loading = use_signal(|| true);

    // Fetch RSS episodes from subscriptions using Podcast Index API
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = nostr_client::has_signer();
        let subs_loaded = *podcast_subscription::SUBSCRIPTIONS_LOADED.read();

        // Need auth for Podcast Index API, and wait for subscriptions to load
        if !client_initialized || !has_signer || !subs_loaded {
            return;
        }

        rss_loading.set(true);
        spawn(async move {
            let subscriptions = podcast_subscription::get_rss_subscriptions();
            // Build a set of numeric IDs for live episode filtering
            let podcast_ids_set: std::collections::HashSet<u64> = subscriptions
                .iter()
                .filter_map(|s| s.numeric_id())
                .collect();
            let mut all_episodes = Vec::new();

            // Fetch live episodes first (filtered to subscribed podcasts)
            if let Ok(live_episodes) = podcast_index::get_live_episodes(Some(50)).await {
                for ep in live_episodes {
                    // Only include live episodes from subscribed podcasts
                    if let Some(feed_id) = ep.feed_id {
                        if podcast_ids_set.contains(&feed_id) {
                            // Get feed metadata for the live episode
                            if let Ok(feed) = podcast_index::get_podcast_by_id(feed_id).await {
                                all_episodes.push(DisplayEpisode::from_podcast_index_live_episode(&ep, &feed));
                            }
                        }
                    }
                }
            }

            // Fetch regular episodes from subscriptions concurrently
            // Handle both GUID-only and ID-cached subscriptions
            let fetch_futures: Vec<_> = subscriptions.iter().take(20).map(|sub| {
                let guid = sub.podcast_guid.clone();
                let id = sub.numeric_id();
                async move {
                    // Try by ID first (cached, more efficient), then by GUID
                    let feed_result = if let Some(id) = id {
                        podcast_index::get_podcast_by_id(id).await
                    } else if let Some(ref guid) = guid {
                        podcast_index::get_podcast_by_guid(guid).await
                    } else {
                        return None;
                    };

                    let Ok(feed) = feed_result else {
                        log::warn!("Failed to fetch podcast: {:?}/{:?}", guid, id);
                        return None;
                    };

                    // Fetch episodes by feed ID (need the ID from the feed response)
                    let episodes = podcast_index::get_episodes_by_feed_id(feed.id, Some(5)).await
                        .unwrap_or_default();

                    let display_episodes: Vec<DisplayEpisode> = episodes
                        .iter()
                        .map(|ep| DisplayEpisode::from_podcast_index_episode(ep, &feed))
                        .collect();
                    Some(display_episodes)
                }
            }).collect();

            // Execute all podcast fetches concurrently
            let results = futures::future::join_all(fetch_futures).await;

            // Collect successful results
            for result in results.into_iter().flatten() {
                all_episodes.extend(result);
            }

            rss_episodes.set(Some(all_episodes));
            rss_loading.set(false);
        });
    });

    // Fetch Nostr episodes from subscriptions (waits for client and subscriptions)
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let subs_loaded = *podcast_subscription::SUBSCRIPTIONS_LOADED.read();
        if !client_initialized || !subs_loaded {
            return;
        }

        nostr_loading.set(true);
        spawn(async move {
            let nostr_coords = podcast_subscription::get_nostr_podcasts();
            let mut all_episodes = Vec::new();

            // Extract pubkeys from subscription coordinates (format: "kind:pubkey:d-tag")
            let subscribed_pubkeys: Vec<String> = nostr_coords
                .iter()
                .filter_map(|coord| {
                    let parts: Vec<&str> = coord.split(':').collect();
                    if parts.len() >= 2 {
                        Some(parts[1].to_lowercase())
                    } else {
                        None
                    }
                })
                .collect();

            if subscribed_pubkeys.is_empty() {
                nostr_episodes.set(Some(Vec::new()));
                nostr_loading.set(false);
                return;
            }

            // Fetch recent nostr episodes and filter by subscribed pubkeys
            if let Ok(episodes) = fetch_recent_nostr_episodes().await {
                for ep in episodes {
                    // Check if episode's source pubkey matches any subscription
                    let is_subscribed = match &ep.source {
                        crate::stores::nostr_music::TrackSource::NostrPodcast { pubkey, .. } => {
                            subscribed_pubkeys.iter().any(|sub_pk| sub_pk == &pubkey.to_lowercase())
                        }
                        _ => false,
                    };
                    if is_subscribed {
                        all_episodes.push(ep);
                    }
                }
            }

            nostr_episodes.set(Some(all_episodes));
            nostr_loading.set(false);
        });
    });

    // Merge and sort episodes based on platform filter
    // Live episodes always sort to the top, then by created_at
    let merged_episodes = {
        let mut all = Vec::new();
        if show_rss {
            if let Some(ref eps) = *rss_episodes.read() {
                all.extend(eps.clone());
            }
        }
        if show_nostr {
            if let Some(ref eps) = *nostr_episodes.read() {
                all.extend(eps.clone());
            }
        }
        all.sort_by(|a, b| {
            // Live episodes always come first
            match (a.is_live, b.is_live) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => b.created_at.cmp(&a.created_at), // Then by date, newest first
            }
        });
        all
    };

    let has_any_data = (show_rss && rss_episodes.read().is_some()) ||
                       (show_nostr && nostr_episodes.read().is_some());
    // Check if we're still initializing - show skeleton until client is ready and subscriptions loaded
    let client_ready = *nostr_client::CLIENT_INITIALIZED.read();
    let has_signer = nostr_client::has_signer();
    let subs_loading = *podcast_subscription::SUBSCRIPTIONS_LOADING.read();
    let subs_loaded = *podcast_subscription::SUBSCRIPTIONS_LOADED.read();
    // Show loading if: client not ready, no signer, subscriptions not loaded, or still fetching episodes
    let is_initial_loading = !client_ready || !has_signer || !subs_loaded || subs_loading ||
                             (show_rss && *rss_loading.read() && rss_episodes.read().is_none()) ||
                             (show_nostr && *nostr_loading.read() && nostr_episodes.read().is_none() && !show_rss);
    let nostr_still_loading = show_nostr && (!client_ready || *nostr_loading.read());

    if is_initial_loading && !has_any_data {
        return rsx! {
            div {
                class: "space-y-1",
                for i in 0..5 {
                    PodcastEpisodeCardSkeleton { key: "{i}" }
                }
            }
        };
    }

    if merged_episodes.is_empty() {
        let platform_msg = match props.platform.as_str() {
            "rss" => "RSS/Podcast 2.0",
            "nostr" => "Nostr",
            _ => "any",
        };
        return rsx! {
            div {
                class: "text-center py-16 space-y-4",
                div {
                    class: "text-4xl",
                    "📭"
                }
                h3 {
                    class: "text-lg font-semibold",
                    "No recent episodes"
                }
                p {
                    class: "text-muted-foreground text-sm max-w-sm mx-auto",
                    "Subscribe to {platform_msg} podcasts in the Discover tab to see their latest episodes here."
                }
            }
        };
    }

    rsx! {
        div {
            class: "space-y-1",
            for ep in merged_episodes.iter().take(50) {
                PodcastEpisodeCard {
                    key: "{ep.id}",
                    episode: ep.clone(),
                    show_podcast_title: true
                }
            }

            if nostr_still_loading {
                div {
                    class: "text-xs text-muted-foreground text-center py-2",
                    "Loading Nostr episodes..."
                }
            }
        }
    }
}

/// Library tab props
#[derive(Props, Clone, PartialEq)]
struct LibraryTabProps {
    platform: String,
}

/// Library tab - shows subscribed podcasts
#[component]
fn LibraryTab(props: LibraryTabProps) -> Element {
    let is_authenticated = auth_store::is_authenticated();
    let show_rss = props.platform == "all" || props.platform == "rss";
    let show_nostr = props.platform == "all" || props.platform == "nostr";

    if !is_authenticated {
        return rsx! {
            div {
                class: "text-center py-16 space-y-4",
                div {
                    class: "text-4xl",
                    "📚"
                }
                h3 {
                    class: "text-lg font-semibold",
                    "Sign in to view your library"
                }
                p {
                    class: "text-muted-foreground text-sm max-w-sm mx-auto",
                    "Subscribe to podcasts and access them here."
                }
            }
        };
    }

    let all_subscriptions = podcast_subscription::get_subscriptions();
    let is_loading = podcast_subscription::is_loading();

    // Filter subscriptions based on platform
    let subscriptions: Vec<_> = all_subscriptions.into_iter().filter(|sub| {
        if sub.is_rss() {
            show_rss
        } else if sub.is_nostr() {
            show_nostr
        } else {
            true // Show unknown types in all views
        }
    }).collect();

    if is_loading {
        return rsx! {
            div {
                class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-3",
                for i in 0..8 {
                    TrendingPodcastCardSkeleton { key: "{i}" }
                }
            }
        };
    }

    if subscriptions.is_empty() {
        let platform_msg = match props.platform.as_str() {
            "rss" => "RSS/Podcast 2.0",
            "nostr" => "Nostr",
            _ => "",
        };
        return rsx! {
            div {
                class: "text-center py-16 space-y-4",
                div {
                    class: "text-4xl",
                    "🎙️"
                }
                h3 {
                    class: "text-lg font-semibold",
                    if platform_msg.is_empty() {
                        "Your library is empty"
                    } else {
                        "No {platform_msg} subscriptions"
                    }
                }
                p {
                    class: "text-muted-foreground text-sm max-w-sm mx-auto",
                    "Subscribe to podcasts from the Discover tab to build your library."
                }
            }
        };
    }

    rsx! {
        div {
            class: "space-y-4",
            div {
                class: "text-sm text-muted-foreground",
                "{subscriptions.len()} subscribed podcast(s)"
            }
            div {
                class: "space-y-2",
                for sub in subscriptions {
                    SubscribedPodcastRow {
                        subscription: sub.clone()
                    }
                }
            }
        }
    }
}

/// Row component for subscribed podcast in Library
#[derive(Props, Clone, PartialEq)]
struct SubscribedPodcastRowProps {
    subscription: podcast_subscription::PodcastSubscription,
}

/// Metadata loaded for a subscribed podcast (unified for RSS and Nostr)
#[derive(Clone, Debug)]
enum SubscriptionMetadata {
    Rss(Box<podcast_index::PodcastFeed>),
    Nostr { title: String, image: Option<String>, author: Option<String> },
}

#[component]
fn SubscribedPodcastRow(props: SubscribedPodcastRowProps) -> Element {
    let sub = props.subscription.clone();
    let is_nostr = sub.nostr_coordinate.is_some();

    // Fetch podcast metadata - RSS from Podcast Index, Nostr from relays
    let podcast_data = use_resource(move || {
        let podcast_id = sub.podcast_id;
        let podcast_guid = sub.podcast_guid.clone();
        let nostr_coord = sub.nostr_coordinate.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = nostr_client::has_signer();
        async move {
            // RSS podcast - fetch from Podcast Index API (by ID or GUID)
            if podcast_id.is_some() || podcast_guid.is_some() {
                if !client_initialized || !has_signer {
                    return None;
                }

                // Try by ID first (more efficient), then by GUID
                if let Some(id) = podcast_id {
                    match podcast_index::get_podcast_by_id(id).await {
                        Ok(feed) => return Some(SubscriptionMetadata::Rss(Box::new(feed))),
                        Err(e) => {
                            log::warn!("Failed to fetch RSS podcast by ID {}: {}", id, e);
                            // Fall through to try GUID if available
                        }
                    }
                }

                // Try by GUID (used when subscription loaded from Nostr with only GUID)
                if let Some(ref guid) = podcast_guid {
                    match podcast_index::get_podcast_by_guid(guid).await {
                        Ok(feed) => return Some(SubscriptionMetadata::Rss(Box::new(feed))),
                        Err(e) => {
                            log::warn!("Failed to fetch RSS podcast by GUID {}: {}", guid, e);
                            return None;
                        }
                    }
                }

                return None;
            }

            // Nostr podcast - fetch from relays using coordinate
            if let Some(coord) = nostr_coord {
                if !client_initialized {
                    return None;
                }

                // Parse coordinate: "kind:pubkey:d-tag"
                let parts: Vec<&str> = coord.split(':').collect();
                if parts.len() >= 3 {
                    // Parse kind - skip this coordinate if invalid (don't default to 0)
                    let kind_num: u16 = match parts[0].parse() {
                        Ok(k) => k,
                        Err(_) => {
                            log::warn!("Invalid kind in coordinate '{}': could not parse '{}'", coord, parts[0]);
                            return None;
                        }
                    };
                    let pubkey_hex = parts[1];
                    let d_tag = parts[2..].join(":"); // d-tag might contain colons

                    // Fetch the event from Nostr
                    if let Ok(pubkey) = nostr_sdk::PublicKey::from_hex(pubkey_hex) {
                        use nostr_sdk::SingleLetterTag;
                        let filter = Filter::new()
                            .author(pubkey)
                            .kind(Kind::from(kind_num))
                            .custom_tag(
                                SingleLetterTag::from_char('d').unwrap(),
                                d_tag
                            )
                            .limit(1);

                        if let Ok(events) = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5)).await {
                            if let Some(event) = events.into_iter().next() {
                                // Parse podcast metadata from the event
                                if let Ok(metadata) = podcast::parse_podcast_metadata(&event) {
                                    return Some(SubscriptionMetadata::Nostr {
                                        title: metadata.title,
                                        image: metadata.image,
                                        author: metadata.author,
                                    });
                                }
                            }
                        }
                    }
                }
                return None;
            }

            None
        }
    });

    // Get display values from fetched data or fallbacks
    let data = podcast_data.read();
    let metadata = data.as_ref().and_then(|r| r.as_ref());

    let (title, image, author) = match metadata {
        Some(SubscriptionMetadata::Rss(feed)) => (
            feed.title.clone(),
            feed.get_image().map(String::from)
                .or_else(|| props.subscription.image.clone())
                .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/shapes/svg?seed={}", feed.title)),
            feed.author.clone(),
        ),
        Some(SubscriptionMetadata::Nostr { title, image, author }) => (
            title.clone(),
            image.clone()
                .or_else(|| props.subscription.image.clone())
                .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/shapes/svg?seed={}", title)),
            author.clone(),
        ),
        None => {
            // Fallback when no metadata loaded yet
            let fallback_title = props.subscription.title.clone()
                .unwrap_or_else(|| {
                    if let Some(id) = props.subscription.podcast_id {
                        format!("Podcast #{}", id)
                    } else if props.subscription.podcast_guid.is_some() {
                        "Loading podcast...".to_string()
                    } else {
                        "Nostr Podcast".to_string()
                    }
                });
            let fallback_image = props.subscription.image.clone()
                .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/shapes/svg?seed={}", fallback_title));
            (fallback_title, fallback_image, None)
        }
    };

    // Determine the route based on subscription type
    // For RSS podcasts, prefer subscription's podcast_id, then fall back to ID from fetched feed
    let route = props.subscription.podcast_id
        .map(|podcast_id| Route::PodcastRssFeedDetail {
            podcast_id: podcast_id.to_string()
        })
        .or_else(|| {
            // If we fetched by GUID, get the ID from the fetched feed
            if let Some(SubscriptionMetadata::Rss(ref feed)) = metadata {
                Some(Route::PodcastRssFeedDetail {
                    podcast_id: feed.id.to_string()
                })
            } else {
                None
            }
        })
        .or_else(|| props.subscription.nostr_coordinate.as_ref().map(|coord| Route::PodcastNostrDetail {
            naddr: coord.clone()
        }));

    // Show loading state while fetching
    let is_loading = data.is_none();

    if let Some(route) = route {
        rsx! {
            Link {
                to: route,
                class: "flex items-center gap-3 p-2 rounded-lg hover:bg-muted transition",
                if is_loading {
                    // Skeleton while loading
                    div {
                        class: "w-12 h-12 rounded bg-muted animate-pulse"
                    }
                    div {
                        class: "flex-1 min-w-0 space-y-1",
                        div { class: "h-4 bg-muted rounded w-32 animate-pulse" }
                        div { class: "h-3 bg-muted rounded w-20 animate-pulse" }
                    }
                } else {
                    img {
                        src: "{image}",
                        alt: "{title}",
                        class: "w-12 h-12 rounded object-cover"
                    }
                    div {
                        class: "flex-1 min-w-0",
                        div {
                            class: "font-medium truncate",
                            "{title}"
                        }
                        div {
                            class: "text-xs text-muted-foreground truncate",
                            if let Some(ref auth) = author {
                                "{auth}"
                            } else if is_nostr {
                                "Nostr Podcast"
                            } else {
                                "RSS Feed"
                            }
                        }
                    }
                }
            }
        }
    } else {
        // Check if this is a GUID-only subscription that's still loading or needs auth
        let has_guid = props.subscription.podcast_guid.is_some();
        let has_signer = nostr_client::has_signer();
        let needs_auth = has_guid && !has_signer;

        let (icon, message) = if needs_auth {
            ("🔐", "Sign in to load podcast")
        } else if has_guid && is_loading {
            ("⏳", "Loading podcast...")
        } else {
            ("?", "Invalid subscription")
        };

        rsx! {
            div {
                class: "flex items-center gap-3 p-2 rounded-lg opacity-50",
                div {
                    class: "w-12 h-12 rounded bg-muted flex items-center justify-center text-muted-foreground",
                    "{icon}"
                }
                div {
                    class: "flex-1 min-w-0",
                    div {
                        class: "font-medium truncate",
                        "{message}"
                    }
                }
            }
        }
    }
}

/// Search results component
#[derive(Props, Clone, PartialEq)]
struct PodcastSearchResultsProps {
    query: String,
    platform: String,
}

#[component]
fn PodcastSearchResults(props: PodcastSearchResultsProps) -> Element {
    // Clone props for use in rsx later
    let query_display = props.query.clone();
    let platform_display = props.platform.clone();

    // Use signals to track props - these will trigger resource re-fetches when they change
    let mut query_signal = use_signal(|| props.query.clone());
    let mut platform_signal = use_signal(|| props.platform.clone());

    // Update signals when props change
    if *query_signal.read() != props.query {
        query_signal.set(props.query.clone());
    }
    if *platform_signal.read() != props.platform {
        platform_signal.set(props.platform.clone());
    }

    // Search Podcast Index API (use_resource to re-run on query/platform change)
    let api_search = use_resource(move || {
        let q = query_signal.read().clone();
        let plat = platform_signal.read().clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = nostr_client::has_signer();
        async move {
            let should_search_api = plat == "all" || plat == "rss";
            if !should_search_api {
                return Ok::<_, String>(Vec::new());
            }

            // Wait for nostr client AND signer - NIP-98 auth requires a signer
            if !client_initialized || !has_signer {
                return Err("Waiting for authentication...".to_string());
            }

            match podcast_index::search_podcasts(&q, Some(20)).await {
                Ok(feeds) => {
                    log::info!("API search for '{}' returned {} results", q, feeds.len());
                    Ok(feeds)
                }
                Err(e) => {
                    log::warn!("API search failed: {}", e);
                    Err(e)
                }
            }
        }
    });

    // Search Nostr (use_resource to re-run on query/platform change)
    let nostr_search = use_resource(move || {
        let q = query_signal.read().clone();
        let plat = platform_signal.read().clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        async move {
            let should_search_nostr = plat == "all" || plat == "nostr";
            if !should_search_nostr {
                return Ok::<_, String>(Vec::new());
            }

            if !client_initialized {
                return Err("Waiting for client initialization...".to_string());
            }

            match search_nostr_podcasts(&q).await {
                Ok(shows) => {
                    log::info!("Nostr search returned {} results", shows.len());
                    Ok(shows)
                }
                Err(e) => {
                    log::warn!("Nostr search failed: {}", e);
                    Err(e)
                }
            }
        }
    });

    // Read API search results from resource
    let api_result = api_search.read();
    let api_feeds = api_result.as_ref().and_then(|r| r.as_ref().ok());
    let api_loading = api_result.is_none();

    // Read Nostr search results from resource
    let nostr_result = nostr_search.read();
    let nostr_shows = nostr_result.as_ref().and_then(|r| r.as_ref().ok());
    let nostr_loading = nostr_result.is_none();

    let api_count = api_feeds.map(|r| r.len()).unwrap_or(0);
    let nostr_count = nostr_shows.map(|r| r.len()).unwrap_or(0);
    let total_count = api_count + nostr_count;
    let has_any_data = api_feeds.is_some() || nostr_shows.is_some();
    let is_initial_loading = api_loading && !has_any_data;
    let nostr_still_loading = !*nostr_client::CLIENT_INITIALIZED.read() || nostr_loading;

    rsx! {
        div {
            class: "space-y-4",

            // Header with result count
            div {
                class: "flex items-center justify-between",
                h3 {
                    class: "font-semibold",
                    "Search Results"
                }
                if has_any_data {
                    span {
                        class: "text-sm text-muted-foreground",
                        "{total_count} result(s)"
                        if nostr_still_loading && platform_display != "rss" {
                            " (searching Nostr...)"
                        }
                    }
                }
            }

            // Loading state
            if is_initial_loading {
                div {
                    class: "space-y-2",
                    for i in 0..5 {
                        SearchResultRowSkeleton { key: "{i}" }
                    }
                }
            } else if total_count == 0 && has_any_data {
                div {
                    class: "text-center py-8 text-muted-foreground",
                    "No podcasts found for \"{query_display}\""
                }
            } else {
                // Unified line-by-line results
                div {
                    class: "divide-y divide-border",

                    // RSS/Podcast Index results
                    if let Some(feeds) = api_feeds {
                        for feed in feeds.iter() {
                            SearchResultRow {
                                key: "rss-{feed.id}",
                                title: feed.title.clone(),
                                author: feed.author.clone(),
                                image: feed.get_image().map(|s| s.to_string()),
                                has_v4v: feed.has_v4v(),
                                source: SearchResultSource::Rss,
                                route: Route::PodcastRssFeedDetail {
                                    podcast_id: feed.id.to_string()
                                }
                            }
                        }
                    }

                    // Nostr results
                    if let Some(shows) = nostr_shows {
                        for show in shows.iter() {
                            SearchResultRow {
                                key: "nostr-{show.id}",
                                title: show.title.clone(),
                                author: show.author.clone(),
                                image: show.image.clone(),
                                has_v4v: show.value.is_some(),
                                source: SearchResultSource::Nostr,
                                route: get_show_route(show)
                            }
                        }
                    }
                }

                // Nostr loading indicator
                if nostr_still_loading && platform_display != "rss" {
                    div {
                        class: "text-xs text-muted-foreground text-center py-2",
                        "Searching Nostr..."
                    }
                }
            }
        }
    }
}

/// Source type for search results
#[derive(Clone, Copy, PartialEq)]
enum SearchResultSource {
    Rss,
    Nostr,
}

/// Helper to get route from a PodcastShow
fn get_show_route(show: &PodcastShow) -> Route {
    match &show.source {
        crate::utils::podcast::PodcastSource::Nostr { coordinate, .. } => {
            Route::PodcastNostrDetail { naddr: coordinate.clone() }
        }
        crate::utils::podcast::PodcastSource::Rss { podcast_id, .. } => {
            if let Some(id) = podcast_id {
                Route::PodcastRssFeedDetail { podcast_id: id.to_string() }
            } else {
                // No podcast ID available, go to home
                Route::PodcastHome {}
            }
        }
    }
}

/// Unified search result row
#[derive(Props, Clone, PartialEq)]
struct SearchResultRowProps {
    title: String,
    author: Option<String>,
    image: Option<String>,
    has_v4v: bool,
    source: SearchResultSource,
    route: Route,
}

#[component]
fn SearchResultRow(props: SearchResultRowProps) -> Element {
    let image = props.image.clone()
        .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/shapes/svg?seed={}", props.title));

    rsx! {
        Link {
            to: props.route.clone(),
            class: "flex items-center gap-3 p-3 hover:bg-muted/50 transition",

            // Image
            img {
                src: "{image}",
                alt: "{props.title}",
                class: "w-12 h-12 rounded object-cover flex-shrink-0"
            }

            // Info
            div {
                class: "flex-1 min-w-0",
                div {
                    class: "font-medium truncate",
                    "{props.title}"
                }
                if let Some(ref author) = props.author {
                    div {
                        class: "text-sm text-muted-foreground truncate",
                        "{author}"
                    }
                }
            }

            // Badges
            div {
                class: "flex items-center gap-2 flex-shrink-0",

                // V4V badge
                if props.has_v4v {
                    span {
                        class: "px-1.5 py-0.5 text-[10px] font-semibold bg-amber-500/20 text-amber-500 rounded",
                        "V4V"
                    }
                }

                // Source badge
                match props.source {
                    SearchResultSource::Rss => rsx! {
                        span {
                            class: "px-1.5 py-0.5 text-[10px] font-semibold bg-green-500/20 text-green-500 rounded",
                            "RSS"
                        }
                    },
                    SearchResultSource::Nostr => rsx! {
                        span {
                            class: "px-1.5 py-0.5 text-[10px] font-semibold bg-purple-500/20 text-purple-500 rounded",
                            "Nostr"
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn SearchResultRowSkeleton() -> Element {
    rsx! {
        div {
            class: "flex items-center gap-3 p-3 animate-pulse",
            div {
                class: "w-12 h-12 rounded bg-muted flex-shrink-0"
            }
            div {
                class: "flex-1 space-y-2",
                div {
                    class: "h-4 bg-muted rounded w-3/4"
                }
                div {
                    class: "h-3 bg-muted rounded w-1/2"
                }
            }
        }
    }
}

/// Nostr podcast shows component
#[derive(Props, Clone, PartialEq)]
struct NostrPodcastShowsProps {
    #[props(default)]
    limit: Option<usize>,
}

#[component]
fn NostrPodcastShows(props: NostrPodcastShowsProps) -> Element {
    let mut shows = use_signal(|| None::<Vec<PodcastShow>>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Load shows when client is initialized
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        loading.set(true);
        error.set(None);

        spawn(async move {
            match fetch_nostr_podcast_shows().await {
                Ok(podcast_shows) => {
                    shows.set(Some(podcast_shows));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Show loading skeleton while waiting for client or loading
    if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && shows.read().is_none()) {
        return rsx! {
            div {
                class: "space-y-1",
                for i in 0..5 {
                    PodcastShowCardSkeleton {
                        key: "{i}"
                    }
                }
            }
        };
    }

    // Show error if any
    if let Some(err) = error.read().as_ref() {
        return rsx! {
            div {
                class: "text-center py-8 text-destructive",
                "Failed to load podcasts: {err}"
            }
        };
    }

    // Show results
    let shows_read = shows.read();
    if let Some(podcast_shows) = shows_read.as_ref() {
        let display_shows: Vec<_> = if let Some(limit) = props.limit {
            podcast_shows.iter().take(limit).cloned().collect()
        } else {
            podcast_shows.clone()
        };

        if display_shows.is_empty() {
            rsx! {
                div {
                    class: "text-center py-8 text-muted-foreground",
                    "No Nostr podcasts found. Be the first to publish one!"
                }
            }
        } else {
            rsx! {
                div {
                    class: "space-y-1",
                    for show in display_shows {
                        PodcastShowCard {
                            key: "{show.id}",
                            show: show.clone()
                        }
                    }
                }
            }
        }
    } else {
        rsx! {
            div {
                class: "space-y-1",
                for i in 0..5 {
                    PodcastShowCardSkeleton {
                        key: "{i}"
                    }
                }
            }
        }
    }
}

/// Recent Nostr episodes component
#[derive(Props, Clone, PartialEq)]
struct RecentNostrEpisodesProps {
    #[props(default)]
    limit: Option<usize>,
}

#[component]
fn RecentNostrEpisodes(props: RecentNostrEpisodesProps) -> Element {
    let mut episodes = use_signal(|| None::<Vec<DisplayEpisode>>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Load episodes when client is initialized
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        loading.set(true);
        error.set(None);

        spawn(async move {
            match fetch_recent_nostr_episodes().await {
                Ok(eps) => {
                    episodes.set(Some(eps));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Show loading skeleton while waiting for client or loading
    if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && episodes.read().is_none()) {
        return rsx! {
            div {
                class: "space-y-1",
                for i in 0..5 {
                    PodcastEpisodeCardSkeleton {
                        key: "{i}"
                    }
                }
            }
        };
    }

    // Show error if any
    if let Some(err) = error.read().as_ref() {
        return rsx! {
            div {
                class: "text-center py-8 text-destructive",
                "Failed to load episodes: {err}"
            }
        };
    }

    // Show results
    let episodes_read = episodes.read();
    if let Some(eps) = episodes_read.as_ref() {
        let display_eps: Vec<_> = if let Some(limit) = props.limit {
            eps.iter().take(limit).cloned().collect()
        } else {
            eps.clone()
        };

        if display_eps.is_empty() {
            rsx! {
                div {
                    class: "text-center py-8 text-muted-foreground",
                    "No episodes found yet."
                }
            }
        } else {
            rsx! {
                div {
                    class: "space-y-1",
                    for ep in display_eps {
                        PodcastEpisodeCard {
                            key: "{ep.id}",
                            episode: ep.clone(),
                            show_podcast_title: true
                        }
                    }
                }
            }
        }
    } else {
        rsx! {
            div {
                class: "space-y-1",
                for i in 0..5 {
                    PodcastEpisodeCardSkeleton {
                        key: "{i}"
                    }
                }
            }
        }
    }
}

/// Merged episodes component - fetches from both RSS and Nostr based on platform filter
#[derive(Props, Clone, PartialEq)]
struct RecentEpisodesMergedProps {
    platform: String,
    #[props(default)]
    limit: Option<usize>,
}

#[component]
fn RecentEpisodesMerged(props: RecentEpisodesMergedProps) -> Element {
    // Separate state for RSS and Nostr episodes (progressive loading)
    let mut rss_episodes = use_signal(|| None::<Vec<DisplayEpisode>>);
    let mut nostr_episodes = use_signal(|| None::<Vec<DisplayEpisode>>);
    let mut rss_loading = use_signal(|| true);
    let mut nostr_loading = use_signal(|| true);
    let error = use_signal(|| None::<String>);
    let platform = props.platform.clone();

    // Effect 1: RSS/API Loading (uses Podcast Index API)
    {
        let platform = platform.clone();
        use_effect(move || {
            let platform = platform.clone();
            let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
            let has_signer = nostr_client::has_signer();

            let should_fetch_rss = platform == "all" || platform == "rss";
            if !should_fetch_rss {
                rss_loading.set(false);
                return;
            }

            // Need auth for Podcast Index API
            if !client_initialized || !has_signer {
                return;
            }

            rss_loading.set(true);

            spawn(async move {
                let subscriptions = podcast_subscription::get_rss_subscriptions();

                // Fetch all podcasts and their episodes concurrently
                // Handle both GUID-only and ID-cached subscriptions
                let fetch_futures: Vec<_> = subscriptions.iter().take(20).map(|sub| {
                    let guid = sub.podcast_guid.clone();
                    let id = sub.numeric_id();
                    async move {
                        // Try by ID first (cached, more efficient), then by GUID
                        let feed_result = if let Some(id) = id {
                            podcast_index::get_podcast_by_id(id).await
                        } else if let Some(ref guid) = guid {
                            podcast_index::get_podcast_by_guid(guid).await
                        } else {
                            return None;
                        };

                        let Ok(feed) = feed_result else {
                            log::warn!("Failed to fetch podcast: {:?}/{:?}", guid, id);
                            return None;
                        };

                        // Fetch episodes by feed ID
                        let episodes = podcast_index::get_episodes_by_feed_id(feed.id, Some(5)).await
                            .unwrap_or_default();

                        let display_episodes: Vec<DisplayEpisode> = episodes
                            .iter()
                            .map(|ep| DisplayEpisode::from_podcast_index_episode(ep, &feed))
                            .collect();
                        Some(display_episodes)
                    }
                }).collect();

                // Execute all podcast fetches concurrently
                let results = futures::future::join_all(fetch_futures).await;

                // Collect successful results
                let all_episodes: Vec<DisplayEpisode> = results.into_iter().flatten().flatten().collect();

                log::info!("Fetched {} RSS episodes from subscriptions", all_episodes.len());
                rss_episodes.set(Some(all_episodes));
                rss_loading.set(false);
            });
        });
    }

    // Effect 2: Nostr Loading (waits for client initialization)
    {
        let platform = platform.clone();
        use_effect(move || {
            let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
            let platform = platform.clone();

            let should_fetch_nostr = platform == "all" || platform == "nostr";
            if !should_fetch_nostr {
                nostr_loading.set(false);
                return;
            }

            // Wait for nostr client - effect will re-trigger when CLIENT_INITIALIZED changes
            if !client_initialized {
                return;
            }

            nostr_loading.set(true);

            spawn(async move {
                match fetch_recent_nostr_episodes().await {
                    Ok(eps) => {
                        log::info!("Fetched {} Nostr episodes", eps.len());
                        nostr_episodes.set(Some(eps));
                    }
                    Err(e) => {
                        log::warn!("Failed to fetch Nostr episodes: {}", e);
                        nostr_episodes.set(Some(Vec::new()));
                    }
                }
                nostr_loading.set(false);
            });
        });
    }

    // Merge and sort episodes reactively (memoized to avoid redundant work)
    let sorted_episodes = use_memo(move || {
        let mut all = Vec::new();
        if let Some(ref eps) = *rss_episodes.read() {
            all.extend(eps.iter().cloned());
        }
        if let Some(ref eps) = *nostr_episodes.read() {
            all.extend(eps.iter().cloned());
        }
        all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        all
    });

    // Check if we have any data yet
    let has_rss_data = rss_episodes.read().is_some();
    let has_nostr_data = nostr_episodes.read().is_some();
    let has_any_data = has_rss_data || has_nostr_data;

    // Determine if still loading based on platform
    let is_initial_loading = match props.platform.as_str() {
        "rss" => *rss_loading.read() && !has_rss_data,
        "nostr" => !*nostr_client::CLIENT_INITIALIZED.read() || (*nostr_loading.read() && !has_nostr_data),
        _ => *rss_loading.read() && !has_any_data, // "all" - show RSS first
    };

    // Is nostr still loading in the background?
    let nostr_still_loading = props.platform == "all" &&
        (!*nostr_client::CLIENT_INITIALIZED.read() || *nostr_loading.read());

    // Show loading skeleton only during initial load
    if is_initial_loading {
        return rsx! {
            div {
                class: "space-y-1",
                for i in 0..5 {
                    PodcastEpisodeCardSkeleton {
                        key: "{i}"
                    }
                }
            }
        };
    }

    // Show error if any
    if let Some(err) = error.read().as_ref() {
        return rsx! {
            div {
                class: "text-center py-8 text-destructive",
                "Failed to load episodes: {err}"
            }
        };
    }

    // Apply limit
    let display_eps: Vec<_> = if let Some(limit) = props.limit {
        sorted_episodes.read().iter().take(limit).cloned().collect()
    } else {
        sorted_episodes.read().clone()
    };

    if display_eps.is_empty() {
        let platform_msg = match props.platform.as_str() {
            "rss" => "No RSS episodes found. Try subscribing to a podcast feed!",
            "nostr" => "No Nostr podcast episodes found yet.",
            _ => "No episodes found. Subscribe to podcasts or check back later!",
        };

        rsx! {
            div {
                class: "text-center py-8 text-muted-foreground space-y-3",
                p { "{platform_msg}" }

                // Show nostr loading indicator
                if nostr_still_loading {
                    div {
                        class: "text-xs text-muted-foreground",
                        "Loading Nostr episodes..."
                    }
                }
            }
        }
    } else {
        rsx! {
            div {
                class: "space-y-1",
                for ep in display_eps {
                    PodcastEpisodeCard {
                        key: "{ep.id}",
                        episode: ep.clone(),
                        show_podcast_title: true
                    }
                }

                // Show nostr loading indicator at bottom when merging
                if nostr_still_loading {
                    div {
                        class: "text-xs text-muted-foreground text-center py-2",
                        "Loading Nostr episodes..."
                    }
                }
            }
        }
    }
}

/// Section showing subscribed RSS feeds
#[component]
fn SubscribedFeedsSection() -> Element {
    let subscriptions = podcast_subscription::get_subscriptions();
    // Filter RSS feeds and extract valid IDs (skip any with None id)
    let rss_feed_ids: Vec<String> = subscriptions
        .iter()
        .filter(|s| s.is_rss())
        .filter_map(|s| s.id())
        .collect();

    if rss_feed_ids.is_empty() {
        return rsx! {
            div {
                class: "bg-muted/30 rounded-lg p-4",
                h3 {
                    class: "font-semibold mb-2",
                    "Your Subscriptions"
                }
                p {
                    class: "text-sm text-muted-foreground",
                    "No podcast subscriptions yet. Search for podcasts or browse categories to find shows to subscribe to."
                }
            }
        };
    }

    rsx! {
        div {
            class: "space-y-3",
            h3 {
                class: "font-semibold",
                "Your Subscriptions"
                span {
                    class: "text-xs text-muted-foreground font-normal ml-2",
                    "({rss_feed_ids.len()} feeds)"
                }
            }
            div {
                class: "grid gap-2",
                for podcast_id in rss_feed_ids {
                    SubscribedFeedCard {
                        podcast_id: podcast_id
                    }
                }
            }
        }
    }
}

/// Card for a subscribed RSS feed
#[derive(Props, Clone, PartialEq)]
struct SubscribedFeedCardProps {
    /// Podcast Index ID (not a URL)
    podcast_id: String,
}

#[component]
fn SubscribedFeedCard(props: SubscribedFeedCardProps) -> Element {
    let mut podcast_info = use_signal(|| None::<(String, Option<String>, Option<String>)>); // (title, author, image)
    let mut is_loading = use_signal(|| true);
    let mut is_removing = use_signal(|| false);
    let podcast_id = props.podcast_id.clone();

    // Fetch podcast metadata from Podcast Index API by ID
    use_effect(move || {
        let id_str = podcast_id.clone();
        spawn(async move {
            // Parse podcast ID
            if let Ok(id) = id_str.parse::<u64>() {
                match podcast_index::get_podcast_by_id(id).await {
                    Ok(feed) => {
                        let image = feed.artwork.or(feed.image);
                        podcast_info.set(Some((feed.title, feed.author, image)));
                    }
                    Err(e) => {
                        log::warn!("Failed to fetch podcast info for ID {}: {}", id_str, e);
                        // Use ID as fallback title
                        podcast_info.set(Some((format!("Podcast #{}", id_str), None, None)));
                    }
                }
            } else {
                // Invalid ID format - use as-is
                podcast_info.set(Some((id_str.clone(), None, None)));
            }
            is_loading.set(false);
        });
    });

    let podcast_id_for_remove = props.podcast_id.clone();
    let handle_remove = move |_| {
        let id = podcast_id_for_remove.clone();
        is_removing.set(true);

        spawn(async move {
            if let Err(e) = podcast_subscription::remove_subscription(&id).await {
                log::error!("Failed to unsubscribe: {}", e);
            }
            is_removing.set(false);
        });
    };

    if *is_loading.read() {
        return rsx! {
            div {
                class: "flex items-center gap-3 p-3 bg-muted/30 rounded-lg animate-pulse",
                div { class: "w-12 h-12 bg-muted rounded-lg" }
                div {
                    class: "flex-1",
                    div { class: "h-4 bg-muted rounded w-3/4 mb-2" }
                    div { class: "h-3 bg-muted rounded w-1/2" }
                }
            }
        };
    }

    let (title, author, image) = podcast_info.read().clone().unwrap_or_else(|| {
        (format!("Podcast #{}", props.podcast_id), None, None)
    });

    rsx! {
        div {
            class: "flex items-center gap-3 p-3 bg-muted/30 rounded-lg hover:bg-muted/50 transition",

            // Cover image
            div {
                class: "w-12 h-12 rounded-lg overflow-hidden bg-muted flex-shrink-0",
                if let Some(ref img) = image {
                    img {
                        src: "{img}",
                        alt: "{title}",
                        class: "w-full h-full object-cover",
                    }
                } else {
                    div {
                        class: "w-full h-full flex items-center justify-center text-muted-foreground",
                        dangerous_inner_html: icons::PODCAST
                    }
                }
            }

            // Info
            div {
                class: "flex-1 min-w-0",
                p {
                    class: "font-medium truncate",
                    "{title}"
                }
                if let Some(ref auth) = author {
                    p {
                        class: "text-sm text-muted-foreground truncate",
                        "{auth}"
                    }
                }
            }

            // Remove button
            button {
                class: "p-2 hover:bg-destructive/10 text-muted-foreground hover:text-destructive rounded-lg transition",
                onclick: handle_remove,
                disabled: *is_removing.read(),
                title: "Unsubscribe",
                if *is_removing.read() {
                    span { class: "text-xs", "..." }
                } else {
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M6 18L18 6M6 6l12 12"
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Data Fetching Functions
// ============================================================================

/// Fetch Nostr podcast shows (Kind 30078 with podcast metadata)
/// Falls back to inferring shows from episode events if no metadata is found
async fn fetch_nostr_podcast_shows() -> std::result::Result<Vec<PodcastShow>, String> {
    use nostr_sdk::SingleLetterTag;

    // Query for podcast metadata events (Kind 30078 with d="podcast-metadata")
    // Per NIP spec, podcast metadata should use d="podcast-metadata"
    let filter = Filter::new()
        .kind(Kind::from(podcast::KIND_APP_DATA))
        .custom_tag(
            SingleLetterTag::from_char('d').unwrap(),
            "podcast-metadata"
        )
        .limit(50);

    log::info!("Fetching podcast metadata events (Kind 30078, d=podcast-metadata)...");

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await?;

    log::info!("Received {} Kind 30078 events", events.len());

    let mut shows = Vec::new();
    for event in events.iter() {
        // Check if this is a podcast metadata event
        if podcast::is_podcast_metadata(event) {
            log::debug!("Found podcast metadata: {:?}", event.id);
            match podcast::parse_podcast_metadata(event) {
                Ok(metadata) => {
                    log::info!("Parsed podcast: {}", metadata.title);
                    shows.push(PodcastShow::from_nostr_metadata(&metadata));
                }
                Err(e) => {
                    log::warn!("Failed to parse podcast metadata: {}", e);
                }
            }
        }
    }

    log::info!("Found {} podcast shows from metadata", shows.len());

    // If no shows found from metadata, try to infer shows from episodes
    if shows.is_empty() {
        log::info!("No metadata found, trying to infer shows from episodes...");
        shows = infer_shows_from_episodes().await?;
    }

    // Sort by created_at descending
    shows.sort_by(|a, b| b.id.cmp(&a.id));

    Ok(shows)
}

/// Infer podcast shows from episode events when no metadata is available
/// Groups episodes by pubkey and creates synthetic show entries
async fn infer_shows_from_episodes() -> std::result::Result<Vec<PodcastShow>, String> {
    use std::collections::HashMap;

    // Fetch recent episodes
    let filter = Filter::new()
        .kind(Kind::from(podcast::KIND_PODCAST_EPISODE))
        .limit(100);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await?;

    log::info!("Inferring shows from {} episode events", events.len());

    // Group episodes by pubkey to find unique "shows"
    let mut shows_by_pubkey: HashMap<String, (String, u64, Option<String>)> = HashMap::new(); // pubkey -> (first_episode_title, created_at, image)

    for event in events.iter() {
        if let Ok(episode) = podcast::parse_podcast_episode(event) {
            let pubkey = episode.pubkey.clone();
            let entry = shows_by_pubkey.entry(pubkey.clone()).or_insert_with(|| {
                // First episode for this pubkey
                (episode.title.clone(), episode.created_at, episode.image.clone())
            });
            // Track the most recent episode
            if episode.created_at > entry.1 {
                entry.0 = episode.title.clone();
                entry.1 = episode.created_at;
                if episode.image.is_some() {
                    entry.2 = episode.image.clone();
                }
            }
        }
    }

    log::info!("Found {} unique podcast publishers", shows_by_pubkey.len());

    // Create synthetic show entries
    let shows: Vec<PodcastShow> = shows_by_pubkey
        .into_iter()
        .filter_map(|(pubkey, (_title, _created_at, image))| {
            use nostr::prelude::*;

            // Generate proper naddr for linking
            let pk = PublicKey::from_hex(&pubkey).ok()?;
            let coord = Coordinate::new(Kind::from(30078), pk)
                .identifier("podcast-metadata");
            let nip19_coord = Nip19Coordinate::new(coord, vec![]);
            let naddr = nip19_coord.to_bech32().ok()?;

            Some(PodcastShow {
                id: format!("inferred:{}", pubkey),
                title: format!("Podcast by {}", truncate_pubkey(&pubkey)),
                description: Some("Podcast discovered from episodes. Metadata not yet published.".to_string()),
                author: Some(pubkey.clone()),
                image,
                categories: vec![],
                value: None,
                explicit: false,
                source: crate::utils::podcast::PodcastSource::Nostr {
                    pubkey: pubkey.clone(),
                    d_tag: "podcast-metadata".to_string(),
                    coordinate: naddr,
                },
                episode_count: None,
            })
        })
        .collect();

    Ok(shows)
}

/// Fetch recent Nostr podcast episodes (Kind 30054)
async fn fetch_recent_nostr_episodes() -> std::result::Result<Vec<DisplayEpisode>, String> {
    // Query for podcast episode events (Kind 30054)
    let filter = Filter::new()
        .kind(Kind::from(podcast::KIND_PODCAST_EPISODE))
        .limit(50);

    log::info!("Fetching podcast episodes (Kind 30054)...");

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await?;

    log::info!("Received {} Kind 30054 events", events.len());

    let mut episodes = Vec::new();
    let mut parse_errors = 0;
    for event in events.iter() {
        match podcast::parse_podcast_episode(event) {
            Ok(episode) => {
                log::debug!("Parsed episode: {}", episode.title);
                // For now, use a placeholder podcast title since we don't have metadata
                // In production, you'd want to fetch the associated metadata
                let display = DisplayEpisode::from_nostr_episode(
                    &episode,
                    "Nostr Podcast", // Placeholder
                    None,
                );
                episodes.push(display);
            }
            Err(e) => {
                parse_errors += 1;
                log::debug!("Failed to parse episode event {}: {}", event.id, e);
            }
        }
    }

    if parse_errors > 0 {
        log::info!("Parsed {} episodes, {} parse failures", episodes.len(), parse_errors);
    } else {
        log::info!("Parsed {} episodes", episodes.len());
    }

    // Sort by created_at descending (most recent first)
    episodes.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(episodes)
}

/// Search Nostr podcasts by query using NIP-50 relay-side search
async fn search_nostr_podcasts(query: &str) -> std::result::Result<Vec<PodcastShow>, String> {
    use nostr_sdk::SingleLetterTag;

    // Use NIP-50 search filter for relay-side search
    let filter = Filter::new()
        .kind(Kind::from(podcast::KIND_APP_DATA))
        .custom_tag(
            SingleLetterTag::from_char('d').unwrap(),
            "podcast-metadata"
        )
        .search(query)
        .limit(50);

    log::info!("NIP-50 search for podcasts: '{}'", query);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await?;

    log::info!("NIP-50 search returned {} events", events.len());

    let mut shows = Vec::new();
    for event in events.iter() {
        if podcast::is_podcast_metadata(event) {
            match podcast::parse_podcast_metadata(event) {
                Ok(metadata) => {
                    shows.push(PodcastShow::from_nostr_metadata(&metadata));
                }
                Err(e) => {
                    log::warn!("Failed to parse podcast metadata: {}", e);
                }
            }
        }
    }

    // If NIP-50 returned no results, fall back to client-side filtering
    // (some relays may not support NIP-50)
    if shows.is_empty() {
        log::info!("NIP-50 returned no results, falling back to client-side search");
        let all_shows = fetch_nostr_podcast_shows().await?;
        let query_lower = query.to_lowercase();
        shows = all_shows
            .into_iter()
            .filter(|show| {
                show.title.to_lowercase().contains(&query_lower)
                    || show.author.as_ref().map(|a| a.to_lowercase().contains(&query_lower)).unwrap_or(false)
                    || show.categories.iter().any(|c| c.to_lowercase().contains(&query_lower))
            })
            .collect();
    }

    Ok(shows)
}
