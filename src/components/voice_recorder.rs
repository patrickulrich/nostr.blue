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
    let mut mime_type = use_signal(|| String::from("audio/webm")); // Store MIME type
    let mut is_playing_preview = use_signal(|| false);

    // Start recording
    let mut start_recording = move |_| {
        state.set(RecorderState::RequestingPermission);

        spawn(async move {
            match request_microphone_permission().await {
                Ok(stream) => {
                    log::info!("Microphone permission granted");

                    // Create and configure MediaRecorder
                    match setup_media_recorder(stream, state, current_time, waveform_data, mime_type).await {
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
                                state.set(RecorderState::Error {
                                    message: format!("Failed to start recording: {:?}", e)
                                });
                                return;
                            }

                            // Monitor duration and auto-stop at 60 seconds
                            spawn(async move {
                                loop {
                                    TimeoutFuture::new(100).await;

                                    let current_state = state.read().clone();
                                    if let RecorderState::Recording { started_at, .. } = current_state {
                                        let elapsed = (js_sys::Date::now() / 1000.0) - started_at;
                                        current_time.set(elapsed);

                                        if elapsed >= MAX_DURATION_SECONDS {
                                            log::info!("Max duration reached, stopping recording");
                                            let _ = recorder.stop();
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
    };

    // Stop recording
    let stop_recording = move |_| {
        // Store the current duration before stopping
        let duration = *current_time.read();

        spawn(async move {
            // Store duration in JavaScript global for the onstop handler
            let script = format!("window.__voiceRecorderSetup.duration = {};", duration);
            let _ = js_sys::eval(&script);

            // Call stop on the recorder via JavaScript
            let stop_script = r#"
                if (window.__voiceRecorderSetup && window.__voiceRecorderSetup.recorder) {
                    window.__voiceRecorderSetup.recorder.stop();
                }
            "#;
            let _ = js_sys::eval(stop_script);

            log::info!("Stop recording requested, duration: {}s", duration);
        });
    };

    // Discard recording
    let mut discard_recording = move |_| {
        state.set(RecorderState::Idle);
        current_time.set(0.0);
        waveform_data.set(Vec::new());
    };

    // Re-record
    let mut re_record = move |_| {
        discard_recording(());
        start_recording(());
    };

    // Toggle preview playback
    let mut toggle_preview = move |_| {
        let is_playing = *is_playing_preview.read();
        is_playing_preview.set(!is_playing);

        if let RecorderState::Stopped { .. } = state.read().clone() {
            spawn(async move {
                let audio_id = "voice-preview-audio";
                let script = format!(
                    r#"
                    (function() {{
                        let audio = document.getElementById('{}');
                        if (!audio) return;

                        if ({}) {{
                            audio.play().catch(e => console.log('Play failed:', e));
                        }} else {{
                            audio.pause();
                        }}
                    }})();
                    "#,
                    audio_id,
                    !is_playing
                );
                let _ = js_sys::eval(&script);
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
                            onclick: move |_| start_recording(()),
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
                            onclick: move |_| re_record(()),
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
) -> Result<MediaRecorder, String> {
    // Use JavaScript to set up MediaRecorder with all event handlers
    // This is simpler than trying to handle all the WASM bindings
    let script = r#"
        (function() {
            if (!window.__voiceRecorderSetup) {
                window.__voiceRecorderSetup = {};
            }

            // Try MIME types in order of preference
            const mimeTypes = [
                'audio/mp4',
                'audio/webm;codecs=opus',
                'audio/webm',
                'audio/ogg;codecs=opus'
            ];

            let selectedMime = 'audio/webm'; // fallback
            for (const mime of mimeTypes) {
                if (MediaRecorder.isTypeSupported(mime)) {
                    selectedMime = mime;
                    console.log('Selected MIME type:', selectedMime);
                    break;
                }
            }

            window.__voiceRecorderSetup.chunks = [];
            window.__voiceRecorderSetup.selectedMime = selectedMime;

            return {mime: selectedMime, ready: true};
        })();
    "#;

    let _ = js_sys::eval(script)
        .map_err(|e| format!("Failed to setup recorder: {:?}", e))?;

    let recorder = MediaRecorder::new_with_media_stream(&stream)
        .map_err(|e| format!("Failed to create MediaRecorder: {:?}", e))?;

    // Set up event handlers using JavaScript
    let setup_handlers_script = r#"
        (function(recorder) {
            window.__voiceRecorderSetup.chunks = [];
            window.__voiceRecorderSetup.recorder = recorder; // Store reference for stop button

            recorder.ondataavailable = function(e) {
                if (e.data.size > 0) {
                    window.__voiceRecorderSetup.chunks.push(e.data);
                    console.log('Chunk recorded:', e.data.size, 'bytes');
                }
            };

            recorder.onstop = async function() {
                console.log('Recording stopped, chunks:', window.__voiceRecorderSetup.chunks.length);

                const blob = new Blob(
                    window.__voiceRecorderSetup.chunks,
                    { type: window.__voiceRecorderSetup.selectedMime }
                );

                const url = URL.createObjectURL(blob);
                const duration = window.__voiceRecorderSetup.duration || 0;
                const mimeType = window.__voiceRecorderSetup.selectedMime || 'audio/webm';

                // Generate simple waveform (100 random values for now)
                const waveform = Array.from({length: 100}, () => Math.floor(Math.random() * 100));

                window.__voiceRecorderSetup.result = {
                    url: url,
                    duration: duration,
                    waveform: waveform,
                    mimeType: mimeType
                };

                // Trigger Rust callback via global flag
                window.__voiceRecorderSetup.ready = true;

                console.log('Recording ready:', url, duration + 's', 'MIME:', mimeType);
            };

            recorder.onerror = function(e) {
                console.error('MediaRecorder error:', e);
            };
        })(arguments[0]);
    "#;

    let _ = js_sys::Function::new_with_args("recorder", setup_handlers_script)
        .call1(&JsValue::NULL, &recorder)
        .map_err(|e| format!("Failed to setup handlers: {:?}", e))?;

    // Monitor for completion
    spawn(async move {
        loop {
            TimeoutFuture::new(200).await;

            let check_script = r#"
                window.__voiceRecorderSetup.ready && window.__voiceRecorderSetup.result
                    ? window.__voiceRecorderSetup.result
                    : null
            "#;

            if let Ok(result) = js_sys::eval(check_script) {
                if !result.is_null() && !result.is_undefined() {
                    // Extract result
                    if let Ok(url) = Reflect::get(&result, &JsValue::from_str("url")) {
                        if let Some(url_str) = url.as_string() {
                            if let Ok(duration) = Reflect::get(&result, &JsValue::from_str("duration")) {
                                let dur = duration.as_f64().unwrap_or(0.0);

                                // Extract waveform
                                if let Ok(wf) = Reflect::get(&result, &JsValue::from_str("waveform")) {
                                    let arr = js_sys::Array::from(&wf).to_vec().into_iter()
                                        .map(|v| v.as_f64().unwrap_or(0.0) as u8)
                                        .collect::<Vec<_>>();
                                    waveform_data.set(arr);
                                }

                                // Extract MIME type
                                if let Ok(mime) = Reflect::get(&result, &JsValue::from_str("mimeType")) {
                                    if let Some(mime_str) = mime.as_string() {
                                        mime_type.set(mime_str);
                                    }
                                }

                                state.set(RecorderState::Stopped {
                                    blob_url: url_str,
                                    duration: dur,
                                });

                                // Clear setup
                                let _ = js_sys::eval("window.__voiceRecorderSetup.ready = false; window.__voiceRecorderSetup.result = null;");
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
