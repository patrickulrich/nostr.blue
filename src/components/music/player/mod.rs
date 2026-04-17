pub mod player_bar;
pub mod player_expanded;
pub mod player_floating;

use crate::stores::music_player::{self, MUSIC_PLAYER, PlayerViewMode};
#[cfg(not(feature = "mobile_platform"))]
use crate::utils::radio::NowPlaying;
use dioxus::prelude::*;
#[cfg(feature = "web")]
use dioxus::web::WebEventExt;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
#[cfg(feature = "web")]
use web_sys;

pub use player_bar::PlayerBar;
pub use player_expanded::PlayerExpanded;
pub use player_floating::PlayerFloating;

pub fn format_time(seconds: f64) -> String {
    if seconds.is_nan() {
        return "0:00".to_string();
    }
    let mins = (seconds / 60.0).floor() as u32;
    let secs = (seconds % 60.0).floor() as u32;
    format!("{}:{:02}", mins, secs)
}

#[cfg(not(feature = "mobile_platform"))]
pub(crate) fn parse_audio_bind_result(val: serde_json::Value) -> String {
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

#[cfg(not(feature = "mobile_platform"))]
pub(crate) async fn ensure_audio_hls_manager() -> Result<(), String> {
    let check = document::eval("return typeof window.hlsManager !== 'undefined'")
        .await
        .map_err(|e| format!("Failed to check HLS manager: {:?}", e))?;
    let loaded = check.as_bool().unwrap_or(false);
    if loaded {
        return Ok(());
    }
    log::info!("[Audio] Injecting HLS manager into WebView");
    let hls_js = include_str!("../../../../public/hls-manager.js");
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

#[component]
pub fn PersistentMusicPlayer() -> Element {
    let state = MUSIC_PLAYER.read().clone();
    #[allow(unused_mut)]
    let mut is_seeking = use_signal(|| false);
    #[allow(unused_mut, unused_variables)]
    let mut seek_gen = use_signal(|| 0u32);
    let _show_share_modal = use_signal(|| false);
    #[cfg(not(feature = "mobile_platform"))]
    #[allow(unused_mut)]
    let mut native_source_bound = use_signal(|| false);
    #[cfg(not(feature = "mobile_platform"))]
    #[allow(unused_mut)]
    let mut native_bind_token = use_signal(|| 0u32);
    #[cfg(not(feature = "mobile_platform"))]
    let mut last_synced_time = use_signal(|| 0.0f64);
    let audio_id: &'static str = "global-music-player-audio";

    #[cfg(all(not(feature = "mobile_platform"), not(feature = "web")))]
    {
        use_effect(move || {
            spawn(async move {
                if let Err(e) = ensure_audio_hls_manager().await {
                    log::error!("[Audio] {}", e);
                }
            });
        });
    }

    #[cfg(not(feature = "mobile_platform"))]
    use_effect(use_reactive(
        (&state.current_track, &state.is_playing),
        move |(current_track, is_playing)| {
            native_source_bound.set(false);
            let previous_bind_token = *native_bind_token.peek();
            let bind_token = native_bind_token.with_mut(|token| {
                *token = token.wrapping_add(1);
                *token
            });

            let Some(track) = current_track.as_ref() else {
                let bind_token = previous_bind_token;
                spawn(async move {
                    let audio_id_json = serde_json::to_string(&"global-music-player-audio")
                        .unwrap_or_else(|_| "\"global-music-player-audio\"".to_string());
                    let script = format!(
                        r#"
                        (function() {{
                            let audio = document.getElementById({audio_id});
                            if (!audio) return;
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
                                if (window.hlsManager) {{
                                    window.hlsManager.detach({audio_id});
                                }}
                                const urlChanged = audio.dataset.currentUrl !== {media_url};
                                if (audio._nostrBlueOnCanPlay) {{
                                    audio.removeEventListener('canplay', audio._nostrBlueOnCanPlay);
                                    audio._nostrBlueOnCanPlay = null;
                                }}
                                if (urlChanged) {{
                                    audio.dataset.currentUrl = {media_url};
                                    audio.dataset.bindToken = "{bind_token}";
                                    audio.src = {media_url};
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
                                } else if !music_player::try_next_stream() {
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
                    Err(e) => {
                        if *native_bind_token.read() == bind_token {
                            native_source_bound.set(false);
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

    #[cfg(not(feature = "mobile_platform"))]
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

    #[cfg(all(not(feature = "web"), not(feature = "mobile_platform")))]
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
                if *seek_gen.peek() == gen {
                    is_seeking.set(false);
                }
            });
        }
    });

    #[cfg(all(not(feature = "web"), not(feature = "mobile_platform")))]
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

    #[cfg(not(feature = "mobile_platform"))]
    let playback_speed = use_memo(move || MUSIC_PLAYER.read().playback_speed);
    #[cfg(not(feature = "mobile_platform"))]
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

    #[cfg(feature = "mobile_platform")]
    let _native_snapshot_poller = use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        loop {
            crate::platform::timer::sleep_ms(250).await;
            if *is_seeking.read() {
                continue;
            }
            if let Ok(snapshot) = crate::platform::android_media::snapshot() {
                music_player::sync_native_playback_snapshot(snapshot);
            }
        }
    });

    #[cfg(not(feature = "mobile_platform"))]
    let _now_playing_poller = use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        loop {
            crate::platform::timer::sleep_ms(2000).await;
            if !is_live() {
                continue;
            }
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
    });

    let onerror_handler = {
        #[cfg(feature = "mobile_platform")]
        {
            move |_| {
                if !music_player::try_next_stream_mobile() {
                    music_player::set_playback_error(Some(
                        "Playback error on this platform. Please retry.".to_string(),
                    ));
                }
            }
        }
        #[cfg(not(feature = "mobile_platform"))]
        {
            move |_| {
                if !music_player::try_next_stream() {
                    music_player::set_playback_error(Some(
                        "Playback error on this platform. Please retry.".to_string(),
                    ));
                }
            }
        }
    };

    if !state.is_visible || state.current_track.is_none() {
        #[cfg(feature = "mobile_platform")]
        {
            return rsx! {};
        }
        #[cfg(not(feature = "mobile_platform"))]
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

    rsx! {
        if cfg!(not(feature = "mobile_platform")) {
            audio {
                id: "{audio_id}",
                preload: if track.is_live_stream { "none" } else { "metadata" },
                style: "display: none;",
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
        if matches!(state.view_mode, PlayerViewMode::Bar) {
            PlayerBar { }
        }
        if matches!(state.view_mode, PlayerViewMode::Expanded) {
            PlayerExpanded { }
        }
        if matches!(state.view_mode, PlayerViewMode::Floating) {
            PlayerFloating { }
        }
    }
}
