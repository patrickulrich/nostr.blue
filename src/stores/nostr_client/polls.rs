//! Polls (kind 1068)
//!
//! Functions for creating and voting on polls (NIP-88).
use super::fetching::{ensure_relays_ready, get_client};
use super::signals::HAS_SIGNER;
use super::types::PublishResult;
use crate::stores::relay;
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
    let client = get_client().ok_or("Client not initialized")?;
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
    let output = if !poll_relays.is_empty() {
        let added_relays = relay::add_relays(&client, &poll_relays).await;
        ensure_relays_ready(&client).await;
        let connected_poll_relays = relay::get_connected(&client, &poll_relays).await;
        if connected_poll_relays.is_empty() {
            log::warn!(
                "None of the {} poll relays are connected, falling back to default relays",
                poll_relays.len()
            );
        } else {
            log::debug!(
                "{}/{} poll relays connected",
                connected_poll_relays.len(),
                poll_relays.len()
            );
        }
        let relay_urls: Vec<nostr::Url> = if !connected_poll_relays.is_empty() {
            connected_poll_relays
                .iter()
                .filter_map(|r| nostr::Url::parse(r.as_str()).ok())
                .collect()
        } else {
            vec![]
        };
        let result = if !relay_urls.is_empty() {
            log::info!(
                "Publishing vote to {} connected poll relays",
                relay_urls.len()
            );
            client
                .send_event_builder_to(
                    relay_urls,
                    crate::utils::nips::nip89::tag_event_builder(builder),
                )
                .await
                .map_err(|e| format!("Failed to publish poll vote to poll relays: {}", e))
        } else {
            client
                .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
                .await
                .map_err(|e| format!("Failed to publish poll vote: {}", e))
        };
        relay::remove_relays(&client, &added_relays).await;
        result?
    } else {
        client
            .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
            .await
            .map_err(|e| format!("Failed to publish poll vote: {}", e))?
    };
    if output.success.is_empty() {
        return Err("No relays accepted event".to_string());
    }
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
    let output = if !relay_urls.is_empty() {
        let added_relays = relay::add_relays(&client, &relay_urls).await;
        ensure_relays_ready(&client).await;
        let connected_poll_relays = relay::get_connected(&client, &relay_urls).await;
        if connected_poll_relays.is_empty() {
            log::warn!(
                "None of the {} poll relays are connected, falling back to default relays",
                relay_urls.len()
            );
        } else {
            log::debug!(
                "{}/{} poll relays connected",
                connected_poll_relays.len(),
                relay_urls.len()
            );
        }
        let result = if !connected_poll_relays.is_empty() {
            let urls: Vec<nostr::Url> = connected_poll_relays
                .iter()
                .filter_map(|r| nostr::Url::parse(r.as_str()).ok())
                .collect();
            log::info!("Publishing poll to {} connected poll relays", urls.len());
            client
                .send_event_builder_to(urls, crate::utils::nips::nip89::tag_event_builder(builder))
                .await
                .map_err(|e| format!("Failed to publish poll to specified relays: {}", e))
        } else {
            client
                .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
                .await
                .map_err(|e| format!("Failed to publish poll: {}", e))
        };
        relay::remove_relays(&client, &added_relays).await;
        result?
    } else {
        client
            .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
            .await
            .map_err(|e| format!("Failed to publish poll: {}", e))?
    };
    if output.success.is_empty() {
        return Err("No relays accepted event".to_string());
    }
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
