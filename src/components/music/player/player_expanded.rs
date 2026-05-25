use crate::components::icons;
use crate::components::{ContentShareModal, ContentType};
use crate::routes::Route;
use crate::stores::music_player::{self, LoopMode, MusicPlayerStateStoreExt, PlayerViewMode, MUSIC_PLAYER};
use dioxus::prelude::*;

use super::ExpandedSeekBar;
use super::super::FALLBACK_ART_URL;

#[component]
pub fn PlayerExpanded() -> Element {
    let store = MUSIC_PLAYER.resolve();
    let track = store.current_track().cloned().unwrap();
    let is_playing = store.is_playing().cloned();
    let is_buffering = store.is_buffering().cloned();
    let volume = store.volume().cloned();
    let is_muted = store.is_muted().cloned();
    let playback_speed = store.playback_speed().cloned();
    let now_playing = store.now_playing().cloned();
    let playback_error = store.playback_error().cloned();
    let current_time = *store.current_time().read();
    let duration = *store.duration().read();

    let mut show_share_modal = use_signal(|| false);

    let art_url = track
        .album_art_url
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| FALLBACK_ART_URL.to_string());
    let mut img_src = use_signal(|| art_url.clone());
    {
        let url_for_sync = art_url.clone();
        use_effect(use_reactive((&url_for_sync,), move |(url,)| {
            img_src.set(url);
        }));
    }

    let share_url = track.share_url();
    let share_content_type = match &track.source {
        crate::stores::nostr_music::TrackSource::NostrPodcast { .. }
        | crate::stores::nostr_music::TrackSource::RssPodcast { .. } => ContentType::PodcastEpisode,
        crate::stores::nostr_music::TrackSource::Radio { .. } => ContentType::RadioStation,
        crate::stores::nostr_music::TrackSource::Bible { .. } => ContentType::BibleVerse,
        _ => ContentType::MusicTrack,
    };

    rsx! {
        div {
            class: "fixed inset-0 z-[60] bg-background flex flex-col",

            // Header
            div { class: "flex items-center justify-between px-4 pt-safe-top pb-2",
                div { class: "text-xs text-muted-foreground uppercase tracking-wider font-medium",
                    if track.is_live_stream { "Live Stream" }
                    else if track.is_podcast { "Now Playing" }
                    else { "Now Playing" }
                }
                button {
                    class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-full hover:bg-accent transition-colors",
                    onclick: move |_| music_player::set_view_mode(PlayerViewMode::Bar),
                    dangerous_inner_html: icons::CHEVRON_DOWN,
                }
            }

            // Content area
            div { class: "flex-1 flex flex-col items-center justify-center px-8 max-w-lg mx-auto w-full",

                // Album artwork
                div { class: "w-64 h-64 lg:w-80 lg:h-80 rounded-2xl overflow-hidden bg-muted shadow-2xl mb-8 shrink-0",
                    img {
                        src: "{img_src}",
                        alt: "Album art",
                        class: "w-full h-full object-cover",
                        referrerpolicy: "no-referrer",
                        onerror: move |_| img_src.set(FALLBACK_ART_URL.to_string()),
                    }
                }

                // Track info
                div { class: "w-full text-center mb-6",
                    div { class: "flex items-center justify-center gap-2 mb-1",
                        if track.is_live_stream {
                            span { class: "inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-bold uppercase bg-red-500/20 text-red-400 shrink-0",
                                span { class: "w-1.5 h-1.5 rounded-full bg-red-500 animate-pulse" }
                                "LIVE"
                            }
                        }
                        h2 { class: "text-xl font-bold truncate",
                            if track.is_live_stream {
                                if let Some(ref np) = now_playing {
                                    if let Some(display) = np.display_string() {
                                        "{display}"
                                    } else {
                                        "{track.title}"
                                    }
                                } else {
                                    "{track.title}"
                                }
                            } else {
                                "{track.title}"
                            }
                        }
                    }
                    p { class: "text-muted-foreground truncate",
                        if let Some(show_route) = track.get_show_route() {
                            Link {
                                to: show_route,
                                class: "hover:text-foreground hover:underline",
                                "{track.artist}"
                            }
                        } else if let Some(artist_id) = &track.artist_id {
                            Link {
                                to: Route::MusicArtist { artist_id: artist_id.clone() },
                                class: "hover:text-foreground hover:underline",
                                "{track.artist}"
                            }
                        } else {
                            "{track.artist}"
                        }
                    }
                    if let Some(ref error) = playback_error {
                        p { class: "text-xs text-red-400 mt-1 flex items-center justify-center gap-1",
                            icons::AlertTriangleIcon { class: "w-3 h-3 shrink-0".to_string() }
                            "{error}"
                        }
                    } else if is_buffering {
                        p { class: "text-xs text-muted-foreground mt-1 flex items-center justify-center gap-1",
                            icons::RefreshIcon { class: "w-3 h-3 animate-spin shrink-0".to_string() }
                            "Buffering..."
                        }
                    }
                }

                // Seek bar (isolated child component - only this re-renders on time ticks)
                if !track.is_live_stream {
                    ExpandedSeekBar { current_time, duration }
                }

                // Transport controls
                div { class: "flex items-center justify-center gap-3 mb-6",
                    {
                        let loop_mode = store.loop_mode();
                        let loop_active = *loop_mode.read() != LoopMode::None;
                        rsx! {
                            button {
                                class: format!("h-10 w-10 p-0 inline-flex items-center justify-center rounded-full transition-colors {}", if loop_active { "text-primary bg-primary/10" } else { "hover:bg-accent" }),
                                title: match *loop_mode.read() {
                                    LoopMode::None => "Repeat off",
                                    LoopMode::Queue => "Repeat queue",
                                    LoopMode::Track => "Repeat track",
                                },
                                onclick: move |_| music_player::toggle_loop(),
                                dangerous_inner_html: match *loop_mode.read() {
                                    LoopMode::Track => icons::REPEAT_ONE,
                                    _ => icons::REPEAT,
                                },
                            }
                        }
                    }
                    if track.is_podcast && !track.is_live_stream {
                        button {
                            class: "h-12 w-12 p-0 inline-flex items-center justify-center rounded-full hover:bg-accent transition-colors",
                            title: "Rewind 15 seconds",
                            onclick: move |_| music_player::skip_backward(15.0),
                            dangerous_inner_html: icons::REWIND_15,
                        }
                    } else if !track.is_live_stream {
                        button {
                            class: "h-12 w-12 p-0 inline-flex items-center justify-center rounded-full hover:bg-accent transition-colors",
                            onclick: move |_| music_player::previous_track(),
                            dangerous_inner_html: icons::SKIP_BACK,
                        }
                    }
                    button {
                        class: "h-16 w-16 p-0 inline-flex items-center justify-center rounded-full bg-primary hover:bg-primary/90 text-primary-foreground transition-colors shadow-lg",
                        onclick: move |_| music_player::toggle_play(),
                        dangerous_inner_html: if is_playing { icons::PAUSE } else { icons::PLAY },
                    }
                    if track.is_podcast && !track.is_live_stream {
                        button {
                            class: "h-12 w-12 p-0 inline-flex items-center justify-center rounded-full hover:bg-accent transition-colors",
                            title: "Forward 15 seconds",
                            onclick: move |_| music_player::skip_forward(15.0),
                            dangerous_inner_html: icons::FORWARD_15,
                        }
                    } else if !track.is_live_stream {
                        button {
                            class: "h-12 w-12 p-0 inline-flex items-center justify-center rounded-full hover:bg-accent transition-colors",
                            onclick: move |_| music_player::next_track(),
                            dangerous_inner_html: icons::SKIP_FORWARD,
                        }
                    }
                    {
                        let shuffle_active = *store.shuffle_enabled().read();
                        rsx! {
                            button {
                                class: format!("h-10 w-10 p-0 inline-flex items-center justify-center rounded-full transition-colors {}", if shuffle_active { "text-primary bg-primary/10" } else { "hover:bg-accent" }),
                                title: if shuffle_active { "Shuffle on" } else { "Shuffle off" },
                                onclick: move |_| music_player::toggle_shuffle(),
                                dangerous_inner_html: icons::SHUFFLE,
                            }
                        }
                    }
                }

                // Volume control
                if !track.is_live_stream {
                    div { class: "w-full flex items-center gap-2 mb-4 px-4",
                        button {
                            class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent transition-colors",
                            onclick: move |_| music_player::toggle_mute(),
                            dangerous_inner_html: if is_muted { icons::VOLUME_X } else { icons::VOLUME_2 },
                        }
                        div { class: "flex-1 relative",
                            input {
                                r#type: "range",
                                min: "0",
                                max: "100",
                                value: "{(volume * 100.0) as u32}",
                                class: "w-full h-2 appearance-none bg-secondary rounded-full cursor-pointer accent-primary [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary [&::-moz-range-thumb]:w-3 [&::-moz-range-thumb]:h-3 [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:bg-primary [&::-moz-range-thumb]:border-0",
                                oninput: move |evt| {
                                    if let Ok(value) = evt.value().parse::<f64>() {
                                        music_player::set_volume(value / 100.0);
                                    }
                                },
                            }
                        }
                    }
                }

                // Speed control (podcasts)
                if track.is_podcast {
                    div { class: "flex items-center gap-2 mb-4",
                        span { class: "text-sm text-muted-foreground",
                            "Speed:"
                        }
                        select {
                            class: "bg-transparent text-sm text-foreground cursor-pointer hover:text-foreground border border-border rounded-md px-2 py-1 focus:outline-hidden appearance-none",
                            value: "{playback_speed}",
                            onchange: move |evt| {
                                if let Ok(speed) = evt.value().parse::<f64>() {
                                    music_player::set_playback_speed(speed);
                                }
                            },
                            option { value: "0.5", "0.5x" }
                            option { value: "0.75", "0.75x" }
                            option { value: "1", "1x" }
                            option { value: "1.25", "1.25x" }
                            option { value: "1.5", "1.5x" }
                            option { value: "1.75", "1.75x" }
                            option { value: "2", "2x" }
                            option { value: "2.5", "2.5x" }
                            option { value: "3", "3x" }
                        }
                    }
                }

                // Action buttons
                div { class: "flex items-center justify-center gap-3",
                    button {
                        class: "h-10 px-4 inline-flex items-center justify-center gap-2 rounded-lg hover:bg-accent transition-colors text-sm",
                        onclick: move |_| show_share_modal.set(true),
                        dangerous_inner_html: icons::SHARE,
                        "Share"
                    }
                    button {
                        class: "h-10 px-4 inline-flex items-center justify-center gap-2 rounded-lg hover:bg-accent transition-colors text-sm",
                        onclick: move |_| music_player::show_zap_dialog(),
                        dangerous_inner_html: icons::ZAP,
                        "Zap"
                    }
                    button {
                        class: "h-10 px-4 inline-flex items-center justify-center gap-2 rounded-lg hover:bg-accent transition-colors text-sm text-destructive",
                        onclick: move |_| music_player::close_player(),
                        dangerous_inner_html: icons::X,
                        "Close"
                    }
                }
            }
        }
        if *show_share_modal.read() {
            ContentShareModal {
                title: format!("{} - {}", track.title, track.artist),
                url: share_url.clone(),
                content_type: share_content_type,
                image_url: track.album_art_url.clone(),
                on_close: move |_| show_share_modal.set(false),
            }
        }
    }
}
