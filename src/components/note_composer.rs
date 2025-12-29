use dioxus::prelude::*;
use crate::stores::{nostr_client::publish_note_tracked, auth_store};
use crate::components::{MediaUploader, EmojiPicker, GifPicker, MentionAutocomplete, PollCreatorModal};
use crate::components::icons::{CameraIcon, BarChartIcon};

const MAX_LENGTH: usize = 5000;

#[component]
pub fn NoteComposer() -> Element {
    let mut content = use_signal(String::new);
    let mut is_publishing = use_signal(|| false);
    let mut is_focused = use_signal(|| false);
    let mut show_image_uploader = use_signal(|| false);
    let mut show_poll_modal = use_signal(|| false);
    let mut publish_feedback = use_signal(|| Option::<(bool, String)>::None);
    // Version counter to prevent stale timeout from clearing newer feedback
    let mut feedback_version = use_signal(|| 0u32);

    // Check if user is authenticated (can publish) using auth_store
    let is_authenticated = use_memo(move || auth_store::AUTH_STATE.read().is_authenticated);

    let char_count = content.read().chars().count();
    let remaining = MAX_LENGTH.saturating_sub(char_count);
    let is_over_limit = char_count > MAX_LENGTH;
    let show_warning = remaining < 100 && !is_over_limit;
    let can_publish = char_count > 0 && !is_over_limit && !*is_publishing.read();

    // Determine counter color
    let counter_color = if is_over_limit {
        "text-red-500"
    } else if show_warning {
        "text-yellow-500"
    } else {
        "text-gray-500"
    };

    let mut cursor_position = use_signal(|| 0usize);

    let handle_publish = move |_| {
        let content_value = content.read().clone();

        if content_value.is_empty() || is_over_limit {
            return;
        }

        is_publishing.set(true);
        publish_feedback.set(None);

        spawn(async move {
            match publish_note_tracked(content_value, Vec::new()).await {
                Ok(result) => {
                    let success_count = result.success_count();
                    let total = result.total_attempted();

                    log::info!("Note published: {} ({}/{} relays)", result.event_id, success_count, total);

                    // Show feedback based on relay results
                    if result.has_failures() && success_count > 0 {
                        // Partial success
                        feedback_version.set(feedback_version() + 1);
                        let current_version = feedback_version();
                        publish_feedback.set(Some((true, format!("Published to {}/{} relays", success_count, total))));

                        content.set(String::new());
                        show_image_uploader.set(false);
                        is_publishing.set(false);

                        // Auto-hide feedback after 3 seconds (only if version unchanged)
                        gloo_timers::future::TimeoutFuture::new(3000).await;
                        if feedback_version() == current_version {
                            publish_feedback.set(None);
                        }
                    } else if success_count == 0 {
                        // All failed
                        feedback_version.set(feedback_version() + 1);
                        let current_version = feedback_version();
                        publish_feedback.set(Some((false, "Failed to publish to any relay".to_string())));

                        content.set(String::new());
                        show_image_uploader.set(false);
                        is_publishing.set(false);

                        // Auto-hide feedback after 3 seconds (only if version unchanged)
                        gloo_timers::future::TimeoutFuture::new(3000).await;
                        if feedback_version() == current_version {
                            publish_feedback.set(None);
                        }
                    } else {
                        // Full success: no feedback needed, just clear
                        content.set(String::new());
                        show_image_uploader.set(false);
                        is_publishing.set(false);
                    }
                }
                Err(e) => {
                    log::error!("Failed to publish note: {}", e);
                    feedback_version.set(feedback_version() + 1);
                    let current_version = feedback_version();
                    publish_feedback.set(Some((false, format!("Error: {}", e))));
                    is_publishing.set(false);

                    // Auto-hide error after 5 seconds (only if version unchanged)
                    gloo_timers::future::TimeoutFuture::new(5000).await;
                    if feedback_version() == current_version {
                        publish_feedback.set(None);
                    }
                }
            }
        });
    };

    let handle_cancel = move |_| {
        content.set(String::new());
        show_image_uploader.set(false);
        is_focused.set(false);
    };

    // Helper to insert text at cursor position
    let mut insert_at_cursor = move |text: String| {
        let mut current = content.read().clone();
        let pos = *cursor_position.read();
        
        // Ensure position is valid
        let pos = pos.min(current.len());
        
        // Insert text
        current.insert_str(pos, &text);
        
        // Update content
        content.set(current);
        
        // Update cursor position to be after inserted text
        cursor_position.set(pos + text.len());
    };

    // Helper to insert text with smart spacing (space before if needed, space after)
    let mut insert_with_spacing = move |text: String| {
        let mut text_with_space = text.clone();
        // Add space before if not at start and not preceded by whitespace
        {
            let current = content.read();
            let pos = *cursor_position.read();
            if pos > 0 && pos <= current.len() {
                if let Some(prev_char) = current[..pos].chars().last() {
                    if !prev_char.is_whitespace() {
                        text_with_space.insert(0, ' ');
                    }
                }
            }
        }
        // Add space after
        text_with_space.push(' ');
        insert_at_cursor(text_with_space);
    };

    // Handler when image upload completes
    let handle_image_uploaded = move |url: String| {
        insert_with_spacing(url.clone());
        log::info!("Image URL inserted: {}", url);
    };

    // Handler when emoji is selected
    let handle_emoji_selected = move |emoji: String| {
        insert_at_cursor(emoji);
    };

    // Handler when GIF is selected
    let handle_gif_selected = move |gif_url: String| {
        insert_with_spacing(gif_url.clone());
        log::info!("GIF URL inserted: {}", gif_url);
    };

    // Handler when poll is created
    let handle_poll_created = move |nevent_ref: String| {
        insert_with_spacing(nevent_ref.clone());
        show_poll_modal.set(false);
        log::info!("Poll reference inserted: {}", nevent_ref);
    };

    rsx! {
        div {
            class: "border-b border-border p-4 bg-background",

            if !*is_authenticated.read() {
                div {
                    class: "text-center py-8 text-muted-foreground",
                    p { "Sign in to create posts" }
                }
            } else {
                // Publish feedback toast
                if let Some((is_success, message)) = publish_feedback.read().clone() {
                    div {
                        class: if is_success {
                            "mb-3 p-3 rounded-lg bg-yellow-500/10 border border-yellow-500/20 text-yellow-600 dark:text-yellow-400 text-sm flex items-center gap-2"
                        } else {
                            "mb-3 p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-red-600 dark:text-red-400 text-sm flex items-center gap-2"
                        },
                        span { "{message}" }
                        button {
                            class: "ml-auto text-current opacity-60 hover:opacity-100",
                            onclick: move |_| publish_feedback.set(None),
                            "×"
                        }
                    }
                }

                // Composer area
                div {
                    class: "w-full",

                        // Mention Autocomplete Textarea
                        MentionAutocomplete {
                            content: content,
                            on_input: move |new_value: String| {
                                content.set(new_value);
                            },
                            placeholder: "What's happening?".to_string(),
                            rows: if *is_focused.read() { 4 } else { 2 },
                            disabled: *is_publishing.read(),
                            onfocus: move |_| {
                                is_focused.set(true);
                            },
                            cursor_position: cursor_position
                        }

                        // Media uploader (conditionally shown)
                        if *show_image_uploader.read() {
                            div {
                                class: "mt-3",
                                MediaUploader {
                                    on_upload: handle_image_uploaded,
                                    button_label: "Upload Media"
                                }
                            }
                        }

                        // Actions (only show when focused or has content)
                        if *is_focused.read() || char_count > 0 {
                            div {
                                class: "mt-3 flex items-center justify-between",

                                // Left side: Media buttons and character counter
                                div {
                                    class: "flex items-center gap-2",

                                    // Media upload toggle button (icon-only)
                                    button {
                                        class: if *show_image_uploader.read() {
                                            "p-2 rounded-full bg-primary text-primary-foreground transition"
                                        } else {
                                            "p-2 rounded-full hover:bg-accent transition"
                                        },
                                        title: "Add media",
                                        onclick: move |_| {
                                            let current = *show_image_uploader.read();
                                            show_image_uploader.set(!current);
                                        },
                                        disabled: *is_publishing.read(),
                                        CameraIcon { class: "w-5 h-5".to_string() }
                                    }

                                    // Emoji picker (icon-only)
                                    EmojiPicker {
                                        on_emoji_selected: handle_emoji_selected,
                                        icon_only: true
                                    }

                                    // GIF picker (icon-only)
                                    GifPicker {
                                        on_gif_selected: handle_gif_selected,
                                        icon_only: true
                                    }

                                    // Poll button (icon-only)
                                    button {
                                        class: "p-2 rounded-full hover:bg-accent transition",
                                        title: "Create poll",
                                        onclick: move |_| show_poll_modal.set(true),
                                        disabled: *is_publishing.read(),
                                        BarChartIcon { class: "w-5 h-5".to_string() }
                                    }

                                    // Character counter
                                    div {
                                        class: "text-sm {counter_color} ml-2",
                                        if is_over_limit {
                                            span { "Over limit by {char_count - MAX_LENGTH}" }
                                        } else {
                                            span { "{char_count} / {MAX_LENGTH}" }
                                        }
                                    }
                                }

                                // Action buttons
                                div {
                                    class: "flex gap-2",

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
                                            "Post"
                                        }
                                    }
                                }
                            }
                        }

                // Poll creator modal (inside auth block)
                PollCreatorModal {
                    show: show_poll_modal,
                    on_poll_created: handle_poll_created
                }
                }
            }
        }
    }
}
