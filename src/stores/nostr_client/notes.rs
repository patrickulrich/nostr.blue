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

    // Track tagged pubkeys for Outbox routing (currently unused but prepared for future outbox implementation)
    let mut _tagged_pubkeys: Vec<PublicKey> = mentioned_pubkeys.clone();

    // Convert tags to nostr Tag format
    use nostr::Tag;
    use nostr_sdk::nips::nip10::Marker;
    let nostr_tags: Vec<Tag> = tags
        .into_iter()
        .filter_map(|tag_vec| {
            if tag_vec.is_empty() {
                return None;
            }
            // Convert string vector to Tag
            match tag_vec[0].as_str() {
                "e" if tag_vec.len() >= 4 && !tag_vec[3].is_empty() => {
                    // E-tag with marker (for threading)
                    let event_id = nostr::EventId::from_hex(&tag_vec[1]).ok()?;

                    // Parse marker from 4th element (NIP-10: only "root" and "reply")
                    let marker = match tag_vec[3].as_str() {
                        "root" => Some(Marker::Root),
                        "reply" => Some(Marker::Reply),
                        _ => None,
                    };

                    if let Some(m) = marker {
                        // Parse optional relay URL (3rd element)
                        let relay_url = if !tag_vec[2].is_empty() {
                            nostr_sdk::RelayUrl::parse(&tag_vec[2]).ok()
                        } else {
                            None
                        };

                        // Construct event tag with marker
                        let tag_standard = nostr::TagStandard::Event {
                            event_id,
                            relay_url,
                            marker: Some(m),
                            public_key: None,
                            uppercase: false,
                        };

                        Some(Tag::from(tag_standard))
                    } else {
                        // Invalid marker, fallback to simple event tag
                        Some(Tag::event(event_id))
                    }
                },
                "e" if tag_vec.len() >= 2 => {
                    // Simple e-tag without marker
                    Some(Tag::event(
                        nostr::EventId::from_hex(&tag_vec[1]).ok()?
                    ))
                },
                "p" if tag_vec.len() >= 2 => {
                    // Extract pubkey for Outbox routing (currently unused but prepared for future)
                    if let Ok(pubkey) = nostr::PublicKey::from_hex(&tag_vec[1]) {
                        _tagged_pubkeys.push(pubkey);
                        Some(Tag::public_key(pubkey))
                    } else {
                        None
                    }
                },
                _ => {
                    // Generic tag
                    Some(Tag::custom(
                        nostr::TagKind::Custom(tag_vec[0].clone().into()),
                        tag_vec[1..].to_vec()
                    ))
                }
            }
        })
        .collect();

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
