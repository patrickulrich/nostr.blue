use crate::components::icons;
use crate::components::{ContentShareModal, ContentType};
#[cfg(feature = "mobile")]
use crate::platform::android_media;
use crate::routes::Route;
use crate::stores::music_player::{self, MUSIC_PLAYER};
#[cfg(not(feature = "mobile"))]
use crate::utils::radio::NowPlaying;
use dioxus::prelude::*;
#[cfg(feature = "web")]
use dioxus::web::WebEventExt;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
#[cfg(feature = "web")]
use web_sys;
/// Format seconds as M:SS
fn format_time(seconds: f64) -> String {
    if seconds.is_nan() {
        return "0:00".to_string();
    }
    let mins = (seconds / 60.0).floor() as u32;
    let secs = (seconds % 60.0).floor() as u32;
    format!("{}:{:02}", mins, secs)
}

#[cfg(not(feature = "mobile"))]
fn parse_audio_bind_result(val: serde_json::Value) -> String {
    match val {
        serde_json::Value::String(result) => result,
        serde_json::Value::Null => {
            log::warn!("[Audio] Native audio bind returned null");
            "error:Native audio binding returned no result".to_string()
        }
        other => {
            log::warn!("[Audio] Native audio bind returned unexpected result: {other}");
            "error:Native audio binding returned unexpected result".to_string()
        }
    }
}

#[cfg(not(feature = "mobile"))]
async fn ensure_audio_hls_manager() -> Result<(), String> {
    let check = document::eval("return typeof window.hlsManager !== 'undefined'")
        .await
        .map_err(|e| format!("Failed to check HLS manager: {:?}", e))?;
    let loaded = check.as_bool().unwrap_or(false);
    if loaded {
        return Ok(());
    }
    log::info!("[Audio] Injecting HLS manager into WebView");
    let hls_js = include_str!("../../../public/hls-manager.js");
    document::eval(hls_js)
        .await
        .map_err(|e| format!("Failed to inject HLS manager: {:?}", e))?;
    for _ in 0..10 {
        let check = document::eval("return typeof window.hlsManager !== 'undefined'")
            .await
            .map_err(|e| format!("Failed to confirm HLS manager: {:?}", e))?;
        if check.as_bool().unwrap_or(false) {
            return Ok(());
        }
        crate::platform::timer::sleep_ms(25).await;
    }
    Err("HLS manager did not become available".to_string())
}
/// Persistent music player that stays at bottom of screen
#[component]
pub fn PersistentMusicPlayer() -> Element {
    let state = MUSIC_PLAYER.read().clone();
    #[allow(unused_mut)]
    let mut is_seeking = use_signal(|| false);
    #[allow(unused_mut)]
    let mut seek_gen = use_signal(|| 0u32);
    let mut show_share_modal = use_signal(|| false);
    #[cfg(not(feature = "mobile"))]
    #[allow(unused_mut)]
    let mut native_source_bound = use_signal(|| false);
    #[cfg(not(feature = "mobile"))]
    #[allow(unused_mut)]
    let mut native_bind_token = use_signal(|| 0u32);
    let audio_id = "global-music-player-audio";
    // Inject HLS manager JS on desktop builds.
    // On web, it loads via <script> tag in index.html.
    // On mobile, index.html is not used — Dioxus generates its own HTML.
    #[cfg(all(not(feature = "mobile"), not(feature = "web")))]
    {
        use_effect(move || {
            spawn(async move {
                if let Err(e) = ensure_audio_hls_manager().await {
                    log::error!("[Audio] {}", e);
                }
            });
        });
    }
    #[cfg(not(feature = "mobile"))]
    use_effect(use_reactive(
        (&state.current_track, &state.is_playing),
        move |(current_track, is_playing)| {
            // Rotate token FIRST to invalidate any pending bind tasks
            native_source_bound.set(false);
            let previous_bind_token = *native_bind_token.peek();
            let bind_token = native_bind_token.with_mut(|token| {
                *token = token.wrapping_add(1);
                *token
            });

            // Only proceed if we have a track to bind
            let Some(track) = current_track.as_ref() else {
                // Clean up existing playback before returning
                // Capture bind token to fence the cleanup
                let bind_token = previous_bind_token;
                spawn(async move {
                    let audio_id_json = serde_json::to_string(&"global-music-player-audio")
                        .unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());
                    let script = format!(
                        r#"
                        (function() {{
                            // Fence: check if this cleanup belongs to the current bind
                            let audio = document.getElementById({audio_id});
                            if (!audio) return;
                            
                            // Read the current bind token from the element
                            let currentToken = audio.dataset.bindToken ? parseInt(audio.dataset.bindToken, 10) : 0;
                            if (currentToken !== {bind_token}) return;
                            
                            audio.pause();
                            audio.src = "";
                            audio.currentTime = 0;
                            delete audio.dataset.currentUrl;
                            delete audio.dataset.pendingUrl;
                            delete audio.dataset.bindToken;
                            
                            if (window.hlsManager) {{
                                window.hlsManager.detach({audio_id});
                            }}
                        }})();
                        "#,
                        audio_id = audio_id_json,
                        bind_token = bind_token,
                    );
                    let _ = document::eval(&script).await;
                });
                return;
            };

            let media_url = track.media_url.clone();
            let is_hls = media_url.to_lowercase().contains(".m3u8");
            spawn(async move {
                if is_hls {
                    if let Err(e) = ensure_audio_hls_manager().await {
                        if *native_bind_token.read() == bind_token {
                            native_source_bound.set(false);
                            // Try next stream before showing error
                            if !music_player::try_next_stream() {
                                music_player::set_playback_error(Some(format!(
                                    "Failed to load HLS support: {}",
                                    e
                                )));
                            }
                        }
                        return;
                    }
                }
                let audio_id_json = serde_json::to_string(&audio_id)
                    .unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());
                let media_url_json =
                    serde_json::to_string(&media_url).unwrap_or_else(|_| "\"\"".to_string());
                let is_playing_literal = if is_playing { "true" } else { "false" };
                let script = if is_hls {
                    format!(
                        r#"
                            return (async function() {{
                                try {{
                                    let audio = document.getElementById({audio_id});
                                    if (!audio) return "missing";
                                    audio.dataset.isPlaying = {is_playing};

                                    // Skip if already playing this URL
                                    if (audio.dataset.currentUrl === {media_url}) {{
                                        audio.dataset.bindToken = "{bind_token}";
                                        if ({is_playing} && audio.paused) {{
                                            audio.play().catch(e => console.log('Play failed:', e));
                                        }} else if (!{is_playing} && !audio.paused) {{
                                            audio.pause();
                                        }}
                                        return "bound:" + audio.dataset.currentUrl;
                                    }}

                                    if (!window.hlsManager) {{
                                        return "error:HLS manager unavailable";
                                    }}

                                    const result = await window.hlsManager.attachToMedia({audio_id}, {media_url});
                                    console.log('[Radio] Stream attached:', result);
                                    if (result && result.type === 'error') {{
                                        console.error('[Radio] Stream attach returned error:', result.error || 'unknown');
                                        return "error:" + (result.error || 'Failed to attach stream');
                                    }}
                                    if (result && result.type === 'cancelled') {{
                                        return "cancelled";
                                    }}
                                    audio.dataset.currentUrl = {media_url};
                                    audio.dataset.bindToken = "{bind_token}";
                                    if ({is_playing}) {{
                                        audio.play().catch(e => console.log('Play failed:', e));
                                    }}
                                    return "bound:" + audio.dataset.currentUrl;
                                }} catch (e) {{
                                    console.error('[Radio] Stream attach failed:', e);
                                    return "error:" + (e.message || "Failed to attach stream");
                                }}
                            }})();
                            "#,
                        audio_id = audio_id_json,
                        media_url = media_url_json,
                        is_playing = is_playing_literal,
                        bind_token = bind_token,
                    )
                } else {
                    format!(
                        r#"
                            return (function() {{
                                let audio = document.getElementById({audio_id});
                                if (!audio) return "missing";
                                audio.dataset.isPlaying = {is_playing};

                                // Cleanup any existing HLS instance
                                if (window.hlsManager) {{
                                    window.hlsManager.detach({audio_id});
                                }}

                                // Use dataset to track current URL (more reliable than audio.src comparison)
                                const urlChanged = audio.dataset.currentUrl !== {media_url};

                                if (audio._nostrBlueOnCanPlay) {{
                                    audio.removeEventListener('canplay', audio._nostrBlueOnCanPlay);
                                    audio._nostrBlueOnCanPlay = null;
                                }}

                                if (urlChanged) {{
                                    audio.dataset.currentUrl = {media_url};
                                    audio.dataset.bindToken = "{bind_token}";
                                    audio.src = {media_url};
                                    // For live streams, wait for canplay before playing
                                    if ({is_playing}) {{
                                        const onCanPlay = function() {{
                                            if (audio.dataset.currentUrl !== {media_url}) return;
                                            if (audio.dataset.currentUrl !== audio.dataset.pendingUrl) return;
                                            if (audio.dataset.isPlaying !== 'true') return;
                                            audio.removeEventListener('canplay', onCanPlay);
                                            audio._nostrBlueOnCanPlay = null;
                                            audio.play().catch(e => console.log('Play failed:', e.name, e.message));
                                        }};
                                        audio._nostrBlueOnCanPlay = onCanPlay;
                                        audio.dataset.pendingUrl = {media_url};
                                        audio.addEventListener('canplay', onCanPlay);
                                        audio.load();
                                    }}
                                }} else {{
                                    // URL unchanged - just toggle play/pause
                                    audio.dataset.bindToken = "{bind_token}";
                                    if ({is_playing} && audio.paused) {{
                                        audio.play().catch(e => console.log('Play failed:', e.name, e.message));
                                    }} else if (!{is_playing} && !audio.paused) {{
                                        audio.pause();
                                    }}
                                }}
                                return "bound:" + audio.dataset.currentUrl;
                            }})();
                            "#,
                        audio_id = audio_id_json,
                        media_url = media_url_json,
                        is_playing = is_playing_literal,
                        bind_token = bind_token,
                    )
                };
                match document::eval(&script).await {
                    Ok(val) => {
                        let result = parse_audio_bind_result(val);
                        if *native_bind_token.read() == bind_token {
                            if result == format!("bound:{}", media_url) {
                                native_source_bound.set(true);
                                music_player::set_playback_error(None);
                            } else {
                                native_source_bound.set(false);
                                if result == "cancelled" {
                                    log::warn!("[Audio] Stream attach cancelled for {}", media_url);
                                } else {
                                    // Try next stream before showing error
                                    if !music_player::try_next_stream() {
                                        let error_msg = result
                                            .strip_prefix("error:")
                                            .unwrap_or("Failed to attach stream")
                                            .to_string();
                                        log::error!("[Audio] {}", error_msg);
                                        music_player::set_playback_error(Some(error_msg));
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if *native_bind_token.read() == bind_token {
                            native_source_bound.set(false);
                            // Try next stream before showing error
                            if !music_player::try_next_stream() {
                                let error_msg =
                                    format!("Failed to apply audio source script: {:?}", e);
                                log::warn!("{}", error_msg);
                                music_player::set_playback_error(Some(error_msg));
                            }
                        }
                    }
                }
            });
        },
    ));
    #[cfg(all(not(feature = "web"), not(feature = "mobile")))]
    {
        let audio_id_for_volume = audio_id.to_string();
        let volume_memo = use_memo(move || {
            let state = MUSIC_PLAYER.read();
            if state.is_muted {
                0.0
            } else {
                state.volume
            }
        });
        use_effect(move || {
            let volume_value = *volume_memo.read();
            let audio_id_clone = audio_id_for_volume.clone();
            spawn(async move {
                let audio_id_json = serde_json::to_string(&audio_id_clone)
                    .unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());
                let script = format!(
                    r#"
                    (function() {{
                        let audio = document.getElementById({audio_id});
                        if (audio) audio.volume = {volume};
                    }})();
                    "#,
                    audio_id = audio_id_json,
                    volume = volume_value,
                );
                let _ = document::eval(&script).await;
            });
        });
    }
    #[cfg(not(feature = "mobile"))]
    let mut last_synced_time = use_signal(|| 0.0f64);
    #[cfg(all(not(feature = "web"), not(feature = "mobile")))]
    use_effect(move || {
        let state = MUSIC_PLAYER.read();
        let current_time = state.current_time;
        let last_time = last_synced_time();
        if (current_time - last_time).abs() > 0.5 {
            last_synced_time.set(current_time);
            let gen = seek_gen.with_mut(|g| {
                *g = g.wrapping_add(1);
                *g
            });
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
                    let _ = document::eval(&script).await;
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
    #[cfg(all(not(feature = "web"), not(feature = "mobile")))]
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
                        if !time.is_nan() && time >= 0.0 {
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
    #[cfg(all(not(feature = "web"), not(feature = "mobile")))]
    let playback_speed = use_memo(move || MUSIC_PLAYER.read().playback_speed);
    #[cfg(all(not(feature = "web"), not(feature = "mobile")))]
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
                let _ = document::eval(&script).await;
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
    #[cfg(feature = "mobile")]
    let _native_snapshot_poller = use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        loop {
            crate::platform::timer::sleep_ms(250).await;
            if *is_seeking.read() {
                continue;
            }
            if let Ok(snapshot) = android_media::snapshot() {
                music_player::sync_native_playback_snapshot(snapshot);
            }
        }
    });
    let _now_playing_poller = use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        loop {
            crate::platform::timer::sleep_ms(2000).await;
            #[cfg(not(feature = "mobile"))]
            if !is_live() {
                continue;
            }
            #[cfg(not(feature = "mobile"))]
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
                        if let Ok(now_playing) = serde_json::from_str::<NowPlaying>(json_str) {
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
        #[cfg(feature = "mobile")]
        {
            return rsx! {};
        }
        #[cfg(not(feature = "mobile"))]
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
    let is_hls_track = track.media_url.to_lowercase().contains(".m3u8");
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
        crate::stores::nostr_music::TrackSource::RssPodcast {
            feed_url,
            episode_guid,
            podcast_id,
            ..
        } => {
            if let Some(id) = podcast_id {
                (
                    format!(
                        "https://nostr.blue/podcast/rss/{}/episode/{}",
                        id,
                        urlencoding::encode(episode_guid)
                    ),
                    ContentType::PodcastEpisode,
                )
            } else {
                (
                    format!(
                        "https://nostr.blue/podcast/rss/episode?feed={}&ep={}",
                        urlencoding::encode(feed_url),
                        urlencoding::encode(episode_guid)
                    ),
                    ContentType::PodcastEpisode,
                )
            }
        }
        crate::stores::nostr_music::TrackSource::RssMusic {
            feed_id,
            episode_id,
            ..
        } => (
            format!(
                "https://nostr.blue/music/track/rss:{}:{}",
                feed_id, episode_id
            ),
            ContentType::MusicTrack,
        ),
        crate::stores::nostr_music::TrackSource::Radio { d_tag, .. } => (
            format!("https://nostr.blue/radio/{}", urlencoding::encode(d_tag)),
            ContentType::MusicTrack,
        ),
    };
    let progress = if state.duration > 0.0 {
        (state.current_time / state.duration * 100.0).min(100.0)
    } else {
        0.0
    };
    let onerror_handler = move |_evt| {
        // On mobile, playback is managed entirely via document::eval.
        // The RSX audio element has src="" which fires onerror immediately.
        // Suppress placeholder src errors — eval path handles binding/errors.
        #[cfg(all(not(feature = "web"), not(feature = "mobile")))]
        {
            if !*native_source_bound.read() {
                log::warn!("Audio playback error before native source binding completed");
                music_player::set_buffering(false);
                return;
            }
            if !music_player::try_next_stream() {
                music_player::set_buffering(false);
                music_player::set_playback_error(Some(
                    "Playback error on this platform. Please retry.".to_string(),
                ));
            }
        }
        #[cfg(feature = "web")]
        {
            log::warn!("Audio playback error, attempting fallback...");
            music_player::set_buffering(false);
            if !music_player::try_next_stream() {
                log::error!("All streams failed");
                music_player::set_playback_error(Some(
                    "Playback error on this platform. Please retry.".to_string(),
                ));
            }
        }
    };
    rsx! {
        if cfg!(not(feature = "mobile")) {
        audio {
            id: "{audio_id}",
            preload: if track.is_live_stream { "none" } else { "metadata" },
            style: "display: none;",
            // On mobile, empty src prevents mixed content blocking.
            // The use_effect sets audio.src via document::eval dynamically.
            src: if cfg!(feature = "web") && !is_hls_track {
                track.media_url.as_str()
            } else {
                ""
            },
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
                                    #[allow(unused_variables)]
                                    let client_x = evt.client_coordinates().x;
                                    #[allow(unused_variables)]
                                    let client_y = evt.client_coordinates().y;
                                    #[cfg(all(feature = "web", not(feature = "mobile")))]
                                    {
                                        let duration = state.duration;
                                        let mut seek_gen = seek_gen;
                                        let mut is_seeking = is_seeking;
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
                                    #[cfg(feature = "mobile")]
                                    {
                                        let duration = state.duration;
                                        let mut seek_gen = seek_gen;
                                        let mut is_seeking = is_seeking;
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
                                    #[cfg(all(not(feature = "web"), not(feature = "mobile")))]
                                    {
                                        let audio_id_str = audio_id.to_string();
                                        let mut seek_gen = seek_gen;
                                        let mut is_seeking = is_seeking;
                                        spawn(async move {
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

                                                    let percent = Math.max(0, Math.min(1, ({client_x} - rect.left) / rect.width));
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
