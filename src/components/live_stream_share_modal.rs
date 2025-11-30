use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, EventBuilder, PublicKey, FromBech32, Kind};
use crate::stores::{nostr_client, dms};
use crate::stores::nostr_client::HAS_SIGNER;
use crate::components::icons::{
    ShareIcon, CopyIcon, CheckIcon, MessageCircleIcon, SendIcon,
    Link2Icon, HashIcon, ArrowLeftIcon
};
use wasm_bindgen::JsValue;

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
    let mut share_mode = use_signal(|| ShareMode::Main);
    let mut copied = use_signal(|| false);
    let mut nostr_text = use_signal(|| String::new());
    let mut dm_recipient = use_signal(|| String::new());
    let mut is_publishing = use_signal(|| false);
    let mut dm_error = use_signal(|| Option::<String>::None);
    let mut nostr_error = use_signal(|| Option::<String>::None);

    let has_signer = *HAS_SIGNER.read();

    // Extract livestream title
    let content_title = title.unwrap_or_else(|| {
        event.tags.iter()
            .find(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("title"))
            .and_then(|tag| tag.as_slice().get(1).map(|s| s.to_string()))
            .unwrap_or_else(|| "Check out this livestream".to_string())
    });

    // Generate nostr.blue URL using naddr format
    let naddr = format!("30311:{}:{}", event.pubkey, d_tag);
    let stream_url = format!("https://nostr.blue/videos/live/{}", naddr);

    // Generate NIP-19 naddr identifier
    let stream_nip19 = use_signal(|| String::new());
    {
        let event_clone = event.clone();
        let d_tag_clone = d_tag.clone();
        let mut stream_nip19_clone = stream_nip19.clone();
        use_effect(move || {
            let d_tag_inner = d_tag_clone.clone();
            let event_inner = event_clone.clone();
            spawn(async move {
                use nostr_sdk::prelude::Coordinate;
                use nostr_sdk::ToBech32;

                // Create naddr coordinate
                let coord = Coordinate::new(Kind::from(30311), event_inner.pubkey)
                    .identifier(&d_tag_inner);

                let naddr_bech32 = coord.to_bech32().unwrap_or_else(|_| {
                    format!("30311:{}:{}", event_inner.pubkey, d_tag_inner)
                });

                stream_nip19_clone.set(format!("nostr:{}", naddr_bech32));
            });
        });
    }

    let handle_copy_link = {
        let stream_url = stream_url.clone();
        move |_| {
            let url = stream_url.clone();
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
    let stream_url_for_button = stream_url.clone();

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
                    nostr_error.set(Some("Failed to initialize Nostr client".to_string()));
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
        let stream_url = stream_url.clone();
        move |_| {
            let manual_recipient = dm_recipient.read().trim().to_string();

            if manual_recipient.is_empty() {
                return;
            }

            is_publishing.set(true);

            let stream_url_clone = stream_url.clone();

            spawn(async move {
                // Parse recipient as npub or hex
                let recipient_hex = if let Ok(pubkey) = PublicKey::from_bech32(&manual_recipient) {
                    pubkey.to_hex()
                } else if let Ok(pubkey) = PublicKey::parse(&manual_recipient) {
                    pubkey.to_hex()
                } else {
                    log::error!("Invalid recipient pubkey: {}", manual_recipient);
                    dm_error.set(Some("Invalid recipient. Please enter a valid npub or hex public key.".to_string()));
                    is_publishing.set(false);
                    return;
                };

                let message = format!("Check out this livestream on nostr.blue: {}", stream_url_clone);

                // Send DM using NIP-17
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

                // Body
                div {
                    class: "p-6 space-y-4",

                    // Main menu mode
                    if *share_mode.read() == ShareMode::Main {
                        // Livestream preview card
                        div {
                            class: "bg-accent rounded-lg p-4 flex items-center gap-3",
                            div {
                                class: "w-12 h-12 bg-gradient-to-br from-red-500 to-orange-500 rounded-lg flex items-center justify-center flex-shrink-0",
                                // Broadcast/live icon
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
                                        d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"
                                    }
                                }
                            }
                            div {
                                class: "flex-1 min-w-0",
                                p {
                                    class: "font-medium truncate",
                                    "{content_title}"
                                }
                                p {
                                    class: "text-sm text-muted-foreground",
                                    "nostr.blue Livestream"
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
                                        if has_signer { "Post about this livestream" } else { "Login required" }
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
                                placeholder: "Share your thoughts about this livestream...",
                                value: "{nostr_text}",
                                oninput: move |e| {
                                    nostr_text.set(e.value().clone());
                                    nostr_error.set(None);
                                },
                            }
                            // Error message display
                            if let Some(error) = nostr_error.read().as_ref() {
                                div {
                                    class: "mt-2 p-2 bg-red-500/10 border border-red-500/20 rounded text-sm text-red-500",
                                    "{error}"
                                }
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
                                        current.push_str(&stream_url_for_button);
                                        nostr_text.set(current);
                                    },
                                    Link2Icon { class: "w-3 h-3" }
                                    "nostr.blue Link"
                                }
                                button {
                                    class: "px-3 py-1.5 text-sm border border-border rounded-md hover:bg-accent transition flex items-center gap-1",
                                    onclick: move |_| {
                                        let nip19_value = stream_nip19.read().clone();
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
                                    disabled: stream_nip19.read().is_empty() || *stream_nip19.read() == "nostr:",
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
                                    oninput: move |e| {
                                        dm_recipient.set(e.value().clone());
                                        dm_error.set(None);
                                    },
                                }
                                // Error message display
                                if let Some(error) = dm_error.read().as_ref() {
                                    div {
                                        class: "mt-2 p-2 bg-red-500/10 border border-red-500/20 rounded text-sm text-red-500",
                                        "{error}"
                                    }
                                }
                            }

                            // Send button
                            button {
                                class: if dm_recipient.read().trim().is_empty() || *is_publishing.read() {
                                    "w-full px-4 py-2 bg-muted text-muted-foreground rounded-lg cursor-not-allowed flex items-center justify-center gap-2"
                                } else {
                                    "w-full px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition flex items-center justify-center gap-2"
                                },
                                onclick: handle_send_dm,
                                disabled: dm_recipient.read().trim().is_empty() || *is_publishing.read(),
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
