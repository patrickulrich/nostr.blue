use crate::components::icons;
use crate::components::{ContentShareModal, ContentType};
use crate::routes::Route;
use crate::stores::music_player::{self, PlayerViewMode, MUSIC_PLAYER};
use dioxus::prelude::*;

use super::format_time;

#[component]
pub fn PlayerExpanded() -> Element {
    let state = MUSIC_PLAYER.read().clone();
    let track = state.current_track.as_ref().unwrap();
    let progress = if state.duration > 0.0 {
        (state.current_time / state.duration * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    let mut is_scrubbing = use_signal(|| false);
    let mut scrub_position = use_signal(|| None::<f64>);
    let mut show_share_modal = use_signal(|| false);

    let (share_url, share_content_type) = match &track.source {
        crate::stores::nostr_music::TrackSource::Wavlake { .. } => (
            format!("https://nostr.blue/music/track/{}", track.id),
            ContentType::MusicTrack,
        ),
        crate::stores::nostr_music::TrackSource::Nostr { coordinate, .. } => (
            format!("https://nostr.blue/music/track/{}", coordinate),
            ContentType::MusicTrack,
        ),
        crate::stores::nostr_music::TrackSource::NostrPodcast { coordinate, .. } => (
            format!("https://nostr.blue/podcast/episode/{}", coordinate),
            ContentType::PodcastEpisode,
        ),
        crate::stores::nostr_music::TrackSource::RssPodcast { .. } => (
            format!("https://nostr.blue/music/track/{}", track.id),
            ContentType::PodcastEpisode,
        ),
        crate::stores::nostr_music::TrackSource::RssMusic { .. } => (
            format!("https://nostr.blue/music/track/{}", track.id),
            ContentType::MusicTrack,
        ),
        crate::stores::nostr_music::TrackSource::Radio { .. } => (
            format!("https://nostr.blue/music/track/{}", track.id),
            ContentType::MusicTrack,
        ),
        crate::stores::nostr_music::TrackSource::Bible { .. } => (
            format!("https://nostr.blue/music/track/{}", track.id),
            ContentType::MusicTrack,
        ),
    };

    let display_progress = if let Some(pos) = scrub_position() {
        pos
    } else {
        progress
    };
    let display_time = if let Some(pos) = scrub_position() {
        if state.duration > 0.0 {
            pos / 100.0 * state.duration
        } else {
            0.0
        }
    } else {
        state.current_time
    };

    rsx! {
        div {
            class: "fixed inset-0 z-[60] bg-background/98 flex flex-col",
            style: "backdrop-filter: blur(24px); -webkit-backdrop-filter: blur(24px);",

            // Header
            div { class: "flex items-center justify-between px-4 pt-safe-top pb-2",
                button {
                    class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-full hover:bg-accent transition-colors",
                    onclick: move |_| music_player::set_view_mode(PlayerViewMode::Bar),
                    dangerous_inner_html: icons::CHEVRON_DOWN,
                }
                div { class: "text-xs text-muted-foreground uppercase tracking-wider font-medium",
                    if track.is_live_stream { "Live Stream" }
                    else if track.is_podcast { "Now Playing" }
                    else { "Now Playing" }
                }
                button {
                    class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-full hover:bg-accent transition-colors",
                    title: "Minimize to floating",
                    onclick: move |_| music_player::minimize_to_floating(),
                    dangerous_inner_html: icons::MINIMIZE,
                }
            }

            // Content area
            div { class: "flex-1 flex flex-col items-center justify-center px-8 max-w-lg mx-auto w-full",

                // Album artwork
                div { class: "w-64 h-64 lg:w-80 lg:h-80 rounded-2xl overflow-hidden bg-muted shadow-2xl mb-8 shrink-0",
                    if let Some(art_url) = &track.album_art_url {
                        img {
                            src: "{art_url}",
                            alt: "Album art",
                            class: "w-full h-full object-cover",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center text-muted-foreground",
                            div { dangerous_inner_html: icons::MUSIC_NOTE }
                        }
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
                                if let Some(ref np) = state.now_playing {
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
                    if let Some(ref error) = state.playback_error {
                        p { class: "text-xs text-red-400 mt-1 flex items-center justify-center gap-1",
                            icons::AlertTriangleIcon { class: "w-3 h-3 shrink-0".to_string() }
                            "{error}"
                        }
                    } else if state.is_buffering {
                        p { class: "text-xs text-muted-foreground mt-1 flex items-center justify-center gap-1",
                            icons::RefreshIcon { class: "w-3 h-3 animate-spin shrink-0".to_string() }
                            "Buffering..."
                        }
                    }
                }

                // Seek bar (draggable)
                if !track.is_live_stream {
                    div { class: "w-full mb-4",
                        div {
                            class: "relative h-6 flex items-center cursor-pointer touch-none",
                            onpointerdown: move |evt: Event<PointerData>| {
                                is_scrubbing.set(true);
                                let client_x = evt.client_coordinates().x;
                                let el = evt.data.element_coordinates();
                                let width = el.x.max(1.0);
                                let percent = (client_x / width * 100.0).clamp(0.0, 100.0);
                                scrub_position.set(Some(percent));
                            },
                            onpointermove: move |evt: Event<PointerData>| {
                                if *is_scrubbing.read() {
                                    let client_x = evt.client_coordinates().x;
                                    let el = evt.data.element_coordinates();
                                    let width = el.x.max(1.0);
                                    let percent = (client_x / width * 100.0).clamp(0.0, 100.0);
                                    scrub_position.set(Some(percent));
                                }
                            },
                            onpointerup: move |_| {
                                if let Some(pos) = scrub_position() {
                                    let new_time = pos / 100.0 * state.duration;
                                    if new_time.is_finite() && new_time >= 0.0 {
                                        music_player::seek_to(new_time);
                                    }
                                }
                                is_scrubbing.set(false);
                                scrub_position.set(None);
                            },
                            onpointerleave: move |_| {
                                if *is_scrubbing.read() {
                                    if let Some(pos) = scrub_position() {
                                        let new_time = pos / 100.0 * state.duration;
                                        if new_time.is_finite() && new_time >= 0.0 {
                                            music_player::seek_to(new_time);
                                        }
                                    }
                                    is_scrubbing.set(false);
                                    scrub_position.set(None);
                                }
                            },
                            // Track background
                            div { class: "absolute inset-x-0 top-1/2 -translate-y-1/2 h-1.5 bg-secondary rounded-full",
                                div {
                                    class: "absolute h-full bg-primary rounded-full transition-[width] duration-75",
                                    style: "width: {display_progress}%",
                                }
                            }
                            // Thumb indicator
                            div {
                                class: "absolute top-1/2 -translate-y-1/2 w-4 h-4 bg-primary rounded-full shadow-md transition-[left] duration-75",
                                style: "left: calc({display_progress}% - 8px);",
                            }
                        }
                        div { class: "flex justify-between mt-1",
                            span { class: "text-xs text-muted-foreground",
                                "{format_time(display_time)}"
                            }
                            span { class: "text-xs text-muted-foreground",
                                "{format_time(state.duration)}"
                            }
                        }
                    }
                }

                // Transport controls
                div { class: "flex items-center justify-center gap-4 mb-6",
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
                        dangerous_inner_html: if state.is_playing { icons::PAUSE } else { icons::PLAY },
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
                }

                // Volume control
                if !track.is_live_stream {
                    div { class: "w-full flex items-center gap-2 mb-4 px-4",
                        button {
                            class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent transition-colors",
                            onclick: move |_| music_player::toggle_mute(),
                            dangerous_inner_html: if state.is_muted { icons::VOLUME_X } else { icons::VOLUME_2 },
                        }
                        div { class: "flex-1 relative",
                            input {
                                r#type: "range",
                                min: "0",
                                max: "100",
                                value: "{(state.volume * 100.0) as u32}",
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
                            value: "{state.playback_speed}",
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
