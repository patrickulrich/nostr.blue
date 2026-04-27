use super::commands::NoteData;
use super::queries;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MIN_SEND_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Hash, Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnknownId {
    Pubkey([u8; 32]),
    NoteId([u8; 32]),
}

pub struct UnknownIds {
    ids: HashMap<UnknownId, HashSet<String>>,
    event_queue: Vec<nostr::Event>,
    first_updated: Option<Instant>,
    last_updated: Option<Instant>,
}

impl Default for UnknownIds {
    fn default() -> Self {
        Self::new()
    }
}

impl UnknownIds {
    pub fn new() -> Self {
        Self {
            ids: HashMap::new(),
            event_queue: Vec::new(),
            first_updated: None,
            last_updated: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn ready_to_send(&self) -> bool {
        if self.ids.is_empty() {
            return false;
        }
        let now = Instant::now();
        match (self.first_updated, self.last_updated) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(_), Some(last)) => now.duration_since(last) >= MIN_SEND_INTERVAL,
        }
    }

    fn mark_updated(&mut self) {
        let now = Instant::now();
        if self.first_updated.is_none() {
            self.first_updated = Some(now);
        }
        self.last_updated = Some(now);
    }

    pub fn queue_event(&mut self, event: nostr::Event) {
        self.event_queue.push(event);
    }

    pub async fn process_queued_events(&mut self) {
        let events: Vec<_> = self.event_queue.drain(..).collect();
        for event in events {
            let note_data = NoteData {
                id: event.id.to_bytes(),
                pubkey: event.pubkey.to_bytes(),
                kind: event.kind.as_u16(),
                content: event.content.clone(),
                created_at: event.created_at.as_secs(),
                tags: event
                    .tags
                    .iter()
                    .map(|t| t.as_slice().to_vec())
                    .collect(),
                sig: event.sig.as_ref().to_vec(),
            };
            self.update_from_note_data(&note_data).await;
        }
    }

    pub async fn update_from_note_data(&mut self, note: &NoteData) {
        self.add_pubkey_if_missing(&note.pubkey).await;

        for tag in &note.tags {
            if tag.len() >= 2 && tag[0] == "e" {
                if let Ok(id_bytes) = hex::decode(&tag[1]) {
                    if id_bytes.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&id_bytes);
                        self.add_note_id_if_missing(&arr, tag.get(2).map(|s| s.as_str()))
                            .await;
                    }
                }
            }
            if tag.len() >= 2 && tag[0] == "p" {
                if let Ok(pk_bytes) = hex::decode(&tag[1]) {
                    if pk_bytes.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&pk_bytes);
                        self.add_pubkey_if_missing(&arr).await;
                    }
                }
            }
        }
    }

    async fn add_pubkey_if_missing(&mut self, pubkey: &[u8; 32]) {
        match queries::get_profile(*pubkey).await {
            Ok(Some(_)) => {}
            _ => {
                self.ids
                    .entry(UnknownId::Pubkey(*pubkey))
                    .or_default();
                self.mark_updated();
            }
        }
    }

    async fn add_note_id_if_missing(&mut self, id: &[u8; 32], relay_hint: Option<&str>) {
        match queries::get_note_data_by_id(*id).await {
            Ok(Some(_)) => {}
            _ => {
                let entry = self.ids.entry(UnknownId::NoteId(*id)).or_default();
                if let Some(hint) = relay_hint {
                    entry.insert(hint.to_string());
                }
                self.mark_updated();
            }
        }
    }

    pub async fn send_and_clear(&mut self, client: &nostr_sdk::Client) {
        if self.ids.is_empty() {
            return;
        }

        let mut pubkey_bytes: Vec<[u8; 32]> = Vec::new();
        let mut note_id_bytes: Vec<[u8; 32]> = Vec::new();

        for id in self.ids.keys() {
            match id {
                UnknownId::Pubkey(pk) => pubkey_bytes.push(*pk),
                UnknownId::NoteId(nid) => note_id_bytes.push(*nid),
            }
        }

        if !pubkey_bytes.is_empty() {
            let pks: Vec<nostr::PublicKey> = pubkey_bytes
                .iter()
                .filter_map(|pk| nostr::PublicKey::from_slice(pk).ok())
                .collect();
            if !pks.is_empty() {
                let filter = nostr_sdk::Filter::new()
                    .authors(pks)
                    .kind(nostr_sdk::Kind::Metadata);
                let _ = client.subscribe(filter, None).await;
            }
        }

        if !note_id_bytes.is_empty() {
            let ids: Vec<nostr::EventId> = note_id_bytes
                .iter()
                .filter_map(|id| nostr::EventId::from_slice(id).ok())
                .collect();
            if !ids.is_empty() {
                let filter = nostr_sdk::Filter::new().ids(ids);
                let _ = client.subscribe(filter, None).await;
            }
        }

        self.ids.clear();
        self.first_updated = None;
        self.last_updated = None;
    }
}

use once_cell::sync::Lazy;

pub static UNKNOWN_IDS: Lazy<Mutex<UnknownIds>> = Lazy::new(|| Mutex::new(UnknownIds::new()));

pub fn queue_event(event: nostr::Event) {
    let mut ids = UNKNOWN_IDS.lock().unwrap();
    ids.queue_event(event);
}
