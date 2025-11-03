use dioxus::prelude::*;
use crate::stores::{nostr_client::publish_note, auth_store};
use crate::components::{MediaUploader, EmojiPicker, GifPicker, MentionAutocomplete};

const MAX_LENGTH: usize = 5000;

#[component]
pub fn NoteComposer() -> Element {
    let mut content = use_signal(|| String::new());
    let mut is_publishing = use_signal(|| false);
    let mut is_focused = use_signal(|| false);
    let mut show_image_uploader = use_signal(|| false);

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

    let handle_publish = move |_| {
        let content_value = content.read().clone();

        if content_value.is_empty() || is_over_limit {
            return;
        }

        is_publishing.set(true);

        spawn(async move {
            match publish_note(content_value, Vec::new()).await {
                Ok(event_id) => {
                    log::info!("Note published successfully: {}", event_id);
                    content.set(String::new());
                    show_image_uploader.set(false);
                    is_publishing.set(false);
                }
                Err(e) => {
                    log::error!("Failed to publish note: {}", e);
                    is_publishing.set(false);
                }
            }
        });
    };

    let handle_cancel = move |_| {
        content.set(String::new());
        show_image_uploader.set(false);
        is_focused.set(false);
    };

    // Handler when image upload completes
    let handle_image_uploaded = move |url: String| {
        // Insert URL at the end of the current content
        let mut current = content.read().clone();
        if !current.is_empty() && !current.ends_with('\n') && !current.ends_with(' ') {
            current.push(' ');
        }
        current.push_str(&url);
        content.set(current);

        log::info!("Image URL inserted: {}", url);
    };

    // Handler when emoji is selected
    let handle_emoji_selected = move |emoji: String| {
        // Insert emoji at the end of the current content
        let mut current = content.read().clone();
        current.push_str(&emoji);
        content.set(current);
    };

    // Handler when GIF is selected
    let handle_gif_selected = move |gif_url: String| {
        // Insert GIF URL at the end of the current content
        let mut current = content.read().clone();
        if !current.is_empty() && !current.ends_with('\n') && !current.ends_with(' ') {
            current.push(' ');
        }
        current.push_str(&gif_url);
        content.set(current);

        log::info!("GIF URL inserted: {}", gif_url);
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
                            }
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

                                // Left side: Image button and character counter
                                div {
                                    class: "flex items-center gap-3",

                                    // Media upload toggle button
                                    button {
                                        class: if *show_image_uploader.read() {
                                            "px-3 py-2 bg-blue-600 text-white rounded-lg text-sm font-medium transition"
                                        } else {
                                            "px-3 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-lg text-sm font-medium transition"
                                        },
                                        onclick: move |_| {
                                            let current = *show_image_uploader.read();
                                            show_image_uploader.set(!current);
                                        },
                                        disabled: *is_publishing.read(),
                                        "📎 Media"
                                    }

                                    // Emoji picker
                                    EmojiPicker {
                                        on_emoji_selected: handle_emoji_selected
                                    }

                                    // GIF picker
                                    GifPicker {
                                        on_gif_selected: handle_gif_selected
                                    }

                                    // Character counter
                                    div {
                                        class: "text-sm {counter_color}",
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
                }
            }
        }
    }
}
