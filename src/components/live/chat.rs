use crate::components::icons::{MaximizeIcon, XIcon};
use crate::components::{EmojiPicker, RichContent};
use crate::routes::Route;
use crate::stores::nostr_client::{fetch_events_aggregated, get_client, HAS_SIGNER};
use crate::stores::profiles;
use crate::utils::profile_prefetch;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use dioxus_core::use_drop;
use nostr::TagKind;
use nostr_sdk::{
    Event, EventBuilder, Filter, Kind, PublicKey, RelayPoolNotification, SubscriptionId, Tag,
    Timestamp,
};
use std::time::Duration;
#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "web")]
#[wasm_bindgen(inline_js = r#"
export function scrollChatToBottom(elementId) {
    const element = document.getElementById(elementId);
    if (element) {
        element.scrollTop = element.scrollHeight;
    }
}

export function isScrolledNearBottom(elementId, threshold) {
    const element = document.getElementById(elementId);
    if (!element) return true;
    const scrollTop = element.scrollTop;
    const scrollHeight = element.scrollHeight;
    const clientHeight = element.clientHeight;
    return scrollHeight - scrollTop - clientHeight < threshold;
}
"#)]
extern "C" {
    fn scrollChatToBottom(element_id: &str);
    fn isScrolledNearBottom(element_id: &str, threshold: f64) -> bool;
}
#[component]
pub fn LiveChat(stream_author_pubkey: String, stream_d_tag: String) -> Element {
    let mut messages = use_signal(Vec::<Event>::new);
    let mut loading = use_signal(|| false);
    let mut message_input = use_signal(String::new);
    let mut sending = use_signal(|| false);
    let mut expanded = use_signal(|| false);
    let mut chat_sub_id: Signal<Option<SubscriptionId>> = use_signal(|| None);
    let mut request_gen = use_signal(|| 0u32);
    let has_signer = use_memo(move || *HAS_SIGNER.read());
    let chat_container_id = format!(
        "live-chat-messages-{}-{}",
        stream_author_pubkey, stream_d_tag
    );
    let a_tag = format!("30311:{}:{}", stream_author_pubkey, stream_d_tag);
    let a_tag_for_send_keydown = a_tag.clone();
    let a_tag_for_send_click = a_tag.clone();
    let chat_id_for_auto_scroll = chat_container_id.clone();
    use_effect(use_reactive(
        (&stream_author_pubkey, &stream_d_tag),
        move |(author, dtag)| {
            let tag = format!("30311:{}:{}", author, dtag);
            let current_gen = request_gen.peek().wrapping_add(1);
            request_gen.set(current_gen);
            messages.set(Vec::new());
            loading.set(true);
            spawn(async move {
                let previous_sub_id = chat_sub_id.read().clone();
                chat_sub_id.set(None);
                if let Some(sub_id) = previous_sub_id {
                    if let Some(client) = get_client() {
                        client.unsubscribe(&sub_id).await;
                    }
                }
                let parts: Vec<&str> = tag.split(':').collect();
                if parts.len() == 3 && PublicKey::parse(parts[1]).is_ok() {
                    let filter = Filter::new()
                        .kind(Kind::from(1311))
                        .custom_tag(
                            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::A),
                            tag.as_str(),
                        )
                        .limit(200);
                    match fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                        Ok(events) => {
                            if *request_gen.peek() != current_gen {
                                return;
                            }
                            let mut sorted_messages = events;
                            sorted_messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                            messages.set(sorted_messages);
                            log::info!("Loaded {} chat messages", messages.read().len());
                        }
                        Err(e) => {
                            log::error!("Failed to fetch chat messages: {}", e);
                        }
                    }
                }
                if *request_gen.peek() != current_gen {
                    return;
                }
                loading.set(false);

                // Set up real-time subscription for new messages
                if let Some(client) = get_client() {
                    let realtime_filter = Filter::new()
                        .kind(Kind::from(1311))
                        .custom_tag(
                            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::A),
                            tag.as_str(),
                        )
                        .since(Timestamp::now())
                        .limit(0);

                    match client.subscribe(realtime_filter, None).await {
                        Ok(output) => {
                            if *request_gen.peek() != current_gen {
                                client.unsubscribe(&output.val).await;
                                return;
                            }
                            let subscription_id = output.val;
                            chat_sub_id.set(Some(subscription_id.clone()));
                            log::debug!("Subscribed to live chat for {}", tag);

                            spawn(async move {
                                let mut notifications = client.notifications();
                                while let Ok(notification) = notifications.recv().await {
                                    if *request_gen.peek() != current_gen {
                                        break;
                                    }
                                    if let RelayPoolNotification::Event {
                                        subscription_id: recv_sub_id,
                                        event,
                                        ..
                                    } = notification
                                    {
                                        if recv_sub_id == subscription_id {
                                            if *request_gen.peek() != current_gen {
                                                break;
                                            }
                                            let already_exists =
                                                messages.read().iter().any(|e| e.id == event.id);
                                            if !already_exists {
                                                log::info!(
                                                    "New chat message via streaming: {}",
                                                    event.id.to_hex()
                                                );
                                                let mut msgs = messages.write();
                                                msgs.push((*event).clone());
                                                // Enforce 200 message limit
                                                let len = msgs.len();
                                                if len > 200 {
                                                    msgs.drain(0..(len - 200));
                                                }
                                            }
                                        }
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            log::error!("Failed to subscribe to live chat: {}", e);
                        }
                    }
                }
            });
        },
    ));
    let mut is_first_load = use_signal(|| true);
    use_effect(move || {
        let msg_count = messages.read().len();
        let container_id = chat_id_for_auto_scroll.clone();
        spawn(async move {
            crate::platform::timer::sleep_ms(50).await;
            #[cfg(feature = "web")]
            {
                if *is_first_load.peek() && msg_count > 0 {
                    scrollChatToBottom(&container_id);
                    is_first_load.set(false);
                } else if isScrolledNearBottom(&container_id, 100.0) {
                    scrollChatToBottom(&container_id);
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (&container_id, msg_count);
                is_first_load.set(false);
            }
        });
    });
    use_drop(move || {
        if let Some(sub_id) = chat_sub_id.peek().clone() {
            spawn(async move {
                if let Some(client) = get_client() {
                    client.unsubscribe(&sub_id).await;
                    log::debug!("Cleaned up live chat subscription");
                }
            });
        }
    });
    use_effect(move || {
        let msg_count = messages.read().len();
        spawn(async move {
            if msg_count == 0 {
                return;
            }
            let current_messages = messages.peek().clone();
            profile_prefetch::prefetch_event_authors(&current_messages).await;
        });
    });
    let perform_send = move |content: String, tag_clone: String| {
        spawn(async move {
            match get_client() {
                Some(client) => {
                    let tag = Tag::custom(TagKind::a(), vec![tag_clone.clone()]);
                    let builder = EventBuilder::new(Kind::from(1311), content.clone()).tag(tag);
                    // Sign first to get the full event
                    match client.sign_event_builder(builder).await {
                        Ok(event) => {
                            // Send the signed event
                            match client.send_event(&event).await {
                                Ok(output) => {
                                    log::info!("Chat message sent: {:?}", output.id());
                                    message_input.set(String::new());
                                    // Add to messages immediately (optimistic update)
                                    // nostr-sdk excludes self-published events from RelayPoolNotification::Event
                                    let already_exists =
                                        messages.read().iter().any(|e| e.id == event.id);
                                    if !already_exists {
                                        let mut msgs = messages.write();
                                        msgs.push(event);
                                        // Enforce 200 message limit
                                        let len = msgs.len();
                                        if len > 200 {
                                            msgs.drain(0..(len - 200));
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to send chat message: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to sign chat message: {}", e);
                        }
                    }
                }
                None => {
                    log::error!("Client not initialized");
                }
            }
            sending.set(false);
        });
    };
    // Escape key closes expanded chat overlay
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
    rsx! {
        div {
            class: if expanded() {
                "fixed inset-0 z-50 flex flex-col bg-background"
            } else {
                "flex-1 min-h-0 flex flex-col bg-background border-l border-border"
            },
            div { class: "shrink-0 px-4 py-3 border-b border-border flex items-center justify-between",
                h3 { class: "font-bold text-lg", "Live Chat" }
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
                        ChatMessage { key: "{message.id}", event: message.clone() }
                    }
                }
            }
            if *has_signer.read() {
                div { class: "shrink-0 p-4 border-t border-border",
                    div { class: "flex items-center gap-2",
                        EmojiPicker {
                            icon_only: true,
                            on_emoji_selected: move |emoji: String| {
                                let current = message_input.read().clone();
                                message_input.set(format!("{}{}", current, emoji));
                            },
                        }
                        input {
                            r#type: "text",
                            class: "flex-1 px-3 py-2 bg-input border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-blue-500",
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
                                    perform_send(content, a_tag_for_send_keydown.clone());
                                }
                            },
                        }
                        button {
                            class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white font-medium rounded-lg transition disabled:opacity-50 disabled:cursor-not-allowed",
                            disabled: *sending.read() || message_input.read().trim().is_empty(),
                            onclick: move |_| {
                                let content = message_input.read().clone();
                                if content.trim().is_empty() || *sending.read() || !*has_signer.read() {
                                    return;
                                }
                                sending.set(true);
                                perform_send(content, a_tag_for_send_click.clone());
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
fn ChatMessage(event: Event) -> Element {
    let author_pubkey = event.pubkey.to_string();
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
                if let Some(pic_url) = author_picture.read().as_ref() {
                    img {
                        src: "{pic_url}",
                        class: "w-8 h-8 rounded-full object-cover",
                        alt: "Avatar",
                        loading: "lazy",
                    }
                } else {
                    div { class: "w-8 h-8 rounded-full bg-blue-600 flex items-center justify-center text-white text-xs font-bold",
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
