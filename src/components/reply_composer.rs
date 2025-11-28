use dioxus::prelude::*;
use crate::stores::nostr_client::{publish_note, HAS_SIGNER};
use crate::components::{MediaUploader, EmojiPicker, GifPicker, RichContent, MentionAutocomplete, PollCreatorModal};
use crate::components::icons::{CameraIcon, BarChartIcon};
use crate::utils::thread_tree::invalidate_thread_tree_cache;
use nostr_sdk::Event as NostrEvent;
use nostr_sdk::prelude::*;

const MAX_LENGTH: usize = 5000;

#[component]
pub fn ReplyComposer(
    reply_to: NostrEvent,
    on_close: EventHandler<()>,
    on_success: EventHandler<()>,
) -> Element {
    let mut content = use_signal(|| String::new());
    let mut is_publishing = use_signal(|| false);
    let mut show_media_uploader = use_signal(|| false);
    let mut uploaded_media = use_signal(|| Vec::<String>::new());
    let mut show_poll_modal = use_signal(|| false);

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
    let reply_tags: Vec<_> = reply_to.tags.iter().cloned().collect();
    let reply_id = reply_to.id.to_hex();

    // Extract thread participants (author + anyone mentioned in the note)
    let mut thread_participants = Vec::new();
    thread_participants.push(reply_to.pubkey); // Add author

    // Add anyone mentioned in p tags using SDK's public_keys()
    for public_key in reply_to.tags.public_keys() {
        if !thread_participants.contains(public_key) {
            thread_participants.push(*public_key);
        }
    }

    log::info!(
        "Reply composer: Extracted {} thread participants: author={}, others={:?}",
        thread_participants.len(),
        reply_to.pubkey.to_hex(),
        thread_participants.iter().skip(1).map(|pk| pk.to_hex()).collect::<Vec<_>>()
    );

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

    let mut cursor_position = use_signal(|| 0usize);

    // Helper to insert text at cursor position
    let mut insert_at_cursor = move |text: String| {
        let mut current = content.read().clone();
        let pos = *cursor_position.read();

        // Ensure position is a valid UTF-8 char boundary
        let pos = to_char_boundary(&current, pos);

        // Insert text
        current.insert_str(pos, &text);

        // Update content
        content.set(current);

        // Update cursor position to be after inserted text
        cursor_position.set(pos + text.len());
    };

    // Helper to insert text with smart spacing (space before if needed, space after only if needed)
    let mut insert_with_spacing = move |text: String| {
        let mut text_with_space = text;
        let current = content.read().clone();
        let pos = to_char_boundary(&current, *cursor_position.read());

        // Add space before if not at start and not preceded by whitespace
        if pos > 0 {
            if let Some(prev_char) = current[..pos].chars().last() {
                if !prev_char.is_whitespace() {
                    text_with_space.insert(0, ' ');
                }
            }
        }

        // Add space after only if next char exists and is not whitespace
        if pos < current.len() {
            if let Some(next_char) = current[pos..].chars().next() {
                if !next_char.is_whitespace() {
                    text_with_space.push(' ');
                }
            }
        }

        insert_at_cursor(text_with_space);
    };

    // Handler when emoji is selected
    let handle_emoji_selected = move |emoji: String| {
        insert_at_cursor(emoji);
    };

    // Handler when GIF is selected
    let handle_gif_selected = move |gif_url: String| {
        log::info!("GIF URL inserted: {}", gif_url);
        insert_with_spacing(gif_url);
    };

    // Handler when poll is created
    let handle_poll_created = move |nevent_ref: String| {
        log::info!("Poll reference inserted: {}", nevent_ref);
        insert_with_spacing(nevent_ref);
        show_poll_modal.set(false);
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

            // Determine the root event ID for cache invalidation
            let thread_root_id = if let Some(root_id) = &parent_root {
                // This is a nested reply - use the existing root
                root_id.clone()
            } else {
                // This is a direct reply - the parent IS the root
                event_id.clone()
            };

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

                    // Invalidate thread tree cache to ensure fresh data on next view
                    if let Ok(root_event_id) = EventId::from_hex(&thread_root_id) {
                        invalidate_thread_tree_cache(&root_event_id);
                        log::debug!("Invalidated thread tree cache for root: {}", thread_root_id);
                    }

                    content.set(String::new());
                    uploaded_media.set(Vec::new());
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
        uploaded_media.set(Vec::new());
        show_media_uploader.set(false);
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
                        class: "text-sm text-gray-700 dark:text-gray-300 line-clamp-3 overflow-hidden",
                        RichContent {
                            content: reply_content.clone(),
                            tags: reply_tags.clone()
                        }
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

                        // Mention Autocomplete Textarea
                        MentionAutocomplete {
                            content: content,
                            on_input: move |new_value: String| {
                                content.set(new_value);
                            },
                            placeholder: "Write your reply...".to_string(),
                            rows: 6,
                            disabled: *is_publishing.read(),
                            thread_participants: thread_participants.clone(),
                            cursor_position: cursor_position
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

                        // Media uploader
                        if *show_media_uploader.read() {
                            div {
                                class: "mt-3",
                                MediaUploader {
                                    on_upload: handle_media_uploaded,
                                    button_label: "Upload Media"
                                }
                            }
                        }

                        // Display uploaded media
                        if !uploaded_media.read().is_empty() {
                            div {
                                class: "mt-3 space-y-2",
                                p {
                                    class: "text-sm font-medium",
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

                        // Actions
                        div {
                            class: "mt-3 flex items-center justify-between",

                            // Left side - Media buttons (icon-only)
                            div {
                                class: "flex gap-2",

                                // Media upload toggle button (icon-only)
                                button {
                                    class: if *show_media_uploader.read() {
                                        "p-2 rounded-full bg-primary text-primary-foreground transition"
                                    } else {
                                        "p-2 rounded-full hover:bg-accent transition"
                                    },
                                    title: "Add media",
                                    onclick: move |_| {
                                        let current = *show_media_uploader.read();
                                        show_media_uploader.set(!current);
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
                                    "aria-label": "Create poll",
                                    onclick: move |_| show_poll_modal.set(true),
                                    disabled: *is_publishing.read(),
                                    BarChartIcon { class: "w-5 h-5".to_string() }
                                }
                            }

                            // Right side - Action buttons
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

        // Poll creator modal
        PollCreatorModal {
            show: show_poll_modal,
            on_poll_created: handle_poll_created
        }
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
    // Scan backwards at most 3 bytes (UTF-8 chars are max 4 bytes)
    for offset in 1..=3 {
        if pos >= offset && s.is_char_boundary(pos - offset) {
            return pos - offset;
        }
    }
    0
}
