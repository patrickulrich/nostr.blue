use crate::components::icons::{BarChartIcon, CameraIcon};
use crate::components::{
    EmojiPicker, GifPicker, MediaUploader, MentionAutocomplete, PollCreatorModal, RichContent,
};
use crate::stores::nostr_client::{get_client, send_presigned_event_to_relays, HAS_SIGNER};
use crate::stores::relay;
use crate::utils::custom_emoji::{build_custom_emoji_tags, EmojiSelection};
use crate::utils::thread_tree::invalidate_thread_tree_cache;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;
use std::time::Duration;

fn get_relay_hint_for_reply(parent_tags: &nostr_sdk::Tags) -> String {
    for tag in parent_tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.len() >= 3 && tag_vec[0] == "e" && !tag_vec[2].is_empty() {
            return tag_vec[2].clone();
        }
    }
    relay::nip65::get_write_relays()
        .first()
        .cloned()
        .unwrap_or_default()
}

const MAX_LENGTH: usize = 5000;

#[component]
pub fn ReplyComposer(
    reply_to: NostrEvent,
    #[props(default = None)] root_event: Option<NostrEvent>,
    on_close: EventHandler<()>,
    on_success: EventHandler<NostrEvent>,
) -> Element {
    let mut content = use_signal(String::new);
    let mut is_publishing = use_signal(|| false);
    let mut show_media_uploader = use_signal(|| false);
    let mut uploaded_media = use_signal(Vec::<String>::new);
    let mut show_poll_modal = use_signal(|| false);
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

    let author_pubkey = reply_to.pubkey.to_hex();
    let short_author = truncate_pubkey(&author_pubkey);
    let reply_content = reply_to.content.clone();
    let reply_tags: Vec<_> = reply_to.tags.iter().cloned().collect();

    let mut thread_participants = Vec::new();
    thread_participants.push(reply_to.pubkey);
    for public_key in reply_to.tags.public_keys() {
        if !thread_participants.contains(public_key) {
            thread_participants.push(*public_key);
        }
    }

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
        let pos = to_char_boundary(&current, pos);
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
        log::info!("GIF URL inserted: {}", gif_url);
        insert_with_spacing(gif_url);
    };

    let handle_poll_created = move |nevent_ref: String| {
        log::info!("Poll reference inserted: {}", nevent_ref);
        insert_with_spacing(nevent_ref);
        show_poll_modal.set(false);
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

        let reply_to_event = reply_to.clone();
        let root = root_event.clone();
        let thread_root_id = root
            .as_ref()
            .map(|r| r.id.to_hex())
            .unwrap_or_else(|| reply_to_event.id.to_hex());
        let content_for_publish = content_value.clone();
        let relay_hint = get_relay_hint_for_reply(&reply_to_event.tags);
        let toast_for_async = toast;

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

            let relay_url = if relay_hint.is_empty() {
                None
            } else {
                RelayUrl::parse(&relay_hint).ok()
            };

            let builder = EventBuilder::text_note_reply(
                &content_for_publish,
                &reply_to_event,
                root.as_ref(),
                relay_url,
            )
            .tags(build_custom_emoji_tags(&content_for_publish));

            let signed_event = match client
                .sign_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
                .await
            {
                Ok(e) => e,
                Err(e) => {
                    log::error!("Failed to sign reply: {}", e);
                    toast_for_async.error(
                        "Failed to publish".to_string(),
                        ToastOptions::new()
                            .description(e.to_string())
                            .duration(Duration::from_secs(3)),
                    );
                    is_publishing.set(false);
                    return;
                }
            };

            let mut success = false;
            match client.send_event(&signed_event).await {
                Ok(output) => {
                    if !output.success.is_empty() {
                        log::info!(
                            "Reply published: {} ({}/{} relays)",
                            output.id().to_hex(),
                            output.success.len(),
                            output.success.len() + output.failed.len(),
                        );
                        for (relay, error) in &output.failed {
                            log::warn!("Relay {} failed for reply: {}", relay, error);
                        }
                        success = true;
                    } else {
                        log::warn!(
                            "Gossip publish had 0 successes for reply {}, retrying with broadcast",
                            output.id().to_hex()
                        );
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Gossip publish failed for reply: {}, retrying with broadcast",
                        e
                    );
                }
            }

            if !success {
                let write_relays = relay::nip65::get_write_relays();
                if write_relays.is_empty() {
                    toast_for_async.error(
                        "Failed to publish".to_string(),
                        ToastOptions::new()
                            .description("No write relays configured")
                            .duration(Duration::from_secs(3)),
                    );
                    is_publishing.set(false);
                    return;
                }
                match send_presigned_event_to_relays(signed_event.clone(), write_relays).await {
                    Ok(result) if result.success_count() > 0 => {
                        log::info!(
                            "Reply published via broadcast: {} ({}/{} relays)",
                            result.event_id,
                            result.success_count(),
                            result.total_attempted()
                        );
                        success = true;
                    }
                    Ok(result) => {
                        log::warn!(
                            "Broadcast publish also had 0 successes for reply {}",
                            result.event_id
                        );
                        toast_for_async.error(
                            "Failed to publish".to_string(),
                            ToastOptions::new()
                                .description("No relay accepted the event")
                                .duration(Duration::from_secs(3)),
                        );
                    }
                    Err(e) => {
                        log::error!("Broadcast publish failed for reply: {}", e);
                        toast_for_async.error(
                            "Failed to publish".to_string(),
                            ToastOptions::new()
                                .description(e.to_string())
                                .duration(Duration::from_secs(3)),
                        );
                    }
                }
            }

            if success {
                if let Ok(root_event_id) = EventId::from_hex(&thread_root_id) {
                    invalidate_thread_tree_cache(&root_event_id);
                }
                content.set(String::new());
                uploaded_media.set(Vec::new());
                on_success.call(signed_event);
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
                    h3 { class: "text-lg font-bold", "Reply" }
                    button {
                        class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-full transition",
                        onclick: handle_cancel,
                        "✕"
                    }
                }
                div { class: "p-4 bg-gray-50 dark:bg-gray-900 border-b border-border",
                    div { class: "text-sm text-gray-600 dark:text-gray-400 mb-2",
                        "Replying to @{short_author}"
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
                        p { "Sign in to reply" }
                    }
                } else {
                    div { class: "p-4",
                        MentionAutocomplete {
                            content,
                            on_input: move |new_value: String| {
                                content.set(new_value);
                            },
                            placeholder: "Write your reply...".to_string(),
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
                                button {
                                    class: "p-2 rounded-full hover:bg-accent transition",
                                    title: "Create poll",
                                    "aria-label": "Create poll",
                                    onclick: move |_| show_poll_modal.set(true),
                                    disabled: *is_publishing.read(),
                                    BarChartIcon { class: "w-5 h-5".to_string() }
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
        PollCreatorModal { show: show_poll_modal, on_poll_created: handle_poll_created }
    }
}

/// Find the nearest valid UTF-8 char boundary at or before the given byte position.
/// This prevents panics when inserting text at cursor positions in strings with
/// multi-byte characters (emojis, accented characters, etc.).
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
