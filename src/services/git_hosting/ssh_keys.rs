//! SSH Keys Service
//!
//! Manages SSH public keys stored as Kind 52 events on Nostr.
//! Used for git authentication operations.
#![allow(dead_code)]
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;
use crate::stores::nostr_client::{fetch_events_aggregated, get_client, HAS_SIGNER};
/// Default timeout for fetching events
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// SSH public key stored as Kind 52 event
#[derive(Debug, Clone, PartialEq)]
pub struct SshKey {
    pub title: String,
    pub public_key: String,
    pub fingerprint: String,
    pub event_id: String,
    pub created_at: u64,
}
impl SshKey {
    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind != Kind::Custom(52) {
            return None;
        }
        let title = event
            .tags
            .iter()
            .find_map(|t| {
                let v = t.as_slice();
                if v.len() >= 2 && v[0] == "title" {
                    Some(v[1].to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "Untitled Key".to_string());
        let fingerprint = event
            .tags
            .iter()
            .find_map(|t| {
                let v = t.as_slice();
                if v.len() >= 2 && v[0] == "fingerprint" {
                    Some(v[1].to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        Some(Self {
            title,
            public_key: event.content.to_string(),
            fingerprint,
            event_id: event.id.to_hex(),
            created_at: event.created_at.as_secs(),
        })
    }
}
/// Fetch SSH keys for a pubkey
pub async fn fetch_ssh_keys(pubkey: &PublicKey) -> Result<Vec<SshKey>, String> {
    let filter = Filter::new()
        .kind(Kind::Custom(52))
        .author(*pubkey)
        .limit(50);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch SSH keys: {}", e))?;
    let mut keys: Vec<SshKey> = events.iter().filter_map(SshKey::from_event).collect();
    keys.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(keys)
}
/// Publish a new SSH key
pub async fn publish_ssh_key(title: &str, public_key: &str) -> Result<EventId, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".into());
    }
    let builder = EventBuilder::new(Kind::Custom(52), public_key)
        .tag(Tag::custom(TagKind::custom("title"), vec![title.to_string()]));
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish SSH key: {}", e))?;
    let event_id = *output.id();
    Ok(event_id)
}
/// Delete an SSH key by publishing a deletion event
pub async fn delete_ssh_key(event_id: EventId) -> Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".into());
    }
    use nostr::nips::nip09::EventDeletionRequest;
    let request = EventDeletionRequest::new().id(event_id).reason("SSH key deleted");
    let builder = EventBuilder::delete(request);
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to delete SSH key: {}", e))?;
    Ok(())
}
