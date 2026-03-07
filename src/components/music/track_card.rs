use crate::components::icons;
use crate::routes::Route;
use crate::services::wavlake::WavlakeTrack;
use crate::stores::music_player::{self, MusicTrack};
use dioxus::prelude::*;
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
        move |_| {
            let music_track: MusicTrack = track.clone().into();
            music_player::play_or_toggle_track(music_track, None, None);
        }
    };
    let duration_str = {
        let mins = track.duration / 60;
        let secs = track.duration % 60;
        format!("{:02}:{:02}", mins, secs)
    };
    rsx! {
        div {
            class: "flex items-center gap-3 p-3 hover:bg-muted/50 rounded-lg transition group cursor-pointer",
            onclick: handle_play,
            div { class: "relative shrink-0",
                img {
                    src: "{track.album_art_url}",
                    alt: "Album art",
                    class: "w-14 h-14 rounded object-cover",
                    loading: "lazy",
                }
                button {
                    class: "absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 group-hover:opacity-100 transition rounded",
                    dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY },
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "font-medium text-sm truncate",
                    if *is_playing.read() {
                        span { class: "text-primary", "{track.title}" }
                    } else {
                        "{track.title}"
                    }
                }
                div { class: "text-xs text-muted-foreground truncate",
                    Link {
                        to: Route::MusicArtist {
                            artist_id: track.artist_id.clone(),
                        },
                        class: "hover:text-foreground hover:underline",
                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        "{track.artist}"
                    }
                }
                if props.show_album {
                    div { class: "text-xs text-muted-foreground truncate",
                        Link {
                            to: Route::MusicAlbum {
                                album_id: track.album_id.clone(),
                            },
                            class: "hover:text-foreground hover:underline",
                            onclick: move |e: Event<MouseData>| e.stop_propagation(),
                            "{track.album_title}"
                        }
                    }
                }
            }
            div { class: "text-xs text-muted-foreground shrink-0", "{duration_str}" }
            div { class: "flex items-center gap-1 shrink-0 opacity-0 group-hover:opacity-100 transition",
                button {
                    class: "p-2 hover:bg-muted rounded-full transition",
                    title: "Vote for this track",
                    onclick: {
                        let vote_track: MusicTrack = track.clone().into();
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
                    dangerous_inner_html: icons::HEART,
                }
                button {
                    class: "p-2 hover:bg-muted rounded-full transition",
                    title: "Zap this artist",
                    onclick: {
                        let zap_track: MusicTrack = track.clone().into();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            music_player::show_zap_dialog_for_track(Some(zap_track.clone()));
                        }
                    },
                    dangerous_inner_html: icons::ZAP,
                }
            }
        }
    }
}
/// Skeleton loader for track card
#[component]
pub fn TrackCardSkeleton() -> Element {
    rsx! {
        div { class: "flex items-center gap-3 p-3 rounded-lg animate-pulse",
            div { class: "w-14 h-14 bg-muted rounded shrink-0" }
            div { class: "flex-1 min-w-0 space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
            }
            div { class: "w-12 h-3 bg-muted rounded shrink-0" }
        }
    }
}
