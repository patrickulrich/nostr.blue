//! Relay-specific publishing
//!
//! Functions for publishing events to specific relays.
//! Note: With NIP-65 gossip routing, SDK handles relay selection automatically.
//! These functions are available for advanced use cases but not typically needed.
use super::fetching::get_client;
use super::signals::HAS_SIGNER;
use super::types::PublishResult;
use crate::stores::relay;
use dioxus::prelude::ReadableExt;
use futures::future::join_all;
use nostr_sdk::prelude::*;
/// Parse relay URLs with validation logging and deduplication
///
/// Returns validated, deduplicated URLs and logs warnings for any invalid URLs.
/// Returns an error if no valid URLs remain after filtering.
///
/// # Errors
/// Returns `Err` if all provided URLs are invalid.
fn parse_relay_urls(relay_urls: &[String]) -> Result<Vec<nostr::RelayUrl>, String> {
    use std::collections::HashSet;
    let (valid_urls, invalid_urls): (Vec<_>, Vec<_>) = relay_urls
        .iter()
        .map(|r| (r.clone(), nostr::RelayUrl::parse(r)))
        .partition(|(_, result)| result.is_ok());
    for (url, _) in &invalid_urls {
        log::warn!("Invalid relay URL skipped: {}", url);
    }
    let mut seen = HashSet::new();
    let urls: Vec<nostr::RelayUrl> = valid_urls
        .into_iter()
        .filter_map(|(_, r)| r.ok())
        .filter(|url| seen.insert(url.to_string()))
        .collect();
    if urls.is_empty() {
        return Err("No valid relay URLs provided".to_string());
    }
    Ok(urls)
}
/// Publish a note to specific relays only
///
/// Useful for privacy-conscious publishing or targeting specific relay groups.
#[allow(dead_code)]
pub async fn publish_note_to_relays(
    content: String,
    tags: Vec<Vec<String>>,
    relay_urls: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    let _client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let nostr_tags = super::types::convert_raw_tags(tags);
    let builder = nostr::EventBuilder::text_note(&content).tags(nostr_tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign note: {}", e))?;
    let event_id = event.id.to_hex();
    let queue_id = crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Note,
        Some(relay_urls),
        std::collections::HashMap::new(),
    ).await;
    let result = PublishResult::queued(queue_id, event_id);
    log::info!("Note queued for specific relays: {}", result.event_id);
    Ok(result)
}
/// Publish a reaction to specific relays only
#[allow(dead_code)]
pub async fn publish_reaction_to_relays(
    event_id: String,
    event_pubkey: String,
    reaction: String,
    relay_urls: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    let _client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    use nostr::nips::nip25::ReactionTarget;
    let target_event_id = {
        use nostr::nips::nip19::Nip19;
        match Nip19::from_bech32(&event_id) {
            Ok(Nip19::EventId(id)) => id,
            Ok(Nip19::Event(e)) => e.event_id,
            _ => {
                nostr::EventId::parse(&event_id).map_err(|e| format!("Invalid event ID: {}", e))?
            }
        }
    };
    let target_pubkey = PublicKey::parse(&event_pubkey)
        .map_err(|e| format!("Invalid pubkey (expected hex or npub): {}", e))?;
    let target = ReactionTarget {
        event_id: target_event_id,
        public_key: target_pubkey,
        coordinate: None,
        kind: None,
        relay_hint: None,
    };
    let builder = EventBuilder::reaction(target, reaction);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign reaction: {}", e))?;
    let event_id = event.id.to_hex();
    let queue_id = crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Reaction,
        Some(relay_urls),
        std::collections::HashMap::new(),
    ).await;
    let result = PublishResult::queued(queue_id, event_id);
    log::info!("Reaction queued for specific relays: {}", result.event_id);
    Ok(result)
}

/// Publish a NIP-62 vanish request to specific relays only.
pub async fn publish_vanish_request_to_relays(
    relay_urls: Vec<String>,
    reason: String,
) -> std::result::Result<PublishResult, String> {
    let _client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let urls = parse_relay_urls(&relay_urls)?
        .into_iter()
        .filter(|relay_url| !relay::is_relay_blocked(relay_url.as_str()))
        .collect::<Vec<_>>();
    if urls.is_empty() {
        return Err("No unblocked relay URLs provided".to_string());
    }

    let target = VanishTarget::relays(urls.clone());
    let builder = EventBuilder::request_vanish_with_reason(target, reason.trim().to_string())
        .map_err(|e| format!("Failed to build vanish request: {}", e))?;
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign vanish request: {}", e))?;
    let event_id = event.id.to_hex();
    let relay_strs: Vec<String> = urls.into_iter().map(|u| u.to_string()).collect();
    let queue_id = crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("vanish".to_string()),
        Some(relay_strs),
        std::collections::HashMap::new(),
    ).await;
    let result = PublishResult::queued(queue_id, event_id);
    log::info!("Vanish request queued: {}", result.event_id);
    Ok(result)
}
/// Send a pre-signed event to specific relays
///
/// Takes an already-signed Event and sends it directly to the specified relays,
/// preserving the original cryptographic signature.
pub async fn send_presigned_event_to_relays(
    event: nostr::Event,
    relay_urls: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    let _client = get_client().ok_or("Client not initialized")?;
    let event_id = event.id.to_hex();
    let queue_id = crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("broadcast".to_string()),
        Some(relay_urls),
        std::collections::HashMap::new(),
    ).await;
    let result = PublishResult::queued(queue_id, event_id);
    log::info!("Pre-signed event queued: {}", result.event_id);
    Ok(result)
}

/// Broadcast an existing signed event to additional relays.
pub async fn broadcast_presigned_event(
    event: nostr::Event,
    relay_urls: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    send_presigned_event_to_relays(event, relay_urls).await
}
