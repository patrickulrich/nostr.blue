mod logic;
mod tests;
mod types;

use crate::components::icons::{CameraIcon, SendIcon, SettingsIcon, SparklesIcon, TrashIcon};
use crate::components::{
    ClientInitializing, ImageUploadDialog, Sheet, SheetContent, SheetDescription,
    SheetFooter, SheetHeader, SheetSide, SheetTitle,
};
use crate::routes::Route;
use crate::services::ai_chat::{
    get_available_models, ChatModel, ChatModelKind,
};
use crate::services::ppq;
use crate::stores::ai_chat_seed_store;
use crate::stores::ai_chat_store::{
    self, PersistedChatMessage, PersistedChatState,
    PersistedConversation,
};
use crate::stores::ai_provider_store::{
    self, ppq_provider, resolve_providers, AiProviderState, PpqAccountState,
    PROVIDER_STATE_SAVE_EVENT,
};
use crate::stores::nostr_client;
use crate::utils::markdown::render_markdown;
use dioxus::document;
use dioxus::prelude::*;

use types::*;
use logic::*;

pub const DEFAULT_CONVERSATION_TITLE: &str = "New Chat";
const AI_CHAT_HISTORY_LOAD_ENABLED: bool = true;
const AI_CHAT_HISTORY_SAVE_ENABLED: bool = true;
const AI_CHAT_HISTORY_SAVE_FAILURE_COOLDOWN_MS: u64 = 5_000;

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
                on_insert: move |data: crate::components::ImageInsertData| {
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
