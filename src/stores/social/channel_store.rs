//! Channel Store - NIP-28 Public Chat Channels
//!
//! NIP-28 Event Kinds:
//! - 40: Channel creation
//! - 41: Channel metadata update
//! - 42: Channel message
//! - 43: Hide message (client-side moderation)
//! - 44: Mute user (client-side moderation)

use dioxus::prelude::*;
use lru::LruCache;
use nostr::Event;
use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::Duration;

use crate::stores::nostr_client::{fetch_events_aggregated, get_client};

const CHANNEL_CACHE_SIZE: usize = 100;

/// Parsed NIP-28 channel (kind 40)
#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub id: String,
    pub pubkey: String,
    pub name: Option<String>,
    pub about: Option<String>,
    pub picture: Option<String>,
    pub relays: Vec<String>,
    pub created_at: u64,
    pub event: Event,
}

/// Parsed channel metadata update (kind 41)
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelMetadataUpdate {
    pub channel_id: String,
    pub name: Option<String>,
    pub about: Option<String>,
    pub picture: Option<String>,
    pub relays: Vec<String>,
    pub updated_at: u64,
}

/// Channel cache keyed by event ID (hex)
pub static CHANNELS_CACHE: GlobalSignal<LruCache<String, Channel>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(CHANNEL_CACHE_SIZE).unwrap()));

/// Latest kind 41 metadata per channel ID
pub static CHANNEL_METADATA_CACHE: GlobalSignal<HashMap<String, ChannelMetadataUpdate>> =
    GlobalSignal::new(HashMap::new);

// -- Parsing --

/// Parse a kind 40 channel creation event
pub fn parse_channel_creation(event: &Event) -> Option<Channel> {
    if event.kind != Kind::ChannelCreation {
        return None;
    }

    let (name, about, picture, relays) = parse_channel_content(&event.content);

    Some(Channel {
        id: event.id.to_hex(),
        pubkey: event.pubkey.to_hex(),
        name,
        about,
        picture,
        relays,
        created_at: event.created_at.as_secs(),
        event: event.clone(),
    })
}

/// Parse a kind 41 channel metadata event, validating creator pubkey
pub fn parse_channel_metadata(event: &Event, creator_pubkey: &str) -> Option<ChannelMetadataUpdate> {
    if event.kind != Kind::ChannelMetadata {
        return None;
    }

    // Only the channel creator can update metadata
    if event.pubkey.to_hex() != creator_pubkey {
        return None;
    }

    // Extract channel ID from e tag
    let channel_id = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::e())
        .and_then(|t| t.content())
        .map(|s| s.to_string())?;

    let (name, about, picture, relays) = parse_channel_content(&event.content);

    Some(ChannelMetadataUpdate {
        channel_id,
        name,
        about,
        picture,
        relays,
        updated_at: event.created_at.as_secs(),
    })
}

/// Parse JSON content for name/about/picture/relays
fn parse_channel_content(content: &str) -> (Option<String>, Option<String>, Option<String>, Vec<String>) {
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap_or_default();
    let name = parsed.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
    let about = parsed.get("about").and_then(|v| v.as_str()).map(|s| s.to_string());
    let picture = parsed.get("picture").and_then(|v| v.as_str()).map(|s| s.to_string());
    let relays = parsed
        .get("relays")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    (name, about, picture, relays)
}

// -- Caching --

pub fn get_cached_channel(event_id: &str) -> Option<Channel> {
    CHANNELS_CACHE.read().peek(event_id).cloned()
}

pub fn cache_channel(channel: Channel) {
    CHANNELS_CACHE.write().put(channel.id.clone(), channel);
}

pub fn cache_channel_metadata(meta: ChannelMetadataUpdate) {
    CHANNEL_METADATA_CACHE
        .write()
        .insert(meta.channel_id.clone(), meta);
}

/// Get effective channel info (base + latest metadata overlay)
pub fn get_channel_display_info(channel: &Channel) -> (Option<String>, Option<String>, Option<String>) {
    let meta_cache = CHANNEL_METADATA_CACHE.read();
    if let Some(meta) = meta_cache.get(&channel.id) {
        (
            meta.name.clone().or_else(|| channel.name.clone()),
            meta.about.clone().or_else(|| channel.about.clone()),
            meta.picture.clone().or_else(|| channel.picture.clone()),
        )
    } else {
        (channel.name.clone(), channel.about.clone(), channel.picture.clone())
    }
}

// -- Filters --

pub fn channel_messages_filter(channel_id: EventId, limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::ChannelMessage)
        .event(channel_id)
        .limit(limit)
}

pub fn channel_metadata_filter(channel_id: EventId) -> Filter {
    Filter::new()
        .kind(Kind::ChannelMetadata)
        .event(channel_id)
        .limit(1)
}

pub fn channel_messages_realtime_filter(channel_id: EventId) -> Filter {
    Filter::new()
        .kind(Kind::ChannelMessage)
        .event(channel_id)
        .since(Timestamp::now())
        .limit(0)
}

pub fn channel_creation_by_id_filter(event_id: EventId) -> Filter {
    Filter::new()
        .kind(Kind::ChannelCreation)
        .id(event_id)
        .limit(1)
}

// -- Fetching --

pub fn channels_discovery_filter_until(limit: usize, until: Option<u64>) -> Filter {
    let mut filter = Filter::new().kind(Kind::ChannelCreation).limit(limit);
    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }
    filter
}

pub async fn fetch_channels_page(
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<Channel>, String> {
    let filter = channels_discovery_filter_until(limit, until);
    let events = fetch_events_aggregated(filter, Duration::from_secs(15)).await?;
    let mut channels: Vec<Channel> = events.iter().filter_map(parse_channel_creation).collect();
    channels.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    for ch in &channels {
        cache_channel(ch.clone());
    }
    Ok(channels)
}

pub async fn search_channels(
    query: &str,
    limit: usize,
) -> std::result::Result<Vec<Channel>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    let query_lower = query.to_lowercase();

    // Step 1: Try NIP-50 relay search
    let search_filter = Filter::new()
        .kind(Kind::ChannelCreation)
        .search(query)
        .limit(limit);
    if let Ok(events) = client
        .fetch_events(search_filter, Duration::from_secs(5))
        .await
    {
        if !events.is_empty() {
            let channels: Vec<Channel> =
                events.iter().filter_map(parse_channel_creation).collect();
            for ch in &channels {
                cache_channel(ch.clone());
            }
            return Ok(channels);
        }
    }

    // Step 2: Fallback to client-side filtering
    let fallback_filter = Filter::new().kind(Kind::ChannelCreation).limit(500);
    let events = client
        .fetch_events(fallback_filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Search fallback failed: {}", e))?;
    let mut channels: Vec<Channel> = events
        .iter()
        .filter_map(parse_channel_creation)
        .filter(|c| {
            c.name
                .as_ref()
                .map(|n| n.to_lowercase().contains(&query_lower))
                .unwrap_or(false)
                || c.about
                    .as_ref()
                    .map(|d| d.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
        })
        .take(limit)
        .collect();
    channels.sort_by(|a, b| {
        let a_match = a
            .name
            .as_ref()
            .map(|n| n.to_lowercase().contains(&query_lower))
            .unwrap_or(false);
        let b_match = b
            .name
            .as_ref()
            .map(|n| n.to_lowercase().contains(&query_lower))
            .unwrap_or(false);
        b_match.cmp(&a_match)
    });
    for ch in &channels {
        cache_channel(ch.clone());
    }
    Ok(channels)
}

// -- Event ID decoding --

/// Decode a channel_id string (nevent1.../note1.../hex) into EventId + relay hints
pub fn decode_channel_id(channel_id_str: &str) -> std::result::Result<(EventId, Vec<RelayUrl>), String> {
    // Try bech32 first (nevent1... or note1...)
    if let Ok(nip19) = Nip19::from_bech32(channel_id_str) {
        match nip19 {
            Nip19::Event(nip19_event) => {
                return Ok((nip19_event.event_id, nip19_event.relays));
            }
            Nip19::EventId(event_id) => {
                return Ok((event_id, vec![]));
            }
            _ => {}
        }
    }

    // Fallback: hex event ID
    EventId::from_hex(channel_id_str)
        .map(|id| (id, vec![]))
        .map_err(|e| format!("Invalid channel ID: {}", e))
}

// -- Relay URL helpers --

/// Get the best relay URL for sending channel messages
pub async fn get_channel_relay_url(relay_hints: &[RelayUrl]) -> RelayUrl {
    if let Some(url) = relay_hints.first() {
        return url.clone();
    }
    if let Some(client) = get_client() {
        if let Some((url, _)) = client.relays().await.into_iter().next() {
            return url;
        }
    }
    RelayUrl::parse("wss://relay.damus.io").unwrap()
}

// -- Send helpers --

/// Send a message to a channel
pub async fn send_channel_message(
    channel_id: EventId,
    relay_url: RelayUrl,
    content: &str,
) -> std::result::Result<Event, String> {
    let client = get_client().ok_or("Client not initialized")?;
    let builder = EventBuilder::channel_msg(channel_id, relay_url, content);
    let event = client
        .sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    client
        .send_event(&event)
        .await
        .map_err(|e| format!("Failed to send: {}", e))?;
    Ok(event)
}

/// Create a new channel
pub async fn create_channel(
    name: &str,
    about: &str,
    picture: &str,
) -> std::result::Result<Event, String> {
    let client = get_client().ok_or("Client not initialized")?;
    let mut metadata = Metadata::new().name(name);
    if !about.is_empty() {
        metadata = metadata.about(about);
    }
    if !picture.is_empty() {
        metadata = metadata.picture(Url::parse(picture).map_err(|e| format!("Invalid picture URL: {}", e))?);
    }
    let builder = EventBuilder::channel(&metadata);
    let event = client
        .sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    client
        .send_event(&event)
        .await
        .map_err(|e| format!("Failed to send: {}", e))?;
    Ok(event)
}
