use crate::components::icons::{
    ArrowLeftIcon, BarChartIcon, CameraIcon, CheckIcon, CopyIcon, FileVideoIcon,
    HashIcon, Link2Icon, MessageCircleIcon, SendIcon, ShareIcon,
};
use crate::components::{EmojiPicker, GifPicker, MediaUploader, PollCreatorModal};
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::{dms, nostr_client};
use crate::utils::clipboard::copy_to_clipboard;
use dioxus::html::input_data::keyboard_types::Key;
use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, EventBuilder, FromBech32, PublicKey};
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
/// Global counter for generating unique modal IDs
static SHARE_MODAL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
#[derive(Clone, Copy, PartialEq)]
enum ShareMode {
    Main,
    Nostr,
    Dm,
}
/// Share modal for videos
#[component]
pub fn ShareModal(
    /// The event being shared
    event: NostrEvent,
    /// Handler to close the modal
    on_close: EventHandler<()>,
) -> Element {
    let modal_id = use_signal(|| SHARE_MODAL_ID_COUNTER.fetch_add(1, Ordering::Relaxed));
    let title_id = format!("share-modal-title-{}", modal_id());
    let desc_id = format!("share-modal-desc-{}", modal_id());
    let mut share_mode = use_signal(|| ShareMode::Main);
    let mut copied = use_signal(|| false);
    let mut nostr_text = use_signal(String::new);
    let mut dm_recipient = use_signal(String::new);
    let mut is_publishing = use_signal(|| false);
    let mut dm_error = use_signal(|| Option::<String>::None);
    let mut nostr_error = use_signal(|| Option::<String>::None);
    let mut show_image_uploader = use_signal(|| false);
    let mut show_poll_modal = use_signal(|| false);
    let mut cursor_position = use_signal(|| 0usize);
    let textarea_id = use_signal(|| format!("share-textarea-{}", modal_id()));
    let has_signer = *HAS_SIGNER.read();
    #[allow(unused_variables)]
    fn get_cursor_position(textarea_id: &str, current_text: &str) -> usize {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(element) = document.get_element_by_id(textarea_id) {
                        if let Some(textarea) = element
                            .dyn_ref::<web_sys::HtmlTextAreaElement>()
                        {
                            return textarea
                                .selection_start()
                                .unwrap_or(Some(0))
                                .unwrap_or(0) as usize;
                        }
                    }
                }
            }
        }
        current_text.chars().map(|c| c.len_utf16()).sum()
    }
    fn utf16_to_utf8_index(text: &str, utf16_index: usize) -> usize {
        let mut utf8_index = 0;
        let mut utf16_count = 0;
        for c in text.chars() {
            if utf16_count >= utf16_index {
                break;
            }
            utf16_count += c.len_utf16();
            utf8_index += c.len_utf8();
        }
        utf8_index.min(text.len())
    }
    let mut insert_at_cursor = {
        let mut nostr_text = nostr_text;
        let mut cursor_position = cursor_position;
        move |text: String| {
            let mut current = nostr_text.read().clone();
            let pos = (*cursor_position.read()).min(current.len());
            let safe_pos = if current.is_char_boundary(pos) {
                pos
            } else {
                (0..=pos).rev().find(|&i| current.is_char_boundary(i)).unwrap_or(0)
            };
            current.insert_str(safe_pos, &text);
            nostr_text.set(current);
            cursor_position.set(safe_pos + text.len());
        }
    };
    let mut insert_with_spacing = {
        let nostr_text = nostr_text;
        let cursor_position = cursor_position;
        move |text: String| {
            let mut text_with_space = text.clone();
            {
                let current = nostr_text.read();
                let pos = (*cursor_position.read()).min(current.len());
                if pos > 0 {
                    let safe_pos = if current.is_char_boundary(pos) {
                        pos
                    } else {
                        (0..=pos)
                            .rev()
                            .find(|&i| current.is_char_boundary(i))
                            .unwrap_or(0)
                    };
                    if let Some(prev_char) = current[..safe_pos].chars().last() {
                        if !prev_char.is_whitespace() {
                            text_with_space.insert(0, ' ');
                        }
                    }
                }
            }
            text_with_space.push(' ');
            insert_at_cursor(text_with_space);
        }
    };
    let handle_image_uploaded = move |url: String| {
        insert_with_spacing(url);
    };
    let handle_emoji_selected = move |emoji: String| {
        insert_at_cursor(emoji);
    };
    let handle_gif_selected = move |gif_url: String| {
        insert_with_spacing(gif_url);
    };
    let handle_poll_created = move |nevent_ref: String| {
        insert_with_spacing(nevent_ref);
        show_poll_modal.set(false);
    };
    let content_title = event
        .tags
        .iter()
        .find(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("title"))
        .and_then(|tag| tag.as_slice().get(1).map(|s| s.to_string()))
        .unwrap_or_else(|| "Check out this content".to_string());
    let video_mp4_url = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("imeta"))
        .filter_map(|tag| {
            tag.as_slice()
                .iter()
                .skip(1)
                .find_map(|part| {
                    let s = part.as_str();
                    if s.starts_with("url ") {
                        Some(s.trim_start_matches("url ").to_string())
                    } else {
                        None
                    }
                })
        })
        .next()
        .unwrap_or_default();
    use nostr_sdk::{Kind, ToBech32};
    let is_recipe = event.tags.hashtags().any(|tag| tag == "nostrcooking");
    let is_article = event.kind == Kind::LongFormTextNote && !is_recipe;
    let content_url = if event.kind.is_addressable() {
        if let Some(coord) = event.coordinate() {
            match coord.to_bech32() {
                Ok(naddr) => {
                    if is_recipe {
                        format!("https://nostr.blue/recipes/{}", naddr)
                    } else {
                        format!("https://nostr.blue/articles/{}", naddr)
                    }
                }
                Err(_) => format!("https://nostr.blue/articles/{}", event.id.to_hex()),
            }
        } else {
            format!("https://nostr.blue/articles/{}", event.id.to_hex())
        }
    } else {
        format!("https://nostr.blue/videos/{}", event.id.to_hex())
    };
    let content_nip19 = use_signal(String::new);
    {
        let event_clone = event.clone();
        let mut content_nip19_clone = content_nip19;
        use_effect(move || {
            let event_for_async = event_clone.clone();
            spawn(async move {
                let nip19_str = if event_for_async.kind.is_addressable() {
                    if let Some(coord) = event_for_async.coordinate() {
                        coord.to_bech32().unwrap_or_else(|_| event_for_async.id.to_hex())
                    } else {
                        event_for_async
                            .id
                            .to_bech32()
                            .unwrap_or_else(|_| event_for_async.id.to_hex())
                    }
                } else {
                    event_for_async
                        .id
                        .to_bech32()
                        .unwrap_or_else(|_| event_for_async.id.to_hex())
                };
                content_nip19_clone.set(format!("nostr:{}", nip19_str));
            });
        });
    }
    let handle_copy_link = {
        let content_url_copy = content_url.clone();
        move |_| {
            let url = content_url_copy.clone();
            spawn(async move {
                match copy_to_clipboard(&url).await {
                    Ok(_) => {
                        copied.set(true);
                        log::info!("Link copied to clipboard");
                        spawn(async move {
                            #[cfg(target_arch = "wasm32")]
                            {
                                gloo_timers::future::TimeoutFuture::new(2000).await;
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                tokio::time::sleep(std::time::Duration::from_millis(2000))
                                    .await;
                            }
                            copied.set(false);
                        });
                    }
                    Err(e) => {
                        log::error!("Failed to copy to clipboard: {:?}", e);
                    }
                }
            });
        }
    };
    let content_url_for_button1 = content_url.clone();
    let video_mp4_url_for_button = video_mp4_url.clone();
    let handle_share_to_nostr = move |_| {
        let text = nostr_text.read().trim().to_string();
        if text.is_empty() {
            return;
        }
        is_publishing.set(true);
        spawn(async move {
            let client = match nostr_client::get_client() {
                Some(c) => c,
                None => {
                    log::error!("Client not initialized");
                    nostr_error
                        .set(Some("Failed to initialize Nostr client".to_string()));
                    is_publishing.set(false);
                    return;
                }
            };
            let builder = EventBuilder::text_note(&text);
            match client.send_event_builder(builder).await {
                Ok(output) => {
                    log::info!("Shared to Nostr: {:?}", output.val);
                    nostr_error.set(None);
                    nostr_text.set(String::new());
                    share_mode.set(ShareMode::Main);
                    is_publishing.set(false);
                    on_close.call(());
                }
                Err(e) => {
                    log::error!("Failed to share to Nostr: {}", e);
                    nostr_error.set(Some(format!("Failed to post to Nostr: {}", e)));
                    is_publishing.set(false);
                }
            }
        });
    };
    let handle_send_dm = {
        let content_url_dm = content_url.clone();
        let is_recipe_dm = is_recipe;
        let is_article_dm = is_article;
        move |_| {
            let manual_recipient = dm_recipient.read().trim().to_string();
            if manual_recipient.is_empty() {
                return;
            }
            is_publishing.set(true);
            let content_url_clone = content_url_dm.clone();
            let is_recipe_clone = is_recipe_dm;
            let is_article_clone = is_article_dm;
            spawn(async move {
                let recipient_hex = if let Ok(pubkey) = PublicKey::from_bech32(
                    &manual_recipient,
                ) {
                    pubkey.to_hex()
                } else if let Ok(pubkey) = PublicKey::parse(&manual_recipient) {
                    pubkey.to_hex()
                } else {
                    log::error!("Invalid recipient pubkey: {}", manual_recipient);
                    dm_error
                        .set(
                            Some(
                                "Invalid recipient. Please enter a valid npub or hex public key."
                                    .to_string(),
                            ),
                        );
                    is_publishing.set(false);
                    return;
                };
                let content_type = if is_recipe_clone {
                    "recipe"
                } else if is_article_clone {
                    "article"
                } else {
                    "video"
                };
                let message = format!(
                    "Check out this {} on nostr.blue: {}",
                    content_type,
                    content_url_clone,
                );
                match dms::send_dm(recipient_hex.clone(), message).await {
                    Ok(_) => {
                        log::info!("Sent DM to {}", recipient_hex);
                        dm_error.set(None);
                        dm_recipient.set(String::new());
                        share_mode.set(ShareMode::Main);
                        is_publishing.set(false);
                        on_close.call(());
                    }
                    Err(e) => {
                        log::error!("Failed to send DM to {}: {}", recipient_hex, e);
                        dm_error.set(Some(format!("Failed to send message: {}", e)));
                        is_publishing.set(false);
                    }
                }
            });
        }
    };
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-card border border-border rounded-lg shadow-xl max-w-md w-full max-h-[80vh] overflow-y-auto",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "{title_id}",
                aria_describedby: if *share_mode.read() == ShareMode::Main { Some(desc_id.clone()) } else { None },
                tabindex: "-1",
                onmounted: move |_evt| {
                    #[cfg(target_arch = "wasm32")]
                    {
                        if let Some(html_element) = _evt.data().downcast::<web_sys::HtmlElement>() {
                            let _ = html_element.focus();
                        }
                    }
                },
                onkeydown: move |evt: KeyboardEvent| {
                    if evt.key() == Key::Escape {
                        evt.stop_propagation();
                        on_close.call(());
                    }
                },
                onclick: move |e| e.stop_propagation(),
                div { class: "sticky top-0 bg-card border-b border-border px-6 py-4 flex items-center justify-between z-10",
                    div { class: "flex items-center gap-2",
                        if *share_mode.read() != ShareMode::Main {
                            button {
                                class: "text-muted-foreground hover:text-foreground transition p-1",
                                onclick: move |_| share_mode.set(ShareMode::Main),
                                ArrowLeftIcon { class: "w-4 h-4" }
                            }
                        }
                        ShareIcon { class: "w-5 h-5" }
                        h3 {
                            class: "text-lg font-semibold ml-2",
                            id: "{title_id}",
                            match *share_mode.read() {
                                ShareMode::Main => {
                                    if is_recipe {
                                        "Share Recipe"
                                    } else if is_article {
                                        "Share Article"
                                    } else {
                                        "Share Video"
                                    }
                                }
                                ShareMode::Nostr => "Share to Nostr",
                                ShareMode::Dm => "Send via DM",
                            }
                        }
                    }
                    button {
                        class: "text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }
                div { class: "p-6 space-y-4",
                    if *share_mode.read() == ShareMode::Main {
                        div {
                            class: "bg-accent rounded-lg p-4 flex items-center gap-3",
                            id: "{desc_id}",
                            div { class: "w-12 h-12 bg-gradient-to-br from-purple-500 to-pink-500 rounded-lg flex items-center justify-center shrink-0",
                                if is_recipe {
                                    span { class: "text-2xl", "🍳" }
                                } else if is_article {
                                    HashIcon { class: "w-6 h-6 text-white" }
                                } else {
                                    FileVideoIcon { class: "w-6 h-6 text-white" }
                                }
                            }
                            div { class: "flex-1 min-w-0",
                                p { class: "font-medium truncate", "{content_title}" }
                                p { class: "text-sm text-muted-foreground",
                                    if is_recipe {
                                        "nostr.blue Recipe"
                                    } else if is_article {
                                        "nostr.blue Article"
                                    } else {
                                        "nostr.blue Video"
                                    }
                                }
                            }
                        }
                        div { class: "space-y-2",
                            p { class: "text-sm font-medium mb-3", "Choose how to share" }
                            button {
                                class: "w-full flex items-start gap-3 p-3 rounded-lg border border-border hover:bg-accent transition",
                                onclick: handle_copy_link,
                                if *copied.read() {
                                    CheckIcon { class: "w-5 h-5 text-green-500 shrink-0 mt-0.5" }
                                } else {
                                    CopyIcon { class: "w-5 h-5 text-blue-500 shrink-0 mt-0.5" }
                                }
                                div { class: "text-left",
                                    p { class: "font-medium",
                                        if *copied.read() {
                                            "Copied!"
                                        } else {
                                            "Copy to clipboard"
                                        }
                                    }
                                    p { class: "text-xs text-muted-foreground",
                                        "Copy link to share anywhere"
                                    }
                                }
                            }
                            button {
                                class: "w-full flex items-start gap-3 p-3 rounded-lg border border-border hover:bg-accent transition",
                                onclick: move |_| share_mode.set(ShareMode::Nostr),
                                disabled: !has_signer,
                                MessageCircleIcon { class: "w-5 h-5 text-purple-500 shrink-0 mt-0.5" }
                                div { class: "text-left",
                                    p { class: "font-medium", "Share to Nostr" }
                                    p { class: "text-xs text-muted-foreground",
                                        if has_signer {
                                            if is_recipe {
                                                "Post about this recipe"
                                            } else if is_article {
                                                "Post about this article"
                                            } else {
                                                "Post about this video"
                                            }
                                        } else {
                                            "Login required"
                                        }
                                    }
                                }
                            }
                            button {
                                class: "w-full flex items-start gap-3 p-3 rounded-lg border border-border hover:bg-accent transition",
                                onclick: move |_| share_mode.set(ShareMode::Dm),
                                disabled: !has_signer,
                                SendIcon { class: "w-5 h-5 text-pink-500 shrink-0 mt-0.5" }
                                div { class: "text-left",
                                    p { class: "font-medium", "Share via DM" }
                                    p { class: "text-xs text-muted-foreground",
                                        if has_signer {
                                            "Send privately to someone"
                                        } else {
                                            "Login required"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if *share_mode.read() == ShareMode::Nostr {
                        div { class: "space-y-3",
                            if *show_image_uploader.read() {
                                div { class: "mb-3",
                                    MediaUploader {
                                        on_upload: handle_image_uploaded,
                                        button_label: "Upload Media",
                                    }
                                }
                            }
                            label { class: "text-sm font-medium", "Compose your note" }
                            textarea {
                                id: "{textarea_id}",
                                class: "w-full min-h-[120px] p-3 bg-background border border-border rounded-lg resize-none focus:outline-hidden focus:ring-2 focus:ring-primary",
                                placeholder: "Share your thoughts about this video...",
                                value: "{nostr_text}",
                                oninput: move |e| {
                                    nostr_text.set(e.value().clone());
                                    nostr_error.set(None);
                                    let pos = get_cursor_position(&textarea_id.read(), &e.value());
                                    let utf8_pos = utf16_to_utf8_index(&e.value(), pos);
                                    cursor_position.set(utf8_pos);
                                },
                                onclick: move |_| {
                                    let text = nostr_text.read();
                                    let pos = get_cursor_position(&textarea_id.read(), &text);
                                    let utf8_pos = utf16_to_utf8_index(&text, pos);
                                    cursor_position.set(utf8_pos);
                                },
                                onkeyup: move |_| {
                                    let text = nostr_text.read();
                                    let pos = get_cursor_position(&textarea_id.read(), &text);
                                    let utf8_pos = utf16_to_utf8_index(&text, pos);
                                    cursor_position.set(utf8_pos);
                                },
                                onmouseup: move |_| {
                                    let text = nostr_text.read();
                                    let pos = get_cursor_position(&textarea_id.read(), &text);
                                    let utf8_pos = utf16_to_utf8_index(&text, pos);
                                    cursor_position.set(utf8_pos);
                                },
                            }
                            if let Some(error) = nostr_error.read().as_ref() {
                                div { class: "mt-2 p-2 bg-red-500/10 border border-red-500/20 rounded text-sm text-red-500",
                                    "{error}"
                                }
                            }
                            div { class: "flex flex-wrap gap-2",
                                button {
                                    class: "px-3 py-1.5 text-sm border border-border rounded-md hover:bg-accent transition flex items-center gap-1",
                                    onclick: move |_| {
                                        let mut current = nostr_text.read().clone();
                                        if !current.is_empty() {
                                            current.push(' ');
                                        }
                                        current.push_str(&content_url_for_button1);
                                        nostr_text.set(current);
                                    },
                                    Link2Icon { class: "w-3 h-3" }
                                    "nostr.blue Link"
                                }
                                if !is_article && !video_mp4_url.is_empty() {
                                    button {
                                        class: "px-3 py-1.5 text-sm border border-border rounded-md hover:bg-accent transition flex items-center gap-1",
                                        onclick: move |_| {
                                            let mut current = nostr_text.read().clone();
                                            if !current.is_empty() {
                                                current.push(' ');
                                            }
                                            current.push_str(&video_mp4_url_for_button);
                                            nostr_text.set(current);
                                        },
                                        FileVideoIcon { class: "w-3 h-3" }
                                        "MP4 URL"
                                    }
                                }
                                button {
                                    class: "px-3 py-1.5 text-sm border border-border rounded-md hover:bg-accent transition flex items-center gap-1",
                                    onclick: move |_| {
                                        let nip19_value = content_nip19.read().clone();
                                        if nip19_value.is_empty() || nip19_value == "nostr:" {
                                            return;
                                        }
                                        let mut current = nostr_text.read().clone();
                                        if !current.is_empty() {
                                            current.push(' ');
                                        }
                                        current.push_str(&nip19_value);
                                        nostr_text.set(current);
                                    },
                                    disabled: content_nip19.read().is_empty() || *content_nip19.read() == "nostr:",
                                    HashIcon { class: "w-3 h-3" }
                                    "Nostr Event"
                                }
                            }
                            div { class: "flex items-center gap-2",
                                button {
                                    class: if *show_image_uploader.read() { "p-2 rounded-full bg-primary text-primary-foreground transition" } else { "p-2 rounded-full hover:bg-accent transition" },
                                    title: "Add media",
                                    aria_label: "Add media",
                                    onclick: move |_| {
                                        let current = *show_image_uploader.read();
                                        show_image_uploader.set(!current);
                                    },
                                    disabled: *is_publishing.read(),
                                    CameraIcon { class: "w-5 h-5" }
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
                                    aria_label: "Create poll",
                                    onclick: move |_| show_poll_modal.set(true),
                                    disabled: *is_publishing.read(),
                                    BarChartIcon { class: "w-5 h-5" }
                                }
                            }
                            button {
                                class: if nostr_text.read().trim().is_empty() || *is_publishing.read() { "w-full px-4 py-2 bg-muted text-muted-foreground rounded-lg cursor-not-allowed flex items-center justify-center gap-2" } else { "w-full px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition flex items-center justify-center gap-2" },
                                onclick: handle_share_to_nostr,
                                disabled: nostr_text.read().trim().is_empty() || *is_publishing.read(),
                                MessageCircleIcon { class: "w-4 h-4" }
                                span {
                                    if *is_publishing.read() {
                                        "Posting..."
                                    } else {
                                        "Post to Nostr"
                                    }
                                }
                            }
                        }
                    }
                    if *share_mode.read() == ShareMode::Dm {
                        div { class: "space-y-3",
                            div {
                                label { class: "text-sm font-medium", "Send to npub or hex pubkey" }
                                input {
                                    class: "w-full mt-2 p-3 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "npub1... or hex pubkey",
                                    value: "{dm_recipient}",
                                    oninput: move |e| {
                                        dm_recipient.set(e.value().clone());
                                        dm_error.set(None);
                                    },
                                }
                                if let Some(error) = dm_error.read().as_ref() {
                                    div { class: "mt-2 p-2 bg-red-500/10 border border-red-500/20 rounded text-sm text-red-500",
                                        "{error}"
                                    }
                                }
                            }
                            button {
                                class: if dm_recipient.read().trim().is_empty() || *is_publishing.read() { "w-full px-4 py-2 bg-muted text-muted-foreground rounded-lg cursor-not-allowed flex items-center justify-center gap-2" } else { "w-full px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition flex items-center justify-center gap-2" },
                                onclick: handle_send_dm,
                                disabled: dm_recipient.read().trim().is_empty() || *is_publishing.read(),
                                SendIcon { class: "w-4 h-4" }
                                span {
                                    if *is_publishing.read() {
                                        "Sending..."
                                    } else {
                                        "Send Message"
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
