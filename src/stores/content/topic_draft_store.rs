use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct TopicPostDraft {
    pub topic: String,
    pub content: String,
    pub saved_at: u64,
}

const EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;

fn storage_key(pubkey: &str) -> String {
    format!("topic_draft_{}", pubkey)
}

pub fn save_topic_draft(pubkey: &str, draft: &TopicPostDraft) {
    let key = storage_key(pubkey);
    let _ = crate::platform::storage::set(&key, draft);
}

pub fn read_topic_draft(pubkey: &str) -> Option<TopicPostDraft> {
    let key = storage_key(pubkey);
    let draft: TopicPostDraft = crate::platform::storage::get(&key).ok()?;
    let now = crate::platform::timestamp::now_secs();
    if now.saturating_sub(draft.saved_at) > EXPIRY_SECS {
        let _ = crate::platform::storage::delete(&key);
        return None;
    }
    Some(draft)
}

pub fn clear_topic_draft(pubkey: &str) {
    let key = storage_key(pubkey);
    let _ = crate::platform::storage::delete(&key);
}
