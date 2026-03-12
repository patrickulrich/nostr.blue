use crate::components::icons::{
    ArrowLeftIcon, BarChartIcon, BookOpenIcon, CameraIcon, CheckIcon, CopyIcon, Link2Icon,
    MessageCircleIcon, MusicIcon, RssIcon, SendIcon, ShareIcon,
};
use crate::components::{EmojiPicker, GifPicker, MediaUploader, PollCreatorModal};
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::{dms, nostr_client};
use crate::utils::clipboard::copy_to_clipboard;
use crate::utils::custom_emoji::{build_custom_emoji_tags, EmojiSelection};
use crate::utils::text::utf16_to_utf8_index;
use dioxus::prelude::*;
use nostr_sdk::{EventBuilder, PublicKey};
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
/// Global counter for generating unique modal IDs
static CONTENT_SHARE_MODAL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
#[derive(Clone, Copy, PartialEq)]
enum ShareMode {
    Main,
    Nostr,
    Dm,
}
/// Type of content being shared
#[derive(Clone, Copy, PartialEq)]
pub enum ContentType {
    Podcast,
    PodcastEpisode,
    MusicAlbum,
    MusicTrack,
    BibleVerse,
}
impl ContentType {
    fn label(&self) -> &'static str {
        match self {
            ContentType::Podcast => "Podcast",
            ContentType::PodcastEpisode => "Episode",
            ContentType::MusicAlbum => "Album",
            ContentType::MusicTrack => "Track",
            ContentType::BibleVerse => "Bible",
        }
    }
    fn share_label(&self) -> &'static str {
        match self {
            ContentType::Podcast => "Share Podcast",
            ContentType::PodcastEpisode => "Share Episode",
            ContentType::MusicAlbum => "Share Album",
            ContentType::MusicTrack => "Share Track",
            ContentType::BibleVerse => "Share Verses",
        }
    }
    fn post_placeholder(&self) -> &'static str {
        match self {
            ContentType::Podcast => "Share your thoughts about this podcast...",
            ContentType::PodcastEpisode => "Share your thoughts about this episode...",
            ContentType::MusicAlbum => "Share your thoughts about this album...",
            ContentType::MusicTrack => "Share your thoughts about this track...",
            ContentType::BibleVerse => "Share your thoughts about these verses...",
        }
    }
    fn dm_message(&self, url: &str) -> String {
        match self {
            ContentType::Podcast => {
                format!("Check out this podcast on nostr.blue: {}", url)
            }
            ContentType::PodcastEpisode => {
                format!("Check out this episode on nostr.blue: {}", url)
            }
            ContentType::MusicAlbum => {
                format!("Check out this album on nostr.blue: {}", url)
            }
            ContentType::MusicTrack => {
                format!("Check out this track on nostr.blue: {}", url)
            }
            ContentType::BibleVerse => {
                format!("Check out this Bible passage on nostr.blue: {}", url)
            }
        }
    }
}
/// Generic share modal for podcasts and music content
#[component]
pub fn ContentShareModal(
    /// Title of the content
    title: String,
    /// URL to share
    url: String,
    /// Type of content for display text
    content_type: ContentType,
    /// Optional image URL for preview
    image_url: Option<String>,
    /// Optional content text (e.g., verse text for Bible verses)
    #[props(default)]
    content: Option<String>,
    /// Handler to close the modal
    on_close: EventHandler<()>,
) -> Element {
    let modal_id = use_signal(|| CONTENT_SHARE_MODAL_ID_COUNTER.fetch_add(1, Ordering::Relaxed));
    let mut share_mode = use_signal(|| ShareMode::Main);
    let mut copied = use_signal(|| false);
    let mut copy_error = use_signal(|| Option::<String>::None);
    let mut copy_disabled = use_signal(|| false);
    let mut nostr_text = use_signal(String::new);
    let mut dm_recipient = use_signal(String::new);
    let mut is_publishing = use_signal(|| false);
    let mut dm_error = use_signal(|| Option::<String>::None);
    let mut nostr_error = use_signal(|| Option::<String>::None);
    let mut show_image_uploader = use_signal(|| false);
    let mut show_poll_modal = use_signal(|| false);
    let mut cursor_position = use_signal(|| 0usize);
    let textarea_id = use_signal(|| format!("content-share-textarea-{}", modal_id()));
    #[allow(unused_variables)]
    fn get_cursor_position(textarea_id: &str, current_text: &str) -> Option<usize> {
        #[cfg(feature = "web")]
        {
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(element) = document.get_element_by_id(textarea_id) {
                        if let Some(textarea) = element.dyn_ref::<web_sys::HtmlTextAreaElement>() {
                            return textarea
                                .selection_start()
                                .ok()
                                .flatten()
                                .map(|n| n as usize);
                        }
                    }
                }
            }
        }
        None
    }
    let mut insert_with_spacing = {
        let mut nostr_text = nostr_text;
        let mut cursor_position = cursor_position;
        move |text: String| {
            let mut text_with_space = text.clone();
            let current = nostr_text.read().clone();
            let pos = (*cursor_position.read()).min(current.len());
            let safe_pos = if current.is_char_boundary(pos) {
                pos
            } else {
                (0..=pos)
                    .rev()
                    .find(|&i| current.is_char_boundary(i))
                    .unwrap_or(0)
            };
            if safe_pos > 0 {
                if let Some(prev_char) = current[..safe_pos].chars().last() {
                    if !prev_char.is_whitespace() {
                        text_with_space.insert(0, ' ');
                    }
                }
            }
            text_with_space.push(' ');
            let mut new_text = current;
            new_text.insert_str(safe_pos, &text_with_space);
            nostr_text.set(new_text);
            cursor_position.set(safe_pos + text_with_space.len());
        }
    };
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
            nostr_text.set(current);
            cursor_position.set(safe_pos + text.len());
        }
    };
    let handle_image_uploaded = move |url: String| {
        if *is_publishing.read() {
            return;
        }
        insert_with_spacing(url);
    };
    let handle_emoji_selected = move |selection: EmojiSelection| {
        if *is_publishing.read() {
            return;
        }
        insert_at_cursor(selection.insertion_text());
    };
    let handle_gif_selected = move |gif_url: String| {
        if *is_publishing.read() {
            return;
        }
        insert_with_spacing(gif_url);
    };
    let handle_poll_created = move |nevent_ref: String| {
        insert_with_spacing(nevent_ref);
        show_poll_modal.set(false);
    };
    let handle_copy_link = {
        let copy_text = if matches!(content_type, ContentType::BibleVerse) {
            // For Bible verses, copy the URL (canonical reference), not the verse text
            url.clone()
        } else {
            url.clone()
        };
        move |_| {
            let text_to_copy = copy_text.clone();
            spawn(async move {
                match copy_to_clipboard(&text_to_copy).await {
                    Ok(_) => {
                        copy_error.set(None);
                        copy_disabled.set(false);
                        copied.set(true);
                        log::info!("Content copied to clipboard");
                        spawn(async move {
                            crate::platform::timer::sleep_ms(2000).await;
                            copied.set(false);
                        });
                    }
                    Err(e) => {
                        copy_error.set(Some(format!("Clipboard unavailable: {}", e)));
                        copy_disabled.set(true);
                        copied.set(false);
                        log::error!("Failed to copy to clipboard: {:?}", e);
                    }
                }
            });
        }
    };
    let handle_share_to_nostr = move |_| {
        if *is_publishing.read() {
            return;
        }
        if !*HAS_SIGNER.read() {
            log::error!("Attempted to share to Nostr without a signer");
            nostr_error.set(Some(
                "No signer available. Please log in first.".to_string(),
            ));
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
            let builder = EventBuilder::text_note(&text).tags(build_custom_emoji_tags(&text));
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
        let url_dm = url.clone();
        let content_type_dm = content_type;
        move |_| {
            if *is_publishing.read() {
                return;
            }
            // Guard: check signer availability before proceeding
            if !*HAS_SIGNER.read() {
                log::error!("Attempted to send DM without a signer");
                dm_error.set(Some(
                    "No signer available. Please log in first.".to_string(),
                ));
                return;
            }
            let manual_recipient = dm_recipient.read().trim().to_string();
            if manual_recipient.is_empty() {
                return;
            }
            is_publishing.set(true);
            let url_clone = url_dm.clone();
            spawn(async move {
                let recipient_hex = match PublicKey::parse(&manual_recipient) {
                    Ok(pubkey) => pubkey.to_hex(),
                    Err(_) => {
                        log::error!("Invalid recipient pubkey supplied");
                        dm_error.set(Some(
                            "Invalid recipient. Please enter a valid npub, hex, or nostr: URI."
                                .to_string(),
                        ));
                        is_publishing.set(false);
                        return;
                    }
                };
                let message = content_type_dm.dm_message(&url_clone);
                match dms::send_dm(recipient_hex.clone(), message).await {
                    Ok(_) => {
                        log::info!("Sent DM successfully");
                        dm_error.set(None);
                        dm_recipient.set(String::new());
                        share_mode.set(ShareMode::Main);
                        is_publishing.set(false);
                        on_close.call(());
                    }
                    Err(e) => {
                        log::error!("Failed to send DM: {}", e);
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
            onclick: move |_| {
                if !*is_publishing.read() {
                    on_close.call(());
                }
            },
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
                                ShareMode::Main => content_type.share_label(),
                                ShareMode::Nostr => "Share to Nostr",
                                ShareMode::Dm => "Send via DM",
                            }
                        }
                    }
                    button {
                        class: if *is_publishing.read() {
                            "text-muted-foreground transition opacity-50 cursor-not-allowed"
                        } else {
                            "text-muted-foreground hover:text-foreground transition"
                        },
                        disabled: *is_publishing.read(),
                        aria_label: "Close share modal",
                        onclick: move |_| {
                            if !*is_publishing.read() {
                                on_close.call(());
                            }
                        },
                        "✕"
                    }
                }
                div { class: "p-6 space-y-4",
                    if *share_mode.read() == ShareMode::Main {
                        div { class: "bg-accent rounded-lg p-4 flex items-center gap-3",
                            if let Some(ref img_url) = image_url {
                                img {
                                    src: "{img_url}",
                                    alt: "{title}",
                                    class: "w-12 h-12 rounded-lg object-cover shrink-0",
                                }
                            } else {
                                div { class: "w-12 h-12 bg-gradient-to-br from-purple-500 to-pink-500 rounded-lg flex items-center justify-center shrink-0",
                                    match content_type {
                                        ContentType::Podcast | ContentType::PodcastEpisode => rsx! {
                                            RssIcon { class: "w-6 h-6 text-white" }
                                        },
                                        ContentType::MusicAlbum | ContentType::MusicTrack => rsx! {
                                            MusicIcon { class: "w-6 h-6 text-white" }
                                        },
                                        ContentType::BibleVerse => rsx! {
                                            BookOpenIcon { class: "w-6 h-6 text-white" }
                                        },
                                    }
                                }
                            }
                            div { class: "flex-1 min-w-0",
                                p { class: "font-medium truncate", "{title}" }
                                p { class: "text-sm text-muted-foreground",
                                    "nostr.blue {content_type.label()}"
                                }
                            }
                        }
                        div { class: "space-y-2",
                            if let Some(error) = copy_error.read().as_ref() {
                                div { class: "rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-sm text-destructive",
                                    "{error}"
                                }
                            }
                            p { class: "text-sm font-medium mb-3", "Choose how to share" }
                            button {
                                class: if *copy_disabled.read() {
                                    "w-full flex items-start gap-3 p-3 rounded-lg border border-border opacity-50 cursor-not-allowed"
                                } else {
                                    "w-full flex items-start gap-3 p-3 rounded-lg border border-border hover:bg-accent transition"
                                },
                                onclick: handle_copy_link,
                                disabled: *copy_disabled.read(),
                                if *copied.read() {
                                    CheckIcon { class: "w-5 h-5 text-green-500 shrink-0 mt-0.5" }
                                } else {
                                    CopyIcon { class: "w-5 h-5 text-blue-500 shrink-0 mt-0.5" }
                                }
                                div { class: "text-left",
                                    p { class: "font-medium",
                                        if *copied.read() {
                                            "Copied!"
                                        } else if *copy_disabled.read() {
                                            "Clipboard unavailable"
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
                                class: if *HAS_SIGNER.read() { "w-full flex items-start gap-3 p-3 rounded-lg border border-border hover:bg-accent transition" } else { "w-full flex items-start gap-3 p-3 rounded-lg border border-border opacity-50 cursor-not-allowed" },
                                onclick: move |_| share_mode.set(ShareMode::Nostr),
                                disabled: !*HAS_SIGNER.read(),
                                MessageCircleIcon { class: "w-5 h-5 text-purple-500 shrink-0 mt-0.5" }
                                div { class: "text-left",
                                    p { class: "font-medium", "Share to Nostr" }
                                    p { class: "text-xs text-muted-foreground",
                                        if *HAS_SIGNER.read() {
                                            "Post about this content"
                                        } else {
                                            "Login required"
                                        }
                                    }
                                }
                            }
                            button {
                                class: if *HAS_SIGNER.read() { "w-full flex items-start gap-3 p-3 rounded-lg border border-border hover:bg-accent transition" } else { "w-full flex items-start gap-3 p-3 rounded-lg border border-border opacity-50 cursor-not-allowed" },
                                onclick: move |_| share_mode.set(ShareMode::Dm),
                                disabled: !*HAS_SIGNER.read(),
                                SendIcon { class: "w-5 h-5 text-pink-500 shrink-0 mt-0.5" }
                                div { class: "text-left",
                                    p { class: "font-medium", "Share via DM" }
                                    p { class: "text-xs text-muted-foreground",
                                        if *HAS_SIGNER.read() {
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
                                class: if *is_publishing.read() {
                                    "w-full min-h-[120px] p-3 bg-muted border border-border rounded-lg resize-none cursor-not-allowed opacity-70"
                                } else {
                                    "w-full min-h-[120px] p-3 bg-background border border-border rounded-lg resize-none focus:outline-hidden focus:ring-2 focus:ring-primary"
                                },
                                placeholder: "{content_type.post_placeholder()}",
                                value: "{nostr_text}",
                                disabled: *is_publishing.read(),
                                oninput: move |e| {
                                    if *is_publishing.read() {
                                        return;
                                    }
                                    nostr_text.set(e.value().clone());
                                    nostr_error.set(None);
                                    if let Some(pos) = get_cursor_position(&textarea_id.read(), &e.value()) {
                                        let utf8_pos = utf16_to_utf8_index(&e.value(), pos);
                                        cursor_position.set(utf8_pos);
                                    }
                                },
                                onclick: move |_| {
                                    if *is_publishing.read() {
                                        return;
                                    }
                                    let text = nostr_text.read();
                                    if let Some(pos) = get_cursor_position(&textarea_id.read(), &text) {
                                        let utf8_pos = utf16_to_utf8_index(&text, pos);
                                        cursor_position.set(utf8_pos);
                                    }
                                },
                                onkeyup: move |_| {
                                    if *is_publishing.read() {
                                        return;
                                    }
                                    let text = nostr_text.read();
                                    if let Some(pos) = get_cursor_position(&textarea_id.read(), &text) {
                                        let utf8_pos = utf16_to_utf8_index(&text, pos);
                                        cursor_position.set(utf8_pos);
                                    }
                                },
                            }
                            if let Some(error) = nostr_error.read().as_ref() {
                                div { class: "mt-2 p-2 bg-red-500/10 border border-red-500/20 rounded text-sm text-red-500",
                                    "{error}"
                                }
                            }
                            div { class: "flex flex-wrap gap-2",
                                {
                                    let show_verse_button = matches!(content_type, ContentType::BibleVerse);
                                    if show_verse_button {
                                        if let Some(ref verse_content) = content {
                                            let verse_text = verse_content.clone();
                                            let verse_title = title.clone();
                                            rsx! {
                                                button {
                                                    class: if *is_publishing.read() {
                                                        "px-3 py-1.5 text-sm border border-border rounded-md opacity-50 cursor-not-allowed flex items-center gap-1"
                                                    } else {
                                                        "px-3 py-1.5 text-sm border border-border rounded-md hover:bg-accent transition flex items-center gap-1"
                                                    },
                                                    disabled: *is_publishing.read(),
                                                    onclick: move |_| {
                                                        if *is_publishing.read() {
                                                            return;
                                                        }
                                                        let mut current = nostr_text.read().clone();
                                                        if !current.is_empty() {
                                                            current.push_str("\n\n");
                                                        }
                                                        current.push_str(&format!("{}\n\n— {}", verse_text, verse_title));
                                                        nostr_text.set(current.clone());
                                                        cursor_position.set(current.len());
                                                    },
                                                    BookOpenIcon { class: "w-3 h-3" }
                                                    "Add Verse"
                                                }
                                            }
                                        } else {
                                            rsx! {}
                                        }
                                    } else {
                                        rsx! {}
                                    }
                                }
                                button {
                                    class: if *is_publishing.read() {
                                        "px-3 py-1.5 text-sm border border-border rounded-md opacity-50 cursor-not-allowed flex items-center gap-1"
                                    } else {
                                        "px-3 py-1.5 text-sm border border-border rounded-md hover:bg-accent transition flex items-center gap-1"
                                    },
                                    disabled: *is_publishing.read(),
                                    onclick: {
                                        let url_for_button = url.clone();
                                        move |_| {
                                            if *is_publishing.read() {
                                                return;
                                            }
                                            let mut current = nostr_text.read().clone();
                                            if !current.is_empty() {
                                                current.push(' ');
                                            }
                                            current.push_str(&url_for_button);
                                            nostr_text.set(current.clone());
                                            cursor_position.set(current.len());
                                        }
                                    },
                                    Link2Icon { class: "w-3 h-3" }
                                    "Add Link"
                                }
                            }
                            div { class: "flex items-center gap-2",
                                if cfg!(feature = "web") {
                                    button {
                                        class: if *is_publishing.read() {
                                            "p-2 rounded-full opacity-50 cursor-not-allowed"
                                        } else if *show_image_uploader.read() {
                                            "p-2 rounded-full bg-primary text-primary-foreground transition"
                                        } else {
                                            "p-2 rounded-full hover:bg-accent transition"
                                        },
                                        title: "Add media",
                                        aria_label: "Add media",
                                        onclick: move |_| {
                                            if *is_publishing.read() {
                                                return;
                                            }
                                            let current = *show_image_uploader.read();
                                            show_image_uploader.set(!current);
                                        },
                                        disabled: *is_publishing.read(),
                                        CameraIcon { class: "w-5 h-5" }
                                    }
                                    if !*is_publishing.read() {
                                        EmojiPicker {
                                            on_emoji_selected: handle_emoji_selected,
                                            icon_only: true,
                                        }
                                        GifPicker {
                                            on_gif_selected: handle_gif_selected,
                                            icon_only: true,
                                        }
                                    }
                                    button {
                                        class: if *is_publishing.read() {
                                            "p-2 rounded-full opacity-50 cursor-not-allowed"
                                        } else {
                                            "p-2 rounded-full hover:bg-accent transition"
                                        },
                                        title: "Create poll",
                                        aria_label: "Create poll",
                                        onclick: move |_| {
                                            if *is_publishing.read() {
                                                return;
                                            }
                                            show_poll_modal.set(true);
                                        },
                                        disabled: *is_publishing.read(),
                                        BarChartIcon { class: "w-5 h-5" }
                                    }
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
