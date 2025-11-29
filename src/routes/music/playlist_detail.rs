// Playlist Detail Page
// Shows a single playlist with its tracks and allows playback

use dioxus::prelude::*;
use crate::stores::{nostr_music, music_player, profiles};
use crate::components::{UnifiedTrackCard, UnifiedTrackCardSkeleton};

#[component]
pub fn MusicPlaylistDetail(naddr: String) -> Element {
    let mut playlist = use_signal(|| None::<nostr_music::NostrPlaylist>);
    let mut tracks = use_signal(|| Vec::<music_player::MusicTrack>::new());
    let mut creator_name = use_signal(|| String::from("Unknown"));
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);

    // Parse naddr and fetch playlist
    use_effect(move || {
        let naddr_clone = naddr.clone();
        loading.set(true);
        error_msg.set(None);

        spawn(async move {
            // Parse the naddr format: "kind:pubkey:d-tag"
            let parts: Vec<&str> = naddr_clone.split(':').collect();
            if parts.len() < 3 {
                error_msg.set(Some("Invalid playlist address".to_string()));
                loading.set(false);
                return;
            }

            let pubkey = parts[1];
            let d_tag = parts[2..].join(":"); // Handle d-tags with colons

            // Fetch playlist by author
            match nostr_music::fetch_playlists(Some(pubkey), 50).await {
                Ok(playlists) => {
                    if let Some(pl) = playlists.into_iter().find(|p| p.d_tag == d_tag) {
                        // Fetch creator profile
                        if let Ok(profile) = profiles::fetch_profile(pl.pubkey.clone()).await {
                            creator_name.set(profile.get_display_name());
                        }

                        // Resolve playlist tracks
                        match nostr_music::resolve_playlist_tracks(&pl).await {
                            Ok(nostr_tracks) => {
                                let music_tracks: Vec<music_player::MusicTrack> = nostr_tracks
                                    .into_iter()
                                    .map(|t| t.into())
                                    .collect();
                                tracks.set(music_tracks);
                            }
                            Err(e) => {
                                log::error!("Failed to resolve playlist tracks: {}", e);
                            }
                        }

                        playlist.set(Some(pl));
                    } else {
                        error_msg.set(Some("Playlist not found".to_string()));
                    }
                }
                Err(e) => {
                    error_msg.set(Some(format!("Failed to load playlist: {}", e)));
                }
            }
            loading.set(false);
        });
    });

    let play_playlist = move |_| {
        let playlist_tracks = tracks.read().clone();
        if let Some(first_track) = playlist_tracks.first().cloned() {
            music_player::play_track(first_track, Some(playlist_tracks), Some(0));
        }
    };

    let play_track = move |track: music_player::MusicTrack, index: usize| {
        let playlist_tracks = tracks.read().clone();
        music_player::play_track(track, Some(playlist_tracks), Some(index));
    };

    rsx! {
        div {
            class: "max-w-4xl mx-auto p-4",

            // Back button
            a {
                href: "/music",
                class: "inline-flex items-center gap-2 text-muted-foreground hover:text-foreground mb-6 transition",
                svg {
                    xmlns: "http://www.w3.org/2000/svg",
                    class: "w-4 h-4",
                    fill: "none",
                    view_box: "0 0 24 24",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M10 19l-7-7m0 0l7-7m-7 7h18"
                    }
                }
                "Back to Music"
            }

            // Loading state
            if *loading.read() {
                div {
                    class: "space-y-6",
                    // Header skeleton
                    div {
                        class: "flex items-start gap-6 animate-pulse",
                        div {
                            class: "w-48 h-48 bg-muted rounded-lg flex-shrink-0"
                        }
                        div {
                            class: "flex-1 space-y-4",
                            div {
                                class: "h-8 bg-muted rounded w-3/4"
                            }
                            div {
                                class: "h-4 bg-muted rounded w-1/2"
                            }
                            div {
                                class: "h-4 bg-muted rounded w-1/3"
                            }
                        }
                    }
                    // Track list skeleton
                    div {
                        class: "space-y-2 mt-8",
                        for i in 0..5 {
                            UnifiedTrackCardSkeleton { key: "{i}" }
                        }
                    }
                }
            }

            // Error state
            else if let Some(err) = error_msg.read().clone() {
                div {
                    class: "text-center py-16",
                    div {
                        class: "w-16 h-16 mx-auto mb-4 rounded-full bg-destructive/10 flex items-center justify-center",
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
                                d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                            }
                        }
                    }
                    p {
                        class: "text-muted-foreground font-medium",
                        "{err}"
                    }
                    a {
                        href: "/music",
                        class: "inline-flex items-center gap-2 mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition text-sm font-medium",
                        "Back to Music"
                    }
                }
            }

            // Playlist content
            else if let Some(pl) = playlist.read().clone() {
                div {
                    class: "space-y-8",

                    // Playlist header
                    div {
                        class: "flex flex-col sm:flex-row items-start gap-6 p-6 bg-card rounded-xl border border-border",

                        // Cover image
                        div {
                            class: "w-48 h-48 rounded-lg overflow-hidden bg-muted flex-shrink-0",
                            if let Some(ref image) = pl.image {
                                img {
                                    src: "{image}",
                                    alt: "{pl.title}",
                                    class: "w-full h-full object-cover"
                                }
                            } else {
                                div {
                                    class: "w-full h-full bg-gradient-to-br from-purple-500/30 to-blue-500/30 flex items-center justify-center",
                                    svg {
                                        xmlns: "http://www.w3.org/2000/svg",
                                        class: "w-16 h-16 text-muted-foreground/50",
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
                        }

                        // Playlist info
                        div {
                            class: "flex-1 min-w-0",

                            span {
                                class: "text-xs font-medium text-muted-foreground uppercase tracking-wide",
                                "Playlist"
                            }

                            h1 {
                                class: "text-3xl font-bold mt-1",
                                "{pl.title}"
                            }

                            if let Some(ref desc) = pl.description {
                                p {
                                    class: "text-muted-foreground mt-2",
                                    "{desc}"
                                }
                            }

                            div {
                                class: "flex items-center gap-4 mt-4 text-sm text-muted-foreground",
                                span {
                                    "by {creator_name}"
                                }
                                span { "·" }
                                span {
                                    "{pl.track_refs.len()} tracks"
                                }
                                if pl.is_collaborative {
                                    span { "·" }
                                    span {
                                        class: "text-primary",
                                        "Collaborative"
                                    }
                                }
                            }

                            // Categories
                            if !pl.categories.is_empty() {
                                div {
                                    class: "flex flex-wrap gap-2 mt-4",
                                    for cat in pl.categories.iter() {
                                        span {
                                            key: "{cat}",
                                            class: "px-2 py-1 bg-muted rounded-full text-xs",
                                            "{cat}"
                                        }
                                    }
                                }
                            }

                            // Actions
                            div {
                                class: "flex items-center gap-3 mt-6",
                                button {
                                    class: "px-6 py-3 bg-primary text-primary-foreground rounded-full font-medium hover:bg-primary/90 transition flex items-center gap-2 disabled:opacity-50",
                                    disabled: tracks.read().is_empty(),
                                    onclick: play_playlist,
                                    svg {
                                        xmlns: "http://www.w3.org/2000/svg",
                                        class: "w-5 h-5",
                                        fill: "currentColor",
                                        view_box: "0 0 24 24",
                                        path {
                                            d: "M8 5v14l11-7z"
                                        }
                                    }
                                    "Play"
                                }
                            }
                        }
                    }

                    // Track list
                    div {
                        class: "space-y-1",

                        h2 {
                            class: "text-lg font-semibold mb-4",
                            "Tracks"
                        }

                        if tracks.read().is_empty() {
                            div {
                                class: "text-center py-12 text-muted-foreground",
                                p { "No tracks in this playlist yet." }
                            }
                        } else {
                            div {
                                class: "divide-y divide-border/50",
                                for (index, track) in tracks.read().iter().enumerate() {
                                    {
                                        let track_clone = track.clone();
                                        rsx! {
                                            div {
                                                key: "{track.id}",
                                                class: "cursor-pointer",
                                                onclick: move |_| play_track(track_clone.clone(), index),
                                                UnifiedTrackCard {
                                                    track: track.clone(),
                                                    show_album: false,
                                                    show_sats: true
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
    }
}
