use crate::components::icons::CameraIcon;
use crate::components::{EmojiPicker, GifPicker, MediaUploader, MentionAutocomplete, RichContent};
use crate::stores::edit_cache;
use crate::stores::nostr_client::{edits, HAS_SIGNER};
use crate::utils::custom_emoji::EmojiSelection;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr_sdk::Event as NostrEvent;
use std::time::Duration;

const MAX_LENGTH: usize = 5000;

fn to_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    if s.is_char_boundary(pos) {
        return pos;
    }
    for offset in 1..=3 {
        if pos >= offset && s.is_char_boundary(pos - offset) {
            return pos - offset;
        }
    }
    0
}

#[component]
pub fn EditPostView(
    original_event: NostrEvent,
    on_close: EventHandler<()>,
    on_success: EventHandler<NostrEvent>,
    #[props(default)] prefill_content: Option<String>,
    #[props(default)] is_proposal: bool,
) -> Element {
    let initial_content = prefill_content
        .clone()
        .unwrap_or_else(|| original_event.content.clone());
    let mut content = use_signal(move || initial_content);
    let mut is_publishing = use_signal(|| false);
    let mut show_media_uploader = use_signal(|| false);
    let mut uploaded_media = use_signal(Vec::<String>::new);
    let toast = consume_toast();

    let content_len = content.read().len();
    let media_len = if !uploaded_media.read().is_empty() {
        let separator_len = if content_len > 0 { 2 } else { 0 };
        let urls_with_newlines: usize = uploaded_media.read().iter().map(|url| url.len() + 1).sum();
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

    let counter_color = if is_over_limit {
        "text-red-500"
    } else if show_warning {
        "text-yellow-500"
    } else {
        "text-gray-500"
    };

    let author_pubkey = original_event.pubkey.to_hex();
    let short_author = truncate_pubkey(&author_pubkey);
    let reply_content = original_event.content.clone();
    let reply_tags: Vec<_> = original_event.tags.iter().cloned().collect();

    let mut thread_participants = Vec::new();
    thread_participants.push(original_event.pubkey);
    for public_key in original_event.tags.public_keys() {
        if !thread_participants.contains(public_key) {
            thread_participants.push(*public_key);
        }
    }

    let title = if is_proposal {
        "Propose an Edit"
    } else {
        "Edit Post"
    };
    let subtitle = if is_proposal {
        format!("Proposing edit for post by @{short_author}")
    } else {
        format!("Editing post by @{short_author}")
    };
    let placeholder = if is_proposal {
        "Propose your edit...".to_string()
    } else {
        "Edit your post...".to_string()
    };
    let save_label = if is_proposal {
        "Send Proposal"
    } else {
        "Save Edit"
    };
    let notify_pubkey = if is_proposal {
        Some(original_event.pubkey)
    } else {
        None
    };

    let handle_media_uploaded = move |url: String| {
        uploaded_media.write().push(url);
        show_media_uploader.set(false);
    };

    let mut handle_remove_media = move |index: usize| {
        let mut media = uploaded_media.write();
        if index < media.len() {
            media.remove(index);
        }
    };

    let mut cursor_position = use_signal(|| 0usize);
    let mut insert_at_cursor = move |text: String| {
        let mut current = content.read().clone();
        let pos = to_char_boundary(&current, *cursor_position.read());
        current.insert_str(pos, &text);
        content.set(current);
        cursor_position.set(pos + text.len());
    };

    let mut insert_with_spacing = move |text: String| {
        let mut text_with_space = text;
        let current = content.read().clone();
        let pos = to_char_boundary(&current, *cursor_position.read());
        if pos > 0 {
            if let Some(prev_char) = current[..pos].chars().last() {
                if !prev_char.is_whitespace() {
                    text_with_space.insert(0, ' ');
                }
            }
        }
        if pos < current.len() {
            if let Some(next_char) = current[pos..].chars().next() {
                if !next_char.is_whitespace() {
                    text_with_space.push(' ');
                }
            }
        }
        insert_at_cursor(text_with_space);
    };

    let handle_emoji_selected = move |selection: EmojiSelection| {
        insert_at_cursor(selection.insertion_text());
    };

    let handle_gif_selected = move |gif_url: String| {
        insert_with_spacing(gif_url);
    };

    let handle_publish = move |_| {
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

        let content_for_publish = content_value.clone();
        let toast_for_async = toast;
        let original_event_clone = original_event.clone();
        let notify = notify_pubkey;

        spawn(async move {
            match edits::publish_edit(
                &original_event_clone,
                content_for_publish,
                None,
                notify,
            )
            .await
            {
                Ok(result) => {
                    if result.publish.is_success() {
                        log::info!(
                            "Edit published: {} ({}/{} relays)",
                            result.publish.event_id,
                            result.publish.success_count(),
                            result.publish.total_attempted()
                        );
                        edit_cache::process_edit_event(
                            &original_event_clone.id,
                            &result.event,
                            None,
                        );
                        content.set(String::new());
                        uploaded_media.set(Vec::new());
                        on_success.call(original_event_clone);
                    } else {
                        toast_for_async.error(
                            "Failed to publish edit".to_string(),
                            ToastOptions::new()
                                .description("No relay accepted the event")
                                .duration(Duration::from_secs(3)),
                        );
                    }
                }
                Err(e) => {
                    log::error!("Failed to publish edit: {}", e);
                    toast_for_async.error(
                        "Failed to publish edit".to_string(),
                        ToastOptions::new()
                            .description(e)
                            .duration(Duration::from_secs(3)),
                    );
                }
            }

            is_publishing.set(false);
        });
    };

    let handle_cancel = move |_| {
        content.set(String::new());
        uploaded_media.set(Vec::new());
        show_media_uploader.set(false);
        on_close.call(());
    };

    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-start justify-center pt-16 px-4",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-2xl w-full max-h-[80vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between p-4 border-b border-border",
                    h3 { class: "text-lg font-bold", "{title}" }
                    button {
                        class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-full transition",
                        onclick: handle_cancel,
                        "✕"
                    }
                }
                div { class: "p-4 bg-gray-50 dark:bg-gray-900 border-b border-border",
                    div { class: "text-sm text-gray-600 dark:text-gray-400 mb-2",
                        "{subtitle}"
                    }
                    div { class: "text-sm text-gray-700 dark:text-gray-300 line-clamp-3 overflow-hidden",
                        RichContent {
                            content: reply_content.clone(),
                            tags: reply_tags.clone(),
                        }
                    }
                }
                if !has_signer {
                    div { class: "text-center py-8 text-muted-foreground p-4",
                        p { "Sign in to edit" }
                    }
                } else {
                    div { class: "p-4",
                        MentionAutocomplete {
                            content,
                            on_input: move |new_value: String| {
                                content.set(new_value);
                            },
                            placeholder,
                            rows: 6,
                            disabled: *is_publishing.read(),
                            thread_participants: thread_participants.clone(),
                            cursor_position,
                        }
                        div { class: "text-sm {counter_color}",
                            if is_over_limit {
                                span { "Over limit by {char_count - MAX_LENGTH}" }
                            } else {
                                span { "{char_count} / {MAX_LENGTH}" }
                            }
                        }
                        if *show_media_uploader.read() {
                            div { class: "mt-3",
                                MediaUploader {
                                    on_upload: handle_media_uploaded,
                                    button_label: "Upload Media",
                                }
                            }
                        }
                        if !uploaded_media.read().is_empty() {
                            div { class: "mt-3 space-y-2",
                                p { class: "text-sm font-medium", "Uploaded Media:" }
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
                        div { class: "mt-3 flex items-center justify-between",
                            div { class: "flex gap-2",
                                button {
                                    class: if *show_media_uploader.read() { "p-2 rounded-full bg-primary text-primary-foreground transition" } else { "p-2 rounded-full hover:bg-accent transition" },
                                    title: "Add media",
                                    onclick: move |_| {
                                        let current = *show_media_uploader.read();
                                        show_media_uploader.set(!current);
                                    },
                                    disabled: *is_publishing.read(),
                                    CameraIcon { class: "w-5 h-5".to_string() }
                                }
                                EmojiPicker {
                                    on_emoji_selected: handle_emoji_selected,
                                    icon_only: true,
                                }
                                GifPicker {
                                    on_gif_selected: handle_gif_selected,
                                    icon_only: true,
                                }
                            }
                            div { class: "flex gap-2",
                                button {
                                    class: "px-4 py-2 text-sm font-medium hover:bg-accent rounded-full transition",
                                    onclick: handle_cancel,
                                    disabled: *is_publishing.read(),
                                    "Cancel"
                                }
                                button {
                                    class: "px-6 py-2 text-sm font-bold text-white bg-blue-500 hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed rounded-full transition flex items-center gap-2",
                                    disabled: !can_publish,
                                    onclick: handle_publish,
                                    if *is_publishing.read() {
                                        span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" }
                                        "Saving..."
                                    } else {
                                        "{save_label}"
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
