use super::types::{
    ChatImage, DisplayMessage, DisplayRole, ExecutedToolCall, PendingProviderSaveAction,
};
use super::DEFAULT_CONVERSATION_TITLE;
use crate::components::ImageInsertData;
use crate::services::ai_chat::{
    generate_images, send_chat_message, AssistantContent,
    ChatCompletionRequest, ChatCompletionResponse, ChatImageUrl, ChatMessage, ChatMessageContent,
    ChatMessagePart, ChatModel, ChatModelKind, ChatRole, ImageGenerationRequest, ToolCall,
};
use crate::stores::ai_chat_store::{
    PersistedChatImage, PersistedChatMessage, PersistedChatRole, PersistedChatState,
    PersistedConversation, PersistedToolCall,
};
use crate::stores::ai_provider_store::{
    self, ppq_provider, AiProviderConfig, AiProviderState,
    PROVIDER_STATE_SAVE_EVENT,
};
use dioxus::prelude::*;

const SYSTEM_PROMPT: &str = "You are Nostrich, an AI assistant inside nostr.blue, a Nostr client. Be concise and helpful. Your personality is a fun ostrich that represents the nostr community. You have access to nostr tools to fetch profiles, notes, events, interactions, search content, and more. When the user shares or asks about a note, profile, or any Nostr entity, proactively use the available tools to provide enriched context.";
pub const AI_CHAT_PROVIDER_PERSISTENCE_ENABLED: bool = true;

pub fn current_provider(providers: &[AiProviderConfig], state: &AiProviderState) -> AiProviderConfig {
    providers
        .iter()
        .find(|provider| provider.id == state.selected_provider_id)
        .cloned()
        .unwrap_or_else(|| ppq_provider(state.ppq_account.as_ref()))
}

pub fn current_model(models: &[ChatModel], selected_model_id: &str) -> Option<ChatModel> {
    models
        .iter()
        .find(|model| model.id == selected_model_id)
        .cloned()
}

pub fn resolve_active_conversation_id(state: &PersistedChatState) -> Option<String> {
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

pub fn new_conversation() -> PersistedConversation {
    let timestamp = crate::platform::timestamp::now_millis();
    PersistedConversation {
        id: format!("conversation-{timestamp}"),
        title: DEFAULT_CONVERSATION_TITLE.to_string(),
        messages: Vec::new(),
        created_at_ms: timestamp,
        updated_at_ms: timestamp,
    }
}

pub fn seeded_conversation(title_hint: Option<&str>, message: &str) -> PersistedConversation {
    seeded_conversation_with_timestamp(
        crate::platform::timestamp::now_millis(),
        title_hint,
        message,
    )
}

pub fn seeded_conversation_with_timestamp(
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

pub fn sync_active_conversation_state(
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

pub fn merged_conversations_snapshot(
    conversations: &[PersistedConversation],
    active_id: Option<&str>,
    persisted_messages: &[PersistedChatMessage],
) -> Vec<PersistedConversation> {
    let mut merged = conversations.to_vec();
    sync_active_conversation_state(&mut merged, active_id, persisted_messages);
    merged
}

pub fn derive_conversation_title(
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

pub fn rename_conversation(
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

pub fn delete_conversation_state(
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

pub fn sorted_conversations(conversations: &[PersistedConversation]) -> Vec<PersistedConversation> {
    let mut items = conversations.to_vec();
    items.sort_by_key(|conversation| std::cmp::Reverse(conversation.updated_at_ms));
    items
}

pub fn conversation_preview(conversation: &PersistedConversation) -> String {
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

pub fn format_relative_timestamp(timestamp_ms: u64) -> String {
    let now = crate::platform::timestamp::now_millis();
    let elapsed = now.saturating_sub(timestamp_ms);
    match elapsed {
        0..=59_999 => "just now".to_string(),
        60_000..=3_599_999 => format!("{}m ago", elapsed / 60_000),
        3_600_000..=86_399_999 => format!("{}h ago", elapsed / 3_600_000),
        _ => format!("{}d ago", elapsed / 86_400_000),
    }
}

pub fn has_image_models(models: &[ChatModel]) -> bool {
    models
        .iter()
        .any(|model| model.kind == ChatModelKind::Image)
}

pub fn looks_like_image_generation_prompt(text: &str) -> bool {
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

pub fn build_api_messages(messages: &[DisplayMessage]) -> Vec<ChatMessage> {
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
pub fn submit_message(
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

    if super::types::should_suggest_image_model(
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

pub async fn apply_chat_response(
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

pub fn extract_assistant_content(content: Option<AssistantContent>) -> (String, Vec<ChatImage>) {
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

pub async fn execute_tool_calls(tool_calls: &[ToolCall]) -> Vec<ExecutedToolCall> {
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

pub fn persisted_message_from_display(message: DisplayMessage) -> PersistedChatMessage {
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

pub fn display_message_from_persisted(message: PersistedChatMessage) -> DisplayMessage {
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

pub fn persist_selected_model(
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

pub fn resolve_selected_model(
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
