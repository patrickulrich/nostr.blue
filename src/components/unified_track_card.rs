// Unified Track Card Component
// Handles both Wavlake and Nostr tracks with source-aware zaps and prominent sats display

use dioxus::prelude::*;
use crate::routes::Route;
use crate::stores::music_player::{self, MusicTrack};
use crate::stores::nostr_music::TrackSource;
use crate::stores::profiles;
use crate::components::icons;

#[derive(Props, Clone, PartialEq)]
pub struct UnifiedTrackCardProps {
    pub track: MusicTrack,
    #[props(default = false)]
    pub show_album: bool,
    #[props(default = true)]
    pub show_source_badge: bool,
    #[props(default = true)]
    pub show_sats: bool,
    /// Optional playlist to enable continuous playback
    #[props(default)]
    pub playlist: Option<Vec<MusicTrack>>,
}

/// Unified track card that handles both Wavlake and Nostr tracks
#[component]
pub fn UnifiedTrackCard(props: UnifiedTrackCardProps) -> Element {
    let track = props.track.clone();
    let track_id = track.id.clone();
    let track_id_for_memo = track_id.clone();

    // Reactively check if this track is currently playing
    // Use memo so it re-evaluates when MUSIC_PLAYER changes
    let is_playing = use_memo(move || {
        let player_state = music_player::MUSIC_PLAYER.read();
        if let Some(ref current) = player_state.current_track {
            current.id == track_id_for_memo && player_state.is_playing
        } else {
            false
        }
    });

    // For nostr tracks, we need to fetch the artist profile
    let artist_pubkey = track.artist_npub.clone();
    let artist_is_empty = track.artist.is_empty();
    let mut artist_name = use_signal(|| track.artist.clone());

    // Fetch artist name from profile for nostr tracks
    use_effect(move || {
        if let Some(pubkey) = artist_pubkey.clone() {
            if artist_is_empty {
                // Look up profile for artist name
                spawn(async move {
                    if let Ok(profile) = profiles::fetch_profile(pubkey).await {
                        artist_name.set(profile.get_display_name());
                    }
                });
            }
        }
    });

    let playlist = props.playlist.clone();
    let handle_play = {
        let track = track.clone();
        let track_id_clone = track_id.clone();
        let playlist = playlist.clone();
        move |_| {
            let player_state = music_player::MUSIC_PLAYER.read();

            // If this track is currently playing, toggle pause
            if let Some(ref current) = player_state.current_track {
                if current.id == track_id_clone && player_state.is_playing {
                    drop(player_state);
                    music_player::toggle_play();
                    return;
                }
            }
            drop(player_state);

            // Otherwise, play this track (with playlist if provided)
            music_player::play_track(track.clone(), playlist.clone(), None);
        }
    };

    // Format duration from seconds to MM:SS
    let duration_str = track.duration.map(|d| {
        let mins = d / 60;
        let secs = d % 60;
        format!("{:02}:{:02}", mins, secs)
    }).unwrap_or_else(|| "--:--".to_string());

    // Format sats total
    let sats_display = track.msat_total.map(|msats| {
        let sats = msats / 1000;
        if sats >= 1_000_000 {
            format!("{}M sats", sats / 1_000_000)
        } else if sats >= 1_000 {
            format!("{}K sats", sats / 1_000)
        } else {
            format!("{} sats", sats)
        }
    });

    // Determine source badge
    let source_info = match &track.source {
        TrackSource::Wavlake { .. } => ("W", "Wavlake", "bg-orange-500/20 text-orange-400"),
        TrackSource::Nostr { .. } => ("N", "Nostr", "bg-purple-500/20 text-purple-400"),
    };

    // Get artwork URL with fallback
    let artwork_url = track.album_art_url.clone()
        .unwrap_or_else(|| "https://api.dicebear.com/7.x/shapes/svg?seed=music".to_string());

    // Build artist route based on source (both go to music artist page)
    let artist_route = match &track.source {
        TrackSource::Wavlake { artist_id, .. } => Route::MusicArtist { artist_id: artist_id.clone() },
        TrackSource::Nostr { pubkey, .. } => Route::MusicArtist { artist_id: pubkey.clone() },
    };

    rsx! {
        div {
            class: "flex items-center gap-3 p-3 hover:bg-muted/50 rounded-lg transition group",

            // Album art with source badge
            div {
                class: "relative flex-shrink-0",
                img {
                    src: "{artwork_url}",
                    alt: "Album art",
                    class: "w-14 h-14 rounded object-cover",
                    loading: "lazy"
                }

                // Source badge
                if props.show_source_badge {
                    div {
                        class: "absolute -top-1 -right-1 w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold {source_info.2}",
                        title: "{source_info.1}",
                        "{source_info.0}"
                    }
                }

                // Play button overlay
                button {
                    class: "absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 group-hover:opacity-100 transition rounded",
                    onclick: handle_play,
                    dangerous_inner_html: if *is_playing.read() {
                        icons::PAUSE
                    } else {
                        icons::PLAY
                    }
                }
            }

            // Track info
            div {
                class: "flex-1 min-w-0",
                div {
                    class: "font-medium text-sm truncate",
                    if *is_playing.read() {
                        span {
                            class: "text-primary",
                            "{track.title}"
                        }
                    } else {
                        "{track.title}"
                    }
                }
                div {
                    class: "text-xs text-muted-foreground truncate",
                    Link {
                        to: artist_route.clone(),
                        class: "hover:text-foreground hover:underline",
                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        "{artist_name}"
                    }
                }
                if props.show_album {
                    if let Some(ref album) = track.album {
                        div {
                            class: "text-xs text-muted-foreground truncate",
                            match &track.source {
                                TrackSource::Wavlake { album_id, .. } => rsx! {
                                    Link {
                                        to: Route::MusicAlbum { album_id: album_id.clone() },
                                        class: "hover:text-foreground hover:underline",
                                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                                        "{album}"
                                    }
                                },
                                TrackSource::Nostr { .. } => rsx! {
                                    span { "{album}" }
                                }
                            }
                        }
                    }
                }
            }

            // Sats display (prominent)
            if props.show_sats {
                if let Some(sats) = &sats_display {
                    div {
                        class: "flex items-center gap-1 text-xs font-medium text-amber-500 flex-shrink-0",
                        dangerous_inner_html: icons::ZAP,
                        span { "{sats}" }
                    }
                }
            }

            // Duration
            div {
                class: "text-xs text-muted-foreground flex-shrink-0",
                "{duration_str}"
            }

            // Actions (vote, zap)
            div {
                class: "flex items-center gap-1 flex-shrink-0 opacity-0 group-hover:opacity-100 transition",

                // Vote button
                button {
                    class: "p-2 hover:bg-muted rounded-full transition",
                    title: "Vote for this track",
                    onclick: {
                        let vote_track = track.clone();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            let t = vote_track.clone();
                            spawn(async move {
                                if let Err(e) = music_player::vote_for_music(&t).await {
                                    log::error!("Vote failed: {}", e);
                                }
                            });
                        }
                    },
                    dangerous_inner_html: icons::HEART
                }

                // Zap button
                button {
                    class: "p-2 hover:bg-muted rounded-full transition",
                    title: "Zap this artist",
                    onclick: {
                        let zap_track = track.clone();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            music_player::show_zap_dialog_for_track(Some(zap_track.clone()));
                        }
                    },
                    dangerous_inner_html: icons::ZAP
                }
            }
        }
    }
}

/// Skeleton loader for unified track card
#[component]
pub fn UnifiedTrackCardSkeleton() -> Element {
    rsx! {
        div {
            class: "flex items-center gap-3 p-3 rounded-lg animate-pulse",

            // Album art skeleton
            div {
                class: "w-14 h-14 bg-muted rounded flex-shrink-0"
            }

            // Track info skeleton
            div {
                class: "flex-1 min-w-0 space-y-2",
                div {
                    class: "h-4 bg-muted rounded w-3/4"
                }
                div {
                    class: "h-3 bg-muted rounded w-1/2"
                }
            }

            // Sats skeleton
            div {
                class: "w-16 h-4 bg-muted rounded flex-shrink-0"
            }

            // Duration skeleton
            div {
                class: "w-12 h-3 bg-muted rounded flex-shrink-0"
            }
        }
    }
}
