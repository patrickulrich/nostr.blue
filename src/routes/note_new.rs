use dioxus::prelude::*;
use crate::stores::{nostr_client::publish_note, auth_store};
use crate::components::{MediaUploader, EmojiPicker, GifPicker};

const MAX_LENGTH: usize = 5000;

#[component]
pub fn NoteNew() -> Element {
    let navigator = navigator();
    let mut content = use_signal(|| String::new());
    let mut is_publishing = use_signal(|| false);
    let mut show_image_uploader = use_signal(|| false);

    // Check if user is authenticated
    let is_authenticated = use_memo(move || auth_store::AUTH_STATE.read().is_authenticated);

    // Character count logic
    let char_count = content.read().chars().count();
    let remaining = MAX_LENGTH.saturating_sub(char_count);
    let is_over_limit = char_count > MAX_LENGTH;
    let show_warning = remaining < 100 && !is_over_limit;
    let can_publish = char_count > 0 && !is_over_limit && !*is_publishing.read();

    let counter_color = if is_over_limit {
        "text-red-500"
    } else if show_warning {
        "text-yellow-500"
    } else {
        "text-muted-foreground"
    };

    // Handle publishing the note
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
                    is_publishing.set(false);
                    navigator.push(crate::routes::Route::Home {});
                }
                Err(e) => {
                    log::error!("Failed to publish note: {}", e);
                    is_publishing.set(false);
                }
            }
        });
    };

    // Handle closing the modal
    let handle_close = move |_| {
        navigator.go_back();
    };

    // Handler when image upload completes
    let handle_image_uploaded = move |url: String| {
        let mut current = content.read().clone();
        if !current.is_empty() && !current.ends_with('\n') && !current.ends_with(' ') {
            current.push(' ');
        }
        current.push_str(&url);
        content.set(current);
        show_image_uploader.set(false);
    };

    // Handler when emoji is selected
    let handle_emoji_selected = move |emoji: String| {
        let mut current = content.read().clone();
        current.push_str(&emoji);
        content.set(current);
    };

    // Handler when GIF is selected
    let handle_gif_selected = move |gif_url: String| {
        let mut current = content.read().clone();
        if !current.is_empty() && !current.ends_with('\n') && !current.ends_with(' ') {
            current.push(' ');
        }
        current.push_str(&gif_url);
        content.set(current);
    };

    // Redirect if not authenticated - effect must be called unconditionally
    use_effect(move || {
        if !*is_authenticated.read() {
            navigator.push(crate::routes::Route::Home {});
        }
    });

    // Return early with redirect message if not authenticated
    if !*is_authenticated.read() {
        return rsx! {
            div { class: "flex items-center justify-center h-screen",
                "Redirecting..."
            }
        };
    }

    rsx! {
        // Modal overlay backdrop
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-start justify-center overflow-y-auto",
            onclick: handle_close,

            // Modal content
            div {
                class: "bg-background border border-border rounded-lg shadow-xl w-full max-w-2xl m-4 mt-20",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between p-4 border-b border-border",
                    h2 {
                        class: "text-xl font-bold",
                        "Create Note"
                    }
                    button {
                        class: "text-muted-foreground hover:text-foreground transition",
                        onclick: handle_close,
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "w-6 h-6",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M6 18L18 6M6 6l12 12"
                            }
                        }
                    }
                }

                // Content area
                div {
                    class: "p-4",

                    // Textarea
                    textarea {
                        class: "w-full min-h-[200px] p-3 bg-background border border-border rounded-lg resize-y focus:outline-none focus:ring-2 focus:ring-blue-500",
                        placeholder: "What's on your mind?",
                        value: "{content}",
                        oninput: move |e| content.set(e.value()),
                        autofocus: true,
                    }

                    // Character counter
                    div {
                        class: "mt-2 text-sm {counter_color} text-right",
                        "{remaining} / {MAX_LENGTH}"
                    }

                    // Media uploader
                    if *show_image_uploader.read() {
                        div {
                            class: "mt-4",
                            MediaUploader {
                                on_upload: handle_image_uploaded,
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "flex items-center justify-between p-4 border-t border-border",

                    // Action buttons (left side)
                    div {
                        class: "flex gap-2",

                        // Image upload button
                        button {
                            class: "p-2 rounded-full hover:bg-accent transition",
                            title: "Add image",
                            onclick: move |_| {
                                let current = *show_image_uploader.read();
                                show_image_uploader.set(!current);
                            },
                            crate::components::icons::CameraIcon { class: "w-5 h-5".to_string() }
                        }

                        // Emoji picker (opens directly)
                        EmojiPicker {
                            on_emoji_selected: handle_emoji_selected,
                            icon_only: true
                        }

                        // GIF picker (opens directly)
                        GifPicker {
                            on_gif_selected: handle_gif_selected,
                            icon_only: true
                        }
                    }

                    // Publish button
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
        }
    }
}
