use crate::components::icons;
use crate::components::{ContentShareModal, ContentType};
use crate::routes::Route;
use crate::stores::music_player::{self, MUSIC_PLAYER};
use crate::utils::radio::NowPlaying;
use dioxus::prelude::*;
#[cfg(feature = "web")]
use dioxus::web::WebEventExt;
#[cfg(feature = "web")]
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
    let mut seek_gen = use_signal(|| 0u32);
    let mut show_share_modal = use_signal(|| false);
    let audio_id = "global-music-player-audio";
    // Inject HLS manager JS on non-web platforms.
    // On web, it loads via <script> tag in index.html.
    // On mobile, index.html is not used — Dioxus generates its own HTML.
    #[cfg(not(feature = "web"))]
    {
        use_effect(move || {
            spawn(async move {
                let check = document::eval(
                    "return typeof window.hlsManager !== 'undefined'",
                )
                .await;
                let loaded = check.ok().and_then(|v| v.as_bool()).unwrap_or(false);
                if !loaded {
                    log::info!("[Audio] Injecting HLS manager into WebView");
                    let hls_js = include_str!("../../../public/hls-manager.js");
                    if let Err(e) = document::eval(hls_js).await {
                        log::error!("[Audio] Failed to inject HLS manager: {:?}", e);
                    }
                }
            });
        });
    }
    // No Rust-side generation counter needed — the JS IIFEs guard against stale
    // execution via audio.dataset.currentUrl checks, making them idempotent
    // even when multiple async tasks overlap from rapid effect re-runs.
    use_effect(move || {
        let state = MUSIC_PLAYER.read();
        if let Some(ref track) = state.current_track {
            let media_url = track.media_url.clone();
            let is_playing = state.is_playing;
            let _is_live_stream = track.is_live_stream;
            {
                spawn(async move {
                    let audio_id_json = serde_json::to_string(&audio_id)
                        .unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());
                    let media_url_json = serde_json::to_string(&media_url)
                        .unwrap_or_else(|_| "\"\"".to_string());
                    let is_playing_literal = if is_playing { "true" } else { "false" };
                    let is_hls = media_url.contains(".m3u8");
                    let script = if is_hls {
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
                                        const result = await window.hlsManager.attachToMedia({audio_id}, {media_url});
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
                            is_playing = is_playing_literal,
                        )
                    } else {
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
                            is_playing = is_playing_literal,
                        )
                    };
                    let _ = document::eval(&script);
                });
            }
        }
    });
    use_effect(move || {
        let state = MUSIC_PLAYER.read();
        let volume = if state.is_muted { 0.0 } else { state.volume };
        {
            spawn(async move {
                let audio_id_json = serde_json::to_string(&audio_id)
                    .unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());
                let script = format!(
                    r#"
                    (function() {{
                        let audio = document.getElementById({audio_id});
                        if (audio) audio.volume = {volume};
                    }})();
                    "#,
                    audio_id = audio_id_json,
                    volume = volume,
                );
                let _ = document::eval(&script);
            });
        }
    });
    let mut last_synced_time = use_signal(|| 0.0f64);
    use_effect(move || {
        let state = MUSIC_PLAYER.read();
        let current_time = state.current_time;
        let last_time = last_synced_time();
        if (current_time - last_time).abs() > 0.5 {
            last_synced_time.set(current_time);
            let gen = seek_gen.with_mut(|g| { *g = g.wrapping_add(1); *g });
            is_seeking.set(true);
            spawn(async move {
                {
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
                        current_time = current_time,
                    );
                    let _ = document::eval(&script);
                }
                crate::platform::timer::sleep_ms(500).await;
                // Only clear is_seeking if no newer seek has started
                if *seek_gen.peek() == gen {
                    is_seeking.set(false);
                }
            });
        }
    });
    // Poll currentTime/duration on non-web platforms (Android/desktop) since
    // ontimeupdate/onloadedmetadata use web_sys which is WASM-only.
    // NOTE: Bare `return` (no IIFE) — Dioxus wraps eval scripts in an AsyncFunction,
    // so IIFE return values are lost. Bare return exits the outer AsyncFunction correctly.
    #[cfg(not(feature = "web"))]
    {
        let _time_poller = use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
            loop {
                crate::platform::timer::sleep_ms(250).await;
                if !MUSIC_PLAYER.read().is_playing || *is_seeking.read() {
                    continue;
                }
                let result = document::eval(
                    r#"
                    let a = document.getElementById("global-music-player-audio");
                    if (!a) return [0, 0];
                    return [a.currentTime || 0, a.duration || 0];
                    "#,
                )
                .await;
                match result {
                    Ok(val) => {
                        let (time, dur) = if let Some(arr) = val.as_array() {
                            let t = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let d = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                            (t, d)
                        } else if let Some(s) = val.as_str() {
                            serde_json::from_str::<[f64; 2]>(s)
                                .map(|[t, d]| (t, d))
                                .unwrap_or((0.0, 0.0))
                        } else {
                            log::warn!("Unexpected time poll result: {:?}", val);
                            continue;
                        };
                        if !time.is_nan() && time > 0.0 {
                            last_synced_time.set(time);
                            music_player::set_current_time(time);
                        }
                        if !dur.is_nan() && dur > 0.0 {
                            music_player::set_duration(dur);
                        }
                    }
                    Err(e) => {
                        log::warn!("Time poll eval error: {:?}", e);
                    }
                }
            }
        });
    }
    let playback_speed = use_memo(move || MUSIC_PLAYER.read().playback_speed);
    use_effect(move || {
        let speed = playback_speed();
        {
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
                    speed = speed,
                );
                let _ = document::eval(&script);
            });
        }
    });
    let is_live = use_memo(move || {
        MUSIC_PLAYER
            .read()
            .current_track
            .as_ref()
            .map(|t| t.is_live_stream)
            .unwrap_or(false)
    });
    use_effect(move || {
        if !is_live() && MUSIC_PLAYER.read().now_playing.is_some() {
            music_player::clear_now_playing();
        }
    });
    let _now_playing_poller = use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        loop {
            crate::platform::timer::sleep_ms(2000).await;
            if !is_live() {
                continue;
            }
            {
                // Bare return (no IIFE) — Dioxus wraps eval in AsyncFunction on mobile
                let result = document::eval(
                    r#"
                    if (window.hlsManager && window.hlsManager.nowPlaying) {
                        return JSON.stringify(window.hlsManager.nowPlaying);
                    }
                    return null;
                    "#,
                )
                .await;
                if let Ok(json_val) = result {
                    if let Some(json_str) = json_val.as_str() {
                        if let Ok(now_playing) = serde_json::from_str::<
                            NowPlaying,
                        >(json_str) {
                            if now_playing.has_data() {
                                music_player::set_now_playing(Some(now_playing));
                            }
                        }
                    }
                }
            }
        }
    });
    if !state.is_visible || state.current_track.is_none() {
        return rsx! {
            audio {
                id: "{audio_id}",
                preload: "metadata",
                style: "display: none;",
                ontimeupdate: move |_evt| {
                    if !*is_seeking.read() {
                        #[cfg(feature = "web")]
                        if let Some(target) = _evt.data.as_web_event().target() {
                            if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                                let current_time = audio.current_time();
                                if !current_time.is_nan() {
                                    last_synced_time.set(current_time);
                                    music_player::set_current_time(current_time);
                                }
                            }
                        }
                    }
                },
                onloadedmetadata: move |_evt| {
                    #[cfg(feature = "web")]
                    if let Some(target) = _evt.data.as_web_event().target() {
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
            }
        };
    }
    let track = state.current_track.as_ref().unwrap();
    let (share_url, share_content_type) = match &track.source {
        crate::stores::nostr_music::TrackSource::Wavlake { .. } => {
            (format!("https://nostr.blue/music/track/{}", track.id), ContentType::MusicTrack)
        }
        crate::stores::nostr_music::TrackSource::Nostr { coordinate, .. } => {
            (format!("https://nostr.blue/music/track/{}", coordinate), ContentType::MusicTrack)
        }
        crate::stores::nostr_music::TrackSource::NostrPodcast { coordinate, .. } => {
            (format!("https://nostr.blue/podcast/episode/{}", coordinate), ContentType::PodcastEpisode)
        }
        crate::stores::nostr_music::TrackSource::RssPodcast { feed_url, episode_guid, podcast_id, .. } => {
            if let Some(id) = podcast_id {
                (format!("https://nostr.blue/podcast/rss/{}/episode/{}", id, urlencoding::encode(episode_guid)), ContentType::PodcastEpisode)
            } else {
                (format!("https://nostr.blue/podcast/rss/episode?feed={}&ep={}", urlencoding::encode(feed_url), urlencoding::encode(episode_guid)), ContentType::PodcastEpisode)
            }
        }
        crate::stores::nostr_music::TrackSource::RssMusic { feed_id, episode_id, .. } => {
            (format!("https://nostr.blue/music/track/rss:{}:{}", feed_id, episode_id), ContentType::MusicTrack)
        }
        crate::stores::nostr_music::TrackSource::Radio { d_tag, .. } => {
            (format!("https://nostr.blue/radio/{}", urlencoding::encode(d_tag)), ContentType::MusicTrack)
        }
    };
    let progress = if state.duration > 0.0 {
        (state.current_time / state.duration * 100.0).min(100.0)
    } else {
        0.0
    };
    let onerror_handler = move |_evt| {
        // On mobile, playback is managed entirely via document::eval.
        // The RSX audio element has src="" which fires onerror immediately.
        // Suppress all DOM onerror on mobile — eval handles its own errors.
        #[cfg(not(feature = "web"))]
        {
        }
        #[cfg(feature = "web")]
        {
            log::warn!("Audio playback error, attempting fallback...");
            music_player::set_buffering(false);
            if !music_player::try_next_stream() {
                log::error!("All streams failed");
            }
        }
    };
    rsx! {
        audio {
            id: "{audio_id}",
            preload: if track.is_live_stream { "none" } else { "metadata" },
            style: "display: none;",
            // On mobile, empty src prevents mixed content blocking.
            // The use_effect sets audio.src via document::eval dynamically.
            src: if cfg!(feature = "web") { track.media_url.as_str() } else { "" },
            ontimeupdate: move |_evt| {
                if !*is_seeking.read() {
                    #[cfg(feature = "web")]
                    if let Some(target) = _evt.data.as_web_event().target() {
                        if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                            let current_time = audio.current_time();
                            if !current_time.is_nan() {
                                last_synced_time.set(current_time);
                                music_player::set_current_time(current_time);
                            }
                        }
                    }
                }
            },
            onloadedmetadata: move |_evt| {
                #[cfg(feature = "web")]
                if let Some(target) = _evt.data.as_web_event().target() {
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
            onerror: onerror_handler,
            onwaiting: move |_| {
                music_player::set_buffering(true);
            },
            onplaying: move |_| {
                music_player::set_buffering(false);
                music_player::set_playback_error(None);
            },
        }
        div {
            class: "fixed bottom-0 left-0 right-0 bg-background/95 backdrop-blur border-t border-border shadow-lg z-50",
            style: "backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);",
            div { class: "flex items-center justify-between w-full py-4 px-4 gap-3",
                div { class: "flex items-center gap-3 min-w-0 w-80",
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
                                if let Some(ref np) = state.now_playing {
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
                                    class: "font-semibold text-sm truncate hover:text-primary hover:underline",
                                    "{track.title}"
                                }
                            } else if let Some(track_route) = track.get_track_route() {
                                Link {
                                    to: track_route,
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
                        if let Some(ref error) = state.playback_error {
                            div { class: "text-xs text-red-400 truncate flex items-center gap-1",
                                icons::AlertTriangleIcon { class: "w-3 h-3 shrink-0".to_string() }
                                "{error}"
                            }
                        } else if state.is_buffering {
                            div { class: "text-xs text-muted-foreground truncate flex items-center gap-1",
                                icons::RefreshIcon { class: "w-3 h-3 animate-spin shrink-0".to_string() }
                                "Buffering..."
                            }
                        } else if track.is_live_stream && state.now_playing.is_some() {
                            div { class: "text-xs text-muted-foreground truncate", "{track.title}" }
                        } else {
                            div { class: "text-xs text-muted-foreground truncate",
                                if let Some(show_route) = track.get_show_route() {
                                    Link {
                                        to: show_route,
                                        class: "hover:text-foreground hover:underline",
                                        "{track.artist}"
                                    }
                                } else if let Some(artist_id) = &track.artist_id {
                                    Link {
                                        to: Route::MusicArtist {
                                            artist_id: artist_id.clone(),
                                        },
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
                div { class: "flex items-center gap-3 flex-1 justify-center max-w-2xl",
                    div { class: "flex items-center gap-1",
                        if track.is_live_stream {
                            button {
                                class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-md bg-primary hover:bg-primary/90 text-primary-foreground transition-colors",
                                onclick: move |_| music_player::toggle_play(),
                                dangerous_inner_html: if state.is_playing { icons::PAUSE } else { icons::PLAY },
                            }
                        } else if track.is_podcast {
                            button {
                                class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                                title: "Rewind 15 seconds",
                                onclick: move |_| music_player::skip_backward(15.0),
                                dangerous_inner_html: icons::REWIND_15,
                            }
                            button {
                                class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-md bg-primary hover:bg-primary/90 text-primary-foreground transition-colors",
                                onclick: move |_| music_player::toggle_play(),
                                dangerous_inner_html: if state.is_playing { icons::PAUSE } else { icons::PLAY },
                            }
                            button {
                                class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                                title: "Forward 15 seconds",
                                onclick: move |_| music_player::skip_forward(15.0),
                                dangerous_inner_html: icons::FORWARD_15,
                            }
                        } else {
                            button {
                                class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                                onclick: move |_| music_player::previous_track(),
                                dangerous_inner_html: icons::SKIP_BACK,
                            }
                            button {
                                class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-md bg-primary hover:bg-primary/90 text-primary-foreground transition-colors",
                                onclick: move |_| music_player::toggle_play(),
                                dangerous_inner_html: if state.is_playing { icons::PAUSE } else { icons::PLAY },
                            }
                            button {
                                class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                                onclick: move |_| music_player::next_track(),
                                dangerous_inner_html: icons::SKIP_FORWARD,
                            }
                        }
                    }
                    if !track.is_live_stream {
                        div { class: "flex items-center gap-2 flex-1 max-w-md",
                            span { class: "text-xs text-muted-foreground w-8 text-right",
                                "{format_time(state.current_time)}"
                            }
                            div {
                                class: "flex-1 relative h-2 bg-secondary rounded-full overflow-hidden cursor-pointer",
                                onclick: move |evt| {
                                    let client_x = evt.client_coordinates().x;
                                    let client_y = evt.client_coordinates().y;
                                    let audio_id_str = audio_id.to_string();
                                    {
                                    spawn(async move {
                                        let audio_id_json = serde_json::to_string(&audio_id_str)
                                            .unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());
                                        let script = format!(
                                            r#"
                                            (function() {{
                                                let audio = document.getElementById({audio_id});
                                                if (!audio) return;

                                                let element = document.elementFromPoint({client_x}, {client_y});
                                                if (!element) return;

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
                                            client_y = client_y,
                                        );
                                        let _ = document::eval(&script);
                                    });
                                    }
                                },
                                div {
                                    class: "absolute h-full bg-primary transition-all duration-100",
                                    style: "width: {progress}%",
                                }
                            }
                            span { class: "text-xs text-muted-foreground w-8",
                                "{format_time(state.duration)}"
                            }
                        }
                    }
                    div { class: "flex items-center gap-1 hidden md:flex",
                        button {
                            class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                            onclick: move |_| music_player::toggle_mute(),
                            dangerous_inner_html: if state.is_muted { icons::VOLUME_X } else { icons::VOLUME_2 },
                        }
                        div { class: "relative w-16",
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
                    if track.is_podcast {
                        div {
                            class: "flex items-center gap-1 hidden md:flex",
                            title: "Playback speed",
                            span {
                                class: "w-4 h-4 text-muted-foreground",
                                dangerous_inner_html: icons::GAUGE,
                            }
                            select {
                                class: "bg-transparent text-xs text-muted-foreground cursor-pointer hover:text-foreground border-none focus:outline-hidden appearance-none pr-4",
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
                div { class: "flex items-center gap-1",
                    button {
                        class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                        title: "Share",
                        onclick: move |_| show_share_modal.set(true),
                        dangerous_inner_html: icons::SHARE,
                    }
                    button {
                        class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                        title: "Zap the artist",
                        onclick: move |_| music_player::show_zap_dialog(),
                        dangerous_inner_html: icons::ZAP,
                    }
                    button {
                        class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors",
                        onclick: move |_| music_player::close_player(),
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
