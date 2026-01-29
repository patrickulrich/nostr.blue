//! NIP-65: Relay List Metadata (kind 10002)
//! NIP-17: Private Direct Message Relay Lists (kind 10050)
//!
//! This module provides centralized relay management using Nostr-native relay lists.
//! It implements the Outbox model for intelligent relay routing.

use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr_sdk::{Client, EventBuilder, Filter, Kind, PublicKey, RelayUrl, Tag, TagKind};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

use crate::stores::nostr_client;

#[cfg(target_arch = "wasm32")]
use gloo_storage::{LocalStorage, Storage};

#[cfg(target_arch = "wasm32")]
use js_sys;

#[cfg(not(target_arch = "wasm32"))]
use std::fs;

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

/// Current user's relay metadata
pub static USER_RELAY_METADATA: GlobalSignal<Option<RelayListMetadata>> =
    Signal::global(|| None);

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
];

/// Default relays to use when no kind 10002 is found
pub fn default_relays() -> Vec<RelayConfig> {
    DEFAULT_NIP65_RELAYS.iter().map(|url| RelayConfig {
        url: url.to_string(),
        read: true,
        write: true,
    }).collect()
}

/// Default DM relays to use when no kind 10050 is found
pub fn default_dm_relays() -> Vec<String> {
    DEFAULT_DM_RELAYS.iter().map(|s| s.to_string()).collect()
}

// ============================================================================
// NIP-51 Relay Lists (Search & Blocked)
// ============================================================================

/// Search relays (NIP-51 kind 10007)
pub static SEARCH_RELAYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);

/// Blocked relays (NIP-51 kind 10006)
pub static BLOCKED_RELAYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);

/// Local relays (stored in browser, not published to Nostr)
pub static LOCAL_RELAYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);

/// Default search relay URLs (NIP-50 compatible)
pub const DEFAULT_SEARCH_RELAYS: &[&str] = &[
    "wss://relay.nostr.band",
    "wss://search.nos.today",
];

/// Default search relays to use when no kind 10007 is found
pub fn default_search_relays() -> Vec<String> {
    DEFAULT_SEARCH_RELAYS.iter().map(|s| s.to_string()).collect()
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

// ============================================================================
// Relay List Helpers (Phase 2)
// ============================================================================

/// Get the user's read-enabled relays (for discovering content)
/// Returns relays from kind 10002 with read flag set, or defaults if none configured
/// Reserved for future outbox model optimization
#[allow(dead_code)]
pub fn get_read_relays() -> Vec<String> {
    let metadata = USER_RELAY_METADATA.read();
    match metadata.as_ref() {
        Some(m) => m.relays
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
        Some(m) => m.relays
            .iter()
            .filter(|r| r.write)
            .map(|r| r.url.clone())
            .collect(),
        None => DEFAULT_NIP65_RELAYS.iter().map(|s| s.to_string()).collect(),
    }
}

/// Get the user's DM inbox relays (for private messages)
/// Returns relays from kind 10050, or defaults if none configured
pub fn get_dm_relays() -> Vec<String> {
    let metadata = USER_RELAY_METADATA.read();
    match metadata.as_ref() {
        Some(m) if !m.dm_relays.is_empty() => m.dm_relays.clone(),
        _ => DEFAULT_DM_RELAYS.iter().map(|s| s.to_string()).collect(),
    }
}

// ============================================================================
// Event Parsing
// ============================================================================

/// Parse relay list from kind 10002 event
/// NIP-65 tag format:
/// - ["r", "wss://relay.url"] = both read and write
/// - ["r", "wss://relay.url", "read"] = read only
/// - ["r", "wss://relay.url", "write"] = write only
pub fn parse_relay_list_event(event: &nostr_sdk::Event) -> Vec<RelayConfig> {
    let mut relays = Vec::new();

    for tag in event.tags.iter() {
        let slice = tag.as_slice();

        // Check if this is an 'r' tag (NIP-65 relay list)
        if slice.first().map(|s| s.as_str()) != Some("r") {
            continue;
        }

        // Get URL at index 1
        let Some(url) = slice.get(1) else {
            continue;
        };

        // Get optional marker at index 2 to determine read/write permissions
        // No marker = both read and write
        // "read" = read only
        // "write" = write only
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

        log::debug!(
            "Found relay tag: {} (read={}, write={})",
            url,
            read,
            write
        );
        relays.push(RelayConfig {
            url: url.to_string(),
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
        // Check if this is a custom "relay" tag
        if tag.kind() == TagKind::Custom("relay".into()) {
            if let Some(content) = tag.content() {
                dm_relays.push(content.to_string());
            }
        }
    }

    dm_relays
}

// ============================================================================
// Relay List Fetching
// ============================================================================

/// Fetch relay list (kind 10002) and DM relay list (kind 10050) for a user
///
/// # Arguments
/// * `pubkey` - The public key of the user to fetch relay lists for
/// * `client` - The Nostr client instance
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
                updated_at = updated_at.max(event.created_at.as_secs());  // Use max of both events
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

// ============================================================================
// Relay List Publishing
// ============================================================================

/// Publish relay list (kind 10002) using rust-nostr's EventBuilder
///
/// # Arguments
/// * `relays` - List of relay configurations to publish
/// * `client` - The Nostr client instance
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
///
/// # Arguments
/// * `dm_relays` - List of DM inbox relay URLs to publish
/// * `client` - The Nostr client instance
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

// ============================================================================
// Relay List Initialization
// ============================================================================

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
    // Use cached pubkey - no signer call needed
    let user_pubkey = nostr_client::get_cached_pubkey().map_err(|_| "No signer attached")?;

    log::info!("Loading relay lists for Settings UI for {}", user_pubkey.to_hex());

    // Fetch user's relay lists from Nostr
    match fetch_relay_list(user_pubkey, client).await {
        Ok(metadata) => {
            log::info!("Loaded {} general relays and {} DM relays for Settings display",
                metadata.relays.len(), metadata.dm_relays.len());
            *USER_RELAY_METADATA.write() = Some(metadata);

            // Invalidate search relay cache when NIP-65 relay lists are updated
            crate::services::search_relays::invalidate_search_relay_cache().await;
            log::debug!("Invalidated search relay cache after NIP-65 update");

            Ok(())
        }
        Err(e) => {
            // If fetch fails (no published lists), use defaults for display
            log::warn!("No relay lists found: {}, using defaults for Settings", e);

            #[cfg(target_arch = "wasm32")]
            let now_secs = (js_sys::Date::now() / 1000.0) as u64;
            #[cfg(not(target_arch = "wasm32"))]
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(std::time::Duration::ZERO)
                .as_secs();

            let default = RelayListMetadata {
                relays: default_relays(),
                dm_relays: default_dm_relays(),
                updated_at: now_secs,
            };

            *USER_RELAY_METADATA.write() = Some(default);

            log::info!("Using default relays for Settings display. User can configure and publish their relay lists.");

            Ok(())
        }
    }
}

// ============================================================================
// NIP-51 Search & Blocked Relay Publishing
// ============================================================================

/// Publish search relays (kind 10007)
/// SDK: EventBuilder::search_relays() creates ["relay", "url"] tags
pub async fn publish_search_relays(relays: Vec<String>, client: Arc<Client>) -> Result<String, String> {
    log::info!("Publishing search relay list with {} relays", relays.len());

    let urls: Vec<RelayUrl> = relays.iter()
        .filter_map(|s| RelayUrl::parse(s).ok())
        .collect();

    let builder = EventBuilder::search_relays(urls);
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish search relays: {}", e))?;

    log::info!("Search relays published (kind 10007): {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

/// Publish blocked relays (kind 10006)
/// SDK: EventBuilder::blocked_relays() creates ["relay", "url"] tags
pub async fn publish_blocked_relays(relays: Vec<String>, client: Arc<Client>) -> Result<String, String> {
    log::info!("Publishing blocked relay list with {} relays", relays.len());

    let urls: Vec<RelayUrl> = relays.iter()
        .filter_map(|s| RelayUrl::parse(s).ok())
        .collect();

    let builder = EventBuilder::blocked_relays(urls);
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish blocked relays: {}", e))?;

    log::info!("Blocked relays published (kind 10006): {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

// ============================================================================
// NIP-51 Search & Blocked Relay Fetching
// ============================================================================

/// Fetch search relays (kind 10007) for a user
pub async fn fetch_search_relays(pubkey: PublicKey, client: Arc<Client>) -> Result<Vec<String>, String> {
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::SearchRelays)  // 10007
        .limit(1);

    let events = client.fetch_events(filter, Duration::from_secs(5)).await
        .map_err(|e| e.to_string())?;

    // Parse ["relay", "url"] tags
    let relays = events.into_iter()
        .next()
        .map(|event| {
            event.tags.iter()
                .filter_map(|tag| {
                    // Check for "relay" tag kind and extract URL
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
pub async fn fetch_blocked_relays(pubkey: PublicKey, client: Arc<Client>) -> Result<Vec<String>, String> {
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::BlockedRelays)  // 10006
        .limit(1);

    let events = client.fetch_events(filter, Duration::from_secs(5)).await
        .map_err(|e| e.to_string())?;

    // Parse ["relay", "url"] tags
    let relays = events.into_iter()
        .next()
        .map(|event| {
            event.tags.iter()
                .filter_map(|tag| {
                    // Check for "relay" tag kind and extract URL
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

// ============================================================================
// Local Relays (browser storage only, not published to Nostr)
// ============================================================================

const LOCAL_RELAYS_KEY: &str = "nostr_blue_local_relays";

/// Load local relays from browser LocalStorage
#[cfg(target_arch = "wasm32")]
pub fn load_local_relays() -> Vec<String> {
    LocalStorage::get::<String>(LOCAL_RELAYS_KEY)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_local_relays() -> Vec<String> {
    let path = dirs::config_dir()
        .map(|p| p.join("nostr_blue").join(format!("{}.json", LOCAL_RELAYS_KEY)));

    match path {
        Some(p) if p.exists() => {
            fs::read_to_string(&p)
                .ok()
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default()
        }
        _ => Vec::new()
    }
}

/// Save local relays to browser LocalStorage
#[cfg(target_arch = "wasm32")]
pub fn save_local_relays(relays: &[String]) {
    if let Ok(json) = serde_json::to_string(relays) {
        let _ = LocalStorage::set(LOCAL_RELAYS_KEY, json);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_local_relays(relays: &[String]) {
    let Some(config_dir) = dirs::config_dir().map(|p| p.join("nostr_blue")) else {
        return;
    };
    let _ = fs::create_dir_all(&config_dir);
    let path = config_dir.join(format!("{}.json", LOCAL_RELAYS_KEY));
    if let Ok(json) = serde_json::to_string(relays) {
        let _ = fs::write(path, json);
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
}

/// Add local relays to the client connection pool
/// Called after client init to merge local relays with other sources
pub async fn apply_local_relays_to_client(client: Arc<Client>) {
    let local_relays = LOCAL_RELAYS.peek().clone();
    if local_relays.is_empty() {
        return;
    }

    let blocked_relays = BLOCKED_RELAYS.peek();

    log::info!("Adding {} local relays to client pool", local_relays.len());

    for relay_url in local_relays {
        // Check blocked list
        let normalized = relay_url.trim_end_matches('/');
        if blocked_relays.iter().any(|b| b.trim_end_matches('/') == normalized) {
            log::info!("Skipping blocked local relay: {}", relay_url);
            continue;
        }

        if let Ok(url) = RelayUrl::parse(&relay_url) {
            // Use permissive options for local relays (may have higher latency)
            match client.add_relay(url.clone()).await {
                Ok(_) => log::info!("Added local relay: {}", relay_url),
                Err(e) => log::warn!("Failed to add local relay {}: {}", relay_url, e),
            }
        }
    }
}

// ============================================================================
// NIP-51 Relay List Initialization
// ============================================================================

/// Initialize all NIP-51 relay lists for current user
/// Call after signer is attached
pub async fn init_nip51_relay_lists(client: Arc<Client>) -> Result<(), String> {
    let pubkey = nostr_client::get_cached_pubkey().map_err(|_| "No signer attached")?;

    log::info!("Fetching NIP-51 relay lists (search/blocked) for {}", pubkey.to_hex());

    // Fetch all relay types in parallel
    let (search_result, blocked_result) = tokio::join!(
        fetch_search_relays(pubkey, client.clone()),
        fetch_blocked_relays(pubkey, client.clone()),
    );

    // Update signals with results (use defaults if fetch fails)
    match search_result {
        Ok(relays) if !relays.is_empty() => {
            log::info!("Loaded {} search relays from Nostr", relays.len());
            *SEARCH_RELAYS.write() = relays;
        }
        Ok(_) => {
            log::info!("No search relays found, using defaults");
            *SEARCH_RELAYS.write() = default_search_relays();
        }
        Err(e) => {
            log::warn!("Failed to fetch search relays: {}, using defaults", e);
            *SEARCH_RELAYS.write() = default_search_relays();
        }
    }

    match blocked_result {
        Ok(relays) => {
            log::info!("Loaded {} blocked relays from Nostr", relays.len());
            *BLOCKED_RELAYS.write() = relays;
        }
        Err(e) => {
            log::warn!("Failed to fetch blocked relays: {}", e);
            // No defaults for blocked - keep empty
        }
    }

    Ok(())
}
