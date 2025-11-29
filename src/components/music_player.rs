use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use crate::stores::music_player::{self, MUSIC_PLAYER};
use crate::components::icons;
use js_sys::eval;
use wasm_bindgen::JsCast;

/// Format seconds as M:SS
fn format_time(seconds: f64) -> String {
    if seconds.is_nan() {
        return "0:00".to_string();
    }
    let mins = (seconds / 60.0).floor() as u32;
    let secs = (seconds % 60.0).floor() as u32;
    format!("{}:{:02}", mins, secs)
}


/// Persistent music player that stays at bottom of screen
#[component]
pub fn PersistentMusicPlayer() -> Element {
    let state = MUSIC_PLAYER.read().clone();
    let _is_seeking = use_signal(|| false);
    let audio_id = "global-music-player-audio";

    // Update audio element when track or playing state changes
    use_effect(move || {
        let state = MUSIC_PLAYER.read();
        if let Some(ref track) = state.current_track {
            let media_url = track.media_url.clone();
            let is_playing = state.is_playing;

            spawn(async move {
                // Properly escape strings using JSON serialization to prevent injection
                let audio_id_json = serde_json::to_string(&audio_id).unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());
                let media_url_json = serde_json::to_string(&media_url).unwrap_or_else(|_| "\"\"".to_string());
                let is_playing_literal = if is_playing { "true" } else { "false" };

                let script = format!(
                    r#"
                    (function() {{
                        let audio = document.getElementById({audio_id});
                        if (!audio) return;

                        if (audio.src !== {media_url}) {{
                            audio.src = {media_url};
                            audio.load();
                        }}

                        if ({is_playing}) {{
                            audio.play().catch(e => console.log('Play failed:', e));
                        }} else {{
                            audio.pause();
                        }}
                    }})();
                    "#,
                    audio_id = audio_id_json,
                    media_url = media_url_json,
                    is_playing = is_playing_literal
                );
                let _ = eval(&script);
            });
        }
    });

    // Update volume
    use_effect(move || {
        let state = MUSIC_PLAYER.read();
        let volume = if state.is_muted { 0.0 } else { state.volume };

        spawn(async move {
            // Properly escape audio_id using JSON serialization
            let audio_id_json = serde_json::to_string(&audio_id).unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());

            let script = format!(
                r#"
                (function() {{
                    let audio = document.getElementById({audio_id});
                    if (audio) audio.volume = {volume};
                }})();
                "#,
                audio_id = audio_id_json,
                volume = volume
            );
            let _ = eval(&script);
        });
    });


    // Don't render if player is not visible
    if !state.is_visible || state.current_track.is_none() {
        return rsx! {
            // Hidden audio element for playback
            audio {
                id: "{audio_id}",
                preload: "metadata",
                style: "display: none;",
                ontimeupdate: move |evt| {
                    if let Some(target) = evt.data.as_web_event().target() {
                        if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                            let current_time = audio.current_time();
                            if !current_time.is_nan() {
                                music_player::set_current_time(current_time);
                            }
                        }
                    }
                },
                onloadedmetadata: move |evt| {
                    if let Some(target) = evt.data.as_web_event().target() {
                        if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                            let duration = audio.duration();
                            if !duration.is_nan() {
                                music_player::set_duration(duration);
                            }
                        }
                    }
                },
                onended: move |_| {
                    music_player::next_track();
                }
            }
        };
    }

    let track = state.current_track.as_ref().unwrap();

    let progress = if state.duration > 0.0 {
        (state.current_time / state.duration * 100.0).min(100.0)
    } else {
        0.0
    };

    rsx! {
        // Hidden audio element
        audio {
            id: "{audio_id}",
            preload: "metadata",
            style: "display: none;",
            src: "{track.media_url}",
            ontimeupdate: move |evt| {
                if let Some(target) = evt.data.as_web_event().target() {
                    if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                        let current_time = audio.current_time();
                        if !current_time.is_nan() {
                            music_player::set_current_time(current_time);
                        }
                    }
                }
            },
            onloadedmetadata: move |evt| {
                if let Some(target) = evt.data.as_web_event().target() {
                    if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                        let duration = audio.duration();
                        if !duration.is_nan() {
                            music_player::set_duration(duration);
                        }
                    }
                }
            },
            onended: move |_| {
                music_player::next_track();
            }
        }

        div {
            class: "fixed bottom-0 left-0 right-0 bg-background/95 backdrop-blur border-t border-border shadow-lg z-50",
            style: "backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);",

            // Player controls
            div {
                class: "flex items-center justify-between w-full py-4 px-4 gap-3",

                // Left: Track info (fixed width on desktop)
                div {
                    class: "flex items-center gap-3 min-w-0 w-80",

                    div {
                        class: "w-12 h-12 rounded-lg overflow-hidden bg-muted flex-shrink-0",
                        if let Some(art_url) = &track.album_art_url {
                            img {
                                src: "{art_url}",
                                alt: "Album art",
                                class: "w-full h-full object-cover",
                                loading: "lazy"
                            }
                        }
                    }

                    div {
                        class: "flex flex-col min-w-0",
                        div {
                            class: "font-semibold text-sm truncate",
                            "{track.title}"
                        }
                        div {
                            class: "text-xs text-muted-foreground truncate",
                            if let Some(artist_id) = &track.artist_id {
                                a {
                                    href: "/music/artist/{artist_id}",
                                    class: "hover:text-foreground hover:underline",
                                    "{track.artist}"
                                }
                            } else {
                                "{track.artist}"
                            }
                        }
                    }
                }

                // Center: Controls, progress, and volume
                div {
                    class: "flex items-center gap-3 flex-1 justify-center max-w-2xl",

                    // Playback controls
                    div {
                        class: "flex items-center gap-1",

                        // Previous button
                        button {
                            class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                            onclick: move |_| music_player::previous_track(),
                            dangerous_inner_html: icons::SKIP_BACK
                        }

                        // Play/Pause button
                        button {
                            class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-md bg-primary hover:bg-primary/90 text-primary-foreground transition-colors",
                            onclick: move |_| music_player::toggle_play(),
                            dangerous_inner_html: if state.is_playing {
                                icons::PAUSE
                            } else {
                                icons::PLAY
                            }
                        }

                        // Next button
                        button {
                            class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                            onclick: move |_| music_player::next_track(),
                            dangerous_inner_html: icons::SKIP_FORWARD
                        }
                    }

                    // Progress bar with time stamps
                    div {
                        class: "flex items-center gap-2 flex-1 max-w-md",

                        span {
                            class: "text-xs text-muted-foreground w-8 text-right",
                            "{format_time(state.current_time)}"
                        }

                        // Progress slider
                        div {
                            class: "flex-1 relative h-2 bg-secondary rounded-full overflow-hidden cursor-pointer",
                            onclick: move |evt| {
                                let client_x = evt.client_coordinates().x;
                                let client_y = evt.client_coordinates().y;
                                let audio_id_str = audio_id.to_string();

                                spawn(async move {
                                    // Properly escape audio_id using JSON serialization
                                    let audio_id_json = serde_json::to_string(&audio_id_str).unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());

                                    let script = format!(
                                        r#"
                                        (function() {{
                                            let audio = document.getElementById({audio_id});
                                            if (!audio) return;

                                            let element = document.elementFromPoint({client_x}, {client_y});
                                            if (!element) return;

                                            // Find the progress bar element (it might be the clicked element or an ancestor)
                                            let progressBar = element.closest('.cursor-pointer') || element;
                                            let rect = progressBar.getBoundingClientRect();

                                            let percent = Math.max(0, Math.min(1, ({client_x} - rect.left) / rect.width));
                                            let newTime = percent * audio.duration;

                                            if (!isNaN(newTime) && isFinite(newTime)) {{
                                                audio.currentTime = newTime;
                                            }}
                                        }})();
                                        "#,
                                        audio_id = audio_id_json,
                                        client_x = client_x,
                                        client_y = client_y
                                    );
                                    let _ = eval(&script);
                                });
                            },

                            // Filled progress
                            div {
                                class: "absolute h-full bg-primary transition-all duration-100",
                                style: "width: {progress}%"
                            }
                        }

                        span {
                            class: "text-xs text-muted-foreground w-8",
                            "{format_time(state.duration)}"
                        }
                    }

                    // Volume control (moved here, next to progress)
                    div {
                        class: "flex items-center gap-1 hidden md:flex",

                        button {
                            class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                            onclick: move |_| music_player::toggle_mute(),
                            dangerous_inner_html: if state.is_muted {
                                icons::VOLUME_X
                            } else {
                                icons::VOLUME_2
                            }
                        }

                        // Volume slider
                        div {
                            class: "relative w-16",

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
                                }
                            }
                        }
                    }
                }

                // Right: Vote, Zap, and Close
                div {
                    class: "flex items-center gap-1",

                    // Vote button
                    button {
                        class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                        title: "Vote for this track",
                        onclick: {
                            let vote_track = track.clone();
                            move |_| {
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
                        class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                        title: "Zap the artist",
                        onclick: move |_| music_player::show_zap_dialog(),
                        dangerous_inner_html: icons::ZAP
                    }

                    // Close button
                    button {
                        class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                        onclick: move |_| music_player::close_player(),
                        dangerous_inner_html: icons::X
                    }
                }
            }
        }
    }
}
