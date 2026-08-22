//! NIP-65: Relay List Metadata (kind 10002)
//! NIP-17: Private Direct Message Relay Lists (kind 10050)
//!
//! This module provides centralized relay management using Nostr-native relay lists.
//! It implements the Outbox model for intelligent relay routing.

#[cfg(all(feature = "web", feature = "native"))]
compile_error!("Cannot enable both 'web' and 'native' features simultaneously");

#[cfg(not(any(feature = "web", feature = "native")))]
compile_error!("Must enable either 'web' or 'native' feature");

#[cfg(feature = "web")]
use crate::platform::storage;
use crate::stores::nostr_client;
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr_sdk::{
    Client, EventBuilder, Filter, FromBech32, Kind, PublicKey, RelayUrl, SubscriptionId, Tag,
    TagKind, Timestamp,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use std::collections::HashSet;
#[cfg(all(feature = "native", not(feature = "web")))]
use std::fs;
use std::sync::Arc;
use std::time::Duration;
/// Configuration for a single relay with read/write permissions
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RelayConfig {
    pub url: String,
    pub read: bool,
    pub write: bool,
}
/// Complete relay metadata for a user (both kind 10002 and 10050)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RelayListMetadata {
    pub relays: Vec<RelayConfig>,
    pub dm_relays: Vec<String>,
    #[serde(default)]
    pub updated_at: u64,
}
/// Current user's relay metadata
pub static USER_RELAY_METADATA: GlobalSignal<Option<RelayListMetadata>> = Signal::global(|| None);
/// Default NIP-65 relay URLs
pub const DEFAULT_NIP65_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.snort.social",
];
/// Default DM inbox relay URLs (NIP-17)
pub const DEFAULT_DM_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://auth.nostr1.com",
    "wss://relay.0xchat.com",
];
/// Default relays to use when no kind 10002 is found
pub fn default_relays() -> Vec<RelayConfig> {
    DEFAULT_NIP65_RELAYS
        .iter()
        .map(|url| RelayConfig {
            url: url.to_string(),
            read: true,
            write: true,
        })
        .collect()
}
/// Default DM relays to use when no kind 10050 is found
pub fn default_dm_relays() -> Vec<String> {
    DEFAULT_DM_RELAYS.iter().map(|s| s.to_string()).collect()
}
/// Search relays (NIP-51 kind 10007)
pub static SEARCH_RELAYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);
/// Blocked relays (NIP-51 kind 10006)
pub static BLOCKED_RELAYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);
/// Local relays (stored in browser, not published to Nostr)
pub static LOCAL_RELAYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);
/// Broadcast relays (stored locally, used for manual re-broadcasting only)
pub static BROADCAST_RELAYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);
/// Indexer relays (gift-wrapped NIP-59 kind 10086) for discovering user metadata
pub static INDEXER_RELAYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);
/// Proxy relays (gift-wrapped NIP-59 kind 10087) for fallback when users lack NIP-65
pub static PROXY_RELAYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);
/// Trusted relays (gift-wrapped NIP-59 kind 10089) for sensitive operations
pub static TRUSTED_RELAYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);
/// Private outbox relays (kind 10013) for personal storage
pub static OUTBOX_RELAYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);
/// Favorite/feed relays (kind 10012) for feed aggregation
pub static FAVORITE_RELAYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);
pub const DEFAULT_INDEXER_RELAYS: &[&str] = &[
    "wss://purplepag.es",
    "wss://indexer.coracle.social",
    "wss://user.kindpag.es",
    "wss://directory.yabu.me",
    "wss://profiles.nostr1.com",
    "wss://relay.nos.social",
];
pub const DEFAULT_FAVORITE_RELAYS: &[&str] = &[
    "wss://nostr.wine",
    "wss://news.utxo.one",
];
pub fn default_indexer_relays() -> Vec<String> {
    DEFAULT_INDEXER_RELAYS.iter().map(|s| s.to_string()).collect()
}
#[allow(dead_code)]
pub fn default_outbox_relays() -> Vec<String> {
    vec![]
}
#[allow(dead_code)]
pub fn default_proxy_relays() -> Vec<String> {
    vec![]
}
#[allow(dead_code)]
pub fn default_trusted_relays() -> Vec<String> {
    vec![]
}
pub fn default_favorite_relays() -> Vec<String> {
    DEFAULT_FAVORITE_RELAYS.iter().map(|s| s.to_string()).collect()
}
pub fn get_indexer_relay_urls() -> Vec<String> {
    let relays = INDEXER_RELAYS.peek().clone();
    if relays.is_empty() {
        default_indexer_relays()
    } else {
        relays
    }
}
/// Default search relay URLs (NIP-50 compatible)
pub const DEFAULT_SEARCH_RELAYS: &[&str] = &["wss://relay.nostr.band", "wss://search.nos.today"];
/// Default search relays to use when no kind 10007 is found
pub fn default_search_relays() -> Vec<String> {
    DEFAULT_SEARCH_RELAYS
        .iter()
        .map(|s| s.to_string())
        .collect()
}
/// Reset general relays to defaults (local change only)
pub fn reset_general_relays_to_default() {
    let mut metadata = USER_RELAY_METADATA.write();
    match metadata.as_mut() {
        Some(m) => m.relays = default_relays(),
        None => {
            *metadata = Some(RelayListMetadata {
                relays: default_relays(),
                dm_relays: default_dm_relays(),
                updated_at: 0,
            });
        }
    }
}
/// Reset DM relays to defaults (local change only)
pub fn reset_dm_relays_to_default() {
    let mut metadata = USER_RELAY_METADATA.write();
    match metadata.as_mut() {
        Some(m) => m.dm_relays = default_dm_relays(),
        None => {
            *metadata = Some(RelayListMetadata {
                relays: default_relays(),
                dm_relays: default_dm_relays(),
                updated_at: 0,
            });
        }
    }
}
fn parse_relay_tags(tags: &nostr_sdk::Tags) -> Vec<String> {
    tags.iter()
        .filter_map(|tag| {
            if tag.kind() == TagKind::Custom("relay".into()) {
                tag.content().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect()
}
#[allow(dead_code)]
pub fn reset_indexer_relays_to_default() {
    *INDEXER_RELAYS.write() = default_indexer_relays();
}
#[allow(dead_code)]
pub fn reset_outbox_relays_to_default() {
    *OUTBOX_RELAYS.write() = vec![];
}
#[allow(dead_code)]
pub fn reset_proxy_relays_to_default() {
    *PROXY_RELAYS.write() = vec![];
}
#[allow(dead_code)]
pub fn reset_trusted_relays_to_default() {
    *TRUSTED_RELAYS.write() = vec![];
}
#[allow(dead_code)]
pub fn reset_favorite_relays_to_default() {
    *FAVORITE_RELAYS.write() = default_favorite_relays();
}
/// Get the user's read-enabled relays (for discovering content)
/// Returns relays from kind 10002 with read flag set, or defaults if none configured
/// Reserved for future outbox model optimization
#[allow(dead_code)]
pub fn get_read_relays() -> Vec<String> {
    let metadata = USER_RELAY_METADATA.read();
    match metadata.as_ref() {
        Some(m) => m
            .relays
            .iter()
            .filter(|r| r.read)
            .map(|r| r.url.clone())
            .collect(),
        None => DEFAULT_NIP65_RELAYS.iter().map(|s| s.to_string()).collect(),
    }
}
/// Get the user's write-enabled relays (for publishing content)
/// Returns relays from kind 10002 with write flag set, or defaults if none configured
pub fn get_write_relays() -> Vec<String> {
    let metadata = USER_RELAY_METADATA.read();
    match metadata.as_ref() {
        Some(m) => m
            .relays
            .iter()
            .filter(|r| r.write)
            .map(|r| r.url.clone())
            .collect(),
        None => DEFAULT_NIP65_RELAYS.iter().map(|s| s.to_string()).collect(),
    }
}
/// Get the user's DM inbox relays (for private messages)
/// Returns relays from kind 10050, or defaults if none configured
#[allow(dead_code)]
pub fn get_dm_relays() -> Vec<String> {
    let metadata = USER_RELAY_METADATA.read();
    match metadata.as_ref() {
        Some(m) if !m.dm_relays.is_empty() => m.dm_relays.clone(),
        _ => DEFAULT_DM_RELAYS.iter().map(|s| s.to_string()).collect(),
    }
}

/// Get only the user's kind 10050 DM relays (no fallback)
/// Used by ensure_dm_relays_connected for tiered fallback logic
pub fn get_dm_relays_10050_only() -> Vec<String> {
    let metadata = USER_RELAY_METADATA.read();
    match metadata.as_ref() {
        Some(m) if !m.dm_relays.is_empty() => m.dm_relays.clone(),
        _ => Vec::new(),
    }
}
/// Parse relay list from kind 10002 event
/// NIP-65 tag format:
/// - ["r", "wss://relay.url"] = both read and write
/// - ["r", "wss://relay.url", "read"] = read only
/// - ["r", "wss://relay.url", "write"] = write only
pub fn parse_relay_list_event(event: &nostr_sdk::Event) -> Vec<RelayConfig> {
    let mut relays = Vec::new();
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        if slice.first().map(|s| s.as_str()) != Some("r") {
            continue;
        }
        let Some(url) = slice.get(1) else {
            continue;
        };
        let marker = slice.get(2).map(|s| s.as_str());
        let (read, write) = match marker {
            None => (true, true),
            Some("read") => (true, false),
            Some("write") => (false, true),
            Some(unknown) => {
                log::warn!("Unknown relay marker '{}' for {}, skipping", unknown, url);
                continue;
            }
        };
        log::debug!("Found relay tag: {} (read={}, write={})", url, read, write);
        relays.push(RelayConfig {
            url: crate::utils::relay::upgrade_to_secure_relay_url(url),
            read,
            write,
        });
    }
    log::info!("Parsed {} relays from event", relays.len());
    relays
}
/// Parse DM relay list from kind 10050 event
/// NIP-17 tag format: ["relay", "wss://relay.url"]
pub fn parse_dm_relay_list(event: &nostr_sdk::Event) -> Vec<String> {
    let mut dm_relays = Vec::new();
    for tag in event.tags.iter() {
        if tag.kind() == TagKind::Custom("relay".into()) {
            if let Some(content) = tag.content() {
                dm_relays.push(crate::utils::relay::upgrade_to_secure_relay_url(content));
            }
        }
    }
    dm_relays
}
/// Fetch relay list (kind 10002) and DM relay list (kind 10050) for a user
///
/// # Arguments
/// * `pubkey` - The public key of the user to fetch relay lists for
/// * `client` - The Nostr client instance
pub async fn fetch_relay_list(
    pubkey: PublicKey,
    client: Arc<Client>,
) -> Result<RelayListMetadata, String> {
    log::info!("Fetching relay lists for {}", pubkey.to_hex());
    let filter_10002 = Filter::new().author(pubkey).kind(Kind::RelayList).limit(1);
    let filter_10050 = Filter::new()
        .author(pubkey)
        .kind(Kind::from(10050))
        .limit(1);
    let client_10002 = client.clone();
    let client_10050 = client.clone();
    let (result_10002, result_10050) = tokio::join!(
        client_10002.fetch_events(filter_10002, Duration::from_secs(5)),
        client_10050.fetch_events(filter_10050, Duration::from_secs(5))
    );
    let mut relays = Vec::new();
    let mut dm_relays = Vec::new();
    let mut updated_at = 0u64;
    match result_10002 {
        Ok(events) => {
            let event_count = events.len();
            log::info!("Received {} kind 10002 events", event_count);
            if let Some(event) = events.into_iter().next() {
                log::info!("Parsing kind 10002 event with {} tags", event.tags.len());
                relays = parse_relay_list_event(&event);
                updated_at = event.created_at.as_secs();
                log::info!("Parsed {} general relays from kind 10002", relays.len());
                for relay in &relays {
                    log::debug!(
                        "  - {} (read: {}, write: {})",
                        relay.url,
                        relay.read,
                        relay.write
                    );
                }
            } else {
                log::warn!("No kind 10002 events found for user");
            }
        }
        Err(e) => {
            log::error!("Failed to fetch kind 10002: {}", e);
        }
    }
    match result_10050 {
        Ok(events) => {
            let event_count = events.len();
            log::info!("Received {} kind 10050 events", event_count);
            if let Some(event) = events.into_iter().next() {
                log::info!("Parsing kind 10050 event with {} tags", event.tags.len());
                dm_relays = parse_dm_relay_list(&event);
                updated_at = updated_at.max(event.created_at.as_secs());
                log::info!("Parsed {} DM relays from kind 10050", dm_relays.len());
                for relay in &dm_relays {
                    log::debug!("  - {}", relay);
                }
            } else {
                log::warn!("No kind 10050 events found for user");
            }
        }
        Err(e) => {
            log::error!("Failed to fetch kind 10050: {}", e);
        }
    }
    if relays.is_empty() && dm_relays.is_empty() {
        return Err("No relay lists found".to_string());
    }
    Ok(RelayListMetadata {
        relays,
        dm_relays,
        updated_at,
    })
}
/// Publish relay list (kind 10002) using rust-nostr's EventBuilder
///
/// # Arguments
/// * `relays` - List of relay configurations to publish
/// * `client` - The Nostr client instance
pub async fn publish_relay_list(
    relays: Vec<RelayConfig>,
    client: Arc<Client>,
) -> Result<String, String> {
    log::info!("Publishing relay list with {} relays", relays.len());
    let tags: Vec<Tag> = relays
        .into_iter()
        .filter_map(|r| {
            let marker = match (r.read, r.write) {
                (true, true) => vec![r.url],
                (true, false) => vec![r.url, "read".to_string()],
                (false, true) => vec![r.url, "write".to_string()],
                (false, false) => return None,
            };
            Some(Tag::custom(TagKind::Custom("r".into()), marker))
        })
        .collect();
    let builder = EventBuilder::new(Kind::RelayList, "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event.clone(),
        crate::stores::publish_queue::types::QueueEventType::RelayList,
        None,
        std::collections::HashMap::new(),
    ).await;
    // Advertise the kind 10002 to indexer relays (NIP-65: spread to well-known
    // public indexers). Indexers are DISCOVERY-only and can't be reached via
    // the publish queue (which targets WRITE-flagged relays), so use the
    // dedicated ephemeral-publish helper.
    let _ = publish_event_to_indexers(&client, &event).await;
    Ok(event_id)
}
/// Publish DM relay list (kind 10050)
///
/// # Arguments
/// * `dm_relays` - List of DM inbox relay URLs to publish
/// * `client` - The Nostr client instance
pub async fn publish_dm_relay_list(
    dm_relays: Vec<String>,
    client: Arc<Client>,
) -> Result<String, String> {
    log::info!("Publishing DM relay list with {} relays", dm_relays.len());
    let tags: Vec<Tag> = dm_relays
        .into_iter()
        .map(|url| Tag::custom(TagKind::Custom("relay".into()), vec![url]))
        .collect();
    let builder = EventBuilder::new(Kind::from(10050), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event.clone(),
        crate::stores::publish_queue::types::QueueEventType::RelayList,
        None,
        std::collections::HashMap::new(),
    ).await;
    // Advertise kind 10050 to indexer relays so DM addressing can discover it.
    let _ = publish_event_to_indexers(&client, &event).await;
    Ok(event_id)
}
/// Initialize relay lists for current user on startup
/// This is called once when the client starts up with a signer.
///
/// This fetches the user's own relay lists (kind 10002/10050) for Settings UI display.
/// The SDK's gossip feature handles staleness and routing automatically - this is just
/// for showing the user their current published relay configuration.
///
/// # Arguments
/// * `client` - The Nostr client instance
pub async fn init_user_relay_lists(client: Arc<Client>) -> Result<(), String> {
    let user_pubkey = nostr_client::get_cached_pubkey().map_err(|_| "No signer attached")?;
    log::info!(
        "Loading relay lists for Settings UI for {}",
        user_pubkey.to_hex()
    );
    match fetch_relay_list(user_pubkey, client.clone()).await {
        Ok(metadata) => {
            log::info!(
                "Loaded {} general relays and {} DM relays",
                metadata.relays.len(),
                metadata.dm_relays.len()
            );

            let blocked = BLOCKED_RELAYS.peek().clone();
            for relay_config in &metadata.relays {
                let normalized = relay_config.url.trim_end_matches('/');
                if blocked
                    .iter()
                    .any(|b| b.trim_end_matches('/') == normalized)
                {
                    log::info!("Skipping blocked NIP-65 relay: {}", relay_config.url);
                    continue;
                }
                if let Ok(url) = RelayUrl::parse(&relay_config.url) {
                    match client.add_relay(url).await {
                        Ok(added) => {
                            log::info!("NIP-65 relay {} (new={})", relay_config.url, added)
                        }
                        Err(e) => {
                            log::debug!("NIP-65 relay {} skipped: {}", relay_config.url, e)
                        }
                    }
                }
            }
            for dm_relay in &metadata.dm_relays {
                let normalized = dm_relay.trim_end_matches('/');
                if blocked
                    .iter()
                    .any(|b| b.trim_end_matches('/') == normalized)
                {
                    log::info!("Skipping blocked DM relay: {}", dm_relay);
                    continue;
                }
                if let Ok(url) = RelayUrl::parse(dm_relay) {
                    match client.add_relay(url).await {
                        Ok(added) => log::info!("DM relay {} (new={})", dm_relay, added),
                        Err(e) => log::debug!("DM relay {} skipped: {}", dm_relay, e),
                    }
                }
            }

            *USER_RELAY_METADATA.write() = Some(metadata);
            crate::services::search_relays::invalidate_search_relay_cache().await;
            log::debug!("Invalidated search relay cache after NIP-65 update");
            Ok(())
        }
        Err(e) => {
            log::warn!("No relay lists found: {}, using defaults", e);

            let blocked = BLOCKED_RELAYS.peek().clone();
            for dm_relay_url in default_dm_relays() {
                let normalized = dm_relay_url.trim_end_matches('/');
                if blocked
                    .iter()
                    .any(|b| b.trim_end_matches('/') == normalized)
                {
                    continue;
                }
                if let Ok(url) = RelayUrl::parse(&dm_relay_url) {
                    let _ = client.add_relay(url).await;
                }
            }

            let default = RelayListMetadata {
                relays: default_relays(),
                dm_relays: default_dm_relays(),
                updated_at: 0,
            };
            *USER_RELAY_METADATA.write() = Some(default);
            crate::services::search_relays::invalidate_search_relay_cache().await;
            log::debug!("Invalidated search relay cache after NIP-65 fallback");
            log::info!(
                "Using default relays. User can configure and publish their relay lists."
            );
            Ok(())
        }
    }
}
/// Publish search relays (kind 10007)
/// SDK: EventBuilder::search_relays() creates ["relay", "url"] tags
pub async fn publish_search_relays(
    relays: Vec<String>,
    _client: Arc<Client>,
) -> Result<String, String> {
    log::info!("Publishing search relay list with {} relays", relays.len());
    let urls: Vec<RelayUrl> = relays
        .iter()
        .filter_map(|s| RelayUrl::parse(s).ok())
        .collect();
    let builder = EventBuilder::search_relays(urls);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::RelayList,
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok(event_id)
}
/// Publish blocked relays (kind 10006)
/// SDK: EventBuilder::blocked_relays() creates ["relay", "url"] tags
pub async fn publish_blocked_relays(
    relays: Vec<String>,
    _client: Arc<Client>,
) -> Result<String, String> {
    log::info!("Publishing blocked relay list with {} relays", relays.len());
    let urls: Vec<RelayUrl> = relays
        .iter()
        .filter_map(|s| RelayUrl::parse(s).ok())
        .collect();
    let builder = EventBuilder::blocked_relays(urls);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::RelayList,
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok(event_id)
}
pub async fn publish_outbox_relays(
    relays: Vec<String>,
    _client: Arc<Client>,
) -> Result<String, String> {
    log::info!("Publishing outbox relay list with {} relays", relays.len());
    let urls: Vec<RelayUrl> = relays
        .iter()
        .filter_map(|s| RelayUrl::parse(s).ok())
        .collect();
    let tags: Vec<Tag> = urls.into_iter().map(Tag::relay).collect();
    let builder = EventBuilder::new(Kind::Custom(10013), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::RelayList,
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(event_id)
}
pub async fn publish_favorite_relays(
    relays: Vec<String>,
    _client: Arc<Client>,
) -> Result<String, String> {
    log::info!("Publishing favorite relay list with {} relays", relays.len());
    let urls: Vec<RelayUrl> = relays
        .iter()
        .filter_map(|s| RelayUrl::parse(s).ok())
        .collect();
    let tags: Vec<Tag> = urls.into_iter().map(Tag::relay).collect();
    let builder = EventBuilder::new(Kind::Custom(10012), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::RelayList,
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(event_id)
}
pub async fn publish_indexer_relays(
    relays: Vec<String>,
    _client: Arc<Client>,
) -> Result<String, String> {
    log::info!("Publishing indexer relay list (gift-wrapped) with {} relays", relays.len());
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("No signer: {}", e))?;
    let my_pubkey = crate::stores::nostr_client::get_cached_pubkey()?;
    let urls: Vec<RelayUrl> = relays
        .iter()
        .filter_map(|s| RelayUrl::parse(s).ok())
        .collect();
    let tags: Vec<Tag> = urls.into_iter().map(Tag::relay).collect();
    let rumor = EventBuilder::new(Kind::Custom(10086), "")
        .tags(tags)
        .build(my_pubkey);
    let gift_wrap = EventBuilder::gift_wrap(&signer, &my_pubkey, rumor, [])
        .await
        .map_err(|e| format!("Failed to gift wrap: {}", e))?;
    let event_id = gift_wrap.id.to_hex();
    crate::stores::publish_queue::enqueue(
        gift_wrap,
        crate::stores::publish_queue::types::QueueEventType::RelayList,
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(event_id)
}
pub async fn publish_proxy_relays(
    relays: Vec<String>,
    _client: Arc<Client>,
) -> Result<String, String> {
    log::info!("Publishing proxy relay list (gift-wrapped) with {} relays", relays.len());
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("No signer: {}", e))?;
    let my_pubkey = crate::stores::nostr_client::get_cached_pubkey()?;
    let urls: Vec<RelayUrl> = relays
        .iter()
        .filter_map(|s| RelayUrl::parse(s).ok())
        .collect();
    let tags: Vec<Tag> = urls.into_iter().map(Tag::relay).collect();
    let rumor = EventBuilder::new(Kind::Custom(10087), "")
        .tags(tags)
        .build(my_pubkey);
    let gift_wrap = EventBuilder::gift_wrap(&signer, &my_pubkey, rumor, [])
        .await
        .map_err(|e| format!("Failed to gift wrap: {}", e))?;
    let event_id = gift_wrap.id.to_hex();
    crate::stores::publish_queue::enqueue(
        gift_wrap,
        crate::stores::publish_queue::types::QueueEventType::RelayList,
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(event_id)
}
pub async fn publish_trusted_relays(
    relays: Vec<String>,
    _client: Arc<Client>,
) -> Result<String, String> {
    log::info!("Publishing trusted relay list (gift-wrapped) with {} relays", relays.len());
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("No signer: {}", e))?;
    let my_pubkey = crate::stores::nostr_client::get_cached_pubkey()?;
    let urls: Vec<RelayUrl> = relays
        .iter()
        .filter_map(|s| RelayUrl::parse(s).ok())
        .collect();
    let tags: Vec<Tag> = urls.into_iter().map(Tag::relay).collect();
    let rumor = EventBuilder::new(Kind::Custom(10089), "")
        .tags(tags)
        .build(my_pubkey);
    let gift_wrap = EventBuilder::gift_wrap(&signer, &my_pubkey, rumor, [])
        .await
        .map_err(|e| format!("Failed to gift wrap: {}", e))?;
    let event_id = gift_wrap.id.to_hex();
    crate::stores::publish_queue::enqueue(
        gift_wrap,
        crate::stores::publish_queue::types::QueueEventType::RelayList,
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(event_id)
}
/// Fetch search relays (kind 10007) for a user
pub async fn fetch_search_relays(
    pubkey: PublicKey,
    client: Arc<Client>,
) -> Result<Vec<String>, String> {
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::SearchRelays)
        .limit(1);
    let events = client
        .fetch_events(filter, Duration::from_secs(5))
        .await
        .map_err(|e| e.to_string())?;
    let relays = events
        .into_iter()
        .next()
        .map(|event| {
            event
                .tags
                .iter()
                .filter_map(|tag| {
                    if tag.kind() == TagKind::Custom("relay".into()) {
                        tag.content().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(relays)
}
/// Fetch blocked relays (kind 10006) for a user
pub async fn fetch_blocked_relays(
    pubkey: PublicKey,
    client: Arc<Client>,
) -> Result<Vec<String>, String> {
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::BlockedRelays)
        .limit(1);
    let events = client
        .fetch_events(filter, Duration::from_secs(5))
        .await
        .map_err(|e| e.to_string())?;
    let relays = events
        .into_iter()
        .next()
        .map(|event| {
            event
                .tags
                .iter()
                .filter_map(|tag| {
                    if tag.kind() == TagKind::Custom("relay".into()) {
                        tag.content().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(relays)
}
pub async fn fetch_outbox_relays(
    pubkey: PublicKey,
    client: Arc<Client>,
) -> Result<Vec<String>, String> {
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::Custom(10013))
        .limit(1);
    let events = client
        .fetch_events(filter, Duration::from_secs(5))
        .await
        .map_err(|e| e.to_string())?;
    let relays = events
        .into_iter()
        .next()
        .map(|event| parse_relay_tags(&event.tags))
        .unwrap_or_default();
    Ok(relays)
}
pub async fn fetch_favorite_relays(
    pubkey: PublicKey,
    client: Arc<Client>,
) -> Result<Vec<String>, String> {
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::Custom(10012))
        .limit(1);
    let events = client
        .fetch_events(filter, Duration::from_secs(5))
        .await
        .map_err(|e| e.to_string())?;
    let relays = events
        .into_iter()
        .next()
        .map(|event| parse_relay_tags(&event.tags))
        .unwrap_or_default();
    Ok(relays)
}
pub async fn init_private_relay_lists(client: Arc<Client>) -> Result<(), String> {
    let my_pubkey = crate::stores::nostr_client::get_cached_pubkey()?;
    let filter = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(my_pubkey)
        .since(Timestamp::now() - 2_592_000u64)
        .limit(50);
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| e.to_string())?;
    let mut indexer = vec![];
    let mut proxy = vec![];
    let mut trusted = vec![];
    let mut found_indexer = false;
    let mut found_proxy = false;
    let mut found_trusted = false;
    for event in events.iter() {
        if found_indexer && found_proxy && found_trusted {
            break;
        }
        match client.unwrap_gift_wrap(event).await {
            Ok(unwrapped) => match unwrapped.rumor.kind.as_u16() {
                10086 if !found_indexer => {
                    found_indexer = true;
                    indexer = parse_relay_tags(&unwrapped.rumor.tags);
                }
                10087 if !found_proxy => {
                    found_proxy = true;
                    proxy = parse_relay_tags(&unwrapped.rumor.tags);
                }
                10089 if !found_trusted => {
                    found_trusted = true;
                    trusted = parse_relay_tags(&unwrapped.rumor.tags);
                }
                _ => {}
            },
            Err(_) => continue,
        }
    }
    *INDEXER_RELAYS.write() = if indexer.is_empty() {
        default_indexer_relays()
    } else {
        indexer
    };
    *PROXY_RELAYS.write() = proxy;
    *TRUSTED_RELAYS.write() = trusted;
    Ok(())
}
/// Add indexer relays to the pool as DISCOVERY-only.
///
/// DISCOVERY-only relays are available for `fetch_events_from` but are
/// invisible to broadcast subscriptions (no READ flag). They are evictable
/// by `cleanup_gossip_relays` after inactivity.
///
/// Previously this triple-added each URL, upgrading indexers to
/// READ+WRITE+DISCOVERY, causing every broadcast subscription to fan out
/// to all 6 indexers unnecessarily.
pub async fn add_indexer_relays_to_client(client: Arc<Client>) {
    // Use get_indexer_relay_urls() (falls back to DEFAULT_INDEXER_RELAYS when the
    // user hasn't published a custom kind 10086). Reading INDEXER_RELAYS directly
    // returns empty at startup — before init_private_relay_lists populates the
    // signal — so the pool would contain zero indexers and metadata fetches fail.
    let indexer_urls = get_indexer_relay_urls();
    if indexer_urls.is_empty() {
        return;
    }
    log::info!("Adding {} indexer relays as DISCOVERY-only", indexer_urls.len());
    for url_str in &indexer_urls {
        if let Ok(url) = RelayUrl::parse(url_str) {
            let _ = client.add_discovery_relay(url).await;
        }
    }
}

/// Fetch events from the indexer relays.
///
/// Indexer relays are DISCOVERY-only pool members. `can_read()` includes the
/// DISCOVERY flag (`RelayServiceFlags::can_read` = READ | GOSSIP | DISCOVERY),
/// so `fetch_events_from` (which sends REQ messages) is permitted on them even
/// though they have no READ flag. This keeps them invisible to broadcast
/// `subscribe`/`fetch_events` calls (which only target READ-flagged relays),
/// avoiding fan-out on every subscription while still allowing targeted
/// metadata/relay-list queries.
///
/// **Resilience:** only indexers that are pool members **and currently
/// connected** are targeted. This is essential because the SDK's
/// `fetch_events_from` returns `Ok(empty)` — not an `Err` — when every target
/// relay is still `NotReady`/unconnected (it just logs each relay and skips
/// it). If we passed all indexer URLs blindly, a cold start (indexers still
/// handshaking) would yield `Ok(empty)`, which the profile-exhaustion logic
/// would misread as "genuinely no metadata" and mark every pubkey exhausted —
/// poisoning retries for 5 minutes. By returning a clear `Err` when no indexer
/// is connected yet, the exhaustion logic keeps those pubkeys retryable.
///
/// This is the **only sanctioned way** to read from indexer relays. Never use
/// `client.fetch_events()` for metadata/relay-list discovery — it targets only
/// READ-flagged relays and will silently miss every indexer.
pub async fn fetch_events_from_indexers(
    client: &Client,
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let indexer_urls: Vec<RelayUrl> = get_indexer_relay_urls()
        .iter()
        .filter_map(|s| RelayUrl::parse(s).ok())
        .collect();
    if indexer_urls.is_empty() {
        return Err("No indexer relays configured".to_string());
    }
    // `pool().all_relays()` includes DISCOVERY-only relays (unlike
    // `client.relays()` which filters to READ|WRITE). Filter to indexers that
    // are actually connected so the fetch can't silently return empty.
    let all_relays = client.pool().all_relays().await;
    let connected: Vec<RelayUrl> = indexer_urls
        .iter()
        .filter(|url| {
            all_relays
                .get(*url)
                .map(|r| r.is_connected())
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    if connected.is_empty() {
        return Err("No indexer relays connected yet".to_string());
    }
    client
        .fetch_events_from(connected, filter, timeout)
        .await
        .map(|events| events.into_iter().collect())
        .map_err(|e| {
            log::warn!("Indexer fetch failed: {e}");
            format!("Indexer fetch failed: {e}")
        })
}

/// Wait until at least one indexer relay is connected, or `timeout` elapses.
///
/// Indexers connect via `pool.connect()` (called at boot and in
/// `run_post_login_init`). On a cold WASM start the TLS handshakes can take
/// 3-5s each, so callers that need to fetch metadata right after login should
/// await this before issuing the fetch — otherwise the fetch races the
/// handshake and returns nothing. Returns `true` if an indexer connected within
/// the timeout, `false` otherwise.
pub async fn wait_for_indexer_connected(client: &Client, timeout: Duration) -> bool {
    let indexer_urls: Vec<RelayUrl> = get_indexer_relay_urls()
        .iter()
        .filter_map(|s| RelayUrl::parse(s).ok())
        .collect();
    if indexer_urls.is_empty() {
        return false;
    }
    let start = instant::Instant::now();
    loop {
        let all_relays = client.pool().all_relays().await;
        let any_connected = indexer_urls
            .iter()
            .any(|url| all_relays.get(url).map(|r| r.is_connected()).unwrap_or(false));
        if any_connected {
            return true;
        }
        if start.elapsed() >= timeout {
            log::warn!(
                "wait_for_indexer_connected: no indexer connected after {:?}",
                timeout
            );
            return false;
        }
        crate::stores::nostr_client::platform_sleep_ms(500).await;
    }
}

/// Publish one of the user's own "discovery" events to the indexer relays.
///
/// The user's own kind 0 (metadata), kind 10002 (relay list) and kind 10050
/// (DM inbox relays) events MUST be advertised to the well-known public
/// indexers so other clients can discover the user (NIP-65: "Clients SHOULD
/// spread an author's kind:10002 event to... well-known public indexers").
///
/// Indexer relays are DISCOVERY-only, and `can_write()` does NOT include
/// DISCOVERY (`RelayServiceFlags::can_write` = WRITE | GOSSIP), so
/// `send_event_to` on a DISCOVERY relay returns `WriteDisabled`. We therefore
/// open a short-lived ephemeral connection (default flags grant WRITE) for the
/// single publish, which idles out afterwards. This is acceptable because
/// self-data publishes are infrequent (profile/relay edits), unlike the
/// constant metadata-fetch path which must never use ephemeral connections
/// (those would grant READ+WRITE and cause broadcast fan-out).
///
/// This mirrors Wisp's self-data publish pattern
/// (`StartupCoordinator`: "ephemeral if not already connected").
pub async fn publish_event_to_indexers(
    client: &Client,
    event: &nostr::Event,
) -> std::result::Result<usize, String> {
    let indexer_urls = get_indexer_relay_urls();
    if indexer_urls.is_empty() {
        return Ok(0);
    }
    let ephemeral = crate::stores::relay::coverage::connect_ephemeral_relays(client, &indexer_urls).await;
    if ephemeral.connected.is_empty() {
        log::warn!("Could not connect to any indexer relay for self-data publish");
        return Ok(0);
    }
    let urls: Vec<RelayUrl> = ephemeral
        .connected
        .iter()
        .filter_map(|s| RelayUrl::parse(s).ok())
        .collect();
    match client.send_event_to(urls, event).await {
        Ok(output) => {
            let ok = output.success.len();
            if !output.failed.is_empty() {
                log::warn!(
                    "Self-data publish to indexers: {ok} ok, {} failed: {:?}",
                    output.failed.len(),
                    output.failed.keys().collect::<Vec<_>>()
                );
            } else {
                log::info!("Self-data event advertised to {ok} indexer relay(s)");
            }
            Ok(ok)
        }
        Err(e) => {
            log::warn!("Failed to advertise self-data to indexers: {e}");
            Err(format!("Indexer publish failed: {e}"))
        }
    }
}
pub async fn fetch_own_lists_from_indexers(client: Arc<Client>) {
    let my_pubkey = match crate::stores::nostr_client::get_cached_pubkey() {
        Ok(pk) => pk,
        Err(_) => return,
    };
    let indexer_urls = get_indexer_relay_urls();
    if indexer_urls.is_empty() {
        return;
    }
    log::info!("Fetching own relay lists from indexer relays as backup");
    let filter = Filter::new()
        .author(my_pubkey)
        .kinds(vec![
            Kind::RelayList,
            Kind::InboxRelays,
            Kind::SearchRelays,
            Kind::BlockedRelays,
            Kind::Custom(10013),
            Kind::Custom(10012),
        ])
        .limit(20);
    match client
        .fetch_events_from(
            indexer_urls
                .iter()
                .filter_map(|s| RelayUrl::parse(s).ok())
                .collect::<Vec<_>>(),
            filter,
            Duration::from_secs(8),
        )
        .await
    {
        Ok(events) => {
            let mut found_relay_list = false;
            let mut found_inbox = false;
            let mut found_search = false;
            let mut found_blocked = false;
            let mut found_outbox = false;
            let mut found_favorites = false;
            for event in events.iter() {
                match event.kind.as_u16() {
                    10002 if !found_relay_list => {
                        found_relay_list = true;
                        let parsed = parse_relay_list_event(event);
                        if !parsed.is_empty() {
                            let current = USER_RELAY_METADATA.read().clone();
                            match current {
                                Some(ref m)
                                    if m.updated_at
                                        >= event.created_at.as_secs() =>
                                {}
                                _ => {
                                    let mut metadata = current.unwrap_or_default();
                                    metadata.relays = parsed;
                                    metadata.updated_at = event.created_at.as_secs();
                                    *USER_RELAY_METADATA.write() = Some(metadata);
                                    log::info!(
                                        "Updated relay list from indexer backup"
                                    );
                                }
                            }
                        }
                    }
                    10050 if !found_inbox => {
                        found_inbox = true;
                        let dm_relays: Vec<String> = event
                            .tags
                            .iter()
                            .filter_map(|tag| {
                                if tag.kind() == TagKind::Relay {
                                    tag.content().map(crate::utils::relay::upgrade_to_secure_relay_url)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if !dm_relays.is_empty() {
                            let current_dm = get_dm_relays_10050_only();
                            if current_dm.is_empty() {
                                let mut metadata =
                                    USER_RELAY_METADATA.read().clone().unwrap_or_default();
                                metadata.dm_relays = dm_relays;
                                *USER_RELAY_METADATA.write() = Some(metadata);
                                log::info!(
                                    "Updated DM relays from indexer backup"
                                );
                            }
                        }
                    }
                    10007 if !found_search => {
                        found_search = true;
                        let current = SEARCH_RELAYS.peek().clone();
                        if current.is_empty() {
                            let urls: Vec<String> = event
                                .tags
                                .iter()
                                .filter_map(|tag| {
                                    if tag.kind()
                                        == TagKind::Custom("relay".into())
                                    {
                                        tag.content().map(|s| s.to_string())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if !urls.is_empty() {
                                *SEARCH_RELAYS.write() = urls;
                            }
                        }
                    }
                    10006 if !found_blocked => {
                        found_blocked = true;
                        let current = BLOCKED_RELAYS.peek().clone();
                        if current.is_empty() {
                            let urls: Vec<String> = event
                                .tags
                                .iter()
                                .filter_map(|tag| {
                                    if tag.kind()
                                        == TagKind::Custom("relay".into())
                                    {
                                        tag.content().map(|s| s.to_string())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if !urls.is_empty() {
                                *BLOCKED_RELAYS.write() = urls;
                            }
                        }
                    }
                    10013 if !found_outbox => {
                        found_outbox = true;
                        let current = OUTBOX_RELAYS.peek().clone();
                        if current.is_empty() {
                            let urls = parse_relay_tags(&event.tags);
                            if !urls.is_empty() {
                                *OUTBOX_RELAYS.write() = urls;
                            }
                        }
                    }
                    10012 if !found_favorites => {
                        found_favorites = true;
                        let current = FAVORITE_RELAYS.peek().clone();
                        if current.is_empty() {
                            let urls = parse_relay_tags(&event.tags);
                            if !urls.is_empty() {
                                *FAVORITE_RELAYS.write() = urls;
                            }
                        }
                    }
                    _ => {}
                }
            }
            log::info!(
                "Indexer backup fetch complete: relay_list={} inbox={} search={} blocked={} outbox={} favorites={}",
                found_relay_list, found_inbox, found_search, found_blocked, found_outbox, found_favorites
            );
        }
        Err(e) => {
            log::warn!("Failed to fetch own lists from indexers: {}", e);
        }
    }
}
const LOCAL_RELAYS_KEY: &str = "nostr_blue_local_relays";
const BROADCAST_RELAYS_KEY: &str = "nostr_blue_broadcast_relays";
/// Load local relays from storage (web uses LocalStorage, native uses filesystem)
#[cfg(feature = "web")]
pub fn load_local_relays() -> Vec<String> {
    match storage::get::<Vec<String>>(LOCAL_RELAYS_KEY) {
        Ok(relays) => relays,
        Err(e) => {
            let lower = e.to_lowercase();
            if lower.contains("not found") || lower.contains("missing") || lower.contains("no key")
            {
                return Vec::new();
            }
            log::error!(
                "Failed to load local relays from storage: {}, key: {}",
                e,
                LOCAL_RELAYS_KEY
            );
            Vec::new()
        }
    }
}
/// Load broadcast relays from storage (web uses LocalStorage, native uses filesystem)
#[cfg(feature = "web")]
pub fn load_broadcast_relays() -> Vec<String> {
    match storage::get::<Vec<String>>(BROADCAST_RELAYS_KEY) {
        Ok(relays) => relays,
        Err(e) => {
            let lower = e.to_lowercase();
            if lower.contains("not found") || lower.contains("missing") || lower.contains("no key")
            {
                return Vec::new();
            }
            log::error!(
                "Failed to load broadcast relays from storage: {}, key: {}",
                e,
                BROADCAST_RELAYS_KEY
            );
            Vec::new()
        }
    }
}
#[cfg(feature = "native")]
pub fn load_local_relays() -> Vec<String> {
    let path = dirs::config_dir().map(|p| {
        p.join("nostr_blue")
            .join(format!("{}.json", LOCAL_RELAYS_KEY))
    });
    match path {
        Some(p) if p.exists() => match fs::read_to_string(&p) {
            Ok(json) => match serde_json::from_str(&json) {
                Ok(relays) => relays,
                Err(e) => {
                    log::error!("Failed to parse local relays JSON from {:?}: {}", p, e);
                    Vec::new()
                }
            },
            Err(e) => {
                log::error!("Failed to read local relays file {:?}: {}", p, e);
                Vec::new()
            }
        },
        _ => Vec::new(),
    }
}
#[cfg(feature = "native")]
pub fn load_broadcast_relays() -> Vec<String> {
    let path = dirs::config_dir().map(|p| {
        p.join("nostr_blue")
            .join(format!("{}.json", BROADCAST_RELAYS_KEY))
    });
    match path {
        Some(p) if p.exists() => match fs::read_to_string(&p) {
            Ok(json) => match serde_json::from_str(&json) {
                Ok(relays) => relays,
                Err(e) => {
                    log::error!("Failed to parse broadcast relays JSON from {:?}: {}", p, e);
                    Vec::new()
                }
            },
            Err(e) => {
                log::error!("Failed to read broadcast relays file {:?}: {}", p, e);
                Vec::new()
            }
        },
        _ => Vec::new(),
    }
}
/// Save local relays to browser LocalStorage (web-only)
#[cfg(feature = "web")]
pub fn save_local_relays(relays: &[String]) {
    if let Err(e) = storage::set(LOCAL_RELAYS_KEY, &relays) {
        log::error!("Failed to save local relays: {}", e);
    }
}
/// Save broadcast relays to browser LocalStorage (web-only)
#[cfg(feature = "web")]
pub fn save_broadcast_relays(relays: &[String]) -> Result<(), String> {
    storage::set(BROADCAST_RELAYS_KEY, &relays)
        .map_err(|e| format!("Failed to save broadcast relays: {}", e))
}
#[cfg(feature = "native")]
pub fn save_local_relays(relays: &[String]) {
    let Some(config_dir) = dirs::config_dir().map(|p| p.join("nostr_blue")) else {
        log::error!("Could not determine config directory for local relays");
        return;
    };
    if let Err(e) = fs::create_dir_all(&config_dir) {
        log::error!("Failed to create config directory {:?}: {}", config_dir, e);
        return;
    }
    let path = config_dir.join(format!("{}.json", LOCAL_RELAYS_KEY));
    let temp_path = config_dir.join(format!("{}.json.tmp", LOCAL_RELAYS_KEY));
    match serde_json::to_string(relays) {
        Ok(json) => {
            if let Err(e) = fs::write(&temp_path, &json) {
                log::error!(
                    "Failed to write local relays temp file {:?}: {}",
                    temp_path,
                    e
                );
                let _ = fs::remove_file(&temp_path);
                return;
            }
            if let Err(e) = fs::rename(&temp_path, &path) {
                log::error!(
                    "Failed to atomically replace local relays {:?} with {:?}: {}",
                    path,
                    temp_path,
                    e
                );
                let _ = fs::remove_file(&temp_path);
            }
        }
        Err(e) => {
            log::error!("Failed to serialize local relays: {}", e);
        }
    }
}
#[cfg(feature = "native")]
pub fn save_broadcast_relays(relays: &[String]) -> Result<(), String> {
    let Some(config_dir) = dirs::config_dir().map(|p| p.join("nostr_blue")) else {
        return Err("Could not determine config directory for broadcast relays".to_string());
    };
    if let Err(e) = fs::create_dir_all(&config_dir) {
        return Err(format!(
            "Failed to create config directory {:?} for broadcast relays: {}",
            config_dir, e
        ));
    }
    let path = config_dir.join(format!("{}.json", BROADCAST_RELAYS_KEY));
    let temp_path = config_dir.join(format!("{}.json.tmp", BROADCAST_RELAYS_KEY));
    match serde_json::to_string(relays) {
        Ok(json) => {
            if let Err(e) = fs::write(&temp_path, &json) {
                let _ = fs::remove_file(&temp_path);
                return Err(format!(
                    "Failed to write broadcast relays temp file {:?}: {}",
                    temp_path, e
                ));
            }
            if let Err(e) = fs::rename(&temp_path, &path) {
                let _ = fs::remove_file(&temp_path);
                return Err(format!(
                    "Failed to atomically replace broadcast relays {:?} with {:?}: {}",
                    path, temp_path, e
                ));
            }
            Ok(())
        }
        Err(e) => Err(format!("Failed to serialize broadcast relays: {}", e)),
    }
}
/// Initialize local relays from cache
/// Call during app init BEFORE async client init
pub fn init_local_relays_from_cache() {
    let relays = load_local_relays();
    if !relays.is_empty() {
        log::info!("Loaded {} local relays from cache", relays.len());
        *LOCAL_RELAYS.write() = relays;
    }
    let broadcast_relays = load_broadcast_relays();
    if !broadcast_relays.is_empty() {
        log::info!(
            "Loaded {} broadcast relays from cache",
            broadcast_relays.len()
        );
        *BROADCAST_RELAYS.write() = broadcast_relays;
    }
}
/// Add local relays to the client connection pool
/// Called after client init to merge local relays with other sources
pub async fn apply_local_relays_to_client(client: Arc<Client>) {
    let local_relays = LOCAL_RELAYS.peek().clone();
    if local_relays.is_empty() {
        return;
    }
    let blocked_relays = BLOCKED_RELAYS.peek().clone();
    log::info!("Adding {} local relays to client pool", local_relays.len());
    for relay_url in local_relays {
        let normalized = relay_url.trim_end_matches('/');
        if blocked_relays
            .iter()
            .any(|b| b.trim_end_matches('/') == normalized)
        {
            log::info!("Skipping blocked local relay: {}", relay_url);
            continue;
        }
        if let Ok(url) = RelayUrl::parse(&relay_url) {
            match client.add_relay(url.clone()).await {
                Ok(_) => log::info!("Added local relay: {}", relay_url),
                Err(e) => log::warn!("Failed to add local relay {}: {}", relay_url, e),
            }
        }
    }
}
/// Initialize all NIP-51 relay lists for current user
/// Call after signer is attached
pub async fn init_nip51_relay_lists(client: Arc<Client>) -> Result<(), String> {
    let pubkey = nostr_client::get_cached_pubkey().map_err(|_| "No signer attached")?;
    log::info!(
        "Fetching NIP-51 relay lists (search/blocked/outbox/favorites) for {}",
        pubkey.to_hex()
    );
    let (search_result, blocked_result, outbox_result, favorites_result) = tokio::join!(
        fetch_search_relays(pubkey, client.clone()),
        fetch_blocked_relays(pubkey, client.clone()),
        fetch_outbox_relays(pubkey, client.clone()),
        fetch_favorite_relays(pubkey, client.clone()),
    );
    apply_defaults_if_unseeded(
        search_result,
        &SEARCH_RELAYS,
        default_search_relays,
        "search",
    );
    match blocked_result {
        Ok(relays) => {
            log::info!("Loaded {} blocked relays from Nostr", relays.len());
            *BLOCKED_RELAYS.write() = relays;
        }
        Err(e) => {
            log::warn!("Failed to fetch blocked relays: {}", e);
        }
    }
    match outbox_result {
        Ok(relays) if !relays.is_empty() => {
            log::info!("Loaded {} outbox relays from Nostr", relays.len());
            *OUTBOX_RELAYS.write() = relays;
        }
        Ok(_) => {
            log::info!("No outbox relays found");
        }
        Err(e) => {
            log::warn!("Failed to fetch outbox relays: {}", e);
        }
    }
    apply_defaults_if_unseeded(
        favorites_result,
        &FAVORITE_RELAYS,
        default_favorite_relays,
        "favorite",
    );
    Ok(())
}

/// Apply an optional NIP-51 list fetch result with defaults-if-unseeded
/// semantics: a non-empty result always wins; an empty result or an error
/// writes the defaults ONLY when the signal holds nothing (no custom list
/// was seeded from disk). A disk-seeded custom list must survive an
/// empty/failed network refresh — `persist_public_relay_lists` mirrors
/// the signal right after `init_nip51_relay_lists`, so clobbering it here
/// would durably replace the user's list with defaults across sessions
/// (one offline boot = defaults forever).
pub(crate) fn apply_defaults_if_unseeded(
    result: Result<Vec<String>, String>,
    signal: &GlobalSignal<Vec<String>>,
    defaults: fn() -> Vec<String>,
    label: &str,
) {
    match result {
        Ok(relays) if !relays.is_empty() => {
            log::info!("Loaded {} {} relays from Nostr", relays.len(), label);
            *signal.write() = relays;
        }
        outcome => {
            if signal.read().is_empty() {
                match outcome {
                    Err(ref e) => log::warn!("Failed to fetch {label} relays: {e}, using defaults"),
                    Ok(_) => log::info!("No {label} relays found, using defaults"),
                }
                *signal.write() = defaults();
            } else {
                match outcome {
                    Err(ref e) => {
                        log::warn!("Failed to fetch {label} relays: {e}, keeping seeded list")
                    }
                    Ok(_) => log::info!("No {label} relays found, keeping seeded list"),
                }
            }
        }
    }
}
/// Track the current real-time subscription ID for NIP-65 updates
pub static RELAY_LIST_SUBSCRIPTION_ID: GlobalSignal<Option<SubscriptionId>> =
    Signal::global(|| None);
static RELAY_LIST_LISTENER_TASK: GlobalSignal<Option<dioxus_core::Task>> =
    Signal::global(|| None);
/// Returns true if `url` is one of the app's persistent relay sets (defaults,
/// Mostro P2P, or specialty relays for video/GIF/radio).
///
/// The NIP-65 listener uses this to avoid force-removing relays that other
/// code paths depend on when they happen to also appear in the user's
/// kind 10002 list and are later removed from it.
fn is_persistent_relay(url: &RelayUrl) -> bool {
    use crate::stores::relay::pool::DEFAULT_RELAYS;
    use crate::stores::relay::specialty::p2p_urls::MOSTRO_DEFAULT_RELAYS;
    use crate::stores::relay::specialty::urls::{LIVELIER, VIDEO, GIF, RADIO, RADIO_FALLBACK};

    let specialty = [VIDEO, GIF, RADIO, RADIO_FALLBACK, LIVELIER];
    DEFAULT_RELAYS
        .iter()
        .copied()
        .chain(MOSTRO_DEFAULT_RELAYS.iter().copied())
        .chain(specialty.iter().copied())
        .any(|p| RelayUrl::parse(p).map(|parsed| &parsed == url).unwrap_or(false))
}

pub async fn start_relay_list_subscription() {
    if RELAY_LIST_SUBSCRIPTION_ID.read().is_some() {
        log::debug!("Relay list subscription already active");
        return;
    }
    if let Some(old_task) = RELAY_LIST_LISTENER_TASK.write().take() {
        log::info!("Cancelling old relay list listener task");
        old_task.cancel();
    }
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => {
            log::warn!("Cannot start relay list subscription: no client");
            return;
        }
    };
    let my_pubkey_str = match crate::stores::auth_store::get_pubkey() {
        Some(pk) => pk,
        None => {
            log::warn!("Cannot start relay list subscription: not authenticated");
            return;
        }
    };
    let my_pubkey = match PublicKey::from_bech32(&my_pubkey_str)
        .or_else(|_| PublicKey::from_hex(&my_pubkey_str))
    {
        Ok(pk) => pk,
        Err(e) => {
            log::error!("Invalid pubkey for relay list subscription: {}", e);
            return;
        }
    };
    let filter = Filter::new()
        .author(my_pubkey)
        .kind(Kind::RelayList)
        .limit(1);
    let subscription_result =
        crate::stores::subscription_manager::subscribe_realtime(&client, filter, Some(600)).await;
    match subscription_result {
        Ok(sub_id) => {
            RELAY_LIST_SUBSCRIPTION_ID.write().replace(sub_id.clone());
            log::info!("Started relay list subscription: {:?}", sub_id);
            let task = crate::platform::spawn::spawn_catch_unwind("nip65", async move {
                let mut notifications = client.notifications();
                loop {
                    match notifications.recv().await {
                        Ok(nostr_sdk::RelayPoolNotification::Event {
                            subscription_id: event_sub_id,
                            event,
                            ..
                        }) => {
                            if event_sub_id != sub_id {
                                continue;
                            }
                            log::info!("Received NIP-65 relay list update (kind 10002)");
                            let new_relays = parse_relay_list_event(&event);
                            if !new_relays.is_empty() {
                                let old_urls: HashSet<RelayUrl> = USER_RELAY_METADATA
                                    .read()
                                    .clone()
                                    .unwrap_or_default()
                                    .relays
                                    .iter()
                                    .filter_map(|r| RelayUrl::parse(&r.url).ok())
                                    .collect();
                                let new_urls: HashSet<RelayUrl> = new_relays
                                    .iter()
                                    .filter_map(|r| RelayUrl::parse(&r.url).ok())
                                    .collect();

                                let blocked = BLOCKED_RELAYS.peek().clone();
                                for relay_config in &new_relays {
                                    let normalized = relay_config.url.trim_end_matches('/');
                                    if blocked
                                        .iter()
                                        .any(|b| b.trim_end_matches('/') == normalized)
                                    {
                                        continue;
                                    }
                                    if let Ok(url) = RelayUrl::parse(&relay_config.url) {
                                        let _ = client.add_relay(url).await;
                                    }
                                }

                                let to_remove: Vec<RelayUrl> =
                                    old_urls.difference(&new_urls).cloned().collect();
                                for url in to_remove {
                                    if is_persistent_relay(&url) {
                                        continue;
                                    }
                                    log::info!(
                                        "Removing relay no longer in NIP-65 list: {}",
                                        url.as_str()
                                    );
                                    let _ = client.force_remove_relay(url).await;
                                }

                                let mut metadata =
                                    USER_RELAY_METADATA.read().clone().unwrap_or_default();
                                metadata.relays = new_relays;
                                metadata.updated_at = event.created_at.as_secs();
                                *USER_RELAY_METADATA.write() = Some(metadata);
                                super::persistence::persist_public_relay_lists();
                                crate::services::search_relays::invalidate_search_relay_cache().await;
                                log::info!("Invalidated search relay cache after NIP-65 update");
                            }
                        }
                        Ok(nostr_sdk::RelayPoolNotification::Shutdown) => break,
                        // Transient: keep going so NIP-65 updates don't silently stop.
                        Err(RecvError::Lagged(skipped)) => {
                            log::warn!(
                                "nip65 listener: lagged, skipped {} events, continuing",
                                skipped
                            );
                            continue;
                        }
                        Err(RecvError::Closed) => {
                            log::info!("nip65 listener: channel closed, exiting");
                            break;
                        }
                        Ok(_) => {}
                    }
                }
                let current_sub_id = RELAY_LIST_SUBSCRIPTION_ID.read().clone();
                if current_sub_id.as_ref() == Some(&sub_id) {
                    log::warn!("Relay list subscription ended - clearing for reconnect");
                    *RELAY_LIST_SUBSCRIPTION_ID.write() = None;
                }
            });
            *RELAY_LIST_LISTENER_TASK.write() = Some(task);
        }
        Err(e) => {
            log::error!("Failed to start relay list subscription: {}", e);
        }
    }
}
pub async fn stop_relay_list_subscription() {
    if let Some(task) = RELAY_LIST_LISTENER_TASK.write().take() {
        log::info!("Cancelling relay list listener task");
        task.cancel();
    }
    let sub_id = RELAY_LIST_SUBSCRIPTION_ID.read().clone();
    if let Some(id) = sub_id {
        if let Some(client) = crate::stores::nostr_client::get_client() {
            log::info!("Stopping relay list subscription: {:?}", id);
            crate::stores::subscription_manager::unsubscribe(&client, &id).await;
        }
        *RELAY_LIST_SUBSCRIPTION_ID.write() = None;
    }
}

#[cfg(test)]
mod defaults_if_unseeded_tests {
    use super::*;
    use dioxus::prelude::*;

    /// The defaults-if-unseeded guard: an empty or failed NIP-51 fetch must
    /// keep a custom list that was seeded from disk (otherwise the mirror
    /// persist right after would durably replace the user's list with
    /// defaults), while an unconfigured (empty) signal still receives the
    /// defaults.
    #[test]
    fn empty_or_failed_fetch_keeps_seeded_list() {
        // GlobalSignal access needs a Dioxus runtime on this thread.
        let vdom = VirtualDom::new(|| rsx! { div {} });
        let _rt_guard = dioxus_core::RuntimeGuard::new(vdom.runtime());

        let custom = vec!["wss://custom.example".to_string()];
        let defaults = || vec!["wss://default.example".to_string()];

        // Seeded custom list + Err -> kept.
        *SEARCH_RELAYS.write() = custom.clone();
        apply_defaults_if_unseeded(Err("offline".into()), &SEARCH_RELAYS, defaults, "search");
        assert_eq!(*SEARCH_RELAYS.read(), custom);

        // Seeded custom list + Ok(empty) -> kept.
        apply_defaults_if_unseeded(Ok(Vec::new()), &SEARCH_RELAYS, defaults, "search");
        assert_eq!(*SEARCH_RELAYS.read(), custom);

        // Unconfigured + Err -> defaults.
        *FAVORITE_RELAYS.write() = Vec::new();
        apply_defaults_if_unseeded(Err("offline".into()), &FAVORITE_RELAYS, defaults, "favorite");
        assert_eq!(*FAVORITE_RELAYS.read(), defaults());

        // Network result always wins over both.
        *SEARCH_RELAYS.write() = Vec::new();
        let fetched = vec!["wss://fetched.example".to_string()];
        apply_defaults_if_unseeded(Ok(fetched.clone()), &SEARCH_RELAYS, defaults, "search");
        assert_eq!(*SEARCH_RELAYS.read(), fetched);

        // Reset globals for other tests in the process.
        *SEARCH_RELAYS.write() = Vec::new();
        *FAVORITE_RELAYS.write() = Vec::new();
    }
}
