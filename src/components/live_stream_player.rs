use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
use url::Url;
use std::rc::Rc;
use std::cell::Cell;

#[wasm_bindgen(inline_js = r#"
export function initVideoJs(videoId, streamUrl) {
    // Wait for video.js to be loaded
    if (typeof window.videojs === 'undefined') {
        console.error('video.js not loaded');
        return null;
    }

    try {
        const player = window.videojs(videoId, {
            controls: true,
            autoplay: false,
            preload: 'auto',
            fluid: true,
            liveui: true,
            html5: {
                vhs: {
                    overrideNative: true,
                    enableLowInitialPlaylist: true
                },
                nativeAudioTracks: false,
                nativeVideoTracks: false
            }
        });

        // Set the source after player creation
        player.src({
            src: streamUrl,
            type: 'application/x-mpegURL'
        });

        // Handle errors more gracefully
        player.on('error', function() {
            const error = player.error();
            if (error) {
                console.warn('VideoJS playback error:', error.code, error.message, 'URL:', streamUrl);
                // Check if it's a network error vs format error
                if (error.code === 4) {
                    console.warn('Media source not supported or stream not available. This may be due to:');
                    console.warn('- Stream is not currently broadcasting');
                    console.warn('- Invalid HLS URL');
                    console.warn('- CORS issues');
                    console.warn('- Network connectivity problems');
                }
            }
        });

        player.on('loadedmetadata', function() {
            console.log('Stream metadata loaded successfully');
        });

        return videoId;
    } catch (e) {
        console.error('Failed to initialize video.js:', e);
        return null;
    }
}

export function disposeVideoJs(videoId) {
    if (typeof window.videojs === 'undefined') {
        return;
    }

    try {
        const player = window.videojs.getPlayer(videoId);
        if (player) {
            player.dispose();
        }
    } catch (e) {
        console.error('Failed to dispose video.js player:', e);
    }
}
"#)]
extern "C" {
    #[wasm_bindgen(catch)]
    fn initVideoJs(video_id: &str, stream_url: &str) -> Result<JsValue, JsValue>;

    fn disposeVideoJs(video_id: &str);
}

/// Validates and sanitizes a stream URL before passing to JS
fn validate_stream_url(url_str: &str) -> Result<String, String> {
    if url_str.is_empty() {
        return Err("URL is empty".to_string());
    }

    // Parse the URL to ensure it's well-formed
    let parsed_url = Url::parse(url_str)
        .map_err(|e| format!("Invalid URL: {}", e))?;

    // Only allow http and https schemes
    let scheme = parsed_url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!("Unsafe URL scheme '{}'. Only http and https are allowed", scheme));
    }

    // Ensure URL is absolute
    if !parsed_url.has_host() {
        return Err("URL must be absolute with a valid host".to_string());
    }

    // Reject URLs with embedded credentials
    if parsed_url.username() != "" || parsed_url.password().is_some() {
        return Err("URLs with embedded credentials are not allowed".to_string());
    }

    // Return the canonicalized URL
    Ok(parsed_url.to_string())
}

#[component]
pub fn LiveStreamPlayer(stream_url: String) -> Element {
    let video_id = use_signal(|| {
        let timestamp = js_sys::Date::now() as u64;
        let random = (js_sys::Math::random() * 1000000.0) as u64;
        format!("live-stream-player-{}-{}", timestamp, random)
    });
    let mut player_initialized = use_signal(|| false);

    // Mounted flag to track component lifecycle
    let mounted = use_signal(|| Rc::new(Cell::new(true)));

    // Compute the video ID once for the reactive dependency
    let vid = video_id.read().clone();

    // Initialize video.js player
    use_effect(use_reactive((&stream_url, &vid), move |(url, vid)| {
        if url.is_empty() {
            return;
        }

        // Validate the URL before passing to JS
        let validated_url = match validate_stream_url(&url) {
            Ok(safe_url) => safe_url,
            Err(e) => {
                log::error!("Invalid stream URL: {}", e);
                return;
            }
        };

        // Clone mounted flag for async task
        let mounted_flag = mounted.read().clone();

        // Retry logic to wait for video.js to load (due to defer attribute)
        spawn(async move {
            let max_retries = 10;
            let mut retry_count = 0;

            loop {
                // Check if component is still mounted before attempting
                if !mounted_flag.get() {
                    log::debug!("Component unmounted, aborting video.js initialization");
                    break;
                }

                // Wait before attempting
                gloo_timers::future::TimeoutFuture::new(100 * (retry_count + 1)).await;

                match initVideoJs(&vid, &validated_url) {
                    Ok(js_val) => {
                        // Check if the returned value is null or undefined
                        if js_val.is_null() || js_val.is_undefined() {
                            retry_count += 1;
                            if retry_count >= max_retries {
                                log::error!("Failed to initialize video.js player after {} retries: videojs is not present or returned null/undefined", max_retries);
                                break;
                            }
                            log::debug!("video.js not ready yet, retrying ({}/{})", retry_count, max_retries);
                            continue;
                        } else {
                            // Check if component is still mounted after successful init
                            if !mounted_flag.get() {
                                log::debug!("Component unmounted after init, disposing player immediately");
                                disposeVideoJs(&vid);
                                break;
                            }
                            player_initialized.set(true);
                            log::info!("video.js player initialized successfully");
                            break;
                        }
                    }
                    Err(e) => {
                        retry_count += 1;
                        if retry_count >= max_retries {
                            log::error!("Failed to initialize video.js player after {} retries: {:?}", max_retries, e);
                            break;
                        }
                        log::debug!("video.js initialization error, retrying ({}/{}): {:?}", retry_count, max_retries, e);
                        continue;
                    }
                }
            }
        });
    }));

    // Cleanup on unmount
    let video_id_clone = video_id.clone();
    let player_initialized_clone = player_initialized.clone();
    let mounted_clone = mounted.clone();
    use_drop(move || {
        // Mark component as unmounted
        mounted_clone.read().set(false);

        // Dispose player if it was initialized
        if *player_initialized_clone.read() {
            disposeVideoJs(&video_id_clone.read());
        }
    });

    rsx! {
        div {
            class: "w-full bg-black",
            video {
                id: "{video_id.read()}",
                class: "video-js vjs-default-skin vjs-big-play-centered",

                // Fallback text for browsers without JavaScript or video.js
                p {
                    class: "vjs-no-js text-white p-4",
                    "To view this video please enable JavaScript, and consider upgrading to a web browser that "
                    a {
                        href: "https://videojs.com/html5-video-support/",
                        target: "_blank",
                        "supports HTML5 video"
                    }
                }
            }
        }
    }
}
