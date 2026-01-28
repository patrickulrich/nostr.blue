//! Reposts (kind 6)
//!
//! Publishing functions for reposts (NIP-18).

use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

use super::fetching::{fetch_events_from_relays, get_client};
use super::signals::HAS_SIGNER;
use super::types::PublishResult;

// =============================================================================
// Repost Publishing
// =============================================================================

/// Publish a repost (kind 6 event) with relay feedback
/// NIP-18: https://github.com/nostr-protocol/nips/blob/master/18.md
pub async fn publish_repost_tracked(
    event_id: String,
    relay_url: Option<String>,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing repost of event: {}", event_id);

    // nostr-sdk pattern: EventId::parse() handles all formats
    // (hex, note1..., nostr:note1..., nevent1...)
    use nostr::{EventId, RelayUrl};
    let target_event_id = EventId::parse(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;

    // Fetch the original event from database to get full event data
    // This is required for EventBuilder::repost() to serialize the event properly
    // Fallback to relay fetch if not in local DB (uses fetch_events_from_relays for relay readiness)
    let event = match client.database().event_by_id(&target_event_id).await {
        Ok(Some(ev)) => ev,
        Ok(None) | Err(_) => {
            // Fallback: fetch from relays using helper that handles relay readiness
            log::debug!("Event {} not in local DB, fetching from relays", event_id);
            let filter = nostr::Filter::new().id(target_event_id);
            let events = fetch_events_from_relays(filter, std::time::Duration::from_secs(5))
                .await
                .map_err(|e| format!("Failed to fetch event {} from relays: {}", event_id, e))?;
            events.into_iter().next()
                .ok_or_else(|| format!("Event not found locally or on relays: {}", event_id))?
        }
    };

    // nostr-sdk pattern: Propagate relay URL errors explicitly (no silent .ok())
    let relay = match relay_url {
        Some(url) => Some(RelayUrl::parse(&url)
            .map_err(|e| format!("Invalid relay URL '{}': {}", url, e))?),
        None => None,
    };

    // Use EventBuilder::repost() for proper NIP-18 compliance
    // This automatically:
    // - Serializes the event JSON into content field
    // - Adds 'e' tag with relay hint
    // - Adds 'p' tag for event author
    // - Uses Kind 6 for text notes, Kind 16 (generic repost) for others
    let builder = nostr::EventBuilder::repost(&event, relay);

    // Publish using gossip - automatic relay routing
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish repost: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Repost published: {} ({}/{} relays succeeded)",
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

/// Publish a repost (kind 6 event) of another event
/// For relay feedback, use publish_repost_tracked instead
pub async fn publish_repost(
    event_id: String,
    relay_url: Option<String>,
) -> std::result::Result<String, String> {
    publish_repost_tracked(event_id, relay_url)
        .await
        .map(|result| result.event_id)
}

// =============================================================================
// Repost Deletion
// =============================================================================

/// Delete a repost event (Kind 6) using NIP-9 Event Deletion
pub async fn delete_repost(repost_event_id: String) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot delete events.".to_string());
    }

    log::info!("Deleting repost: {}", repost_event_id);

    let event_id = nostr::EventId::from_hex(&repost_event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;

    // Create deletion event (kind 5) using NIP-9
    // Include k-tag per NIP-9 recommendation for better relay interoperability
    use nostr::nips::nip09::EventDeletionRequest;
    let request = EventDeletionRequest::new().id(event_id);
    let builder = nostr::EventBuilder::delete(request)
        .tag(nostr::Tag::custom(nostr::TagKind::k(), vec![nostr::Kind::Repost.as_u16().to_string()]));

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish deletion: {}", e))?;

    log::info!("Repost deleted successfully");
    Ok(())
}
