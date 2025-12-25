//! Podcast Home Route
//!
//! Discovery page for podcasts featuring:
//! - Search for Nostr and RSS podcasts
//! - Trending/recent Nostr podcasts
//! - Podcast categories

use dioxus::prelude::*;
use crate::components::{
    PodcastShowCard, PodcastShowCardSkeleton, PodcastShow,
    PodcastEpisodeCard, PodcastEpisodeCardSkeleton, DisplayEpisode,
    icons,
};
use crate::stores::nostr_client;
use crate::utils::podcast;
use crate::utils::truncate_pubkey;
use nostr_sdk::prelude::{Filter, Kind};
use std::time::Duration;

/// Podcast home page
#[component]
pub fn PodcastHome() -> Element {
    let mut search_query = use_signal(String::new);
    let mut active_tab = use_signal(|| PodcastTab::Discover);

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
                            placeholder: "Search podcasts...",
                            value: "{search_query}",
                            oninput: move |e| search_query.set(e.value())
                        }
                        div {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
                            dangerous_inner_html: icons::SEARCH
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
                        label: "Nostr Shows",
                        active: *active_tab.read() == PodcastTab::NostrShows,
                        onclick: move |_| active_tab.set(PodcastTab::NostrShows)
                    }
                    TabButton {
                        label: "Recent Episodes",
                        active: *active_tab.read() == PodcastTab::RecentEpisodes,
                        onclick: move |_| active_tab.set(PodcastTab::RecentEpisodes)
                    }
                }
            }

            // Content
            div {
                class: "p-4",

                // Search results if query is not empty
                if !search_query.read().is_empty() {
                    PodcastSearchResults {
                        query: search_query.read().clone()
                    }
                } else {
                    // Tab content
                    match *active_tab.read() {
                        PodcastTab::Discover => rsx! {
                            DiscoverTab {}
                        },
                        PodcastTab::NostrShows => rsx! {
                            NostrShowsTab {}
                        },
                        PodcastTab::RecentEpisodes => rsx! {
                            RecentEpisodesTab {}
                        },
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum PodcastTab {
    Discover,
    NostrShows,
    RecentEpisodes,
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
#[component]
fn DiscoverTab() -> Element {
    rsx! {
        div {
            class: "space-y-6",

            // About section
            div {
                class: "bg-muted/50 rounded-lg p-4",
                h2 {
                    class: "font-semibold text-lg mb-2",
                    "Nostr Podcasts"
                }
                p {
                    class: "text-sm text-muted-foreground",
                    "Discover podcasts published natively on Nostr. These shows support Value4Value (V4V) payments, allowing you to directly support creators with sats while you listen."
                }
            }

            // Recent Nostr episodes
            div {
                h3 {
                    class: "font-semibold mb-3",
                    "Latest Episodes"
                }
                RecentNostrEpisodes {
                    limit: 10
                }
            }

            // Nostr podcast shows
            div {
                h3 {
                    class: "font-semibold mb-3",
                    "Nostr Podcast Shows"
                }
                NostrPodcastShows {
                    limit: 6
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

/// Recent episodes tab
#[component]
fn RecentEpisodesTab() -> Element {
    rsx! {
        RecentNostrEpisodes {}
    }
}

/// Search results component
#[derive(Props, Clone, PartialEq)]
struct PodcastSearchResultsProps {
    query: String,
}

#[component]
fn PodcastSearchResults(props: PodcastSearchResultsProps) -> Element {
    let query = props.query.clone();
    let mut results = use_signal(|| None::<Vec<PodcastShow>>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Search when client is initialized and query changes
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let q = query.clone();

        if !client_initialized {
            return;
        }

        loading.set(true);
        error.set(None);

        spawn(async move {
            match search_nostr_podcasts(&q).await {
                Ok(shows) => {
                    results.set(Some(shows));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    rsx! {
        div {
            class: "space-y-4",

            h3 {
                class: "font-semibold",
                "Search Results for \"{props.query}\""
            }

            // Show loading skeleton while waiting for client or loading
            if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && results.read().is_none()) {
                div {
                    class: "space-y-1",
                    for i in 0..3 {
                        PodcastShowCardSkeleton {
                            key: "{i}"
                        }
                    }
                }
            } else if let Some(err) = error.read().as_ref() {
                div {
                    class: "text-center py-8 text-destructive",
                    "Error: {err}"
                }
            } else if let Some(shows) = results.read().as_ref() {
                if shows.is_empty() {
                    div {
                        class: "text-center py-8 text-muted-foreground",
                        "No podcasts found for \"{props.query}\""
                    }
                } else {
                    div {
                        class: "space-y-1",
                        for show in shows.iter() {
                            PodcastShowCard {
                                key: "{show.id}",
                                show: show.clone()
                            }
                        }
                    }
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

/// Search Nostr podcasts by query
async fn search_nostr_podcasts(query: &str) -> std::result::Result<Vec<PodcastShow>, String> {
    // For now, fetch all and filter client-side
    // In production, you'd want relay-side search or NIP-50
    let all_shows = fetch_nostr_podcast_shows().await?;

    let query_lower = query.to_lowercase();
    let filtered: Vec<_> = all_shows
        .into_iter()
        .filter(|show| {
            show.title.to_lowercase().contains(&query_lower)
                || show.author.as_ref().map(|a| a.to_lowercase().contains(&query_lower)).unwrap_or(false)
                || show.categories.iter().any(|c| c.to_lowercase().contains(&query_lower))
        })
        .collect();

    Ok(filtered)
}
