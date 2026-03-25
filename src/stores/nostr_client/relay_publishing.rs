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
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    let nostr_tags = super::types::convert_raw_tags(tags);
    let builder = nostr::EventBuilder::text_note(&content).tags(nostr_tags);
    let urls = parse_relay_urls(&relay_urls)?;
    let output = client
        .send_event_builder_to(urls, builder)
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;
    let result = PublishResult::from_output(output).ignoring_duplicate_event_failures();
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
    let urls = parse_relay_urls(&relay_urls)?;
    let output = client
        .send_event_builder_to(urls, builder)
        .await
        .map_err(|e| format!("Failed to publish reaction: {}", e))?;
    let result = PublishResult::from_output(output).ignoring_duplicate_event_failures();
    log::info!(
        "Reaction published to specific relays: {} ({}/{} relays succeeded)",
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

/// Publish a NIP-62 vanish request to specific relays only.
pub async fn publish_vanish_request_to_relays(
    relay_urls: Vec<String>,
    reason: String,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;
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

    let output = client
        .send_event_builder_to(urls, builder)
        .await
        .map_err(|e| format!("Failed to publish vanish request: {}", e))?;
    let result = PublishResult::from_output(output).ignoring_duplicate_event_failures();
    log::info!(
        "Vanish request published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );
    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed vanish request: {}", relay, error);
        }
    }
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
    let client = get_client().ok_or("Client not initialized")?;
    let urls = parse_relay_urls(&relay_urls)?
        .into_iter()
        .filter(|relay_url| !relay::is_relay_blocked(relay_url.as_str()))
        .collect::<Vec<_>>();
    if urls.is_empty() {
        return Err("No unblocked relay URLs provided".to_string());
    }
    for (relay_url, connected) in join_all(urls.iter().map(|relay_url| {
        let client = client.clone();
        async move {
            let connected = relay::ensure_connected(&client, relay_url.as_str()).await;
            (relay_url, connected)
        }
    }))
    .await
    {
        if !connected {
            log::warn!("Broadcast relay unavailable: {}", relay_url);
        }
    }
    let output = client
        .send_event_to(urls, &event)
        .await
        .map_err(|e| format!("Failed to send event: {}", e))?;
    let result = PublishResult::from_output(output).ignoring_duplicate_event_failures();
    log::info!(
        "Pre-signed event sent to specific relays: {} ({}/{} relays succeeded)",
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

/// Broadcast an existing signed event to additional relays.
pub async fn broadcast_presigned_event(
    event: nostr::Event,
    relay_urls: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    send_presigned_event_to_relays(event, relay_urls).await
}
