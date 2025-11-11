/// NIP-65: Relay List Metadata (kind 10002)
/// NIP-17: Private Direct Message Relay Lists (kind 10050)
///
/// This module provides centralized relay management using Nostr-native relay lists.
/// It implements the Outbox model for intelligent relay routing.

use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_stores::Store;
use nostr_sdk::{Client, EventBuilder, Filter, Kind, PublicKey, Tag, TagKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[cfg(target_arch = "wasm32")]
use js_sys;

/// Configuration for a single relay with read/write permissions
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RelayConfig {
    pub url: String,
    pub read: bool,
    pub write: bool,
}

/// Complete relay metadata for a user (both kind 10002 and 10050)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayListMetadata {
    pub relays: Vec<RelayConfig>,      // kind 10002 - general relays
    pub dm_relays: Vec<String>,        // kind 10050 - DM inbox relays
    pub updated_at: u64,               // timestamp of last update
}

/// Store for relay list cache with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct RelayListCacheStore {
    pub data: HashMap<String, RelayListMetadata>,
}

/// Cache for relay lists by pubkey (24 hour TTL)
pub static RELAY_LIST_CACHE: GlobalSignal<Store<RelayListCacheStore>> =
    Signal::global(|| Store::new(RelayListCacheStore::default()));

/// Current user's relay metadata
pub static USER_RELAY_METADATA: GlobalSignal<Option<RelayListMetadata>> =
    Signal::global(|| None);

/// Default relays to use when no kind 10002 is found
pub fn default_relays() -> Vec<RelayConfig> {
    vec![
        RelayConfig {
            url: "wss://relay.damus.io".to_string(),
            read: true,
            write: true,
        },
        RelayConfig {
            url: "wss://relay.nostr.band".to_string(),
            read: true,
            write: true,
        },
        RelayConfig {
            url: "wss://nos.lol".to_string(),
            read: true,
            write: true,
        },
    ]
}

/// Parse relay list from kind 10002 event
/// NIP-65 tag format:
/// - ["r", "wss://relay.url"] = both read and write
/// - ["r", "wss://relay.url", "read"] = read only
/// - ["r", "wss://relay.url", "write"] = write only
pub fn parse_relay_list_event(event: &nostr_sdk::Event) -> Vec<RelayConfig> {
    let mut relays = Vec::new();

    for tag in event.tags.iter() {
        // Try to extract relay URL from the tag
        // For NIP-65, we're looking for tags like ["r", "wss://relay.url", "read"|"write"]
        if let Some(standardized) = tag.as_standardized() {
            // Check if this is a Relay tag
            if let nostr_sdk::TagStandard::Relay(relay_url) = standardized {
                log::debug!("Found relay tag: {}", relay_url);
                relays.push(RelayConfig {
                    url: relay_url.to_string(),
                    read: true,
                    write: true,
                });
                continue;
            }
        }

        // Fallback: try parsing as custom 'r' tag
        if tag.kind() == TagKind::Custom("r".into()) {
            if let Some(url) = tag.content() {
                log::debug!("Found 'r' tag: {}", url);
                relays.push(RelayConfig {
                    url: url.to_string(),
                    read: true,
                    write: true,
                });
            }
        }
    }

    log::info!("Parsed {} relays from event", relays.len());
    relays
}

/// Parse DM relay list from kind 10050 event
/// NIP-17 tag format: ["relay", "wss://relay.url"]
pub fn parse_dm_relay_list(event: &nostr_sdk::Event) -> Vec<String> {
    let mut dm_relays = Vec::new();

    for tag in event.tags.iter() {
        // Check if this is a custom "relay" tag
        if tag.kind() == TagKind::Custom("relay".into()) {
            if let Some(content) = tag.content() {
                dm_relays.push(content.to_string());
            }
        }
    }

    dm_relays
}

/// Fetch relay list (kind 10002) and DM relay list (kind 10050) for a user
pub async fn fetch_relay_list(pubkey: PublicKey, client: Arc<Client>) -> Result<RelayListMetadata, String> {
    log::info!("Fetching relay lists for {}", pubkey.to_hex());

    // Fetch kind 10002 (general relays)
    let filter_10002 = Filter::new()
        .author(pubkey)
        .kind(Kind::RelayList)
        .limit(1);

    // Fetch kind 10050 (DM inbox relays)
    let filter_10050 = Filter::new()
        .author(pubkey)
        .kind(Kind::from(10050))
        .limit(1);

    // Fetch both in parallel - clone client for parallel ops
    let client_10002 = client.clone();
    let client_10050 = client.clone();
    let (result_10002, result_10050) = tokio::join!(
        client_10002.fetch_events(filter_10002, Duration::from_secs(5)),
        client_10050.fetch_events(filter_10050, Duration::from_secs(5))
    );

    let mut relays = Vec::new();
    let mut dm_relays = Vec::new();
    let mut updated_at = 0u64;

    // Parse kind 10002
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
                    log::debug!("  - {} (read: {}, write: {})", relay.url, relay.read, relay.write);
                }
            } else {
                log::warn!("No kind 10002 events found for user");
            }
        }
        Err(e) => {
            log::error!("Failed to fetch kind 10002: {}", e);
        }
    }

    // Parse kind 10050
    match result_10050 {
        Ok(events) => {
            let event_count = events.len();
            log::info!("Received {} kind 10050 events", event_count);
            if let Some(event) = events.into_iter().next() {
                log::info!("Parsing kind 10050 event with {} tags", event.tags.len());
                dm_relays = parse_dm_relay_list(&event);
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

/// Fetch relay list with caching (24 hour TTL)
/// Publish relay list (kind 10002) using rust-nostr's EventBuilder
pub async fn publish_relay_list(relays: Vec<RelayConfig>, client: Arc<Client>) -> Result<String, String> {
    log::info!("Publishing relay list with {} relays", relays.len());

    // Build tags manually for kind 10002
    let tags: Vec<Tag> = relays
        .into_iter()
        .filter_map(|r| {
            let marker = match (r.read, r.write) {
                (true, true) => vec![r.url],                    // Both = no marker
                (true, false) => vec![r.url, "read".to_string()],   // Read only
                (false, true) => vec![r.url, "write".to_string()],  // Write only
                (false, false) => return None,                      // Invalid - skip
            };
            Some(Tag::custom(TagKind::Custom("r".into()), marker))
        })
        .collect();

    let builder = EventBuilder::new(Kind::RelayList, "").tags(tags);
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish relay list: {}", e))?;

    log::info!("Relay list published: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

/// Publish DM relay list (kind 10050)
pub async fn publish_dm_relay_list(dm_relays: Vec<String>, client: Arc<Client>) -> Result<String, String> {
    log::info!("Publishing DM relay list with {} relays", dm_relays.len());

    let tags: Vec<Tag> = dm_relays
        .into_iter()
        .map(|url| Tag::custom(TagKind::Custom("relay".into()), vec![url]))
        .collect();

    let builder = EventBuilder::new(Kind::from(10050), "").tags(tags);
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish DM relay list: {}", e))?;

    log::info!("DM relay list published: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

/// Sync relay lists on login - compares timestamps and updates if remote is newer
pub async fn sync_relay_lists_on_login(client: Arc<Client>) -> Result<(), String> {
    let signer = client.signer().await.map_err(|_| "No signer attached")?;
    let user_pubkey = signer.get_public_key().await.map_err(|e| format!("Failed to get pubkey: {}", e))?;

    log::info!("Syncing relay lists on login for {}", user_pubkey.to_hex());

    // Fetch latest from Nostr (bypass cache to get fresh data)
    let remote = fetch_relay_list(user_pubkey, client.clone()).await?;

    // Compare timestamps with local cache
    let should_update = {
        let local = USER_RELAY_METADATA.read();
        match local.as_ref() {
            Some(local_metadata) => {
                if remote.updated_at > local_metadata.updated_at {
                    log::info!(
                        "Remote relay list is newer (remote: {}, local: {}), syncing...",
                        remote.updated_at,
                        local_metadata.updated_at
                    );
                    true
                } else {
                    log::info!("Local relay list is up to date");
                    false
                }
            }
            None => {
                log::info!("No local relay list, using remote");
                true
            }
        }
    };

    if should_update {
        *USER_RELAY_METADATA.write() = Some(remote.clone());

        // Update cache
        RELAY_LIST_CACHE.read().data().write().insert(user_pubkey.to_hex(), remote);

        log::info!("Relay lists synced successfully");
    }

    Ok(())
}

/// Initialize relay lists for current user on startup
/// This is called once when the client starts up with a signer
pub async fn init_user_relay_lists(client: Arc<Client>) -> Result<(), String> {
    // Just call sync function - it handles both initial load and updates
    sync_relay_lists_on_login(client).await.or_else(|e| {
        // If sync fails (no remote lists), use defaults
        log::warn!("Sync failed: {}, using defaults for this session", e);

        #[cfg(target_arch = "wasm32")]
        let now_secs = (js_sys::Date::now() / 1000.0) as u64;
        #[cfg(not(target_arch = "wasm32"))]
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let default = RelayListMetadata {
            relays: default_relays(),
            dm_relays: vec!["wss://relay.damus.io".to_string()],
            updated_at: now_secs,
        };

        *USER_RELAY_METADATA.write() = Some(default);

        log::info!("Using default relays for this session. Go to Settings to configure and publish your relay lists.");

        Ok(())
    })
}
