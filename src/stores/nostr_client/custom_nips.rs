//! Custom NIPs (kind 30817)
//!
//! Functions for community NIP proposals - addressable events for custom NIPs.

use std::time::Duration;
use nostr_sdk::prelude::*;

use super::fetching::{get_client, fetch_events_aggregated};
use super::types::PublishResult;

// =============================================================================
// Constants
// =============================================================================

/// Kind 30817 - Custom NIP (addressable event)
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

            let filter = Filter::new()
                .kind(coord.kind)
                .author(coord.public_key)
                .identifier(coord.identifier);

            let events = fetch_events_aggregated(filter, Duration::from_secs(10)).await?;
            Ok(events.into_iter().next())
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

    use nostr::{EventBuilder, Kind, Tag, SingleLetterTag, Alphabet};

    // Build event with required d-tag and optional tags
    let mut builder = EventBuilder::new(Kind::Custom(KIND_CUSTOM_NIP), &content)
        .tag(Tag::identifier(&identifier))
        .tag(Tag::title(&title));

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

    let relay_urls: Vec<nostr::RelayUrl> = relays
        .iter()
        .filter_map(|r| nostr::RelayUrl::parse(r).ok())
        .collect();

    let nip19_coord = Nip19Coordinate::new(coordinate, relay_urls);

    nip19_coord.to_bech32()
        .map_err(|e| format!("Failed to generate naddr: {}", e))
}

/// Search custom NIPs using NIP-50 full-text search
///
/// **Note**: Requires NIP-50-capable relays (e.g., relay.nostr.band,
/// cache1.primal.net) for server-side full-text search. Returns empty
/// results on relays without NIP-50 support.
pub async fn search_custom_nips(
    query: &str,
    limit: usize,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_CUSTOM_NIP))
        .search(query)
        .limit(limit);

    fetch_events_aggregated(filter, Duration::from_secs(10)).await
}
