use crate::components::icons::{MaximizeIcon, XIcon};
use crate::components::{EmojiPicker, RichContent};
use crate::hooks::{use_mute_block_cache, use_relay_subscription};
use crate::routes::Route;
use crate::stores::nostr_client::{self, fetch_events_aggregated, get_client, HAS_SIGNER};
use crate::stores::profiles;
use crate::utils::custom_emoji::{build_custom_emoji_tags, EmojiSelection};
use crate::utils::profile_prefetch;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr::TagKind;
use nostr_sdk::{
    Event, EventBuilder, Filter, Kind, PublicKey,
    SingleLetterTag, Alphabet,
    Tag, Timestamp,
};
use std::collections::HashSet;
use std::rc::Rc;
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
    let has_signer = use_memo(move || *HAS_SIGNER.read());
    let (_, cached_blocked_users) = use_mute_block_cache();
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
            messages.set(Vec::new());
            loading.set(true);
            spawn(async move {
                if PublicKey::parse(&author).is_ok() {
                    let filter = Filter::new()
                        .kind(Kind::from(1311))
                        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), tag.as_str())
                        .limit(200);
                    match fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                        Ok(events) => {
                            let mut sorted_messages = events;
                            sorted_messages.sort_by_key(|a| a.created_at);
                            messages.set(sorted_messages);
                            log::info!("Loaded {} chat messages", messages.read().len());
                        }
                        Err(e) => {
                            log::error!("Failed to fetch chat messages: {}", e);
                        }
                    }
                }
                loading.set(false);
            });
        },
    ));

    {
        let a_tag = format!("30311:{}:{}", stream_author_pubkey, stream_d_tag);
        let chat_filter = if PublicKey::parse(&stream_author_pubkey).is_ok() {
            Some(
                Filter::new()
                    .kind(Kind::from(1311))
                    .custom_tag(SingleLetterTag::lowercase(Alphabet::A), a_tag.as_str())
                    .since(Timestamp::now())
                    .limit(0),
            )
        } else {
            None
        };
        use_relay_subscription(chat_filter, move |event: &nostr::Event| {
            let already_exists = messages.read().iter().any(|e| e.id == event.id);
            if !already_exists {
                log::info!("New chat message via streaming: {}", event.id.to_hex());
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
    let mut is_first_load = use_signal(|| true);
    let mut chat_scroll_gen = use_signal(|| 0u32);
    use_effect(use_reactive(
        (&stream_author_pubkey, &stream_d_tag),
        move |_| {
            is_first_load.set(true);
            chat_scroll_gen.with_mut(|gen| *gen = gen.wrapping_add(1));
        },
    ));
    use_effect(move || {
        let msg_count = messages.read().len();
        let container_id = chat_id_for_auto_scroll.clone();
        let scroll_gen = chat_scroll_gen.with_mut(|gen| {
            *gen = gen.wrapping_add(1);
            *gen
        });
        spawn(async move {
            crate::platform::timer::sleep_ms(50).await;
            #[cfg(feature = "web")]
            {
                let mut did_scroll = false;
                let should_scroll = msg_count > 0
                    && (*is_first_load.peek() || isScrolledNearBottom(&container_id, 100.0));
                if should_scroll {
                    scrollChatToBottom(&container_id);
                    did_scroll = true;
                }
                if did_scroll && msg_count > 0 && *chat_scroll_gen.read() == scroll_gen {
                    is_first_load.set(false);
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (&container_id, msg_count);
                if msg_count > 0 && *chat_scroll_gen.read() == scroll_gen {
                    is_first_load.set(false);
                }
            }
        });
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
                Some(_client) => {
                    let tag = Tag::custom(TagKind::a(), vec![tag_clone.clone()]);
                    let builder = EventBuilder::new(Kind::from(1311), content.clone())
                        .tag(tag)
                        .tags(build_custom_emoji_tags(&content));
                    match crate::stores::publish_queue::signing::sign_event_builder(builder)
                        .await
                    {
                        Ok(event) => {
                            crate::stores::publish_queue::enqueue(
                                event.clone(),
                                crate::stores::publish_queue::types::QueueEventType::Other("live".to_string()),
                                None,
                                std::collections::HashMap::new(),
                            ).await;
                            message_input.set(String::new());
                            let already_exists =
                                messages.read().iter().any(|e| e.id == event.id);
                            if !already_exists {
                                let mut msgs = messages.write();
                                msgs.push(event);
                                let len = msgs.len();
                                if len > 200 {
                                    msgs.drain(0..(len - 200));
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
                        ChatMessage {
                            key: "{message.id}",
                            event: message.clone(),
                            cached_blocked_users: cached_blocked_users.read().clone(),
                        }
                    }
                }
            }
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
fn ChatMessage(
    event: Event,
    #[props(default = None)] cached_blocked_users: Option<Rc<HashSet<String>>>,
) -> Element {
    let author_pubkey = event.pubkey.to_string();
    let mut is_author_blocked = use_signal(|| None::<bool>);
    let mut show_hidden_anyway = use_signal(|| false);
    let author_pubkey_check = author_pubkey.clone();

    use_effect(use_reactive!(|(
        cached_blocked_users,
        author_pubkey_check,
    )| {
        let author_pubkey = author_pubkey_check.clone();
        if let Some(ref blocked_set) = cached_blocked_users {
            if let Ok(blocked) = nostr_client::is_user_blocked_cached(&author_pubkey, blocked_set) {
                is_author_blocked.set(Some(blocked));
            }
        }
        if cached_blocked_users.is_none() {
            spawn(async move {
                match nostr_client::is_user_blocked(author_pubkey).await {
                    Ok(blocked) => is_author_blocked.set(Some(blocked)),
                    Err(_) => is_author_blocked.set(Some(false)),
                }
            });
        }
    }));

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

    let is_hidden = is_author_blocked.read().unwrap_or(false) && !*show_hidden_anyway.read();

    rsx! {
        if is_hidden {
            div { class: "flex items-center gap-3 py-2",
                div { class: "flex-1 text-muted-foreground text-sm",
                    "Message from blocked user"
                }
                button {
                    class: "px-3 py-1 text-sm text-primary hover:underline",
                    onclick: move |_| {
                        show_hidden_anyway.set(true);
                    },
                    "Show anyway"
                }
            }
        } else {
            div { class: "flex gap-3",
                Link {
                    to: Route::Profile {
                        pubkey: crate::utils::nip19_urls::profile_route_id(&author_pk_for_display),
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
                                pubkey: crate::utils::nip19_urls::profile_route_id(&author_pk_for_display),
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
}
