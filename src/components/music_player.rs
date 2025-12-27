use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use crate::routes::Route;
use crate::stores::music_player::{self, MUSIC_PLAYER};
use crate::utils::radio::NowPlaying;
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
    let mut is_seeking = use_signal(|| false);
    let audio_id = "global-music-player-audio";

    // Update audio element when track or playing state changes
    use_effect(move || {
        let state = MUSIC_PLAYER.read();
        if let Some(ref track) = state.current_track {
            let media_url = track.media_url.clone();
            let is_playing = state.is_playing;
            let _is_live_stream = track.is_live_stream;

            spawn(async move {
                // Properly escape strings using JSON serialization to prevent injection
                let audio_id_json = serde_json::to_string(&audio_id).unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());
                let media_url_json = serde_json::to_string(&media_url).unwrap_or_else(|_| "\"\"".to_string());
                let is_playing_literal = if is_playing { "true" } else { "false" };

                // Check if this is an HLS stream (only .m3u8 needs HLS.js)
                let is_hls = media_url.contains(".m3u8");

                let script = if is_hls {
                    // Use HLS manager only for .m3u8 streams
                    format!(
                        r#"
                        (async function() {{
                            try {{
                                let audio = document.getElementById({audio_id});
                                if (!audio) return;

                                // Skip if already playing this URL
                                if (audio.dataset.currentUrl === {media_url}) {{
                                    if ({is_playing} && audio.paused) {{
                                        audio.play().catch(e => console.log('Play failed:', e));
                                    }} else if (!{is_playing} && !audio.paused) {{
                                        audio.pause();
                                    }}
                                    return;
                                }}

                                if (window.hlsManager) {{
                                    const result = await window.hlsManager.attachToAudio({audio_id}, {media_url});
                                    console.log('[Radio] Stream attached:', result);
                                    audio.dataset.currentUrl = {media_url};
                                }}
                                if ({is_playing}) {{
                                    audio.play().catch(e => console.log('Play failed:', e));
                                }}
                            }} catch (e) {{
                                console.error('[Radio] Stream attach failed:', e);
                            }}
                        }})();
                        "#,
                        audio_id = audio_id_json,
                        media_url = media_url_json,
                        is_playing = is_playing_literal
                    )
                } else {
                    // Direct audio playback for non-HLS streams (MP3, AAC, OGG)
                    format!(
                        r#"
                        (function() {{
                            let audio = document.getElementById({audio_id});
                            if (!audio) return;

                            // Cleanup any existing HLS instance
                            if (window.hlsManager) {{
                                window.hlsManager.detach({audio_id});
                            }}

                            // Use dataset to track current URL (more reliable than audio.src comparison)
                            const urlChanged = audio.dataset.currentUrl !== {media_url};

                            if (urlChanged) {{
                                audio.dataset.currentUrl = {media_url};
                                audio.src = {media_url};
                                // For live streams, wait for canplay before playing
                                if ({is_playing}) {{
                                    audio.addEventListener('canplay', function onCanPlay() {{
                                        audio.removeEventListener('canplay', onCanPlay);
                                        audio.play().catch(e => console.log('Play failed:', e.name, e.message));
                                    }}, {{ once: true }});
                                    audio.load();
                                }}
                            }} else {{
                                // URL unchanged - just toggle play/pause
                                if ({is_playing} && audio.paused) {{
                                    audio.play().catch(e => console.log('Play failed:', e.name, e.message));
                                }} else if (!{is_playing} && !audio.paused) {{
                                    audio.pause();
                                }}
                            }}
                        }})();
                        "#,
                        audio_id = audio_id_json,
                        media_url = media_url_json,
                        is_playing = is_playing_literal
                    )
                };

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

    // Track last synced time to detect programmatic changes (skip buttons)
    let mut last_synced_time = use_signal(|| 0.0f64);

    // Sync current_time to audio element when changed programmatically (skip forward/backward)
    use_effect(move || {
        let state = MUSIC_PLAYER.read();
        let current_time = state.current_time;
        let last_time = last_synced_time();

        // Only sync if the time changed significantly (more than 0.5 second jump indicates programmatic change)
        // This prevents fighting with the ontimeupdate event that continuously syncs audio → state
        // Threshold aligned with JS check below for consistency
        if (current_time - last_time).abs() > 0.5 {
            last_synced_time.set(current_time);
            is_seeking.set(true);

            spawn(async move {
                let audio_id_json = serde_json::to_string(&audio_id)
                    .unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());

                let script = format!(
                    r#"
                    (function() {{
                        let audio = document.getElementById({audio_id});
                        if (!audio) return;
                        // Only seek if the difference is significant (avoids fighting with timeupdate)
                        if (Math.abs(audio.currentTime - {current_time}) > 0.5) {{
                            audio.currentTime = {current_time};
                        }}
                    }})();
                    "#,
                    audio_id = audio_id_json,
                    current_time = current_time
                );
                let _ = eval(&script);

                // Clear seeking flag after a short delay to allow the seek to complete
                #[cfg(target_family = "wasm")]
                {
                    gloo_timers::future::TimeoutFuture::new(500).await;
                }
                #[cfg(not(target_family = "wasm"))]
                {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                is_seeking.set(false);
            });
        }
    });

    // Memoize playback speed to ensure effect only runs when it changes
    let playback_speed = use_memo(move || MUSIC_PLAYER.read().playback_speed);

    // Sync playback speed to audio element
    use_effect(move || {
        let speed = playback_speed();

        spawn(async move {
            let audio_id_json = serde_json::to_string(&audio_id)
                .unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());

            let script = format!(
                r#"
                (function() {{
                    let audio = document.getElementById({audio_id});
                    if (audio) audio.playbackRate = {speed};
                }})();
                "#,
                audio_id = audio_id_json,
                speed = speed
            );
            let _ = eval(&script);
        });
    });

    // Memoize is_live to prevent effect re-running on every render
    let is_live = use_memo(move || {
        MUSIC_PLAYER.read().current_track.as_ref().map(|t| t.is_live_stream).unwrap_or(false)
    });

    // Clear now playing when switching away from live stream
    // Only clear if there's actually something to clear (avoid re-render loop)
    use_effect(move || {
        if !is_live() && MUSIC_PLAYER.read().now_playing.is_some() {
            music_player::clear_now_playing();
        }
    });

    // Poll for HLS now-playing metadata (for live streams) using a coroutine
    let _now_playing_poller = use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        loop {
            // Wait 2 seconds between checks
            gloo_timers::future::TimeoutFuture::new(2000).await;

            // Only poll when playing a live stream
            if !is_live() {
                continue;
            }

            // Read hlsManager.nowPlaying from JavaScript
            let result = eval(r#"
                (function() {
                    if (window.hlsManager && window.hlsManager.nowPlaying) {
                        return JSON.stringify(window.hlsManager.nowPlaying);
                    }
                    return null;
                })()
            "#);

            if let Ok(js_value) = result {
                if let Some(json_str) = js_value.as_string() {
                    if let Ok(now_playing) = serde_json::from_str::<NowPlaying>(&json_str) {
                        if now_playing.has_data() {
                            music_player::set_now_playing(Some(now_playing));
                        }
                    }
                }
            }
        }
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
                    // Skip updates while programmatic seek is in progress
                    if *is_seeking.read() {
                        return;
                    }
                    if let Some(target) = evt.data.as_web_event().target() {
                        if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                            let current_time = audio.current_time();
                            if !current_time.is_nan() {
                                last_synced_time.set(current_time);
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
            crossorigin: "anonymous",
            preload: if track.is_live_stream { "none" } else { "metadata" },
            style: "display: none;",
            src: "{track.media_url}",
            ontimeupdate: move |evt| {
                // Skip updates while programmatic seek is in progress
                if *is_seeking.read() {
                    return;
                }
                if let Some(target) = evt.data.as_web_event().target() {
                    if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                        let current_time = audio.current_time();
                        if !current_time.is_nan() {
                            last_synced_time.set(current_time);
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
            },
            onerror: move |_evt| {
                // HTML5 audio error codes:
                // MEDIA_ERR_ABORTED (1), MEDIA_ERR_NETWORK (2),
                // MEDIA_ERR_DECODE (3), MEDIA_ERR_SRC_NOT_SUPPORTED (4)
                log::warn!("Audio playback error, attempting fallback...");

                // Try next stream if available
                if !music_player::try_next_stream() {
                    // All streams failed - show error to user
                    log::error!("All streams failed");
                }
                // Note: try_next_stream will set playback_error if all streams fail
                // The use_effect will detect the media_url change and try the new stream
            },
            onwaiting: move |_| {
                music_player::set_buffering(true);
            },
            onplaying: move |_| {
                music_player::set_buffering(false);
                music_player::set_playback_error(None);
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
                        // Title row with LIVE badge
                        div {
                            class: "flex items-center gap-2",
                            // Show now playing title for live streams, or track title otherwise
                            if track.is_live_stream {
                                if let Some(ref np) = state.now_playing {
                                    if let Some(display) = np.display_string() {
                                        // Now playing from HLS metadata
                                        div {
                                            class: "font-semibold text-sm truncate text-primary",
                                            "{display}"
                                        }
                                    } else {
                                        div {
                                            class: "font-semibold text-sm truncate",
                                            "{track.title}"
                                        }
                                    }
                                } else {
                                    div {
                                        class: "font-semibold text-sm truncate",
                                        "{track.title}"
                                    }
                                }
                            } else if let Some(episode_route) = track.get_episode_route() {
                                Link {
                                    to: episode_route,
                                    class: "font-semibold text-sm truncate hover:text-primary hover:underline",
                                    "{track.title}"
                                }
                            } else {
                                div {
                                    class: "font-semibold text-sm truncate",
                                    "{track.title}"
                                }
                            }
                            // LIVE badge for live streams
                            if track.is_live_stream {
                                span {
                                    class: "inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-bold uppercase bg-red-500/20 text-red-400 flex-shrink-0",
                                    span {
                                        class: "w-1.5 h-1.5 rounded-full bg-red-500 animate-pulse"
                                    }
                                    "LIVE"
                                }
                            }
                        }
                        // Show error message if playback failed, otherwise show artist/station
                        if let Some(ref error) = state.playback_error {
                            div {
                                class: "text-xs text-red-400 truncate flex items-center gap-1",
                                icons::AlertTriangleIcon { class: "w-3 h-3 flex-shrink-0".to_string() }
                                "{error}"
                            }
                        } else if state.is_buffering {
                            div {
                                class: "text-xs text-muted-foreground truncate flex items-center gap-1",
                                icons::RefreshIcon { class: "w-3 h-3 animate-spin flex-shrink-0".to_string() }
                                "Buffering..."
                            }
                        } else if track.is_live_stream && state.now_playing.is_some() {
                            // For live streams with now playing, show station name as subtitle
                            div {
                                class: "text-xs text-muted-foreground truncate",
                                "{track.title}"
                            }
                        } else {
                            div {
                                class: "text-xs text-muted-foreground truncate",
                                // Link to show page (podcast) or artist page (music)
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
                        }
                    }
                }

                // Center: Controls, progress, and volume
                div {
                    class: "flex items-center gap-3 flex-1 justify-center max-w-2xl",

                    // Playback controls - different for podcasts vs music vs live streams
                    div {
                        class: "flex items-center gap-1",

                        if track.is_live_stream {
                            // Live stream controls: only play/pause (no seeking or skipping)
                            button {
                                class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-md bg-primary hover:bg-primary/90 text-primary-foreground transition-colors",
                                onclick: move |_| music_player::toggle_play(),
                                dangerous_inner_html: if state.is_playing {
                                    icons::PAUSE
                                } else {
                                    icons::PLAY
                                }
                            }
                        } else if track.is_podcast {
                            // Podcast controls: skip back 15s
                            button {
                                class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                                title: "Rewind 15 seconds",
                                onclick: move |_| music_player::skip_backward(15.0),
                                dangerous_inner_html: icons::REWIND_15
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

                            // Podcast controls: skip forward 15s
                            button {
                                class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                                title: "Forward 15 seconds",
                                onclick: move |_| music_player::skip_forward(15.0),
                                dangerous_inner_html: icons::FORWARD_15
                            }
                        } else {
                            // Music controls: previous/next track
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
                    }

                    // Progress bar with time stamps (hidden for live streams)
                    if !track.is_live_stream {
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

                    // Playback speed control (for podcasts only)
                    if track.is_podcast {
                        div {
                            class: "flex items-center gap-1 hidden md:flex",
                            title: "Playback speed",

                            span {
                                class: "w-4 h-4 text-muted-foreground",
                                dangerous_inner_html: icons::GAUGE
                            }

                            select {
                                class: "bg-transparent text-xs text-muted-foreground cursor-pointer hover:text-foreground border-none focus:outline-none appearance-none pr-4",
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
