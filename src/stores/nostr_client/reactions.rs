//! Reactions (kind 7)
//!
//! Publishing functions for reactions (NIP-25, NIP-30).

use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

use super::fetching::get_client;
use super::signals::HAS_SIGNER;
use super::types::PublishResult;

// =============================================================================
// Reaction Publishing
// =============================================================================

/// Publish a reaction (kind 7 event) with relay feedback
/// NIP-25: https://github.com/nostr-protocol/nips/blob/master/25.md
/// NIP-30: Custom emoji support via emoji_tag parameter
pub async fn publish_reaction_tracked(
    event_id: String,
    event_author: String,
    content: String,
    emoji_tag: Option<(String, String)>, // (shortcode, url) for custom emoji reactions
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing reaction to event: {}", event_id);

    // Parse event ID and author pubkey
    use nostr::event::tag::TagStandard;
    use nostr::nips::nip25::ReactionTarget;
    use nostr::{EventId, PublicKey, Tag, Url};
    use nostr_sdk::nips::nip01::Coordinate;

    let target_event_id =
        EventId::from_hex(&event_id).map_err(|e| format!("Invalid event ID: {}", e))?;
    let target_pubkey =
        PublicKey::parse(&event_author).map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Try to fetch the original event to get its kind and coordinate
    // This enables proper NIP-25 compliance with 'a' and 'k' tags
    let (event_kind, event_coordinate) = match client.database().event_by_id(&target_event_id).await
    {
        Ok(Some(event)) => {
            let kind = Some(event.kind);
            // For addressable events (30000-39999), include coordinate
            let coordinate = if event.kind.is_addressable() {
                // Use SDK's identifier() method for d-tag lookup
                event.tags.identifier().map(|id| Coordinate {
                    kind: event.kind,
                    public_key: event.pubkey,
                    identifier: id.to_string(),
                })
            } else {
                None
            };
            (kind, coordinate)
        }
        Ok(None) => {
            log::debug!("Event {} not found in DB for NIP-25 tags", event_id);
            (None, None)
        }
        Err(e) => {
            log::debug!("Failed to fetch event {} for NIP-25 tags: {}", event_id, e);
            (None, None)
        }
    };

    // Use EventBuilder::reaction() with ReactionTarget for proper NIP-25 compliance
    let target = ReactionTarget {
        event_id: target_event_id,
        public_key: target_pubkey,
        coordinate: event_coordinate,
        kind: event_kind,
        relay_hint: None,
    };

    let mut builder = nostr::EventBuilder::reaction(target, content);

    // Add emoji tag for custom emojis (NIP-30)
    // Validate http/https scheme only (security concern)
    if let Some((shortcode, url_str)) = emoji_tag {
        if let Ok(parsed_url) = Url::parse(&url_str) {
            match parsed_url.scheme() {
                "http" | "https" => {
                    builder =
                        builder.tag(Tag::from_standardized_without_cell(TagStandard::Emoji {
                            shortcode,
                            url: parsed_url,
                        }));
                    log::info!("Added custom emoji tag to reaction");
                }
                scheme => {
                    log::warn!(
                        "Rejected emoji URL with invalid scheme '{}': {}",
                        scheme,
                        url_str
                    );
                }
            }
        } else {
            log::warn!("Failed to parse custom emoji URL: {}", url_str);
        }
    }

    // Publish using gossip - automatic relay routing
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish reaction: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Reaction published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    // nostr-sdk pattern: Total failure when no relays succeeded
    if result.success_count() == 0 {
        return Err("Reaction failed: no relays accepted the event".to_string());
    }

    Ok(result)
}

/// Publish a reaction (kind 7 event) to another event
/// For relay feedback, use publish_reaction_tracked instead
pub async fn publish_reaction(
    event_id: String,
    event_author: String,
    content: String,
    emoji_tag: Option<(String, String)>,
) -> std::result::Result<String, String> {
    publish_reaction_tracked(event_id, event_author, content, emoji_tag)
        .await
        .map(|result| result.event_id)
}
