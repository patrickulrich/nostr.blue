use crate::components::{
    DiscoveryTab, DiscoveryTabs, LoginModal, UnifiedTrackCard, UnifiedTrackCardSkeleton,
};
use super::{MusicExplore, MusicLibrarySection};
use crate::services::podcast_index;
use crate::services::wavlake::WavlakeAPI;
use crate::stores::auth_store;
use crate::stores::music_player::MusicTrack;
use crate::stores::nostr_client;
use crate::stores::nostr_music::{self, MusicFeedFilter, TrackSource};
use dioxus::prelude::*;
use std::sync::Arc;
#[component]
pub fn MusicHome() -> Element {
    let navigator = navigator();
    let is_authenticated = auth_store::is_authenticated();
    let mut search_query = use_signal(String::new);
    let mut discovery_tab = use_signal(|| DiscoveryTab::Trending);
    let mut selected_genre = use_signal(|| String::from("all"));
    let mut selected_days = use_signal(|| 7u32);
    let mut selected_platform = use_signal(|| String::from("all"));
    let mut unified_tracks = use_signal(Vec::<MusicTrack>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    let mut chart_tracks = use_signal(Vec::<(u32, String, MusicTrack)>::new);
    let mut chart_loading = use_signal(|| true);
    let mut chart_error = use_signal(|| None::<String>);
    let mut chart_auth_required = use_signal(|| false);
    let mut show_login_modal = use_signal(|| false);
    use_effect(move || {
        if nostr_client::has_signer() && *show_login_modal.read() {
            show_login_modal.set(false);
        }
    });
    let genres = [
        "all",
        "Rock",
        "Pop",
        "Hip-Hop",
        "Electronic",
        "Folk",
        "Jazz",
        "Classical",
        "Blues",
        "Country",
        "Reggae",
        "Punk",
        "Metal",
    ];
    let time_periods = [(1, "24h"), (7, "7d"), (30, "30d"), (90, "90d")];
    use_effect(move || {
        let tab = discovery_tab.read().clone();
        let genre = selected_genre.read().clone();
        let days = *selected_days.read();
        let platform = selected_platform.read().clone();
        if matches!(tab, DiscoveryTab::V4v | DiscoveryTab::Explore | DiscoveryTab::Library) {
            return;
        }
        // Reactive signer/relay gating (dms.rs pattern): reading these
        // signals here re-runs the effect once they flip, instead of
        // blocking inside the fetch on wait_for_user_relays.
        let has_signer = nostr_client::has_signer();
        let relays_applied = *crate::stores::relay::USER_RELAYS_APPLIED.read();
        let client_ready = *nostr_client::CLIENT_INITIALIZED.read();
        loading.set(true);
        error_msg.set(None);
        spawn(async move {
            let should_fetch_wavlake = platform == "all" || platform == "wavlake";
            // Nostr branch waits for the user's NIP-65 relays (logged-out
            // users proceed immediately on DEFAULT_RELAYS).
            let should_fetch_nostr = client_ready
                && (!has_signer || relays_applied)
                && (platform == "all" || platform == "nostr" || tab == DiscoveryTab::Following);

            // Independent sources run in parallel — Wavlake is plain HTTP,
            // the nostr branch has its own relay waits; serializing them
            // stacked their latencies on the critical path.
            let wavlake_branch = async {
                if !should_fetch_wavlake {
                    return Vec::new();
                }
                let api = WavlakeAPI::new();
                let genre_filter = if genre == "all" {
                    None
                } else {
                    Some(genre.as_str())
                };
                let sort = "sats";
                match api
                    .get_rankings(sort, Some(days), None, None, genre_filter, Some(30))
                    .await
                {
                    Ok(wavlake_tracks) => wavlake_tracks
                        .into_iter()
                        .map(Into::into)
                        .collect::<Vec<MusicTrack>>(),
                    Err(e) => {
                        log::error!("Failed to fetch Wavlake tracks: {}", e);
                        Vec::new()
                    }
                }
            };
            // Zap totals never gate the paint: tracks render the moment
            // they arrive (msat_total unfilled), zaps merge in afterwards
            // and the list re-sorts — same pattern as the note feed's
            // interaction counts.
            let nostr_branch = async {
                if !should_fetch_nostr {
                    return Vec::new();
                }
                let nostr_filter = if tab == DiscoveryTab::Following {
                    MusicFeedFilter::Following
                } else {
                    MusicFeedFilter::All
                };
                let nostr_genre = if genre == "all" {
                    None
                } else {
                    Some(genre.as_str())
                };
                match nostr_music::fetch_nostr_tracks(nostr_filter, 30, nostr_genre, None).await {
                    Ok(nostr_tracks) => {
                        let coords: Vec<String> =
                            nostr_tracks.iter().map(|t| t.coordinate.clone()).collect();
                        // Async zap merge into the rendered list.
                        let tab_for_zaps = tab.clone();
                        spawn(async move {
                            let zap_totals = nostr_music::fetch_track_zap_totals(
                                coords,
                                Some(days),
                            )
                            .await
                            .unwrap_or_default();
                            if zap_totals.is_empty() {
                                return;
                            }
                            let mut updated = unified_tracks.read().clone();
                            for track in updated.iter_mut() {
                                if let TrackSource::Nostr { coordinate, .. } = &track.source {
                                    if let Some(total) = zap_totals.get(coordinate) {
                                        track.msat_total = Some(*total);
                                    }
                                }
                            }
                            if matches!(tab_for_zaps, DiscoveryTab::Trending) {
                                updated
                                    .sort_by_key(|b| std::cmp::Reverse(b.msat_total.unwrap_or(0)));
                            }
                            unified_tracks.set(updated);
                        });
                        nostr_tracks
                            .into_iter()
                            .map(|nt| {
                                let track: MusicTrack = nt.into();
                                track
                            })
                            .collect::<Vec<MusicTrack>>()
                    }
                    Err(e) => {
                        log::error!("Failed to fetch Nostr tracks: {}", e);
                        Vec::new()
                    }
                }
            };
            let (wavlake_tracks, nostr_tracks) = futures::join!(wavlake_branch, nostr_branch);
            let mut all_tracks: Vec<MusicTrack> = wavlake_tracks;
            all_tracks.extend(nostr_tracks);
            match tab {
                DiscoveryTab::Trending => {
                    all_tracks
                        .sort_by_key(|b| std::cmp::Reverse(b.msat_total.unwrap_or(0)));
                }
                DiscoveryTab::Following => {
                    all_tracks
                        .sort_by_key(|b| std::cmp::Reverse(b.created_at.unwrap_or(0)));
                }
                DiscoveryTab::V4v | DiscoveryTab::Explore | DiscoveryTab::Library => {}
            }
            unified_tracks.set(all_tracks.clone());
            loading.set(false);
            // Android Auto browse-cache mirror: deferred until after the
            // list is committed so the (JNI) writes stay off the critical
            // render path.
            #[cfg(feature = "mobile_platform")]
            {
                spawn(async move {
                    if let Ok(json) = serde_json::to_string(&all_tracks) {
                        let _ = crate::platform::android_media::save_browse_cache(
                            "trending_music",
                            &json,
                        );
                    }
                    for t in &all_tracks {
                        let _ = crate::platform::android_media::save_browse_cache(
                            &format!("item:{}", t.id),
                            &serde_json::to_string(t).unwrap_or_default(),
                        );
                    }
                });
            }
        });
    });
    use_effect(move || {
        let tab = discovery_tab.read().clone();
        if tab != DiscoveryTab::V4v {
            return;
        }
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = nostr_client::has_signer();
        if !client_initialized {
            return;
        }
        if !has_signer {
            chart_loading.set(false);
            chart_auth_required.set(true);
            return;
        }
        chart_auth_required.set(false);
        chart_loading.set(true);
        chart_error.set(None);
        spawn(async move {
            match podcast_index::get_v4v_music_chart().await {
                Ok(chart) => {
                    let fetch_futures: Vec<_> = chart
                        .items
                        .iter()
                        .take(30)
                        .map(|item| {
                            let item = item.clone();
                            async move {
                                match podcast_index::get_episode_by_guid(
                                    &item.item_guid,
                                    Some(&item.feed_guid),
                                )
                                .await
                                {
                                    Ok((episode, feed)) => {
                                        let feed = feed.unwrap_or_else(|| {
                                            podcast_index::PodcastFeed {
                                                id: item.feed_id,
                                                title: item.author.clone().unwrap_or_default(),
                                                url: item.feed_url.clone(),
                                                original_url: None,
                                                link: None,
                                                description: None,
                                                author: item.author.clone(),
                                                owner_name: None,
                                                image: item.image.clone(),
                                                artwork: None,
                                                language: None,
                                                itunes_id: None,
                                                podcast_guid: Some(item.feed_guid.clone()),
                                                episode_count: None,
                                                categories: None,
                                                trending_score: None,
                                                value: None,
                                            }
                                        });
                                        let track =
                                            MusicTrack::from_rss_music_track(&episode, &feed, item.image.as_deref());
                                        Some((item.rank, item.boosts, track))
                                    }
                                    Err(e) => {
                                        log::warn!(
                                            "Failed to fetch episode for chart item {}: {}",
                                            item.rank,
                                            e
                                        );
                                        None
                                    }
                                }
                            }
                        })
                        .collect();
                    let results = futures::future::join_all(fetch_futures).await;
                    let tracks: Vec<(u32, String, MusicTrack)> =
                        results.into_iter().flatten().collect();
                    chart_tracks.set(tracks);
                    chart_error.set(None);
                }
                Err(e) => {
                    log::error!("Failed to fetch V4V music chart: {}", e);
                    chart_error.set(Some(e));
                }
            }
            chart_loading.set(false);
        });
    });
    let handle_search = move |_| {
        let query = search_query.read().trim().to_string();
        if !query.is_empty() {
            let encoded_query = urlencoding::encode(&query).to_string();
            navigator.push(crate::routes::Route::MusicSearch { q: encoded_query });
        }
    };
    let current_tab = discovery_tab.read().clone();
    let show_filters = matches!(
        current_tab,
        DiscoveryTab::Trending | DiscoveryTab::Following
    );
    rsx! {
        div { class: "max-w-5xl mx-auto p-4 space-y-6",
            div { class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4",
                h1 { class: "text-3xl font-bold", "Music Discovery" }
                div { class: "flex items-center gap-2 flex-wrap",
                    if is_authenticated {
                        Link {
                            to: crate::routes::Route::MusicTrackNew {
                            },
                            class: "px-3 py-2 bg-muted text-muted-foreground rounded-lg hover:bg-muted/80 transition text-sm font-medium",
                            "+ Track"
                        }
                        Link {
                            to: crate::routes::Route::MusicPlaylistNew {
                            },
                            class: "px-3 py-2 bg-muted text-muted-foreground rounded-lg hover:bg-muted/80 transition text-sm font-medium",
                            "+ Playlist"
                        }
                    }
                    Link {
                        to: crate::routes::Route::MusicRadio {
                        },
                        class: "px-3 py-2 bg-muted text-muted-foreground rounded-lg hover:bg-muted/80 transition text-sm font-medium",
                        "Radio"
                    }
                    Link {
                        to: crate::routes::Route::MusicLeaderboard {
                        },
                        class: "px-3 py-2 bg-muted text-muted-foreground rounded-lg hover:bg-muted/80 transition text-sm font-medium",
                        "Leaderboard"
                    }
                }
            }
            div { class: "relative",
                input {
                    r#type: "text",
                    placeholder: "Search for tracks, artists, or albums...",
                    class: "w-full px-4 py-3 pr-12 border border-border rounded-full focus:outline-hidden focus:ring-2 focus:ring-primary bg-background",
                    value: "{search_query}",
                    oninput: move |evt| search_query.set(evt.value()),
                    onkeydown: move |evt| {
                        if evt.key() == Key::Enter {
                            handle_search(());
                        }
                    },
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
                            d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z",
                        }
                    }
                }
            }
            DiscoveryTabs {
                selected: discovery_tab.read().clone(),
                on_change: move |tab: DiscoveryTab| {
                    discovery_tab.set(tab.clone());
                    if tab == DiscoveryTab::Following {
                        selected_platform.set("nostr".to_string());
                    }
                },
            }
            if show_filters {
                div { class: "flex flex-col sm:flex-row gap-4",
                    div { class: "flex-1",
                        div { class: "text-xs font-medium text-muted-foreground mb-2 uppercase tracking-wide",
                            "Genre"
                        }
                        div { class: "flex flex-wrap gap-1.5",
                            for genre in genres.iter() {
                                {
                                    let is_selected = *selected_genre.read() == *genre;
                                    let genre_val = genre.to_string();
                                    rsx! {
                                        button {
                                            key: "{genre}",
                                            class: if is_selected { "px-3 py-1.5 rounded-full text-xs font-medium transition bg-primary text-primary-foreground" } else { "px-3 py-1.5 rounded-full text-xs font-medium transition bg-muted/50 hover:bg-muted text-muted-foreground" },
                                            onclick: move |_| selected_genre.set(genre_val.clone()),
                                            "{genre}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if *discovery_tab.read() == DiscoveryTab::Trending {
                        div { class: "sm:w-auto",
                            div { class: "text-xs font-medium text-muted-foreground mb-2 uppercase tracking-wide",
                                "Time Period"
                            }
                            div { class: "flex gap-1.5",
                                for (days , label) in time_periods.iter() {
                                    {
                                        let is_selected = *selected_days.read() == *days;
                                        let days_val = *days;
                                        rsx! {
                                            button {
                                                key: "{days}",
                                                class: if is_selected { "px-3 py-1.5 rounded-full text-xs font-medium transition bg-primary text-primary-foreground" } else { "px-3 py-1.5 rounded-full text-xs font-medium transition bg-muted/50 hover:bg-muted text-muted-foreground" },
                                                onclick: move |_| selected_days.set(days_val),
                                                "{label}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "sm:w-auto",
                        div { class: "text-xs font-medium text-muted-foreground mb-2 uppercase tracking-wide",
                            "Platform"
                        }
                        div { class: "flex gap-1.5",
                            {
                                let platforms = vec![("all", "All"), ("wavlake", "Wavlake"), ("nostr", "Nostr")];
                                rsx! {
                                    for (value , label) in platforms {
                                        {
                                            let is_selected = *selected_platform.read() == value;
                                            let platform_val = value.to_string();
                                            rsx! {
                                                button {
                                                    key: "{value}",
                                                    class: if is_selected { "px-3 py-1.5 rounded-full text-xs font-medium transition bg-primary text-primary-foreground" } else { "px-3 py-1.5 rounded-full text-xs font-medium transition bg-muted/50 hover:bg-muted text-muted-foreground" },
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
            div { class: "space-y-1",
                if *discovery_tab.read() == DiscoveryTab::Explore {
                    MusicExplore {}
                } else if *discovery_tab.read() == DiscoveryTab::Library {
                    MusicLibrarySection {}
                } else if *discovery_tab.read() == DiscoveryTab::V4v {
                    div { class: "mb-4",
                        h2 { class: "text-lg font-semibold", "V4V Music Chart" }
                        p { class: "text-sm text-muted-foreground",
                            "Top tracks by listener boosts over the last 7 days. Updated hourly."
                        }
                    }
                    if *chart_loading.read() {
                        for i in 0..8 {
                            UnifiedTrackCardSkeleton { key: "{i}" }
                        }
                    } else if *chart_auth_required.read() {
                        div { class: "text-center py-16",
                            div { class: "w-20 h-20 mx-auto mb-6 rounded-full bg-muted flex items-center justify-center",
                                svg {
                                    class: "w-10 h-10 text-muted-foreground",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "24",
                                    height: "24",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    rect { x: "3", y: "11", width: "18", height: "10", rx: "2" }
                                    path { d: "M7 11V7a5 5 0 0 1 10 0v4" }
                                }
                            }
                            h2 { class: "font-semibold text-xl mb-2", "Sign In Required" }
                            p { class: "text-muted-foreground mb-6",
                                "Sign in with your Nostr identity to browse the V4V Music Chart."
                            }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                onclick: move |_| show_login_modal.set(true),
                                "Sign In"
                            }
                        }
                    } else if let Some(ref err) = *chart_error.read() {
                        div { class: "text-center py-16",
                            div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-destructive/10 flex items-center justify-center",
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "w-8 h-8 text-destructive",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z",
                                    }
                                }
                            }
                            p { class: "text-destructive font-medium", "Failed to load V4V Music Chart" }
                            p { class: "text-sm text-muted-foreground mt-1 max-w-md mx-auto",
                                "{err}"
                            }
                        }
                    } else if chart_tracks.read().is_empty() {
                        div { class: "text-center py-16",
                            div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
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
                                        d: "M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3",
                                    }
                                }
                            }
                            p { class: "text-muted-foreground font-medium", "No chart data available" }
                            p { class: "text-sm text-muted-foreground/70 mt-1",
                                "Check back later for the latest V4V Music Chart"
                            }
                        }
                    } else {
                        div { class: "py-2 text-sm text-muted-foreground",
                            span { "Top {chart_tracks.read().len()} tracks by listener boosts" }
                        }
                        div { class: "divide-y divide-border/50",
                            {
                                let tracks_with_playlist: Vec<MusicTrack> =
                                    chart_tracks.read().iter().map(|(_, _, t)| t.clone()).collect();
                                let playlist = Arc::new(tracks_with_playlist);
                                let entries = chart_tracks.read().clone();
                                rsx! {
                                    for (rank, boosts, track) in entries {
                                        div { class: "flex items-center gap-2",
                                            div {
                                                class: if rank <= 3 { "w-10 shrink-0 text-center font-bold text-lg" } else { "w-10 shrink-0 text-center font-semibold text-muted-foreground" },
                                                class: if rank == 1 { " text-amber-400" } else if rank == 2 { " text-gray-400" } else if rank == 3 { " text-amber-700" },
                                                "#{rank}"
                                            }
                                            div { class: "flex-1 min-w-0",
                                                UnifiedTrackCard {
                                                    key: "{track.id}",
                                                    track: track.clone(),
                                                    show_album: true,
                                                    show_sats: false,
                                                    playlist: Some(playlist.clone()),
                                                }
                                            }
                                            div { class: "flex items-center gap-1 text-xs font-medium text-amber-500 shrink-0 pr-2",
                                                svg {
                                                    xmlns: "http://www.w3.org/2000/svg",
                                                    class: "w-3.5 h-3.5",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    polygon { points: "13 2 3 14 12 14 11 22 21 10 12 10 13 2" }
                                                }
                                                span { "{boosts}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    if *loading.read() {
                        for i in 0..8 {
                            UnifiedTrackCardSkeleton { key: "{i}" }
                        }
                    } else if unified_tracks.read().is_empty() {
                        div { class: "text-center py-16",
                            div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
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
                                        d: "M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3",
                                    }
                                }
                            }
                            p { class: "text-muted-foreground font-medium", "No tracks found" }
                            p { class: "text-sm text-muted-foreground/70 mt-1",
                                "Try a different filter or check back later"
                            }
                        }
                    } else {
                        div { class: "py-2 text-sm text-muted-foreground",
                            span { "{unified_tracks.read().len()} tracks" }
                        }
                        div { class: "divide-y divide-border/50",
                            {
                                let tracks = Arc::new(unified_tracks.read().clone());
                                rsx! {
                                    for track in tracks.iter() {
                                        UnifiedTrackCard {
                                            key: "{track.id}",
                                            track: track.clone(),
                                            show_album: true,
                                            show_sats: true,
                                            playlist: Some(tracks.clone()),
                                        }
                                    }
                                }
                            }
                        }
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

