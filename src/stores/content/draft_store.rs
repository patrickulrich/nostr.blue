//! NIP-37: Draft Wraps - Kind 31234
//! NIP-37: Relay List for Private Content - Kind 10013
//!
//! This module provides draft management for articles using NIP-37 compliant
//! encrypted draft storage. Drafts are published to private relays (Kind 10013).
use dioxus::prelude::*;
use nostr_sdk::{
    Alphabet, EventBuilder, Filter, Kind, SingleLetterTag, Tag, TagKind, Timestamp,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::stores::nostr_client;
#[cfg(target_arch = "wasm32")]
use js_sys;
/// Kind 31234 - Draft Wraps (NIP-37)
pub const KIND_DRAFT: u16 = 31234;
/// Kind 10013 - Relay List for Private Content (NIP-37)
pub const KIND_PRIVATE_RELAY_LIST: u16 = 10013;
/// Kind 30023 - Long-form Content / Articles (NIP-23)
pub const KIND_LONG_FORM: u16 = 30023;
/// Default expiration for drafts: 90 days
pub const DEFAULT_DRAFT_EXPIRATION_DAYS: u64 = 90;
/// Default private relays if user has none configured
pub const DEFAULT_PRIVATE_RELAYS: &[&str] = &["wss://relay.damus.io", "wss://nos.lol"];
/// Represents an article draft (maps to unsigned Kind 30023 event)
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ArticleDraft {
    pub title: String,
    pub summary: String,
    pub content: String,
    pub identifier: String,
    pub cover_image: String,
    pub hashtags: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}
impl ArticleDraft {
    /// Create a new draft with current timestamp
    /// Kept for future programmatic draft creation
    #[allow(dead_code)]
    pub fn new() -> Self {
        let now = current_timestamp();
        Self {
            created_at: now,
            updated_at: now,
            ..Default::default()
        }
    }
    /// Check if draft has any content worth saving
    pub fn has_content(&self) -> bool {
        !self.title.trim().is_empty() || !self.content.trim().is_empty()
    }
    /// Convert to unsigned Kind 30023 event JSON for encryption
    pub fn to_unsigned_event_json(&self, pubkey: &str) -> Result<String, String> {
        let now = current_timestamp();
        let mut tags: Vec<Vec<String>> = vec![
            vec!["d".to_string(), self.identifier.clone()],
            vec!["title".to_string(), self.title.clone()],
        ];
        if !self.summary.is_empty() {
            tags.push(vec!["summary".to_string(), self.summary.clone()]);
        }
        if !self.cover_image.is_empty() {
            tags.push(vec!["image".to_string(), self.cover_image.clone()]);
        }
        for tag in &self.hashtags {
            if !tag.trim().is_empty() {
                tags.push(vec!["t".to_string(), tag.trim().to_lowercase()]);
            }
        }
        let unsigned_event = serde_json::json!(
            { "kind" : KIND_LONG_FORM, "pubkey" : pubkey, "created_at" : now, "content" :
            self.content, "tags" : tags, }
        );
        serde_json::to_string(&unsigned_event)
            .map_err(|e| format!("Failed to serialize draft event: {}", e))
    }
    /// Parse from unsigned event JSON (after decryption)
    pub fn from_unsigned_event_json(json: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| format!("Failed to parse draft JSON: {}", e))?;
        let content = value
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let created_at = value.get("created_at").and_then(|v| v.as_u64()).unwrap_or(0);
        let tags = value
            .get("tags")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut draft = ArticleDraft {
            content,
            created_at,
            updated_at: created_at,
            ..Default::default()
        };
        for tag in tags {
            if let Some(arr) = tag.as_array() {
                if arr.len() >= 2 {
                    let key = arr[0].as_str().unwrap_or("");
                    let value = arr[1].as_str().unwrap_or("");
                    match key {
                        "d" => draft.identifier = value.to_string(),
                        "title" => draft.title = value.to_string(),
                        "summary" => draft.summary = value.to_string(),
                        "image" => draft.cover_image = value.to_string(),
                        "t" => draft.hashtags.push(value.to_string()),
                        _ => {}
                    }
                }
            }
        }
        Ok(draft)
    }
}
/// Draft save status for UI feedback
#[derive(Clone, Debug, Default, PartialEq)]
pub enum DraftStatus {
    /// No changes since last save (or new empty draft)
    #[default]
    Clean,
    /// Content has changed, needs saving
    Dirty,
    /// Currently saving to relays
    Saving,
    /// Successfully saved
    Saved { event_id: String, saved_at: u64 },
    /// Save failed
    Error(String),
}
/// A loaded draft with metadata
#[derive(Clone, Debug)]
pub struct LoadedDraft {
    pub draft: ArticleDraft,
    /// Event ID of the draft wrap - kept for future draft deletion by event ID
    #[allow(dead_code)]
    pub event_id: String,
    /// Identifier for direct access - kept for future deep linking to drafts
    #[allow(dead_code)]
    pub identifier: String,
}
/// Cached private relay URLs (Kind 10013)
pub static PRIVATE_RELAYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);
/// Flag indicating if private relays have been loaded
pub static PRIVATE_RELAYS_LOADED: GlobalSignal<bool> = Signal::global(|| false);
/// Fetch and decrypt Kind 10013 private relay list
pub async fn get_private_relays() -> Result<Vec<String>, String> {
    if *PRIVATE_RELAYS_LOADED.read() {
        let relays = PRIVATE_RELAYS.read().clone();
        if !relays.is_empty() {
            return Ok(relays);
        }
    }
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let signer = client.signer().await.map_err(|e| format!("No signer: {}", e))?;
    let pubkey = nostr_client::get_cached_pubkey()?;
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(KIND_PRIVATE_RELAY_LIST))
        .limit(1);
    let events = client
        .fetch_events(filter, Duration::from_secs(5))
        .await
        .map_err(|e| format!("Failed to fetch private relay list: {}", e))?;
    let Some(event) = events.into_iter().next() else {
        log::info!("No Kind 10013 found, will use defaults");
        return Ok(default_private_relays());
    };
    if event.content.is_empty() {
        log::info!("Kind 10013 has empty content, using defaults");
        return Ok(default_private_relays());
    }
    let decrypted = signer
        .nip44_decrypt(&event.pubkey, &event.content)
        .await
        .map_err(|e| format!("Failed to decrypt Kind 10013: {}", e))?;
    let tag_arrays: Vec<Vec<String>> = serde_json::from_str(&decrypted)
        .map_err(|e| format!("Invalid Kind 10013 format: {}", e))?;
    let mut relays = Vec::new();
    for arr in tag_arrays {
        if arr.len() >= 2 && arr[0] == "relay" {
            relays.push(arr[1].clone());
        }
    }
    *PRIVATE_RELAYS.write() = relays.clone();
    *PRIVATE_RELAYS_LOADED.write() = true;
    log::info!("Loaded {} private relays from Kind 10013", relays.len());
    Ok(relays)
}
/// Set and publish Kind 10013 private relay list
pub async fn set_private_relays(relays: Vec<String>) -> Result<String, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let signer = client.signer().await.map_err(|e| format!("No signer: {}", e))?;
    let pubkey = nostr_client::get_cached_pubkey()?;
    let tag_arrays: Vec<Vec<String>> = relays
        .iter()
        .map(|url| vec!["relay".to_string(), url.clone()])
        .collect();
    let json = serde_json::to_string(&tag_arrays)
        .map_err(|e| format!("Failed to serialize relays: {}", e))?;
    let encrypted = signer
        .nip44_encrypt(&pubkey, &json)
        .await
        .map_err(|e| format!("Failed to encrypt: {}", e))?;
    let builder = EventBuilder::new(Kind::from(KIND_PRIVATE_RELAY_LIST), encrypted);
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish Kind 10013: {}", e))?;
    *PRIVATE_RELAYS.write() = relays;
    *PRIVATE_RELAYS_LOADED.write() = true;
    log::info!("Published Kind 10013: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}
/// Ensure user has private relays configured, creating default if needed
pub async fn ensure_private_relays() -> Result<Vec<String>, String> {
    let relays = get_private_relays().await?;
    if relays.is_empty() {
        let defaults = default_private_relays();
        set_private_relays(defaults.clone()).await?;
        Ok(defaults)
    } else {
        Ok(relays)
    }
}
/// Get default private relays
pub fn default_private_relays() -> Vec<String> {
    DEFAULT_PRIVATE_RELAYS.iter().map(|s| s.to_string()).collect()
}
/// Save a draft as Kind 31234 (encrypted, to private relays)
pub async fn save_draft(draft: &ArticleDraft) -> Result<String, String> {
    if !draft.has_content() {
        return Err("Draft has no content to save".to_string());
    }
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let signer = client.signer().await.map_err(|e| format!("No signer: {}", e))?;
    let pubkey = nostr_client::get_cached_pubkey()?;
    let private_relays = ensure_private_relays().await?;
    if private_relays.is_empty() {
        return Err("No private relays configured".to_string());
    }
    let unsigned_json = draft.to_unsigned_event_json(&pubkey.to_hex())?;
    let encrypted = signer
        .nip44_encrypt(&pubkey, &unsigned_json)
        .await
        .map_err(|e| format!("Failed to encrypt draft: {}", e))?;
    let expiration = Timestamp::now()
        + Duration::from_secs(DEFAULT_DRAFT_EXPIRATION_DAYS * 24 * 60 * 60);
    let tags = vec![
        Tag::identifier(&draft.identifier),
        Tag::custom(TagKind::Custom("k".into()), vec![KIND_LONG_FORM.to_string()]),
        Tag::expiration(expiration),
    ];
    let builder = EventBuilder::new(Kind::from(KIND_DRAFT), encrypted).tags(tags);
    let event = client
        .sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign draft event: {}", e))?;
    let result = nostr_client::send_presigned_event_to_relays(
            event.clone(),
            private_relays,
        )
        .await?;
    log::info!(
        "Draft saved: {} ({}/{} relays succeeded)", result.event_id, result
        .success_count(), result.total_attempted()
    );
    Ok(result.event_id)
}
/// Load all drafts for the current user
pub async fn load_drafts() -> Result<Vec<LoadedDraft>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let signer = client.signer().await.map_err(|e| format!("No signer: {}", e))?;
    let pubkey = nostr_client::get_cached_pubkey()?;
    let private_relays = get_private_relays()
        .await
        .unwrap_or_else(|_| default_private_relays());
    for relay_url in &private_relays {
        if let Err(e) = client.add_relay(relay_url.as_str()).await {
            log::debug!("Could not add private relay {}: {}", relay_url, e);
        }
    }
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(KIND_DRAFT))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::K), KIND_LONG_FORM.to_string());
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch drafts: {}", e))?;
    let mut drafts = Vec::new();
    for event in events {
        if event.content.is_empty() {
            continue;
        }
        let identifier = event
            .tags
            .iter()
            .find(|t| t.kind() == TagKind::d())
            .and_then(|t| t.content().map(|s| s.to_string()))
            .unwrap_or_default();
        match signer.nip44_decrypt(&event.pubkey, &event.content).await {
            Ok(decrypted) => {
                match ArticleDraft::from_unsigned_event_json(&decrypted) {
                    Ok(mut draft) => {
                        if draft.identifier.is_empty() {
                            draft.identifier = identifier.clone();
                        }
                        drafts
                            .push(LoadedDraft {
                                draft,
                                event_id: event.id.to_hex(),
                                identifier,
                            });
                    }
                    Err(e) => {
                        log::warn!("Failed to parse draft {}: {}", event.id.to_hex(), e);
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to decrypt draft {}: {}", event.id.to_hex(), e);
            }
        }
    }
    drafts.sort_by(|a, b| b.draft.updated_at.cmp(&a.draft.updated_at));
    log::info!("Loaded {} drafts", drafts.len());
    Ok(drafts)
}
/// Load a specific draft by identifier
/// Kept for future deep linking to specific drafts via URL parameter
#[allow(dead_code)]
pub async fn load_draft(identifier: &str) -> Result<Option<ArticleDraft>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let signer = client.signer().await.map_err(|e| format!("No signer: {}", e))?;
    let pubkey = nostr_client::get_cached_pubkey()?;
    let private_relays = get_private_relays()
        .await
        .unwrap_or_else(|_| default_private_relays());
    for relay_url in &private_relays {
        if let Err(e) = client.add_relay(relay_url.as_str()).await {
            log::debug!("Could not add private relay {}: {}", relay_url, e);
        }
    }
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(KIND_DRAFT))
        .identifier(identifier)
        .limit(1);
    let events = client
        .fetch_events(filter, Duration::from_secs(5))
        .await
        .map_err(|e| format!("Failed to fetch draft: {}", e))?;
    let Some(event) = events.into_iter().next() else {
        return Ok(None);
    };
    if event.content.is_empty() {
        return Ok(None);
    }
    let decrypted = signer
        .nip44_decrypt(&event.pubkey, &event.content)
        .await
        .map_err(|e| format!("Failed to decrypt draft: {}", e))?;
    let draft = ArticleDraft::from_unsigned_event_json(&decrypted)?;
    Ok(Some(draft))
}
/// Delete a draft by publishing with empty content
pub async fn delete_draft(identifier: &str) -> Result<(), String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let private_relays = get_private_relays()
        .await
        .unwrap_or_else(|_| default_private_relays());
    let tags = vec![
        Tag::identifier(identifier),
        Tag::custom(TagKind::Custom("k".into()), vec![KIND_LONG_FORM.to_string()]),
    ];
    let builder = EventBuilder::new(Kind::from(KIND_DRAFT), "").tags(tags);
    let event = client
        .sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign delete event: {}", e))?;
    nostr_client::send_presigned_event_to_relays(event, private_relays).await?;
    log::info!("Draft deleted: {}", identifier);
    Ok(())
}
/// Get current WASM-compatible timestamp
fn current_timestamp() -> u64 {
    #[cfg(target_arch = "wasm32")] { (js_sys::Date::now() / 1000.0) as u64 }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}
/// Calculate content hash for dirty state tracking
/// Kept for future draft-level change detection
#[allow(dead_code)]
pub fn calculate_draft_hash(draft: &ArticleDraft) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    draft.title.hash(&mut hasher);
    draft.summary.hash(&mut hasher);
    draft.content.hash(&mut hasher);
    draft.identifier.hash(&mut hasher);
    draft.cover_image.hash(&mut hasher);
    draft.hashtags.hash(&mut hasher);
    hasher.finish()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_article_draft_default() {
        let draft = ArticleDraft::default();
        assert!(draft.title.is_empty());
        assert!(!draft.has_content());
    }
    #[test]
    fn test_article_draft_has_content() {
        let mut draft = ArticleDraft::default();
        assert!(!draft.has_content());
        draft.title = "Test".to_string();
        assert!(draft.has_content());
        draft.title = String::new();
        draft.content = "Some content".to_string();
        assert!(draft.has_content());
    }
    #[test]
    fn test_draft_status_default() {
        let status = DraftStatus::default();
        assert_eq!(status, DraftStatus::Clean);
    }
    #[test]
    fn test_draft_hash_changes() {
        let mut draft = ArticleDraft::default();
        let hash1 = calculate_draft_hash(&draft);
        draft.title = "Changed".to_string();
        let hash2 = calculate_draft_hash(&draft);
        assert_ne!(hash1, hash2);
    }
}
