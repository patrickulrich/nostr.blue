use dioxus::prelude::*;
use crate::services::wavlake::WavlakeAPI;
use crate::stores::music_player::MusicTrack;
use crate::stores::nostr_music::{self, MusicFeedFilter};
use crate::stores::auth_store;
use crate::components::{
    DiscoveryTabs, DiscoveryTab,
    UnifiedTrackCard, UnifiedTrackCardSkeleton,
};

#[component]
pub fn MusicHome() -> Element {
    let navigator = navigator();
    let is_authenticated = auth_store::is_authenticated();

    // State signals
    let mut search_query = use_signal(|| String::new());
    let mut discovery_tab = use_signal(|| DiscoveryTab::Trending);
    let mut selected_genre = use_signal(|| String::from("all"));
    let mut selected_days = use_signal(|| 7u32);
    let mut selected_platform = use_signal(|| String::from("all")); // "all", "wavlake", "nostr"

    // Track state
    let mut unified_tracks = use_signal(|| Vec::<MusicTrack>::new());
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);

    let genres = vec![
        "all", "Rock", "Pop", "Hip-Hop", "Electronic", "Folk", "Jazz",
        "Classical", "Blues", "Country", "Reggae", "Punk", "Metal"
    ];

    let time_periods = vec![
        (1, "24h"),
        (7, "7d"),
        (30, "30d"),
        (90, "90d"),
    ];

    // Fetch tracks when filters change
    use_effect(move || {
        let tab = discovery_tab.read().clone();
        let genre = selected_genre.read().clone();
        let days = *selected_days.read();
        let platform = selected_platform.read().clone();

        // Skip fetch if on playlists tab (handled separately)
        if tab == DiscoveryTab::Playlists {
            return;
        }

        loading.set(true);
        error_msg.set(None);

        spawn(async move {
            let mut all_tracks: Vec<MusicTrack> = Vec::new();

            // Fetch based on platform filter and discovery tab
            let should_fetch_wavlake = platform == "all" || platform == "wavlake";
            // For Following tab, only fetch nostr tracks from people the user follows
            // Also check if nostr client is initialized
            let nostr_client_ready = crate::stores::nostr_client::get_client().is_some();
            let should_fetch_nostr = nostr_client_ready && ((platform == "all" || platform == "nostr") || tab == DiscoveryTab::Following);

            // Fetch Wavlake tracks
            if should_fetch_wavlake {
                let api = WavlakeAPI::new();
                let genre_filter = if genre == "all" { None } else { Some(genre.as_str()) };

                // Sort by sats for trending, release_date for new
                let sort = match tab {
                    DiscoveryTab::Trending => "sats",
                    DiscoveryTab::New => "release_date",
                    _ => "sats",
                };

                match api.get_rankings(sort, Some(days), None, None, genre_filter, Some(30)).await {
                    Ok(wavlake_tracks) => {
                        for wt in wavlake_tracks {
                            all_tracks.push(wt.into());
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to fetch Wavlake tracks: {}", e);
                    }
                }
            }

            // Fetch Nostr tracks
            if should_fetch_nostr {
                // Use Following filter when on Following tab, otherwise All
                let nostr_filter = if tab == DiscoveryTab::Following {
                    MusicFeedFilter::Following
                } else {
                    MusicFeedFilter::All
                };
                // Apply same genre filter as Wavlake
                let nostr_genre = if genre == "all" { None } else { Some(genre.as_str()) };
                match nostr_music::fetch_nostr_tracks(nostr_filter, 30, nostr_genre).await {
                    Ok(nostr_tracks) => {
                        // Get zap totals for ranking (filtered by selected time period)
                        let coords: Vec<String> = nostr_tracks.iter()
                            .map(|t| t.coordinate.clone())
                            .collect();

                        let zap_totals = nostr_music::fetch_track_zap_totals(coords, Some(days)).await.unwrap_or_default();

                        for nt in nostr_tracks {
                            let mut track: MusicTrack = nt.clone().into();
                            track.msat_total = zap_totals.get(&nt.coordinate).copied();
                            all_tracks.push(track);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to fetch Nostr tracks: {}", e);
                    }
                }
            }

            // Sort based on discovery tab
            match tab {
                DiscoveryTab::Trending => {
                    // Sort by msat_total descending (hottest first)
                    all_tracks.sort_by(|a, b| {
                        b.msat_total.unwrap_or(0).cmp(&a.msat_total.unwrap_or(0))
                    });
                }
                DiscoveryTab::New => {
                    // Sort by created_at descending (newest first)
                    all_tracks.sort_by(|a, b| {
                        b.created_at.unwrap_or(0).cmp(&a.created_at.unwrap_or(0))
                    });
                }
                DiscoveryTab::Following => {
                    // Already filtered by Following in fetch, sort by recency
                    all_tracks.sort_by(|a, b| {
                        b.created_at.unwrap_or(0).cmp(&a.created_at.unwrap_or(0))
                    });
                }
                DiscoveryTab::Playlists => {
                    // Handled separately
                }
            }

            unified_tracks.set(all_tracks);
            loading.set(false);
        });
    });

    // Search handler
    let handle_search = move |_| {
        let query = search_query.read().trim().to_string();
        if !query.is_empty() {
            let encoded_query = urlencoding::encode(&query).to_string();
            navigator.push(crate::routes::Route::MusicSearch { q: encoded_query });
        }
    };

    // Show filters only for track tabs (not playlists)
    let show_filters = *discovery_tab.read() != DiscoveryTab::Playlists;

    rsx! {
        div {
            class: "max-w-5xl mx-auto p-4 space-y-6",

            // Header with title and actions
            div {
                class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4",

                h1 {
                    class: "text-3xl font-bold",
                    "Music Discovery"
                }

                div {
                    class: "flex items-center gap-2 flex-wrap",

                    // Publish buttons (authenticated only)
                    if is_authenticated {
                        Link {
                            to: crate::routes::Route::MusicTrackNew {},
                            class: "px-3 py-2 bg-muted text-muted-foreground rounded-lg hover:bg-muted/80 transition text-sm font-medium",
                            "+ Track"
                        }
                        Link {
                            to: crate::routes::Route::MusicPlaylistNew {},
                            class: "px-3 py-2 bg-muted text-muted-foreground rounded-lg hover:bg-muted/80 transition text-sm font-medium",
                            "+ Playlist"
                        }
                    }

                    Link {
                        to: crate::routes::Route::MusicRadio {},
                        class: "px-3 py-2 bg-muted text-muted-foreground rounded-lg hover:bg-muted/80 transition text-sm font-medium",
                        "Radio"
                    }
                    Link {
                        to: crate::routes::Route::MusicLeaderboard {},
                        class: "px-3 py-2 bg-muted text-muted-foreground rounded-lg hover:bg-muted/80 transition text-sm font-medium",
                        "Leaderboard"
                    }
                }
            }

            // Search Bar
            div {
                class: "relative",
                input {
                    r#type: "text",
                    placeholder: "Search for tracks, artists, or albums...",
                    class: "w-full px-4 py-3 pr-12 border border-border rounded-full focus:outline-none focus:ring-2 focus:ring-primary bg-background",
                    value: "{search_query}",
                    oninput: move |evt| search_query.set(evt.value()),
                    onkeydown: move |evt| {
                        if evt.key() == Key::Enter {
                            handle_search(());
                        }
                    }
                }
                button {
                    class: "absolute right-3 top-1/2 -translate-y-1/2 p-2 hover:bg-muted rounded-full transition text-muted-foreground",
                    onclick: move |_| handle_search(()),
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        class: "w-5 h-5",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                        }
                    }
                }
            }

            // Discovery Tabs (Trending | New | Playlists | Following)
            DiscoveryTabs {
                selected: discovery_tab.read().clone(),
                on_change: move |tab| discovery_tab.set(tab)
            }

            // Contextual Filters (hide for Playlists tab)
            if show_filters {
                div {
                    class: "flex flex-col sm:flex-row gap-4",

                    // Genre filter
                    div {
                        class: "flex-1",
                        div {
                            class: "text-xs font-medium text-muted-foreground mb-2 uppercase tracking-wide",
                            "Genre"
                        }
                        div {
                            class: "flex flex-wrap gap-1.5",
                            for genre in genres.iter() {
                                {
                                    let is_selected = *selected_genre.read() == *genre;
                                    let genre_val = genre.to_string();
                                    rsx! {
                                        button {
                                            key: "{genre}",
                                            class: if is_selected {
                                                "px-3 py-1.5 rounded-full text-xs font-medium transition bg-primary text-primary-foreground"
                                            } else {
                                                "px-3 py-1.5 rounded-full text-xs font-medium transition bg-muted/50 hover:bg-muted text-muted-foreground"
                                            },
                                            onclick: move |_| selected_genre.set(genre_val.clone()),
                                            "{genre}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Time period filter (only for Trending)
                    if *discovery_tab.read() == DiscoveryTab::Trending {
                        div {
                            class: "sm:w-auto",
                            div {
                                class: "text-xs font-medium text-muted-foreground mb-2 uppercase tracking-wide",
                                "Time Period"
                            }
                            div {
                                class: "flex gap-1.5",
                                for (days, label) in time_periods.iter() {
                                    {
                                        let is_selected = *selected_days.read() == *days;
                                        let days_val = *days;
                                        rsx! {
                                            button {
                                                key: "{days}",
                                                class: if is_selected {
                                                    "px-3 py-1.5 rounded-full text-xs font-medium transition bg-primary text-primary-foreground"
                                                } else {
                                                    "px-3 py-1.5 rounded-full text-xs font-medium transition bg-muted/50 hover:bg-muted text-muted-foreground"
                                                },
                                                onclick: move |_| selected_days.set(days_val),
                                                "{label}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Platform filter
                    div {
                        class: "sm:w-auto",
                        div {
                            class: "text-xs font-medium text-muted-foreground mb-2 uppercase tracking-wide",
                            "Platform"
                        }
                        div {
                            class: "flex gap-1.5",
                            {
                                let platforms = vec![("all", "All"), ("wavlake", "Wavlake"), ("nostr", "Nostr")];
                                rsx! {
                                    for (value, label) in platforms {
                                        {
                                            let is_selected = *selected_platform.read() == value;
                                            let platform_val = value.to_string();
                                            rsx! {
                                                button {
                                                    key: "{value}",
                                                    class: if is_selected {
                                                        "px-3 py-1.5 rounded-full text-xs font-medium transition bg-primary text-primary-foreground"
                                                    } else {
                                                        "px-3 py-1.5 rounded-full text-xs font-medium transition bg-muted/50 hover:bg-muted text-muted-foreground"
                                                    },
                                                    onclick: move |_| selected_platform.set(platform_val.clone()),
                                                    "{label}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Content Area
            div {
                class: "space-y-1",

                // Show playlists when on Playlists tab
                if *discovery_tab.read() == DiscoveryTab::Playlists {
                    PlaylistSection {
                        platform_filter: selected_platform.read().clone()
                    }
                } else {
                    // Track list
                    if *loading.read() {
                        for i in 0..8 {
                            UnifiedTrackCardSkeleton { key: "{i}" }
                        }
                    } else if unified_tracks.read().is_empty() {
                        div {
                            class: "text-center py-16",
                            div {
                                class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "w-8 h-8 text-muted-foreground",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3"
                                    }
                                }
                            }
                            p {
                                class: "text-muted-foreground font-medium",
                                "No tracks found"
                            }
                            p {
                                class: "text-sm text-muted-foreground/70 mt-1",
                                "Try a different filter or check back later"
                            }
                        }
                    } else {
                        // Stats bar
                        div {
                            class: "py-2 text-sm text-muted-foreground",
                            span {
                                "{unified_tracks.read().len()} tracks"
                            }
                        }

                        // Track cards
                        div {
                            class: "divide-y divide-border/50",
                            {
                                let tracks = unified_tracks.read().clone();
                                rsx! {
                                    for track in tracks.iter() {
                                        UnifiedTrackCard {
                                            key: "{track.id}",
                                            track: track.clone(),
                                            show_album: true,
                                            show_sats: true,
                                            playlist: Some(tracks.clone())
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Playlist discovery section
#[component]
fn PlaylistSection(platform_filter: String) -> Element {
    let mut playlists = use_signal(|| Vec::<nostr_music::NostrPlaylist>::new());
    let mut loading = use_signal(|| true);

    // Fetch playlists
    use_effect(move || {
        let platform = platform_filter.clone();
        loading.set(true);

        spawn(async move {
            // Nostr playlists only (Wavlake playlists are not yet supported)
            // Show when platform is "all" or "nostr"
            let should_fetch = platform == "all" || platform == "nostr";

            if should_fetch {
                match nostr_music::fetch_playlists(None, 20).await {
                    Ok(result) => {
                        playlists.set(result);
                    }
                    Err(e) => {
                        log::error!("Failed to fetch playlists: {}", e);
                    }
                }
            } else {
                playlists.set(Vec::new());
            }
            loading.set(false);
        });
    });

    rsx! {
        if *loading.read() {
            div {
                class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-4",
                for i in 0..8 {
                    PlaylistCardSkeleton { key: "{i}" }
                }
            }
        } else if playlists.read().is_empty() {
            div {
                class: "text-center py-16",
                div {
                    class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        class: "w-8 h-8 text-muted-foreground",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"
                        }
                    }
                }
                p {
                    class: "text-muted-foreground font-medium",
                    "No playlists yet"
                }
                p {
                    class: "text-sm text-muted-foreground/70 mt-1",
                    "Be the first to create one!"
                }
                Link {
                    to: crate::routes::Route::MusicPlaylistNew {},
                    class: "inline-flex items-center gap-2 mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition text-sm font-medium",
                    span { "+" }
                    "Create Playlist"
                }
            }
        } else {
            div {
                class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-4",
                for playlist in playlists.read().iter() {
                    PlaylistCard {
                        key: "{playlist.coordinate}",
                        playlist: playlist.clone()
                    }
                }
            }
        }
    }
}

/// Playlist card component (inline for now, will move to separate file)
#[component]
fn PlaylistCard(playlist: nostr_music::NostrPlaylist) -> Element {
    let track_count = playlist.track_refs.len();

    rsx! {
        Link {
            to: crate::routes::Route::MusicPlaylistDetail { naddr: format!("{}:{}:{}", nostr_music::KIND_PLAYLIST, playlist.pubkey, playlist.d_tag) },
            class: "group block",

            div {
                class: "aspect-square rounded-lg overflow-hidden bg-muted relative",

                // Cover image or gradient
                if let Some(ref image) = playlist.image {
                    img {
                        src: "{image}",
                        alt: "{playlist.title}",
                        class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-300"
                    }
                } else if let Some(ref gradient) = playlist.gradient {
                    div {
                        class: "w-full h-full",
                        style: "background: linear-gradient(135deg, {gradient})"
                    }
                } else {
                    // Default gradient
                    div {
                        class: "w-full h-full bg-gradient-to-br from-purple-500/30 to-blue-500/30 flex items-center justify-center",
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "w-12 h-12 text-muted-foreground/50",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "1.5",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3"
                            }
                        }
                    }
                }

                // Track count badge
                div {
                    class: "absolute bottom-2 right-2 px-2 py-1 bg-black/70 rounded text-xs text-white font-medium",
                    "{track_count} tracks"
                }

                // Play overlay on hover
                div {
                    class: "absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition flex items-center justify-center",
                    div {
                        class: "w-12 h-12 bg-primary rounded-full flex items-center justify-center",
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "w-6 h-6 text-primary-foreground ml-1",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                d: "M8 5v14l11-7z"
                            }
                        }
                    }
                }
            }

            div {
                class: "mt-2",
                h3 {
                    class: "font-medium text-sm truncate group-hover:text-primary transition",
                    "{playlist.title}"
                }
                if let Some(ref desc) = playlist.description {
                    p {
                        class: "text-xs text-muted-foreground truncate mt-0.5",
                        "{desc}"
                    }
                }
            }
        }
    }
}

/// Playlist card skeleton
#[component]
fn PlaylistCardSkeleton() -> Element {
    rsx! {
        div {
            class: "animate-pulse",
            div {
                class: "aspect-square rounded-lg bg-muted"
            }
            div {
                class: "mt-2 space-y-1",
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
