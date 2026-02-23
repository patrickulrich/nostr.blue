//! Releases Service
//!
//! Handles fetching and publishing Kind 30618 release events for git repositories.
#![allow(dead_code)]
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::borrow::Cow;
use std::time::Duration;
use crate::stores::code_store::{cache_release_events, get_cached_release};
use crate::stores::nostr_client::{fetch_events_aggregated, get_client, HAS_SIGNER};
use crate::utils::nip34::{decode_event_id, Release};
/// Default timeout for fetching events
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// Fetch releases for a repository by naddr
pub async fn fetch_repo_releases(naddr: &str) -> Result<Vec<Release>, String> {
    use crate::utils::nip34::decode_naddr;
    let coord = decode_naddr(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(Release::KIND))
        .author(coord.public_key)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), coord.to_string())
        .limit(50);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch releases: {}", e))?;
    cache_release_events(&events);
    Ok(events.iter().filter_map(Release::from_event).collect())
}
/// Publish a new release
pub async fn publish_release(
    repository: &Coordinate,
    tag_name: &str,
    title: Option<&str>,
    description: &str,
    assets: &[&str],
    prerelease: bool,
) -> Result<EventId, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let mut builder = EventBuilder::new(Kind::Custom(Release::KIND), description)
        .tag(Tag::identifier(tag_name))
        .tag(Tag::coordinate(repository.clone(), None));
    if let Some(t) = title {
        builder = builder.tag(Tag::custom(
            TagKind::Custom(Cow::Borrowed("name")),
            [t],
        ));
    }
    for asset_url in assets {
        builder = builder.tag(Tag::custom(
            TagKind::Custom(Cow::Borrowed("url")),
            [*asset_url],
        ));
    }
    if prerelease {
        builder = builder.tag(Tag::custom(
            TagKind::Custom(Cow::Borrowed("prerelease")),
            ["true"],
        ));
    }
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;
    let event_id = *output.id();
    let filter = Filter::new().id(event_id);
    if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(2)).await {
        cache_release_events(&events);
    }
    Ok(event_id)
}
/// Delete a release by publishing a Kind 5 deletion event
pub async fn delete_release(release_event_id: EventId) -> Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    use nostr::nips::nip09::EventDeletionRequest;
    let request = EventDeletionRequest::new().id(release_event_id).reason("Release deleted");
    let builder = EventBuilder::delete(request);
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to delete: {}", e))?;
    Ok(())
}
/// Fetch a release by its event ID (note1 or nevent1)
pub async fn fetch_release(event_ref: &str) -> Result<Release, String> {
    let event_id = decode_event_id(event_ref)
        .map_err(|e| format!("Invalid event reference: {}", e))?;
    let hex_id = event_id.to_hex();
    if let Some(release) = get_cached_release(&hex_id) {
        return Ok(release);
    }
    let filter = Filter::new().id(event_id).kind(Kind::Custom(Release::KIND));
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch release: {}", e))?;
    cache_release_events(&events);
    get_cached_release(&hex_id)
        .ok_or_else(|| "Release not found".to_string())
}
