use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use js_sys::{Reflect, Uint8Array};
use web_sys::{Blob, MediaStream, MediaRecorder};
use gloo_timers::future::TimeoutFuture;

const MAX_DURATION_SECONDS: f64 = 60.0;

#[derive(Clone, PartialEq)]
enum RecorderState {
    Idle,
    RequestingPermission,
    Recording { started_at: f64, chunks: Vec<Blob> },
    Stopped { blob_url: String, duration: f64 },
    Error { message: String },
}

#[component]
pub fn VoiceRecorder(
    on_recording_complete: EventHandler<(Vec<u8>, f64, Vec<u8>, String)>, // Added MIME type
) -> Element {
    let mut state = use_signal(|| RecorderState::Idle);
    let mut current_time = use_signal(|| 0.0);
    let mut waveform_data = use_signal(|| Vec::<u8>::new());
    let mime_type = use_signal(|| String::from("audio/webm")); // Store MIME type
    let mut is_playing_preview = use_signal(|| false);
    let is_mounted = use_signal(|| true);
    let mut is_stopping = use_signal(|| false);

    // Generate a unique namespace for this instance to avoid data leakage
    let namespace = use_signal(|| {
        // Generate a cryptographically secure random namespace using browser crypto API
        let random_bytes = match (|| -> Option<Vec<u8>> {
            // Use JavaScript to access crypto.getRandomValues
            let script = r#"
                (function() {
                    try {
                        const arr = new Uint8Array(16);
                        window.crypto.getRandomValues(arr);
                        return Array.from(arr);
                    } catch(e) {
                        console.error('Crypto API failed:', e);
                        return null;
                    }
                })()
            "#;

            let result = js_sys::eval(script).ok()?;
            if result.is_null() || result.is_undefined() {
                return None;
            }

            let array = js_sys::Array::from(&result);
            let bytes: Vec<u8> = array.iter()
                .filter_map(|v| v.as_f64().map(|f| f as u8))
                .collect();

            if bytes.len() == 16 {
                Some(bytes)
            } else {
                None
            }
        })() {
            Some(bytes) => bytes,
            None => {
                log::warn!("Failed to use crypto API, falling back to less secure random");
                // Fallback: use Math.random if crypto API fails
                (0..16)
                    .map(|_| (js_sys::Math::random() * 256.0) as u8)
                    .collect()
            }
        };

        let namespace = format!("__voiceRecorder_{}",
            random_bytes.iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()
        );
        log::debug!("VoiceRecorder instance created with namespace: {}", namespace);
        namespace
    });

    // Set up cleanup on drop
    let namespace_for_cleanup = namespace.read().clone();
    use_hook(|| {
        #[derive(Clone)]
        struct CleanupOnDrop {
            is_mounted: Signal<bool>,
            namespace: String,
        }
        impl Drop for CleanupOnDrop {
            fn drop(&mut self) {
                self.is_mounted.set(false);
                log::debug!("VoiceRecorder unmounted, cleaning up namespace: {}", self.namespace);

                // Clean up MediaStream, MediaRecorder, and blob URLs
                let cleanup_script = format!(
                    r#"
                    (function() {{
                        const ns = window['{}'];
                        if (!ns) return;

                        // Stop and remove MediaStream tracks
                        if (ns.stream && ns.stream.getTracks) {{
                            ns.stream.getTracks().forEach(track => {{
                                track.stop();
                                console.log('Stopped media track');
                            }});
                            ns.stream = null;
                        }}

                        // Stop MediaRecorder if recording
                        if (ns.recorder && ns.recorder.state !== 'inactive') {{
                            try {{
                                ns.recorder.stop();
                                console.log('Stopped MediaRecorder');
                            }} catch (e) {{
                                console.error('Error stopping recorder:', e);
                            }}
                        }}
                        ns.recorder = null;

                        // Revoke all created blob URLs
                        if (ns.blob_urls && Array.isArray(ns.blob_urls)) {{
                            ns.blob_urls.forEach(url => {{
                                try {{
                                    URL.revokeObjectURL(url);
                                    console.log('Revoked blob URL');
                                }} catch (e) {{
                                    console.error('Error revoking URL:', e);
                                }}
                            }});
                            ns.blob_urls = [];
                        }}

                        // Clear the namespace
                        delete window['{}'];
                        console.log('Namespace {} cleaned up');
                    }})();
                    "#,
                    self.namespace, self.namespace, self.namespace
                );

                if let Err(e) = js_sys::eval(&cleanup_script) {
                    log::error!("Failed to execute cleanup script: {:?}", e);
                }
            }
        }
        CleanupOnDrop { is_mounted, namespace: namespace_for_cleanup }
    });

    // Start recording handler
    let namespace_for_recording = namespace.read().clone();
    let start_recording_handler = {
        let namespace_for_recording = namespace_for_recording.clone();
        move |_| {
            state.set(RecorderState::RequestingPermission);
            let ns = namespace_for_recording.clone();

            spawn(async move {
            match request_microphone_permission().await {
                Ok(stream) => {
                    log::info!("Microphone permission granted");

                    // Create and configure MediaRecorder
                    let ns_for_cleanup = ns.clone();
                    match setup_media_recorder(stream, state, current_time, waveform_data, mime_type, is_mounted, is_stopping, ns).await {
                        Ok(recorder) => {
                            log::info!("MediaRecorder setup complete");

                            let started_at = js_sys::Date::now() / 1000.0;
                            state.set(RecorderState::Recording {
                                started_at,
                                chunks: Vec::new()
                            });

                            // Start recording
                            if let Err(e) = recorder.start() {
                                log::error!("Failed to start recording: {:?}", e);

                                // Clean up MediaStream before returning
                                let cleanup_script = format!(
                                    r#"
                                    (function() {{
                                        const ns = window['{}'];
                                        if (ns && ns.stream && ns.stream.getTracks) {{
                                            ns.stream.getTracks().forEach(track => {{
                                                track.stop();
                                                console.log('Stopped media track after start failure');
                                            }});
                                            ns.stream = null;
                                        }}
                                    }})();
                                    "#,
                                    ns_for_cleanup
                                );
                                if let Err(cleanup_err) = js_sys::eval(&cleanup_script) {
                                    log::error!("Failed to cleanup MediaStream: {:?}", cleanup_err);
                                }

                                state.set(RecorderState::Error {
                                    message: format!("Failed to start recording: {:?}", e)
                                });
                                return;
                            }

                            // Monitor duration and auto-stop at 60 seconds
                            spawn(async move {
                                loop {
                                    TimeoutFuture::new(100).await;

                                    // Check if component is still mounted
                                    if !*is_mounted.read() {
                                        log::debug!("Component unmounted, stopping monitoring loop");
                                        break;
                                    }

                                    let current_state = state.read().clone();
                                    if let RecorderState::Recording { started_at, .. } = current_state {
                                        let elapsed = (js_sys::Date::now() / 1000.0) - started_at;
                                        current_time.set(elapsed);

                                        if elapsed >= MAX_DURATION_SECONDS {
                                            log::info!("Max duration reached, stopping recording");
                                            // Handle the stop result properly
                                            match recorder.stop() {
                                                Ok(_) => {
                                                    log::info!("Auto-stop successful, awaiting onstop handler");
                                                    // State will be updated by the onstop handler in setup_media_recorder
                                                }
                                                Err(e) => {
                                                    log::error!("Auto-stop failed: {:?}", e);
                                                    state.set(RecorderState::Error {
                                                        message: format!("Failed to stop recording at max duration: {:?}", e)
                                                    });
                                                    is_stopping.set(false);
                                                }
                                            }
                                            break;
                                        }
                                    } else {
                                        break;
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            log::error!("Failed to setup MediaRecorder: {}", e);
                            state.set(RecorderState::Error {
                                message: format!("Failed to setup recorder: {}", e)
                            });
                        }
                    }
                }
                Err(e) => {
                    log::error!("Microphone permission denied: {:?}", e);
                    state.set(RecorderState::Error {
                        message: "Microphone permission denied. Please allow microphone access.".to_string()
                    });
                }
            }
            });
        }
    };

    // Stop recording
    let namespace_for_stop = namespace.read().clone();
    let mut stop_recording = move |_| {
        // Guard against repeated stop attempts
        if *is_stopping.read() {
            log::debug!("Stop already in progress, ignoring");
            return;
        }

        is_stopping.set(true);
        let duration = *current_time.read();
        let ns = namespace_for_stop.clone();

        spawn(async move {
            // Store duration in the instance's namespace for the onstop handler
            let script = format!("window['{}'].duration = {};", ns, duration);
            match js_sys::eval(&script) {
                Ok(_) => log::debug!("Duration stored: {}s", duration),
                Err(e) => {
                    log::error!("Failed to store duration: {:?}", e);
                    state.set(RecorderState::Error {
                        message: format!("Failed to stop recording: {:?}", e)
                    });
                    is_stopping.set(false);
                    return;
                }
            }

            // Call stop on the recorder via JavaScript with safety check
            let stop_script = format!(
                r#"
                (function() {{
                    const ns = window['{}'];
                    if (ns && ns.recorder) {{
                        try {{
                            ns.recorder.stop();
                            return true;
                        }} catch (e) {{
                            console.error("Stop error:", e);
                            return false;
                        }}
                    }} else {{
                        console.warn("Recorder not found in namespace {}");
                        return false;
                    }}
                }})()
                "#,
                ns, ns
            );

            match js_sys::eval(&stop_script) {
                Ok(result) => {
                    if result.as_bool() == Some(true) {
                        log::info!("Stop recording requested, duration: {}s", duration);
                    } else {
                        log::error!("Failed to stop recorder or recorder not found");
                        state.set(RecorderState::Error {
                            message: "Failed to stop recording: recorder not available".to_string()
                        });
                        is_stopping.set(false);
                    }
                }
                Err(e) => {
                    log::error!("Failed to execute stop script: {:?}", e);
                    state.set(RecorderState::Error {
                        message: format!("Failed to stop recording: {:?}", e)
                    });
                    is_stopping.set(false);
                }
            }
        });
    };

    // Discard recording
    let mut discard_recording = move |_| {
        state.set(RecorderState::Idle);
        current_time.set(0.0);
        waveform_data.set(Vec::new());
        is_stopping.set(false);
    };

    // Re-record handler
    let re_record = {
        let mut start_recording_handler = start_recording_handler.clone();
        move |_| {
            discard_recording(());
            start_recording_handler(());
        }
    };

    // Toggle preview playback
    let toggle_preview = move |_| {
        let is_playing = *is_playing_preview.read();

        if let RecorderState::Stopped { .. } = state.read().clone() {
            spawn(async move {
                let audio_id = "voice-preview-audio";

                // Only flip state after successful play/pause, and add event listeners to keep state in sync
                let script = format!(
                    r#"
                    (function() {{
                        let audio = document.getElementById('{}');
                        if (!audio) {{
                            console.error('Audio element not found');
                            return;
                        }}

                        // Remove old listeners to prevent duplicates
                        if (audio._preview_listeners_attached) {{
                            return; // Listeners already attached, just toggle
                        }}

                        // Add event listeners to sync state with actual playback
                        audio.addEventListener('play', () => {{
                            console.log('Audio play event');
                            window.__voice_preview_playing = true;
                        }});

                        audio.addEventListener('playing', () => {{
                            console.log('Audio playing event');
                            window.__voice_preview_playing = true;
                        }});

                        audio.addEventListener('pause', () => {{
                            console.log('Audio pause event');
                            window.__voice_preview_playing = false;
                        }});

                        audio.addEventListener('ended', () => {{
                            console.log('Audio ended event');
                            window.__voice_preview_playing = false;
                        }});

                        audio.addEventListener('error', (e) => {{
                            console.error('Audio error event:', e);
                            window.__voice_preview_playing = false;
                        }});

                        audio._preview_listeners_attached = true;
                    }})();
                    "#,
                    audio_id
                );
                let _ = js_sys::eval(&script);

                // Now toggle playback
                let toggle_script = format!(
                    r#"
                    (function() {{
                        let audio = document.getElementById('{}');
                        if (!audio) return false;

                        if ({}) {{
                            // Try to play
                            audio.play()
                                .then(() => {{
                                    console.log('Play succeeded');
                                    window.__voice_preview_playing = true;
                                }})
                                .catch(e => {{
                                    console.error('Play failed:', e);
                                    window.__voice_preview_playing = false;
                                }});
                        }} else {{
                            audio.pause();
                            window.__voice_preview_playing = false;
                        }}
                        return true;
                    }})();
                    "#,
                    audio_id,
                    !is_playing
                );

                if let Ok(result) = js_sys::eval(&toggle_script) {
                    if result.as_bool() == Some(true) {
                        // Wait a bit for the play promise to resolve
                        TimeoutFuture::new(100).await;

                        // Check actual playback state
                        let check_script = "window.__voice_preview_playing === true";
                        if let Ok(playing) = js_sys::eval(check_script) {
                            let actual_playing = playing.as_bool().unwrap_or(false);
                            is_playing_preview.set(actual_playing);
                        }
                    }
                }
            });
        }
    };

    // Use recording (convert to bytes and call callback)
    let use_recording = move |_| {
        if let RecorderState::Stopped { blob_url, duration } = state.read().clone() {
            let waveform = waveform_data.read().clone();
            let mime = mime_type.read().clone();

            spawn(async move {
                match blob_url_to_bytes(&blob_url).await {
                    Ok(bytes) => {
                        log::info!("Recording ready: {} bytes, duration: {}s, waveform points: {}, MIME: {}",
                            bytes.len(), duration, waveform.len(), mime);
                        on_recording_complete.call((bytes, duration, waveform, mime));
                    }
                    Err(e) => {
                        log::error!("Failed to convert recording to bytes: {}", e);
                    }
                }
            });
        }
    };

    // Format time display
    let format_time = |seconds: f64| -> String {
        let mins = (seconds / 60.0).floor() as u32;
        let secs = (seconds % 60.0).floor() as u32;
        format!("{:02}:{:02}", mins, secs)
    };

    let remaining_time = MAX_DURATION_SECONDS - *current_time.read();
    let progress_percent = (*current_time.read() / MAX_DURATION_SECONDS * 100.0).min(100.0);

    rsx! {
        div {
            class: "bg-muted/30 rounded-lg p-6 space-y-4",

            // Status indicator
            div {
                class: "text-center",
                match state.read().clone() {
                    RecorderState::Idle => rsx! {
                        div {
                            class: "text-muted-foreground",
                            "Ready to record voice message"
                        }
                    },
                    RecorderState::RequestingPermission => rsx! {
                        div {
                            class: "text-primary",
                            "Requesting microphone permission..."
                        }
                    },
                    RecorderState::Recording { .. } => rsx! {
                        div {
                            class: "space-y-2",
                            div {
                                class: "flex items-center justify-center gap-2 text-red-500 font-bold",
                                div {
                                    class: "w-3 h-3 bg-red-500 rounded-full animate-pulse"
                                }
                                "RECORDING"
                            }
                            div {
                                class: "text-2xl font-mono",
                                "{format_time(*current_time.read())}"
                            }
                            div {
                                class: "text-sm text-muted-foreground",
                                "Time remaining: {format_time(remaining_time)}"
                            }

                            // Progress bar
                            div {
                                class: "w-full h-2 bg-muted rounded-full overflow-hidden",
                                div {
                                    class: "h-full bg-red-500 transition-all",
                                    style: "width: {progress_percent}%"
                                }
                            }
                        }
                    },
                    RecorderState::Stopped { duration, .. } => rsx! {
                        div {
                            class: "text-green-600",
                            "Recording complete: {format_time(duration)}"
                        }
                    },
                    RecorderState::Error { message } => rsx! {
                        div {
                            class: "text-red-500",
                            "{message}"
                        }
                    },
                }
            }

            // Waveform visualization (placeholder for now)
            div {
                class: "h-20 bg-muted rounded flex items-center justify-center",
                match state.read().clone() {
                    RecorderState::Recording { .. } => rsx! {
                        div {
                            class: "flex items-center gap-1",
                            for i in 0..40 {
                                div {
                                    key: "{i}",
                                    class: "w-1 bg-primary rounded-full animate-pulse",
                                    style: "height: {((i % 5) + 1) * 8}px; animation-delay: {i * 50}ms;"
                                }
                            }
                        }
                    },
                    RecorderState::Stopped { blob_url, .. } => rsx! {
                        audio {
                            id: "voice-preview-audio",
                            src: "{blob_url}",
                            preload: "metadata",
                            style: "display: none;",
                        }
                        div {
                            class: "text-muted-foreground",
                            "🎵 Recording ready for preview"
                        }
                    },
                    _ => rsx! {
                        div {
                            class: "text-muted-foreground",
                            "Waveform will appear here"
                        }
                    }
                }
            }

            // Controls
            div {
                class: "flex items-center justify-center gap-4",

                match state.read().clone() {
                    RecorderState::Idle | RecorderState::Error { .. } => rsx! {
                        button {
                            class: "w-16 h-16 rounded-full bg-red-500 hover:bg-red-600 text-white flex items-center justify-center transition shadow-lg",
                            onclick: {
                                let mut handler = start_recording_handler.clone();
                                move |_| handler(())
                            },
                            svg {
                                class: "w-8 h-8",
                                view_box: "0 0 24 24",
                                fill: "currentColor",
                                path { d: "M12 14c1.66 0 3-1.34 3-3V5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 3 3 3z" }
                                path { d: "M17 11c0 2.76-2.24 5-5 5s-5-2.24-5-5H5c0 3.53 2.61 6.43 6 6.92V21h2v-3.08c3.39-.49 6-3.39 6-6.92h-2z" }
                            }
                        }
                    },
                    RecorderState::Recording { .. } => rsx! {
                        button {
                            class: "w-16 h-16 rounded-full bg-gray-700 hover:bg-gray-800 text-white flex items-center justify-center transition shadow-lg",
                            onclick: move |_| stop_recording(()),
                            svg {
                                class: "w-8 h-8",
                                view_box: "0 0 24 24",
                                fill: "currentColor",
                                rect { x: "6", y: "6", width: "12", height: "12", rx: "2" }
                            }
                        }
                    },
                    RecorderState::Stopped { .. } => rsx! {
                        // Preview button
                        button {
                            class: "px-6 py-3 bg-gray-600 hover:bg-gray-700 text-white rounded-full transition",
                            onclick: move |_| toggle_preview(()),
                            if *is_playing_preview.read() {
                                "⏸ Pause"
                            } else {
                                "▶️ Preview"
                            }
                        }

                        // Re-record button
                        button {
                            class: "px-6 py-3 bg-orange-500 hover:bg-orange-600 text-white rounded-full transition",
                            onclick: {
                                let mut handler = re_record.clone();
                                move |_| handler(())
                            },
                            "🔄 Re-record"
                        }

                        // Use recording button
                        button {
                            class: "px-6 py-3 bg-green-600 hover:bg-green-700 text-white rounded-full font-bold transition",
                            onclick: move |_| use_recording(()),
                            "✓ Use Recording"
                        }

                        // Discard button
                        button {
                            class: "px-6 py-3 bg-red-500 hover:bg-red-600 text-white rounded-full transition",
                            onclick: move |_| discard_recording(()),
                            "✕ Discard"
                        }
                    },
                    _ => rsx! {
                        div {
                            class: "text-muted-foreground",
                            "Initializing..."
                        }
                    }
                }
            }
        }
    }
}

// Helper functions for MediaRecorder API

async fn request_microphone_permission() -> Result<MediaStream, JsValue> {
    let window = web_sys::window().ok_or("No window")?;
    let navigator = window.navigator();
    let media_devices = navigator.media_devices()?;

    let constraints = web_sys::MediaStreamConstraints::new();
    constraints.set_audio(&JsValue::from_bool(true));
    constraints.set_video(&JsValue::from_bool(false));

    let promise = media_devices.get_user_media_with_constraints(&constraints)?;
    let result = wasm_bindgen_futures::JsFuture::from(promise).await?;
    let stream: MediaStream = result.dyn_into()?;

    Ok(stream)
}

async fn setup_media_recorder(
    stream: MediaStream,
    mut state: Signal<RecorderState>,
    _current_time: Signal<f64>,
    mut waveform_data: Signal<Vec<u8>>,
    mut mime_type: Signal<String>,
    is_mounted: Signal<bool>,
    mut is_stopping: Signal<bool>,
    namespace: String,
) -> Result<MediaRecorder, String> {
    // Use JavaScript to set up MediaRecorder with all event handlers in the unique namespace
    // This is simpler than trying to handle all the WASM bindings
    let script = format!(
        r#"
        (function() {{
            if (!window['{}']) {{
                window['{}'] = {{}};
            }}
            const ns = window['{}'];

            // Try MIME types in order of preference
            const mimeTypes = [
                'audio/mp4',
                'audio/webm;codecs=opus',
                'audio/webm',
                'audio/ogg;codecs=opus'
            ];

            let selectedMime = 'audio/webm'; // fallback
            for (const mime of mimeTypes) {{
                if (MediaRecorder.isTypeSupported(mime)) {{
                    selectedMime = mime;
                    console.log('Selected MIME type:', selectedMime);
                    break;
                }}
            }}

            ns.chunks = [];
            ns.selectedMime = selectedMime;
            ns.blob_urls = []; // Track blob URLs for cleanup

            return {{mime: selectedMime, ready: true}};
        }})();
        "#,
        namespace, namespace, namespace
    );

    let _ = js_sys::eval(&script)
        .map_err(|e| format!("Failed to setup recorder: {:?}", e))?;

    // Create MediaRecorder with MIME type via JavaScript to avoid web-sys limitations
    let create_recorder_script = format!(
        r#"
        (function() {{
            const ns = window['{}'];
            const mimeType = ns.selectedMime;
            const options = {{ mimeType: mimeType }};
            ns.tempRecorder = new MediaRecorder(arguments[0], options);
            return ns.tempRecorder;
        }})
        "#,
        namespace
    );

    let recorder_js = js_sys::Function::new_with_args(
        "stream",
        &create_recorder_script
    ).call1(&wasm_bindgen::JsValue::NULL, &stream.clone().into())
        .map_err(|e| format!("Failed to create MediaRecorder: {:?}", e))?;

    let recorder: MediaRecorder = recorder_js.dyn_into()
        .map_err(|_| "Failed to convert to MediaRecorder")?;

    // Store the MediaStream in the namespace for cleanup
    let store_stream_script = format!(
        r#"
        (function(stream) {{
            window['{}'].stream = stream;
            console.log('MediaStream stored in namespace for cleanup');
        }})(arguments[0]);
        "#,
        namespace
    );
    let _ = js_sys::Function::new_with_args("stream", &store_stream_script)
        .call1(&JsValue::NULL, &stream.clone().into())
        .map_err(|e| format!("Failed to store stream: {:?}", e))?;

    // Set up event handlers using JavaScript with namespace
    let setup_handlers_script = format!(
        r#"
        (function(recorder) {{
            const ns = window['{}'];
            ns.chunks = [];
            ns.recorder = recorder; // Store reference for stop button

            recorder.ondataavailable = function(e) {{
                if (e.data.size > 0) {{
                    ns.chunks.push(e.data);
                    console.log('Chunk recorded:', e.data.size, 'bytes');
                }}
            }};

            recorder.onstop = async function() {{
                console.log('Recording stopped, chunks:', ns.chunks.length);

                const blob = new Blob(
                    ns.chunks,
                    {{ type: ns.selectedMime }}
                );

                const url = URL.createObjectURL(blob);
                // Track blob URL for cleanup
                ns.blob_urls.push(url);

                const duration = ns.duration || 0;
                const mimeType = ns.selectedMime || 'audio/webm';

                // Extract real waveform from audio data
                const waveformResult = await extractWaveform(blob);

                ns.result = {{
                    url: url,
                    duration: duration,
                    waveform: waveformResult.waveform,
                    waveformError: waveformResult.error,
                    mimeType: mimeType
                }};

                // Trigger Rust callback via global flag
                ns.ready = true;

                console.log('Recording ready:', url, duration + 's', 'MIME:', mimeType, 'waveform points:', waveformResult.waveform.length, 'error:', waveformResult.error);
            }};

            async function extractWaveform(blob) {{
                try {{
                    // Decode the blob to get raw audio samples
                    const arrayBuffer = await blob.arrayBuffer();
                    const audioContext = new (window.OfflineAudioContext || window.webkitOfflineAudioContext)(1, 1, 44100);
                    const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);

                    // Get the raw audio samples from the first channel
                    const samples = audioBuffer.getChannelData(0);
                    const numBuckets = 100;
                    const bucketSize = Math.floor(samples.length / numBuckets);

                    const waveform = [];

                    // Calculate RMS amplitude for each bucket
                    for (let i = 0; i < numBuckets; i++) {{
                        const start = i * bucketSize;
                        const end = Math.min(start + bucketSize, samples.length);

                        // Compute RMS (root mean square) for this bucket
                        let sumSquares = 0;
                        for (let j = start; j < end; j++) {{
                            sumSquares += samples[j] * samples[j];
                        }}
                        const rms = Math.sqrt(sumSquares / (end - start));

                        // Normalize to 0-100 range (audio samples are typically -1 to 1)
                        // Apply slight amplification for better visualization
                        const normalized = Math.min(100, Math.floor(rms * 200));
                        waveform.push(normalized);
                    }}

                    console.log('Extracted waveform from audio data, peak:', Math.max(...waveform));
                    return {{ waveform: waveform, error: false }};
                }} catch (err) {{
                    console.error('Failed to extract waveform from audio:', err);
                    // Return zeros with error flag on failure
                    return {{ waveform: Array(100).fill(0), error: true }};
                }}
            }}

            recorder.onerror = function(e) {{
                console.error('MediaRecorder error:', e);
                // Propagate error to Rust via namespace state
                const errorMsg = e.error?.message || e.message || 'Unknown MediaRecorder error';
                ns.error = errorMsg;
                ns.ready = true;
            }};
        }})(arguments[0]);
        "#,
        namespace
    );

    let _ = js_sys::Function::new_with_args("recorder", &setup_handlers_script)
        .call1(&JsValue::NULL, &recorder)
        .map_err(|e| format!("Failed to setup handlers: {:?}", e))?;

    // Monitor for completion with timeout and unique ID to prevent race conditions
    let namespace_for_monitor = namespace.clone();

    // Generate a unique monitoring loop ID
    let loop_id: u64 = (js_sys::Math::random() * 1_000_000_000.0) as u64;

    // Store the loop ID in the namespace to identify the active monitor
    let set_loop_id_script = format!(
        r#"
        (function() {{
            const ns = window['{}'];
            if (ns) {{
                ns.activeMonitorId = {};
                console.log('Set active monitor ID:', {});
            }}
        }})();
        "#,
        namespace_for_monitor, loop_id, loop_id
    );
    let _ = js_sys::eval(&set_loop_id_script);

    spawn(async move {
        const MAX_POLL_DURATION_MS: u64 = 65_000; // 65 seconds timeout (slightly more than max recording)
        const POLL_INTERVAL_MS: u32 = 200;
        let start_time = js_sys::Date::now();

        loop {
            TimeoutFuture::new(POLL_INTERVAL_MS).await;

            // Check if component is still mounted
            if !*is_mounted.read() {
                log::debug!("Component unmounted, stopping completion monitoring loop {}", loop_id);
                break;
            }

            // Check if this monitor is still active (not replaced by a newer one)
            let is_active_script = format!(
                r#"
                (function() {{
                    const ns = window['{}'];
                    return ns && ns.activeMonitorId === {} ? true : false;
                }})()
                "#,
                namespace_for_monitor, loop_id
            );

            if let Ok(is_active) = js_sys::eval(&is_active_script) {
                if is_active.as_bool() != Some(true) {
                    log::debug!("Monitor loop {} is no longer active, stopping", loop_id);
                    break;
                }
            }

            // Timeout check to prevent infinite polling
            let elapsed = js_sys::Date::now() - start_time;
            if elapsed > MAX_POLL_DURATION_MS as f64 {
                log::error!("Monitor loop {} timed out after {}ms", loop_id, elapsed);
                state.set(RecorderState::Error {
                    message: "Recording monitoring timed out".to_string()
                });
                is_stopping.set(false);
                break;
            }

            // First check for errors
            let error_check_script = format!(
                r#"
                (function() {{
                    const ns = window['{}'];
                    return (ns && ns.ready && ns.error) ? ns.error : null;
                }})()
                "#,
                namespace_for_monitor
            );

            if let Ok(error_val) = js_sys::eval(&error_check_script) {
                if !error_val.is_null() && !error_val.is_undefined() {
                    if let Some(error_msg) = error_val.as_string() {
                        log::error!("MediaRecorder error in loop {}: {}", loop_id, error_msg);
                        state.set(RecorderState::Error {
                            message: format!("Recording error: {}", error_msg)
                        });
                        is_stopping.set(false);
                        // Clear error state
                        let clear_script = format!("window['{}'].ready = false; window['{}'].error = null;", namespace_for_monitor, namespace_for_monitor);
                        let _ = js_sys::eval(&clear_script);
                        break;
                    }
                }
            }

            // Then check for successful result
            let check_script = format!(
                r#"
                (function() {{
                    const ns = window['{}'];
                    return (ns && ns.ready && ns.result) ? ns.result : null;
                }})()
                "#,
                namespace_for_monitor
            );

            if let Ok(result) = js_sys::eval(&check_script) {
                if !result.is_null() && !result.is_undefined() {
                    // Extract result
                    if let Ok(url) = Reflect::get(&result, &JsValue::from_str("url")) {
                        if let Some(url_str) = url.as_string() {
                            if let Ok(duration) = Reflect::get(&result, &JsValue::from_str("duration")) {
                                let dur = duration.as_f64().unwrap_or(0.0);

                                // Extract waveform and check for errors
                                if let Ok(wf) = Reflect::get(&result, &JsValue::from_str("waveform")) {
                                    let arr = js_sys::Array::from(&wf).to_vec().into_iter()
                                        .map(|v| v.as_f64().unwrap_or(0.0) as u8)
                                        .collect::<Vec<_>>();

                                    // Check if waveform extraction had an error
                                    let waveform_error = if let Ok(err_flag) = Reflect::get(&result, &JsValue::from_str("waveformError")) {
                                        err_flag.as_bool().unwrap_or(false)
                                    } else {
                                        false
                                    };

                                    if waveform_error {
                                        log::warn!("Waveform extraction failed in loop {}, using placeholder data", loop_id);
                                        // Could set a UI flag here to show waveform error indicator
                                    }

                                    waveform_data.set(arr);
                                }

                                // Extract MIME type
                                if let Ok(mime) = Reflect::get(&result, &JsValue::from_str("mimeType")) {
                                    if let Some(mime_str) = mime.as_string() {
                                        mime_type.set(mime_str);
                                    }
                                }

                                log::debug!("Monitor loop {} successfully processed recording", loop_id);
                                state.set(RecorderState::Stopped {
                                    blob_url: url_str,
                                    duration: dur,
                                });

                                is_stopping.set(false);

                                // Clear setup
                                let clear_script = format!("window['{}'].ready = false; window['{}'].result = null;", namespace_for_monitor, namespace_for_monitor);
                                let _ = js_sys::eval(&clear_script);
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    Ok(recorder)
}

async fn blob_url_to_bytes(blob_url: &str) -> Result<Vec<u8>, String> {
    let script = format!(
        r#"
        (async function() {{
            const response = await fetch('{}');
            const blob = await response.blob();
            const arrayBuffer = await blob.arrayBuffer();
            return new Uint8Array(arrayBuffer);
        }})()
        "#,
        blob_url
    );

    let promise = js_sys::eval(&script)
        .map_err(|e| format!("Failed to eval script: {:?}", e))?;

    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| format!("Failed to fetch blob: {:?}", e))?;

    let uint8_array: Uint8Array = result.dyn_into()
        .map_err(|e| format!("Failed to convert to Uint8Array: {:?}", e))?;

    Ok(uint8_array.to_vec())
}
