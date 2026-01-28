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
// Relay URL Parsing Helper
// =============================================================================

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

    // Log invalid URLs
    for (url, _) in &invalid_urls {
        log::warn!("Invalid relay URL skipped: {}", url);
    }

    // Deduplicate relay URLs to avoid double-publishing
    let mut seen = HashSet::new();
    let urls: Vec<nostr::RelayUrl> = valid_urls
        .into_iter()
        .filter_map(|(_, r)| r.ok())
        .filter(|url| seen.insert(url.to_string()))  // Only keep first occurrence
        .collect();

    if urls.is_empty() {
        return Err("No valid relay URLs provided".to_string());
    }

    Ok(urls)
}

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

    // Convert raw tags to nostr::Tag format preserving relay hints and markers
    let nostr_tags = super::types::convert_raw_tags(tags);

    let builder = nostr::EventBuilder::text_note(&content)
        .tags(nostr_tags);

    let urls = parse_relay_urls(&relay_urls)?;

    let output = client.send_event_builder_to(urls, builder)
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
    // Use PublicKey::parse() which handles hex, bech32 (npub), and NIP-21 URIs (nostr-sdk pattern)
    let target_pubkey = PublicKey::parse(&event_pubkey)
        .map_err(|e| format!("Invalid pubkey (expected hex or npub): {}", e))?;

    // Create reaction target
    let target = ReactionTarget {
        event_id: target_event_id,
        public_key: target_pubkey,
        coordinate: None,
        kind: None,
        relay_hint: None,
    };

    let builder = EventBuilder::reaction(target, reaction);

    let urls = parse_relay_urls(&relay_urls)?;

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

    let urls = parse_relay_urls(&relay_urls)?;

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

    // Log per-relay failures for debugging (matching publish_note_to_relays pattern)
    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}
