use dioxus::prelude::*;
use crate::services::wavlake::WavlakeTrack;
use crate::stores::music_player::{self, MusicTrack};
use crate::components::icons;

#[derive(Props, Clone, PartialEq)]
pub struct TrackCardProps {
    pub track: WavlakeTrack,
    #[props(default = false)]
    pub show_album: bool,
}

/// Track card component for displaying a music track
#[component]
pub fn TrackCard(props: TrackCardProps) -> Element {
    let track = &props.track;
    let track_id = track.id.clone();
    let track_id_for_effect = track_id.clone();
    let mut is_playing = use_signal(|| false);

    // Check if this track is currently playing
    use_effect(move || {
        let player_state = music_player::MUSIC_PLAYER.read();
        if let Some(ref current) = player_state.current_track {
            is_playing.set(current.id == track_id_for_effect && player_state.is_playing);
        } else {
            is_playing.set(false);
        }
    });

    let handle_play = {
        let track = track.clone();
        let track_id_clone = track_id.clone();
        move |_| {
            let player_state = music_player::MUSIC_PLAYER.read();

            // If this track is currently playing, toggle pause
            if let Some(ref current) = player_state.current_track {
                if current.id == track_id_clone && player_state.is_playing {
                    drop(player_state); // Release the read lock
                    music_player::toggle_play();
                    return;
                }
            }
            drop(player_state); // Release the read lock

            // Otherwise, play this track
            let music_track: MusicTrack = track.clone().into();
            music_player::play_track(music_track, None, None);
        }
    };

    // Format duration from seconds to MM:SS
    let duration_str = {
        let mins = track.duration / 60;
        let secs = track.duration % 60;
        format!("{:02}:{:02}", mins, secs)
    };

    rsx! {
        div {
            class: "flex items-center gap-3 p-3 hover:bg-muted/50 rounded-lg transition group",

            // Album art
            div {
                class: "relative flex-shrink-0",
                img {
                    src: "{track.album_art_url}",
                    alt: "Album art",
                    class: "w-14 h-14 rounded object-cover"
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
                    a {
                        href: "/music/artist/{track.artist_id}",
                        class: "hover:text-foreground hover:underline",
                        onclick: move |e| e.stop_propagation(),
                        "{track.artist}"
                    }
                }
                if props.show_album {
                    div {
                        class: "text-xs text-muted-foreground truncate",
                        a {
                            href: "/music/album/{track.album_id}",
                            class: "hover:text-foreground hover:underline",
                            onclick: move |e| e.stop_propagation(),
                            "{track.album_title}"
                        }
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

                // Vote button placeholder
                button {
                    class: "p-2 hover:bg-muted rounded-full transition",
                    title: "Vote for this track",
                    dangerous_inner_html: icons::HEART
                }

                // Zap button placeholder
                button {
                    class: "p-2 hover:bg-muted rounded-full transition",
                    title: "Zap this artist",
                    dangerous_inner_html: icons::ZAP
                }
            }
        }
    }
}

/// Skeleton loader for track card
#[component]
pub fn TrackCardSkeleton() -> Element {
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

            // Duration skeleton
            div {
                class: "w-12 h-3 bg-muted rounded flex-shrink-0"
            }
        }
    }
}
