use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, EventId};
use crate::stores::{nostr_client::HAS_SIGNER, blossom_store};
use crate::components::{VoiceRecorder, RichContent};
use crate::utils::thread_tree::invalidate_thread_tree_cache;

#[component]
pub fn VoiceReplyComposer(
    reply_to: NostrEvent,
    on_close: EventHandler<()>,
    on_success: EventHandler<()>,
) -> Element {
    let mut audio_data = use_signal(|| None::<(Vec<u8>, f64, Vec<u8>, String)>); // (bytes, duration, waveform, mime_type)
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let has_signer = *HAS_SIGNER.read();

    let can_publish = audio_data.read().is_some() && !*is_publishing.read() && has_signer;

    // Get author info
    let author_pubkey = reply_to.pubkey.to_hex();
    let short_author = if author_pubkey.len() > 16 {
        format!("{}...{}", &author_pubkey[..8], &author_pubkey[author_pubkey.len()-4..])
    } else {
        author_pubkey.clone()
    };
    let reply_content = reply_to.content.clone();
    let reply_tags: Vec<_> = reply_to.tags.iter().cloned().collect();
    let reply_event = reply_to.clone();

    // Handle recording complete
    let handle_recording_complete = move |(bytes, duration, waveform, mime_type): (Vec<u8>, f64, Vec<u8>, String)| {
        log::info!("Voice reply recording complete: {} bytes, duration: {}s, MIME: {}",
            bytes.len(), duration, mime_type);
        audio_data.set(Some((bytes, duration, waveform, mime_type)));
    };

    // Handle publish
    let handle_publish = move |_| {
        let Some((bytes, duration, waveform, mime_type)) = audio_data.read().clone() else {
            return;
        };

        if !can_publish {
            return;
        }

        is_publishing.set(true);
        error_message.set(None);
        let event_for_reply = reply_event.clone();

        // Determine the root event ID for cache invalidation
        let parent_root = event_for_reply.tags.iter().find_map(|tag| {
            let tag_vec = tag.clone().to_vec();
            if tag_vec.len() >= 4
                && tag_vec[0] == "e"
                && tag_vec[3] == "root" {
                Some(tag_vec[1].clone())
            } else {
                None
            }
        });

        let thread_root_id = if let Some(root_id) = parent_root {
            // This is a nested reply - use the existing root
            root_id
        } else {
            // This is a direct reply - the parent IS the root
            event_for_reply.id.to_hex()
        };

        spawn(async move {
            // Upload to Blossom with actual MIME type from recorder
            match blossom_store::upload_audio(bytes, mime_type.clone()).await {
                Ok(audio_url) => {
                    log::info!("Voice reply audio uploaded successfully: {}", audio_url);

                    // Publish voice message reply
                    match crate::stores::nostr_client::publish_voice_message_reply(
                        audio_url,
                        duration,
                        waveform,
                        event_for_reply,
                        Some(mime_type),
                    ).await {
                        Ok(event_id) => {
                            log::info!("Voice reply published successfully: {}", event_id);

                            // Invalidate thread tree cache to ensure fresh data on next view
                            if let Ok(root_event_id) = EventId::from_hex(&thread_root_id) {
                                invalidate_thread_tree_cache(&root_event_id);
                                log::debug!("Invalidated thread tree cache for root: {}", thread_root_id);
                            }

                            audio_data.set(None);
                            error_message.set(None);
                            is_publishing.set(false);
                            on_success.call(());
                        }
                        Err(e) => {
                            log::error!("Failed to publish voice reply: {}", e);
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

    let handle_cancel = move |_| {
        audio_data.set(None);
        on_close.call(());
    };

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-start justify-center pt-16 px-4",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-2xl w-full max-h-[80vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between p-4 border-b border-border",
                    h3 {
                        class: "text-lg font-bold",
                        "Voice Reply"
                    }
                    button {
                        class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-full transition",
                        onclick: handle_cancel,
                        "✕"
                    }
                }

                // Original message preview
                div {
                    class: "p-4 bg-gray-50 dark:bg-gray-900 border-b border-border",
                    div {
                        class: "text-sm text-gray-600 dark:text-gray-400 mb-2",
                        "Replying to @{short_author}"
                    }
                    div {
                        class: "text-sm text-gray-700 dark:text-gray-300 line-clamp-3 overflow-hidden",
                        // Check if this is a voice message (content will be a URL)
                        if reply_content.starts_with("http") && (reply_content.contains(".mp4") || reply_content.contains(".webm") || reply_content.contains(".ogg")) {
                            div {
                                class: "flex items-center gap-2",
                                svg {
                                    class: "w-5 h-5 text-primary",
                                    view_box: "0 0 24 24",
                                    fill: "currentColor",
                                    path { d: "M12 14c1.66 0 3-1.34 3-3V5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 3 3 3z" }
                                    path { d: "M17 11c0 2.76-2.24 5-5 5s-5-2.24-5-5H5c0 3.53 2.61 6.43 6 6.92V21h2v-3.08c3.39-.49 6-3.39 6-6.92h-2z" }
                                }
                                span { "Voice message" }
                            }
                        } else {
                            RichContent {
                                content: reply_content.clone(),
                                tags: reply_tags.clone()
                            }
                        }
                    }
                }

                if !has_signer {
                    div {
                        class: "text-center py-8 text-muted-foreground p-4",
                        p { "Sign in to reply with voice" }
                    }
                } else {
                    // Voice reply recorder
                    div {
                        class: "p-4",

                        div {
                            class: "mb-4",
                            label {
                                class: "block text-sm font-medium mb-2",
                                "Record your voice reply (up to 60 seconds)"
                            }

                            VoiceRecorder {
                                on_recording_complete: handle_recording_complete
                            }

                            if audio_data.read().is_some() {
                                div {
                                    class: "mt-2 text-sm text-green-600",
                                    "✓ Recording ready to publish"
                                }
                            }
                        }

                        // Error message
                        if let Some(err) = error_message.read().as_ref() {
                            div {
                                class: "mb-4 p-4 bg-red-100 dark:bg-red-900/20 border border-red-300 dark:border-red-800 rounded-lg text-red-800 dark:text-red-200",
                                "{err}"
                            }
                        }

                        // Actions
                        div {
                            class: "flex items-center justify-end gap-2 mt-4",

                            // Cancel button
                            button {
                                class: "px-4 py-2 text-sm font-medium hover:bg-accent rounded-full transition",
                                onclick: handle_cancel,
                                disabled: *is_publishing.read(),
                                "Cancel"
                            }

                            // Publish button
                            button {
                                class: "px-6 py-2 text-sm font-bold text-white bg-blue-500 hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed rounded-full transition flex items-center gap-2",
                                disabled: !can_publish,
                                onclick: handle_publish,

                                if *is_publishing.read() {
                                    span {
                                        class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"
                                    }
                                    "Publishing..."
                                } else {
                                    "Publish Voice Reply"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
