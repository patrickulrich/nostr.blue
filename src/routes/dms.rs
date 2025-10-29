use dioxus::prelude::*;
use crate::stores::{auth_store, dms, profiles};
use crate::routes::Route;
use crate::utils::time;
use nostr_sdk::Event as NostrEvent;
use wasm_bindgen::JsCast;

#[component]
pub fn DMs() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut loading = use_signal(|| false);
    let mut refreshing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut selected_conversation = use_signal(|| None::<String>);
    let mut new_dm_mode = use_signal(|| false);
    let _new_recipient = use_signal(|| String::new());

    // Load DMs on mount
    use_effect(move || {
        if !auth_store::is_authenticated() {
            return;
        }

        loading.set(true);
        error.set(None);

        spawn(async move {
            match dms::init_dms().await {
                Ok(_) => {
                    log::info!("DMs loaded successfully");
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    // Auto-refresh polling (60 seconds)
    use_effect(move || {
        if !auth_store::is_authenticated() {
            return;
        }

        spawn(async move {
            loop {
                gloo_timers::future::sleep(std::time::Duration::from_secs(60)).await;

                // Only refresh if user is authenticated
                if auth_store::is_authenticated() {
                    log::debug!("Auto-refreshing DMs...");
                    let _ = dms::init_dms().await;
                }
            }
        });
    });

    // Manual refresh function
    let refresh_dms = move |_| {
        if refreshing.read().clone() {
            return;
        }

        refreshing.set(true);
        spawn(async move {
            match dms::init_dms().await {
                Ok(_) => {
                    log::info!("DMs refreshed successfully");
                }
                Err(e) => {
                    log::error!("Failed to refresh DMs: {}", e);
                }
            }
            refreshing.set(false);
        });
    };

    rsx! {
        div {
            class: "h-screen flex flex-col overflow-hidden",

            // Header
            div {
                class: "flex-shrink-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center justify-between",
                    h2 {
                        class: "text-xl font-bold",
                        "✉️ Direct Messages"
                    }
                    div {
                        class: "flex items-center gap-2",
                        // Refresh button
                        button {
                            class: "px-3 py-1 border border-border hover:bg-accent rounded-lg text-sm transition disabled:opacity-50",
                            disabled: *refreshing.read(),
                            onclick: refresh_dms,
                            if *refreshing.read() {
                                "↻ Refreshing..."
                            } else {
                                "↻ Refresh"
                            }
                        }
                        // New DM button
                        button {
                            class: "px-3 py-1 bg-blue-500 hover:bg-blue-600 text-white rounded-lg text-sm transition",
                            onclick: move |_| {
                                new_dm_mode.set(true);
                                selected_conversation.set(None);
                            },
                            "+ New DM"
                        }
                    }
                }
            }

            // Not authenticated
            if !auth.is_authenticated {
                div {
                    class: "text-center py-12",
                    div {
                        class: "text-6xl mb-4",
                        "🔐"
                    }
                    h3 {
                        class: "text-xl font-semibold mb-2",
                        "Sign in to view messages"
                    }
                    p {
                        class: "text-muted-foreground",
                        "Connect your account to send and receive encrypted messages"
                    }
                }
            } else {
                // Error state
                if let Some(err) = error.read().as_ref() {
                    div {
                        class: "p-4",
                        div {
                            class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                            "❌ {err}"
                        }
                    }
                }

                // Loading state
                if *loading.read() {
                    div {
                        class: "flex items-center justify-center p-12",
                        div {
                            class: "text-center",
                            div {
                                class: "animate-spin text-4xl mb-3",
                                "✉️"
                            }
                            p {
                                class: "text-muted-foreground",
                                "Loading messages..."
                            }
                        }
                    }
                }

                // Main DM interface
                if !*loading.read() {
                    div {
                        class: "flex-1 flex overflow-hidden h-full",

                        // Conversations list (left sidebar)
                        div {
                            class: "w-full sm:w-80 border-r border-border overflow-y-auto flex-shrink-0 hide-scrollbar",
                            {
                                let conversations = dms::get_conversations_sorted();
                                if conversations.is_empty() && !*new_dm_mode.read() {
                                    rsx! {
                                        div {
                                            class: "text-center py-12 px-4",
                                            div {
                                                class: "text-6xl mb-4",
                                                "📭"
                                            }
                                            h3 {
                                                class: "text-lg font-semibold mb-2",
                                                "No messages yet"
                                            }
                                            p {
                                                class: "text-sm text-muted-foreground",
                                                "Start a conversation by clicking '+ New DM'"
                                            }
                                        }
                                    }
                                } else {
                                    rsx! {
                                        div {
                                            class: "divide-y divide-border",
                                            for conversation in conversations {
                                                {
                                                    let conv_pubkey = conversation.pubkey.clone();
                                                    rsx! {
                                                        ConversationListItem {
                                                            key: "{conv_pubkey}",
                                                            conversation: conversation.clone(),
                                                            selected: selected_conversation.read().as_ref() == Some(&conversation.pubkey),
                                                            on_select: move |pk: String| {
                                                                log::info!("Selected conversation: {}", pk);
                                                                selected_conversation.set(Some(pk));
                                                                new_dm_mode.set(false);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Message view (right side)
                        div {
                            class: "flex-1 flex flex-col overflow-hidden",

                            if *new_dm_mode.read() {
                                // New DM composer
                                NewDMComposer {
                                    on_cancel: move |_| new_dm_mode.set(false),
                                    on_send: move |recipient: String| {
                                        selected_conversation.set(Some(recipient));
                                        new_dm_mode.set(false);
                                    }
                                }
                            } else if let Some(pubkey) = selected_conversation.read().as_ref() {
                                // Show selected conversation
                                // Use key to force re-render when conversation changes
                                ConversationView {
                                    key: "{pubkey}",
                                    pubkey: pubkey.clone()
                                }
                            } else {
                                // Empty state
                                div {
                                    class: "flex-1 flex items-center justify-center",
                                    div {
                                        class: "text-center text-muted-foreground",
                                        div {
                                            class: "text-6xl mb-4",
                                            "💬"
                                        }
                                        p {
                                            "Select a conversation to start messaging"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ConversationListItem(
    conversation: dms::Conversation,
    selected: bool,
    on_select: EventHandler<String>
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let mut decrypted_preview = use_signal(|| "Loading...".to_string());
    let pubkey = conversation.pubkey.clone();

    // Fetch profile on mount
    use_effect(move || {
        let pk = pubkey.clone();
        spawn(async move {
            match profiles::fetch_profile(pk).await {
                Ok(p) => profile.set(Some(p)),
                Err(e) => log::error!("Failed to fetch profile: {}", e),
            }
        });
    });

    // Decrypt the last message for preview
    let last_msg = conversation.messages.last().cloned();
    use_effect(move || {
        if let Some(msg) = &last_msg {
            let msg_clone = msg.clone();
            spawn(async move {
                match dms::decrypt_dm(&msg_clone).await {
                    Ok(content) => {
                        // UTF-8 safe truncation using characters instead of bytes
                        let preview = if content.chars().count() > 50 {
                            let truncated: String = content.chars().take(50).collect();
                            format!("{}...", truncated)
                        } else {
                            content
                        };
                        decrypted_preview.set(preview);
                    }
                    Err(_) => {
                        decrypted_preview.set("[Unable to decrypt]".to_string());
                    }
                }
            });
        } else {
            decrypted_preview.set("No messages".to_string());
        }
    });

    let preview = decrypted_preview.read().clone();

    let display_name = profile.read().as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| format!("{}...{}",
            &conversation.pubkey[..8],
            &conversation.pubkey[conversation.pubkey.len()-8..]));

    let avatar_url = profile.read().as_ref()
        .map(|p| p.get_avatar_url())
        .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/identicon/svg?seed={}", conversation.pubkey));

    let time_ago = conversation.messages.last()
        .map(|m| time::format_relative_time(m.created_at))
        .unwrap_or_else(|| "".to_string());

    let bg_class = if selected {
        "bg-accent"
    } else {
        "hover:bg-accent/50"
    };

    rsx! {
        div {
            class: "p-4 cursor-pointer transition {bg_class}",
            onclick: move |_| on_select.call(conversation.pubkey.clone()),

            div {
                class: "flex items-center gap-3",
                // Avatar
                img {
                    src: "{avatar_url}",
                    alt: "{display_name}",
                    class: "w-12 h-12 rounded-full object-cover flex-shrink-0",
                }

                div {
                    class: "flex-1 min-w-0",
                    // Display name and time
                    div {
                        class: "flex items-center justify-between gap-2 mb-1",
                        p {
                            class: "font-semibold text-sm truncate",
                            "{display_name}"
                        }
                        if !time_ago.is_empty() {
                            span {
                                class: "text-xs text-muted-foreground flex-shrink-0",
                                "{time_ago}"
                            }
                        }
                    }
                    // Preview
                    p {
                        class: "text-sm text-muted-foreground truncate",
                        "{preview}"
                    }
                }

                // Unread indicator
                if conversation.unread_count > 0 {
                    div {
                        class: "w-6 h-6 bg-blue-500 rounded-full flex items-center justify-center text-white text-xs font-bold flex-shrink-0",
                        "{conversation.unread_count}"
                    }
                }
            }
        }
    }
}

#[component]
fn ConversationView(pubkey: String) -> Element {
    let mut message_input = use_signal(|| String::new());
    let mut sending = use_signal(|| false);
    let mut decrypted_messages = use_signal(|| Vec::<(NostrEvent, String)>::new());
    let mut decrypt_loading = use_signal(|| true);
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let messages_container_id = use_signal(|| format!("messages-{}", uuid::Uuid::new_v4()));

    // Clone pubkey for different uses
    let pubkey_for_effect = pubkey.clone();
    let pubkey_for_send = pubkey.clone();
    let pubkey_for_input = pubkey.clone();
    let pubkey_for_display = pubkey.clone();
    let pubkey_for_profile = pubkey.clone();

    // Fetch profile on mount
    use_effect(move || {
        let pk = pubkey_for_profile.clone();
        spawn(async move {
            match profiles::fetch_profile(pk).await {
                Ok(p) => profile.set(Some(p)),
                Err(e) => log::error!("Failed to fetch profile: {}", e),
            }
        });
    });

    // Decrypt messages when conversation loads
    use_effect(move || {
        let pk = pubkey_for_effect.clone();
        decrypt_loading.set(true);
        decrypted_messages.set(Vec::new()); // Clear previous messages

        spawn(async move {
            log::info!("Loading conversation for: {}", pk);

            if let Some(conversation) = dms::get_conversation(&pk) {
                log::info!("Found {} messages in conversation", conversation.messages.len());
                let mut decrypted = Vec::new();

                for event in conversation.messages {
                    match dms::decrypt_dm(&event).await {
                        Ok(content) => {
                            log::debug!("Decrypted message: {}", &content[..content.len().min(50)]);
                            decrypted.push((event, content));
                        }
                        Err(e) => {
                            log::error!("Failed to decrypt message: {}", e);
                            decrypted.push((event, "[Failed to decrypt]".to_string()));
                        }
                    }
                }

                log::info!("Decrypted {} messages", decrypted.len());
                decrypted_messages.set(decrypted);
            } else {
                log::warn!("No conversation found for: {}", pk);
            }
            decrypt_loading.set(false);
        });
    });

    // Auto-scroll to bottom when messages load or conversation changes
    use_effect(move || {
        let container_id = messages_container_id.read().clone();
        let loading = *decrypt_loading.read();

        if !loading {
            spawn(async move {
                // Small delay to let DOM update
                gloo_timers::future::sleep(std::time::Duration::from_millis(100)).await;

                // Scroll to bottom using JavaScript
                let window = web_sys::window().expect("no global window");
                let document = window.document().expect("should have document");

                if let Some(element) = document.get_element_by_id(&container_id) {
                    if let Some(html_element) = element.dyn_ref::<web_sys::HtmlElement>() {
                        html_element.set_scroll_top(html_element.scroll_height());
                    }
                }
            });
        }
    });

    let send_message = move |_| {
        let content = message_input.read().clone();
        if content.trim().is_empty() {
            return;
        }

        sending.set(true);
        let recipient = pubkey_for_send.clone();

        spawn(async move {
            match dms::send_dm(recipient, content).await {
                Ok(_) => {
                    message_input.set(String::new());
                    log::info!("Message sent successfully");
                }
                Err(e) => {
                    log::error!("Failed to send message: {}", e);
                }
            }
            sending.set(false);
        });
    };

    let display_name = profile.read().as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| format!("{}...{}",
            &pubkey_for_display[..8],
            &pubkey_for_display[pubkey_for_display.len()-8..]));

    let avatar_url = profile.read().as_ref()
        .map(|p| p.get_avatar_url())
        .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/identicon/svg?seed={}", pubkey_for_display));

    let nip05 = profile.read().as_ref()
        .and_then(|p| p.nip05.clone());

    let container_id = messages_container_id.read().clone();

    rsx! {
        div {
            class: "flex-1 flex flex-col overflow-hidden h-full",

            // Conversation header
            div {
                class: "flex-shrink-0 p-4 border-b border-border flex items-center gap-3",
                img {
                    src: "{avatar_url}",
                    alt: "{display_name}",
                    class: "w-10 h-10 rounded-full object-cover flex-shrink-0",
                }
                div {
                    class: "flex-1 min-w-0",
                    h3 {
                        class: "font-semibold truncate",
                        "{display_name}"
                    }
                    if let Some(nip05_id) = nip05 {
                        p {
                            class: "text-xs text-muted-foreground truncate",
                            "{nip05_id}"
                        }
                    }
                    Link {
                        to: Route::Profile { pubkey: pubkey },
                        class: "text-xs text-blue-500 hover:underline",
                        "View profile"
                    }
                }
            }

            // Messages area
            div {
                id: "{container_id}",
                class: "flex-1 overflow-y-auto p-4 space-y-4",

                if *decrypt_loading.read() {
                    div {
                        class: "flex items-center justify-center p-8",
                        p {
                            class: "text-muted-foreground",
                            "Decrypting messages..."
                        }
                    }
                } else if decrypted_messages.read().is_empty() {
                    div {
                        class: "flex items-center justify-center p-8",
                        p {
                            class: "text-muted-foreground text-center",
                            "No messages yet. Start the conversation!"
                        }
                    }
                } else {
                    for (event, content) in decrypted_messages.read().iter() {
                        {
                            let my_pubkey = auth_store::get_pubkey().unwrap_or_default();
                            let is_mine = event.pubkey.to_string() == my_pubkey;
                            let sender_pubkey = event.pubkey.to_string();

                            rsx! {
                                MessageBubble {
                                    content: content.clone(),
                                    is_mine: is_mine,
                                    timestamp: event.created_at,
                                    sender_pubkey: sender_pubkey
                                }
                            }
                        }
                    }
                }
            }

            // Message input
            div {
                class: "flex-shrink-0 p-4 border-t border-border",
                div {
                    class: "flex gap-2",
                    input {
                        r#type: "text",
                        class: "flex-1 px-4 py-2 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-blue-500",
                        placeholder: "Type a message...",
                        value: "{message_input.read()}",
                        oninput: move |evt| message_input.set(evt.value().clone()),
                        onkeydown: move |evt| {
                            if evt.key() == Key::Enter && !evt.modifiers().shift() {
                                // Clone necessary values
                                let content = message_input.read().clone();
                                if content.trim().is_empty() {
                                    return;
                                }

                                sending.set(true);
                                let recipient = pubkey_for_input.clone();

                                spawn(async move {
                                    match dms::send_dm(recipient, content).await {
                                        Ok(_) => {
                                            message_input.set(String::new());
                                            log::info!("Message sent successfully");
                                        }
                                        Err(e) => {
                                            log::error!("Failed to send message: {}", e);
                                        }
                                    }
                                    sending.set(false);
                                });
                            }
                        }
                    }
                    button {
                        class: "px-6 py-2 bg-blue-500 hover:bg-blue-600 disabled:bg-gray-400 text-white rounded-lg font-medium transition",
                        disabled: *sending.read() || message_input.read().trim().is_empty(),
                        onclick: send_message,
                        if *sending.read() {
                            "Sending..."
                        } else {
                            "Send"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MessageBubble(
    content: String,
    is_mine: bool,
    timestamp: nostr_sdk::Timestamp,
    sender_pubkey: String
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let sender_pk = sender_pubkey.clone();
    let sender_pk_for_avatar = sender_pubkey.clone();

    // Fetch profile for sender (only if not mine)
    use_effect(move || {
        if !is_mine {
            let pk = sender_pk.clone();
            spawn(async move {
                match profiles::fetch_profile(pk).await {
                    Ok(p) => profile.set(Some(p)),
                    Err(e) => log::error!("Failed to fetch profile: {}", e),
                }
            });
        }
    });

    let avatar_url = if is_mine {
        if let Some(my_pubkey) = auth_store::get_pubkey() {
            profiles::get_cached_profile(&my_pubkey)
                .map(|p| p.get_avatar_url())
                .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/identicon/svg?seed={}", my_pubkey))
        } else {
            String::new()
        }
    } else {
        profile.read().as_ref()
            .map(|p| p.get_avatar_url())
            .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/identicon/svg?seed={}", sender_pk_for_avatar))
    };

    let time_ago = time::format_relative_time(timestamp);
    let alignment = if is_mine { "flex-row-reverse" } else { "flex-row" };
    let bg_color = if is_mine { "bg-blue-500 text-white" } else { "bg-accent" };
    let items_align = if is_mine { "items-end" } else { "items-start" };

    rsx! {
        div {
            class: "flex gap-3 mb-4 {alignment}",
            // Avatar
            img {
                src: "{avatar_url}",
                alt: "Avatar",
                class: "w-8 h-8 rounded-full object-cover flex-shrink-0",
            }
            // Message bubble and timestamp
            div {
                class: "flex flex-col gap-1 max-w-[70%] {items_align}",
                div {
                    class: "{bg_color} rounded-2xl px-4 py-2 break-words",
                    p {
                        class: "text-sm whitespace-pre-wrap",
                        "{content}"
                    }
                }
                span {
                    class: "text-xs text-muted-foreground px-2",
                    "{time_ago}"
                }
            }
        }
    }
}

#[component]
fn NewDMComposer(
    on_cancel: EventHandler<()>,
    on_send: EventHandler<String>
) -> Element {
    let mut recipient_input = use_signal(|| String::new());
    let mut message_input = use_signal(|| String::new());
    let mut sending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let send_message = move |_| {
        let recipient = recipient_input.read().clone();
        let content = message_input.read().clone();

        if recipient.trim().is_empty() || content.trim().is_empty() {
            error.set(Some("Please enter both recipient and message".to_string()));
            return;
        }

        sending.set(true);
        error.set(None);

        spawn(async move {
            match dms::send_dm(recipient.clone(), content).await {
                Ok(_) => {
                    on_send.call(recipient);
                }
                Err(e) => {
                    error.set(Some(e));
                    sending.set(false);
                }
            }
        });
    };

    rsx! {
        div {
            class: "flex-1 flex flex-col p-4",

            div {
                class: "mb-4",
                h3 {
                    class: "text-lg font-semibold mb-2",
                    "New Direct Message"
                }
                p {
                    class: "text-sm text-muted-foreground",
                    "Enter the recipient's public key (npub or hex)"
                }
            }

            if let Some(err) = error.read().as_ref() {
                div {
                    class: "mb-4 p-3 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg text-sm",
                    "{err}"
                }
            }

            div {
                class: "space-y-4",
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Recipient"
                    }
                    input {
                        r#type: "text",
                        class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-blue-500",
                        placeholder: "npub... or hex pubkey",
                        value: "{recipient_input.read()}",
                        oninput: move |evt| recipient_input.set(evt.value().clone())
                    }
                }

                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Message"
                    }
                    textarea {
                        class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none",
                        rows: "6",
                        placeholder: "Type your message...",
                        value: "{message_input.read()}",
                        oninput: move |evt| message_input.set(evt.value().clone())
                    }
                }

                div {
                    class: "flex gap-2",
                    button {
                        class: "flex-1 px-4 py-2 border border-border rounded-lg hover:bg-accent transition",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-2 bg-blue-500 hover:bg-blue-600 disabled:bg-gray-400 text-white rounded-lg font-medium transition",
                        disabled: *sending.read(),
                        onclick: send_message,
                        if *sending.read() {
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
