use dioxus::prelude::*;
use crate::stores::nostr_client::{HAS_SIGNER, get_client};
use crate::components::{MediaUploader, EmojiPicker, GifPicker, MentionAutocomplete};
use nostr_sdk::{Event as NostrEvent, EventBuilder};
use nostr_sdk::prelude::*;

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
    let mut show_media_uploader = use_signal(|| false);
    let mut uploaded_media = use_signal(|| Vec::<String>::new());

    // Calculate total length including media URLs
    let content_len = content.read().len();
    let media_len = if !uploaded_media.read().is_empty() {
        let separator_len = if content_len > 0 { 2 } else { 0 }; // "\n\n"
        let urls_with_newlines: usize = uploaded_media.read().iter()
            .map(|url| url.len() + 1) // +1 for '\n' after each URL
            .sum();
        separator_len + urls_with_newlines
    } else {
        0
    };
    let char_count = content_len + media_len;
    let remaining = MAX_LENGTH.saturating_sub(char_count);
    let is_over_limit = char_count > MAX_LENGTH;
    let show_warning = remaining < 100 && !is_over_limit;
    let has_signer = *HAS_SIGNER.read();
    let can_publish = char_count > 0 && !is_over_limit && !*is_publishing.read() && has_signer;

    let is_reply = parent_comment.is_some();

    // Extract thread participants (author of commented event + anyone in parent comment)
    let mut thread_participants = Vec::new();
    thread_participants.push(comment_on.pubkey); // Add author of original content

    // Add parent comment author if this is a reply to a comment
    if let Some(parent) = &parent_comment {
        if !thread_participants.contains(&parent.pubkey) {
            thread_participants.push(parent.pubkey);
        }

        // Add anyone mentioned in parent comment's p tags
        for tag in parent.tags.iter() {
            if let Some(TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() {
                if !thread_participants.contains(public_key) {
                    thread_participants.push(*public_key);
                }
            }
        }
    }

    // Add anyone mentioned in the original event's p tags
    for tag in comment_on.tags.iter() {
        if let Some(TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() {
            if !thread_participants.contains(public_key) {
                thread_participants.push(*public_key);
            }
        }
    }

    // Determine counter color
    let counter_color = if is_over_limit {
        "text-red-500"
    } else if show_warning {
        "text-yellow-500"
    } else {
        "text-gray-500"
    };

    // Handle media upload
    let handle_media_uploaded = move |url: String| {
        uploaded_media.write().push(url);
        show_media_uploader.set(false);
    };

    // Handle removing uploaded media
    let mut handle_remove_media = move |index: usize| {
        let mut media = uploaded_media.write();
        if index < media.len() {
            media.remove(index);
        } else {
            log::warn!("Attempted to remove media at invalid index: {}", index);
        }
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

    let handle_publish = move |_| {
        let mut content_value = content.read().clone();

        // Append media URLs to content
        if !uploaded_media.read().is_empty() {
            if !content_value.is_empty() {
                content_value.push_str("\n\n");
            }
            for url in uploaded_media.read().iter() {
                content_value.push_str(&url);
                content_value.push('\n');
            }
        }

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
                    uploaded_media.set(Vec::new());
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

                    // Mention Autocomplete Textarea
                    MentionAutocomplete {
                        content: content,
                        on_input: move |new_value: String| {
                            content.set(new_value);
                        },
                        placeholder: if is_reply {
                            "Write your reply...".to_string()
                        } else {
                            "Write your comment...".to_string()
                        },
                        class: "w-full min-h-[200px] p-4 bg-background border border-border rounded-lg resize-y focus:outline-none focus:ring-2 focus:ring-primary".to_string(),
                        rows: 8,
                        disabled: !has_signer,
                        thread_participants: thread_participants.clone()
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

                    // Media uploader
                    if *show_media_uploader.read() {
                        MediaUploader {
                            on_upload: handle_media_uploaded,
                            button_label: "Upload Media"
                        }
                    }

                    // Display uploaded media
                    if !uploaded_media.read().is_empty() {
                        div {
                            class: "space-y-2",
                            p {
                                class: "text-sm font-medium text-foreground",
                                "Uploaded Media:"
                            }
                            for (index, url) in uploaded_media.read().iter().enumerate() {
                                div {
                                    key: "{index}",
                                    class: "flex items-center gap-2 p-2 bg-accent rounded-lg",
                                    if url.ends_with(".mp4") || url.ends_with(".webm") || url.contains("video") {
                                        span { class: "text-sm", "🎥 Video" }
                                    } else {
                                        span { class: "text-sm", "🖼️ Image" }
                                    }
                                    a {
                                        class: "text-sm text-primary hover:underline truncate flex-1",
                                        href: "{url}",
                                        target: "_blank",
                                        "{url}"
                                    }
                                    button {
                                        class: "px-2 py-1 text-xs text-red-600 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300",
                                        onclick: move |_| handle_remove_media(index),
                                        "Remove"
                                    }
                                }
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
                    class: "sticky bottom-0 bg-card border-t border-border px-6 py-4 flex items-center justify-between gap-3 z-10",

                    // Left side - Media upload button
                    div {
                        class: "flex items-center gap-3",
                        if has_signer {
                            button {
                                class: if *show_media_uploader.read() {
                                    "px-3 py-2 bg-blue-600 text-white rounded-lg text-sm font-medium transition"
                                } else {
                                    "px-3 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-lg text-sm font-medium transition"
                                },
                                onclick: move |_| {
                                    let current = *show_media_uploader.read();
                                    show_media_uploader.set(!current);
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
                        }
                    }

                    // Right side - Action buttons
                    div {
                        class: "flex items-center gap-3",
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
}
