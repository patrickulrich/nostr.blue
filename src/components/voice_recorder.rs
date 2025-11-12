use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
use js_sys::Reflect;
use gloo_timers::future::TimeoutFuture;
use uuid::Uuid;

const MAX_DURATION_SECONDS: f64 = 60.0;

#[derive(Clone, PartialEq)]
enum RecorderState {
    Idle,
    Recording { started_at: f64 },
    Stopped { duration: f64 },
    Error { message: String },
}

#[component]
pub fn VoiceRecorder(
    on_recording_complete: EventHandler<(Vec<u8>, f64, Vec<u8>, String)>,
) -> Element {
    let mut state = use_signal(|| RecorderState::Idle);
    let mut current_time = use_signal(|| 0.0);
    let is_mounted = use_signal(|| true);
    let mut is_playing_preview = use_signal(|| false);
    let mut blob_url_cache = use_signal(|| None::<String>);

    // Generate a unique recorder ID for this instance (only once, persists across renders)
    let recorder_id = use_signal(|| Uuid::new_v4().to_string());

    // Set up cleanup to run only when component unmounts
    // We do NOT use the Drop trait approach as it gets triggered on re-renders with use_hook
    // Instead, we'll handle cleanup manually when needed and rely on browser cleanup

    // Start recording handler
    let start_recording_handler = {
        let recorder_id = recorder_id.read().clone();
        move |_| {
            let recorder_id = recorder_id.clone();
            log::info!("Start recording button clicked, recorder_id: {}", recorder_id);

            state.set(RecorderState::Recording {
                started_at: js_sys::Date::now() / 1000.0
            });
            log::debug!("State set to Recording");

            spawn(async move {
                // Call the JavaScript manager to start recording
                let start_script = format!(
                    r#"
                    (async function() {{
                        if (!window.voiceRecorderManager) {{
                            return {{ success: false, error: 'Voice recorder not initialized' }};
                        }}
                        return await window.voiceRecorderManager.startRecording('{}');
                    }})()
                    "#,
                    recorder_id
                );

                match js_sys::eval(&start_script) {
                    Ok(promise_val) => {
                        let promise = js_sys::Promise::from(promise_val);
                        match wasm_bindgen_futures::JsFuture::from(promise).await {
                            Ok(result) => {
                                // Check if start was successful
                                if let Ok(success) = Reflect::get(&result, &JsValue::from_str("success")) {
                                    if !success.as_bool().unwrap_or(false) {
                                        if let Ok(error) = Reflect::get(&result, &JsValue::from_str("error")) {
                                            let error_msg = error.as_string().unwrap_or_else(|| "Unknown error".to_string());
                                            log::error!("Failed to start recording: {}", error_msg);
                                            state.set(RecorderState::Error {
                                                message: error_msg
                                            });
                                            return;
                                        }
                                    }
                                }

                                log::info!("Recording started successfully");

                                // Monitor duration and auto-stop at 60 seconds
                                // Clone signals and recorder_id for the monitoring loop
                                let monitor_recorder_id = recorder_id.clone();
                                let mut monitor_current_time = current_time.clone();
                                let monitor_state = state.clone();
                                let monitor_is_mounted = is_mounted.clone();

                                spawn(async move {
                                    let started_at = js_sys::Date::now() / 1000.0;
                                    log::info!("Monitoring loop started at: {}", started_at);

                                    loop {
                                        TimeoutFuture::new(100).await;

                                        // Check if component is still mounted
                                        if !*monitor_is_mounted.read() {
                                            log::debug!("Component unmounted, stopping monitoring loop");
                                            break;
                                        }

                                        let current_state = monitor_state.read().clone();
                                        if let RecorderState::Recording { .. } = current_state {
                                            let elapsed = (js_sys::Date::now() / 1000.0) - started_at;
                                            monitor_current_time.set(elapsed);

                                            // Log every second
                                            if (elapsed * 10.0) as u32 % 10 == 0 {
                                                log::debug!("Recording time: {:.1}s", elapsed);
                                            }

                                            if elapsed >= MAX_DURATION_SECONDS {
                                                log::info!("Max duration reached, stopping recording");

                                                // Stop via JavaScript manager
                                                let stop_script = format!(
                                                    "window.voiceRecorderManager.stopRecording('{}')",
                                                    monitor_recorder_id
                                                );
                                                match js_sys::eval(&stop_script) {
                                                    Ok(_) => log::debug!("Auto-stop requested"),
                                                    Err(e) => log::error!("Failed to auto-stop: {:?}", e),
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
                                log::error!("Failed to start recording: {:?}", e);
                                state.set(RecorderState::Error {
                                    message: format!("Failed to start recording: {:?}", e)
                                });
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to eval start script: {:?}", e);
                        state.set(RecorderState::Error {
                            message: "Failed to initialize recording".to_string()
                        });
                    }
                }
            });
        }
    };

    // Stop recording and poll for result
    let stop_recording = {
        let recorder_id = recorder_id.read().clone();
        move |_| {
            let recorder_id = recorder_id.clone();
            let duration = *current_time.read();

            spawn(async move {
                // Stop via JavaScript manager
                let stop_script = format!(
                    "window.voiceRecorderManager.stopRecording('{}')",
                    recorder_id
                );

                match js_sys::eval(&stop_script) {
                    Ok(_) => {
                        log::info!("Stop recording requested");

                        // Poll for result
                        let mut attempts = 0;
                        const MAX_ATTEMPTS: u32 = 50; // 10 seconds max wait

                        loop {
                            TimeoutFuture::new(200).await;
                            attempts += 1;

                            if attempts > MAX_ATTEMPTS {
                                log::error!("Timeout waiting for recording result");
                                state.set(RecorderState::Error {
                                    message: "Timeout waiting for recording".to_string()
                                });
                                break;
                            }

                            let get_result_script = format!(
                                "window.voiceRecorderManager.getResult('{}')",
                                recorder_id
                            );

                            if let Ok(result) = js_sys::eval(&get_result_script) {
                                if !result.is_null() && !result.is_undefined() {
                                    // Check if successful
                                    if let Ok(success) = Reflect::get(&result, &JsValue::from_str("success")) {
                                        if success.as_bool().unwrap_or(false) {
                                            // Extract duration
                                            let dur = if let Ok(dur_val) = Reflect::get(&result, &JsValue::from_str("duration")) {
                                                dur_val.as_f64().unwrap_or(duration)
                                            } else {
                                                duration
                                            };

                                            log::info!("Recording completed: {}s", dur);
                                            state.set(RecorderState::Stopped { duration: dur });

                                            // Create blob URL once and cache it
                                            let create_blob_script = format!(
                                                r#"
                                                (function() {{
                                                    const result = window.voiceRecorderManager.getResult('{}');
                                                    if (result && result.bytes) {{
                                                        const blob = new Blob([result.bytes], {{ type: result.mimeType || 'audio/webm' }});
                                                        const url = URL.createObjectURL(blob);
                                                        console.log('[VoiceRecorder] Created blob URL for preview:', url);
                                                        return url;
                                                    }}
                                                    console.warn('[VoiceRecorder] No result or bytes available for preview');
                                                    return null;
                                                }})()
                                                "#,
                                                recorder_id
                                            );

                                            if let Ok(url_val) = js_sys::eval(&create_blob_script) {
                                                if let Some(url) = url_val.as_string() {
                                                    blob_url_cache.set(Some(url));
                                                }
                                            }

                                            break;
                                        } else {
                                            // Error case
                                            if let Ok(error) = Reflect::get(&result, &JsValue::from_str("error")) {
                                                let error_msg = error.as_string().unwrap_or_else(|| "Unknown error".to_string());
                                                log::error!("Recording error: {}", error_msg);
                                                state.set(RecorderState::Error {
                                                    message: error_msg
                                                });
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to stop recording: {:?}", e);
                        state.set(RecorderState::Error {
                            message: format!("Failed to stop recording: {:?}", e)
                        });
                    }
                }
            });
        }
    };

    // Discard recording
    let mut discard_recording = move |_| {
        let recorder_id = recorder_id.read().clone();

        // Revoke blob URL if it exists
        if let Some(url) = blob_url_cache.read().clone() {
            let revoke_script = format!("URL.revokeObjectURL('{}')", url);
            let _ = js_sys::eval(&revoke_script);
        }
        blob_url_cache.set(None);

        // Cleanup the recording
        let cleanup_script = format!(
            "if (window.voiceRecorderManager) {{ window.voiceRecorderManager.cleanup('{}'); }}",
            recorder_id
        );
        let _ = js_sys::eval(&cleanup_script);

        state.set(RecorderState::Idle);
        current_time.set(0.0);
        is_playing_preview.set(false);
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
            log::info!("Toggle preview clicked, is_playing: {}", is_playing);
            spawn(async move {
                let audio_id = "voice-preview-audio";

                let script = format!(
                    r#"
                    (function() {{
                        let audio = document.getElementById('{}');
                        if (!audio) {{
                            console.error('[VoiceRecorder] Audio element not found - blob URL may not be ready');
                            return false;
                        }}

                        console.log('[VoiceRecorder] Audio element found, src:', audio.src);

                        if ({}) {{
                            audio.play()
                                .then(() => {{
                                    console.log('[VoiceRecorder] Play succeeded');
                                    window.__voice_preview_playing = true;
                                }})
                                .catch(e => {{
                                    console.error('[VoiceRecorder] Play failed:', e);
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

                match js_sys::eval(&script) {
                    Ok(result) => {
                        if result.as_bool() == Some(true) {
                            TimeoutFuture::new(100).await;

                            let check_script = "window.__voice_preview_playing === true";
                            if let Ok(playing) = js_sys::eval(check_script) {
                                let actual_playing = playing.as_bool().unwrap_or(false);
                                is_playing_preview.set(actual_playing);
                                log::debug!("Preview playback state updated: {}", actual_playing);
                            }
                        } else {
                            log::error!("Preview toggle failed - audio element not found or blob URL not ready");
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to execute preview toggle script: {:?}", e);
                    }
                }
            });
        } else {
            log::warn!("Preview clicked but state is not Stopped");
        }
    };

    // Use recording (convert to bytes and call callback)
    let use_recording = move |_| {
        let recorder_id = recorder_id.read().clone();

        if let RecorderState::Stopped { duration } = state.read().clone() {
            log::info!("Use recording clicked, retrieving data for recorder: {}", recorder_id);
            spawn(async move {
                // Get the result from JavaScript manager
                let get_result_script = format!(
                    "window.voiceRecorderManager.getResult('{}')",
                    recorder_id
                );

                match js_sys::eval(&get_result_script) {
                    Ok(result) => {
                        if result.is_null() || result.is_undefined() {
                            log::error!("getResult returned null/undefined for recorder: {}", recorder_id);
                            return;
                        }

                        // Extract all the data
                        match Reflect::get(&result, &JsValue::from_str("bytes")) {
                            Ok(bytes_val) => {
                                match bytes_val.dyn_into::<js_sys::Uint8Array>() {
                                    Ok(bytes_array) => {
                                        let bytes = bytes_array.to_vec();

                                        // Extract waveform
                                        let waveform = if let Ok(wf_val) = Reflect::get(&result, &JsValue::from_str("waveform")) {
                                            let arr = js_sys::Array::from(&wf_val).to_vec().into_iter()
                                                .map(|v| v.as_f64().unwrap_or(0.0) as u8)
                                                .collect::<Vec<_>>();
                                            arr
                                        } else {
                                            log::warn!("Failed to extract waveform, using placeholder");
                                            vec![0u8; 100]
                                        };

                                        // Extract MIME type
                                        let mime_type = if let Ok(mime_val) = Reflect::get(&result, &JsValue::from_str("mimeType")) {
                                            mime_val.as_string().unwrap_or_else(|| "audio/webm".to_string())
                                        } else {
                                            log::warn!("Failed to extract MIME type, using default");
                                            "audio/webm".to_string()
                                        };

                                        log::info!("Recording ready: {} bytes, duration: {}s, waveform points: {}, MIME: {}",
                                            bytes.len(), duration, waveform.len(), mime_type);

                                        // Revoke blob URL before calling callback
                                        if let Some(url) = blob_url_cache.read().clone() {
                                            let revoke_script = format!("URL.revokeObjectURL('{}')", url);
                                            let _ = js_sys::eval(&revoke_script);
                                        }
                                        blob_url_cache.set(None);

                                        on_recording_complete.call((bytes, duration, waveform, mime_type));
                                    }
                                    Err(e) => {
                                        log::error!("Failed to convert bytes to Uint8Array: {:?}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to get bytes from result: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to execute getResult script: {:?}", e);
                    }
                }
            });
        } else {
            log::warn!("Use recording clicked but state is not Stopped");
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

    // Use cached blob URL for preview
    let blob_url = blob_url_cache.read().clone();

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
                    RecorderState::Stopped { duration } => rsx! {
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

            // Waveform visualization
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
                    RecorderState::Stopped { .. } => {
                        let audio_src = blob_url.as_ref().map(|s| s.as_str()).unwrap_or("");
                        rsx! {
                            // Always render audio element, even if blob URL is not ready yet
                            audio {
                                id: "voice-preview-audio",
                                src: "{audio_src}",
                                preload: "metadata",
                                style: "display: none;",
                            }
                            div {
                                class: "text-muted-foreground",
                                if blob_url.is_some() {
                                    "Recording ready for preview"
                                } else {
                                    "Processing recording..."
                                }
                            }
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
                                "Pause"
                            } else {
                                "Preview"
                            }
                        }

                        // Re-record button
                        button {
                            class: "px-6 py-3 bg-orange-500 hover:bg-orange-600 text-white rounded-full transition",
                            onclick: {
                                let mut handler = re_record.clone();
                                move |_| handler(())
                            },
                            "Re-record"
                        }

                        // Use recording button
                        button {
                            class: "px-6 py-3 bg-green-600 hover:bg-green-700 text-white rounded-full font-bold transition",
                            onclick: move |_| use_recording(()),
                            "Use Recording"
                        }

                        // Discard button
                        button {
                            class: "px-6 py-3 bg-red-500 hover:bg-red-600 text-white rounded-full transition",
                            onclick: move |_| discard_recording(()),
                            "Discard"
                        }
                    },
                }
            }
        }
    }
}
