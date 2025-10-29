use dioxus::prelude::*;
use crate::stores::{nostr_client::publish_note, auth_store};

const MAX_LENGTH: usize = 5000;

#[component]
pub fn NoteComposer() -> Element {
    let mut content = use_signal(|| String::new());
    let mut is_publishing = use_signal(|| false);
    let mut is_focused = use_signal(|| false);

    // Check if user is authenticated (can publish) using auth_store
    let is_authenticated = use_memo(move || auth_store::AUTH_STATE.read().is_authenticated);

    let char_count = content.read().len();
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
        is_focused.set(false);
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

                        // Textarea
                        textarea {
                            class: "w-full p-3 text-lg bg-transparent border border-input rounded-lg focus:outline-none focus:ring-2 focus:ring-ring resize-none",
                            placeholder: "What's happening?",
                            rows: if *is_focused.read() { "4" } else { "2" },
                            value: "{content}",
                            disabled: *is_publishing.read(),
                            oninput: move |evt| {
                                content.set(evt.value().clone());
                            },
                            onfocus: move |_| {
                                is_focused.set(true);
                            }
                        }

                        // Actions (only show when focused or has content)
                        if *is_focused.read() || char_count > 0 {
                            div {
                                class: "mt-3 flex items-center justify-between",

                                // Character counter
                                div {
                                    class: "text-sm {counter_color}",
                                    if is_over_limit {
                                        span { "Over limit by {char_count - MAX_LENGTH}" }
                                    } else {
                                        span { "{char_count} / {MAX_LENGTH}" }
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
