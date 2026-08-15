use dioxus_stores::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum QueueEventStatus {
    Pending,
    Publishing,
    Success,
    PartialFailure,
    Failed { error: String },
    MaxRetriesExceeded { error: String },
    Aborted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum QueueEventType {
    Note,
    Reaction,
    Repost,
    Article,
    Profile,
    Contacts,
    Media,
    Edit,
    DirectMessage,
    /// Mostro daemon protocol traffic (restore session, LastTradeIndex,
    /// take/settle, disputes). Not a user-authored DM — labeled distinctly
    /// in the publish-queue UI to avoid "I sent no DMs" confusion.
    Mostro,
    Calendar,
    Shop,
    Cashu,
    Community,
    Channel,
    Group,
    PinBoard,
    Topic,
    Pack,
    Mute,
    Poll,
    Bookmark,
    GitHosting,
    Nsite,
    RelayList,
    Other(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueuedEvent {
    pub id: String,
    pub event_json: String,
    pub event_type: QueueEventType,
    pub event_id: String,
    pub pubkey: String,
    pub status: QueueEventStatus,
    pub target_relays: Option<Vec<String>>,
    pub created_at: u64,
    pub retry_count: u32,
    pub max_retries: u32,
    pub last_retry_at: Option<u64>,
    pub metadata: HashMap<String, String>,
    #[serde(default)]
    pub kind: Option<nostr_sdk::Kind>,
    #[serde(default)]
    pub d_tag: Option<String>,
}

impl QueuedEvent {
    #[allow(dead_code)]
    pub fn kind(&self) -> Option<nostr_sdk::Kind> {
        if let Some(k) = self.kind {
            return Some(k);
        }
        let event: Result<nostr_sdk::Event, _> = serde_json::from_str(&self.event_json);
        event.ok().map(|e| e.kind)
    }

    #[allow(dead_code)]
    pub fn d_tag(&self) -> Option<String> {
        if let Some(ref d) = self.d_tag {
            return Some(d.clone());
        }
        let event: Result<nostr_sdk::Event, _> = serde_json::from_str(&self.event_json);
        event.ok().and_then(|e| e.tags.identifier().map(|s| s.to_string()))
    }
}

#[derive(Clone, Debug, Default, Store)]
pub struct PublishQueueStore {
    pub events: Vec<QueuedEvent>,
}
