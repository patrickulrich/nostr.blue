use dioxus::prelude::*;
use crate::stores::nostr_client::{HAS_SIGNER, get_client};
use nostr_sdk::{Event as NostrEvent, EventBuilder};

const MAX_LENGTH: usize = 5000;

/// NIP-22 Comment Composer for articles, videos, photos, etc.
#[component]
pub fn CommentComposer(
    /// The event being commented on (article, video, etc.)
    comment_on: NostrEvent,
    /// Optional parent comment (if replying to another comment)
    parent_comment: Option<NostrEvent>,
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

    let is_reply = parent_comment.is_some();

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

        let target_event = comment_on.clone();
        let parent = parent_comment.clone();

        spawn(async move {
            let client = match get_client() {
                Some(c) => c,
                None => {
                    log::error!("Client not initialized");
                    is_publishing.set(false);
                    return;
                }
            };

            // Build NIP-22 comment using EventBuilder::comment
            // This automatically creates the proper K/k, E/e, P/p tags
            // If we have a parent comment, we're replying to a comment (root = original event)
            // If no parent, we're commenting directly on the event (root = None)
            let (comment_to, root) = if let Some(parent_ref) = parent.as_ref() {
                // Replying to a comment: comment_to = parent comment, root = original event
                (parent_ref, Some(&target_event))
            } else {
                // Top-level comment: comment_to = original event, root = None
                (&target_event, None)
            };

            let builder = EventBuilder::comment(content_value, comment_to, root, None);

            match client.send_event_builder(builder).await {
                Ok(event_id) => {
                    log::info!("NIP-22 comment published: {}", event_id.to_hex());
                    content.set(String::new());
                    is_publishing.set(false);
                    on_success.call(());
                }
                Err(e) => {
                    log::error!("Failed to publish comment: {}", e);
                    is_publishing.set(false);
                }
            }
        });
    };

    rsx! {
        // Modal backdrop
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4",
            onclick: move |_| on_close.call(()),

            // Modal content (prevent clicks from closing)
            div {
                class: "bg-card border border-border rounded-lg shadow-xl max-w-2xl w-full max-h-[80vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "sticky top-0 bg-card border-b border-border px-6 py-4 flex items-center justify-between z-10",
                    h3 {
                        class: "text-lg font-semibold",
                        if is_reply {
                            "Reply to Comment"
                        } else {
                            "Add Comment"
                        }
                    }
                    button {
                        class: "text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }

                // Body
                div {
                    class: "p-6 space-y-4",

                    // Textarea
                    textarea {
                        class: "w-full min-h-[200px] p-4 bg-background border border-border rounded-lg resize-y focus:outline-none focus:ring-2 focus:ring-primary",
                        placeholder: if is_reply {
                            "Write your reply..."
                        } else {
                            "Write your comment..."
                        },
                        value: "{content.read()}",
                        oninput: move |e| content.set(e.value()),
                        disabled: !has_signer,
                    }

                    // Character counter
                    div {
                        class: "flex items-center justify-between text-sm",
                        span {
                            class: "{counter_color}",
                            "{char_count} / {MAX_LENGTH}"
                        }
                        if show_warning {
                            span {
                                class: "text-yellow-500",
                                "{remaining} characters remaining"
                            }
                        }
                        if is_over_limit {
                            span {
                                class: "text-red-500 font-semibold",
                                "Character limit exceeded!"
                            }
                        }
                    }

                    if !has_signer {
                        div {
                            class: "p-4 bg-yellow-100 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-lg",
                            p {
                                class: "text-yellow-800 dark:text-yellow-200 text-sm",
                                "⚠️ Please sign in to post comments"
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "sticky bottom-0 bg-card border-t border-border px-6 py-4 flex items-center justify-end gap-3 z-10",
                    button {
                        class: "px-4 py-2 rounded-lg border border-border hover:bg-accent transition",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: if can_publish {
                            "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition"
                        } else {
                            "px-4 py-2 bg-muted text-muted-foreground rounded-lg cursor-not-allowed"
                        },
                        disabled: !can_publish,
                        onclick: handle_publish,
                        if *is_publishing.read() {
                            "Publishing..."
                        } else {
                            "Publish Comment"
                        }
                    }
                }
            }
        }
    }
}
