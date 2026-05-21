use crate::services::ai_chat::ChatModelKind;
use crate::stores::ai_chat_store::PersistedChatState;
use dioxus::prelude::*;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayMessage {
    pub id: String,
    pub role: DisplayRole,
    pub content: String,
    pub images: Vec<ChatImage>,
    pub tool_calls: Vec<ExecutedToolCall>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatImage {
    pub url: String,
    pub alt: String,
    pub title: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DisplayRole {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutedToolCall {
    pub id: String,
    pub name: String,
    pub result: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FailedHistorySnapshot {
    pub snapshot_key: String,
    pub failed_at_ms: u64,
}

pub fn is_current_chat_history_request(
    generation_signal: Signal<u32>,
    account_key: &str,
    captured_generation: u32,
) -> bool {
    *generation_signal.read() == captured_generation
        && crate::stores::ai_chat_store::current_account_key() == account_key
}

pub fn should_suggest_image_model(
    active_model: &crate::services::ai_chat::ChatModel,
    attached_images: &[ChatImage],
    image_models_available: bool,
    text: &str,
) -> bool {
    active_model.kind != ChatModelKind::Image
        && attached_images.is_empty()
        && image_models_available
        && super::logic::looks_like_image_generation_prompt(text)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PendingProviderSaveAction {
    #[default]
    None,
    BootstrapPpq,
}

pub fn history_save_snapshot_key(
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
