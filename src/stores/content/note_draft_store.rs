use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct NoteDraft {
    pub content: String,
    pub saved_at: u64,
}

fn storage_key(pubkey: &str, context: &str) -> String {
    format!("note_draft_{}_{}", pubkey, context)
}

pub fn save_note_draft(pubkey: &str, context: &str, draft: &NoteDraft) {
    let key = storage_key(pubkey, context);
    let _ = crate::platform::storage::set(&key, draft);
}

pub fn read_note_draft(pubkey: &str, context: &str) -> Option<NoteDraft> {
    let key = storage_key(pubkey, context);
    crate::platform::storage::get::<NoteDraft>(&key).ok()
}

pub fn clear_note_draft(pubkey: &str, context: &str) {
    let key = storage_key(pubkey, context);
    let _ = crate::platform::storage::delete(&key);
}
