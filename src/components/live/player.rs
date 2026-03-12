use dioxus::prelude::*;
#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "native")]
fn build_native_setup_script(video_id: &str, stream_url: &str, autoplay: bool) -> String {
    let video_id_json = serde_json::to_string(video_id).unwrap_or_default();
    let stream_url_json = serde_json::to_string(stream_url).unwrap_or_default();
    let autoplay_str = if autoplay { "true" } else { "false" };
    format!(
        r#"
                    return (async () => {{
                        try {{
                            let video = document.getElementById({video_id});
                            if (!video) return "error:Video element not found";

                            let url = {stream_url};
                            let isHls = url.toLowerCase().includes('.m3u8');

                            // Always detach any existing stream first
                            if (window.hlsManager) {{
                                window.hlsManager.detach({video_id});
                            }}

                            if (!isHls) {{
                                video.removeAttribute('src');
                                video.load();
                                video.src = url;
                            }} else if (window.hlsManager) {{
                                let result = await window.hlsManager.attachToMedia({video_id}, url);
                                console.log('[Live] HLS stream attached:', result);
                                if (result && result.type === 'error') {{
                                    return "error:" + (result.error || "Failed to attach HLS stream");
                                }}
                                if (result && result.type === 'cancelled') {{
                                    return "cancelled";
                                }}
                            }} else {{
                                return "error:HLS manager unavailable";
                            }}

                            if ({autoplay}) {{
                                try {{
                                    await video.play();
                                }} catch (e) {{
                                    if (e.name === 'NotAllowedError') {{
                                        return "blocked";
                                    }}
                                    return "error:" + (e.message || "Autoplay failed");
                                }}
                            }}

                            return "success";
                        }} catch (e) {{
                            console.error('[Live] Player setup error:', e);
                            return "error:" + (e.message || "Unknown error");
                        }}
                    }})();
                "#,
        video_id = video_id_json,
        stream_url = stream_url_json,
        autoplay = autoplay_str
    )
}

#[cfg(feature = "native")]
fn parse_native_setup_result(val: serde_json::Value) -> String {
    match val {
        serde_json::Value::String(result) => result,
        serde_json::Value::Null => {
            log::warn!("[Live] Native stream setup returned null");
            "error:Stream setup returned no result".to_string()
        }
        other => {
            log::warn!("[Live] Native stream setup returned unexpected result: {other}");
            "error:Stream setup returned unexpected result".to_string()
        }
    }
}

#[cfg(feature = "native")]
#[derive(Clone, Copy)]
struct NativeSetupState {
    init_gen: Signal<u32>,
    mounted: Signal<bool>,
    error: Signal<Option<String>>,
    loading: Signal<bool>,
    playback_blocked: Signal<bool>,
}

#[cfg(feature = "native")]
async fn run_native_setup(
    video_id: &str,
    stream_url: &str,
    autoplay: bool,
    gen: u32,
    mut state: NativeSetupState,
) {
    if !*state.mounted.peek() || *state.init_gen.peek() != gen {
        return;
    }

    if let Err(e) = ensure_hls_manager().await {
        log::error!("[Live] {}", e);
        if *state.init_gen.peek() == gen && *state.mounted.peek() {
            state
                .error
                .set(Some(format!("Failed to load HLS support: {}", e)));
            state.loading.set(false);
        }
        return;
    }

    if !*state.mounted.peek() || *state.init_gen.peek() != gen {
        return;
    }

    let setup_script = build_native_setup_script(video_id, stream_url, autoplay);
    match document::eval(&setup_script).await {
        Ok(val) => {
            if !*state.mounted.peek() || *state.init_gen.peek() != gen {
                return;
            }
            let result = parse_native_setup_result(val);
            if let Some(err_msg) = result.strip_prefix("error:") {
                state.error.set(Some(err_msg.to_string()));
                state.playback_blocked.set(false);
            } else if result == "cancelled" {
                state
                    .error
                    .set(Some("Stream setup was cancelled".to_string()));
                state.playback_blocked.set(false);
            } else if result == "blocked" {
                state.error.set(None);
                state.playback_blocked.set(true);
            } else {
                state.error.set(None);
                state.playback_blocked.set(false);
            }
            state.loading.set(false);
        }
        Err(e) => {
            if !*state.mounted.peek() || *state.init_gen.peek() != gen {
                return;
            }
            state
                .error
                .set(Some(format!("Failed to setup stream: {:?}", e)));
            state.loading.set(false);
        }
    }
}

#[cfg(feature = "native")]
async fn ensure_hls_manager() -> Result<(), String> {
    let hls_js = include_str!("../../../public/hls-manager.js");
    let inject_script = format!(
        r#"
        if (window.hlsManager) {{
            return true;
        }}
        if (!window.hlsManagerInit) {{
            window.hlsManagerInit = (async () => {{
                try {{
                    {}
                    return true;
                }} finally {{
                    window.hlsManagerInit = null;
                }}
            }})();
        }}
        await window.hlsManagerInit;
        return typeof window.hlsManager !== 'undefined';
        "#,
        hls_js
    );
    log::info!("[Live] Ensuring HLS manager is available in WebView");
    let ready = document::eval(&inject_script)
        .await
        .map_err(|e| format!("Failed to inject HLS manager: {:?}", e))?;
    if ready.as_bool().unwrap_or(false) {
        Ok(())
    } else {
        Err("HLS manager unavailable after initialization".to_string())
    }
}
/// Cleanup guard that destroys player on drop
///
/// Note: `video_id` is marked #[allow(dead_code)] because native cleanup requires
/// async `document::eval` which cannot be performed in a synchronous `Drop` implementation.
/// Native cleanup is handled separately in the `use_drop` logic (see `use_drop` around
/// lines 418-431) while the `Drop` impl only runs web-specific `destroy_video_js_player` calls.
/// This intentional runtime split ensures proper cleanup on both platforms.
#[derive(Clone)]
struct CleanupGuard {
    #[allow(dead_code)]
    video_id: String,
}
impl Drop for CleanupGuard {
    fn drop(&mut self) {
        #[cfg(feature = "web")]
        destroy_video_js_player(&self.video_id);
    }
}
/// Props for the LiveStreamPlayer component
#[derive(Props, Clone, PartialEq)]
pub struct LiveStreamPlayerProps {
    /// The stream URL (HLS, MP4, WebM, etc.)
    pub stream_url: String,
    /// Optional poster image URL
    #[props(default = None)]
    pub poster: Option<String>,
    /// Auto-play the stream (default: true)
    #[props(default = true)]
    pub autoplay: bool,
}
#[cfg(feature = "web")]
#[wasm_bindgen(inline_js = r#"
// Store for Video.js player instances
window.videojsPlayers = window.videojsPlayers || new Map();

// Load Video.js from CDN if not already loaded
export async function loadVideoJs() {
    if (window.videojs) {
        return;
    }

    return new Promise((resolve, reject) => {
        // Load CSS with SRI
        const link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = 'https://vjs.zencdn.net/8.23.4/video-js.css';
        link.integrity = 'sha384-FTL3/NcK7fyX2Wjq1fJtFmQq0ZrBsTOYLSLybPXwARdpRxVO44w30lMGxfg/9lpp';
        link.crossOrigin = 'anonymous';
        link.onerror = () => reject(new Error('Failed to load Video.js CSS'));
        document.head.appendChild(link);

        // Load JS with SRI
        const script = document.createElement('script');
        script.src = 'https://vjs.zencdn.net/8.23.4/video.min.js';
        script.integrity = 'sha384-GK0UA/tBku6A4JblKlmFXkAtBuKfr8BbYGvrdgqmSkNbVQ1M7VfSRCZO75VUS/O6';
        script.crossOrigin = 'anonymous';
        script.onload = () => {
            console.log('Video.js loaded successfully');
            resolve();
        };
        script.onerror = () => reject(new Error('Failed to load Video.js'));
        document.head.appendChild(script);
    });
}

// Detect MIME type from URL
function detectSourceType(url) {
    const urlLower = url.toLowerCase();

    if (urlLower.includes('.m3u8')) {
        return 'application/x-mpegURL';
    } else if (urlLower.includes('.mpd')) {
        return 'application/dash+xml';
    } else if (urlLower.includes('.mp4')) {
        return 'video/mp4';
    } else if (urlLower.includes('.webm')) {
        return 'video/webm';
    } else if (urlLower.includes('.ogg')) {
        return 'video/ogg';
    }

    // Default to mp4
    return 'video/mp4';
}

// Initialize Video.js player
export async function initVideoJsPlayer(videoId, url, autoplay) {
    const videoElement = document.getElementById(videoId);
    if (!videoElement) {
        throw new Error('Video element not found: ' + videoId);
    }

    // Clean up any existing player
    destroyVideoJsPlayer(videoId);

    // Load Video.js library
    await loadVideoJs();

    if (!window.videojs) {
        throw new Error('Video.js failed to load');
    }

    console.log('Initializing Video.js player for:', url);

    // Initialize Video.js with options
    const player = window.videojs(videoId, {
        controls: true,
        autoplay: autoplay,
        preload: 'auto',
        fluid: true,
        responsive: true,
        html5: {
            vhs: {
                // Video.js HTTP Streaming (VHS) options for HLS
                enableLowInitialPlaylist: true,
                smoothQualityChange: true,
                overrideNative: !window.videojs.browser.IS_SAFARI,
            },
            nativeAudioTracks: false,
            nativeVideoTracks: false,
        },
        liveui: true,
    });

    // Set source
    player.src({
        src: url,
        type: detectSourceType(url),
    });

    // Error handling
    player.on('error', function() {
        const error = player.error();
        console.error('Video.js player error:', error);
    });

    // Ready event
    player.on('ready', function() {
        console.log('Video.js player ready');
    });

    // Store player instance
    window.videojsPlayers.set(videoId, player);

    return player;
}

// Destroy Video.js player
export function destroyVideoJsPlayer(videoId) {
    const player = window.videojsPlayers.get(videoId);

    if (player) {
        console.log('Destroying Video.js player:', videoId);
        try {
            player.dispose();
        } catch (e) {
            console.warn('Error disposing player:', e);
        }
        window.videojsPlayers.delete(videoId);
    }
}
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = "initVideoJsPlayer")]
    async fn init_video_js_player(
        video_id: &str,
        url: &str,
        autoplay: bool,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(js_name = "destroyVideoJsPlayer")]
    fn destroy_video_js_player(video_id: &str);
}
/// LiveStreamPlayer component - Universal video player using Video.js
///
/// Supports HLS, DASH, MP4, WebM, and more
#[component]
pub fn LiveStreamPlayer(props: LiveStreamPlayerProps) -> Element {
    let stream_url = props.stream_url.clone();
    let poster = props.poster.clone();
    let autoplay = props.autoplay;
    let url_valid = validate_stream_url(&stream_url);
    let instance_id = use_signal(|| {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(1);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    });
    let video_id = format!("videojs-player-{}", instance_id());
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);
    let mut mounted = use_signal(|| false);
    #[allow(unused_mut)]
    let mut playback_blocked = use_signal(|| false);
    #[allow(unused_mut, unused_variables)]
    let mut cleanup_guard = use_signal(|| None::<CleanupGuard>);
    let mut init_gen = use_signal(|| 0u32);
    let video_id_str = video_id.clone();
    let stream_url_for_effect = stream_url.clone();
    let video_id_for_rsx = video_id.clone();
    use_effect(use_reactive(
        (&stream_url_for_effect, &autoplay),
        move |(stream_url_prop, autoplay_prop)| {
            let video_id = video_id_str.clone();
            let _stream_url = stream_url_prop.clone();
            let url_valid = validate_stream_url(&_stream_url);
            let gen = init_gen.with_mut(|g| {
                *g = g.wrapping_add(1);
                *g
            });
            error.set(None);
            loading.set(url_valid);
            playback_blocked.set(false);
            if url_valid {
                cleanup_guard.set(None);
                mounted.set(true);
                #[cfg(feature = "web")]
                {
                    let stream_url = _stream_url;
                    spawn(async move {
                        crate::platform::timer::sleep_ms(300).await;
                        if !*mounted.peek() {
                            return;
                        }
                        if *init_gen.peek() != gen {
                            return;
                        }
                        match init_video_js_player(&video_id, &stream_url, autoplay_prop).await {
                            Ok(_) => {
                                if *init_gen.peek() == gen {
                                    loading.set(false);
                                    error.set(None);
                                    cleanup_guard.set(Some(CleanupGuard {
                                        video_id: video_id.clone(),
                                    }));
                                }
                            }
                            Err(e) => {
                                if *init_gen.peek() == gen {
                                    let error_msg = format!("Failed to load stream: {:?}", e);
                                    log::error!("{}", error_msg);
                                    error.set(Some(error_msg));
                                    loading.set(false);
                                }
                            }
                        }
                    });
                }
                #[cfg(feature = "native")]
                {
                    let stream_url = _stream_url;
                    spawn(async move {
                        crate::platform::timer::sleep_ms(300).await;
                        run_native_setup(
                            &video_id,
                            &stream_url,
                            autoplay_prop,
                            gen,
                            NativeSetupState {
                                init_gen,
                                mounted,
                                error,
                                loading,
                                playback_blocked,
                            },
                        )
                        .await;
                    });
                }
            } else {
                cleanup_guard.set(None);
                mounted.set(false);
                #[cfg(feature = "native")]
                {
                    let video_id_clone = video_id.clone();
                    spawn(async move {
                        let video_id_json =
                            serde_json::to_string(&video_id_clone).unwrap_or_default();
                        let _ = document::eval(&format!(
                            "if (window.hlsManager) {{ window.hlsManager.detach({}); }}",
                            video_id_json
                        ))
                        .await;
                    });
                }
                error.set(Some("Invalid stream URL".to_string()));
                loading.set(false);
            }
        },
    ));
    // Cleanup HLS on unmount for native platforms.
    // Can't use struct Drop (document::eval becomes NoOp there).
    // use_drop runs while the Dioxus runtime is still active.
    #[cfg(feature = "native")]
    {
        let video_id_for_cleanup = video_id.clone();
        use_drop(move || {
            let video_id_json = serde_json::to_string(&video_id_for_cleanup).unwrap_or_default();
            spawn(async move {
                let _ = document::eval(&format!(
                    "if (window.hlsManager) {{ window.hlsManager.detach({}); }}",
                    video_id_json
                ))
                .await;
            });
        });
    }
    let handle_retry = move |_| {
        if *loading.peek() {
            return;
        } // Guard: already loading
        let gen = init_gen.with_mut(|g| {
            *g = g.wrapping_add(1);
            *g
        });
        error.set(None);
        playback_blocked.set(false);
        loading.set(true);
        cleanup_guard.set(None);
        let _video_id = video_id.clone();
        let _stream_url = stream_url.clone();
        #[cfg(feature = "web")]
        {
            let video_id = _video_id;
            let stream_url = _stream_url;
            spawn(async move {
                crate::platform::timer::sleep_ms(100).await;
                if *init_gen.peek() != gen {
                    return;
                }
                match init_video_js_player(&video_id, &stream_url, autoplay).await {
                    Ok(_) => {
                        if *init_gen.peek() == gen {
                            loading.set(false);
                            error.set(None);
                            cleanup_guard.set(Some(CleanupGuard {
                                video_id: video_id.clone(),
                            }));
                        }
                    }
                    Err(e) => {
                        if *init_gen.peek() == gen {
                            let error_msg = format!("Failed to load stream: {:?}", e);
                            log::error!("{}", error_msg);
                            error.set(Some(error_msg));
                            loading.set(false);
                        }
                    }
                }
            });
        }
        #[cfg(feature = "native")]
        {
            let video_id = _video_id;
            let stream_url = _stream_url;
            let init_gen = init_gen;
            spawn(async move {
                crate::platform::timer::sleep_ms(100).await;
                run_native_setup(
                    &video_id,
                    &stream_url,
                    autoplay,
                    gen,
                    NativeSetupState {
                        init_gen,
                        mounted,
                        error,
                        loading,
                        playback_blocked,
                    },
                )
                .await;
            });
        }
    };
    if !url_valid {
        return rsx! {
            div { class: "relative w-full aspect-video bg-black rounded-lg overflow-hidden flex items-center justify-center",
                div { class: "text-center p-6",
                    p { class: "text-white text-lg", "Invalid stream URL" }
                }
            }
        };
    }
    rsx! {
        div { class: "relative w-full aspect-video bg-black rounded-lg overflow-hidden",
            video {
                id: "{video_id_for_rsx}",
                class: "video-js vjs-big-play-centered vjs-fluid",
                poster: poster.as_deref().unwrap_or(""),
                playsinline: true,
                controls: cfg!(feature = "native"),
                p { class: "vjs-no-js",
                    "To view this video please enable JavaScript, and consider upgrading to a web browser that supports HTML5 video"
                }
            }
            if *loading.read() && error.read().is_none() {
                div { class: "absolute inset-0 flex items-center justify-center bg-black/70 backdrop-blur-sm pointer-events-none z-10",
                    div { class: "flex flex-col items-center gap-4",
                        div { class: "w-12 h-12 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" }
                        p { class: "text-white text-lg", "Loading stream..." }
                    }
                }
            }
            if let Some(error_msg) = error.read().as_ref() {
                div { class: "absolute inset-0 flex items-center justify-center bg-black/80 backdrop-blur-sm z-10",
                    div { class: "flex flex-col items-center gap-4 p-6 max-w-md text-center",
                        svg {
                            class: "w-16 h-16 text-red-500",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z",
                            }
                        }
                        h3 { class: "text-xl font-bold text-white", "Stream Unavailable" }
                        p { class: "text-gray-300 text-sm", "{error_msg}" }
                        button {
                            class: "px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition-colors",
                            onclick: handle_retry,
                            "Retry"
                        }
                    }
                }
            }
            if cfg!(feature = "native") && *playback_blocked.read() && error.read().is_none() {
                div { class: "absolute bottom-4 left-1/2 -translate-x-1/2 z-10 rounded-lg bg-black/75 px-4 py-2 text-center text-sm text-white",
                    "Autoplay was blocked. Use the player controls to start playback."
                }
            }
        }
    }
}
/// Validates a stream URL
fn validate_stream_url(url_str: &str) -> bool {
    if url_str.is_empty() {
        return false;
    }
    match url::Url::parse(url_str) {
        Ok(url) => {
            let scheme = url.scheme();
            if scheme != "http" && scheme != "https" {
                return false;
            }
            if url.username() != "" || url.password().is_some() {
                return false;
            }
            true
        }
        Err(_) => false,
    }
}
