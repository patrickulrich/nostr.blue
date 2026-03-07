use dioxus::prelude::*;
#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;

#[allow(dead_code)]
fn build_native_setup_script(
    video_id: &str,
    stream_url: &str,
    autoplay: bool,
    detach_first: bool,
) -> String {
    let video_id_json = serde_json::to_string(video_id).unwrap_or_default();
    let stream_url_json = serde_json::to_string(stream_url).unwrap_or_default();
    let autoplay_str = if autoplay { "true" } else { "false" };
    let detach_block = if detach_first {
        format!(
            r#"
                        // Detach any existing HLS stream first
                        if (window.hlsManager) {{
                            window.hlsManager.detach({});
                        }}
"#,
            video_id_json
        )
    } else {
        String::new()
    };
    format!(
        r#"
                    (async () => {{
                        try {{
                            let video = document.getElementById({video_id});
                            if (!video) return "error:Video element not found";

                            {detach_block}
                            let url = {stream_url};
                            let isHls = url.toLowerCase().includes('.m3u8');

                            if (isHls && window.hlsManager) {{
                                let result = await window.hlsManager.attachToMedia({video_id}, url);
                                console.log('[Live] HLS stream {log_msg}:', result);
                            }} else {{
                                video.src = url;
                            }}

                            if ({autoplay}) {{
                                await video.play().catch(e => console.log('[Live] Autoplay failed:', e));
                            }}
                            return "ok";
                        }} catch (e) {{
                            console.error('[Live] {error_label} error:', e);
                            return "error:" + e.message;
                        }}
                    }})().catch(e => {{ console.error('[Live] {error_label} IIFE error:', e); return "error:" + e.message; }})
                    "#,
        video_id = video_id_json,
        stream_url = stream_url_json,
        autoplay = autoplay_str,
        detach_block = detach_block,
        log_msg = if detach_first { "re-attached" } else { "attached" },
        error_label = if detach_first { "Retry" } else { "Setup" }
    )
}
/// Cleanup guard that destroys player on drop
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
#[wasm_bindgen(
    inline_js = r#"
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
        link.href = 'https://vjs.zencdn.net/8.10.0/video-js.css';
        link.integrity = 'sha384-6LeG/ONVwTyNrI1eNFYoIcUrglv6y7o8hvl3DB8Qd4K2/wD8niobYgHS3RJSO7uL';
        link.crossOrigin = 'anonymous';
        link.onerror = () => reject(new Error('Failed to load Video.js CSS'));
        document.head.appendChild(link);

        // Load JS with SRI
        const script = document.createElement('script');
        script.src = 'https://vjs.zencdn.net/8.10.0/video.min.js';
        script.integrity = 'sha384-KUwosImoEnKt2Q36Bs3MxeOh0vghQarvecyjG6yVFknPMBxWK1YB0/gqXVWFgzsj';
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
"#
)]
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
    let stream_url_for_validation = stream_url.clone();
    let url_valid = validate_stream_url(&stream_url_for_validation);
    let stream_url_for_id = stream_url.clone();
    let instance_id = use_signal(|| {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(1);
        COUNTER.fetch_add(1, Ordering::SeqCst)
    });
    let video_id = format!("videojs-player-{}-{}", simple_hash(&stream_url_for_id), instance_id());
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);
    let mut mounted = use_signal(|| false);
    #[allow(unused_mut, unused_variables)]
    let mut cleanup_guard = use_signal(|| None::<CleanupGuard>);
    let mut init_gen = use_signal(|| 0u32);
    let video_id_str = video_id.clone();
    let stream_url_for_effect = stream_url.clone();
    let video_id_for_rsx = video_id.clone();
    use_effect(move || {
        let video_id = video_id_str.clone();
        let _stream_url = stream_url_for_effect.clone();
        let gen = init_gen.with_mut(|g| { *g = g.wrapping_add(1); *g });
        if url_valid {
            mounted.set(true);
            #[cfg(feature = "web")]
            {
                let stream_url = _stream_url;
                spawn(async move {
                    crate::platform::timer::sleep_ms(300).await;
                    if !*mounted.peek() {
                        return;
                    }
                    match init_video_js_player(&video_id, &stream_url, autoplay).await {
                        Ok(_) => {
                            if *init_gen.peek() == gen {
                                loading.set(false);
                                error.set(None);
                                cleanup_guard
                                    .set(
                                        Some(CleanupGuard {
                                            video_id: video_id.clone(),
                                        }),
                                    );
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
                    if !*mounted.peek() {
                        return;
                    }

                    // Ensure hlsManager is available
                    let check = document::eval(
                        "return typeof window.hlsManager !== 'undefined'",
                    )
                    .await;
                    let hls_loaded =
                        check.ok().and_then(|v| v.as_bool()).unwrap_or(false);
                    if !hls_loaded {
                        log::info!("[Live] Injecting HLS manager into WebView");
                        let hls_js = include_str!("../../../public/hls-manager.js");
                        if let Err(e) = document::eval(hls_js).await {
                            log::error!("[Live] Failed to inject HLS manager: {:?}", e);
                            error.set(Some(format!(
                                "Failed to load HLS support: {:?}",
                                e
                            )));
                            loading.set(false);
                            return;
                        }
                    }

                    // Set up the video element via eval
                    let setup_script =
                        build_native_setup_script(&video_id, &stream_url, autoplay, false);

                    if *init_gen.peek() != gen {
                        return;
                    }
                    match document::eval(&setup_script).await {
                        Ok(val) => {
                            if *init_gen.peek() == gen {
                                let result = val.as_str().unwrap_or("ok");
                                if let Some(err_msg) = result.strip_prefix("error:") {
                                    error.set(Some(err_msg.to_string()));
                                } else {
                                    error.set(None);
                                }
                                loading.set(false);
                            }
                        }
                        Err(e) => {
                            if *init_gen.peek() == gen {
                                error.set(Some(format!(
                                    "Failed to setup stream: {:?}",
                                    e
                                )));
                                loading.set(false);
                            }
                        }
                    }
                });
            }
        } else {
            error.set(Some("Invalid stream URL".to_string()));
            loading.set(false);
        }
    });
    // Cleanup HLS on unmount for native platforms.
    // Can't use struct Drop (document::eval becomes NoOp there).
    // use_drop runs while the Dioxus runtime is still active.
    #[cfg(feature = "native")]
    {
        let video_id_for_cleanup = video_id.clone();
        use_drop(move || {
            let video_id_json =
                serde_json::to_string(&video_id_for_cleanup).unwrap_or_default();
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
        if *loading.peek() { return; } // Guard: already loading
        error.set(None);
        loading.set(true);
        let _video_id = video_id.clone();
        let _stream_url = stream_url.clone();
        #[cfg(feature = "web")]
        {
            let video_id = _video_id;
            let stream_url = _stream_url;
            spawn(async move {
                crate::platform::timer::sleep_ms(100).await;
                match init_video_js_player(&video_id, &stream_url, autoplay).await {
                    Ok(_) => {
                        loading.set(false);
                        error.set(None);
                        cleanup_guard.set(Some(CleanupGuard { video_id: video_id.clone() }));
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to load stream: {:?}", e);
                        log::error!("{}", error_msg);
                        error.set(Some(error_msg));
                        loading.set(false);
                    }
                }
            });
        }
        #[cfg(feature = "native")]
        {
            let video_id = _video_id;
            let stream_url = _stream_url;
            spawn(async move {
                crate::platform::timer::sleep_ms(100).await;

                // Ensure hlsManager is available
                let check = document::eval(
                    "return typeof window.hlsManager !== 'undefined'",
                )
                .await;
                let hls_loaded =
                    check.ok().and_then(|v| v.as_bool()).unwrap_or(false);
                if !hls_loaded {
                    let hls_js = include_str!("../../../public/hls-manager.js");
                    if let Err(e) = document::eval(hls_js).await {
                        log::error!("[Live] Failed to inject HLS manager: {:?}", e);
                        error.set(Some(format!(
                            "Failed to load HLS support: {:?}",
                            e
                        )));
                        loading.set(false);
                        return;
                    }
                }

                let setup_script =
                    build_native_setup_script(&video_id, &stream_url, autoplay, true);

                match document::eval(&setup_script).await {
                    Ok(val) => {
                        let result = val.as_str().unwrap_or("ok");
                        if let Some(err_msg) = result.strip_prefix("error:") {
                            error.set(Some(err_msg.to_string()));
                        } else {
                            error.set(None);
                        }
                        loading.set(false);
                    }
                    Err(e) => {
                        error.set(Some(format!(
                            "Failed to setup stream: {:?}",
                            e
                        )));
                        loading.set(false);
                    }
                }
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
/// Generates a simple hash for a string (for creating stable IDs)
fn simple_hash(s: &str) -> u32 {
    s.bytes()
        .fold(0u32, |hash, byte| { hash.wrapping_mul(31).wrapping_add(byte as u32) })
}
