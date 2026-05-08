use crate::components::icons::{MaximizeIcon, XIcon};
use crate::components::{EmojiPicker, RichContent};
use crate::hooks::{use_mute_block_cache, use_relay_subscription};
use crate::routes::Route;
use crate::stores::nostr_client::{self, fetch_events_aggregated, HAS_SIGNER};
use crate::stores::profiles;
use crate::stores::social::channel_store::{
    cache_channel, cache_channel_metadata, channel_creation_by_id_filter, channel_messages_filter,
    channel_messages_realtime_filter, channel_metadata_filter, decode_channel_id,
    get_cached_channel, get_channel_display_info, get_channel_relay_url, parse_channel_creation,
    parse_channel_metadata, send_channel_message, Channel,
};
use crate::utils::custom_emoji::EmojiSelection;
use crate::utils::profile_prefetch;
use crate::utils::truncate_pubkey;
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
use nostr_sdk::{Event, PublicKey, RelayUrl};
use std::time::Duration;
#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "web")]
#[wasm_bindgen(inline_js = r#"
export function scrollChannelChatToBottom(elementId) {
    const element = document.getElementById(elementId);
    if (element) {
        element.scrollTop = element.scrollHeight;
    }
}

export function isChannelChatScrolledNearBottom(elementId, threshold) {
    const element = document.getElementById(elementId);
    if (!element) return true;
    const scrollTop = element.scrollTop;
    const scrollHeight = element.scrollHeight;
    const clientHeight = element.clientHeight;
    return scrollHeight - scrollTop - clientHeight < threshold;
}
"#)]
extern "C" {
    fn scrollChannelChatToBottom(element_id: &str);
    fn isChannelChatScrolledNearBottom(element_id: &str, threshold: f64) -> bool;
}

/// Reusable NIP-28 channel chat component.
/// Takes a channel_id (hex event ID of the kind 40 channel creation event).
#[component]
pub fn ChannelChat(channel_id: String) -> Element {
    let mut messages = use_signal(Vec::<Event>::new);
    let mut loading = use_signal(|| false);
    let mut message_input = use_signal(String::new);
    let mut sending = use_signal(|| false);
    let mut expanded = use_signal(|| false);
    let mut channel_info: Signal<Option<Channel>> = use_signal(|| None);
    let mut relay_url_for_send: Signal<Option<RelayUrl>> = use_signal(|| None);
    let has_signer = use_memo(move || *HAS_SIGNER.read());

    let channel_id_for_effect = channel_id.clone();
    let channel_id_for_send = channel_id.clone();
    let channel_id_for_send2 = channel_id.clone();

    let chat_container_id = format!(
        "channel-chat-messages-{}",
        channel_id.chars().take(8).collect::<String>()
    );

    // Load channel info + historical messages
    use_effect(use_reactive(&channel_id_for_effect, move |cid| {
        messages.write().clear();
        channel_info.set(None);
        relay_url_for_send.set(None);
        loading.set(true);
        spawn(async move {
            let (event_id, relay_hints) = match decode_channel_id(&cid) {
                Ok(v) => v,
                Err(e) => {
                    log::error!("Failed to decode channel ID: {}", e);
                    loading.set(false);
                    return;
                }
            };

            let relay = get_channel_relay_url(&relay_hints).await;
            relay_url_for_send.set(Some(relay));

            if let Some(cached) = get_cached_channel(&event_id.to_hex()) {
                channel_info.set(Some(cached));
            } else {
                match fetch_events_aggregated(
                    channel_creation_by_id_filter(event_id),
                    Duration::from_secs(10),
                )
                .await
                {
                    Ok(events) => {
                        if let Some(event) = events.first() {
                            if let Some(ch) = parse_channel_creation(event) {
                                cache_channel(ch.clone());
                                channel_info.set(Some(ch));
                            }
                        }
                    }
                    Err(e) => log::error!("Failed to fetch channel info: {}", e),
                }
            }

            if let Some(ref ch) = *channel_info.peek() {
                match fetch_events_aggregated(
                    channel_metadata_filter(event_id),
                    Duration::from_secs(5),
                )
                .await
                {
                    Ok(events) => {
                        if let Some(event) = events.first() {
                            if let Some(meta) = parse_channel_metadata(event, &ch.pubkey) {
                                cache_channel_metadata(meta);
                            }
                        }
                    }
                    Err(e) => log::warn!("Failed to fetch channel metadata: {}", e),
                }
            }

            match fetch_events_aggregated(
                channel_messages_filter(event_id, 200),
                Duration::from_secs(10),
            )
            .await
            {
                Ok(events) => {
                    let mut sorted = events;
                    sorted.sort_by_key(|a| a.created_at);
                    messages.set(sorted);
                    log::info!("Loaded {} channel messages", messages.read().len());
                }
                Err(e) => log::error!("Failed to fetch channel messages: {}", e),
            }

            loading.set(false);
        });
    }));

    {
        let realtime_filter = decode_channel_id(&channel_id)
            .ok()
            .map(|(event_id, _)| channel_messages_realtime_filter(event_id));
        use_relay_subscription(realtime_filter, move |event: &nostr::Event| {
            let already_exists = messages.read().iter().any(|e| e.id == event.id);
            if !already_exists {
                log::info!("New channel message: {}", event.id.to_hex());
                let mut msgs = messages.write();
                let insert_at = msgs
                    .iter()
                    .position(|msg| msg.created_at > event.created_at)
                    .unwrap_or(msgs.len());
                msgs.insert(insert_at, event.clone());
                let len = msgs.len();
                if len > 200 {
                    msgs.drain(0..(len - 200));
                }
            }
        });
    }

    // Auto-scroll
    let chat_container_id_for_scroll = chat_container_id.clone();
    let mut is_first_load = use_signal(|| true);
    let mut channel_scroll_gen = use_signal(|| 0u32);
    use_effect(use_reactive(&channel_id, move |_| {
        is_first_load.set(true);
        channel_scroll_gen.with_mut(|gen| *gen = gen.wrapping_add(1));
    }));
    use_effect(move || {
        let msg_count = messages.read().len();
        let container_id = chat_container_id_for_scroll.clone();
        let scroll_gen = channel_scroll_gen.with_mut(|gen| {
            *gen = gen.wrapping_add(1);
            *gen
        });
        spawn(async move {
            crate::platform::timer::sleep_ms(50).await;
            #[cfg(feature = "web")]
            {
                let mut did_scroll = false;
                let should_scroll = msg_count > 0
                    && (*is_first_load.peek()
                        || isChannelChatScrolledNearBottom(&container_id, 100.0));
                if should_scroll {
                    scrollChannelChatToBottom(&container_id);
                    did_scroll = true;
                }
                if did_scroll && msg_count > 0 && *channel_scroll_gen.read() == scroll_gen {
                    is_first_load.set(false);
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let mut did_scroll = false;
                if msg_count > 0 {
                    let element_id =
                        serde_json::to_string(&container_id).unwrap_or_else(|_| "\"\"".to_string());
                    let near_bottom_script = format!(
                        "(() => {{ const el = document.getElementById({}); if (!el) return false; return (el.scrollHeight - (el.scrollTop + el.clientHeight)) <= 100; }})()",
                        element_id
                    );
                    let near_bottom = document::eval(&near_bottom_script)
                        .await
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if *is_first_load.peek() || near_bottom {
                        let scroll_script = format!(
                            "(() => {{ const el = document.getElementById({}); if (el) {{ el.scrollTop = el.scrollHeight; return true; }} return false; }})()",
                            element_id
                        );
                        did_scroll = document::eval(&scroll_script)
                            .await
                            .ok()
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                    }
                }
                if did_scroll && *channel_scroll_gen.read() == scroll_gen {
                    is_first_load.set(false);
                }
            }
        });
    });


    // Profile prefetch
    use_effect(move || {
        let msg_count = messages.read().len();
        spawn(async move {
            if msg_count == 0 {
                return;
            }
            let pubkeys: Vec<PublicKey> = messages.peek().iter().map(|e| e.pubkey).collect();
            profile_prefetch::prefetch_pubkeys(pubkeys).await;
        });
    });

    // Send message handler
    let perform_send = move |content: String, cid: String| {
        spawn(async move {
            let (event_id, relay_hints) = match decode_channel_id(&cid) {
                Ok(v) => v,
                Err(e) => {
                    log::error!("Failed to decode channel ID for send: {}", e);
                    sending.set(false);
                    return;
                }
            };

            let relay = match relay_url_for_send.peek().clone() {
                Some(r) => r,
                None => get_channel_relay_url(&relay_hints).await,
            };

            match send_channel_message(event_id, relay, &content).await {
                Ok(event) => {
                    message_input.set(String::new());
                    let already_exists = messages.read().iter().any(|e| e.id == event.id);
                    if !already_exists {
                        let mut msgs = messages.write();
                        msgs.push(event);
                        let len = msgs.len();
                        if len > 200 {
                            msgs.drain(0..(len - 200));
                        }
                    }
                }
                Err(e) => log::error!("Failed to send channel message: {}", e),
            }
            sending.set(false);
        });
    };

    // Channel display info
    let channel_name = use_memo(move || {
        channel_info
            .read()
            .as_ref()
            .map(|ch| {
                let (name, _, _) = get_channel_display_info(ch);
                name.unwrap_or_else(|| "Unnamed Channel".to_string())
            })
            .unwrap_or_else(|| "Channel".to_string())
    });

    // Escape key handler
    #[cfg(feature = "web")]
    let mut escape_cb = use_signal(|| None::<Closure<dyn FnMut(web_sys::KeyboardEvent)>>);
    #[cfg(feature = "web")]
    use_effect(move || {
        let Some(window) = web_sys::window() else {
            return;
        };
        let cb = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
            if e.key() == "Escape" && expanded() {
                expanded.set(false);
            }
        }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);
        window
            .add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())
            .ok();
        escape_cb.set(Some(cb));
    });
    #[cfg(feature = "web")]
    use_drop(move || {
        if let Some(cb) = escape_cb.peek().as_ref() {
            if let Some(window) = web_sys::window() {
                let _ = window
                    .remove_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref());
            }
        }
    });

    let cid_for_keydown = channel_id_for_send.clone();
    let cid_for_click = channel_id_for_send2.clone();

    rsx! {
        div {
            class: if expanded() {
                "fixed inset-0 z-50 flex flex-col bg-background"
            } else {
                "flex-1 min-h-0 flex flex-col bg-background"
            },
            // Header
            div { class: "shrink-0 px-4 py-3 border-b border-border flex items-center justify-between",
                h3 { class: "font-bold text-lg truncate", "{channel_name}" }
                button {
                    class: "lg:hidden p-2 hover:bg-accent rounded-lg transition",
                    aria_label: if expanded() { "Collapse chat" } else { "Expand chat" },
                    aria_expanded: expanded(),
                    onclick: move |_| expanded.toggle(),
                    if expanded() {
                        XIcon { class: "w-5 h-5".to_string() }
                    } else {
                        MaximizeIcon { class: "w-5 h-5".to_string() }
                    }
                }
            }
            // Messages
            div {
                id: "{chat_container_id}",
                class: "flex-1 overflow-y-auto p-4 space-y-3 hide-scrollbar",
                if *loading.read() {
                    div { class: "flex items-center justify-center h-full text-muted-foreground",
                        "Loading messages..."
                    }
                } else if messages.read().is_empty() {
                    div { class: "flex items-center justify-center h-full text-muted-foreground text-center",
                        div {
                            "No messages yet."
                            br {}
                            "Be the first to chat!"
                        }
                    }
                } else {
                    for message in messages.read().iter() {
                        ChannelChatMessage { key: "{message.id}", event: message.clone() }
                    }
                }
            }
            // Input
            if *has_signer.read() {
                div { class: "shrink-0 p-4 border-t border-border",
                    div { class: "flex items-center gap-2",
                        EmojiPicker {
                            icon_only: true,
                            on_emoji_selected: move |selection: EmojiSelection| {
                                let current = message_input.read().clone();
                                message_input.set(format!("{}{}", current, selection.insertion_text()));
                            },
                        }
                        input {
                            r#type: "text",
                            class: "flex-1 px-3 py-2 bg-input border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                            placeholder: "Send a message...",
                            value: "{message_input.read()}",
                            disabled: *sending.read(),
                            oninput: move |e| message_input.set(e.value().clone()),
                            onkeydown: move |e| {
                                if e.key() == Key::Enter && !e.modifiers().shift() {
                                    e.prevent_default();
                                    let content = message_input.read().clone();
                                    if content.trim().is_empty() || *sending.read() || !*has_signer.read() {
                                        return;
                                    }
                                    sending.set(true);
                                    perform_send(content, cid_for_keydown.clone());
                                }
                            },
                        }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90 font-medium rounded-lg transition disabled:opacity-50 disabled:cursor-not-allowed",
                            disabled: *sending.read() || message_input.read().trim().is_empty(),
                            onclick: move |_| {
                                let content = message_input.read().clone();
                                if content.trim().is_empty() || *sending.read() || !*has_signer.read() {
                                    return;
                                }
                                sending.set(true);
                                perform_send(content, cid_for_click.clone());
                            },
                            if *sending.read() {
                                "Sending..."
                            } else {
                                "Send"
                            }
                        }
                    }
                }
            } else {
                div { class: "p-4 border-t border-border text-center text-sm text-muted-foreground",
                    "Sign in to chat"
                }
            }
        }
    }
}

#[component]
fn ChannelChatMessage(event: Event) -> Element {
    let author_pubkey = event.pubkey.to_string();
    let (_cached_muted_posts, cached_blocked_users) = use_mute_block_cache();
    let mut is_author_blocked = use_signal(|| None::<bool>);
    let author_pubkey_check = author_pubkey.clone();

    {
        let cached_blocked_users_val = cached_blocked_users.read().clone();
        use_effect(use_reactive!(|(
            cached_blocked_users_val,
            author_pubkey_check,
        )| {
            let author_pubkey = author_pubkey_check.clone();
            if let Some(ref blocked_set) = cached_blocked_users_val {
                if let Ok(blocked) = nostr_client::is_user_blocked_cached(&author_pubkey, blocked_set) {
                    is_author_blocked.set(Some(blocked));
                }
            }
            if cached_blocked_users_val.is_none() {
                spawn(async move {
                    match nostr_client::is_user_blocked(author_pubkey).await {
                        Ok(blocked) => is_author_blocked.set(Some(blocked)),
                        Err(_) => is_author_blocked.set(Some(false)),
                    }
                });
            }
        }));
    }

    if is_author_blocked.read().unwrap_or(false) {
        return rsx! {};
    }

    let timestamp = event.created_at;
    let author_pk_for_metadata = author_pubkey.clone();
    let author_pk_for_name = author_pubkey.clone();
    let author_pk_for_display = author_pubkey.clone();

    let metadata = use_memo(move || profiles::get_profile(&author_pk_for_metadata));
    let author_name = use_memo(move || {
        if let Some(ref meta) = *metadata.read() {
            meta.display_name
                .clone()
                .or_else(|| meta.name.clone())
                .unwrap_or_else(|| truncate_pubkey(&author_pk_for_name))
        } else {
            truncate_pubkey(&author_pk_for_name)
        }
    });
    let author_picture = use_memo(move || metadata.read().as_ref().and_then(|m| m.picture.clone()));

    rsx! {
        div { class: "flex gap-3",
            Link {
                to: Route::Profile {
                    pubkey: author_pk_for_display.clone(),
                },
                class: "shrink-0",
                if let Some(pic_url) = author_picture.read().as_ref().filter(|u| is_valid_http_url(u)) {
                    img {
                        src: "{pic_url}",
                        class: "w-8 h-8 rounded-full object-cover",
                        alt: "Avatar",
                        loading: "lazy",
                    }
                } else {
                    div { class: "w-8 h-8 rounded-full bg-accent flex items-center justify-center text-accent-foreground text-xs font-bold",
                        {
                            let name = author_name.read();
                            let first_char = name.chars().next().unwrap_or('?').to_uppercase().to_string();
                            rsx! { "{first_char}" }
                        }
                    }
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "flex items-baseline gap-2",
                    Link {
                        to: Route::Profile {
                            pubkey: author_pk_for_display.clone(),
                        },
                        class: "font-semibold text-sm hover:underline truncate",
                        "{author_name.read()}"
                    }
                    span { class: "text-xs text-muted-foreground", "{timestamp.to_human_datetime()}" }
                }
                div { class: "text-sm mt-1",
                    RichContent {
                        content: event.content.clone(),
                        tags: event.tags.to_vec(),
                    }
                }
            }
        }
    }
}
