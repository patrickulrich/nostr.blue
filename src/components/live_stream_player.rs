use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
export function initVideoJs(videoId, streamUrl) {
    // Wait for video.js to be loaded
    if (typeof window.videojs === 'undefined') {
        console.error('video.js not loaded');
        return null;
    }

    // Validate that this looks like an HLS stream
    if (!streamUrl || (!streamUrl.includes('.m3u8') && !streamUrl.includes('application/x-mpegURL'))) {
        console.warn('Stream URL does not appear to be an HLS stream:', streamUrl);
        // Still try to load it, but log a warning
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

#[component]
pub fn LiveStreamPlayer(stream_url: String) -> Element {
    let video_id = use_signal(|| {
        let timestamp = js_sys::Date::now() as u64;
        let random = (js_sys::Math::random() * 1000000.0) as u64;
        format!("live-stream-player-{}-{}", timestamp, random)
    });
    let mut player_initialized = use_signal(|| false);

    // Initialize video.js player
    use_effect(use_reactive((&stream_url, &*video_id.read()), move |(url, vid)| {
        if url.is_empty() {
            return;
        }

        // Small delay to ensure DOM is ready
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(100).await;

            match initVideoJs(&vid, &url) {
                Ok(js_val) => {
                    // Check if the returned value is null or undefined
                    if js_val.is_null() || js_val.is_undefined() {
                        log::error!("Failed to initialize video.js player: videojs is not present or returned null/undefined");
                    } else {
                        player_initialized.set(true);
                        log::info!("video.js player initialized successfully");
                    }
                }
                Err(e) => {
                    log::error!("Failed to initialize video.js player: {:?}", e);
                }
            }
        });
    }));

    // Cleanup on unmount
    let video_id_clone = video_id.clone();
    let player_initialized_clone = player_initialized.clone();
    use_drop(move || {
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
