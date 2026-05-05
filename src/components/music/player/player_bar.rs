use crate::components::icons;
use crate::components::{ContentShareModal, ContentType};
use crate::routes::Route;
use crate::stores::music_player::{self, MusicPlayerStateStoreExt, MUSIC_PLAYER, PlayerViewMode};
use dioxus::prelude::*;

use super::format_time;

#[component]
pub fn PlayerBar() -> Element {
    let store = MUSIC_PLAYER.resolve();
    let track = store.current_track().cloned().unwrap();
    let current_time = store.current_time().cloned();
    let duration = store.duration().cloned();
    let is_playing = store.is_playing().cloned();
    let is_buffering = store.is_buffering().cloned();
    let volume = store.volume().cloned();
    let is_muted = store.is_muted().cloned();
    let playback_speed = store.playback_speed().cloned();
    let now_playing = store.now_playing().cloned();
    let playback_error = store.playback_error().cloned();

    let progress = if duration > 0.0 {
        (current_time / duration * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    #[allow(unused_mut)]
    let mut is_seeking = use_signal(|| false);
    #[allow(unused_mut)]
    let mut seek_gen = use_signal(|| 0u32);
    let mut show_share_modal = use_signal(|| false);

    let share_url = track.share_url();
    let share_content_type = match &track.source {
        crate::stores::nostr_music::TrackSource::NostrPodcast { .. }
        | crate::stores::nostr_music::TrackSource::RssPodcast { .. } => ContentType::PodcastEpisode,
        crate::stores::nostr_music::TrackSource::Radio { .. } => ContentType::RadioStation,
        crate::stores::nostr_music::TrackSource::Bible { .. } => ContentType::BibleVerse,
        _ => ContentType::MusicTrack,
    };

    let on_seek_click = {
        #[cfg(feature = "web")]
        {
            let mut seek_gen = seek_gen;
            let mut is_seeking = is_seeking;
            move |evt: Event<MouseData>| {
                evt.stop_propagation();
                let client_x = evt.client_coordinates().x;
                let client_y = evt.client_coordinates().y;
                spawn(async move {
                    if let Some(window) = web_sys::window() {
                        if let Some(document) = window.document() {
                            let element = document.element_from_point(client_x as f32, client_y as f32);
                            if let Some(el) = element {
                                let closest_result = el.closest(".cursor-pointer");
                                let progress_bar = match closest_result {
                                    Ok(Some(closest_el)) => closest_el,
                                    _ => el,
                                };
                                let rect = progress_bar.get_bounding_client_rect();
                                let left = rect.left();
                                let width = rect.width();
                                let percent = ((client_x - left) / width).clamp(0.0, 1.0);
                                let new_time = percent * duration;
                                if new_time.is_finite() {
                                    let gen = seek_gen.with_mut(|g| {
                                        *g = g.wrapping_add(1);
                                        *g
                                    });
                                    is_seeking.set(true);
                                    music_player::seek_to(new_time);
                                    crate::platform::timer::sleep_ms(500).await;
                                    if *seek_gen.peek() == gen {
                                        is_seeking.set(false);
                                    }
                                }
                            }
                        }
                    }
                });
            }
        }
        #[cfg(all(not(feature = "web"), feature = "mobile_platform"))]
        {
            let mut seek_gen = seek_gen;
            let mut is_seeking = is_seeking;
            move |evt: Event<MouseData>| {
                evt.stop_propagation();
                let client_x = evt.client_coordinates().x;
                let client_y = evt.client_coordinates().y;
                spawn(async move {
                    let script = format!(
                        r#"
                        let element = document.elementFromPoint({client_x}, {client_y});
                        if (!element) return -1;
                        let progressBar = element.closest('.cursor-pointer') || element;
                        let rect = progressBar.getBoundingClientRect();
                        let percent = Math.max(0, Math.min(1, ({client_x} - rect.left) / rect.width));
                        return percent * {duration};
                        "#,
                        client_x = client_x,
                        client_y = client_y,
                        duration = duration,
                    );
                    if let Ok(result) = document::eval(&script).await {
                        let new_time = result
                            .as_f64()
                            .or_else(|| result.as_str().and_then(|s| s.parse::<f64>().ok()))
                            .unwrap_or(-1.0);
                        if new_time >= 0.0 && new_time.is_finite() {
                            let gen = seek_gen.with_mut(|g| {
                                *g = g.wrapping_add(1);
                                *g
                            });
                            is_seeking.set(true);
                            music_player::seek_to(new_time);
                            crate::platform::timer::sleep_ms(500).await;
                            if *seek_gen.peek() == gen {
                                is_seeking.set(false);
                            }
                        }
                    }
                });
            }
        }
        #[cfg(all(not(feature = "web"), not(feature = "mobile_platform")))]
        {
            let mut seek_gen = seek_gen;
            let mut is_seeking = is_seeking;
            move |evt: Event<MouseData>| {
                evt.stop_propagation();
                let client_x = evt.client_coordinates().x;
                let client_y = evt.client_coordinates().y;
                spawn(async move {
                    let audio_id_str = "global-music-player-audio";
                    let audio_id_json = serde_json::to_string(&audio_id_str)
                        .unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());
                    let script = format!(
                        r#"
                        (function() {{
                            let audio = document.getElementById({audio_id});
                            if (!audio) return null;
                            let element = document.elementFromPoint({client_x}, {client_y});
                            if (!element) return null;
                            let progressBar = element.closest('.cursor-pointer') || element;
                            let rect = progressBar.getBoundingClientRect();
                            let percent = Math.max(0, Math.min(1, ({client_x} - rect.width));
                            let newTime = percent * audio.duration;
                            if (!isNaN(newTime) && isFinite(newTime)) {{
                                audio.currentTime = newTime;
                                return newTime;
                            }}
                            return null;
                        }})();
                        "#,
                        audio_id = audio_id_json,
                        client_x = client_x,
                        client_y = client_y,
                    );
                    if let Ok(result) = document::eval(&script).await {
                        let new_time = result
                            .as_f64()
                            .or_else(|| result.as_str().and_then(|s| s.parse::<f64>().ok()))
                            .unwrap_or(-1.0);
                        if new_time >= 0.0 && new_time.is_finite() {
                            let gen = seek_gen.with_mut(|g| {
                                *g = g.wrapping_add(1);
                                *g
                            });
                            is_seeking.set(true);
                            music_player::seek_to(new_time);
                            crate::platform::timer::sleep_ms(500).await;
                            if *seek_gen.peek() == gen {
                                is_seeking.set(false);
                            }
                        }
                    }
                });
            }
        }
    };

    rsx! {
        div {
            class: "fixed bottom-0 left-0 right-0 bg-background/95 backdrop-blur border-t border-border shadow-lg z-50",
            style: "backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);",
            onclick: move |_| music_player::set_view_mode(PlayerViewMode::Expanded),
            div { class: "flex items-center justify-between w-full pt-3 px-4 gap-3 pb-safe-controls",
                // LEFT: Album art + title/artist
                div { class: "flex items-center gap-3 min-w-0 flex-1 md:flex-initial md:w-80",
                    div { class: "w-12 h-12 rounded-lg overflow-hidden bg-muted shrink-0",
                        if let Some(art_url) = &track.album_art_url {
                            img {
                                src: "{art_url}",
                                alt: "Album art",
                                class: "w-full h-full object-cover",
                                loading: "lazy",
                            }
                        }
                    }
                    div { class: "flex flex-col min-w-0",
                        div { class: "flex items-center gap-2",
                            if track.is_live_stream {
                                if let Some(ref np) = now_playing {
                                    if let Some(display) = np.display_string() {
                                        div { class: "font-semibold text-sm truncate text-primary",
                                            "{display}"
                                        }
                                    } else {
                                        div { class: "font-semibold text-sm truncate",
                                            "{track.title}"
                                        }
                                    }
                                } else {
                                    div { class: "font-semibold text-sm truncate", "{track.title}" }
                                }
                            } else if let Some(episode_route) = track.get_episode_route() {
                                Link {
                                    to: episode_route,
                                    onclick: move |evt: Event<MouseData>| { evt.stop_propagation(); },
                                    class: "font-semibold text-sm truncate hover:text-primary hover:underline",
                                    "{track.title}"
                                }
                            } else if let Some(track_route) = track.get_track_route() {
                                Link {
                                    to: track_route,
                                    onclick: move |evt: Event<MouseData>| { evt.stop_propagation(); },
                                    class: "font-semibold text-sm truncate hover:text-primary hover:underline",
                                    "{track.title}"
                                }
                            } else {
                                div { class: "font-semibold text-sm truncate", "{track.title}" }
                            }
                            if track.is_live_stream {
                                span { class: "inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-bold uppercase bg-red-500/20 text-red-400 shrink-0",
                                    span { class: "w-1.5 h-1.5 rounded-full bg-red-500 animate-pulse" }
                                    "LIVE"
                                }
                            }
                        }
                        if let Some(ref error) = playback_error {
                            div { class: "text-xs text-red-400 truncate flex items-center gap-1",
                                icons::AlertTriangleIcon { class: "w-3 h-3 shrink-0".to_string() }
                                "{error}"
                            }
                        } else if is_buffering {
                            div { class: "text-xs text-muted-foreground truncate flex items-center gap-1",
                                icons::RefreshIcon { class: "w-3 h-3 animate-spin shrink-0".to_string() }
                                "Buffering..."
                            }
                        } else if track.is_live_stream && now_playing.is_some() {
                            div { class: "text-xs text-muted-foreground truncate", "{track.title}" }
                        } else {
                            div { class: "text-xs text-muted-foreground truncate",
                                if let Some(show_route) = track.get_show_route() {
                                    Link {
                                        to: show_route,
                                        onclick: move |evt: Event<MouseData>| { evt.stop_propagation(); },
                                        class: "hover:text-foreground hover:underline",
                                        "{track.artist}"
                                    }
                                } else if let Some(artist_id) = &track.artist_id {
                                    Link {
                                        to: Route::MusicArtist {
                                            artist_id: artist_id.clone(),
                                        },
                                        onclick: move |evt: Event<MouseData>| { evt.stop_propagation(); },
                                        class: "hover:text-foreground hover:underline",
                                        "{track.artist}"
                                    }
                                } else {
                                    "{track.artist}"
                                }
                            }
                        }
                    }
                }

                // CENTER: Transport controls + progress bar (desktop only for progress)
                div { class: "flex items-center gap-3 flex-1 justify-center max-w-2xl hidden md:flex",
                    div { class: "flex items-center gap-1",
                        if track.is_live_stream {
                            button {
                                class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-md bg-primary hover:bg-primary/90 text-primary-foreground transition-colors",
                                onclick: move |evt| { evt.stop_propagation(); music_player::toggle_play(); },
                                dangerous_inner_html: if is_playing { icons::PAUSE } else { icons::PLAY },
                            }
                        } else if track.is_podcast {
                            button {
                                class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                                title: "Rewind 15 seconds",
                                onclick: move |evt| { evt.stop_propagation(); music_player::skip_backward(15.0); },
                                dangerous_inner_html: icons::REWIND_15,
                            }
                            button {
                                class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-md bg-primary hover:bg-primary/90 text-primary-foreground transition-colors",
                                onclick: move |evt| { evt.stop_propagation(); music_player::toggle_play(); },
                                dangerous_inner_html: if is_playing { icons::PAUSE } else { icons::PLAY },
                            }
                            button {
                                class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                                title: "Forward 15 seconds",
                                onclick: move |evt| { evt.stop_propagation(); music_player::skip_forward(15.0); },
                                dangerous_inner_html: icons::FORWARD_15,
                            }
                        } else {
                            button {
                                class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                                onclick: move |evt| { evt.stop_propagation(); music_player::previous_track(); },
                                dangerous_inner_html: icons::SKIP_BACK,
                            }
                            button {
                                class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-md bg-primary hover:bg-primary/90 text-primary-foreground transition-colors",
                                onclick: move |evt| { evt.stop_propagation(); music_player::toggle_play(); },
                                dangerous_inner_html: if is_playing { icons::PAUSE } else { icons::PLAY },
                            }
                            button {
                                class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                                onclick: move |evt| { evt.stop_propagation(); music_player::next_track(); },
                                dangerous_inner_html: icons::SKIP_FORWARD,
                            }
                        }
                    }
                    if !track.is_live_stream {
                        div { class: "flex items-center gap-2 flex-1 max-w-md",
                            span { class: "text-xs text-muted-foreground w-8 text-right",
                                "{format_time(current_time)}"
                            }
                            div {
                                class: "flex-1 relative h-2 bg-secondary rounded-full overflow-hidden cursor-pointer",
                                onclick: on_seek_click,
                                div {
                                    class: "absolute h-full bg-primary transition-all duration-100",
                                    style: "width: {progress}%",
                                }
                            }
                            span { class: "text-xs text-muted-foreground w-8",
                                "{format_time(duration)}"
                            }
                        }
                    }
                    // Volume (desktop only)
                    div { class: "flex items-center gap-1",
                        button {
                            class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                            onclick: move |evt| { evt.stop_propagation(); music_player::toggle_mute(); },
                            dangerous_inner_html: if is_muted { icons::VOLUME_X } else { icons::VOLUME_2 },
                        }
                        div { class: "relative w-16",
                            input {
                                r#type: "range",
                                min: "0",
                                max: "100",
                                value: "{(volume * 100.0) as u32}",
                                class: "w-full h-2 appearance-none bg-secondary rounded-full cursor-pointer accent-primary [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary [&::-moz-range-thumb]:w-3 [&::-moz-range-thumb]:h-3 [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:bg-primary [&::-moz-range-thumb]:border-0",
                                onclick: move |evt| { evt.stop_propagation(); },
                                oninput: move |evt| {
                                    if let Ok(value) = evt.value().parse::<f64>() {
                                        music_player::set_volume(value / 100.0);
                                    }
                                },
                            }
                        }
                    }
                    if track.is_podcast {
                        div {
                            class: "flex items-center gap-1",
                            title: "Playback speed",
                            span {
                                class: "w-4 h-4 text-muted-foreground",
                                dangerous_inner_html: icons::GAUGE,
                            }
                            select {
                                class: "bg-transparent text-xs text-muted-foreground cursor-pointer hover:text-foreground border-none focus:outline-hidden appearance-none pr-4",
                                value: "{playback_speed}",
                                onclick: move |evt| { evt.stop_propagation(); },
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
                }

                // MOBILE: Transport controls only (no scrubber/share/zap/volume/speed)
                div { class: "flex items-center gap-1 md:hidden",
                    if track.is_live_stream {
                        button {
                            class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-md bg-primary hover:bg-primary/90 text-primary-foreground transition-colors",
                            onclick: move |evt| { evt.stop_propagation(); music_player::toggle_play(); },
                            dangerous_inner_html: if is_playing { icons::PAUSE } else { icons::PLAY },
                        }
                    } else if track.is_podcast {
                        button {
                            class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                            onclick: move |evt| { evt.stop_propagation(); music_player::skip_backward(15.0); },
                            dangerous_inner_html: icons::REWIND_15,
                        }
                        button {
                            class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-md bg-primary hover:bg-primary/90 text-primary-foreground transition-colors",
                            onclick: move |evt| { evt.stop_propagation(); music_player::toggle_play(); },
                            dangerous_inner_html: if is_playing { icons::PAUSE } else { icons::PLAY },
                        }
                        button {
                            class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                            onclick: move |evt| { evt.stop_propagation(); music_player::skip_forward(15.0); },
                            dangerous_inner_html: icons::FORWARD_15,
                        }
                    } else {
                        button {
                            class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                            onclick: move |evt| { evt.stop_propagation(); music_player::previous_track(); },
                            dangerous_inner_html: icons::SKIP_BACK,
                        }
                        button {
                            class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-md bg-primary hover:bg-primary/90 text-primary-foreground transition-colors",
                            onclick: move |evt| { evt.stop_propagation(); music_player::toggle_play(); },
                            dangerous_inner_html: if is_playing { icons::PAUSE } else { icons::PLAY },
                        }
                        button {
                            class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                            onclick: move |evt| { evt.stop_propagation(); music_player::next_track(); },
                            dangerous_inner_html: icons::SKIP_FORWARD,
                        }
                    }
                }

                // RIGHT: Action buttons
                div { class: "flex items-center gap-1 shrink-0",
                    button {
                        class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                        title: "Minimize to floating",
                        onclick: move |evt| { evt.stop_propagation(); music_player::minimize_to_floating(); },
                        dangerous_inner_html: icons::MINIMIZE,
                    }
                    button {
                        class: "h-8 w-8 p-0 items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors hidden md:inline-flex",
                        title: "Share",
                        onclick: move |evt| { evt.stop_propagation(); show_share_modal.set(true); },
                        dangerous_inner_html: icons::SHARE,
                    }
                    button {
                        class: "h-8 w-8 p-0 items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors hidden md:inline-flex",
                        title: "Zap the artist",
                        onclick: move |evt| { evt.stop_propagation(); music_player::show_zap_dialog(); },
                        dangerous_inner_html: icons::ZAP,
                    }
                    button {
                        class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                        onclick: move |evt| { evt.stop_propagation(); music_player::close_player(); },
                        dangerous_inner_html: icons::X,
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
