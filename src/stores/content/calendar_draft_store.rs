use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct CalendarEventDraft {
    pub title: String,
    pub summary: String,
    pub content: String,
    pub event_type: String,
    pub start_date: String,
    pub start_time: String,
    pub end_date: String,
    pub end_time: String,
    pub location: String,
    pub locations: Vec<String>,
    pub image_url: String,
    pub hashtags_input: String,
    pub timezone: String,
    pub participants: Vec<(String, String, String)>,
    pub saved_at: u64,
}

fn storage_key(pubkey: &str) -> String {
    format!("calendar_event_draft_{}", pubkey)
}

pub fn save_calendar_draft(pubkey: &str, draft: &CalendarEventDraft) {
    let key = storage_key(pubkey);
    let _ = crate::platform::storage::set(&key, draft);
}

pub fn read_calendar_draft(pubkey: &str) -> Option<CalendarEventDraft> {
    let key = storage_key(pubkey);
    crate::platform::storage::get::<CalendarEventDraft>(&key).ok()
}

pub fn clear_calendar_draft(pubkey: &str) {
    let key = storage_key(pubkey);
    let _ = crate::platform::storage::delete(&key);
}
