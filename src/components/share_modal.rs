use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, EventBuilder, Kind, Filter, PublicKey, FromBech32};
use crate::stores::{nostr_client, auth_store, dms};
use crate::stores::nostr_client::HAS_SIGNER;
use crate::components::icons::{
    ShareIcon, CopyIcon, CheckIcon, MessageCircleIcon, SendIcon,
    UsersIcon, SearchIcon, FileVideoIcon, Link2Icon, HashIcon, ArrowLeftIcon
};
use std::time::Duration;
use wasm_bindgen::JsValue;

#[derive(Clone, Copy, PartialEq)]
enum ShareMode {
    Main,
    Nostr,
    Dm,
}

/// Share modal for videos, articles, and other content
#[component]
pub fn ShareModal(
    /// The event being shared
    event: NostrEvent,
    /// Handler to close the modal
    on_close: EventHandler<()>,
) -> Element {
    let mut share_mode = use_signal(|| ShareMode::Main);
    let mut copied = use_signal(|| false);
    let mut nostr_text = use_signal(|| String::new());
    let mut dm_recipient = use_signal(|| String::new());
    let mut search_query = use_signal(|| String::new());
    let mut selected_recipients = use_signal(|| Vec::<String>::new());
    let mut following_list = use_signal(|| Vec::<(String, String, String)>::new()); // (pubkey, display_name, picture)
    let mut is_publishing = use_signal(|| false);
    let mut is_loading_following = use_signal(|| false);

    let has_signer = *HAS_SIGNER.read();

    // Extract video/content information
    let content_title = event.tags.iter()
        .find(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("title"))
        .and_then(|tag| tag.as_slice().get(1).map(|s| s.to_string()))
        .unwrap_or_else(|| "Check out this content".to_string());

    // Get MP4 URL from imeta tags
    let video_mp4_url = event.tags.iter()
        .filter(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("imeta"))
        .filter_map(|tag| {
            // Parse imeta tag to find url
            tag.as_slice().iter().skip(1)
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

    // Generate nostr.blue URL
    let video_url = format!("https://nostr.blue/videos/{}", event.id.to_hex());

    // Generate NIP-19 nevent identifier (note)
    let video_nip19 = {
        use nostr_sdk::ToBech32;
        format!("nostr:{}", event.id.to_bech32().unwrap_or_default())
    };

    // Load following list when entering DM mode
    use_effect(move || {
        if *share_mode.read() == ShareMode::Dm && following_list.read().is_empty() && !*is_loading_following.read() {
            is_loading_following.set(true);
            spawn(async move {
                if let Some(user_pubkey_str) = auth_store::get_pubkey() {
                    if let Ok(user_pubkey) = PublicKey::parse(&user_pubkey_str) {
                        // Fetch contact list (kind 3)
                        let filter = Filter::new()
                            .kind(Kind::ContactList)
                            .author(user_pubkey)
                            .limit(1);

                        match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5)).await {
                            Ok(events) => {
                                if let Some(contact_event) = events.first() {
                                    let mut contacts = Vec::new();

                                    // Extract p tags (contacts)
                                    for tag in contact_event.tags.iter() {
                                        if let Some(nostr_sdk::TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() {
                                            // Fetch metadata for this contact
                                            let metadata_filter = Filter::new()
                                                .kind(Kind::Metadata)
                                                .author(*public_key)
                                                .limit(1);

                                            if let Ok(meta_events) = nostr_client::fetch_events_aggregated(metadata_filter, Duration::from_secs(3)).await {
                                                if let Some(meta_event) = meta_events.first() {
                                                    if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&meta_event.content) {
                                                        let display_name = metadata.get("display_name")
                                                            .and_then(|v| v.as_str())
                                                            .or_else(|| metadata.get("name").and_then(|v| v.as_str()))
                                                            .unwrap_or("Unknown")
                                                            .to_string();

                                                        let picture = metadata.get("picture")
                                                            .and_then(|v| v.as_str())
                                                            .unwrap_or("")
                                                            .to_string();

                                                        contacts.push((
                                                            public_key.to_hex(),
                                                            display_name,
                                                            picture
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    following_list.set(contacts);
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to fetch following list: {}", e);
                            }
                        }
                    }
                }
                is_loading_following.set(false);
            });
        }
    });

    // Filter following list based on search
    let filtered_following = use_memo(move || {
        let query = search_query.read().to_lowercase();
        if query.is_empty() {
            following_list.read().clone()
        } else {
            following_list.read()
                .iter()
                .filter(|(_, name, _)| name.to_lowercase().contains(&query))
                .cloned()
                .collect()
        }
    });

    let handle_copy_link = {
        let video_url = video_url.clone();
        move |_| {
            let url = video_url.clone();
            spawn(async move {
            match copy_to_clipboard(&url).await {
                Ok(_) => {
                    copied.set(true);
                    log::info!("Link copied to clipboard");
                    spawn(async move {
                        gloo_timers::future::TimeoutFuture::new(2000).await;
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

    // Clone URLs for button handlers
    let video_url_for_button1 = video_url.clone();
    let video_mp4_url_for_button = video_mp4_url.clone();
    let video_nip19_for_button = video_nip19.clone();

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
                    is_publishing.set(false);
                    return;
                }
            };

            let builder = EventBuilder::text_note(&text);

            match client.send_event_builder(builder).await {
                Ok(output) => {
                    log::info!("Shared to Nostr: {:?}", output.val);
                    nostr_text.set(String::new());
                    share_mode.set(ShareMode::Main);
                    is_publishing.set(false);
                    on_close.call(());
                }
                Err(e) => {
                    log::error!("Failed to share to Nostr: {}", e);
                    is_publishing.set(false);
                }
            }
        });
    };

    let handle_send_dm = {
        let video_url = video_url.clone();
        move |_| {
            let recipients = selected_recipients.read().clone();
            let manual_recipient = dm_recipient.read().trim().to_string();

            if recipients.is_empty() && manual_recipient.is_empty() {
                return;
            }

            is_publishing.set(true);

            let video_url_clone = video_url.clone();

            spawn(async move {
            let mut all_recipients = recipients;

            // Add manual recipient if provided
            if !manual_recipient.is_empty() {
                // Try to parse as npub or hex
                if let Ok(pubkey) = PublicKey::from_bech32(&manual_recipient) {
                    all_recipients.push(pubkey.to_hex());
                } else if let Ok(pubkey) = PublicKey::parse(&manual_recipient) {
                    all_recipients.push(pubkey.to_hex());
                }
            }

            let message = format!("Check out this video on nostr.blue: {}", video_url_clone);

            // Send DM to each recipient using NIP-17
            for recipient_hex in &all_recipients {
                match dms::send_dm(recipient_hex.clone(), message.clone()).await {
                    Ok(_) => {
                        log::info!("Sent DM to {}", recipient_hex);
                    }
                    Err(e) => {
                        log::error!("Failed to send DM to {}: {}", recipient_hex, e);
                    }
                }
            }

            log::info!("Sent DMs to {} recipient(s)", all_recipients.len());
            dm_recipient.set(String::new());
            selected_recipients.set(Vec::new());
            search_query.set(String::new());
            share_mode.set(ShareMode::Main);
            is_publishing.set(false);
                on_close.call(());
            });
        }
    };

    let mut toggle_recipient = move |pubkey: String| {
        let mut recipients = selected_recipients.read().clone();
        if let Some(pos) = recipients.iter().position(|p| p == &pubkey) {
            recipients.remove(pos);
        } else {
            recipients.push(pubkey);
        }
        selected_recipients.set(recipients);
    };

    rsx! {
        // Modal backdrop
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-card border border-border rounded-lg shadow-xl max-w-md w-full max-h-[80vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "sticky top-0 bg-card border-b border-border px-6 py-4 flex items-center justify-between z-10",
                    div {
                        class: "flex items-center gap-2",
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
                            match *share_mode.read() {
                                ShareMode::Main => "Share Video",
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

                // Body
                div {
                    class: "p-6 space-y-4",

                    // Main menu mode
                    if *share_mode.read() == ShareMode::Main {
                        // Video preview card
                        div {
                            class: "bg-accent rounded-lg p-4 flex items-center gap-3",
                            div {
                                class: "w-12 h-12 bg-gradient-to-br from-purple-500 to-pink-500 rounded-lg flex items-center justify-center flex-shrink-0",
                                FileVideoIcon { class: "w-6 h-6 text-white" }
                            }
                            div {
                                class: "flex-1 min-w-0",
                                p {
                                    class: "font-medium truncate",
                                    "{content_title}"
                                }
                                p {
                                    class: "text-sm text-muted-foreground",
                                    "nostr.blue Video"
                                }
                            }
                        }

                        // Share options
                        div {
                            class: "space-y-2",
                            p {
                                class: "text-sm font-medium mb-3",
                                "Choose how to share"
                            }

                            // Copy link button
                            button {
                                class: "w-full flex items-start gap-3 p-3 rounded-lg border border-border hover:bg-accent transition",
                                onclick: handle_copy_link,
                                if *copied.read() {
                                    CheckIcon { class: "w-5 h-5 text-green-500 flex-shrink-0 mt-0.5" }
                                } else {
                                    CopyIcon { class: "w-5 h-5 text-blue-500 flex-shrink-0 mt-0.5" }
                                }
                                div {
                                    class: "text-left",
                                    p {
                                        class: "font-medium",
                                        if *copied.read() { "Copied!" } else { "Copy to clipboard" }
                                    }
                                    p {
                                        class: "text-xs text-muted-foreground",
                                        "Copy link to share anywhere"
                                    }
                                }
                            }

                            // Share to Nostr button
                            button {
                                class: "w-full flex items-start gap-3 p-3 rounded-lg border border-border hover:bg-accent transition",
                                onclick: move |_| share_mode.set(ShareMode::Nostr),
                                disabled: !has_signer,
                                MessageCircleIcon { class: "w-5 h-5 text-purple-500 flex-shrink-0 mt-0.5" }
                                div {
                                    class: "text-left",
                                    p {
                                        class: "font-medium",
                                        "Share to Nostr"
                                    }
                                    p {
                                        class: "text-xs text-muted-foreground",
                                        if has_signer { "Post about this video" } else { "Login required" }
                                    }
                                }
                            }

                            // Send via DM button
                            button {
                                class: "w-full flex items-start gap-3 p-3 rounded-lg border border-border hover:bg-accent transition",
                                onclick: move |_| share_mode.set(ShareMode::Dm),
                                disabled: !has_signer,
                                SendIcon { class: "w-5 h-5 text-pink-500 flex-shrink-0 mt-0.5" }
                                div {
                                    class: "text-left",
                                    p {
                                        class: "font-medium",
                                        "Share via DM"
                                    }
                                    p {
                                        class: "text-xs text-muted-foreground",
                                        if has_signer { "Send privately to someone" } else { "Login required" }
                                    }
                                }
                            }
                        }
                    }

                    // Nostr share mode
                    if *share_mode.read() == ShareMode::Nostr {
                        div {
                            class: "space-y-3",
                            label {
                                class: "text-sm font-medium",
                                "Compose your note"
                            }
                            textarea {
                                class: "w-full min-h-[120px] p-3 bg-background border border-border rounded-lg resize-none focus:outline-none focus:ring-2 focus:ring-primary",
                                placeholder: "Share your thoughts about this video...",
                                value: "{nostr_text}",
                                oninput: move |e| nostr_text.set(e.value().clone()),
                            }

                            // Link format buttons
                            div {
                                class: "flex flex-wrap gap-2",
                                button {
                                    class: "px-3 py-1.5 text-sm border border-border rounded-md hover:bg-accent transition flex items-center gap-1",
                                    onclick: move |_| {
                                        let mut current = nostr_text.read().clone();
                                        if !current.is_empty() {
                                            current.push(' ');
                                        }
                                        current.push_str(&video_url_for_button1);
                                        nostr_text.set(current);
                                    },
                                    Link2Icon { class: "w-3 h-3" }
                                    "nostr.blue Link"
                                }
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
                                    disabled: video_mp4_url.is_empty(),
                                    FileVideoIcon { class: "w-3 h-3" }
                                    "MP4 URL"
                                }
                                button {
                                    class: "px-3 py-1.5 text-sm border border-border rounded-md hover:bg-accent transition flex items-center gap-1",
                                    onclick: move |_| {
                                        let mut current = nostr_text.read().clone();
                                        if !current.is_empty() {
                                            current.push(' ');
                                        }
                                        current.push_str(&video_nip19_for_button);
                                        nostr_text.set(current);
                                    },
                                    HashIcon { class: "w-3 h-3" }
                                    "Nostr Event"
                                }
                            }

                            // Post button
                            button {
                                class: if nostr_text.read().trim().is_empty() || *is_publishing.read() {
                                    "w-full px-4 py-2 bg-muted text-muted-foreground rounded-lg cursor-not-allowed flex items-center justify-center gap-2"
                                } else {
                                    "w-full px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition flex items-center justify-center gap-2"
                                },
                                onclick: handle_share_to_nostr,
                                disabled: nostr_text.read().trim().is_empty() || *is_publishing.read(),
                                MessageCircleIcon { class: "w-4 h-4" }
                                span {
                                    if *is_publishing.read() { "Posting..." } else { "Post to Nostr" }
                                }
                            }
                        }
                    }

                    // DM mode
                    if *share_mode.read() == ShareMode::Dm {
                        div {
                            class: "space-y-3",

                            // Manual recipient input
                            div {
                                label {
                                    class: "text-sm font-medium",
                                    "Send to npub or hex pubkey"
                                }
                                input {
                                    class: "w-full mt-2 p-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "npub1... or hex pubkey",
                                    value: "{dm_recipient}",
                                    oninput: move |e| dm_recipient.set(e.value().clone()),
                                }
                            }

                            // Following list
                            if !following_list.read().is_empty() {
                                div {
                                    div {
                                        class: "flex items-center gap-2 mb-2",
                                        UsersIcon { class: "w-4 h-4" }
                                        label {
                                            class: "text-sm font-medium",
                                            "Or select from following"
                                        }
                                    }

                                    // Search input
                                    div {
                                        class: "relative mb-2",
                                        div {
                                            class: "absolute left-3 top-1/2 transform -translate-y-1/2",
                                            SearchIcon { class: "w-4 h-4 text-muted-foreground" }
                                        }
                                        input {
                                            class: "w-full pl-9 pr-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary",
                                            r#type: "text",
                                            placeholder: "Search following...",
                                            value: "{search_query}",
                                            oninput: move |e| search_query.set(e.value().clone()),
                                        }
                                    }

                                    // Following list scroll area
                                    div {
                                        class: "max-h-[200px] overflow-y-auto border border-border rounded-lg p-2 space-y-1",
                                        for (pubkey, display_name, picture) in filtered_following() {
                                            button {
                                                key: "{pubkey}",
                                                class: if selected_recipients.read().contains(&pubkey) {
                                                    "w-full flex items-center gap-2 p-2 rounded-lg bg-primary text-primary-foreground"
                                                } else {
                                                    "w-full flex items-center gap-2 p-2 rounded-lg hover:bg-accent"
                                                },
                                                onclick: {
                                                    let pubkey_clone = pubkey.clone();
                                                    move |_| toggle_recipient(pubkey_clone.clone())
                                                },

                                                // Avatar
                                                if !picture.is_empty() {
                                                    img {
                                                        class: "w-6 h-6 rounded-full",
                                                        src: "{picture}",
                                                        alt: "{display_name}",
                                                    }
                                                } else {
                                                    div {
                                                        class: "w-6 h-6 rounded-full bg-gradient-to-br from-blue-400 to-purple-500 flex items-center justify-center text-xs text-white",
                                                        {display_name.chars().next().unwrap_or('?').to_uppercase().to_string()}
                                                    }
                                                }

                                                span {
                                                    class: "truncate flex-1 text-left",
                                                    "{display_name}"
                                                }

                                                if selected_recipients.read().contains(&pubkey) {
                                                    CheckIcon { class: "w-4 h-4 flex-shrink-0" }
                                                }
                                            }
                                        }
                                    }

                                    if selected_recipients.read().len() > 0 {
                                        p {
                                            class: "text-sm text-muted-foreground mt-2",
                                            {
                                                let count = selected_recipients.read().len();
                                                format!("{} recipient{} selected", count, if count > 1 { "s" } else { "" })
                                            }
                                        }
                                    }
                                }
                            }

                            if *is_loading_following.read() {
                                p {
                                    class: "text-sm text-muted-foreground text-center",
                                    "Loading following list..."
                                }
                            }

                            // Send button
                            button {
                                class: if (dm_recipient.read().trim().is_empty() && selected_recipients.read().is_empty()) || *is_publishing.read() {
                                    "w-full px-4 py-2 bg-muted text-muted-foreground rounded-lg cursor-not-allowed flex items-center justify-center gap-2"
                                } else {
                                    "w-full px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition flex items-center justify-center gap-2"
                                },
                                onclick: handle_send_dm,
                                disabled: (dm_recipient.read().trim().is_empty() && selected_recipients.read().is_empty()) || *is_publishing.read(),
                                SendIcon { class: "w-4 h-4" }
                                span {
                                    if *is_publishing.read() { "Sending..." } else { "Send Message" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// Web API clipboard function
async fn copy_to_clipboard(text: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;
    let navigator = window.navigator();
    let clipboard = navigator.clipboard();

    wasm_bindgen_futures::JsFuture::from(clipboard.write_text(text))
        .await
        .map(|_| ())
}
