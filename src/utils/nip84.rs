//! NIP-84: Highlights
//!
//! Kind 9802 events for highlighting content from articles, URLs, or other sources.
//! The `.content` field contains the highlighted text, with source references in tags.

// These functions are used in wasm builds only
#![allow(dead_code)]

use crate::stores::nostr_client;
use nostr::Event as NostrEvent;
use nostr_sdk::prelude::*;
use std::borrow::Cow;
use std::time::Duration;

// ============================================================================
// Constants
// ============================================================================

/// NIP-84 Highlights use Kind 9802
pub const KIND_HIGHLIGHT: u16 = 9802;

/// Get the Kind for highlights (no built-in Kind::Highlight in SDK)
pub fn highlight_kind() -> Kind {
    Kind::Custom(KIND_HIGHLIGHT)
}

// ============================================================================
// Data Structures
// ============================================================================

/// Parsed highlight from a Kind 9802 event
#[derive(Clone, Debug, PartialEq)]
pub struct Highlight {
    /// Original Nostr event
    pub event: NostrEvent,
    /// The highlighted text (from event content)
    pub content: String,
    /// Context tag - surrounding text or reference (e.g., "John 3:16 (BSB)")
    pub context: Option<String>,
    /// Comment tag - user's commentary on the highlight
    pub comment: Option<String>,
    /// Parsed source from a/e/r tags
    pub source: HighlightSource,
    /// Author's pubkey (hex)
    pub pubkey: String,
    /// Created timestamp (unix seconds)
    pub created_at: u64,
}

/// Source of the highlighted content
#[derive(Clone, Debug, PartialEq)]
pub enum HighlightSource {
    /// Nostr article reference (from "a" tag) - format: "kind:pubkey:identifier"
    Article {
        coordinate: String,
        relay_hint: Option<String>,
    },
    /// Nostr event reference (from "e" tag)
    Event {
        event_id: String,
        relay_hint: Option<String>,
    },
    /// URL reference (from "r" tag)
    Url(String),
    /// No source specified
    Unknown,
}

impl HighlightSource {
    /// Returns the URL if this is a URL source
    pub fn as_url(&self) -> Option<&str> {
        match self {
            HighlightSource::Url(url) => Some(url),
            _ => None,
        }
    }
}

// ============================================================================
// Parsing Functions
// ============================================================================

/// Parse a Kind 9802 event into a Highlight struct
pub fn parse_highlight(event: &NostrEvent) -> Option<Highlight> {
    // Validate kind
    if event.kind.as_u16() != KIND_HIGHLIGHT {
        return None;
    }

    // Verify signature
    if event.verify().is_err() {
        log::warn!("Invalid highlight event signature: {}", event.id);
        return None;
    }

    let content = event.content.clone();
    let context = get_tag_value(event, "context");
    let comment = get_tag_value(event, "comment");
    let source = parse_highlight_source(event);

    Some(Highlight {
        event: event.clone(),
        content,
        context,
        comment,
        source,
        pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_secs(),
    })
}

/// Extract source reference from highlight event tags
fn parse_highlight_source(event: &NostrEvent) -> HighlightSource {
    // Check for 'a' tag (addressable event reference) first - requires "source" marker
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        if slice.first().map(|s| s.as_str()) == Some("a") {
            // Only accept tags with "source" marker
            let has_source_marker = slice.iter().any(|s| s == "source");
            if !has_source_marker {
                continue;
            }
            if let Some(coord) = slice.get(1) {
                // Filter out "source" marker from relay hint
                let relay = slice
                    .get(2)
                    .filter(|s| *s != "source")
                    .map(|s| s.to_string());
                return HighlightSource::Article {
                    coordinate: coord.to_string(),
                    relay_hint: relay,
                };
            }
        }
    }

    // Check for 'e' tag (event reference) - requires "source" marker
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        if slice.first().map(|s| s.as_str()) == Some("e") {
            // Only accept tags with "source" marker
            let has_source_marker = slice.iter().any(|s| s == "source");
            if !has_source_marker {
                continue;
            }
            if let Some(id) = slice.get(1) {
                // Filter out "source" marker from relay hint
                let relay = slice
                    .get(2)
                    .filter(|s| *s != "source")
                    .map(|s| s.to_string());
                return HighlightSource::Event {
                    event_id: id.to_string(),
                    relay_hint: relay,
                };
            }
        }
    }

    // Check for 'r' tag (URL reference) - requires "source" marker
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        if slice.first().map(|s| s.as_str()) == Some("r") {
            // Only accept tags with "source" marker
            let has_source_marker = slice.iter().any(|s| s == "source");
            if !has_source_marker {
                continue;
            }
            if let Some(url) = slice.get(1) {
                return HighlightSource::Url(url.to_string());
            }
        }
    }

    HighlightSource::Unknown
}

/// Helper to extract a tag value by name
fn get_tag_value(event: &NostrEvent, tag_name: &str) -> Option<String> {
    event.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.first().map(|s| s.as_str()) == Some(tag_name) {
            slice.get(1).map(|s| s.to_string())
        } else {
            None
        }
    })
}

/// Check if event has a specific hashtag
pub fn has_hashtag(event: &NostrEvent, hashtag: &str) -> bool {
    event.tags.iter().any(|tag| {
        let slice = tag.as_slice();
        slice.first().map(|s| s.as_str()) == Some("t")
            && slice.get(1).map(|s| s.as_str()) == Some(hashtag)
    })
}

// ============================================================================
// Fetching Functions
// ============================================================================

/// Fetch highlights by a specific user
pub async fn fetch_user_highlights(pubkey: &PublicKey) -> Result<Vec<Highlight>, String> {
    let client = nostr_client::get_client().ok_or("Nostr client not initialized")?;

    // Ensure relays are connected before fetching
    nostr_client::ensure_relays_ready(&client).await;

    let filter = Filter::new()
        .author(*pubkey)
        .kind(highlight_kind())
        .limit(100);

    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch highlights: {}", e))?;

    let mut highlights: Vec<Highlight> = events.iter().filter_map(parse_highlight).collect();

    // Sort by created_at descending (newest first)
    highlights.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(highlights)
}

/// Fetch highlights for a specific URL (r tag)
pub async fn fetch_highlights_by_url(url: &str) -> Result<Vec<Highlight>, String> {
    let client = nostr_client::get_client().ok_or("Nostr client not initialized")?;

    // Ensure relays are connected before fetching
    nostr_client::ensure_relays_ready(&client).await;

    let filter = Filter::new()
        .kind(highlight_kind())
        .reference(url)
        .limit(100);

    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch highlights: {}", e))?;

    let mut highlights: Vec<Highlight> = events.iter().filter_map(parse_highlight).collect();

    highlights.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(highlights)
}

/// Fetch highlights by authors (for Following feed)
///
/// Uses fast fetch (bypasses gossip) for many-author queries to avoid 30+ second timeouts.
pub async fn fetch_highlights_by_authors(
    authors: Vec<PublicKey>,
    limit: usize,
    until: Option<u64>,
) -> Result<Vec<Highlight>, String> {
    let client = nostr_client::get_client().ok_or("Nostr client not initialized")?;

    // Ensure relays are connected before fetching
    nostr_client::ensure_relays_ready(&client).await;

    let mut filter = Filter::new()
        .authors(authors)
        .kind(highlight_kind())
        .limit(limit);

    if let Some(ts) = until {
        filter = filter.until(Timestamp::from_secs(ts));
    }

    // Get connected relay URLs (bypass gossip for many-author queries)
    let relays = client.relays().await;
    let connected_urls: Vec<nostr::RelayUrl> = relays
        .iter()
        .filter(|(_, r)| r.status() == nostr_relay_pool::RelayStatus::Connected)
        .filter_map(|(url, _)| nostr::RelayUrl::parse(url.as_str()).ok())
        .collect();

    let events = if connected_urls.is_empty() {
        log::warn!("No connected relays for highlights, falling back to gossip");
        client.fetch_events(filter, Duration::from_secs(10)).await
    } else {
        log::info!(
            "Fast fetching highlights from {} connected relays",
            connected_urls.len()
        );
        client
            .fetch_events_from(connected_urls, filter, Duration::from_secs(10))
            .await
    }
    .map_err(|e| format!("Failed to fetch highlights: {}", e))?;

    let mut highlights: Vec<Highlight> = events.iter().filter_map(parse_highlight).collect();

    highlights.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(highlights)
}

/// Fetch global highlights (discovery feed)
pub async fn fetch_global_highlights(
    limit: usize,
    until: Option<u64>,
) -> Result<Vec<Highlight>, String> {
    let client = nostr_client::get_client().ok_or("Nostr client not initialized")?;

    // Ensure relays are connected before fetching
    nostr_client::ensure_relays_ready(&client).await;

    let mut filter = Filter::new().kind(highlight_kind()).limit(limit);

    if let Some(ts) = until {
        filter = filter.until(Timestamp::from_secs(ts));
    }

    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch highlights: {}", e))?;

    let mut highlights: Vec<Highlight> = events.iter().filter_map(parse_highlight).collect();

    highlights.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(highlights)
}

// ============================================================================
// Publishing Functions
// ============================================================================

/// Create and publish a highlight event
/// Uses proper nostr-sdk Tag helpers where available
pub async fn create_highlight(
    content: &str,
    source: HighlightSource,
    context: Option<&str>,
    comment: Option<&str>,
    hashtags: Vec<&str>,
) -> Result<EventId, String> {
    let client = nostr_client::get_client().ok_or("Nostr client not initialized")?;

    if !nostr_client::has_signer() {
        return Err("No signer attached - please sign in".to_string());
    }

    // Ensure relays are connected before publishing
    nostr_client::ensure_relays_ready(&client).await;

    let mut tags = Vec::new();

    // Add source tag using SDK helpers where possible
    match &source {
        HighlightSource::Article {
            coordinate,
            relay_hint,
        } => {
            // "a" tag for addressable event reference
            let mut tag_parts = vec!["a".to_string(), coordinate.clone()];
            if let Some(relay) = relay_hint {
                tag_parts.push(relay.clone());
            }
            tag_parts.push("source".to_string());
            tags.push(Tag::parse(&tag_parts).map_err(|e| e.to_string())?);
        }
        HighlightSource::Event {
            event_id,
            relay_hint,
        } => {
            // Build "e" tag with optional relay hint and "source" marker
            let mut tag_parts = vec!["e".to_string(), event_id.clone()];
            if let Some(relay) = relay_hint {
                tag_parts.push(relay.clone());
            }
            tag_parts.push("source".to_string());
            tags.push(Tag::parse(&tag_parts).map_err(|e| e.to_string())?);
        }
        HighlightSource::Url(url) => {
            // Use SDK's reference tag for "r" tag with "source" marker
            tags.push(Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::R)),
                vec![url.as_str(), "source"],
            ));
        }
        HighlightSource::Unknown => {}
    }

    // Add context tag (custom tag - no SDK helper)
    if let Some(ctx) = context {
        if !ctx.trim().is_empty() {
            tags.push(Tag::custom(
                TagKind::Custom(Cow::Owned("context".to_string())),
                [ctx.to_string()],
            ));
        }
    }

    // Add comment tag (custom tag - no SDK helper)
    if let Some(cmt) = comment {
        if !cmt.trim().is_empty() {
            tags.push(Tag::custom(
                TagKind::Custom(Cow::Owned("comment".to_string())),
                [cmt.to_string()],
            ));
        }
    }

    // Add hashtags using SDK's Tag::hashtag()
    for hashtag in hashtags {
        if !hashtag.trim().is_empty() {
            tags.push(Tag::hashtag(hashtag));
        }
    }

    let event = EventBuilder::new(highlight_kind(), content).tags(tags);

    let output = client
        .send_event_builder(event)
        .await
        .map_err(|e| format!("Failed to publish highlight: {}", e))?;

    log::info!("Highlight published: {}", output.id().to_hex());
    Ok(*output.id())
}

// ============================================================================
// Bible-specific helpers
// ============================================================================

/// Check if a highlight is a Bible highlight (has "bible" hashtag or nostr.blue/bible URL)
pub fn is_bible_highlight(highlight: &Highlight) -> bool {
    // Check for bible hashtag
    let has_bible_tag = has_hashtag(&highlight.event, "bible");

    // Check for nostr.blue/bible URL
    let has_bible_url = matches!(&highlight.source,
        HighlightSource::Url(url) if url.contains("nostr.blue/bible"));

    has_bible_tag || has_bible_url
}

/// Filter highlights to only Bible highlights
pub fn filter_bible_highlights(highlights: Vec<Highlight>) -> Vec<Highlight> {
    highlights.into_iter().filter(is_bible_highlight).collect()
}
