use dioxus::prelude::*;
use crate::stores::nostr_client::{publish_note, HAS_SIGNER};
use nostr_sdk::Event as NostrEvent;

const MAX_LENGTH: usize = 5000;

#[component]
pub fn ReplyComposer(
    reply_to: NostrEvent,
    on_close: EventHandler<()>,
    on_success: EventHandler<()>,
) -> Element {
    let mut content = use_signal(|| String::new());
    let mut is_publishing = use_signal(|| false);

    let char_count = content.read().len();
    let remaining = MAX_LENGTH.saturating_sub(char_count);
    let is_over_limit = char_count > MAX_LENGTH;
    let show_warning = remaining < 100 && !is_over_limit;
    let has_signer = *HAS_SIGNER.read();
    let can_publish = char_count > 0 && !is_over_limit && !*is_publishing.read() && has_signer;

    // Determine counter color
    let counter_color = if is_over_limit {
        "text-red-500"
    } else if show_warning {
        "text-yellow-500"
    } else {
        "text-gray-500"
    };

    // Get author info
    let author_pubkey = reply_to.pubkey.to_hex();
    let short_author = if author_pubkey.len() > 16 {
        format!("{}...{}", &author_pubkey[..8], &author_pubkey[author_pubkey.len()-4..])
    } else {
        author_pubkey.clone()
    };
    let reply_content = reply_to.content.clone();
    let reply_id = reply_to.id.to_hex();

    let handle_publish = move |_| {
        let content_value = content.read().clone();

        if content_value.is_empty() || is_over_limit {
            return;
        }

        is_publishing.set(true);

        let event_id = reply_id.clone();
        let author_pk = author_pubkey.clone();

        // Clone the tags from reply_to before moving into async block
        let parent_tags = reply_to.tags.clone();

        spawn(async move {
            // Build tags for reply following NIP-10 properly
            let mut tags = Vec::new();

            // Check if the event we're replying to has a root marker
            // to determine if this is a top-level reply or nested reply
            let parent_root = parent_tags.iter().find_map(|tag| {
                let tag_vec = tag.clone().to_vec();
                if tag_vec.len() >= 4
                    && tag_vec[0] == "e"
                    && tag_vec[3] == "root" {
                    Some(tag_vec[1].clone())
                } else {
                    None
                }
            });

            if let Some(root_id) = parent_root {
                // This is a nested reply (replying to a reply)
                // Add root marker for the thread root
                tags.push(vec!["e".to_string(), root_id, "".to_string(), "root".to_string()]);
                // Add reply marker for the immediate parent
                tags.push(vec!["e".to_string(), event_id.clone(), "".to_string(), "reply".to_string()]);
            } else {
                // This is a direct reply to root
                // Use only root marker (not reply)
                tags.push(vec!["e".to_string(), event_id.clone(), "".to_string(), "root".to_string()]);
            }

            // Collect all p tags from parent event plus the parent's author
            // Start with the parent's author
            tags.push(vec!["p".to_string(), author_pk.clone()]);

            // Add all p tags from the parent event (to notify everyone in thread)
            for tag in parent_tags.iter() {
                let tag_vec = tag.clone().to_vec();
                if tag_vec.len() >= 2 && tag_vec[0] == "p" {
                    let pubkey = tag_vec[1].clone();
                    // Don't duplicate the author we already added
                    if pubkey != author_pk {
                        tags.push(vec!["p".to_string(), pubkey]);
                    }
                }
            }

            match publish_note(content_value, tags).await {
                Ok(event_id) => {
                    log::info!("Reply published successfully: {}", event_id);
                    content.set(String::new());
                    is_publishing.set(false);
                    on_success.call(());
                }
                Err(e) => {
                    log::error!("Failed to publish reply: {}", e);
                    is_publishing.set(false);
                }
            }
        });
    };

    let handle_cancel = move |_| {
        content.set(String::new());
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
                        "Reply"
                    }
                    button {
                        class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-full transition",
                        onclick: handle_cancel,
                        "✕"
                    }
                }

                // Original note preview
                div {
                    class: "p-4 bg-gray-50 dark:bg-gray-900 border-b border-border",
                    div {
                        class: "text-sm text-gray-600 dark:text-gray-400 mb-2",
                        "Replying to @{short_author}"
                    }
                    div {
                        class: "text-sm text-gray-700 dark:text-gray-300 line-clamp-3",
                        "{reply_content}"
                    }
                }

                if !has_signer {
                    div {
                        class: "text-center py-8 text-muted-foreground p-4",
                        p { "Sign in to reply" }
                    }
                } else {
                    // Reply composer
                    div {
                        class: "p-4",

                        // Textarea
                        textarea {
                            class: "w-full p-3 text-lg bg-transparent border border-input rounded-lg focus:outline-none focus:ring-2 focus:ring-ring resize-none",
                            placeholder: "Write your reply...",
                            rows: "6",
                            value: "{content}",
                            disabled: *is_publishing.read(),
                            autofocus: true,
                            oninput: move |evt| {
                                content.set(evt.value().clone());
                            }
                        }

                        // Actions
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

                                // Reply button
                                button {
                                    class: "px-6 py-2 text-sm font-bold text-white bg-blue-500 hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed rounded-full transition flex items-center gap-2",
                                    disabled: !can_publish,
                                    onclick: handle_publish,

                                    if *is_publishing.read() {
                                        span {
                                            class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"
                                        }
                                        "Replying..."
                                    } else {
                                        "Reply"
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
