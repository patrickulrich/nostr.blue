//! Reactions (kind 7)
//!
//! Publishing functions for reactions (NIP-25, NIP-30).
use super::fetching::get_client;
use super::signals::HAS_SIGNER;
use super::types::PublishResult;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
/// Publish a reaction (kind 7 event) with relay feedback
/// NIP-25: https://github.com/nostr-protocol/nips/blob/master/25.md
/// NIP-30: Custom emoji support via emoji_tag parameter
pub async fn publish_reaction_tracked(
    event_id: String,
    event_author: String,
    content: String,
    emoji_tag: Option<(String, String)>,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    log::info!("Publishing reaction to event: {}", event_id);
    use nostr::event::tag::TagStandard;
    use nostr::nips::nip25::ReactionTarget;
    use nostr::{EventId, PublicKey, Tag, Url};
    use nostr_sdk::nips::nip01::Coordinate;
    let target_event_id =
        EventId::from_hex(&event_id).map_err(|e| format!("Invalid event ID: {}", e))?;
    let target_pubkey =
        PublicKey::parse(&event_author).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let (event_kind, event_coordinate) = match client.database().event_by_id(&target_event_id).await
    {
        Ok(Some(event)) => {
            let kind = Some(event.kind);
            let coordinate = if event.kind.is_addressable() {
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
    let target = ReactionTarget {
        event_id: target_event_id,
        public_key: target_pubkey,
        coordinate: event_coordinate,
        kind: event_kind,
        relay_hint: None,
    };
    let mut builder = nostr::EventBuilder::reaction(target, content);
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
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
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
