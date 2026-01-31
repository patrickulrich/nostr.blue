use crate::components::{EmojiPicker, GifPicker, MediaUploader, MentionAutocomplete};
use crate::stores::nostr_client::{get_client, HAS_SIGNER};
use crate::utils::thread_tree::invalidate_thread_tree_cache;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;
use std::time::Duration;

const MAX_LENGTH: usize = 5000;

/// NIP-22 Comment Composer for articles, videos, photos, etc.
#[component]
pub fn CommentComposer(
    /// The event being commented on (article, video, etc.)
    comment_on: NostrEvent,
    /// Optional parent comment (if replying to another comment)
    parent_comment: Option<NostrEvent>,
    on_close: EventHandler<()>,
    on_success: EventHandler<NostrEvent>,
) -> Element {
    let mut content = use_signal(String::new);
    let mut show_media_uploader = use_signal(|| false);
    let mut uploaded_media = use_signal(Vec::<String>::new);
    let mut is_publishing = use_signal(|| false);
    let toast = consume_toast();

    let content_len = content.read().len();
    let media_len = if !uploaded_media.read().is_empty() {
        let separator_len = if content_len > 0 { 2 } else { 0 };
        let urls_with_newlines: usize = uploaded_media
            .read()
            .iter()
            .map(|url| url.len() + 1)
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
    let can_publish = char_count > 0 && !is_over_limit && has_signer && !*is_publishing.read();
    let is_reply = parent_comment.is_some();

    let mut thread_participants = Vec::new();
    thread_participants.push(comment_on.pubkey);
    if let Some(parent) = &parent_comment {
        if !thread_participants.contains(&parent.pubkey) {
            thread_participants.push(parent.pubkey);
        }
        for tag in parent.tags.iter() {
            if let Some(TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() {
                if !thread_participants.contains(public_key) {
                    thread_participants.push(*public_key);
                }
            }
        }
    }
    for tag in comment_on.tags.iter() {
        if let Some(TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() {
            if !thread_participants.contains(public_key) {
                thread_participants.push(*public_key);
            }
        }
    }

    let counter_color = if is_over_limit {
        "text-red-500"
    } else if show_warning {
        "text-yellow-500"
    } else {
        "text-gray-500"
    };

    let handle_media_uploaded = move |url: String| {
        uploaded_media.write().push(url);
        show_media_uploader.set(false);
    };

    let mut handle_remove_media = move |index: usize| {
        let mut media = uploaded_media.write();
        if index < media.len() {
            media.remove(index);
        } else {
            log::warn!("Attempted to remove media at invalid index: {}", index);
        }
    };

    let mut cursor_position = use_signal(|| 0usize);
    let mut insert_at_cursor = move |text: String| {
        let mut current = content.read().clone();
        let pos = *cursor_position.read();
        let pos = if pos > current.len() {
            current.len()
        } else if !current.is_char_boundary(pos) {
            (0..=pos)
                .rev()
                .find(|&i| current.is_char_boundary(i))
                .unwrap_or(0)
        } else {
            pos
        };
        current.insert_str(pos, &text);
        content.set(current);
        cursor_position.set(pos + text.len());
    };

    let handle_emoji_selected = move |emoji: String| {
        insert_at_cursor(emoji);
    };

    let handle_gif_selected = move |gif_url: String| {
        let mut url_with_space = gif_url.clone();
        {
            let current = content.read();
            let pos = *cursor_position.read();
            if pos > 0 {
                if let Some(slice) = current.get(..pos) {
                    if let Some(prev_char) = slice.chars().last() {
                        if !prev_char.is_whitespace() {
                            url_with_space.insert(0, ' ');
                        }
                    }
                }
            }
        }
        url_with_space.push(' ');
        insert_at_cursor(url_with_space);
        log::info!("GIF URL inserted: {}", gif_url);
    };

    let handle_publish = {
        let toast_api = toast;
        move |_| {
            let mut content_value = content.read().clone();
            if !uploaded_media.read().is_empty() {
                if !content_value.is_empty() {
                    content_value.push_str("\n\n");
                }
                for url in uploaded_media.read().iter() {
                    content_value.push_str(url);
                    content_value.push('\n');
                }
            }

            if content_value.is_empty() || is_over_limit {
                return;
            }

            is_publishing.set(true);

            let target_event = comment_on.clone();
            let parent = parent_comment.clone();
            let content_for_publish = content_value.clone();
            let toast_for_async = toast_api;

            spawn(async move {
                let client = match get_client() {
                    Some(c) => c,
                    None => {
                        log::error!("Client not initialized");
                        toast_for_async.error(
                            "Unable to publish".to_string(),
                            ToastOptions::new()
                                .description("Client not initialized")
                                .duration(Duration::from_secs(3)),
                        );
                        is_publishing.set(false);
                        return;
                    }
                };

                let (comment_to, root) = if let Some(parent_ref) = parent.as_ref() {
                    (parent_ref, Some(&target_event))
                } else {
                    (&target_event, None)
                };

                let builder = EventBuilder::comment(content_for_publish, comment_to, root);

                // Sign the event first to get the full event
                match client.sign_event_builder(builder).await {
                    Ok(signed_event) => {
                        // Send the signed event
                        match client.send_event(&signed_event).await {
                            Ok(send_output) => {
                                log::info!("NIP-22 comment published: {}", send_output.id().to_hex());
                                // Invalidate cache so new comments appear on refresh
                                invalidate_thread_tree_cache(&target_event.id);
                                // Clear UI and call on_success with signed event for optimistic update
                                // nostr-sdk excludes self-published events from RelayPoolNotification::Event
                                content.set(String::new());
                                uploaded_media.set(Vec::new());
                                on_success.call(signed_event);
                            }
                            Err(e) => {
                                log::error!("Failed to send comment: {}", e);
                                toast_for_async.error(
                                    "Failed to publish".to_string(),
                                    ToastOptions::new()
                                        .description(format!("{}", e))
                                        .duration(Duration::from_secs(3)),
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to sign comment: {}", e);
                        toast_for_async.error(
                            "Failed to publish".to_string(),
                            ToastOptions::new()
                                .description(format!("{}", e))
                                .duration(Duration::from_secs(3)),
                        );
                    }
                }
                is_publishing.set(false);
            });
        }
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-card border border-border rounded-lg shadow-xl max-w-2xl w-full max-h-[80vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),
                div { class: "sticky top-0 bg-card border-b border-border px-6 py-4 flex items-center justify-between z-10",
                    h3 { class: "text-lg font-semibold",
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
                div { class: "p-6 space-y-4",
                    MentionAutocomplete {
                        content,
                        on_input: move |new_value: String| {
                            content.set(new_value);
                        },
                        placeholder: if is_reply { "Write your reply...".to_string() } else { "Write your comment...".to_string() },
                        class: "w-full min-h-[200px] p-4 bg-background border border-border rounded-lg resize-y focus:outline-hidden focus:ring-2 focus:ring-primary"
                            .to_string(),
                        rows: 8,
                        disabled: !has_signer || *is_publishing.read(),
                        thread_participants: thread_participants.clone(),
                        cursor_position,
                    }
                    div { class: "flex items-center justify-between text-sm",
                        span { class: "{counter_color}", "{char_count} / {MAX_LENGTH}" }
                        if show_warning {
                            span { class: "text-yellow-500", "{remaining} characters remaining" }
                        }
                        if is_over_limit {
                            span { class: "text-red-500 font-semibold", "Character limit exceeded!" }
                        }
                    }
                    if *show_media_uploader.read() {
                        MediaUploader {
                            on_upload: handle_media_uploaded,
                            button_label: "Upload Media",
                        }
                    }
                    if !uploaded_media.read().is_empty() {
                        div { class: "space-y-2",
                            p { class: "text-sm font-medium text-foreground", "Uploaded Media:" }
                            for (index , url) in uploaded_media.read().iter().enumerate() {
                                div {
                                    key: "{index}",
                                    class: "flex items-center gap-2 p-2 bg-accent rounded-lg",
                                    if url.ends_with(".mp4") || url.ends_with(".webm") || url.contains("video") {
                                        span { class: "text-sm", "Video" }
                                    } else {
                                        span { class: "text-sm", "Image" }
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
                        div { class: "p-4 bg-yellow-100 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-lg",
                            p { class: "text-yellow-800 dark:text-yellow-200 text-sm",
                                "Please sign in to post comments"
                            }
                        }
                    }
                }
                div { class: "sticky bottom-0 bg-card border-t border-border px-6 py-4 flex items-center justify-between gap-3 z-10",
                    div { class: "flex items-center gap-3",
                        if has_signer {
                            button {
                                class: if *show_media_uploader.read() { "px-3 py-2 bg-blue-600 text-white rounded-lg text-sm font-medium transition" } else { "px-3 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-lg text-sm font-medium transition" },
                                onclick: move |_| {
                                    let current = *show_media_uploader.read();
                                    show_media_uploader.set(!current);
                                },
                                disabled: *is_publishing.read(),
                                "Media"
                            }
                            EmojiPicker { on_emoji_selected: handle_emoji_selected }
                            GifPicker { on_gif_selected: handle_gif_selected }
                        }
                    }
                    div { class: "flex items-center gap-3",
                        button {
                            class: "px-4 py-2 rounded-lg border border-border hover:bg-accent transition",
                            onclick: move |_| on_close.call(()),
                            "Cancel"
                        }
                        button {
                            class: if can_publish { "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition flex items-center gap-2" } else { "px-4 py-2 bg-muted text-muted-foreground rounded-lg cursor-not-allowed" },
                            disabled: !can_publish,
                            onclick: handle_publish,
                            if *is_publishing.read() {
                                span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" }
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
