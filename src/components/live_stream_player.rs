use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

/// Cleanup guard that destroys player on drop
#[derive(Clone)]
struct CleanupGuard {
    video_id: String,
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        destroyVideoJsPlayer(&self.video_id);
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

// Inline JavaScript for Video.js integration
#[wasm_bindgen(inline_js = r#"
// Store for Video.js player instances
window.videojsPlayers = window.videojsPlayers || new Map();

// Load Video.js from CDN if not already loaded
export async function loadVideoJs() {
    if (window.videojs) {
        return;
    }

    return new Promise((resolve, reject) => {
        // Load CSS
        const link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = 'https://vjs.zencdn.net/8.10.0/video-js.css';
        document.head.appendChild(link);

        // Load JS
        const script = document.createElement('script');
        script.src = 'https://vjs.zencdn.net/8.10.0/video.min.js';
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
    #[wasm_bindgen(catch)]
    async fn initVideoJsPlayer(video_id: &str, url: &str, autoplay: bool) -> Result<JsValue, JsValue>;

    fn destroyVideoJsPlayer(video_id: &str);
}

/// LiveStreamPlayer component - Universal video player using Video.js
///
/// Supports HLS, DASH, MP4, WebM, and more
#[component]
pub fn LiveStreamPlayer(props: LiveStreamPlayerProps) -> Element {
    let stream_url = props.stream_url.clone();
    let poster = props.poster.clone();
    let autoplay = props.autoplay;

    // Validate stream URL
    let stream_url_for_validation = stream_url.clone();
    let url_valid = use_memo(move || validate_stream_url(&stream_url_for_validation));

    // Generate stable video ID
    let stream_url_for_id = stream_url.clone();
    let video_id = use_memo(move || {
        let hash = simple_hash(&stream_url_for_id);
        format!("videojs-player-{}", hash)
    });

    // Player state
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);
    let mut mounted = use_signal(|| false);

    // Store cleanup guard to keep it alive
    let mut cleanup_guard = use_signal(|| None::<CleanupGuard>);

    // Initialize player
    let video_id_str = video_id.read().clone();
    let stream_url_for_effect = stream_url.clone();
    use_effect(move || {
        let video_id = video_id_str.clone();
        let stream_url = stream_url_for_effect.clone();

        // Check if URL is valid and initialize
        if *url_valid.read() {
            mounted.set(true);

            // Initialize player after DOM is ready
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(300).await;

                if !*mounted.peek() {
                    return;
                }

                match initVideoJsPlayer(&video_id, &stream_url, autoplay).await {
                    Ok(_) => {
                        loading.set(false);
                        error.set(None);

                        // Store cleanup guard after successful init
                        cleanup_guard.set(Some(CleanupGuard {
                            video_id: video_id.clone(),
                        }));
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to load stream: {:?}", e);
                        log::error!("{}", error_msg);
                        error.set(Some(error_msg));
                        loading.set(false);
                    }
                }
            });
        } else {
            error.set(Some("Invalid stream URL".to_string()));
            loading.set(false);
        }
    });

    // Handle retry
    let handle_retry = move |_| {
        error.set(None);
        loading.set(true);

        let video_id = video_id.peek().clone();
        let stream_url = stream_url.clone();

        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(100).await;

            match initVideoJsPlayer(&video_id, &stream_url, autoplay).await {
                Ok(_) => {
                    loading.set(false);
                    error.set(None);
                }
                Err(e) => {
                    let error_msg = format!("Failed to load stream: {:?}", e);
                    log::error!("{}", error_msg);
                    error.set(Some(error_msg));
                    loading.set(false);
                }
            }
        });
    };

    // Check if URL is invalid
    if !*url_valid.read() {
        return rsx! {
            div {
                class: "relative w-full aspect-video bg-black rounded-lg overflow-hidden flex items-center justify-center",
                div {
                    class: "text-center p-6",
                    p { class: "text-white text-lg", "Invalid stream URL" }
                }
            }
        };
    }

    rsx! {
        div {
            class: "relative w-full aspect-video bg-black rounded-lg overflow-hidden",

            // Video.js video element
            video {
                id: "{video_id}",
                class: "video-js vjs-big-play-centered vjs-fluid",
                poster: poster.as_deref().unwrap_or(""),
                playsinline: true,

                // Fallback message
                p { class: "vjs-no-js",
                    "To view this video please enable JavaScript, and consider upgrading to a web browser that supports HTML5 video"
                }
            }

            // Loading overlay
            if *loading.read() && error.read().is_none() {
                div {
                    class: "absolute inset-0 flex items-center justify-center bg-black/70 backdrop-blur-sm pointer-events-none z-10",
                    div {
                        class: "flex flex-col items-center gap-4",
                        div {
                            class: "w-12 h-12 border-4 border-blue-500 border-t-transparent rounded-full animate-spin"
                        }
                        p {
                            class: "text-white text-lg",
                            "Loading stream..."
                        }
                    }
                }
            }

            // Error overlay
            if let Some(error_msg) = error.read().as_ref() {
                div {
                    class: "absolute inset-0 flex items-center justify-center bg-black/80 backdrop-blur-sm z-10",
                    div {
                        class: "flex flex-col items-center gap-4 p-6 max-w-md text-center",
                        // Error icon
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
                                d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                            }
                        }
                        h3 {
                            class: "text-xl font-bold text-white",
                            "Stream Unavailable"
                        }
                        p {
                            class: "text-gray-300 text-sm",
                            "{error_msg}"
                        }
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

            // Reject URLs with embedded credentials
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
    s.bytes().fold(0u32, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(byte as u32)
    })
}
