use crate::components::icons::{SendIcon, SettingsIcon, SparklesIcon, TrashIcon};
use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::services::ai_chat::{
    get_available_models, send_chat_message, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessage, ChatModel, ChatRole, ToolCall, ToolDefinition, ToolFunction,
};
use crate::stores::ai_chat_store::{
    self, PersistedChatMessage, PersistedChatRole, PersistedToolCall,
};
use crate::stores::ai_provider_store::{
    self, resolve_providers, shakespeare_provider, AiProviderConfig, AiProviderState,
};
use crate::stores::{nostr_client, theme_store};
use crate::utils::markdown::render_markdown;
use dioxus::document;
use dioxus::prelude::*;
use serde_json::json;
use std::hash::{Hash, Hasher};

const SYSTEM_PROMPT: &str = "You are Nostrich, an AI assistant inside nostr.blue. Be concise and helpful for the user. Your personality is a fun ostrich that represents the nostr community.";
const THEME_TOOL_NAME: &str = "set_theme";
const AI_CHAT_PROVIDER_PERSISTENCE_ENABLED: bool = true;
const AI_CHAT_HISTORY_LOAD_ENABLED: bool = true;
const AI_CHAT_HISTORY_SAVE_ENABLED: bool = true;

#[derive(Clone, Debug, PartialEq)]
struct DisplayMessage {
    id: String,
    role: DisplayRole,
    content: String,
    tool_calls: Vec<ExecutedToolCall>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DisplayRole {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq)]
struct ExecutedToolCall {
    id: String,
    name: String,
    result: String,
}

#[derive(Clone, serde::Deserialize)]
struct ThemeToolArgs {
    theme: String,
}

fn history_save_snapshot_key(
    account_key: &str,
    persisted_messages: &[PersistedChatMessage],
    initial_loaded_messages: &[PersistedChatMessage],
) -> String {
    fn hash_messages(messages: &[PersistedChatMessage]) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        messages.len().hash(&mut hasher);
        for message in messages {
            message.id.hash(&mut hasher);
            message.role.hash(&mut hasher);
            message.content.hash(&mut hasher);
            message.tool_calls.len().hash(&mut hasher);
            for call in &message.tool_calls {
                call.id.hash(&mut hasher);
                call.name.hash(&mut hasher);
                call.result.hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    format!(
        "{}:{:016x}:{:016x}",
        account_key,
        hash_messages(persisted_messages),
        hash_messages(initial_loaded_messages),
    )
}

#[component]
pub fn AIChat() -> Element {
    let mut messages = use_signal(Vec::<DisplayMessage>::new);
    let mut input = use_signal(String::new);
    let loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut models = use_signal(Vec::<ChatModel>::new);
    let mut selected_model = use_signal(String::new);
    let mut provider_state = use_signal(AiProviderState::default);
    let mut providers = use_signal(|| vec![shakespeare_provider()]);
    let mut provider_state_loaded = use_signal(|| false);
    let mut provider_state_loading = use_signal(|| false);
    let mut chat_history_loaded = use_signal(|| false);
    let mut chat_history_loading = use_signal(|| false);
    let mut chat_history_generation = use_signal(|| 0u32);
    let mut persisted_messages_dirty = use_signal(|| false);
    let persisted_messages_save_generation = use_signal(|| 0u32);
    let persisted_messages_save_in_flight = use_signal(|| false);
    let mut persisted_messages_failed_snapshot = use_signal(|| None::<String>);
    let provider_state_save_generation = use_signal(|| 0u32);
    let mut provider_state_save_in_flight = use_signal(|| false);
    let mut provider_models_generation = use_signal(|| 0u32);
    let mut initial_loaded_messages = use_signal(Vec::<PersistedChatMessage>::new);
    let messages_container_id = use_signal(|| "ai-chat-messages".to_string());
    let mut last_account_key = use_signal(|| None::<String>);
    let persisted_messages = use_memo(move || {
        messages
            .read()
            .iter()
            .cloned()
            .map(persisted_message_from_display)
            .collect::<Vec<PersistedChatMessage>>()
    });

    use_effect(move || {
        if !AI_CHAT_PROVIDER_PERSISTENCE_ENABLED {
            provider_state_loaded.set(true);
            return;
        }
        if *provider_state_loaded.read() || *provider_state_loading.peek() {
            return;
        }
        provider_state_loading.set(true);
        spawn(async move {
            match ai_provider_store::load_provider_state().await {
                Ok(mut loaded_state) => {
                    let resolved = resolve_providers(&loaded_state);
                    if !resolved
                        .iter()
                        .any(|provider| provider.id == loaded_state.selected_provider_id)
                    {
                        loaded_state.selected_provider_id = shakespeare_provider().id;
                    }
                    providers.set(resolve_providers(&loaded_state));
                    provider_state.set(loaded_state);
                }
                Err(e) => {
                    error.set(Some(e));
                    let default_state = AiProviderState::default();
                    providers.set(resolve_providers(&default_state));
                    provider_state.set(default_state);
                }
            }
            provider_state_loading.set(false);
            provider_state_loaded.set(true);
        });
    });

    use_effect(move || {
        let _ = messages.read().len();
        let id = messages_container_id.read().clone();
        spawn(async move {
            let script = format!(
                "(() => {{ const el = document.getElementById({:?}); if (el) {{ el.scrollTop = el.scrollHeight; return true; }} return false; }})()",
                id
            );
            let _ = document::eval(&script).await;
        });
    });

    use_effect(move || {
        let account_key = ai_chat_store::current_account_key();
        if last_account_key.read().as_deref() == Some(account_key.as_str()) {
            return;
        }
        last_account_key.set(Some(account_key));
        messages.set(Vec::new());
        chat_history_loaded.set(false);
        chat_history_loading.set(false);
        persisted_messages_dirty.set(false);
        initial_loaded_messages.set(Vec::new());
        if !AI_CHAT_HISTORY_LOAD_ENABLED && !AI_CHAT_HISTORY_SAVE_ENABLED {
            return;
        }
        let next_generation = chat_history_generation.read().wrapping_add(1);
        chat_history_generation.set(next_generation);
    });

    use_effect(move || {
        if persisted_messages_failed_snapshot.read().is_none() {
            return;
        }
        let account_key = ai_chat_store::current_account_key();
        let snapshot_key = history_save_snapshot_key(
            &account_key,
            &persisted_messages.read(),
            &initial_loaded_messages.read(),
        );
        if persisted_messages_failed_snapshot
            .read()
            .as_ref()
            .is_some_and(|failed| failed != &snapshot_key)
        {
            persisted_messages_failed_snapshot.set(None);
        }
    });

    use_effect(move || {
        if !AI_CHAT_HISTORY_LOAD_ENABLED {
            chat_history_loaded.set(true);
            return;
        }
        let provider_state_ready = *provider_state_loaded.read();
        let chat_history_ready = *chat_history_loaded.read();
        let generation = *chat_history_generation.read();
        let account_key = ai_chat_store::current_account_key();

        if !provider_state_ready || chat_history_ready || *chat_history_loading.peek() {
            return;
        }

        chat_history_loading.set(true);
        spawn(async move {
            match ai_chat_store::load_chat_history(&account_key).await {
                Ok(history) => {
                    if *chat_history_generation.read() != generation
                        || ai_chat_store::current_account_key() != account_key
                    {
                        return;
                    }
                    if *persisted_messages_dirty.read() || !messages.read().is_empty() {
                        return;
                    }
                    initial_loaded_messages.set(history.clone());
                    persisted_messages_dirty.set(false);
                    messages.set(
                        history
                            .into_iter()
                            .map(display_message_from_persisted)
                            .collect(),
                    );
                }
                Err(e) => {
                    if *chat_history_generation.read() != generation
                        || ai_chat_store::current_account_key() != account_key
                    {
                        return;
                    }
                    error.set(Some(e));
                }
            }
            if *chat_history_generation.read() == generation
                && ai_chat_store::current_account_key() == account_key
            {
                chat_history_loaded.set(true);
            }
            chat_history_loading.set(false);
        });
    });

    use_effect(move || {
        if !AI_CHAT_HISTORY_SAVE_ENABLED {
            return;
        }
        if *persisted_messages_save_in_flight.read() {
            return;
        }
        let account_key = ai_chat_store::current_account_key();
        let chat_history_ready = *chat_history_loaded.read();
        let persisted_messages_dirty_value = *persisted_messages_dirty.read();
        let initial_loaded_messages_snapshot = initial_loaded_messages.read().clone();
        let persisted_messages_snapshot = persisted_messages.read().clone();
        let failed_snapshot_key = history_save_snapshot_key(
            &account_key,
            &persisted_messages_snapshot,
            &initial_loaded_messages_snapshot,
        );

        if !chat_history_ready
            || !persisted_messages_dirty_value
            || persisted_messages_snapshot == initial_loaded_messages_snapshot
            || ai_chat_store::current_account_key() != account_key
            || persisted_messages_failed_snapshot.read().as_ref() == Some(&failed_snapshot_key)
        {
            return;
        }

        let mut persisted_messages_dirty_signal = persisted_messages_dirty;
        let mut persisted_messages_save_generation_signal = persisted_messages_save_generation;
        let mut persisted_messages_save_in_flight_signal = persisted_messages_save_in_flight;
        let persisted_messages_signal = persisted_messages;
        let chat_history_generation_signal = chat_history_generation;
        let mut initial_loaded_messages_signal = initial_loaded_messages;
        let mut persisted_messages_failed_snapshot_signal = persisted_messages_failed_snapshot;
        let generation = persisted_messages_save_generation_signal
            .read()
            .wrapping_add(1);
        let chat_generation = *chat_history_generation_signal.read();
        persisted_messages_save_in_flight_signal.set(true);
        persisted_messages_save_generation_signal.set(generation);
        spawn(async move {
            const MAX_SAVE_ATTEMPTS: u32 = 4;

            for attempt in 0..MAX_SAVE_ATTEMPTS {
                if *persisted_messages_save_generation_signal.read() != generation
                    || *chat_history_generation_signal.read() != chat_generation
                    || ai_chat_store::current_account_key() != account_key
                {
                    persisted_messages_save_in_flight_signal.set(false);
                    return;
                }

                let result = if persisted_messages_snapshot.is_empty() {
                    ai_chat_store::clear_chat_history(&account_key).await
                } else {
                    ai_chat_store::save_chat_history(&account_key, &persisted_messages_snapshot)
                        .await
                };

                match result {
                    Ok(()) => {
                        if *persisted_messages_save_generation_signal.read() == generation
                            && *chat_history_generation_signal.read() == chat_generation
                            && ai_chat_store::current_account_key() == account_key
                        {
                            let latest_persisted_messages =
                                persisted_messages_signal.read().clone();
                            if latest_persisted_messages == persisted_messages_snapshot {
                                persisted_messages_dirty_signal.set(false);
                                initial_loaded_messages_signal
                                    .set(persisted_messages_snapshot.clone());
                            }
                            persisted_messages_failed_snapshot_signal.set(None);
                        }
                        persisted_messages_save_in_flight_signal.set(false);
                        return;
                    }
                    Err(e) => {
                        if attempt + 1 == MAX_SAVE_ATTEMPTS {
                            if *persisted_messages_save_generation_signal.read() == generation
                                && *chat_history_generation_signal.read() == chat_generation
                                && ai_chat_store::current_account_key() == account_key
                            {
                                error.set(Some(e));
                                persisted_messages_failed_snapshot_signal
                                    .set(Some(failed_snapshot_key.clone()));
                            }
                            persisted_messages_save_in_flight_signal.set(false);
                            return;
                        }

                        crate::platform::timer::sleep_ms((attempt + 1) * 100).await;
                    }
                }
            }
        });
    });

    use_effect(move || {
        if !AI_CHAT_PROVIDER_PERSISTENCE_ENABLED {
            return;
        }
        let save_generation = *provider_state_save_generation.read();
        if save_generation == 0 || *provider_state_save_in_flight.peek() {
            return;
        }

        provider_state_save_in_flight.set(true);
        spawn(async move {
            loop {
                let generation = *provider_state_save_generation.peek();
                let snapshot = provider_state.read().clone();
                match ai_provider_store::save_provider_state(&snapshot).await {
                    Ok(()) => {
                        if *provider_state_save_generation.peek() == generation {
                            provider_state_save_in_flight.set(false);
                            break;
                        }
                    }
                    Err(e) => {
                        error.set(Some(e));
                        if *provider_state_save_generation.peek() == generation {
                            provider_state_save_in_flight.set(false);
                            break;
                        }
                    }
                }
            }
        });
    });

    use_effect(move || {
        let provider_state_ready = *provider_state_loaded.read();
        let selected_provider_id = provider_state.read().selected_provider_id.clone();
        let has_signer = *nostr_client::HAS_SIGNER.read();

        if !provider_state_ready {
            return;
        }

        let available_providers = providers.read().clone();
        let local_generation = provider_models_generation.with_mut(|generation| {
            *generation = generation.wrapping_add(1);
            *generation
        });
        spawn(async move {
            let Some(provider) = available_providers
                .into_iter()
                .find(|provider| provider.id == selected_provider_id)
            else {
                return;
            };

            if provider.requires_signer() && !has_signer {
                models.set(Vec::new());
                selected_model.set(String::new());
                error.set(None);
                return;
            }

            match get_available_models(&provider).await {
                Ok(available_models) => {
                    if *provider_models_generation.peek() != local_generation
                        || provider_state.read().selected_provider_id != provider.id
                    {
                        return;
                    }
                    let saved_model = provider_state
                        .read()
                        .selected_model_by_provider
                        .get(&provider.id)
                        .cloned();
                    let selected_is_valid = available_models
                        .iter()
                        .any(|model| model.id == *selected_model.read());
                    let saved_is_valid = saved_model
                        .as_ref()
                        .map(|saved| available_models.iter().any(|model| model.id == *saved))
                        .unwrap_or(false);
                    let next_model = if selected_is_valid {
                        selected_model.read().clone()
                    } else if saved_is_valid {
                        saved_model.clone().unwrap_or_default()
                    } else {
                        available_models
                            .first()
                            .map(|model| model.id.clone())
                            .unwrap_or_default()
                    };

                    if *provider_models_generation.peek() != local_generation
                        || provider_state.read().selected_provider_id != provider.id
                    {
                        return;
                    }

                    if !next_model.is_empty() {
                        selected_model.set(next_model.clone());
                        if saved_model.as_deref() != Some(next_model.as_str()) {
                            persist_selected_model(
                                provider.id.clone(),
                                next_model,
                                provider_state,
                                provider_state_save_generation,
                                error,
                            );
                        }
                    } else {
                        selected_model.set(String::new());
                    }
                    models.set(available_models);
                    error.set(None);
                }
                Err(e) => {
                    if *provider_models_generation.peek() != local_generation
                        || provider_state.read().selected_provider_id != provider.id
                    {
                        return;
                    }
                    models.set(Vec::new());
                    selected_model.set(String::new());
                    error.set(Some(e));
                }
            }
        });
    });

    if !*nostr_client::CLIENT_INITIALIZED.read() || !*provider_state_loaded.read() {
        return rsx! { ClientInitializing {} };
    }

    let active_provider = current_provider(&providers.read(), &provider_state.read());
    let shakespeare_blocked =
        active_provider.requires_signer() && !*nostr_client::HAS_SIGNER.read();

    let provider_for_keydown = active_provider.clone();
    let provider_for_click = active_provider.clone();

    rsx! {
        div { class: "min-h-screen flex flex-col bg-background",
            div { class: "sticky top-0 z-20 border-b border-border bg-background/90 backdrop-blur-sm",
                div { class: "mx-auto flex max-w-5xl items-center justify-between gap-4 px-4 py-4",
                    div { class: "flex items-center gap-3",
                        div { class: "flex h-10 w-10 items-center justify-center rounded-xl bg-primary/10 text-primary",
                            SparklesIcon { class: "w-5 h-5".to_string() }
                        }
                        div {
                            h1 { class: "text-xl font-semibold", "AI Chat" }
                            p { class: "text-sm text-muted-foreground",
                                "Provider: {active_provider.name}"
                            }
                        }
                    }
                    div { class: "flex items-center gap-2",
                        select {
                            class: "h-10 rounded-lg border border-border bg-card px-3 text-sm text-foreground focus:outline-hidden",
                            value: "{selected_model}",
                            disabled: models.read().is_empty() || *loading.read() || shakespeare_blocked,
                            onchange: move |evt| {
                                let value = evt.value();
                                selected_model.set(value.clone());
                                persist_selected_model(
                                    active_provider.id.clone(),
                                    value,
                                    provider_state,
                                    provider_state_save_generation,
                                    error,
                                );
                            },
                            if models.read().is_empty() {
                                option { value: "", if shakespeare_blocked { "Sign in for Shakespeare models" } else { "Loading models..." } }
                            } else {
                                for model in models.read().iter() {
                                    {
                                        let label = if model.total_cost == Some(0.0) {
                                            format!("{} · FREE", model.name)
                                        } else {
                                            model.name.clone()
                                        };
                                        rsx! {
                                            option { key: "{model.id}", value: "{model.id}", "{label}" }
                                        }
                                    }
                                }
                            }
                        }
                        button {
                            class: if messages.read().is_empty() {
                                "flex h-10 w-10 items-center justify-center rounded-lg border border-border text-muted-foreground opacity-50"
                            } else {
                                "flex h-10 w-10 items-center justify-center rounded-lg border border-border text-muted-foreground transition hover:bg-accent"
                            },
                            disabled: messages.read().is_empty() || *loading.read(),
                            title: "Clear conversation",
                            onclick: move |_| {
                                messages.set(Vec::new());
                                persisted_messages_dirty.set(true);
                                error.set(None);
                            },
                            TrashIcon { class: "w-4 h-4".to_string() }
                        }
                        button {
                            class: "flex h-10 w-10 items-center justify-center rounded-lg border border-border text-muted-foreground transition hover:bg-accent",
                            disabled: *loading.read(),
                            title: "AI settings",
                            onclick: move |_| {
                                navigator().push(Route::SettingsAi {});
                            },
                            SettingsIcon { class: "w-4 h-4".to_string() }
                        }
                    }
                }
            }

            div {
                id: "{messages_container_id}",
                class: "flex-1 overflow-y-auto",
                div { class: "mx-auto flex max-w-5xl flex-col gap-6 px-4 py-6",
                    if shakespeare_blocked {
                        SignInGate {}
                    } else if messages.read().is_empty() {
                        EmptyState { provider_name: active_provider.name.clone() }
                    } else {
                        for message in messages.read().iter() {
                            MessageBubble { key: "{message.id}", message: message.clone() }
                        }
                    }

                    if *loading.read() {
                        div { class: "max-w-3xl rounded-2xl border border-border bg-card px-4 py-3 text-sm text-muted-foreground shadow-sm",
                            "Thinking..."
                        }
                    }

                    if let Some(err) = error.read().as_ref() {
                        div { class: "max-w-3xl rounded-2xl border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm text-red-600 dark:text-red-400",
                            "{err}"
                        }
                    }
                }
            }

            div { class: "border-t border-border bg-background",
                div { class: "mx-auto max-w-5xl px-4 py-4",
                    div { class: "rounded-2xl border border-border bg-card p-3 shadow-sm",
                        textarea {
                            class: "min-h-[96px] w-full resize-none bg-transparent text-sm text-foreground placeholder:text-muted-foreground focus:outline-hidden disabled:cursor-not-allowed disabled:opacity-60",
                            placeholder: if shakespeare_blocked {
                                "Open AI settings to switch providers or sign in for Shakespeare..."
                            } else if selected_model.read().is_empty() {
                                "Select a model first..."
                            } else {
                                "Send a message..."
                            },
                            value: "{input}",
                            disabled: selected_model.read().is_empty() || *loading.read() || shakespeare_blocked,
                            oninput: move |evt| input.set(evt.value()),
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter && !evt.modifiers().shift() {
                                    evt.prevent_default();
                                    submit_message(
                                        input,
                                        selected_model,
                                        loading,
                                        error,
                                        messages,
                                        persisted_messages_dirty,
                                        provider_for_keydown.clone(),
                                    );
                                }
                            },
                        }
                        div { class: "mt-3 flex items-center justify-between gap-3",
                            p { class: "text-xs text-muted-foreground", "Enter to send. Shift+Enter for newline." }
                            button {
                                class: if input.read().trim().is_empty() || selected_model.read().is_empty() || *loading.read() || shakespeare_blocked {
                                    "inline-flex h-11 w-11 items-center justify-center rounded-xl bg-muted text-muted-foreground cursor-not-allowed"
                                } else {
                                    "inline-flex h-11 w-11 items-center justify-center rounded-xl bg-primary text-primary-foreground transition hover:bg-primary/90"
                                },
                                disabled: input.read().trim().is_empty() || selected_model.read().is_empty() || *loading.read() || shakespeare_blocked,
                                onclick: move |_| {
                                    submit_message(
                                        input,
                                        selected_model,
                                        loading,
                                        error,
                                        messages,
                                        persisted_messages_dirty,
                                        provider_for_click.clone(),
                                    )
                                },
                                SendIcon { class: "w-4 h-4".to_string() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SignInGate() -> Element {
    rsx! {
        div { class: "flex min-h-[50vh] items-center justify-center p-6",
            div { class: "max-w-md w-full rounded-2xl border border-border bg-card p-8 text-center shadow-sm",
                div { class: "mx-auto mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-primary/10 text-primary",
                    SparklesIcon { class: "w-7 h-7".to_string() }
                }
                h2 { class: "text-2xl font-semibold", "Sign in to use Shakespeare" }
                p { class: "mt-2 text-sm text-muted-foreground",
                    "Shakespeare uses NIP-98 authenticated requests. Sign in, or open AI settings and switch to a custom provider with your own API key."
                }
            }
        }
    }
}

#[component]
fn EmptyState(provider_name: String) -> Element {
    rsx! {
        div { class: "flex min-h-[50vh] items-center justify-center",
            div { class: "max-w-2xl text-center",
                div { class: "mx-auto mb-5 flex h-16 w-16 items-center justify-center rounded-3xl bg-primary/10 text-primary",
                    SparklesIcon { class: "w-8 h-8".to_string() }
                }
                h2 { class: "text-3xl font-semibold tracking-tight", "AI Chat" }
                p { class: "mt-3 text-base text-muted-foreground",
                    "Ask questions, iterate on ideas, or switch the app between light, dark, and system theme. Current provider: {provider_name}."
                }
            }
        }
    }
}

#[component]
fn MessageBubble(message: DisplayMessage) -> Element {
    let is_user = message.role == DisplayRole::User;
    let html_content = if is_user {
        None
    } else {
        Some(render_markdown(&message.content))
    };

    rsx! {
        div { class: if is_user { "flex justify-end" } else { "flex justify-start" },
            div { class: if is_user {
                    "max-w-3xl rounded-2xl bg-primary px-4 py-3 text-sm text-primary-foreground shadow-sm"
                } else {
                    "max-w-3xl rounded-2xl border border-border bg-card px-4 py-3 text-sm text-foreground shadow-sm"
                },
                if is_user {
                    p { class: "whitespace-pre-wrap break-words", "{message.content}" }
                } else if let Some(rendered) = html_content {
                    div {
                        class: "prose prose-sm max-w-none prose-neutral dark:prose-invert [&_p]:my-2 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:bg-muted [&_pre]:p-3 [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5",
                        dangerous_inner_html: "{rendered}",
                    }
                }
                if !message.tool_calls.is_empty() {
                    div { class: "mt-4 space-y-2 border-t border-border pt-3",
                        for call in message.tool_calls.iter() {
                            div { key: "{call.id}", class: "rounded-lg bg-muted/60 px-3 py-2 text-xs text-muted-foreground",
                                p { class: "font-medium text-foreground", "Tool: {call.name}" }
                                p { class: "mt-1 whitespace-pre-wrap break-words", "{call.result}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn current_provider(providers: &[AiProviderConfig], state: &AiProviderState) -> AiProviderConfig {
    providers
        .iter()
        .find(|provider| provider.id == state.selected_provider_id)
        .cloned()
        .unwrap_or_else(shakespeare_provider)
}

fn build_api_messages(messages: &[DisplayMessage]) -> Vec<ChatMessage> {
    let mut api_messages = vec![ChatMessage {
        role: ChatRole::System,
        content: SYSTEM_PROMPT.to_string(),
    }];
    for message in messages {
        api_messages.push(ChatMessage {
            role: match message.role {
                DisplayRole::User => ChatRole::User,
                DisplayRole::Assistant => ChatRole::Assistant,
            },
            content: message.content.clone(),
        });
    }
    api_messages
}

fn submit_message(
    mut input: Signal<String>,
    selected_model: Signal<String>,
    mut loading: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut messages: Signal<Vec<DisplayMessage>>,
    mut persisted_messages_dirty: Signal<bool>,
    provider: AiProviderConfig,
) {
    if *loading.read() {
        return;
    }
    let text = input.read().trim().to_string();
    let model = selected_model.read().clone();
    if text.is_empty() || model.is_empty() {
        return;
    }

    let user_message = DisplayMessage {
        id: format!("user-{}", crate::platform::timestamp::now_millis()),
        role: DisplayRole::User,
        content: text,
        tool_calls: Vec::new(),
    };
    let mut next_messages = messages.read().clone();
    next_messages.push(user_message);
    messages.set(next_messages.clone());
    persisted_messages_dirty.set(true);
    input.set(String::new());
    error.set(None);
    loading.set(true);

    spawn(async move {
        let base_request = ChatCompletionRequest {
            model: model.clone(),
            messages: build_api_messages(&next_messages),
            tools: provider.supports_tools().then(theme_tool_definitions),
        };

        match send_chat_message(&provider, &base_request).await {
            Ok(response) => {
                apply_chat_response(
                    response,
                    next_messages,
                    model,
                    provider,
                    messages,
                    error,
                    persisted_messages_dirty,
                )
                .await;
            }
            Err(e) => {
                error.set(Some(e));
            }
        }
        loading.set(false);
    });
}

fn theme_tool_definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        tool_type: "function".to_string(),
        function: ToolFunction {
            name: THEME_TOOL_NAME.to_string(),
            description: "Switch the app theme. Supported values are: light, dark, system."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "theme": {
                        "type": "string",
                        "enum": ["light", "dark", "system"],
                        "description": "Theme mode to apply."
                    }
                },
                "required": ["theme"]
            }),
        },
    }]
}

async fn apply_chat_response(
    response: ChatCompletionResponse,
    prior_messages: Vec<DisplayMessage>,
    model: String,
    provider: AiProviderConfig,
    mut messages: Signal<Vec<DisplayMessage>>,
    mut error: Signal<Option<String>>,
    mut persisted_messages_dirty: Signal<bool>,
) {
    let Some(choice) = response.choices.into_iter().next() else {
        error.set(Some(
            "Chat response did not include any choices".to_string(),
        ));
        return;
    };

    let assistant_content = choice.message.content.unwrap_or_default();
    if !provider.supports_tools() {
        let mut next_messages = prior_messages;
        next_messages.push(DisplayMessage {
            id: format!("assistant-{}", crate::platform::timestamp::now_millis()),
            role: DisplayRole::Assistant,
            content: assistant_content,
            tool_calls: Vec::new(),
        });
        messages.set(next_messages);
        persisted_messages_dirty.set(true);
        return;
    }

    if choice.message.tool_calls.is_empty() {
        let mut next_messages = prior_messages;
        next_messages.push(DisplayMessage {
            id: format!("assistant-{}", crate::platform::timestamp::now_millis()),
            role: DisplayRole::Assistant,
            content: assistant_content,
            tool_calls: Vec::new(),
        });
        messages.set(next_messages);
        persisted_messages_dirty.set(true);
        return;
    }

    let executed = execute_tool_calls(&choice.message.tool_calls);
    let mut intermediate_messages = prior_messages.clone();
    intermediate_messages.push(DisplayMessage {
        id: format!(
            "assistant-tool-{}",
            crate::platform::timestamp::now_millis()
        ),
        role: DisplayRole::Assistant,
        content: assistant_content.clone(),
        tool_calls: executed.clone(),
    });
    messages.set(intermediate_messages.clone());
    persisted_messages_dirty.set(true);

    let mut follow_up_messages = build_api_messages(&prior_messages);
    follow_up_messages.push(ChatMessage {
        role: ChatRole::Assistant,
        content: assistant_content,
    });
    for tool in executed {
        follow_up_messages.push(ChatMessage {
            role: ChatRole::User,
            content: format!("[Tool \"{}\" returned: {}]", tool.name, tool.result),
        });
    }

    let follow_up_request = ChatCompletionRequest {
        model,
        messages: follow_up_messages,
        tools: None,
    };

    match send_chat_message(&provider, &follow_up_request).await {
        Ok(follow_up_response) => {
            if let Some(follow_up_choice) = follow_up_response.choices.into_iter().next() {
                let mut final_messages = intermediate_messages;
                final_messages.push(DisplayMessage {
                    id: format!(
                        "assistant-final-{}",
                        crate::platform::timestamp::now_millis()
                    ),
                    role: DisplayRole::Assistant,
                    content: follow_up_choice.message.content.unwrap_or_default(),
                    tool_calls: Vec::new(),
                });
                messages.set(final_messages);
                persisted_messages_dirty.set(true);
            } else {
                error.set(Some(
                    "Follow-up response did not include any choices".to_string(),
                ));
            }
        }
        Err(e) => {
            error.set(Some(e));
        }
    }
}

fn execute_tool_calls(tool_calls: &[ToolCall]) -> Vec<ExecutedToolCall> {
    tool_calls
        .iter()
        .map(|call| ExecutedToolCall {
            id: call.id.clone(),
            name: call.function.name.clone(),
            result: execute_tool_call(&call.function.name, &call.function.arguments),
        })
        .collect()
}

fn execute_tool_call(name: &str, arguments: &str) -> String {
    match name {
        THEME_TOOL_NAME => match serde_json::from_str::<ThemeToolArgs>(arguments) {
            Ok(args) => {
                let theme = match args.theme.trim().to_lowercase().as_str() {
                    "light" => theme_store::Theme::Light,
                    "dark" => theme_store::Theme::Dark,
                    "system" => theme_store::Theme::System,
                    other => {
                        return format!(
                            "{{\"error\":\"Unsupported theme '{}'. Supported values: light, dark, system.\"}}",
                            other
                        );
                    }
                };
                theme_store::set_theme(theme);
                format!("{{\"success\":true,\"theme\":\"{}\"}}", theme.as_str())
            }
            Err(e) => format!("{{\"error\":\"Invalid tool arguments: {}\"}}", e),
        },
        other => format!("{{\"error\":\"Unknown tool: {}\"}}", other),
    }
}

fn persisted_message_from_display(message: DisplayMessage) -> PersistedChatMessage {
    PersistedChatMessage {
        id: message.id,
        role: match message.role {
            DisplayRole::User => PersistedChatRole::User,
            DisplayRole::Assistant => PersistedChatRole::Assistant,
        },
        content: message.content,
        tool_calls: message
            .tool_calls
            .into_iter()
            .map(|call| PersistedToolCall {
                id: call.id,
                name: call.name,
                result: call.result,
            })
            .collect(),
    }
}

fn display_message_from_persisted(message: PersistedChatMessage) -> DisplayMessage {
    DisplayMessage {
        id: message.id,
        role: match message.role {
            PersistedChatRole::User => DisplayRole::User,
            PersistedChatRole::Assistant => DisplayRole::Assistant,
        },
        content: message.content,
        tool_calls: message
            .tool_calls
            .into_iter()
            .map(|call| ExecutedToolCall {
                id: call.id,
                name: call.name,
                result: call.result,
            })
            .collect(),
    }
}

fn persist_selected_model(
    provider_id: String,
    model_id: String,
    mut provider_state: Signal<AiProviderState>,
    mut provider_state_save_generation: Signal<u32>,
    mut error: Signal<Option<String>>,
) {
    let mut next_state = provider_state.read().clone();
    next_state
        .selected_model_by_provider
        .insert(provider_id, model_id);
    provider_state.set(next_state.clone());
    let next_generation = provider_state_save_generation.read().wrapping_add(1);
    provider_state_save_generation.set(next_generation);
    error.set(None);
}

#[cfg(test)]
mod tests {
    use super::history_save_snapshot_key;
    use crate::stores::ai_chat_store::{
        PersistedChatMessage, PersistedChatRole, PersistedToolCall,
    };

    #[test]
    fn history_snapshot_key_is_deterministic() {
        let messages = vec![PersistedChatMessage {
            id: "1".to_string(),
            role: PersistedChatRole::User,
            content: "hello".to_string(),
            tool_calls: vec![],
        }];

        let first = history_save_snapshot_key("account", &messages, &messages);
        let second = history_save_snapshot_key("account", &messages, &messages);

        assert_eq!(first, second);
    }

    #[test]
    fn history_snapshot_key_changes_when_message_content_changes() {
        let messages = vec![PersistedChatMessage {
            id: "1".to_string(),
            role: PersistedChatRole::Assistant,
            content: "before".to_string(),
            tool_calls: vec![PersistedToolCall {
                id: "tool-1".to_string(),
                name: "set_theme".to_string(),
                result: "{\"success\":true}".to_string(),
            }],
        }];
        let mut edited_messages = messages.clone();
        edited_messages[0].content = "after".to_string();

        assert_ne!(
            history_save_snapshot_key("account", &messages, &messages),
            history_save_snapshot_key("account", &edited_messages, &messages),
        );
    }
}
