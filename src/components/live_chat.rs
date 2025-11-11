use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, PublicKey, FromBech32, EventBuilder, Tag, JsonUtil};
use nostr::{TagKind};
use crate::stores::nostr_client::{get_client, fetch_events_aggregated, HAS_SIGNER};
use crate::routes::Route;
use std::time::Duration;
use wasm_bindgen::prelude::*;

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
pub fn LiveChat(
    stream_author_pubkey: String,
    stream_d_tag: String,
) -> Element {
    let mut messages = use_signal(|| Vec::<Event>::new());
    let mut loading = use_signal(|| false);
    let mut message_input = use_signal(|| String::new());
    let mut sending = use_signal(|| false);
    // Make has_signer reactive - read from the store when needed instead of capturing once
    let has_signer = use_memo(move || *HAS_SIGNER.read());

    // Create unique ID for this chat container
    let chat_container_id = use_signal(|| {
        let timestamp = js_sys::Date::now() as u64;
        format!("live-chat-messages-{}", timestamp)
    });

    // Create the 'a' tag for this livestream
    let a_tag = format!("30311:{}:{}", stream_author_pubkey, stream_d_tag);
    let a_tag_for_fetch = a_tag.clone();
    let a_tag_for_send = a_tag.clone();
    let a_tag_for_send_keydown = a_tag_for_send.clone();
    let a_tag_for_send_onclick = a_tag_for_send.clone();

    // Fetch chat messages
    use_effect(use_reactive(&a_tag_for_fetch, move |tag| {
        spawn(async move {
            loading.set(true);

            // Parse the 'a' tag to create proper filter
            let parts: Vec<&str> = tag.split(':').collect();
            if parts.len() == 3 {
                let _kind_num = parts[0].parse::<u16>().unwrap_or(30311);
                if let Ok(_pubkey) = PublicKey::parse(parts[1]) {
                    let _identifier = parts[2];

                    // Fetch Kind 1311 chat messages that reference this livestream
                    let filter = Filter::new()
                        .kind(Kind::from(1311))
                        .custom_tag(
                            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::A),
                            tag.as_str()
                        )
                        .limit(200);

                    match fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                        Ok(events) => {
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
            }

            loading.set(false);
        });
    }));

    // Auto-refresh messages every 5 seconds with cancellable polling
    // Track the spawned polling task so we can cancel it when needed
    let mut poll_task = use_signal(|| None::<Task>);

    use_effect(use_reactive(&a_tag_for_fetch, move |tag| {
        // Cancel the previous polling task if it exists
        if let Some(task) = poll_task.read().as_ref() {
            task.cancel();
        }

        // Start new polling loop and store its handle
        let new_task = spawn(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(5000).await;

                let parts: Vec<&str> = tag.split(':').collect();
                if parts.len() == 3 {
                    let _kind_num = parts[0].parse::<u16>().unwrap_or(30311);
                    if let Ok(_pubkey) = PublicKey::parse(parts[1]) {
                        let _identifier = parts[2];

                        let filter = Filter::new()
                            .kind(Kind::from(1311))
                            .custom_tag(
                                nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::A),
                                tag.as_str()
                            )
                            .limit(200);

                        if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                            let mut sorted_messages = events;
                            sorted_messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                            messages.set(sorted_messages);
                        }
                    }
                }
            }
        });

        poll_task.set(Some(new_task));
    }));

    // Scroll to bottom on initial load
    use_effect(use_reactive(&*chat_container_id.read(), move |container_id| {
        spawn(async move {
            // Wait for DOM to render
            gloo_timers::future::TimeoutFuture::new(100).await;
            scrollChatToBottom(&container_id);
        });
    }));

    // Auto-scroll to bottom when messages change (if user is already near bottom)
    use_effect(use_reactive(&messages.read().len(), move |_| {
        let container_id = chat_container_id.read().clone();
        // Small delay to ensure DOM has updated with new messages
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(50).await;
            // Only auto-scroll if user is already near the bottom (within 150px)
            if isScrolledNearBottom(&container_id, 150.0) {
                scrollChatToBottom(&container_id);
            }
        });
    }));

    // Stop polling on unmount by canceling the task
    use_drop(move || {
        if let Some(task) = poll_task.read().as_ref() {
            task.cancel();
        }
    });


    rsx! {
        div {
            class: "h-full flex flex-col bg-background border-l border-border",

            // Chat header
            div {
                class: "px-4 py-3 border-b border-border",
                h3 {
                    class: "font-bold text-lg",
                    "Live Chat"
                }
            }

            // Messages container
            div {
                id: "{chat_container_id.read()}",
                class: "flex-1 overflow-y-auto p-4 space-y-3",
                if *loading.read() {
                    div {
                        class: "flex items-center justify-center h-full text-muted-foreground",
                        "Loading messages..."
                    }
                } else if messages.read().is_empty() {
                    div {
                        class: "flex items-center justify-center h-full text-muted-foreground text-center",
                        div {
                            "No messages yet."
                            br {}
                            "Be the first to chat!"
                        }
                    }
                } else {
                    for message in messages.read().iter() {
                        ChatMessage { event: message.clone() }
                    }
                }
            }

            // Message input
            if *has_signer.read() {
                div {
                    class: "p-4 border-t border-border",
                    div {
                        class: "flex gap-2",
                        input {
                            r#type: "text",
                            class: "flex-1 px-3 py-2 bg-input border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
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
                                    let tag_clone = a_tag_for_send_keydown.clone();
                                    sending.set(true);
                                    spawn(async move {
                                        match get_client() {
                                            Some(client) => {
                                                let tag = Tag::custom(TagKind::a(), vec![tag_clone.clone()]);
                                                let builder = EventBuilder::new(Kind::from(1311), content.clone()).tag(tag);
                                                match client.send_event_builder(builder).await {
                                                    Ok(event_id) => {
                                                        log::info!("Chat message sent: {:?}", event_id);
                                                        message_input.set(String::new());
                                                        gloo_timers::future::TimeoutFuture::new(1000).await;
                                                        let parts: Vec<&str> = tag_clone.split(':').collect();
                                                        if parts.len() == 3 {
                                                            if let Ok(_pubkey) = PublicKey::parse(parts[1]) {
                                                                let filter = Filter::new()
                                                                    .kind(Kind::from(1311))
                                                                    .custom_tag(
                                                                        nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::A),
                                                                        tag_clone.as_str()
                                                                    )
                                                                    .limit(200);
                                                                if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                                                                    let mut sorted_messages = events;
                                                                    sorted_messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                                                                    messages.set(sorted_messages);
                                                                }
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        log::error!("Failed to send chat message: {}", e);
                                                    }
                                                }
                                            }
                                            None => {
                                                log::error!("Client not initialized");
                                            }
                                        }
                                        sending.set(false);
                                    });
                                }
                            }
                        }
                        button {
                            class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white font-medium rounded-lg transition disabled:opacity-50 disabled:cursor-not-allowed",
                            disabled: *sending.read() || message_input.read().trim().is_empty(),
                            onclick: move |_| {
                                let content = message_input.read().clone();
                                if content.trim().is_empty() || *sending.read() || !*has_signer.read() {
                                    return;
                                }
                                let tag_clone = a_tag_for_send_onclick.clone();
                                sending.set(true);
                                spawn(async move {
                                    match get_client() {
                                        Some(client) => {
                                            let tag = Tag::custom(TagKind::a(), vec![tag_clone.clone()]);
                                            let builder = EventBuilder::new(Kind::from(1311), content.clone()).tag(tag);
                                            match client.send_event_builder(builder).await {
                                                Ok(event_id) => {
                                                    log::info!("Chat message sent: {:?}", event_id);
                                                    message_input.set(String::new());
                                                    gloo_timers::future::TimeoutFuture::new(1000).await;
                                                    let parts: Vec<&str> = tag_clone.split(':').collect();
                                                    if parts.len() == 3 {
                                                        if let Ok(_pubkey) = PublicKey::parse(parts[1]) {
                                                            let filter = Filter::new()
                                                                .kind(Kind::from(1311))
                                                                .custom_tag(
                                                                    nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::A),
                                                                    tag_clone.as_str()
                                                                )
                                                                .limit(200);
                                                            if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                                                                let mut sorted_messages = events;
                                                                sorted_messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                                                                messages.set(sorted_messages);
                                                            }
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    log::error!("Failed to send chat message: {}", e);
                                                }
                                            }
                                        }
                                        None => {
                                            log::error!("Client not initialized");
                                        }
                                    }
                                    sending.set(false);
                                });
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
                div {
                    class: "p-4 border-t border-border text-center text-sm text-muted-foreground",
                    "Sign in to chat"
                }
            }
        }
    }
}

#[component]
fn ChatMessage(event: Event) -> Element {
    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_for_fetch = author_pubkey.clone();
    let content = event.content.clone();
    let timestamp = event.created_at;

    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // Fetch author metadata
    use_effect(use_reactive(&author_pubkey_for_fetch, move |pk| {
        spawn(async move {
            if let Ok(pubkey) = PublicKey::from_bech32(&pk).or_else(|_| PublicKey::parse(&pk)) {
                if let Some(client) = get_client() {
                    let filter = Filter::new()
                        .kind(Kind::Metadata)
                        .author(pubkey)
                        .limit(1);

                    match client.fetch_events(filter, Duration::from_secs(5)).await {
                        Ok(events) => {
                            if let Some(event) = events.first() {
                                if let Ok(metadata) = nostr_sdk::Metadata::from_json(&event.content) {
                                    author_metadata.set(Some(metadata));
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to fetch author metadata: {}", e);
                        }
                    }
                }
            }
        });
    }));

    let author_name = if let Some(ref metadata) = *author_metadata.read() {
        metadata.display_name.clone()
            .or_else(|| metadata.name.clone())
            .unwrap_or_else(|| format!("{}...", &author_pubkey[..8]))
    } else {
        format!("{}...", &author_pubkey[..8])
    };

    let author_picture = author_metadata.read().as_ref()
        .and_then(|m| m.picture.clone());

    rsx! {
        div {
            class: "flex gap-3",
            Link {
                to: Route::Profile { pubkey: author_pubkey.clone() },
                class: "flex-shrink-0",
                if let Some(pic_url) = author_picture {
                    img {
                        src: "{pic_url}",
                        class: "w-8 h-8 rounded-full object-cover",
                        alt: "Avatar",
                        loading: "lazy"
                    }
                } else {
                    div {
                        class: "w-8 h-8 rounded-full bg-blue-600 flex items-center justify-center text-white text-xs font-bold",
                        "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                    }
                }
            }
            div {
                class: "flex-1 min-w-0",
                div {
                    class: "flex items-baseline gap-2",
                    Link {
                        to: Route::Profile { pubkey: author_pubkey.clone() },
                        class: "font-semibold text-sm hover:underline truncate",
                        "{author_name}"
                    }
                    span {
                        class: "text-xs text-muted-foreground",
                        "{timestamp.to_human_datetime()}"
                    }
                }
                p {
                    class: "text-sm whitespace-pre-wrap break-words mt-1",
                    "{content}"
                }
            }
        }
    }
}
