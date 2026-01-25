//! Custom NIPs (kind 30817)
//!
//! Functions for community NIP proposals - addressable events for custom NIPs.

use std::time::Duration;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

use super::fetching::{get_client, fetch_events_aggregated};
use super::signals::HAS_SIGNER;
use super::types::PublishResult;

// =============================================================================
// Constants
// =============================================================================

/// Kind 30817 - Custom NIP (addressable event)
///
/// A community-driven event type for proposing and discussing custom NIPs.
/// Uses the addressable event pattern (30000-39999 range) for deduplication.
///
/// # Event Schema
/// - **Kind**: 30817 (addressable, parameterized replaceable)
/// - **Content**: Markdown body of the NIP proposal
///
/// # Required Tags
/// - `d`: Unique identifier for this NIP (e.g., "my-custom-nip" or UUID)
/// - `title`: Human-readable title of the proposal
///
/// # Optional Tags
/// - `k`: Related event kinds this NIP affects (can appear multiple times)
/// - `alt`: NIP-31 fallback description for clients that don't support this kind
///
/// # Example
/// ```json
/// {
///   "kind": 30817,
///   "content": "# My Custom NIP\n\nThis NIP defines...",
///   "tags": [
///     ["d", "my-custom-nip"],
///     ["title", "My Custom NIP"],
///     ["k", "1"],
///     ["k", "6"],
///     ["alt", "Custom NIP proposal: My Custom NIP"]
///   ]
/// }
/// ```
///
/// # References
/// - NIP-33: Parameterized Replaceable Events
/// - NIP-31: Alt tag for unknown event kinds
pub const KIND_CUSTOM_NIP: u16 = 30817;

// =============================================================================
// Custom NIP Fetching
// =============================================================================

/// Fetch custom NIPs (kind 30817) from relays
pub async fn fetch_custom_nips(
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let filter = {
        let mut f = Filter::new()
            .kind(Kind::Custom(KIND_CUSTOM_NIP))
            .limit(limit);

        if let Some(until_ts) = until {
            f = f.until(Timestamp::from(until_ts));
        }

        f
    };

    fetch_events_aggregated(filter, Duration::from_secs(10)).await
}

/// Fetch a specific custom NIP by decoding an naddr identifier
pub async fn fetch_custom_nip_by_naddr(
    naddr: &str,
) -> std::result::Result<Option<nostr::Event>, String> {
    use nostr::nips::nip19::Nip19;

    // Decode naddr to get coordinate
    let nip19 = Nip19::from_bech32(naddr)
        .map_err(|e| format!("Invalid naddr: {}", e))?;

    match nip19 {
        Nip19::Coordinate(nip19_coord) => {
            let coord = nip19_coord.coordinate;

            // Validate kind matches KIND_CUSTOM_NIP (30817) - nostr-sdk pattern
            if coord.kind != Kind::Custom(KIND_CUSTOM_NIP) {
                return Err(format!(
                    "Invalid kind for custom NIP: expected {}, got {}",
                    KIND_CUSTOM_NIP,
                    coord.kind.as_u16()
                ));
            }

            let filter = Filter::new()
                .kind(coord.kind)
                .author(coord.public_key)
                .identifier(coord.identifier);

            // nostr-sdk pattern: For addressable events, always get max created_at
            let events = fetch_events_aggregated(filter, Duration::from_secs(10)).await?;
            Ok(events.into_iter().max_by_key(|e| e.created_at))
        }
        _ => Err("Not a coordinate (naddr) identifier".to_string()),
    }
}

// =============================================================================
// Custom NIP Publishing
// =============================================================================

/// Publish a custom NIP as a kind 30817 addressable event with relay tracking
pub async fn publish_custom_nip_tracked(
    title: String,
    content: String,
    identifier: String,
    related_kinds: Vec<u32>,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    use nostr::{EventBuilder, Kind, Tag, TagKind, SingleLetterTag, Alphabet};

    // Build event with required d-tag and optional tags
    // nostr-sdk NIP-31 pattern: Add alt tag for clients that don't understand custom kinds
    let mut builder = EventBuilder::new(Kind::Custom(KIND_CUSTOM_NIP), &content)
        .tag(Tag::identifier(&identifier))
        .tag(Tag::title(&title))
        .tag(Tag::alt(format!("Custom NIP proposal: {}", title)));

    // Add k tags for related event kinds
    for kind in related_kinds {
        builder = builder.tag(Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::K)),
            vec![kind.to_string()],
        ));
    }

    let output = client.send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish custom NIP: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Custom NIP published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a custom NIP as a kind 30817 addressable event
pub async fn publish_custom_nip(
    title: String,
    content: String,
    identifier: String,
    related_kinds: Vec<u32>,
) -> std::result::Result<String, String> {
    publish_custom_nip_tracked(title, content, identifier, related_kinds)
        .await
        .map(|result| result.event_id)
}

// =============================================================================
// Custom NIP Utilities
// =============================================================================

/// Generate an naddr for a custom NIP event
pub fn generate_custom_nip_naddr(
    pubkey: &PublicKey,
    identifier: &str,
    relays: Vec<String>,
) -> std::result::Result<String, String> {
    use nostr::nips::nip01::Coordinate;
    use nostr::nips::nip19::Nip19Coordinate;

    let coordinate = Coordinate::new(Kind::Custom(KIND_CUSTOM_NIP), *pubkey)
        .identifier(identifier);

    let mut failed_count = 0;
    let relay_urls: Vec<nostr::RelayUrl> = relays
        .iter()
        .filter_map(|r| {
            match nostr::RelayUrl::parse(r) {
                Ok(url) => Some(url),
                Err(_) => {
                    failed_count += 1;
                    None
                }
            }
        })
        .collect();

    if failed_count > 0 {
        log::warn!("{} relay URL(s) failed to parse for naddr", failed_count);
    }

    let nip19_coord = Nip19Coordinate::new(coordinate, relay_urls);

    nip19_coord.to_bech32()
        .map_err(|e| format!("Failed to generate naddr: {}", e))
}

/// Search custom NIPs using NIP-50 full-text search
///
/// Falls back to client-side filtering if NIP-50 returns empty.
/// Primary search uses NIP-50-capable relays (e.g., relay.nostr.band,
/// cache1.primal.net) for server-side full-text search.
pub async fn search_custom_nips(
    query: &str,
    limit: usize,
) -> std::result::Result<Vec<nostr::Event>, String> {
    // Early return for empty query (Dioxus pattern: trim then is_empty check)
    // Avoids relay-dependent .search("") behavior
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let timeout = Duration::from_secs(10);

    // Try NIP-50 server-side search first
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_CUSTOM_NIP))
        .search(query)
        .limit(limit);

    let results = fetch_events_aggregated(filter, timeout).await?;

    // Fallback: client-side filter if NIP-50 returned empty
    if results.is_empty() && !query.is_empty() {
        log::info!("NIP-50 search returned empty, trying client-side filter");

        let fallback_filter = Filter::new()
            .kind(Kind::Custom(KIND_CUSTOM_NIP))
            .limit(500);

        let all_events = fetch_events_aggregated(fallback_filter, timeout).await?;
        let query_lower = query.to_lowercase();

        return Ok(all_events.into_iter()
            .filter(|e| {
                let content_matches = e.content.to_lowercase().contains(&query_lower);
                let title_matches = e.tags.iter().any(|tag| {
                    if let Some(nostr::TagStandard::Title(title)) = tag.as_standardized() {
                        title.to_lowercase().contains(&query_lower)
                    } else {
                        false
                    }
                });
                content_matches || title_matches
            })
            .take(limit)
            .collect());
    }

    Ok(results)
}
