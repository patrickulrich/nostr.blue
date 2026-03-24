use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiChatSeedPayload {
    pub source: String,
    pub title_hint: Option<String>,
    pub message: String,
}

pub static PENDING_AI_CHAT_SEED: GlobalSignal<Option<AiChatSeedPayload>> = Signal::global(|| None);

fn store_pending_seed(pending: &mut Option<AiChatSeedPayload>, payload: AiChatSeedPayload) {
    *pending = Some(payload);
}

fn take_pending_seed(pending: &mut Option<AiChatSeedPayload>) -> Option<AiChatSeedPayload> {
    pending.take()
}

pub fn queue_ai_chat_seed(payload: AiChatSeedPayload) {
    PENDING_AI_CHAT_SEED.with_mut(|pending| store_pending_seed(pending, payload));
}

pub fn take_ai_chat_seed() -> Option<AiChatSeedPayload> {
    PENDING_AI_CHAT_SEED.with_mut(take_pending_seed)
}

#[cfg(test)]
mod tests {
    use super::{store_pending_seed, take_pending_seed, AiChatSeedPayload};

    #[test]
    fn queued_seed_is_consumed_once() {
        let mut pending = None;

        let payload = AiChatSeedPayload {
            source: "bible".to_string(),
            title_hint: Some("John 3:16".to_string()),
            message: "Bible passage: John 3:16".to_string(),
        };

        store_pending_seed(&mut pending, payload.clone());

        assert_eq!(take_pending_seed(&mut pending), Some(payload));
        assert_eq!(take_pending_seed(&mut pending), None);
    }
}
