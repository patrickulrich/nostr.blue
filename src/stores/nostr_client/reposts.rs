//! Reposts (kind 6)
//!
//! Publishing functions for reposts (NIP-18).
use super::fetching::{fetch_events_from_relays, get_client};
use super::signals::HAS_SIGNER;
use super::types::PublishResult;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
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
    use nostr::{EventId, RelayUrl};
    let target_event_id =
        EventId::parse(&event_id).map_err(|e| format!("Invalid event ID: {}", e))?;
    let event = match client.database().event_by_id(&target_event_id).await {
        Ok(Some(ev)) => ev,
        Ok(None) | Err(_) => {
            log::debug!("Event {} not in local DB, fetching from relays", event_id);
            let filter = nostr::Filter::new().id(target_event_id);
            let events = fetch_events_from_relays(filter, std::time::Duration::from_secs(5))
                .await
                .map_err(|e| format!("Failed to fetch event {} from relays: {}", event_id, e))?;
            events
                .into_iter()
                .next()
                .ok_or_else(|| format!("Event not found locally or on relays: {}", event_id))?
        }
    };
    let write_relay_urls = client.pool().__write_relay_urls().await;
    let relay = relay_url
        .as_deref()
        .and_then(|u| RelayUrl::parse(u).ok())
        .or_else(|| write_relay_urls.first().cloned());
    if relay.is_none() {
        log::warn!(
            "No relay hint available for repost of {}",
            event.id.to_hex()
        );
    }
    let builder = nostr::EventBuilder::repost(&event, relay);
    let signed_event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign repost: {}", e))?;
    let event_id = signed_event.id.to_hex();
    let write_relays: Vec<String> = write_relay_urls
        .into_iter()
        .map(|u| u.to_string())
        .collect();
    let queue_id = crate::stores::publish_queue::enqueue(
        signed_event,
        crate::stores::publish_queue::types::QueueEventType::Repost,
        Some(write_relays),
        std::collections::HashMap::new(),
    ).await;
    let result = PublishResult::queued(queue_id, event_id);
    log::info!("Repost queued: {}", result.event_id);
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
/// Delete a repost event (Kind 6) using NIP-9 Event Deletion
pub async fn delete_repost(repost_event_id: String) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot delete events.".to_string());
    }
    log::info!("Deleting repost: {}", repost_event_id);
    let event_id =
        nostr::EventId::parse(&repost_event_id).map_err(|e| format!("Invalid event ID: {}", e))?;
    use nostr::nips::nip09::EventDeletionRequest;
    let request = EventDeletionRequest::new().id(event_id);
    let builder = nostr::EventBuilder::delete(request).tag(nostr::Tag::custom(
        nostr::TagKind::k(),
        vec![nostr::Kind::Repost.as_u16().to_string()],
    ));
    let signed_event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign deletion: {}", e))?;
    let write_relays: Vec<String> = client
        .pool()
        .__write_relay_urls()
        .await
        .into_iter()
        .map(|u| u.to_string())
        .collect();
    crate::stores::publish_queue::enqueue(
        signed_event,
        crate::stores::publish_queue::types::QueueEventType::Repost,
        Some(write_relays),
        std::collections::HashMap::new(),
    ).await;
    log::info!("Repost deletion queued");
    Ok(())
}
