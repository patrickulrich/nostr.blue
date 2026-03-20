//! Discussions Service
//!
//! Handles fetching and publishing Kind 9805 discussion events for git repositories.
#![allow(dead_code)]
use crate::stores::code_store::{cache_discussion_events, get_cached_discussion};
use crate::stores::nostr_client::{fetch_events_aggregated, get_client, HAS_SIGNER};
use crate::utils::nip34::{decode_event_id, Discussion, GitComment};
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::borrow::Cow;
use std::time::Duration;
/// Default timeout for fetching events
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// Fetch a discussion by its event ID (hex, note1, or nevent1)
pub async fn fetch_discussion(event_ref: &str) -> Result<Discussion, String> {
    let event_id =
        EventId::parse(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
    let hex_id = event_id.to_hex();
    if let Some(discussion) = get_cached_discussion(&hex_id) {
        return Ok(discussion);
    }
    let filter = Filter::new()
        .id(event_id)
        .kind(Kind::Custom(Discussion::KIND));
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch discussion: {}", e))?;
    cache_discussion_events(&events);
    get_cached_discussion(&hex_id).ok_or_else(|| "Discussion not found".to_string())
}
/// Fetch discussions for a repository by naddr
pub async fn fetch_repo_discussions(naddr: &str) -> Result<Vec<Discussion>, String> {
    use crate::utils::nip34::decode_naddr;
    let coord = decode_naddr(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(Discussion::KIND))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), coord.to_string())
        .limit(50);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch discussions: {}", e))?;
    cache_discussion_events(&events);
    Ok(events.iter().filter_map(Discussion::from_event).collect())
}
/// Publish a new discussion
pub async fn publish_discussion(
    repository: &Coordinate,
    subject: Option<&str>,
    content: &str,
    category: Option<&str>,
    labels: &[&str],
) -> Result<EventId, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let mut builder = EventBuilder::new(Kind::Custom(Discussion::KIND), content)
        .tag(Tag::coordinate(repository.clone(), None));
    if let Some(s) = subject {
        builder = builder.tag(Tag::custom(TagKind::Custom(Cow::Borrowed("subject")), [s]));
    }
    if let Some(cat) = category {
        builder = builder.tag(Tag::hashtag(cat));
    }
    for label in labels {
        builder = builder.tag(Tag::hashtag(*label));
    }
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;
    let event_id = *output.id();
    let filter = Filter::new().id(event_id);
    if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(2)).await {
        cache_discussion_events(&events);
    }
    Ok(event_id)
}
/// Publish a comment on a discussion (NIP-22 Comment, Kind 1111)
pub async fn publish_discussion_comment(
    discussion_id: EventId,
    discussion_author: PublicKey,
    repository: Option<&Coordinate>,
    content: &str,
) -> Result<EventId, String> {
    use nostr::nips::nip22::CommentTarget;
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let comment_to = CommentTarget::event(
        discussion_id,
        Kind::Custom(Discussion::KIND),
        Some(discussion_author),
        None,
    );
    let mut builder = EventBuilder::comment(content, comment_to, None);
    if let Some(coord) = repository {
        builder = builder.tag(Tag::coordinate(coord.clone(), None));
    }
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish comment: {}", e))?;
    Ok(*output.id())
}
/// Fetch comments for a discussion (by EventId)
pub async fn fetch_discussion_comments(discussion_id: EventId) -> Result<Vec<GitComment>, String> {
    let filter = Filter::new().kind(Kind::Comment).event(discussion_id);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch comments: {}", e))?;
    Ok(events.iter().filter_map(GitComment::from_event).collect())
}
/// Publish discussion by naddr string
pub async fn publish_discussion_by_naddr(
    naddr: &str,
    subject: Option<&str>,
    content: &str,
    category: Option<&str>,
    labels: &[&str],
) -> Result<String, String> {
    use crate::utils::nip34::decode_naddr;
    let coord = decode_naddr(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    let result = publish_discussion(&coord, subject, content, category, labels).await?;
    Ok(result.to_hex())
}
/// Publish comment on discussion by event ID string
pub async fn publish_discussion_comment_by_id(
    event_ref: &str,
    author_hex: &str,
    content: &str,
) -> Result<String, String> {
    let event_id =
        decode_event_id(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
    let discussion_author =
        PublicKey::from_hex(author_hex).map_err(|e| format!("Invalid author pubkey: {}", e))?;
    let result = publish_discussion_comment(event_id, discussion_author, None, content).await?;
    Ok(result.to_hex())
}
/// Fetch comments for discussion by event ID string
pub async fn fetch_discussion_comments_by_id(event_ref: &str) -> Result<Vec<GitComment>, String> {
    let event_id =
        decode_event_id(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
    fetch_discussion_comments(event_id).await
}
