use crate::components::icons::{
    ArrowLeftIcon, BarChartIcon, CameraIcon, CheckIcon, CopyIcon, HashIcon, Link2Icon,
    MessageCircleIcon, SendIcon, ShareIcon,
};
use crate::components::{EmojiPicker, GifPicker, MediaUploader, PollCreatorModal};
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::{dms, nostr_client};
use crate::utils::custom_emoji::{build_custom_emoji_tags, EmojiSelection};
use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, EventBuilder, FromBech32, Kind, PublicKey};
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
/// Global counter for generating unique modal IDs
static LIVE_SHARE_MODAL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
#[derive(Clone, Copy, PartialEq)]
enum ShareMode {
    Main,
    Nostr,
    Dm,
}
/// Share modal for livestreams
#[component]
pub fn LiveStreamShareModal(
    /// The livestream event being shared
    event: NostrEvent,
    /// The d-tag for naddr construction
    d_tag: String,
    /// Stream title for display
    title: Option<String>,
    /// Handler to close the modal
    on_close: EventHandler<()>,
) -> Element {
    let modal_id = use_signal(|| LIVE_SHARE_MODAL_ID_COUNTER.fetch_add(1, Ordering::Relaxed));
    let mut share_mode = use_signal(|| ShareMode::Main);
    let mut copied = use_signal(|| false);
    let mut clipboard_error = use_signal(|| None::<String>);
    let mut nostr_text = use_signal(String::new);
    let mut dm_recipient = use_signal(String::new);
    let mut is_publishing = use_signal(|| false);
    let mut dm_error = use_signal(|| Option::<String>::None);
    let mut nostr_error = use_signal(|| Option::<String>::None);
    let mut show_image_uploader = use_signal(|| false);
    let mut show_poll_modal = use_signal(|| false);
    let mut cursor_position = use_signal(|| 0usize);
    let textarea_id = use_signal(|| format!("live-share-textarea-{}", modal_id()));
    let has_signer = use_memo(move || *HAS_SIGNER.read());
    #[allow(unused_variables)]
    fn get_cursor_position(textarea_id: &str) -> usize {
        #[cfg(feature = "web")]
        {
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(element) = document.get_element_by_id(textarea_id) {
                        if let Some(textarea) = element.dyn_ref::<web_sys::HtmlTextAreaElement>() {
                            return textarea.selection_start().unwrap_or(Some(0)).unwrap_or(0)
                                as usize;
                        }
                    }
                }
            }
        }
        0
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
    fn utf8_to_utf16_index(text: &str, utf8_index: usize) -> usize {
        text[..utf8_index.min(text.len())].encode_utf16().count()
    }
    #[allow(unused_variables, dead_code)]
    fn set_cursor_position(textarea_id: &str, utf16_pos: usize) {
        #[cfg(feature = "web")]
        {
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(element) = document.get_element_by_id(textarea_id) {
                        if let Some(textarea) = element.dyn_ref::<web_sys::HtmlTextAreaElement>() {
                            let _ =
                                textarea.set_selection_range(utf16_pos as u32, utf16_pos as u32);
                        }
                    }
                }
            }
        }
    }
    #[allow(unused_variables)]
    fn set_cursor_position_deferred(textarea_id: String, utf16_pos: usize) {
        #[cfg(feature = "web")]
        {
            use wasm_bindgen::prelude::*;
            if let Some(window) = web_sys::window() {
                let js_closure = Closure::once_into_js(move || {
                    set_cursor_position(&textarea_id, utf16_pos);
                });
                let _ = window.request_animation_frame(js_closure.unchecked_ref());
            }
        }
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
                (0..=pos)
                    .rev()
                    .find(|&i| current.is_char_boundary(i))
                    .unwrap_or(0)
            };
            current.insert_str(safe_pos, &text);
            let new_cursor_pos = safe_pos + text.len();
            nostr_text.set(current.clone());
            cursor_position.set(new_cursor_pos);
            let utf16_pos = utf8_to_utf16_index(&current, new_cursor_pos);
            set_cursor_position_deferred(textarea_id.read().clone(), utf16_pos);
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
    let handle_emoji_selected = move |selection: EmojiSelection| {
        insert_at_cursor(selection.insertion_text());
    };
    let handle_gif_selected = move |gif_url: String| {
        insert_with_spacing(gif_url);
    };
    let handle_poll_created = move |nevent_ref: String| {
        insert_with_spacing(nevent_ref);
        show_poll_modal.set(false);
    };
    let content_title = title.unwrap_or_else(|| {
        event
            .tags
            .iter()
            .find(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("title"))
            .and_then(|tag| tag.as_slice().get(1).map(|s| s.to_string()))
            .unwrap_or_else(|| "Check out this livestream".to_string())
    });
    use nostr_sdk::prelude::Coordinate;
    use nostr_sdk::ToBech32;
    let coord = Coordinate::new(Kind::from(30311), event.pubkey).identifier(&d_tag);
    let naddr_bech32 = coord
        .to_bech32()
        .unwrap_or_else(|_| format!("30311:{}:{}", event.pubkey, d_tag));
    let stream_url = format!("https://nostr.blue/videos/live/{}", naddr_bech32);
    let stream_nip19 = format!("nostr:{}", naddr_bech32);
    let handle_copy_link = {
        let stream_url = stream_url.clone();
        move |_| {
            let url = stream_url.clone();
            spawn(async move {
                match copy_to_clipboard(&url).await {
                    Ok(_) => {
                        clipboard_error.set(None);
                        copied.set(true);
                        log::info!("Link copied to clipboard");
                        spawn(async move {
                            crate::platform::timer::sleep_ms(2000).await;
                            copied.set(false);
                        });
                    }
                    Err(e) => {
                        log::error!("Failed to copy to clipboard: {:?}", e);
                        clipboard_error.set(Some(e));
                    }
                }
            });
        }
    };
    let stream_url_for_button = stream_url.clone();
    let event_id = event.id;
    let handle_share_to_nostr = move |_| {
        if *is_publishing.read() {
            return;
        }
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
                    nostr_error.set(Some("Failed to initialize Nostr client".to_string()));
                    is_publishing.set(false);
                    return;
                }
            };
            let builder = EventBuilder::text_note(&text)
                .tag(nostr_sdk::Tag::event(event_id))
                .tags(build_custom_emoji_tags(&text));
            match client
                .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
                .await
            {
                Ok(output) => {
                    if output.success.is_empty() {
                        let details: Vec<String> = output
                            .failed
                            .iter()
                            .map(|(relay, reason)| format!("{}: {}", relay, reason))
                            .collect();
                        log::error!(
                            "Failed to share to Nostr: no relays accepted (failures: {})",
                            details.join(", ")
                        );
                        nostr_error.set(Some(
                            "Failed to post to Nostr: no relays accepted the event".to_string(),
                        ));
                        is_publishing.set(false);
                    } else {
                        log::info!("Shared to Nostr: {:?}", output.val);
                        nostr_error.set(None);
                        nostr_text.set(String::new());
                        share_mode.set(ShareMode::Main);
                        is_publishing.set(false);
                        on_close.call(());
                    }
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
        let stream_url = stream_url.clone();
        move |_| {
            if *is_publishing.read() {
                return;
            }
            let manual_recipient = dm_recipient.read().trim().to_string();
            if manual_recipient.is_empty() {
                return;
            }
            is_publishing.set(true);
            let stream_url_clone = stream_url.clone();
            spawn(async move {
                let recipient_hex = if let Ok(pubkey) = PublicKey::from_bech32(&manual_recipient) {
                    pubkey.to_hex()
                } else if let Ok(pubkey) = PublicKey::parse(&manual_recipient) {
                    pubkey.to_hex()
                } else {
                    log::error!("Invalid recipient pubkey: {}", manual_recipient);
                    dm_error.set(Some(
                        "Invalid recipient. Please enter a valid npub or hex public key."
                            .to_string(),
                    ));
                    is_publishing.set(false);
                    return;
                };
                let message = format!(
                    "Check out this livestream on nostr.blue: {}",
                    stream_url_clone,
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
                        h3 { class: "text-lg font-semibold ml-2",
                            match *share_mode.read() {
                                ShareMode::Main => "Share Livestream",
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
                        div { class: "bg-accent rounded-lg p-4 flex items-center gap-3",
                            div { class: "w-12 h-12 bg-gradient-to-br from-red-500 to-orange-500 rounded-lg flex items-center justify-center shrink-0",
                                svg {
                                    class: "w-6 h-6 text-white",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z",
                                    }
                                }
                            }
                            div { class: "flex-1 min-w-0",
                                p { class: "font-medium truncate", "{content_title}" }
                                p { class: "text-sm text-muted-foreground", "nostr.blue Livestream" }
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
                            if let Some(err) = clipboard_error.read().as_ref() {
                                p { class: "text-xs text-red-500", "{err}" }
                            }
                            button {
                                class: "w-full flex items-start gap-3 p-3 rounded-lg border border-border hover:bg-accent transition",
                                onclick: move |_| share_mode.set(ShareMode::Nostr),
                                disabled: !has_signer(),
                                MessageCircleIcon { class: "w-5 h-5 text-purple-500 shrink-0 mt-0.5" }
                                div { class: "text-left",
                                    p { class: "font-medium", "Share to Nostr" }
                                    p { class: "text-xs text-muted-foreground",
                                        if has_signer() {
                                            "Post about this livestream"
                                        } else {
                                            "Login required"
                                        }
                                    }
                                }
                            }
                            button {
                                class: "w-full flex items-start gap-3 p-3 rounded-lg border border-border hover:bg-accent transition",
                                onclick: move |_| share_mode.set(ShareMode::Dm),
                                disabled: !has_signer(),
                                SendIcon { class: "w-5 h-5 text-pink-500 shrink-0 mt-0.5" }
                                div { class: "text-left",
                                    p { class: "font-medium", "Share via DM" }
                                    p { class: "text-xs text-muted-foreground",
                                        if has_signer() {
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
                                placeholder: "Share your thoughts about this livestream...",
                                value: "{nostr_text}",
                                oninput: move |e| {
                                    nostr_text.set(e.value().clone());
                                    nostr_error.set(None);
                                    let pos = get_cursor_position(&textarea_id.read());
                                    let utf8_pos = utf16_to_utf8_index(&e.value(), pos);
                                    cursor_position.set(utf8_pos);
                                },
                                onclick: move |_| {
                                    let pos = get_cursor_position(&textarea_id.read());
                                    let text = nostr_text.read();
                                    let utf8_pos = utf16_to_utf8_index(&text, pos);
                                    cursor_position.set(utf8_pos);
                                },
                                onkeyup: move |_| {
                                    let pos = get_cursor_position(&textarea_id.read());
                                    let text = nostr_text.read();
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
                                        current.push_str(&stream_url_for_button);
                                        nostr_text.set(current);
                                    },
                                    Link2Icon { class: "w-3 h-3" }
                                    "nostr.blue Link"
                                }
                                {
                                    let stream_nip19_for_btn = stream_nip19.clone();
                                    rsx! {
                                        button {
                                            class: "px-3 py-1.5 text-sm border border-border rounded-md hover:bg-accent transition flex items-center gap-1",
                                            onclick: move |_| {
                                                let mut current = nostr_text.read().clone();
                                                if !current.is_empty() {
                                                    current.push(' ');
                                                }
                                                current.push_str(&stream_nip19_for_btn);
                                                nostr_text.set(current);
                                            },
                                            HashIcon { class: "w-3 h-3" }
                                            "Nostr Event"
                                        }
                                    }
                                }
                            }
                            div { class: "flex items-center gap-2",
                                button {
                                    class: if *show_image_uploader.read() { "p-2 rounded-full bg-primary text-primary-foreground transition" } else { "p-2 rounded-full hover:bg-accent transition" },
                                    title: "Add media",
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
async fn copy_to_clipboard(text: &str) -> Result<(), String> {
    crate::platform::clipboard::copy_to_clipboard(text).await
}
