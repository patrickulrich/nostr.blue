use crate::components::icons::{CameraIcon, SendIcon, SettingsIcon, SparklesIcon, TrashIcon};
use crate::components::{ClientInitializing, ImageInsertData, ImageUploadDialog};
use crate::routes::Route;
use crate::services::ai_chat::{
    generate_images, get_available_models, send_chat_message, AssistantContent,
    ChatCompletionRequest, ChatCompletionResponse, ChatImageUrl, ChatMessage, ChatMessageContent,
    ChatMessagePart, ChatModel, ChatModelKind, ChatRole, ImageGenerationRequest, ToolCall,
    ToolDefinition, ToolFunction,
};
use crate::services::ppq;
use crate::stores::ai_chat_store::{
    self, PersistedChatImage, PersistedChatMessage, PersistedChatRole, PersistedToolCall,
};
use crate::stores::ai_provider_store::{
    self, ppq_provider, resolve_providers, AiProviderConfig, AiProviderState, PpqAccountState,
};
use crate::stores::{nostr_client, theme_store};
use crate::utils::markdown::render_markdown;
use dioxus::core::spawn_forever;
use dioxus::document;
use dioxus::prelude::*;
use serde_json::json;
use std::hash::{Hash, Hasher};

const SYSTEM_PROMPT: &str = "You are Nostrich, an AI assistant inside nostr.blue. Be concise and helpful for the user. Your personality is a fun ostrich that represents the nostr community.";
const THEME_TOOL_NAME: &str = "set_theme";
const AI_CHAT_PROVIDER_PERSISTENCE_ENABLED: bool = true;
const AI_CHAT_HISTORY_LOAD_ENABLED: bool = true;
const AI_CHAT_HISTORY_SAVE_ENABLED: bool = true;
const AI_CHAT_HISTORY_SAVE_FAILURE_COOLDOWN_MS: u64 = 5_000;

#[derive(Clone, Debug, PartialEq)]
struct DisplayMessage {
    id: String,
    role: DisplayRole,
    content: String,
    images: Vec<ChatImage>,
    tool_calls: Vec<ExecutedToolCall>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ChatImage {
    url: String,
    alt: String,
    title: String,
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

#[derive(Clone, Debug, PartialEq)]
struct FailedHistorySnapshot {
    snapshot_key: String,
    failed_at_ms: u64,
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
            message.hash(&mut hasher);
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
    let mut pending_images = use_signal(Vec::<ChatImage>::new);
    let mut show_image_upload = use_signal(|| false);
    let mut provider_state = use_signal(AiProviderState::default);
    let mut providers = use_signal(|| vec![ppq_provider(None)]);
    let mut provider_state_loaded = use_signal(|| false);
    let mut provider_state_loading = use_signal(|| false);
    let mut ppq_bootstrap_loading = use_signal(|| false);
    let mut chat_history_loaded = use_signal(|| false);
    let mut chat_history_loading = use_signal(|| false);
    let mut chat_history_generation = use_signal(|| 0u32);
    let mut persisted_messages_dirty = use_signal(|| false);
    let persisted_messages_save_generation = use_signal(|| 0u32);
    let persisted_messages_save_in_flight = use_signal(|| false);
    let mut persisted_messages_failed_snapshot = use_signal(|| None::<FailedHistorySnapshot>);
    let mut failed_snapshot_retry_generation = use_signal(|| 0u32);
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
                        loaded_state.selected_provider_id = ppq_provider(None).id;
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
        let failed_snapshot = persisted_messages_failed_snapshot.read().clone();
        let Some(failed_snapshot) = failed_snapshot else {
            return;
        };
        let account_key = ai_chat_store::current_account_key();
        let snapshot_key = history_save_snapshot_key(
            &account_key,
            &persisted_messages.read(),
            &initial_loaded_messages.read(),
        );
        if failed_snapshot.snapshot_key != snapshot_key {
            persisted_messages_failed_snapshot.set(None);
            return;
        }
        let generation = failed_snapshot_retry_generation.with_mut(|value| {
            *value = value.wrapping_add(1);
            *value
        });
        let elapsed =
            crate::platform::timestamp::now_millis().saturating_sub(failed_snapshot.failed_at_ms);
        let remaining_ms = AI_CHAT_HISTORY_SAVE_FAILURE_COOLDOWN_MS.saturating_sub(elapsed);
        spawn(async move {
            if remaining_ms > 0 {
                crate::platform::timer::sleep_ms(remaining_ms as u32).await;
            }
            if *failed_snapshot_retry_generation.read() != generation {
                return;
            }
            if persisted_messages_failed_snapshot
                .read()
                .as_ref()
                .is_some_and(|current| {
                    current.snapshot_key == failed_snapshot.snapshot_key
                        && current.failed_at_ms == failed_snapshot.failed_at_ms
                })
            {
                persisted_messages_failed_snapshot.set(None);
            }
        });
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
                        chat_history_loading.set(false);
                        return;
                    }
                    if *persisted_messages_dirty.read() || !messages.read().is_empty() {
                        chat_history_loading.set(false);
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
                        chat_history_loading.set(false);
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
        let computed_snapshot_key = history_save_snapshot_key(
            &account_key,
            &persisted_messages_snapshot,
            &initial_loaded_messages_snapshot,
        );
        let failed_snapshot_cooldown_active = persisted_messages_failed_snapshot
            .read()
            .as_ref()
            .is_some_and(|failed| {
                failed.snapshot_key == computed_snapshot_key
                    && crate::platform::timestamp::now_millis().saturating_sub(failed.failed_at_ms)
                        <= AI_CHAT_HISTORY_SAVE_FAILURE_COOLDOWN_MS
            });

        if !chat_history_ready
            || !persisted_messages_dirty_value
            || persisted_messages_snapshot == initial_loaded_messages_snapshot
            || ai_chat_store::current_account_key() != account_key
            || failed_snapshot_cooldown_active
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
                                persisted_messages_failed_snapshot_signal.set(Some(
                                    FailedHistorySnapshot {
                                        snapshot_key: computed_snapshot_key.clone(),
                                        failed_at_ms: crate::platform::timestamp::now_millis(),
                                    },
                                ));
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
        let provider_state_ready = *provider_state_loaded.read();
        let selected_provider_id = provider_state.read().selected_provider_id.clone();

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

            if provider.requires_setup() {
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
                    let next_model = resolve_selected_model(
                        &selected_model.read(),
                        saved_model.as_deref(),
                        &available_models,
                    );

                    if *provider_models_generation.peek() != local_generation
                        || provider_state.read().selected_provider_id != provider.id
                    {
                        return;
                    }

                    models.set(available_models);

                    if !next_model.is_empty() {
                        selected_model.set(next_model.clone());
                        if saved_model.as_deref() != Some(next_model.as_str()) {
                            persist_selected_model(
                                provider.id.clone(),
                                next_model,
                                provider_state,
                                error,
                            );
                        }
                    } else {
                        selected_model.set(String::new());
                    }
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
    let ppq_blocked = active_provider.requires_setup();
    let active_model = current_model(&models.read(), selected_model.read().as_str());
    let supports_image_attachments = active_model
        .as_ref()
        .map(|model| model.supports_image_input || model.kind == ChatModelKind::Image)
        .unwrap_or(false);
    let can_submit = match active_model.as_ref().map(|model| model.kind) {
        Some(ChatModelKind::Image) => !input.read().trim().is_empty(),
        Some(ChatModelKind::Chat) => {
            !input.read().trim().is_empty() || !pending_images.read().is_empty()
        }
        None => false,
    };

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
                            key: "{active_provider.id}:{models.read().len()}:{selected_model}",
                            class: "h-10 rounded-lg border border-border bg-card px-3 text-sm text-foreground focus:outline-hidden",
                            value: "{selected_model}",
                            disabled: models.read().is_empty() || *loading.read() || ppq_blocked,
                            onchange: move |evt| {
                                let value = evt.value();
                                selected_model.set(value.clone());
                                persist_selected_model(
                                    active_provider.id.clone(),
                                    value,
                                    provider_state,
                                    error,
                                );
                            },
                            if models.read().is_empty() {
                                option {
                                    value: "",
                                    selected: selected_model.read().is_empty(),
                                    if ppq_blocked { "Set up PPQ or switch providers" } else { "Loading models..." }
                                }
                            } else {
                                for model in models.read().iter() {
                                    {
                                        let kind_label = match model.kind {
                                            ChatModelKind::Chat => None,
                                            ChatModelKind::Image => Some("🖼️"),
                                        };
                                        let label = match (kind_label, model.total_cost) {
                                            (Some(kind), Some(0.0)) => {
                                                format!("{} · {} · FREE", model.name, kind)
                                            }
                                            (Some(kind), _) => format!("{} · {}", model.name, kind),
                                            (None, Some(0.0)) => format!("{} · FREE", model.name),
                                            (None, _) => model.name.clone(),
                                        };
                                        rsx! {
                                            option {
                                                key: "{model.id}",
                                                value: "{model.id}",
                                                selected: *selected_model.read() == model.id,
                                                "{label}"
                                            }
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
                    if ppq_blocked {
                        PpqSetupGate {
                            loading: ppq_bootstrap_loading,
                            on_create_account: move |_| {
                                if *ppq_bootstrap_loading.read() {
                                    return;
                                }
                                ppq_bootstrap_loading.set(true);
                                error.set(None);
                                spawn(async move {
                                    match ppq::create_account().await {
                                        Ok(account) => {
                                            let mut next_state = provider_state.read().clone();
                                            next_state.selected_provider_id = ppq_provider(None).id;
                                            next_state.ppq_account = Some(PpqAccountState {
                                                credit_id: account.credit_id,
                                                api_key: account.api_key,
                                                managed_api_key: None,
                                                active_api_key_id: None,
                                            });
                                            if let Err(err) =
                                                ai_provider_store::cache_provider_state(&next_state)
                                            {
                                                error.set(Some(err));
                                                ppq_bootstrap_loading.set(false);
                                                return;
                                            }
                                            providers.set(resolve_providers(&next_state));
                                            provider_state.set(next_state.clone());
                                            match ai_provider_store::save_provider_state(&next_state)
                                                .await
                                            {
                                                Ok(()) => {}
                                                Err(err) => error.set(Some(err)),
                                            }
                                        }
                                        Err(err) => error.set(Some(err)),
                                    }
                                    ppq_bootstrap_loading.set(false);
                                });
                            }
                        }
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
                        if !pending_images.read().is_empty() {
                            div { class: "mb-3 flex flex-wrap gap-3",
                                for (index, image) in pending_images.read().iter().enumerate() {
                                    div {
                                        key: "pending-image-{index}",
                                        class: "relative overflow-hidden rounded-xl border border-border bg-background",
                                        img {
                                            src: "{image.url}",
                                            alt: "{image.alt}",
                                            title: "{image.title}",
                                            class: "h-24 w-24 object-cover",
                                        }
                                        button {
                                            class: "absolute right-1 top-1 inline-flex h-7 w-7 items-center justify-center rounded-full bg-background/90 text-foreground shadow-sm transition hover:bg-background",
                                            title: "Remove image",
                                            onclick: move |_| {
                                                pending_images.with_mut(|images| {
                                                    if index < images.len() {
                                                        images.remove(index);
                                                    }
                                                });
                                            },
                                            "×"
                                        }
                                    }
                                }
                            }
                        }
                        textarea {
                            class: "min-h-[96px] w-full resize-none bg-transparent text-sm text-foreground placeholder:text-muted-foreground focus:outline-hidden disabled:cursor-not-allowed disabled:opacity-60",
                            placeholder: if ppq_blocked {
                                "Create a PPQ account here or open AI settings to switch to a custom provider..."
                            } else if active_model.as_ref().is_some_and(|model| model.kind == ChatModelKind::Image) {
                                "Describe the image you want to generate..."
                            } else if selected_model.read().is_empty() {
                                "Select a model first..."
                            } else {
                                "Send a message..."
                            },
                            value: "{input}",
                            disabled: selected_model.read().is_empty() || *loading.read() || ppq_blocked,
                            oninput: move |evt| input.set(evt.value()),
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter && !evt.modifiers().shift() {
                                    evt.prevent_default();
                                    submit_message(
                                        input,
                                        models,
                                        selected_model,
                                        pending_images,
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
                            div { class: "flex items-center gap-3",
                                if supports_image_attachments {
                                    button {
                                        class: if selected_model.read().is_empty() || *loading.read() || ppq_blocked {
                                            "inline-flex h-11 w-11 items-center justify-center rounded-xl border border-border text-muted-foreground opacity-50"
                                        } else {
                                            "inline-flex h-11 w-11 items-center justify-center rounded-xl border border-border text-muted-foreground transition hover:bg-accent"
                                        },
                                        title: "Attach image",
                                        disabled: selected_model.read().is_empty() || *loading.read() || ppq_blocked,
                                        onclick: move |_| show_image_upload.set(true),
                                        CameraIcon { class: "w-4 h-4".to_string() }
                                    }
                                }
                                p { class: "text-xs text-muted-foreground",
                                    if active_model.as_ref().is_some_and(|model| model.kind == ChatModelKind::Image) {
                                        "Enter to generate. Shift+Enter for newline."
                                    } else {
                                        "Enter to send. Shift+Enter for newline."
                                    }
                                }
                            }
                            button {
                                class: if !can_submit || selected_model.read().is_empty() || *loading.read() || ppq_blocked {
                                    "inline-flex h-11 w-11 items-center justify-center rounded-xl bg-muted text-muted-foreground cursor-not-allowed"
                                } else {
                                    "inline-flex h-11 w-11 items-center justify-center rounded-xl bg-primary text-primary-foreground transition hover:bg-primary/90"
                                },
                                disabled: !can_submit || selected_model.read().is_empty() || *loading.read() || ppq_blocked,
                                onclick: move |_| {
                                    submit_message(
                                        input,
                                        models,
                                        selected_model,
                                        pending_images,
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
            ImageUploadDialog {
                open: show_image_upload,
                on_insert: move |data: ImageInsertData| {
                    pending_images.with_mut(|images| images.push(data.into()));
                },
            }
        }
    }
}

#[component]
fn PpqSetupGate(loading: Signal<bool>, on_create_account: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div { class: "flex min-h-[50vh] items-center justify-center p-6",
            div { class: "max-w-md w-full rounded-2xl border border-border bg-card p-8 text-center shadow-sm",
                div { class: "mx-auto mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-primary/10 text-primary",
                    SparklesIcon { class: "w-7 h-7".to_string() }
                }
                h2 { class: "text-2xl font-semibold", "Set Up PPQ to Use AI Chat" }
                p { class: "mt-2 text-sm text-muted-foreground",
                    "PPQ is the default built-in AI provider. Create a PPQ account here, or open AI settings and switch to your own custom OpenAI-compatible provider."
                }
                div { class: "mt-5 flex flex-col gap-3 sm:flex-row sm:justify-center",
                    button {
                        class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-60",
                        disabled: *loading.read(),
                        onclick: move |evt| on_create_account.call(evt),
                        if *loading.read() { "Creating PPQ Account..." } else { "Create PPQ Account" }
                    }
                    button {
                        class: "rounded-lg border border-border px-4 py-2 text-sm transition hover:bg-accent",
                        onclick: move |_| {
                            navigator().push(Route::SettingsAi {});
                        },
                        "Use Custom Provider Instead"
                    }
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
                if !message.images.is_empty() {
                    div { class: if message.content.is_empty() { "space-y-3" } else { "mt-3 space-y-3" },
                        for (index, image) in message.images.iter().enumerate() {
                            a {
                                key: "{message.id}-image-{index}",
                                href: "{image.url}",
                                target: "_blank",
                                rel: "noreferrer noopener",
                                class: "block overflow-hidden rounded-xl border border-border bg-background transition hover:opacity-95",
                                img {
                                    src: "{image.url}",
                                    alt: "{image.alt}",
                                    title: "{image.title}",
                                    class: "max-h-96 w-full object-contain bg-background",
                                }
                            }
                        }
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
        .unwrap_or_else(|| ppq_provider(state.ppq_account.as_ref()))
}

fn current_model(models: &[ChatModel], selected_model_id: &str) -> Option<ChatModel> {
    models
        .iter()
        .find(|model| model.id == selected_model_id)
        .cloned()
}

fn has_image_models(models: &[ChatModel]) -> bool {
    models
        .iter()
        .any(|model| model.kind == ChatModelKind::Image)
}

fn looks_like_image_generation_prompt(text: &str) -> bool {
    let normalized = text.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    [
        "generate an image",
        "generate a picture",
        "generate a photo",
        "generate art",
        "create an image",
        "create a picture",
        "create a photo",
        "make an image",
        "make a picture",
        "make a photo",
        "draw ",
        "render ",
        "illustrate ",
        "image of",
        "picture of",
        "photo of",
        "logo for",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn build_api_messages(messages: &[DisplayMessage]) -> Vec<ChatMessage> {
    let mut api_messages = vec![ChatMessage {
        role: ChatRole::System,
        content: ChatMessageContent::Text(SYSTEM_PROMPT.to_string()),
    }];
    for message in messages {
        let content = if !message.images.is_empty() {
            let mut parts = Vec::new();
            if !message.content.trim().is_empty() {
                parts.push(ChatMessagePart::Text {
                    text: message.content.clone(),
                });
            }
            parts.extend(message.images.iter().cloned().map(|image| {
                ChatMessagePart::ImageUrl {
                    image_url: ChatImageUrl { url: image.url },
                }
            }));
            ChatMessageContent::Parts(parts)
        } else {
            ChatMessageContent::Text(message.content.clone())
        };
        api_messages.push(ChatMessage {
            role: match message.role {
                DisplayRole::User => ChatRole::User,
                DisplayRole::Assistant => ChatRole::Assistant,
            },
            content,
        });
    }
    api_messages
}

#[allow(clippy::too_many_arguments)]
fn submit_message(
    mut input: Signal<String>,
    models: Signal<Vec<ChatModel>>,
    selected_model: Signal<String>,
    mut pending_images: Signal<Vec<ChatImage>>,
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
    let Some(active_model) = current_model(&models.read(), &model) else {
        return;
    };
    let image_models_available = has_image_models(&models.read());
    let attached_images = pending_images.read().clone();
    if model.is_empty()
        || (text.is_empty() && attached_images.is_empty())
        || (active_model.kind == ChatModelKind::Image && text.is_empty())
    {
        return;
    }

    if active_model.kind != ChatModelKind::Image
        && attached_images.is_empty()
        && image_models_available
        && looks_like_image_generation_prompt(&text)
    {
        error.set(Some(
            "The selected model is text-only. Choose an IMAGE model from the model picker to generate images.".to_string(),
        ));
        return;
    }

    if active_model.kind == ChatModelKind::Image && attached_images.len() > 1 {
        error.set(Some(
            "Image generation currently supports only one reference image per request."
                .to_string(),
        ));
        return;
    }

    let user_message = DisplayMessage {
        id: format!("user-{}", crate::platform::timestamp::now_millis()),
        role: DisplayRole::User,
        content: text.clone(),
        images: attached_images.clone(),
        tool_calls: Vec::new(),
    };
    let mut next_messages = messages.read().clone();
    next_messages.push(user_message);
    messages.set(next_messages.clone());
    persisted_messages_dirty.set(true);
    input.set(String::new());
    pending_images.set(Vec::new());
    error.set(None);
    loading.set(true);

    spawn(async move {
        match active_model.kind {
            ChatModelKind::Chat => {
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
            }
            ChatModelKind::Image => {
                let request = ImageGenerationRequest {
                    model: model.clone(),
                    prompt: text,
                    image_url: attached_images.first().map(|image| image.url.clone()),
                };

                match generate_images(&provider, &request).await {
                    Ok(response) => {
                        let mut final_messages = next_messages;
                        final_messages.push(DisplayMessage {
                            id: format!(
                                "assistant-image-{}",
                                crate::platform::timestamp::now_millis()
                            ),
                            role: DisplayRole::Assistant,
                            content: String::new(),
                            images: response
                                .images
                                .into_iter()
                                .map(|image| ChatImage {
                                    url: image.url,
                                    alt: "Generated image".to_string(),
                                    title: String::new(),
                                })
                                .collect(),
                            tool_calls: Vec::new(),
                        });
                        messages.set(final_messages);
                        persisted_messages_dirty.set(true);
                    }
                    Err(e) => {
                        error.set(Some(e));
                    }
                }
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

    let (assistant_content, assistant_images) = extract_assistant_content(choice.message.content);
    if !provider.supports_tools() {
        let mut next_messages = prior_messages;
        next_messages.push(DisplayMessage {
            id: format!("assistant-{}", crate::platform::timestamp::now_millis()),
            role: DisplayRole::Assistant,
            content: assistant_content,
            images: assistant_images,
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
            images: assistant_images,
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
        images: assistant_images.clone(),
        tool_calls: executed.clone(),
    });
    messages.set(intermediate_messages.clone());
    persisted_messages_dirty.set(true);

    let mut follow_up_messages = build_api_messages(&prior_messages);
    let follow_up_assistant_content = if assistant_images.is_empty() {
        ChatMessageContent::Text(assistant_content)
    } else {
        let mut parts = Vec::new();
        if !assistant_content.trim().is_empty() {
            parts.push(ChatMessagePart::Text {
                text: assistant_content,
            });
        }
        parts.extend(assistant_images.into_iter().map(|image| ChatMessagePart::ImageUrl {
            image_url: ChatImageUrl { url: image.url },
        }));
        ChatMessageContent::Parts(parts)
    };
    follow_up_messages.push(ChatMessage {
        role: ChatRole::Assistant,
        content: follow_up_assistant_content,
    });
    for tool in executed {
        follow_up_messages.push(ChatMessage {
            role: ChatRole::User,
            content: ChatMessageContent::Text(format!(
                "[Tool \"{}\" returned: {}]",
                tool.name, tool.result
            )),
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
                let (follow_up_content, follow_up_images) =
                    extract_assistant_content(follow_up_choice.message.content);
                let mut final_messages = intermediate_messages;
                final_messages.push(DisplayMessage {
                    id: format!(
                        "assistant-final-{}",
                        crate::platform::timestamp::now_millis()
                    ),
                    role: DisplayRole::Assistant,
                    content: follow_up_content,
                    images: follow_up_images,
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

fn extract_assistant_content(content: Option<AssistantContent>) -> (String, Vec<ChatImage>) {
    match content {
        Some(AssistantContent::Text(text)) => (text, Vec::new()),
        Some(AssistantContent::Parts(parts)) => {
            let mut text_parts = Vec::new();
            let mut images = Vec::new();
            for part in parts {
                match part {
                    crate::services::ai_chat::AssistantContentPart::Text { text } => {
                        if !text.is_empty() {
                            text_parts.push(text);
                        }
                    }
                    crate::services::ai_chat::AssistantContentPart::ImageUrl { image_url } => {
                        images.push(ChatImage {
                            url: image_url.url,
                            alt: "Generated image".to_string(),
                            title: String::new(),
                        });
                    }
                }
            }
            (text_parts.join("\n\n"), images)
        }
        None => (String::new(), Vec::new()),
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
        images: message
            .images
            .into_iter()
            .map(|image| PersistedChatImage {
                url: image.url,
                alt: image.alt,
                title: image.title,
            })
            .collect(),
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
        images: message
            .images
            .into_iter()
            .map(|image| ChatImage {
                url: image.url,
                alt: image.alt,
                title: image.title,
            })
            .collect(),
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
    mut error: Signal<Option<String>>,
) {
    let mut next_state = provider_state.read().clone();
    next_state
        .selected_model_by_provider
        .insert(provider_id.clone(), model_id.clone());
    provider_state.set(next_state.clone());
    match ai_provider_store::cache_provider_state(&next_state) {
        Ok(()) => error.set(None),
        Err(e) => {
            error.set(Some(e));
            return;
        }
    }

    if !AI_CHAT_PROVIDER_PERSISTENCE_ENABLED {
        return;
    }

    if let Some(snapshot) = ai_provider_store::queue_provider_state_save(next_state) {
        spawn_forever(async move {
            let mut next_snapshot = Some(snapshot);
            while let Some(current_snapshot) = next_snapshot {
                if let Err(e) = ai_provider_store::save_provider_state(&current_snapshot).await {
                    error.set(Some(e));
                }
                next_snapshot = ai_provider_store::finish_provider_state_save();
            }
        });
    }
}

fn resolve_selected_model(
    current_model_id: &str,
    saved_model_id: Option<&str>,
    available_models: &[ChatModel],
) -> String {
    if available_models
        .iter()
        .any(|model| model.id == current_model_id)
    {
        return current_model_id.to_string();
    }

    if let Some(saved_model_id) = saved_model_id {
        if available_models
            .iter()
            .any(|model| model.id == saved_model_id)
        {
            return saved_model_id.to_string();
        }
    }

    available_models
        .first()
        .map(|model| model.id.clone())
        .unwrap_or_default()
}

impl From<ImageInsertData> for ChatImage {
    fn from(value: ImageInsertData) -> Self {
        Self {
            url: value.url,
            alt: value.alt,
            title: value.title,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_api_messages, history_save_snapshot_key, resolve_selected_model, ChatImage,
        DisplayMessage, DisplayRole,
    };
    use crate::services::ai_chat::{
        ChatMessageContent, ChatMessagePart, ChatModel, ChatModelKind,
    };
    use crate::stores::ai_chat_store::{
        PersistedChatMessage, PersistedChatRole, PersistedToolCall,
    };

    #[test]
    fn history_snapshot_key_is_deterministic() {
        let messages = vec![PersistedChatMessage {
            id: "1".to_string(),
            role: PersistedChatRole::User,
            content: "hello".to_string(),
            images: vec![],
            tool_calls: vec![],
        }];
        let initial_loaded_messages = messages.clone();

        let first = history_save_snapshot_key("account", &messages, &initial_loaded_messages);
        let second = history_save_snapshot_key(
            "account",
            &messages.clone(),
            &initial_loaded_messages.clone(),
        );

        assert_eq!(first, second);
    }

    #[test]
    fn history_snapshot_key_changes_when_message_content_changes() {
        let messages = vec![PersistedChatMessage {
            id: "1".to_string(),
            role: PersistedChatRole::Assistant,
            content: "before".to_string(),
            images: vec![],
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

    #[test]
    fn history_snapshot_key_changes_when_tool_calls_change() {
        let messages = vec![PersistedChatMessage {
            id: "1".to_string(),
            role: PersistedChatRole::Assistant,
            content: "hello".to_string(),
            images: vec![],
            tool_calls: vec![],
        }];
        let mut with_tool_call = messages.clone();
        with_tool_call[0].tool_calls.push(PersistedToolCall {
            id: "tool-1".to_string(),
            name: "set_theme".to_string(),
            result: "{\"success\":true}".to_string(),
        });

        assert_ne!(
            history_save_snapshot_key("account", &messages, &messages),
            history_save_snapshot_key("account", &with_tool_call, &messages),
        );
    }

    #[test]
    fn history_snapshot_key_changes_when_account_key_changes() {
        let messages = vec![PersistedChatMessage {
            id: "1".to_string(),
            role: PersistedChatRole::Assistant,
            content: "hello".to_string(),
            images: vec![],
            tool_calls: vec![PersistedToolCall {
                id: "tool-1".to_string(),
                name: "set_theme".to_string(),
                result: "{\"success\":true}".to_string(),
            }],
        }];

        assert_ne!(
            history_save_snapshot_key("account-a", &messages, &messages),
            history_save_snapshot_key("account-b", &messages, &messages),
        );
    }

    #[test]
    fn history_snapshot_key_changes_when_initial_loaded_messages_change() {
        let messages = vec![PersistedChatMessage {
            id: "1".to_string(),
            role: PersistedChatRole::User,
            content: "hello".to_string(),
            images: vec![],
            tool_calls: vec![],
        }];
        let mut different_initial_loaded_messages = messages.clone();
        different_initial_loaded_messages.push(PersistedChatMessage {
            id: "2".to_string(),
            role: PersistedChatRole::Assistant,
            content: "welcome".to_string(),
            images: vec![],
            tool_calls: vec![],
        });

        assert_ne!(
            history_save_snapshot_key("account", &messages, &messages),
            history_save_snapshot_key("account", &messages, &different_initial_loaded_messages,),
        );
    }

    #[test]
    fn resolve_selected_model_prefers_current_when_valid() {
        let models = vec![
            ChatModel {
                id: "model-a".to_string(),
                name: "Model A".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: true,
                total_cost: None,
            },
            ChatModel {
                id: "model-b".to_string(),
                name: "Model B".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: true,
                total_cost: None,
            },
        ];

        assert_eq!(
            resolve_selected_model("model-b", Some("model-a"), &models),
            "model-b"
        );
    }

    #[test]
    fn resolve_selected_model_falls_back_to_saved_model() {
        let models = vec![
            ChatModel {
                id: "model-a".to_string(),
                name: "Model A".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: true,
                total_cost: None,
            },
            ChatModel {
                id: "model-b".to_string(),
                name: "Model B".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: true,
                total_cost: None,
            },
        ];

        assert_eq!(
            resolve_selected_model("missing", Some("model-b"), &models),
            "model-b"
        );
    }

    #[test]
    fn resolve_selected_model_defaults_to_first_available_model() {
        let models = vec![
            ChatModel {
                id: "model-a".to_string(),
                name: "Model A".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: true,
                total_cost: None,
            },
            ChatModel {
                id: "model-b".to_string(),
                name: "Model B".to_string(),
                description: String::new(),
                kind: ChatModelKind::Chat,
                supports_image_input: true,
                total_cost: None,
            },
        ];

        assert_eq!(
            resolve_selected_model("missing", Some("also-missing"), &models),
            "model-a"
        );
    }

    #[test]
    fn resolve_selected_model_returns_empty_without_available_models() {
        assert!(resolve_selected_model("missing", Some("also-missing"), &[]).is_empty());
    }

    #[test]
    fn build_api_messages_keeps_assistant_images_as_parts() {
        let api_messages = build_api_messages(&[DisplayMessage {
            id: "1".to_string(),
            role: DisplayRole::Assistant,
            content: "See attached".to_string(),
            images: vec![ChatImage {
                url: "https://cdn.example.com/image.png".to_string(),
                alt: String::new(),
                title: String::new(),
            }],
            tool_calls: vec![],
        }]);

        assert_eq!(api_messages.len(), 2);
        match &api_messages[1].content {
            ChatMessageContent::Parts(parts) => {
                assert_eq!(
                    parts[0],
                    ChatMessagePart::Text {
                        text: "See attached".to_string()
                    }
                );
                assert_eq!(
                    parts[1],
                    ChatMessagePart::ImageUrl {
                        image_url: crate::services::ai_chat::ChatImageUrl {
                            url: "https://cdn.example.com/image.png".to_string()
                        }
                    }
                );
            }
            other => panic!("expected multipart assistant message, got {other:?}"),
        }
    }
}
