//! Text notes (kind 1)
//!
//! Publishing functions for text notes (NIP-01).
use super::fetching::get_client;
use super::signals::HAS_SIGNER;
use super::types::PublishResult;
use crate::utils::custom_emoji::build_custom_emoji_tags;
use crate::utils::mention_extractor::{create_mention_tags, extract_mentioned_pubkeys};
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
/// Extract quote tags from content containing nostr: URIs (NIP-18 compliance)
/// Returns q tags for note1/nevent1/naddr1 references
/// Deduplicates tags to avoid duplicate q tags for the same reference
fn extract_quote_tags(content: &str) -> Vec<nostr::event::Tag> {
    use nostr::nips::nip18::Nip18Tag;
    use nostr::nips::nip19::Nip19;
    use nostr::nips::nip01::Coordinate;
    use std::collections::HashSet;
    let mut tags = Vec::new();
    let mut seen_identifier = HashSet::new();
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
            if !seen_identifier.insert(bech32.to_string()) {
                continue;
            }
            match Nip19::from_bech32(bech32) {
                Ok(nip19) => {
                    let tag = match nip19 {
                        Nip19::EventId(id) => Some(nostr::event::Tag::from(Nip18Tag::Quote {
                            id,
                            relay_hint: None,
                            public_key: None,
                        })),
                        Nip19::Event(nevent) => Some(nostr::event::Tag::from(Nip18Tag::Quote {
                            id: nevent.event_id,
                            relay_hint: nevent.relays.first().cloned(),
                            public_key: nevent.author,
                        })),
                        Nip19::Coordinate(coord) => Some(nostr::event::Tag::from(
                            Nip18Tag::QuoteAddress {
                                coordinate: Coordinate::new(coord.kind, coord.public_key)
                                    .identifier(coord.identifier.clone()),
                                relay_hint: coord.relays.first().cloned(),
                            },
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
/// Publish a text note (kind 1 event) with relay feedback
/// Returns PublishResult with success/failure tracking per relay
pub async fn publish_note_tracked(
    content: String,
    tags: Vec<Vec<String>>,
    content_warning: Option<String>,
    is_protected: bool,
) -> std::result::Result<PublishResult, String> {
    let _client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    log::info!("Publishing note with {} characters", content.len());
    let mentioned_pubkeys = extract_mentioned_pubkeys(&content);
    let mut mention_tags = create_mention_tags(&mentioned_pubkeys);
    log::debug!(
        "Extracted {} mentions from content",
        mentioned_pubkeys.len()
    );
    let nostr_tags = super::types::convert_raw_tags(tags);
    mention_tags.extend(nostr_tags);
    let quote_tags = extract_quote_tags(&content);
    mention_tags.extend(quote_tags);
    let custom_emoji_tags = build_custom_emoji_tags(&content);
    mention_tags.extend(custom_emoji_tags);
    // NIP-92 imeta attachments: emit one tag per pending attachment whose
    // URL appears in the content (registry entries are consumed here).
    let imeta_raw = crate::stores::media::imeta::take_imeta_tags_for(&content);
    if !imeta_raw.is_empty() {
        log::debug!("Attaching {} imeta tag(s)", imeta_raw.len());
        mention_tags.extend(crate::stores::media::imeta::to_nostr_tags(imeta_raw));
    }
    if let Some(reason) = content_warning {
        mention_tags.push(nostr::event::Tag::custom(
            "content-warning",
            if reason.is_empty() { None } else { Some(reason) },
        ));
    }
    if is_protected {
        mention_tags.push(nostr::event::Tag::protected());
    }
    let mut seen_pubkeys = std::collections::HashSet::new();
    mention_tags.retain(|tag| {
        if tag.kind() == "p" {
            if let Some(pk) = tag.content() {
                return seen_pubkeys.insert(pk.to_string());
            }
        }
        true
    });
    let builder = nostr::event::EventBuilder::new(nostr::event::Kind::TextNote, &content).tags(mention_tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign note: {}", e))?;
    // Inbox-union routing: also deliver to the mentioned users' known read
    // relays (from the outbox coverage map) so mentions actually reach them.
    let mut mention_relay_urls: Vec<String> = Vec::new();
    for pk in &mentioned_pubkeys {
        if let Some(relays) =
            crate::stores::relay::coverage::get_known_user_relays(&pk.to_hex())
        {
            for url in relays {
                if !mention_relay_urls.contains(&url) {
                    mention_relay_urls.push(url);
                }
            }
        }
    }
    mention_relay_urls.truncate(20);
    let target_relays = if mention_relay_urls.is_empty() {
        None
    } else {
        Some(mention_relay_urls)
    };
    let queue_id = crate::stores::publish_queue::enqueue(
        event.clone(),
        crate::stores::publish_queue::types::QueueEventType::Note,
        target_relays,
        std::collections::HashMap::new(),
    ).await;
    let result = PublishResult::queued_with_event(queue_id, event);
    log::info!("Note queued: {}", result.event_id);
    Ok(result)
}
/// Publish a text note (kind 1 event)
/// For relay feedback, use publish_note_tracked instead
pub async fn publish_note(
    content: String,
    tags: Vec<Vec<String>>,
) -> std::result::Result<String, String> {
    publish_note_tracked(content, tags, None, false)
        .await
        .map(|result| result.event_id)
}
