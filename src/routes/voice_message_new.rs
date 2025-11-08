use dioxus::prelude::*;
use crate::stores::{auth_store, blossom_store};
use crate::components::VoiceRecorder;

#[component]
pub fn VoiceMessageNew() -> Element {
    let navigator = navigator();
    let mut audio_data = use_signal(|| None::<(Vec<u8>, f64, Vec<u8>, String)>); // (bytes, duration, waveform, mime_type)
    let mut hashtags = use_signal(|| String::new());
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);

    // Check if user is authenticated
    let is_authenticated = use_memo(move || auth_store::AUTH_STATE.read().is_authenticated);

    // Validation
    let can_publish = audio_data.read().is_some() && !*is_publishing.read();

    // Handle close
    let handle_close = move |_| {
        navigator.go_back();
    };

    // Handle recording complete
    let handle_recording_complete = move |(bytes, duration, waveform, mime_type): (Vec<u8>, f64, Vec<u8>, String)| {
        log::info!("Recording complete: {} bytes, duration: {}s, waveform points: {}, MIME: {}",
            bytes.len(), duration, waveform.len(), mime_type);
        audio_data.set(Some((bytes, duration, waveform, mime_type)));
    };

    // Handle publishing
    let handle_publish = move |_| {
        if !can_publish {
            return;
        }

        let Some((bytes, duration, waveform, mime_type)) = audio_data.read().clone() else {
            return;
        };

        let hashtags_val = hashtags.read().clone();

        is_publishing.set(true);
        error_message.set(None);

        spawn(async move {
            // Parse hashtags
            let tags_vec: Vec<String> = hashtags_val
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            // Upload to Blossom with actual MIME type from recorder
            match blossom_store::upload_audio(bytes, mime_type.clone()).await {
                Ok(audio_url) => {
                    log::info!("Audio uploaded successfully: {}", audio_url);

                    // Publish voice message event
                    match crate::stores::nostr_client::publish_voice_message(
                        audio_url,
                        duration,
                        waveform,
                        tags_vec,
                        Some(mime_type),
                    ).await {
                        Ok(event_id) => {
                            log::info!("Voice message published successfully: {}", event_id);
                            is_publishing.set(false);
                            navigator.push(crate::routes::Route::VoiceMessages {});
                        }
                        Err(e) => {
                            log::error!("Failed to publish voice message: {}", e);
                            error_message.set(Some(format!("Failed to publish: {}", e)));
                            is_publishing.set(false);
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to upload audio: {}", e);
                    error_message.set(Some(format!("Failed to upload audio: {}", e)));
                    is_publishing.set(false);
                }
            }
        });
    };

    // Redirect if not authenticated
    if !*is_authenticated.read() {
        use_effect(move || {
            navigator.push(crate::routes::Route::Home {});
        });
        return rsx! {
            div { class: "flex items-center justify-center h-screen",
                "Redirecting..."
            }
        };
    }

    rsx! {
        div {
            class: "min-h-screen bg-background",

            // Header
            div {
                class: "border-b border-border bg-background sticky top-0 z-10",
                div {
                    class: "max-w-4xl mx-auto px-4 py-4 flex items-center justify-between",

                    div {
                        class: "flex items-center gap-4",
                        button {
                            class: "text-muted-foreground hover:text-foreground transition",
                            onclick: handle_close,
                            crate::components::icons::ArrowLeftIcon { class: "w-6 h-6".to_string() }
                        }
                        h1 {
                            class: "text-2xl font-bold",
                            "Record Voice Message"
                        }
                    }

                    button {
                        class: if can_publish {
                            "px-6 py-2 bg-blue-500 hover:bg-blue-600 text-white font-bold rounded-full transition"
                        } else {
                            "px-6 py-2 bg-gray-300 text-gray-500 font-bold rounded-full cursor-not-allowed"
                        },
                        disabled: !can_publish,
                        onclick: handle_publish,

                        if *is_publishing.read() {
                            "Publishing..."
                        } else {
                            "Post"
                        }
                    }
                }
            }

            // Main content
            div {
                class: "max-w-4xl mx-auto px-4 py-8",

                // Error message
                if let Some(err) = error_message.read().as_ref() {
                    div {
                        class: "mb-4 p-4 bg-red-100 dark:bg-red-900/20 border border-red-300 dark:border-red-800 rounded-lg text-red-800 dark:text-red-200",
                        "{err}"
                    }
                }

                div {
                    class: "space-y-6",

                    // Voice Recorder
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Voice Recording * (up to 60 seconds)"
                        }

                        VoiceRecorder {
                            on_recording_complete: handle_recording_complete
                        }

                        if audio_data.read().is_some() {
                            div {
                                class: "mt-2 text-sm text-green-600",
                                "✓ Recording ready"
                            }
                        }
                    }

                    // Hashtags
                    div {
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Hashtags (optional)"
                        }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                            placeholder: "music, podcast, voicenote (comma separated)",
                            value: "{hashtags}",
                            oninput: move |e| hashtags.set(e.value()),
                        }
                    }

                    // Info box
                    div {
                        class: "p-4 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg",
                        div {
                            class: "text-sm text-blue-800 dark:text-blue-200",
                            p {
                                class: "font-semibold mb-2",
                                "Voice Message Guidelines:"
                            }
                            ul {
                                class: "list-disc list-inside space-y-1",
                                li { "Maximum duration: 60 seconds" }
                                li { "Audio will be uploaded to Blossom server" }
                                li { "Audio format: MP4/WebM/OGG (browser dependent)" }
                                li { "Published as NIP-A0 Kind 1222 event" }
                            }
                        }
                    }
                }
            }
        }
    }
}
