//! Polls (kind 1068)
//!
//! Functions for creating and voting on polls (NIP-88).

use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

use crate::stores::relay;
use super::fetching::{get_client, ensure_relays_ready};
use super::signals::HAS_SIGNER;
use super::types::PublishResult;

// =============================================================================
// Pubkey Helpers
// =============================================================================

/// Get user's public key from cache (no signer call needed)
///
/// This is much faster than calling signer().get_public_key() especially for:
/// - NIP-46 remote signers (avoids network roundtrip)
/// - Browser extensions (avoids extension API call)
///
/// Use this when you just need the pubkey, not for signing operations.
pub fn get_cached_pubkey() -> std::result::Result<PublicKey, String> {
    let pubkey_str = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;
    PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid cached pubkey: {}", e))
}

/// Get the current user's public key (uses cache, no signer call)
/// Note: This is a sync wrapper for API consistency; prefer get_cached_pubkey() directly
pub fn get_user_pubkey() -> std::result::Result<PublicKey, String> {
    get_cached_pubkey()
}

// =============================================================================
// Poll Voting
// =============================================================================

/// Publish a poll vote (Kind 1018) with relay feedback
/// NIP-88: https://github.com/nostr-protocol/nips/blob/master/88.md
/// Votes are published to the relays specified in the poll event
pub async fn publish_poll_vote_tracked(
    poll_id: nostr::EventId,
    response: nostr::nips::nip88::PollResponse,
    poll_relays: Vec<nostr::RelayUrl>,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    // Validate that the poll_id matches the poll referenced in the PollResponse
    let referenced_poll_id = match &response {
        nostr::nips::nip88::PollResponse::SingleChoice { poll_id: ref_id, .. } => ref_id,
        nostr::nips::nip88::PollResponse::MultipleChoice { poll_id: ref_id, .. } => ref_id,
    };

    if *referenced_poll_id != poll_id {
        return Err(format!(
            "Poll ID mismatch: expected {}, but PollResponse references {}",
            poll_id.to_hex(),
            referenced_poll_id.to_hex()
        ));
    }

    log::info!("Publishing poll vote for poll: {}", poll_id.to_hex());

    // Build event using EventBuilder::poll_response
    let builder = nostr::EventBuilder::poll_response(response);

    // NIP-88: Votes should be published to the relays specified in the poll
    let output = if !poll_relays.is_empty() {
        // Add poll relays temporarily using specialty helpers
        let added_relays = relay::add_relays(&client, &poll_relays).await;

        // Use non-blocking relay ready check instead of blocking connect()
        ensure_relays_ready(&client).await;

        // Check if any poll relays are actually connected
        let connected_poll_relays = relay::get_connected(&client, &poll_relays).await;

        if connected_poll_relays.is_empty() {
            log::warn!("None of the {} poll relays are connected, falling back to default relays", poll_relays.len());
        } else {
            log::debug!("{}/{} poll relays connected", connected_poll_relays.len(), poll_relays.len());
        }

        // Build from connected relays, not all poll relays (use only those actually connected)
        let relay_urls: Vec<nostr::Url> = if !connected_poll_relays.is_empty() {
            connected_poll_relays.iter()
                .filter_map(|r| nostr::Url::parse(r.as_str()).ok())
                .collect()
        } else {
            // Fallback already logged above
            vec![]
        };

        let result = if !relay_urls.is_empty() {
            log::info!("Publishing vote to {} connected poll relays", relay_urls.len());
            client.send_event_builder_to(relay_urls, builder).await
                .map_err(|e| format!("Failed to publish poll vote to poll relays: {}", e))
        } else {
            // No connected poll relays - use default relays
            client.send_event_builder(builder).await
                .map_err(|e| format!("Failed to publish poll vote: {}", e))
        };

        // Cleanup: remove only the relays we added
        relay::remove_relays(&client, &added_relays).await;

        result?
    } else {
        // No poll relays specified, use default relays
        client.send_event_builder(builder).await
            .map_err(|e| format!("Failed to publish poll vote: {}", e))?
    };

    let result = PublishResult::from_output(output);

    log::info!(
        "Poll vote published: {} ({}/{} relays succeeded)",
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

/// Publish a poll vote (Kind 1018) following NIP-88
/// For relay feedback, use publish_poll_vote_tracked instead
pub async fn publish_poll_vote(
    poll_id: nostr::EventId,
    response: nostr::nips::nip88::PollResponse,
    poll_relays: Vec<nostr::RelayUrl>,
) -> std::result::Result<String, String> {
    publish_poll_vote_tracked(poll_id, response, poll_relays)
        .await
        .map(|result| result.event_id)
}

// =============================================================================
// Poll Creation
// =============================================================================

/// Publish a poll (Kind 1068) with relay feedback
/// NIP-88: https://github.com/nostr-protocol/nips/blob/master/88.md
pub async fn publish_poll_tracked(
    title: String,
    poll_type: nostr::nips::nip88::PollType,
    options: Vec<nostr::nips::nip88::PollOption>,
    relays: Vec<String>,
    ends_at: Option<nostr::Timestamp>,
    hashtags: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    // Validate inputs
    if title.trim().is_empty() {
        return Err("Poll title cannot be empty".to_string());
    }

    if options.len() < 2 {
        return Err("Poll must have at least 2 options".to_string());
    }

    if options.len() > 10 {
        return Err("Poll cannot have more than 10 options".to_string());
    }

    log::info!("Publishing poll: {}", title);

    // Parse relay URLs
    let relay_urls: Vec<nostr::RelayUrl> = relays
        .into_iter()
        .filter_map(|r| nostr::RelayUrl::parse(&r).ok())
        .collect();

    // Build poll struct (clone relay_urls since we need it for publishing too)
    let poll = nostr::nips::nip88::Poll {
        title: title.clone(),
        r#type: poll_type,
        options,
        relays: relay_urls.clone(),
        ends_at,
    };

    // Build event using EventBuilder::poll
    // Batch hashtag tags for efficiency (nostr-sdk pattern)
    use nostr::Tag;
    let hashtag_tags: Vec<Tag> = hashtags.into_iter().map(Tag::hashtag).collect();
    let builder = nostr::EventBuilder::poll(poll).tags(hashtag_tags);

    // NIP-88: Polls specify relays where votes should be published
    // We should also publish the poll to those relays for discoverability
    let output = if !relay_urls.is_empty() {
        // Add poll relays temporarily (mirrors publish_poll_vote_tracked pattern)
        let added_relays = relay::add_relays(&client, &relay_urls).await;

        // Use non-blocking relay ready check
        ensure_relays_ready(&client).await;

        // Check which poll relays are connected
        let connected_poll_relays = relay::get_connected(&client, &relay_urls).await;

        if connected_poll_relays.is_empty() {
            log::warn!("None of the {} poll relays are connected, falling back to default relays", relay_urls.len());
        } else {
            log::debug!("{}/{} poll relays connected", connected_poll_relays.len(), relay_urls.len());
        }

        // Publish to connected poll relays if available
        let result = if !connected_poll_relays.is_empty() {
            let urls: Vec<nostr::Url> = connected_poll_relays.iter()
                .filter_map(|r| nostr::Url::parse(r.as_str()).ok())
                .collect();
            log::info!("Publishing poll to {} connected poll relays", urls.len());
            client.send_event_builder_to(urls, builder).await
                .map_err(|e| format!("Failed to publish poll to specified relays: {}", e))
        } else {
            // No connected poll relays - use default relays
            client.send_event_builder(builder).await
                .map_err(|e| format!("Failed to publish poll: {}", e))
        };

        // Cleanup: remove only the relays we added
        relay::remove_relays(&client, &added_relays).await;

        result?
    } else {
        // No poll relays specified, use default relays
        client.send_event_builder(builder).await
            .map_err(|e| format!("Failed to publish poll: {}", e))?
    };

    let result = PublishResult::from_output(output);

    log::info!(
        "Poll '{}' published: {} ({}/{} relays succeeded)",
        title,
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

/// Publish a poll (Kind 1068) following NIP-88
/// For relay feedback, use publish_poll_tracked instead
pub async fn publish_poll(
    title: String,
    poll_type: nostr::nips::nip88::PollType,
    options: Vec<nostr::nips::nip88::PollOption>,
    relays: Vec<String>,
    ends_at: Option<nostr::Timestamp>,
    hashtags: Vec<String>,
) -> std::result::Result<String, String> {
    publish_poll_tracked(title, poll_type, options, relays, ends_at, hashtags)
        .await
        .map(|result| result.event_id)
}
