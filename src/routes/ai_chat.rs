use crate::components::icons::{CameraIcon, SendIcon, SettingsIcon, SparklesIcon, TrashIcon};
use crate::components::{
    ClientInitializing, ImageInsertData, ImageUploadDialog, Sheet, SheetContent, SheetDescription,
    SheetFooter, SheetHeader, SheetSide, SheetTitle,
};
use crate::routes::Route;
use crate::services::ai_chat::{
    generate_images, get_available_models, send_chat_message, AssistantContent,
    ChatCompletionRequest, ChatCompletionResponse, ChatImageUrl, ChatMessage, ChatMessageContent,
    ChatMessagePart,     ChatModel, ChatModelKind, ChatRole, ImageGenerationRequest, ToolCall,
};
use crate::services::ppq;
use crate::stores::ai_chat_seed_store;
use crate::stores::ai_chat_store::{
    self, PersistedChatImage, PersistedChatMessage, PersistedChatRole, PersistedChatState,
    PersistedConversation, PersistedToolCall,
};
use crate::stores::ai_provider_store::{
    self, ppq_provider, resolve_providers, AiProviderConfig, AiProviderState, PpqAccountState,
    PROVIDER_STATE_SAVE_EVENT,
};
use crate::stores::nostr_client;
use crate::utils::markdown::render_markdown;
use dioxus::document;
use dioxus::prelude::*;
use std::hash::{Hash, Hasher};

const SYSTEM_PROMPT: &str = "You are Nostrich, an AI assistant inside nostr.blue, a Nostr client. Be concise and helpful. Your personality is a fun ostrich that represents the nostr community. You have access to nostr tools to fetch profiles, notes, events, interactions, search content, and more. When the user shares or asks about a note, profile, or any Nostr entity, proactively use the available tools to provide enriched context.";
const AI_CHAT_PROVIDER_PERSISTENCE_ENABLED: bool = true;
const AI_CHAT_HISTORY_LOAD_ENABLED: bool = true;
const AI_CHAT_HISTORY_SAVE_ENABLED: bool = true;
const AI_CHAT_HISTORY_SAVE_FAILURE_COOLDOWN_MS: u64 = 5_000;
const DEFAULT_CONVERSATION_TITLE: &str = "New Chat";

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

fn is_current_chat_history_request(
    generation_signal: Signal<u32>,
    account_key: &str,
    captured_generation: u32,
) -> bool {
    *generation_signal.read() == captured_generation
        && ai_chat_store::current_account_key() == account_key
}

fn should_suggest_image_model(
    active_model: &ChatModel,
    attached_images: &[ChatImage],
    image_models_available: bool,
    text: &str,
) -> bool {
    active_model.kind != ChatModelKind::Image
        && attached_images.is_empty()
        && image_models_available
        && looks_like_image_generation_prompt(text)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PendingProviderSaveAction {
    #[default]
    None,
    BootstrapPpq,
}

fn history_save_snapshot_key(
    account_key: &str,
    persisted_state: &PersistedChatState,
    initial_loaded_state: &PersistedChatState,
) -> String {
    fn hash_state(state: &PersistedChatState) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        state.hash(&mut hasher);
        hasher.finish()
    }

    format!(
        "{}:{:016x}:{:016x}",
        account_key,
        hash_state(persisted_state),
        hash_state(initial_loaded_state),
    )
}

#[component]
pub fn AIChat() -> Element {
    let mut messages = use_signal(Vec::<DisplayMessage>::new);
    let mut conversations = use_signal(Vec::<PersistedConversation>::new);
    let mut active_conversation_id = use_signal(|| None::<String>);
    let mut input = use_signal(String::new);
    let loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut compose_notice = use_signal(|| None::<String>);
    let mut models = use_signal(Vec::<ChatModel>::new);
    let mut selected_model = use_signal(String::new);
    let mut pending_images = use_signal(Vec::<ChatImage>::new);
    let mut show_image_upload = use_signal(|| false);
    let mut show_conversation_sheet = use_signal(|| false);
    let mut editing_conversation_id = use_signal(|| None::<String>);
    let mut editing_conversation_title = use_signal(String::new);
    let mut confirm_delete_conversation_id = use_signal(|| None::<String>);
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
    let mut initial_loaded_state = use_signal(PersistedChatState::default);
    let messages_container_id = use_signal(|| "ai-chat-messages".to_string());
    let mut last_account_key = use_signal(|| None::<String>);
    let mut pending_provider_save_snapshot = use_signal(|| None::<AiProviderState>);
    let mut pending_provider_save_min_event_id = use_signal(|| 0u64);
    let mut pending_provider_save_action = use_signal(PendingProviderSaveAction::default);
    let persisted_messages = use_memo(move || {
        messages
            .read()
            .iter()
            .cloned()
            .map(persisted_message_from_display)
            .collect::<Vec<PersistedChatMessage>>()
    });
    let persisted_chat_state = use_memo(move || PersistedChatState {
        conversations: conversations.read().clone(),
        active_conversation_id: active_conversation_id.read().clone(),
    });
    let active_conversation_id_value = active_conversation_id.read().clone();
    let persisted_messages_value = persisted_messages.read().clone();

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
        let pending_snapshot = pending_provider_save_snapshot.read().clone();
        let Some(pending_snapshot) = pending_snapshot else {
            return;
        };
        let Some(event) = PROVIDER_STATE_SAVE_EVENT.read().clone() else {
            return;
        };
        if event.event_id <= *pending_provider_save_min_event_id.read()
            || event.snapshot != pending_snapshot
        {
            return;
        }

        let action = *pending_provider_save_action.read();
        pending_provider_save_snapshot.set(None);
        pending_provider_save_min_event_id.set(event.event_id);
        pending_provider_save_action.set(PendingProviderSaveAction::None);

        match event.result {
            Ok(()) => {
                if matches!(action, PendingProviderSaveAction::BootstrapPpq) {
                    ppq_bootstrap_loading.set(false);
                }
            }
            Err(err) => {
                error.set(Some(err));
                if matches!(action, PendingProviderSaveAction::BootstrapPpq) {
                    ppq_bootstrap_loading.set(false);
                }
            }
        }
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

    use_effect(use_reactive(
        (&active_conversation_id_value, &persisted_messages_value),
        move |(active_id, persisted_messages)| {
            if active_id.is_none() {
                return;
            }

            conversations.with_mut(|items| {
                sync_active_conversation_state(items, active_id.as_deref(), &persisted_messages);
            });
        },
    ));

    use_effect(move || {
        let selected = selected_model.read().clone();
        let active_model = current_model(&models.read(), &selected);
        if active_model
            .as_ref()
            .is_some_and(|model| model.supports_image_input)
            || pending_images.read().is_empty()
        {
            return;
        }
        pending_images.set(Vec::new());
    });

    use_effect(move || {
        let account_key = ai_chat_store::current_account_key();
        if last_account_key.read().as_deref() == Some(account_key.as_str()) {
            return;
        }
        last_account_key.set(Some(account_key));
        messages.set(Vec::new());
        conversations.set(Vec::new());
        active_conversation_id.set(None);
        chat_history_loaded.set(false);
        chat_history_loading.set(false);
        persisted_messages_dirty.set(false);
        initial_loaded_state.set(PersistedChatState::default());
        show_conversation_sheet.set(false);
        editing_conversation_id.set(None);
        editing_conversation_title.set(String::new());
        confirm_delete_conversation_id.set(None);
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
            &persisted_chat_state.read(),
            &initial_loaded_state.read(),
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
            match ai_chat_store::load_chat_state(&account_key).await {
                Ok(state) => {
                    if !is_current_chat_history_request(
                        chat_history_generation,
                        &account_key,
                        generation,
                    ) {
                        return;
                    }
                    if *persisted_messages_dirty.read() || !messages.read().is_empty() {
                        if is_current_chat_history_request(
                            chat_history_generation,
                            &account_key,
                            generation,
                        ) {
                            initial_loaded_state.set(state);
                            chat_history_loaded.set(true);
                            chat_history_loading.set(false);
                        }
                        return;
                    }
                    let resolved_active_id = resolve_active_conversation_id(&state);
                    let active_messages = resolved_active_id
                        .as_deref()
                        .and_then(|id| {
                            state
                                .conversations
                                .iter()
                                .find(|conversation| conversation.id == id)
                        })
                        .map(|conversation| {
                            conversation
                                .messages
                                .iter()
                                .cloned()
                                .map(display_message_from_persisted)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    initial_loaded_state.set(PersistedChatState {
                        conversations: state.conversations.clone(),
                        active_conversation_id: resolved_active_id.clone(),
                    });
                    persisted_messages_dirty.set(false);
                    conversations.set(state.conversations);
                    active_conversation_id.set(resolved_active_id);
                    messages.set(active_messages);
                }
                Err(e) => {
                    if !is_current_chat_history_request(
                        chat_history_generation,
                        &account_key,
                        generation,
                    ) {
                        return;
                    }
                    error.set(Some(e));
                }
            }
            if is_current_chat_history_request(chat_history_generation, &account_key, generation) {
                chat_history_loaded.set(true);
                chat_history_loading.set(false);
            }
        });
    });

    use_effect(move || {
        if !*chat_history_loaded.read() {
            return;
        }

        let Some(seed_payload) = ai_chat_seed_store::take_ai_chat_seed() else {
            return;
        };

        let merged = merged_conversations_snapshot(
            &conversations.read(),
            active_conversation_id.read().as_deref(),
            &persisted_messages.read(),
        );
        let new_conversation =
            seeded_conversation(seed_payload.title_hint.as_deref(), &seed_payload.message);
        let next_messages = new_conversation
            .messages
            .iter()
            .cloned()
            .map(display_message_from_persisted)
            .collect::<Vec<_>>();
        let next_active_id = new_conversation.id.clone();
        let mut next_conversations = merged;
        next_conversations.push(new_conversation);

        conversations.set(next_conversations);
        active_conversation_id.set(Some(next_active_id));
        messages.set(next_messages);
        input.set(String::new());
        pending_images.set(Vec::new());
        compose_notice.set(None);
        error.set(None);
        show_conversation_sheet.set(false);
        editing_conversation_id.set(None);
        editing_conversation_title.set(String::new());
        confirm_delete_conversation_id.set(None);
        persisted_messages_dirty.set(true);
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
        let initial_loaded_state_snapshot = initial_loaded_state.read().clone();
        let persisted_state_snapshot = persisted_chat_state.read().clone();
        let computed_snapshot_key = history_save_snapshot_key(
            &account_key,
            &persisted_state_snapshot,
            &initial_loaded_state_snapshot,
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
            || persisted_state_snapshot == initial_loaded_state_snapshot
            || ai_chat_store::current_account_key() != account_key
            || failed_snapshot_cooldown_active
        {
            return;
        }

        let mut persisted_messages_dirty_signal = persisted_messages_dirty;
        let mut persisted_messages_save_generation_signal = persisted_messages_save_generation;
        let mut persisted_messages_save_in_flight_signal = persisted_messages_save_in_flight;
        let persisted_state_signal = persisted_chat_state;
        let chat_history_generation_signal = chat_history_generation;
        let mut initial_loaded_state_signal = initial_loaded_state;
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

                let result = if persisted_state_snapshot.conversations.is_empty() {
                    ai_chat_store::clear_chat_state(&account_key).await
                } else {
                    ai_chat_store::save_chat_state(&account_key, &persisted_state_snapshot).await
                };

                match result {
                    Ok(()) => {
                        if *persisted_messages_save_generation_signal.read() == generation
                            && *chat_history_generation_signal.read() == chat_generation
                            && ai_chat_store::current_account_key() == account_key
                        {
                            let latest_persisted_state = persisted_state_signal.read().clone();
                            if latest_persisted_state == persisted_state_snapshot {
                                persisted_messages_dirty_signal.set(false);
                                initial_loaded_state_signal.set(persisted_state_snapshot.clone());
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
                                pending_provider_save_snapshot,
                                pending_provider_save_min_event_id,
                                pending_provider_save_action,
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
        .map(|model| model.supports_image_input)
        .unwrap_or(false);
    let has_pending_images = !pending_images.read().is_empty();
    let can_submit = match active_model.as_ref().map(|model| model.kind) {
        Some(ChatModelKind::Image) => !input.read().trim().is_empty(),
        Some(ChatModelKind::Chat) => {
            !input.read().trim().is_empty()
                || (has_pending_images
                    && active_model
                        .as_ref()
                        .is_some_and(|m| m.supports_image_input))
        }
        None => false,
    };
    let conversation_items = sorted_conversations(&conversations.read());
    let active_conversation_title = active_conversation_id
        .read()
        .as_deref()
        .and_then(|id| {
            conversation_items
                .iter()
                .find(|conversation| conversation.id == id)
        })
        .map(|conversation| conversation.title.clone());

    let provider_for_keydown = active_provider.clone();
    let provider_for_click = active_provider.clone();

    rsx! {
        div { class: "min-h-screen flex flex-col bg-background",
            Sheet {
                open: show_conversation_sheet(),
                on_open_change: move |open| {
                    show_conversation_sheet.set(open);
                    if !open {
                        editing_conversation_id.set(None);
                        editing_conversation_title.set(String::new());
                        confirm_delete_conversation_id.set(None);
                    }
                },
                SheetContent {
                    side: SheetSide::Left,
                    class: "border-r border-border bg-background",
                    SheetHeader {
                        SheetTitle { "Conversations" }
                        SheetDescription {
                            if conversation_items.is_empty() {
                                "Create separate chats for different topics and switch between them later."
                            } else {
                                "{conversation_items.len()} saved conversation(s)"
                            }
                        }
                    }
                    div { class: "flex-1 overflow-y-auto px-3 pb-3",
                        if conversation_items.is_empty() {
                            div { class: "rounded-xl border border-dashed border-border bg-card/60 px-4 py-5 text-sm text-muted-foreground",
                                "No saved conversations yet."
                            }
                        } else {
                            for conversation in conversation_items.iter() {
                                {
                                    let is_active = active_conversation_id.read().as_deref()
                                        == Some(conversation.id.as_str());
                                    let is_editing = editing_conversation_id.read().as_deref()
                                        == Some(conversation.id.as_str());
                                    let confirm_delete = confirm_delete_conversation_id.read().as_deref()
                                        == Some(conversation.id.as_str());
                                    let preview = conversation_preview(conversation);
                                    let timestamp = format_relative_timestamp(conversation.updated_at_ms);
                                    let select_conversation_id = conversation.id.clone();
                                    let rename_keydown_conversation_id = conversation.id.clone();
                                    let rename_blur_conversation_id = conversation.id.clone();
                                    let begin_rename_conversation_id = conversation.id.clone();
                                    let begin_delete_conversation_id = conversation.id.clone();
                                    let delete_conversation_id = conversation.id.clone();
                                    let conversation_title = conversation.title.clone();
                                    let conversation_messages = conversation.messages.clone();

                                    rsx! {
                                        div {
                                            key: "{conversation.id}",
                                            class: if is_active {
                                                "mb-3 rounded-2xl border border-primary/30 bg-primary/5 p-3"
                                            } else {
                                                "mb-3 rounded-2xl border border-border bg-card p-3"
                                            },
                                            div { class: "flex items-start gap-3",
                                                button {
                                                    class: "min-w-0 flex-1 text-left",
                                                    onclick: move |_| {
                                                        let merged = merged_conversations_snapshot(
                                                            &conversations.read(),
                                                            active_conversation_id.read().as_deref(),
                                                            &persisted_messages.read(),
                                                        );
                                                        let next_messages = conversation_messages
                                                            .iter()
                                                            .cloned()
                                                            .map(display_message_from_persisted)
                                                            .collect::<Vec<_>>();
                                                        conversations.set(merged);
                                                        active_conversation_id.set(Some(select_conversation_id.clone()));
                                                        messages.set(next_messages);
                                                        input.set(String::new());
                                                        pending_images.set(Vec::new());
                                                        compose_notice.set(None);
                                                        error.set(None);
                                                        persisted_messages_dirty.set(true);
                                                        show_conversation_sheet.set(false);
                                                        confirm_delete_conversation_id.set(None);
                                                        editing_conversation_id.set(None);
                                                        editing_conversation_title.set(String::new());
                                                    },
                                                    if is_editing {
                                                        input {
                                                            class: "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm font-medium text-foreground focus:outline-hidden",
                                                            value: "{editing_conversation_title}",
                                                            oninput: move |evt| editing_conversation_title.set(evt.value()),
                                                            onkeydown: move |evt| {
                                                                if evt.key() == Key::Enter {
                                                                    evt.prevent_default();
                                                                    rename_conversation(
                                                                        conversations,
                                                                        &rename_keydown_conversation_id,
                                                                        editing_conversation_title.read().as_str(),
                                                                    );
                                                                    editing_conversation_id.set(None);
                                                                    editing_conversation_title.set(String::new());
                                                                    persisted_messages_dirty.set(true);
                                                                } else if evt.key() == Key::Escape {
                                                                    editing_conversation_id.set(None);
                                                                    editing_conversation_title.set(String::new());
                                                                }
                                                            },
                                                            onblur: move |_| {
                                                                rename_conversation(
                                                                    conversations,
                                                                    &rename_blur_conversation_id,
                                                                    editing_conversation_title.read().as_str(),
                                                                );
                                                                editing_conversation_id.set(None);
                                                                editing_conversation_title.set(String::new());
                                                                persisted_messages_dirty.set(true);
                                                            },
                                                        }
                                                    } else {
                                                        div { class: "flex items-center gap-2",
                                                            p { class: "truncate text-sm font-medium text-foreground", "{conversation.title}" }
                                                            if is_active {
                                                                span { class: "rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary", "Active" }
                                                            }
                                                        }
                                                        p { class: "mt-1 line-clamp-2 text-xs text-muted-foreground", "{preview}" }
                                                        p { class: "mt-2 text-[11px] uppercase tracking-[0.08em] text-muted-foreground", "{timestamp}" }
                                                    }
                                                }
                                                div { class: "flex shrink-0 items-center gap-2",
                                                    button {
                                                        class: "inline-flex h-9 w-9 items-center justify-center rounded-lg border border-border text-muted-foreground transition hover:bg-accent",
                                                        title: "Rename conversation",
                                                        onclick: move |_| {
                                                            editing_conversation_id.set(Some(begin_rename_conversation_id.clone()));
                                                            editing_conversation_title.set(conversation_title.clone());
                                                            confirm_delete_conversation_id.set(None);
                                                        },
                                                        "✎"
                                                    }
                                                    button {
                                                        class: "inline-flex h-9 w-9 items-center justify-center rounded-lg border border-border text-muted-foreground transition hover:bg-accent",
                                                        title: "Delete conversation",
                                                        onclick: move |_| {
                                                            confirm_delete_conversation_id.set(Some(begin_delete_conversation_id.clone()));
                                                            editing_conversation_id.set(None);
                                                            editing_conversation_title.set(String::new());
                                                        },
                                                        TrashIcon { class: "h-4 w-4".to_string() }
                                                    }
                                                }
                                            }
                                            if confirm_delete {
                                                div { class: "mt-3 flex items-center justify-between gap-3 rounded-xl border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-600 dark:text-red-400",
                                                    span { "Delete this conversation?" }
                                                    div { class: "flex items-center gap-2",
                                                        button {
                                                            class: "rounded-lg border border-red-500/30 px-2.5 py-1 font-medium transition hover:bg-red-500/10",
                                                            onclick: move |_| {
                                                                let (next_conversations, next_active_id, next_messages) = delete_conversation_state(
                                                                    &merged_conversations_snapshot(
                                                                        &conversations.read(),
                                                                        active_conversation_id.read().as_deref(),
                                                                        &persisted_messages.read(),
                                                                    ),
                                                                    &delete_conversation_id,
                                                                );
                                                                conversations.set(next_conversations);
                                                                active_conversation_id.set(next_active_id);
                                                                messages.set(next_messages);
                                                                input.set(String::new());
                                                                pending_images.set(Vec::new());
                                                                compose_notice.set(None);
                                                                error.set(None);
                                                                confirm_delete_conversation_id.set(None);
                                                                editing_conversation_id.set(None);
                                                                editing_conversation_title.set(String::new());
                                                                persisted_messages_dirty.set(true);
                                                            },
                                                            "Delete"
                                                        }
                                                        button {
                                                            class: "rounded-lg border border-border px-2.5 py-1 text-foreground transition hover:bg-background",
                                                            onclick: move |_| confirm_delete_conversation_id.set(None),
                                                            "Cancel"
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
                    SheetFooter {
                        button {
                            class: "rounded-xl bg-primary px-4 py-2.5 text-sm font-medium text-primary-foreground transition hover:bg-primary/90",
                            onclick: move |_| {
                                let merged = merged_conversations_snapshot(
                                    &conversations.read(),
                                    active_conversation_id.read().as_deref(),
                                    &persisted_messages.read(),
                                );
                                let new_conversation = new_conversation();
                                let mut next_conversations = merged;
                                next_conversations.push(new_conversation.clone());
                                conversations.set(next_conversations);
                                active_conversation_id.set(Some(new_conversation.id.clone()));
                                messages.set(Vec::new());
                                input.set(String::new());
                                pending_images.set(Vec::new());
                                compose_notice.set(None);
                                error.set(None);
                                editing_conversation_id.set(None);
                                editing_conversation_title.set(String::new());
                                confirm_delete_conversation_id.set(None);
                                persisted_messages_dirty.set(true);
                                show_conversation_sheet.set(false);
                            },
                            "New Chat"
                        }
                    }
                }
            }
            div { class: "sticky top-0 z-20 border-b border-border bg-background/90 backdrop-blur-sm",
                div { class: "mx-auto flex max-w-5xl flex-col gap-3 px-4 py-4 sm:flex-row sm:items-center sm:justify-between",
                    div { class: "flex min-w-0 items-center gap-3",
                        button {
                            class: "flex h-10 w-10 items-center justify-center rounded-xl bg-primary/10 text-primary transition hover:bg-primary/15",
                            title: "Open conversations",
                            onclick: move |_| show_conversation_sheet.set(true),
                            SparklesIcon { class: "w-5 h-5".to_string() }
                        }
                        div { class: "min-w-0",
                            h1 { class: "text-xl font-semibold", "AI Chat" }
                            p { class: "truncate text-sm text-muted-foreground",
                                if let Some(title) = active_conversation_title.as_ref() {
                                    "{title} · Provider: {active_provider.name}"
                                } else {
                                    "Provider: {active_provider.name}"
                                }
                            }
                        }
                    }
                    div { class: "flex w-full min-w-0 items-center gap-2 sm:w-auto sm:justify-end",
                        select {
                            key: "{active_provider.id}:{models.read().len()}:{selected_model}",
                            class: "h-10 min-w-0 flex-1 rounded-lg border border-border bg-card px-3 text-sm text-foreground focus:outline-hidden sm:w-[18rem] sm:flex-none",
                            value: "{selected_model}",
                            disabled: models.read().is_empty() || *loading.read() || ppq_blocked,
                            onchange: move |evt| {
                                let value = evt.value();
                                selected_model.set(value.clone());
                                compose_notice.set(None);
                                persist_selected_model(
                                    active_provider.id.clone(),
                                    value,
                                    provider_state,
                                    error,
                                    pending_provider_save_snapshot,
                                    pending_provider_save_min_event_id,
                                    pending_provider_save_action,
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
                                "shrink-0 flex h-10 w-10 items-center justify-center rounded-lg border border-border text-muted-foreground opacity-50"
                            } else {
                                "shrink-0 flex h-10 w-10 items-center justify-center rounded-lg border border-border text-muted-foreground transition hover:bg-accent"
                            },
                            disabled: messages.read().is_empty() || *loading.read(),
                            title: "Clear conversation",
                            onclick: move |_| {
                                messages.set(Vec::new());
                                persisted_messages_dirty.set(true);
                                error.set(None);
                                compose_notice.set(None);
                            },
                            TrashIcon { class: "w-4 h-4".to_string() }
                        }
                        button {
                            class: "shrink-0 flex h-10 w-10 items-center justify-center rounded-lg border border-border text-muted-foreground transition hover:bg-accent",
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
                div { class: "mx-auto flex max-w-5xl flex-col gap-6 overflow-hidden px-4 py-6",
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
                                            pending_provider_save_snapshot
                                                .set(Some(next_state.clone()));
                                            pending_provider_save_min_event_id.set(
                                                PROVIDER_STATE_SAVE_EVENT
                                                    .read()
                                                    .as_ref()
                                                    .map(|event| event.event_id)
                                                    .unwrap_or(0),
                                            );
                                            pending_provider_save_action
                                                .set(PendingProviderSaveAction::BootstrapPpq);
                                            if let Some(snapshot) =
                                                ai_provider_store::queue_provider_state_save(
                                                    next_state,
                                                )
                                            {
                                                ai_provider_store::process_queued_provider_state_saves(snapshot);
                                            }
                                        }
                                        Err(err) => {
                                            error.set(Some(err));
                                            ppq_bootstrap_loading.set(false);
                                        }
                                    }
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

                    if let Some(notice) = compose_notice.read().as_ref() {
                        div { class: "max-w-3xl rounded-2xl border border-amber-500/20 bg-amber-500/10 px-4 py-3 text-sm text-amber-700 dark:text-amber-300",
                            "{notice}"
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
                                                compose_notice.set(None);
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
                            oninput: move |evt| {
                                compose_notice.set(None);
                                input.set(evt.value());
                            },
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
                                        compose_notice,
                                        messages,
                                        conversations,
                                        active_conversation_id,
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
                                        compose_notice,
                                        messages,
                                        conversations,
                                        active_conversation_id,
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
                    compose_notice.set(None);
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
        div { class: if is_user { "flex min-w-0 justify-end" } else { "flex min-w-0 justify-start" },
            div { class: if is_user {
                    "max-w-3xl min-w-0 overflow-hidden rounded-2xl bg-primary px-4 py-3 text-sm text-primary-foreground shadow-sm"
                } else {
                    "max-w-3xl min-w-0 overflow-hidden rounded-2xl border border-border bg-card px-4 py-3 text-sm text-foreground shadow-sm"
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
                            div { key: "{call.id}", class: "overflow-x-auto max-h-64 overflow-y-auto rounded-lg bg-muted/60 px-3 py-2 text-xs text-muted-foreground",
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

fn resolve_active_conversation_id(state: &PersistedChatState) -> Option<String> {
    if let Some(active_id) = state.active_conversation_id.as_deref() {
        if state
            .conversations
            .iter()
            .any(|conversation| conversation.id == active_id)
        {
            return Some(active_id.to_string());
        }
    }

    state
        .conversations
        .iter()
        .max_by_key(|conversation| conversation.updated_at_ms)
        .map(|conversation| conversation.id.clone())
}

fn new_conversation() -> PersistedConversation {
    let timestamp = crate::platform::timestamp::now_millis();
    PersistedConversation {
        id: format!("conversation-{timestamp}"),
        title: DEFAULT_CONVERSATION_TITLE.to_string(),
        messages: Vec::new(),
        created_at_ms: timestamp,
        updated_at_ms: timestamp,
    }
}

fn seeded_conversation(title_hint: Option<&str>, message: &str) -> PersistedConversation {
    seeded_conversation_with_timestamp(
        crate::platform::timestamp::now_millis(),
        title_hint,
        message,
    )
}

fn seeded_conversation_with_timestamp(
    timestamp: u64,
    title_hint: Option<&str>,
    message: &str,
) -> PersistedConversation {
    PersistedConversation {
        id: format!("conversation-{timestamp}"),
        title: title_hint
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .unwrap_or(DEFAULT_CONVERSATION_TITLE)
            .to_string(),
        messages: vec![PersistedChatMessage {
            id: format!("seed-{timestamp}"),
            role: PersistedChatRole::User,
            content: message.to_string(),
            images: vec![],
            tool_calls: vec![],
        }],
        created_at_ms: timestamp,
        updated_at_ms: timestamp,
    }
}

fn sync_active_conversation_state(
    conversations: &mut [PersistedConversation],
    active_id: Option<&str>,
    persisted_messages: &[PersistedChatMessage],
) {
    let Some(active_id) = active_id else {
        return;
    };
    let Some(conversation) = conversations
        .iter_mut()
        .find(|conversation| conversation.id == active_id)
    else {
        return;
    };

    let next_title = derive_conversation_title(&conversation.title, persisted_messages);
    let messages_changed = conversation.messages != persisted_messages;
    let title_changed = conversation.title != next_title;

    if !messages_changed && !title_changed {
        return;
    }

    if messages_changed {
        conversation.messages = persisted_messages.to_vec();
        conversation.updated_at_ms = crate::platform::timestamp::now_millis();
    }
    if title_changed {
        conversation.title = next_title;
    }
}

fn merged_conversations_snapshot(
    conversations: &[PersistedConversation],
    active_id: Option<&str>,
    persisted_messages: &[PersistedChatMessage],
) -> Vec<PersistedConversation> {
    let mut merged = conversations.to_vec();
    sync_active_conversation_state(&mut merged, active_id, persisted_messages);
    merged
}

fn derive_conversation_title(
    current_title: &str,
    persisted_messages: &[PersistedChatMessage],
) -> String {
    if current_title != DEFAULT_CONVERSATION_TITLE {
        return current_title.to_string();
    }

    let Some(first_user_message) = persisted_messages.iter().find(|message| {
        message.role == PersistedChatRole::User
            && (!message.content.trim().is_empty() || !message.images.is_empty())
    }) else {
        return current_title.to_string();
    };

    let normalized = first_user_message.content.trim();
    if normalized.is_empty() {
        return "Image".to_string();
    }

    if normalized.chars().count() <= 48 {
        normalized.to_string()
    } else {
        format!("{}...", normalized.chars().take(45).collect::<String>())
    }
}

fn rename_conversation(
    mut conversations: Signal<Vec<PersistedConversation>>,
    conversation_id: &str,
    title: &str,
) {
    let next_title = if title.trim().is_empty() {
        DEFAULT_CONVERSATION_TITLE.to_string()
    } else {
        title.trim().to_string()
    };

    conversations.with_mut(|items| {
        if let Some(conversation) = items
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        {
            conversation.title = next_title.clone();
            conversation.updated_at_ms = crate::platform::timestamp::now_millis();
        }
    });
}

fn delete_conversation_state(
    conversations: &[PersistedConversation],
    conversation_id: &str,
) -> (
    Vec<PersistedConversation>,
    Option<String>,
    Vec<DisplayMessage>,
) {
    let mut next_conversations = conversations
        .iter()
        .filter(|conversation| conversation.id != conversation_id)
        .cloned()
        .collect::<Vec<_>>();

    if next_conversations.is_empty() {
        return (next_conversations, None, Vec::new());
    }

    next_conversations.sort_by_key(|conversation| std::cmp::Reverse(conversation.updated_at_ms));
    let next_active = next_conversations[0].id.clone();
    let next_messages = next_conversations[0]
        .messages
        .iter()
        .cloned()
        .map(display_message_from_persisted)
        .collect();

    (next_conversations, Some(next_active), next_messages)
}

fn sorted_conversations(conversations: &[PersistedConversation]) -> Vec<PersistedConversation> {
    let mut items = conversations.to_vec();
    items.sort_by_key(|conversation| std::cmp::Reverse(conversation.updated_at_ms));
    items
}

fn conversation_preview(conversation: &PersistedConversation) -> String {
    conversation
        .messages
        .iter()
        .rev()
        .find_map(|message| {
            if !message.content.trim().is_empty() {
                Some(message.content.trim().to_string())
            } else if !message.images.is_empty() {
                Some("Image".to_string())
            } else if !message.tool_calls.is_empty() {
                Some("Tool activity".to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "Empty conversation".to_string())
}

fn format_relative_timestamp(timestamp_ms: u64) -> String {
    let now = crate::platform::timestamp::now_millis();
    let elapsed = now.saturating_sub(timestamp_ms);
    match elapsed {
        0..=59_999 => "just now".to_string(),
        60_000..=3_599_999 => format!("{}m ago", elapsed / 60_000),
        3_600_000..=86_399_999 => format!("{}h ago", elapsed / 3_600_000),
        _ => format!("{}d ago", elapsed / 86_400_000),
    }
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
        tool_call_id: None,
        tool_calls: None,
    }];
    for message in messages {
        let content =
            if !message.images.is_empty() {
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
        let has_tool_calls = !message.tool_calls.is_empty();
        let tool_calls_for_msg: Option<Vec<ToolCall>> = if has_tool_calls {
            Some(
                message
                    .tool_calls
                    .iter()
                    .map(|tc| ToolCall {
                        id: tc.id.clone(),
                        function: crate::services::ai_chat::ToolCallFunction {
                            name: tc.name.clone(),
                            arguments: String::new(),
                        },
                    })
                    .collect(),
            )
        } else {
            None
        };
        api_messages.push(ChatMessage {
            role: match message.role {
                DisplayRole::User => ChatRole::User,
                DisplayRole::Assistant => ChatRole::Assistant,
            },
            content,
            tool_call_id: None,
            tool_calls: tool_calls_for_msg,
        });
        if has_tool_calls {
            for tc in &message.tool_calls {
                api_messages.push(ChatMessage {
                    role: ChatRole::Tool,
                    content: ChatMessageContent::Text(tc.result.clone()),
                    tool_call_id: Some(tc.id.clone()),
                    tool_calls: None,
                });
            }
        }
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
    mut compose_notice: Signal<Option<String>>,
    mut messages: Signal<Vec<DisplayMessage>>,
    mut conversations: Signal<Vec<PersistedConversation>>,
    mut active_conversation_id: Signal<Option<String>>,
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

    if should_suggest_image_model(
        &active_model,
        &attached_images,
        image_models_available,
        &text,
    ) {
        compose_notice.set(Some(
            "The selected model is text-only. Choose an IMAGE model from the model picker to generate images.".to_string(),
        ));
    } else {
        compose_notice.set(None);
    }

    if !attached_images.is_empty() && !active_model.supports_image_input {
        error.set(Some(
            "The selected model does not support image input. Remove attached images or choose a model that supports images.".to_string(),
        ));
        return;
    }

    if active_model.kind == ChatModelKind::Image && attached_images.len() > 1 {
        error.set(Some(
            "Image generation currently supports only one reference image per request.".to_string(),
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
    if active_conversation_id.read().is_none() {
        let conversation = new_conversation();
        active_conversation_id.set(Some(conversation.id.clone()));
        conversations.with_mut(|items| items.push(conversation));
    }
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
                    tools: Some(crate::services::ai_tools::nostr_tool_definitions()),
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

    let executed = execute_tool_calls(&choice.message.tool_calls).await;
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
        parts.extend(
            assistant_images
                .into_iter()
                .map(|image| ChatMessagePart::ImageUrl {
                    image_url: ChatImageUrl { url: image.url },
                }),
        );
        ChatMessageContent::Parts(parts)
    };
    follow_up_messages.push(ChatMessage {
        role: ChatRole::Assistant,
        content: follow_up_assistant_content,
        tool_call_id: None,
        tool_calls: Some(choice.message.tool_calls.clone()),
    });
    for tool in &executed {
        follow_up_messages.push(ChatMessage {
            role: ChatRole::Tool,
            content: ChatMessageContent::Text(tool.result.clone()),
            tool_call_id: Some(tool.id.clone()),
            tool_calls: None,
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

async fn execute_tool_calls(tool_calls: &[ToolCall]) -> Vec<ExecutedToolCall> {
    let mut results = Vec::with_capacity(tool_calls.len());
    for call in tool_calls {
        let result = crate::services::ai_tools::execute_nostr_tool(
            &call.function.name,
            &call.function.arguments,
        )
        .await;
        results.push(ExecutedToolCall {
            id: call.id.clone(),
            name: call.function.name.clone(),
            result,
        });
    }
    results
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
    mut pending_provider_save_snapshot: Signal<Option<AiProviderState>>,
    mut pending_provider_save_min_event_id: Signal<u64>,
    mut pending_provider_save_action: Signal<PendingProviderSaveAction>,
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

    pending_provider_save_snapshot.set(Some(next_state.clone()));
    pending_provider_save_min_event_id.set(
        PROVIDER_STATE_SAVE_EVENT
            .read()
            .as_ref()
            .map(|event| event.event_id)
            .unwrap_or(0),
    );
    pending_provider_save_action.set(PendingProviderSaveAction::None);

    if let Some(snapshot) = ai_provider_store::queue_provider_state_save(next_state) {
        ai_provider_store::process_queued_provider_state_saves(snapshot);
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
        build_api_messages, delete_conversation_state, derive_conversation_title,
        history_save_snapshot_key, resolve_selected_model, seeded_conversation_with_timestamp,
        should_suggest_image_model, ChatImage, DisplayMessage, DisplayRole,
    };
    use crate::services::ai_chat::{ChatMessageContent, ChatMessagePart, ChatModel, ChatModelKind};
    use crate::stores::ai_chat_store::{
        PersistedChatImage, PersistedChatMessage, PersistedChatRole, PersistedChatState,
        PersistedConversation, PersistedToolCall,
    };

    fn state_with_messages(messages: Vec<PersistedChatMessage>) -> PersistedChatState {
        PersistedChatState {
            active_conversation_id: Some("conversation-1".to_string()),
            conversations: vec![PersistedConversation {
                id: "conversation-1".to_string(),
                title: "New Chat".to_string(),
                messages,
                created_at_ms: 1,
                updated_at_ms: 1,
            }],
        }
    }

    #[test]
    fn history_snapshot_key_is_deterministic() {
        let messages = vec![PersistedChatMessage {
            id: "1".to_string(),
            role: PersistedChatRole::User,
            content: "hello".to_string(),
            images: vec![],
            tool_calls: vec![],
        }];
        let state = state_with_messages(messages.clone());
        let initial_loaded_state = state_with_messages(messages);

        let first = history_save_snapshot_key("account", &state, &initial_loaded_state);
        let second =
            history_save_snapshot_key("account", &state.clone(), &initial_loaded_state.clone());

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
        let messages_state = state_with_messages(messages);
        let edited_state = state_with_messages(edited_messages);

        assert_ne!(
            history_save_snapshot_key("account", &messages_state, &messages_state),
            history_save_snapshot_key("account", &edited_state, &messages_state),
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
        let messages_state = state_with_messages(messages);
        let with_tool_call_state = state_with_messages(with_tool_call);

        assert_ne!(
            history_save_snapshot_key("account", &messages_state, &messages_state),
            history_save_snapshot_key("account", &with_tool_call_state, &messages_state),
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
        let messages_state = state_with_messages(messages);

        assert_ne!(
            history_save_snapshot_key("account-a", &messages_state, &messages_state),
            history_save_snapshot_key("account-b", &messages_state, &messages_state),
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
        let messages_state = state_with_messages(messages);
        let different_initial_state = state_with_messages(different_initial_loaded_messages);

        assert_ne!(
            history_save_snapshot_key("account", &messages_state, &messages_state),
            history_save_snapshot_key("account", &messages_state, &different_initial_state,),
        );
    }

    #[test]
    fn seeded_conversation_uses_title_hint_and_creates_user_message() {
        let conversation =
            seeded_conversation_with_timestamp(42, Some("Bible: John 3:16"), "Bible passage text");

        assert_eq!(conversation.title, "Bible: John 3:16");
        assert_eq!(conversation.id, "conversation-42");
        assert_eq!(conversation.messages.len(), 1);
        assert_eq!(conversation.messages[0].role, PersistedChatRole::User);
        assert_eq!(conversation.messages[0].content, "Bible passage text");
    }

    #[test]
    fn seeded_conversation_falls_back_to_default_title() {
        let conversation = seeded_conversation_with_timestamp(42, Some("   "), "Seeded message");

        assert_eq!(conversation.title, "New Chat");
    }

    #[test]
    fn derive_conversation_title_uses_image_fallback_for_image_only_first_message() {
        let title = derive_conversation_title(
            "New Chat",
            &[PersistedChatMessage {
                id: "1".to_string(),
                role: PersistedChatRole::User,
                content: String::new(),
                images: vec![PersistedChatImage {
                    url: "https://cdn.example.com/image.png".to_string(),
                    alt: String::new(),
                    title: String::new(),
                }],
                tool_calls: vec![],
            }],
        );

        assert_eq!(title, "Image");
    }

    #[test]
    fn delete_conversation_state_returns_empty_state_when_last_conversation_is_removed() {
        let conversation = PersistedConversation {
            id: "conversation-1".to_string(),
            title: "New Chat".to_string(),
            messages: vec![PersistedChatMessage {
                id: "message-1".to_string(),
                role: PersistedChatRole::User,
                content: "hello".to_string(),
                images: vec![],
                tool_calls: vec![],
            }],
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let (conversations, active_id, messages) =
            delete_conversation_state(&[conversation], "conversation-1");

        assert!(conversations.is_empty());
        assert!(active_id.is_none());
        assert!(messages.is_empty());
    }

    #[test]
    fn delete_conversation_state_selects_most_recent_remaining_conversation() {
        let older = PersistedConversation {
            id: "conversation-1".to_string(),
            title: "Older".to_string(),
            messages: vec![],
            created_at_ms: 1,
            updated_at_ms: 5,
        };
        let newer = PersistedConversation {
            id: "conversation-2".to_string(),
            title: "Newer".to_string(),
            messages: vec![PersistedChatMessage {
                id: "message-1".to_string(),
                role: PersistedChatRole::Assistant,
                content: "welcome".to_string(),
                images: vec![],
                tool_calls: vec![],
            }],
            created_at_ms: 2,
            updated_at_ms: 10,
        };

        let (conversations, active_id, messages) =
            delete_conversation_state(&[older, newer.clone()], "conversation-1");

        assert_eq!(conversations.len(), 1);
        assert_eq!(active_id, Some("conversation-2".to_string()));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, newer.messages[0].content);
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
    fn suggests_image_model_without_blocking_conditions() {
        let active_model = ChatModel {
            id: "text-model".to_string(),
            name: "Text Model".to_string(),
            description: String::new(),
            kind: ChatModelKind::Chat,
            supports_image_input: false,
            total_cost: None,
        };

        assert!(should_suggest_image_model(
            &active_model,
            &[],
            true,
            "Generate a photorealistic sunset over mountains"
        ));
        assert!(!should_suggest_image_model(
            &active_model,
            &[ChatImage::default()],
            true,
            "Generate a photorealistic sunset over mountains"
        ));
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
