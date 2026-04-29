//! Polls (kind 1068)
//!
//! Functions for creating and voting on polls (NIP-88).
use super::signals::HAS_SIGNER;
use super::types::PublishResult;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
/// Get user's public key from cache (no signer call needed)
///
/// This is much faster than calling signer().get_public_key() especially for:
/// - NIP-46 remote signers (avoids network roundtrip)
/// - Browser extensions (avoids extension API call)
///
/// Use this when you just need the pubkey, not for signing operations.
pub fn get_cached_pubkey() -> std::result::Result<PublicKey, String> {
    let pubkey_str = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid cached pubkey: {}", e))
}
/// Publish a poll vote (Kind 1018) with relay feedback
/// NIP-88: https://github.com/nostr-protocol/nips/blob/master/88.md
/// Votes are published to the relays specified in the poll event
pub async fn publish_poll_vote_tracked(
    poll_id: nostr::EventId,
    response: nostr::nips::nip88::PollResponse,
    poll_relays: Vec<nostr::RelayUrl>,
) -> std::result::Result<PublishResult, String> {
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let referenced_poll_id = match &response {
        nostr::nips::nip88::PollResponse::SingleChoice {
            poll_id: ref_id, ..
        } => ref_id,
        nostr::nips::nip88::PollResponse::MultipleChoice {
            poll_id: ref_id, ..
        } => ref_id,
    };
    if *referenced_poll_id != poll_id {
        return Err(format!(
            "Poll ID mismatch: expected {}, but PollResponse references {}",
            poll_id.to_hex(),
            referenced_poll_id.to_hex(),
        ));
    }
    log::info!("Publishing poll vote for poll: {}", poll_id.to_hex());
    let builder = nostr::EventBuilder::poll_response(response);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign poll vote: {}", e))?;
    let event_id = event.id.to_hex();
    let target_relays: Option<Vec<String>> = if !poll_relays.is_empty() {
        Some(poll_relays.iter().map(|r| r.to_string()).collect())
    } else {
        None
    };
    let queue_id = crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Poll,
        target_relays,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Poll vote enqueued: {} (queue: {})", event_id, queue_id);
    Ok(PublishResult::queued(queue_id, event_id))
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
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
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
    let relay_urls: Vec<nostr::RelayUrl> = relays
        .into_iter()
        .filter_map(|r| match nostr::RelayUrl::parse(&r) {
            Ok(url) => Some(url),
            Err(e) => {
                log::warn!("Invalid relay URL skipped: {} ({})", r, e);
                None
            }
        })
        .collect();
    let poll = nostr::nips::nip88::Poll {
        title: title.clone(),
        r#type: poll_type,
        options,
        relays: relay_urls.clone(),
        ends_at,
    };
    use nostr::Tag;
    let hashtag_tags: Vec<Tag> = hashtags.into_iter().map(Tag::hashtag).collect();
    let builder = nostr::EventBuilder::poll(poll).tags(hashtag_tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign poll: {}", e))?;
    let event_id = event.id.to_hex();
    let target_relays: Option<Vec<String>> = if !relay_urls.is_empty() {
        Some(relay_urls.iter().map(|r| r.to_string()).collect())
    } else {
        None
    };
    let queue_id = crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Poll,
        target_relays,
        std::collections::HashMap::new(),
    ).await;
    log::info!(
        "Poll '{}' enqueued: {} (queue: {})",
        title,
        event_id,
        queue_id
    );
    Ok(PublishResult::queued(queue_id, event_id))
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
