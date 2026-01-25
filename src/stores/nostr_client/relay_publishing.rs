//! Relay-specific publishing
//!
//! Functions for publishing events to specific relays.
//! Note: With NIP-65 gossip routing, SDK handles relay selection automatically.
//! These functions are available for advanced use cases but not typically needed.

use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

use super::fetching::get_client;
use super::signals::HAS_SIGNER;
use super::types::PublishResult;

// =============================================================================
// Relay-Specific Note Publishing
// =============================================================================

/// Publish a note to specific relays only
///
/// Useful for privacy-conscious publishing or targeting specific relay groups.
#[allow(dead_code)]
pub async fn publish_note_to_relays(
    content: String,
    tags: Vec<Vec<String>>,
    relay_urls: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Convert raw tags to nostr::Tag format
    let nostr_tags: Vec<nostr::Tag> = tags.iter()
        .filter_map(|tag| {
            if tag.is_empty() {
                return None;
            }
            Some(nostr::Tag::custom(
                nostr::TagKind::Custom(std::borrow::Cow::Owned(tag[0].clone())),
                tag[1..].to_vec(),
            ))
        })
        .collect();

    let builder = nostr::EventBuilder::text_note(&content)
        .tags(nostr_tags);

    // Parse relay URLs with validation logging
    let (valid_urls, invalid_urls): (Vec<_>, Vec<_>) = relay_urls
        .iter()
        .map(|r| (r.clone(), nostr::RelayUrl::parse(r)))
        .partition(|(_, result)| result.is_ok());

    for (url, _) in &invalid_urls {
        log::warn!("Invalid relay URL skipped: {}", url);
    }

    let urls: Vec<nostr::RelayUrl> = valid_urls
        .into_iter()
        .filter_map(|(_, r)| r.ok())
        .collect();

    if urls.is_empty() {
        return Err("No valid relay URLs provided".to_string());
    }

    let output = client.send_event_builder_to(urls.clone(), builder)
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Note published to specific relays: {} ({}/{} relays succeeded)",
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

// =============================================================================
// Relay-Specific Reaction Publishing
// =============================================================================

/// Publish a reaction to specific relays only
#[allow(dead_code)]
pub async fn publish_reaction_to_relays(
    event_id: String,
    event_pubkey: String,
    reaction: String,
    relay_urls: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    use nostr::nips::nip25::ReactionTarget;

    let target_event_id = nostr::EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    let target_pubkey = PublicKey::from_hex(&event_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Create reaction target
    let target = ReactionTarget {
        event_id: target_event_id,
        public_key: target_pubkey,
        coordinate: None,
        kind: None,
        relay_hint: None,
    };

    let builder = EventBuilder::reaction(target, reaction);

    // Parse relay URLs with validation logging (matching publish_note_to_relays pattern)
    let (valid_urls, invalid_urls): (Vec<_>, Vec<_>) = relay_urls
        .iter()
        .map(|r| (r.clone(), nostr::RelayUrl::parse(r)))
        .partition(|(_, result)| result.is_ok());

    for (url, _) in &invalid_urls {
        log::warn!("Invalid relay URL skipped in reaction publish: {}", url);
    }

    let urls: Vec<nostr::RelayUrl> = valid_urls
        .into_iter()
        .filter_map(|(_, r)| r.ok())
        .collect();

    if urls.is_empty() {
        return Err("No valid relay URLs provided".to_string());
    }

    let output = client.send_event_builder_to(urls, builder)
        .await
        .map_err(|e| format!("Failed to publish reaction: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Reaction published to specific relays: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    // Log per-relay failures for debugging (matching publish_note_to_relays pattern)
    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

// =============================================================================
// Pre-Signed Event Sending
// =============================================================================

/// Send a pre-signed event to specific relays
///
/// Takes an already-signed Event and sends it directly to the specified relays,
/// preserving the original cryptographic signature.
pub async fn send_presigned_event_to_relays(
    event: nostr::Event,
    relay_urls: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    // Parse relay URLs with validation logging
    let (valid_urls, invalid_urls): (Vec<_>, Vec<_>) = relay_urls
        .iter()
        .map(|r| (r.clone(), nostr::RelayUrl::parse(r)))
        .partition(|(_, result)| result.is_ok());

    for (url, _) in &invalid_urls {
        log::warn!("Invalid relay URL skipped: {}", url);
    }

    let urls: Vec<nostr::RelayUrl> = valid_urls
        .into_iter()
        .filter_map(|(_, r)| r.ok())
        .collect();

    if urls.is_empty() {
        return Err("No valid relay URLs provided".to_string());
    }

    let output = client.send_event_to(urls, &event)
        .await
        .map_err(|e| format!("Failed to send event: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Pre-signed event sent to specific relays: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    Ok(result)
}
