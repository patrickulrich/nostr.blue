use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::nostr_client::edits::KIND_NOTE_EDIT;

#[derive(Clone, Debug, PartialEq)]
pub struct EditInfo {
    pub edited_content: String,
    pub edited_at: u64,
    pub edit_event_id: String,
    pub edit_count: usize,
}

struct EditCacheInner {
    edits: HashMap<String, EditInfo>,
}

static EDIT_CACHE: OnceLock<Mutex<EditCacheInner>> = OnceLock::new();

fn get_cache() -> &'static Mutex<EditCacheInner> {
    EDIT_CACHE.get_or_init(|| {
        Mutex::new(EditCacheInner {
            edits: HashMap::new(),
        })
    })
}

pub static EDIT_VERSION: GlobalSignal<u64> = Signal::global(|| 0);

fn bump_version() {
    *EDIT_VERSION.write() += 1;
}

pub fn get_latest_edit(note_id: &str) -> Option<EditInfo> {
    let cache = get_cache().lock().unwrap_or_else(|p| p.into_inner());
    cache.edits.get(note_id).cloned()
}

pub fn process_edit_event(original_note_id: &EventId, edit_event: &NostrEvent) {
    if edit_event.kind.as_u16() != KIND_NOTE_EDIT {
        return;
    }
    let mut cache = get_cache().lock().unwrap_or_else(|p| p.into_inner());
    let key = original_note_id.to_hex();
    let new_created_at = edit_event.created_at.as_secs();
    let should_update = match cache.edits.get(&key) {
        Some(existing) => new_created_at > existing.edited_at,
        None => true,
    };
    if should_update {
        let edit_count = cache.edits.get(&key).map(|e| e.edit_count + 1).unwrap_or(1);
        cache.edits.insert(
            key,
            EditInfo {
                edited_content: edit_event.content.clone(),
                edited_at: new_created_at,
                edit_event_id: edit_event.id.to_hex(),
                edit_count,
            },
        );
        drop(cache);
        bump_version();
    }
}

pub fn process_edit_events_batch(
    edit_events: &[NostrEvent],
    original_author_map: &HashMap<EventId, PublicKey>,
) {
    let mut cache = get_cache().lock().unwrap_or_else(|p| p.into_inner());
    let mut changed = false;
    for event in edit_events {
        if event.kind.as_u16() != KIND_NOTE_EDIT {
            continue;
        }
        if let Some(original_id) = event.tags.event_ids().next() {
            let key = original_id.to_hex();
            if let Some(original_author) = original_author_map.get(original_id) {
                if event.pubkey != *original_author {
                    continue;
                }
            }
            let new_created_at = event.created_at.as_secs();
            let should_update = match cache.edits.get(&key) {
                Some(existing) => new_created_at > existing.edited_at,
                None => true,
            };
            if should_update {
                let edit_count = cache.edits.get(&key).map(|e| e.edit_count + 1).unwrap_or(1);
                cache.edits.insert(
                    key,
                    EditInfo {
                        edited_content: event.content.clone(),
                        edited_at: new_created_at,
                        edit_event_id: event.id.to_hex(),
                        edit_count,
                    },
                );
                changed = true;
            }
        }
    }
    drop(cache);
    if changed {
        bump_version();
    }
}

pub fn apply_edits_to_event_map(
    edit_events: &[NostrEvent],
    event_map: &HashMap<String, NostrEvent>,
) {
    let mut edits_by_original: HashMap<String, Vec<&NostrEvent>> = HashMap::new();
    for event in edit_events {
        if event.kind.as_u16() != KIND_NOTE_EDIT {
            continue;
        }
        if let Some(original_id) = event.tags.event_ids().next() {
            let key = original_id.to_hex();
            edits_by_original.entry(key).or_default().push(event);
        }
    }
    let mut cache = get_cache().lock().unwrap_or_else(|p| p.into_inner());
    let mut changed = false;
    for (original_id_hex, edits) in edits_by_original {
        if let Some(original_event) = event_map.get(&original_id_hex) {
            let original_author = original_event.pubkey;
            let same_author_edits: Vec<&&NostrEvent> = edits
                .iter()
                .filter(|e| e.pubkey == original_author)
                .collect();
            if same_author_edits.is_empty() {
                continue;
            }
            let latest = same_author_edits
                .iter()
                .max_by_key(|e| e.created_at)
                .unwrap();
            let new_created_at = latest.created_at.as_secs();
            let should_update = match cache.edits.get(&original_id_hex) {
                Some(existing) => new_created_at > existing.edited_at,
                None => true,
            };
            if should_update {
                cache.edits.insert(
                    original_id_hex,
                    EditInfo {
                        edited_content: latest.content.clone(),
                        edited_at: new_created_at,
                        edit_event_id: latest.id.to_hex(),
                        edit_count: same_author_edits.len(),
                    },
                );
                changed = true;
            }
        }
    }
    drop(cache);
    if changed {
        bump_version();
    }
}
