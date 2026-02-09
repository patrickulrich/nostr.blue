use crate::routes::Route;
use crate::stores::dms::ConversationMessage;
use crate::stores::{auth_store, dms, nostr_client, profiles};
use crate::utils::time;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use dioxus_core::Task;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
/// Guard struct that cancels polling task on drop
#[derive(Clone)]
struct PollTaskGuard {
    task: Signal<Option<Task>>,
}
impl Drop for PollTaskGuard {
    fn drop(&mut self) {
        if let Some(task) = self.task.read().as_ref() {
            task.cancel();
        }
    }
}
#[wasm_bindgen(
    inline_js = r#"
export function scrollDMsToBottom(elementId) {
    const element = document.getElementById(elementId);
    if (element) {
        element.scrollTop = element.scrollHeight;
    }
}

export function isDMsScrolledNearBottom(elementId, threshold) {
    const element = document.getElementById(elementId);
    if (!element) return true;
    const scrollTop = element.scrollTop;
    const scrollHeight = element.scrollHeight;
    const clientHeight = element.clientHeight;
    return scrollHeight - scrollTop - clientHeight < threshold;
}

export function isPageVisible() {
    return !document.hidden;
}
"#
)]
extern "C" {
    fn scrollDMsToBottom(element_id: &str);
    fn isDMsScrolledNearBottom(element_id: &str, threshold: f64) -> bool;
    fn isPageVisible() -> bool;
}
/// Run the decrypt previews pass if not already running.
async fn run_decrypt_if_idle(
    mut decrypting: Signal<bool>,
    previews: Signal<HashMap<String, String>>,
) {
    if !*decrypting.peek() {
        decrypting.set(true);
        decrypt_previews_sequentially(previews).await;
        decrypting.set(false);
    }
}
/// Decrypt the last message preview for each conversation sequentially,
/// so extension signers show at most one popup at a time.
async fn decrypt_previews_sequentially(mut previews: Signal<HashMap<String, String>>) {
    let conversations = dms::get_conversations_sorted();
    for conversation in &conversations {
        let pubkey = conversation.pubkey.clone();
        if let Some(last_msg) = conversation.messages.last() {
            match dms::decrypt_dm(last_msg).await {
                Ok(content) => {
                    let preview = if content.chars().count() > 50 {
                        let truncated: String = content.chars().take(50).collect();
                        format!("{}...", truncated)
                    } else {
                        content
                    };
                    previews.write().insert(pubkey, preview);
                }
                Err(_) => {
                    previews.write().insert(pubkey, "[Unable to decrypt]".to_string());
                }
            }
        } else {
            previews.write().insert(pubkey, "No messages".to_string());
        }
    }
}

#[component]
pub fn DMs() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut loading = use_signal(|| false);
    let mut refreshing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut selected_conversation = use_signal(|| None::<String>);
    let mut new_dm_mode = use_signal(|| false);
    let mut previews = use_signal(HashMap::<String, String>::new);
    let mut decrypting = use_signal(|| false);
    let mut dm_poll_task = use_signal(|| None::<Task>);
    use_hook(move || PollTaskGuard { task: dm_poll_task });
    use_effect(
        use_reactive(
            (&*nostr_client::CLIENT_INITIALIZED.read(), &auth_store::AUTH_STATE.read().is_authenticated),
            move |(client_initialized, is_authenticated)| {
                if !client_initialized {
                    log::debug!(
                        "Waiting for client initialization before loading DMs..."
                    );
                    return;
                }
                if !is_authenticated {
                    previews.write().clear();
                    decrypting.set(false);
                    return;
                }
                loading.set(true);
                error.set(None);
                spawn(async move {
                    match dms::init_dms().await {
                        Ok(_) => {
                            log::info!("DMs loaded successfully");
                            run_decrypt_if_idle(decrypting, previews).await;
                        }
                        Err(e) => {
                            error.set(Some(e));
                        }
                    }
                    loading.set(false);
                });
            },
        ),
    );
    use_effect(
        use_reactive(
            (&*nostr_client::CLIENT_INITIALIZED.read(), &auth_store::AUTH_STATE.read().is_authenticated),
            move |(client_initialized, is_authenticated)| {
                if let Some(task) = dm_poll_task.peek().as_ref() {
                    task.cancel();
                }
                if !client_initialized || !is_authenticated {
                    return;
                }
                let task = spawn(async move {
                    loop {
                        gloo_timers::future::sleep(std::time::Duration::from_secs(30))
                            .await;
                        if auth_store::is_authenticated() && isPageVisible() {
                            log::debug!("Auto-refreshing DMs...");
                            if dms::init_dms().await.is_ok() {
                                run_decrypt_if_idle(decrypting, previews).await;
                            }
                        }
                    }
                });
                dm_poll_task.set(Some(task));
            },
        ),
    );
    let refresh_dms = move |_| {
        if *refreshing.read() {
            return;
        }
        refreshing.set(true);
        spawn(async move {
            match dms::init_dms().await {
                Ok(_) => {
                    log::info!("DMs refreshed successfully");
                    run_decrypt_if_idle(decrypting, previews).await;
                }
                Err(e) => {
                    log::error!("Failed to refresh DMs: {}", e);
                }
            }
            refreshing.set(false);
        });
    };
    rsx! {
        div { class: "h-screen flex flex-col overflow-hidden",
            div { class: "shrink-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center justify-between",
                    h2 { class: "text-xl font-bold", "✉️ Messages" }
                    div { class: "flex items-center gap-2",
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
            if !auth.is_authenticated {
                div { class: "text-center py-12",
                    div { class: "text-6xl mb-4", "🔐" }
                    h3 { class: "text-xl font-semibold mb-2", "Sign in to view messages" }
                    p { class: "text-muted-foreground",
                        "Connect your account to send and receive encrypted messages"
                    }
                }
            } else {
                if let Some(err) = error.read().as_ref() {
                    div { class: "p-4",
                        div { class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                            "❌ {err}"
                        }
                    }
                }
                if *loading.read() {
                    div { class: "flex items-center justify-center p-12",
                        div { class: "text-center",
                            div { class: "animate-spin text-4xl mb-3", "✉️" }
                            p { class: "text-muted-foreground", "Loading messages..." }
                        }
                    }
                }
                if !*loading.read() {
                    div { class: "flex-1 flex overflow-hidden h-full",
                        div { class: "w-full sm:w-80 border-r border-border overflow-y-auto shrink-0 hide-scrollbar",
                            {
                                let conversations = dms::get_conversations_sorted();
                                if conversations.is_empty() && !*new_dm_mode.read() {
                                    rsx! {
                                        div { class: "text-center py-12 px-4",
                                            div { class: "text-6xl mb-4", "📭" }
                                            h3 { class: "text-lg font-semibold mb-2", "No messages yet" }
                                            p { class: "text-sm text-muted-foreground", "Start a conversation by clicking '+ New DM'" }
                                        }
                                    }
                                } else {
                                    rsx! {
                                        div { class: "divide-y divide-border",
                                            for conversation in conversations {
                                                {
                                                    let conv_pubkey = conversation.pubkey.clone();
                                                    let preview = previews.read().get(&conv_pubkey).cloned().unwrap_or_else(|| "Decrypting...".to_string());
                                                    rsx! {
                                                        ConversationListItem {
                                                            key: "{conv_pubkey}",
                                                            conversation: conversation.clone(),
                                                            selected: selected_conversation.read().as_ref() == Some(&conversation.pubkey),
                                                            preview,
                                                            on_select: move |pk: String| {
                                                                log::info!("Selected conversation: {}", pk);
                                                                selected_conversation.set(Some(pk));
                                                                new_dm_mode.set(false);
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "flex-1 flex flex-col overflow-hidden",
                            if *new_dm_mode.read() {
                                NewDMComposer {
                                    on_cancel: move |_| new_dm_mode.set(false),
                                    on_send: move |recipient: String| {
                                        selected_conversation.set(Some(recipient));
                                        new_dm_mode.set(false);
                                    },
                                }
                            } else if let Some(pubkey) = selected_conversation.read().as_ref() {
                                ConversationView { key: "{pubkey}", pubkey: pubkey.clone() }
                            } else {
                                div { class: "flex-1 flex items-center justify-center",
                                    div { class: "text-center text-muted-foreground",
                                        div { class: "text-6xl mb-4", "💬" }
                                        p { "Select a conversation to start messaging" }
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
    preview: String,
    on_select: EventHandler<String>,
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let pubkey = conversation.pubkey.clone();
    use_effect(move || {
        let pk = pubkey.clone();
        spawn(async move {
            match profiles::fetch_profile(pk).await {
                Ok(p) => profile.set(Some(p)),
                Err(e) => log::error!("Failed to fetch profile: {}", e),
            }
        });
    });
    let display_name = profile
        .read()
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| truncate_pubkey(&conversation.pubkey));
    let avatar_url = profile
        .read()
        .as_ref()
        .map(|p| p.get_avatar_url())
        .unwrap_or_else(|| {
            format!(
                "https://api.dicebear.com/7.x/identicon/svg?seed={}",
                conversation.pubkey,
            )
        });
    let time_ago = conversation
        .messages
        .last()
        .map(|m| time::format_relative_time(m.created_at()))
        .unwrap_or_else(|| "".to_string());
    let bg_class = if selected { "bg-accent" } else { "hover:bg-accent/50" };
    rsx! {
        div {
            class: "p-4 cursor-pointer transition {bg_class}",
            onclick: move |_| on_select.call(conversation.pubkey.clone()),
            div { class: "flex items-center gap-3",
                img {
                    src: "{avatar_url}",
                    alt: "{display_name}",
                    class: "w-12 h-12 rounded-full object-cover shrink-0",
                }
                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center justify-between gap-2 mb-1",
                        p { class: "font-semibold text-sm truncate", "{display_name}" }
                        if !time_ago.is_empty() {
                            span { class: "text-xs text-muted-foreground shrink-0", "{time_ago}" }
                        }
                    }
                    p { class: "text-sm text-muted-foreground truncate", "{preview}" }
                }
                if conversation.unread_count > 0 {
                    div { class: "w-6 h-6 bg-blue-500 rounded-full flex items-center justify-center text-white text-xs font-bold shrink-0",
                        "{conversation.unread_count}"
                    }
                }
            }
        }
    }
}
#[component]
fn ConversationView(pubkey: String) -> Element {
    let mut message_input = use_signal(String::new);
    let mut sending = use_signal(|| false);
    let mut decrypted_messages = use_signal(Vec::<(ConversationMessage, String)>::new);
    let mut decrypt_loading = use_signal(|| true);
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let messages_container_id = use_signal(|| {
        format!("messages-{}", uuid::Uuid::new_v4())
    });
    let mut send_feedback = use_signal(|| Option::<(bool, String)>::None);
    let mut feedback_version = use_signal(|| 0u32);
    let mut poll_task = use_signal(|| None::<Task>);
    let mut is_first_load = use_signal(|| true);
    let pubkey_for_effect = pubkey.clone();
    let pubkey_for_send = pubkey.clone();
    let pubkey_for_input = pubkey.clone();
    let pubkey_for_display = pubkey.clone();
    let pubkey_for_profile = pubkey.clone();
    let pubkey_for_poll = pubkey.clone();
    use_effect(move || {
        let pk = pubkey_for_profile.clone();
        spawn(async move {
            match profiles::fetch_profile(pk).await {
                Ok(p) => profile.set(Some(p)),
                Err(e) => log::error!("Failed to fetch profile: {}", e),
            }
        });
    });
    use_effect(
        use_reactive(
            (&pubkey_for_poll, &*nostr_client::CLIENT_INITIALIZED.read()),
            move |(pk, client_initialized)| {
                if let Some(task) = poll_task.peek().as_ref() {
                    task.cancel();
                }
                if !client_initialized {
                    return;
                }
                let pk_clone = pk.clone();
                let new_task = spawn(async move {
                    loop {
                        gloo_timers::future::TimeoutFuture::new(5000).await;
                        if !isPageVisible() {
                            continue;
                        }
                        if let Err(e) = dms::init_dms().await {
                            log::warn!("DM poll refresh failed: {}", e);
                            continue;
                        }
                        if let Some(conversation) = dms::get_conversation(&pk_clone) {
                            let mut decrypted = Vec::new();
                            for msg in conversation.messages {
                                match dms::decrypt_dm(&msg).await {
                                    Ok(content) => decrypted.push((msg, content)),
                                    Err(_) => {
                                        decrypted.push((msg, "[Failed to decrypt]".to_string()))
                                    }
                                }
                            }
                            decrypted_messages.set(decrypted);
                        }
                    }
                });
                poll_task.set(Some(new_task));
            },
        ),
    );
    use_hook(move || PollTaskGuard { task: poll_task });
    use_effect(move || {
        let pk = pubkey_for_effect.clone();
        decrypt_loading.set(true);
        decrypted_messages.set(Vec::new());
        spawn(async move {
            log::info!("Loading conversation for: {}", pk);
            if let Some(conversation) = dms::get_conversation(&pk) {
                log::info!(
                    "Found {} messages in conversation", conversation.messages.len()
                );
                let mut decrypted = Vec::new();
                for msg in conversation.messages {
                    match dms::decrypt_dm(&msg).await {
                        Ok(content) => {
                            log::debug!(
                                "Decrypted message: {}", & content[..content.len().min(50)]
                            );
                            decrypted.push((msg, content));
                        }
                        Err(e) => {
                            log::error!("Failed to decrypt message: {}", e);
                            decrypted.push((msg, "[Failed to decrypt]".to_string()));
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
    use_effect(move || {
        let msg_count = decrypted_messages.read().len();
        let container_id = messages_container_id.read().clone();
        let loading = *decrypt_loading.read();
        if loading || msg_count == 0 {
            return;
        }
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(50).await;
            if *is_first_load.peek() {
                scrollDMsToBottom(&container_id);
                is_first_load.set(false);
            } else if isDMsScrolledNearBottom(&container_id, 100.0) {
                scrollDMsToBottom(&container_id);
            }
        });
    });
    let send_message = move |_| {
        let content = message_input.read().clone();
        if content.trim().is_empty() {
            return;
        }
        sending.set(true);
        send_feedback.set(None);
        let recipient = pubkey_for_send.clone();
        spawn(async move {
            match dms::send_dm(recipient, content).await {
                Ok(result) => {
                    sending.set(false);
                    let rate = result.success_rate();
                    if !result.is_success() {
                        feedback_version.set(feedback_version() + 1);
                        let current_version = feedback_version();
                        send_feedback
                            .set(
                                Some((false, "Failed to send to any relay".to_string())),
                            );
                        gloo_timers::future::TimeoutFuture::new(3000).await;
                        if feedback_version() == current_version {
                            send_feedback.set(None);
                        }
                    } else if result.has_failures() {
                        message_input.set(String::new());
                        log::info!("Message sent successfully");
                        feedback_version.set(feedback_version() + 1);
                        let current_version = feedback_version();
                        send_feedback
                            .set(
                                Some((
                                    true,
                                    format!(
                                        "Sent to {:.0}% of relays ({}/{})",
                                        rate,
                                        result.success_count(),
                                        result.total_attempted(),
                                    ),
                                )),
                            );
                        gloo_timers::future::TimeoutFuture::new(3000).await;
                        if feedback_version() == current_version {
                            send_feedback.set(None);
                        }
                    } else {
                        message_input.set(String::new());
                        log::info!("Message sent successfully");
                    }
                }
                Err(e) => {
                    sending.set(false);
                    log::error!("Failed to send message: {}", e);
                    feedback_version.set(feedback_version() + 1);
                    let current_version = feedback_version();
                    send_feedback.set(Some((false, format!("Error: {}", e))));
                    gloo_timers::future::TimeoutFuture::new(5000).await;
                    if feedback_version() == current_version {
                        send_feedback.set(None);
                    }
                }
            }
        });
    };
    let display_name = profile
        .read()
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| truncate_pubkey(&pubkey_for_display));
    let avatar_url = profile
        .read()
        .as_ref()
        .map(|p| p.get_avatar_url())
        .unwrap_or_else(|| {
            format!(
                "https://api.dicebear.com/7.x/identicon/svg?seed={}",
                pubkey_for_display,
            )
        });
    let nip05 = profile.read().as_ref().and_then(|p| p.nip05.clone());
    let container_id = messages_container_id.read().clone();
    rsx! {
        div { class: "flex-1 flex flex-col overflow-hidden h-full",
            div { class: "shrink-0 p-4 border-b border-border flex items-center gap-3",
                img {
                    src: "{avatar_url}",
                    alt: "{display_name}",
                    class: "w-10 h-10 rounded-full object-cover shrink-0",
                }
                div { class: "flex-1 min-w-0",
                    h3 { class: "font-semibold truncate", "{display_name}" }
                    if let Some(nip05_id) = nip05 {
                        p { class: "text-xs text-muted-foreground truncate", "{nip05_id}" }
                    }
                    Link {
                        to: Route::Profile { pubkey },
                        class: "text-xs text-blue-500 hover:underline",
                        "View profile"
                    }
                }
            }
            div {
                id: "{container_id}",
                class: "flex-1 overflow-y-auto p-4 space-y-4",
                if *decrypt_loading.read() {
                    div { class: "flex items-center justify-center p-8",
                        p { class: "text-muted-foreground", "Decrypting messages..." }
                    }
                } else if decrypted_messages.read().is_empty() {
                    div { class: "flex items-center justify-center p-8",
                        p { class: "text-muted-foreground text-center",
                            "No messages yet. Start the conversation!"
                        }
                    }
                } else {
                    for (msg , content) in decrypted_messages.read().iter() {
                        {
                            let my_pubkey = auth_store::get_pubkey().unwrap_or_default();
                            let is_mine = msg.sender().to_string() == my_pubkey;
                            let sender_pubkey = msg.sender().to_string();
                            let enc_type = msg.encryption_type().to_string();
                            rsx! {
                                MessageBubble {
                                    key: "{msg.id()}",
                                    content: content.clone(),
                                    is_mine,
                                    timestamp: msg.created_at(),
                                    sender_pubkey,
                                    encryption_type: enc_type,
                                }
                            }
                        }
                    }
                }
            }
            if let Some((is_success, message)) = send_feedback.read().clone() {
                div { class: if is_success { "mx-4 mb-2 p-3 rounded-lg bg-yellow-500/10 border border-yellow-500/20 text-yellow-600 dark:text-yellow-400 text-sm flex items-center gap-2" } else { "mx-4 mb-2 p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-red-600 dark:text-red-400 text-sm flex items-center gap-2" },
                    span { "{message}" }
                    button {
                        class: "ml-auto text-current opacity-60 hover:opacity-100",
                        onclick: move |_| send_feedback.set(None),
                        "×"
                    }
                }
            }
            div { class: "shrink-0 p-4 border-t border-border",
                div { class: "flex gap-2",
                    input {
                        r#type: "text",
                        class: "flex-1 px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                        placeholder: "Type a message...",
                        value: "{message_input.read()}",
                        oninput: move |evt| message_input.set(evt.value().clone()),
                        onkeydown: move |evt| {
                            if evt.key() == Key::Enter && !evt.modifiers().shift() {
                                let content = message_input.read().clone();
                                if content.trim().is_empty() {
                                    return;
                                }
                                sending.set(true);
                                send_feedback.set(None);
                                let recipient = pubkey_for_input.clone();
                                spawn(async move {
                                    match dms::send_dm(recipient, content).await {
                                        Ok(result) => {
                                            sending.set(false);
                                            let rate = result.success_rate();
                                            if !result.is_success() {
                                                feedback_version.set(feedback_version() + 1);
                                                let current_version = feedback_version();
                                                send_feedback
                                                    .set(
                                                        Some((false, "Failed to send to any relay".to_string())),
                                                    );
                                                gloo_timers::future::TimeoutFuture::new(3000).await;
                                                if feedback_version() == current_version {
                                                    send_feedback.set(None);
                                                }
                                            } else if result.has_failures() {
                                                message_input.set(String::new());
                                                log::info!("Message sent successfully");
                                                feedback_version.set(feedback_version() + 1);
                                                let current_version = feedback_version();
                                                send_feedback
                                                    .set(
                                                        Some((
                                                            true,
                                                            format!(
                                                                "Sent to {:.0}% of relays ({}/{})",
                                                                rate,
                                                                result.success_count(),
                                                                result.total_attempted(),
                                                            ),
                                                        )),
                                                    );
                                                gloo_timers::future::TimeoutFuture::new(3000).await;
                                                if feedback_version() == current_version {
                                                    send_feedback.set(None);
                                                }
                                            } else {
                                                message_input.set(String::new());
                                                log::info!("Message sent successfully");
                                            }
                                        }
                                        Err(e) => {
                                            sending.set(false);
                                            log::error!("Failed to send message: {}", e);
                                            feedback_version.set(feedback_version() + 1);
                                            let current_version = feedback_version();
                                            send_feedback.set(Some((false, format!("Error: {}", e))));
                                            gloo_timers::future::TimeoutFuture::new(5000).await;
                                            if feedback_version() == current_version {
                                                send_feedback.set(None);
                                            }
                                        }
                                    }
                                });
                            }
                        },
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
    sender_pubkey: String,
    #[props(default = "NIP-17".to_string())]
    encryption_type: String,
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let sender_pk = sender_pubkey.clone();
    let sender_pk_for_avatar = sender_pubkey.clone();
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
                .unwrap_or_else(|| {
                    format!(
                        "https://api.dicebear.com/7.x/identicon/svg?seed={}",
                        my_pubkey,
                    )
                })
        } else {
            String::new()
        }
    } else {
        profile
            .read()
            .as_ref()
            .map(|p| p.get_avatar_url())
            .unwrap_or_else(|| {
                format!(
                    "https://api.dicebear.com/7.x/identicon/svg?seed={}",
                    sender_pk_for_avatar,
                )
            })
    };
    let time_ago = time::format_relative_time(timestamp);
    let alignment = if is_mine { "flex-row-reverse" } else { "flex-row" };
    let bg_color = if is_mine { "bg-blue-500 text-white" } else { "bg-accent" };
    let items_align = if is_mine { "items-end" } else { "items-start" };
    let (badge_class, badge_icon) = match encryption_type.as_str() {
        "NIP-04" => ("text-orange-500 dark:text-orange-400", "⚠️"),
        "NIP-17" => ("text-blue-500 dark:text-blue-400", "🔒"),
        _ => ("text-muted-foreground", ""),
    };
    rsx! {
        div { class: "flex gap-3 mb-4 {alignment}",
            img {
                src: "{avatar_url}",
                alt: "Avatar",
                class: "w-8 h-8 rounded-full object-cover shrink-0",
            }
            div { class: "flex flex-col gap-1 max-w-[70%] min-w-0 {items_align}",
                div { class: "{bg_color} rounded-2xl px-4 py-2 overflow-hidden",
                    p { class: "text-sm whitespace-pre-wrap break-words [overflow-wrap:anywhere]",
                        "{content}"
                    }
                }
                div { class: "flex items-center gap-2 px-2",
                    span { class: "text-xs text-muted-foreground", "{time_ago}" }
                    span {
                        class: "text-xs {badge_class}",
                        title: "{encryption_type}",
                        "{badge_icon}"
                    }
                }
            }
        }
    }
}
#[component]
fn NewDMComposer(on_cancel: EventHandler<()>, on_send: EventHandler<String>) -> Element {
    let mut recipient_input = use_signal(String::new);
    let mut message_input = use_signal(String::new);
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
        div { class: "flex-1 flex flex-col p-4",
            div { class: "mb-4",
                h3 { class: "text-lg font-semibold mb-2", "New Direct Message" }
                p { class: "text-sm text-muted-foreground",
                    "Enter the recipient's public key (npub or hex)"
                }
            }
            if let Some(err) = error.read().as_ref() {
                div { class: "mb-4 p-3 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg text-sm",
                    "{err}"
                }
            }
            div { class: "space-y-4",
                div {
                    label { class: "block text-sm font-medium mb-2", "Recipient" }
                    input {
                        r#type: "text",
                        class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                        placeholder: "npub... or hex pubkey",
                        value: "{recipient_input.read()}",
                        oninput: move |evt| recipient_input.set(evt.value().clone()),
                    }
                }
                div {
                    label { class: "block text-sm font-medium mb-2", "Message" }
                    textarea {
                        class: "w-full px-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-blue-500 resize-none",
                        rows: "6",
                        placeholder: "Type your message...",
                        value: "{message_input.read()}",
                        oninput: move |evt| message_input.set(evt.value().clone()),
                    }
                }
                div { class: "flex gap-2",
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
