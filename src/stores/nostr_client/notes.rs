//! Text notes (kind 1)
//!
//! Publishing functions for text notes (NIP-01).

use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

use super::fetching::get_client;
use super::signals::HAS_SIGNER;
use super::types::PublishResult;
use crate::utils::mention_extractor::{extract_mentioned_pubkeys, create_mention_tags};

// =============================================================================
// Quote Tag Extraction (NIP-18)
// =============================================================================

/// Extract quote tags from content containing nostr: URIs (NIP-18 compliance)
/// Returns q tags for note1/nevent1/naddr1 references
fn extract_quote_tags(content: &str) -> Vec<nostr::Tag> {
    use nostr::nips::nip19::Nip19;
    use nostr::event::tag::TagStandard;
    use nostr_sdk::nips::nip01::Coordinate;

    let mut tags = Vec::new();

    // Match nostr:note1..., nostr:nevent1..., nostr:naddr1...
    let re = match regex::Regex::new(r"nostr:(note1[a-z0-9]+|nevent1[a-z0-9]+|naddr1[a-z0-9]+)") {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to compile quote regex: {}", e);
            return tags;
        }
    };

    for cap in re.captures_iter(content) {
        if let Some(bech32_match) = cap.get(1) {
            let bech32 = bech32_match.as_str();
            match Nip19::from_bech32(bech32) {
                Ok(nip19) => {
                    let tag = match nip19 {
                        Nip19::EventId(id) => Some(nostr::Tag::from_standardized_without_cell(
                            TagStandard::Quote {
                                event_id: id,
                                relay_url: None,
                                public_key: None,
                            }
                        )),
                        Nip19::Event(nevent) => Some(nostr::Tag::from_standardized_without_cell(
                            TagStandard::Quote {
                                event_id: nevent.event_id,
                                relay_url: nevent.relays.first().cloned(),
                                public_key: nevent.author,
                            }
                        )),
                        Nip19::Coordinate(coord) => Some(nostr::Tag::from_standardized_without_cell(
                            TagStandard::QuoteAddress {
                                coordinate: Coordinate::new(coord.kind, coord.public_key)
                                    .identifier(coord.identifier.clone()),
                                relay_url: coord.relays.first().cloned(),
                            }
                        )),
                        _ => None,
                    };
                    if let Some(t) = tag {
                        tags.push(t);
                    }
                }
                Err(e) => {
                    log::debug!("Failed to parse nostr URI '{}': {}", bech32, e);
                }
            }
        }
    }

    log::debug!("Extracted {} quote tags from content", tags.len());
    tags
}

// =============================================================================
// Note Publishing
// =============================================================================

/// Publish a text note (kind 1 event) with relay feedback
/// Returns PublishResult with success/failure tracking per relay
pub async fn publish_note_tracked(content: String, tags: Vec<Vec<String>>) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing note with {} characters", content.len());

    // Extract mentions from content and create p tags
    let mentioned_pubkeys = extract_mentioned_pubkeys(&content);
    let mut mention_tags = create_mention_tags(&mentioned_pubkeys);
    log::debug!("Extracted {} mentions from content", mentioned_pubkeys.len());

    // Note: tagged pubkeys can be derived from mentioned_pubkeys for future outbox routing

    // Convert tags to nostr Tag format using shared helper
    let nostr_tags = super::types::convert_raw_tags(tags);

    // Combine mention tags with other tags
    mention_tags.extend(nostr_tags);

    // Extract and add quote tags (NIP-18 compliance)
    let quote_tags = extract_quote_tags(&content);
    mention_tags.extend(quote_tags);

    // Build the event
    let builder = nostr::EventBuilder::text_note(&content).tags(mention_tags);

    // Publish using gossip - automatic relay routing
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish: {}", e))?;

    let result = PublishResult::from_output(output);

    // Log relay feedback
    log::info!(
        "Note published: {} ({}/{} relays succeeded)",
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

/// Publish a text note (kind 1 event)
/// For relay feedback, use publish_note_tracked instead
pub async fn publish_note(content: String, tags: Vec<Vec<String>>) -> std::result::Result<String, String> {
    publish_note_tracked(content, tags)
        .await
        .map(|result| result.event_id)
}
