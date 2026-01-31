//! Pull Requests Service
//!
//! Handles fetching and publishing NIP-34 Git patch events (Kind 1617).
#![allow(dead_code)]
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;
use crate::stores::code_store::{cache_pr_events, get_cached_pr, update_pr_statuses};
use crate::stores::nostr_client::{fetch_events_aggregated, get_client, HAS_SIGNER};
use crate::utils::nip34::{decode_event_id, GitComment, IssueStatus, PullRequest};
/// Default timeout for fetching events
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// Fetch a pull request by its event ID (note1 or nevent1)
pub async fn fetch_pull_request(event_ref: &str) -> Result<PullRequest, String> {
    if let Some(pr) = get_cached_pr(event_ref) {
        return Ok(pr);
    }
    let event_id = decode_event_id(event_ref)
        .map_err(|e| format!("Invalid event reference: {}", e))?;
    let filter = Filter::new().id(event_id).kind(Kind::GitPatch);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch pull request: {}", e))?;
    cache_pr_events(&events);
    let status_filter = Filter::new()
        .kinds(
            vec![
                Kind::GitStatusOpen,
                Kind::GitStatusApplied,
                Kind::GitStatusClosed,
                Kind::GitStatusDraft,
            ],
        )
        .event(event_id);
    if let Ok(status_events) = fetch_events_aggregated(status_filter, FETCH_TIMEOUT)
        .await
    {
        update_pr_statuses(&status_events);
    }
    get_cached_pr(&event_id.to_hex()).ok_or_else(|| "Pull request not found".to_string())
}
/// Fetch pull requests for a repository by naddr
pub async fn fetch_repo_prs(naddr: &str) -> Result<Vec<PullRequest>, String> {
    use crate::utils::nip34::decode_naddr;
    let coord = decode_naddr(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    fetch_repository_prs(&coord, 50).await
}
/// Fetch pull requests for a repository coordinate
pub async fn fetch_repository_prs(
    coordinate: &Coordinate,
    limit: usize,
) -> Result<Vec<PullRequest>, String> {
    let filter = Filter::new()
        .kind(Kind::GitPatch)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), coordinate.to_string())
        .limit(limit);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch pull requests: {}", e))?;
    cache_pr_events(&events);
    let event_ids: Vec<EventId> = events.iter().map(|e| e.id).collect();
    if !event_ids.is_empty() {
        let status_filter = Filter::new()
            .kinds(
                vec![
                    Kind::GitStatusOpen,
                    Kind::GitStatusApplied,
                    Kind::GitStatusClosed,
                    Kind::GitStatusDraft,
                ],
            )
            .events(event_ids);
        if let Ok(status_events) = fetch_events_aggregated(status_filter, FETCH_TIMEOUT)
            .await
        {
            update_pr_statuses(&status_events);
        }
    }
    Ok(events.iter().filter_map(PullRequest::from_event).collect())
}
/// Search PRs by text (NIP-50)
pub async fn search_prs(query: &str, limit: usize) -> Result<Vec<PullRequest>, String> {
    let filter = Filter::new().kind(Kind::GitPatch).search(query).limit(limit);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to search pull requests: {}", e))?;
    cache_pr_events(&events);
    Ok(events.iter().filter_map(PullRequest::from_event).collect())
}
/// Fetch pull requests by author
pub async fn fetch_user_prs(
    pubkey: &PublicKey,
    limit: usize,
) -> Result<Vec<PullRequest>, String> {
    let filter = Filter::new().kind(Kind::GitPatch).author(*pubkey).limit(limit);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch pull requests: {}", e))?;
    cache_pr_events(&events);
    Ok(events.iter().filter_map(PullRequest::from_event).collect())
}
/// Publish a new patch/pull request
pub async fn publish_patch(
    repository: &Coordinate,
    content: &str,
    commit: Option<&str>,
    parent_commit: Option<&str>,
    is_cover_letter: bool,
    labels: &[&str],
) -> Result<EventId, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let mut builder = EventBuilder::new(Kind::GitPatch, content)
        .tag(Tag::coordinate(repository.clone(), None))
        .tag(Tag::public_key(repository.public_key));
    if let Some(hash) = commit {
        builder = builder
            .tag(
                Tag::custom(
                    TagKind::Custom(std::borrow::Cow::Borrowed("commit")),
                    [hash],
                ),
            );
    }
    if let Some(hash) = parent_commit {
        builder = builder
            .tag(
                Tag::custom(
                    TagKind::Custom(std::borrow::Cow::Borrowed("parent-commit")),
                    [hash],
                ),
            );
    }
    if is_cover_letter {
        builder = builder.tag(Tag::hashtag("cover-letter"));
    }
    for label in labels {
        builder = builder.tag(Tag::hashtag(*label));
    }
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;
    let event_id = *output.id();
    let filter = Filter::new().id(event_id);
    if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(2)).await {
        cache_pr_events(&events);
    }
    Ok(event_id)
}
/// Update PR status (Kind 1630-1633)
pub async fn update_pr_status(
    pr_id: EventId,
    status: IssueStatus,
) -> Result<EventId, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let kind = status.to_kind();
    let builder = EventBuilder::new(kind, "").tag(Tag::event(pr_id));
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish status: {}", e))?;
    let event_id = *output.id();
    let filter = Filter::new().id(event_id);
    if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(2)).await {
        update_pr_statuses(&events);
    }
    Ok(event_id)
}
/// Publish a comment on a PR (NIP-22 Comment, Kind 1111)
pub async fn publish_pr_comment(
    pr_id: EventId,
    pr_author: PublicKey,
    repository: Option<&Coordinate>,
    content: &str,
) -> Result<EventId, String> {
    use nostr::nips::nip22::CommentTarget;
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let comment_to = CommentTarget::event(pr_id, Kind::GitPatch, Some(pr_author), None);
    let mut builder = EventBuilder::comment(content, comment_to, None);
    if let Some(coord) = repository {
        builder = builder.tag(Tag::coordinate(coord.clone(), None));
    }
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish comment: {}", e))?;
    Ok(*output.id())
}
/// Fetch comments for a PR
pub async fn fetch_pr_comments(pr_id: EventId) -> Result<Vec<GitComment>, String> {
    let filter = Filter::new().kind(Kind::Comment).event(pr_id);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch comments: {}", e))?;
    Ok(events.iter().filter_map(GitComment::from_event).collect())
}
/// Publish patch/PR by naddr string
pub async fn publish_patch_by_naddr(
    naddr: &str,
    content: &str,
    commit: Option<&str>,
    parent_commit: Option<&str>,
    is_cover_letter: bool,
    labels: &[&str],
) -> Result<String, String> {
    use crate::utils::nip34::decode_naddr;
    let coord = decode_naddr(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    let result = publish_patch(
            &coord,
            content,
            commit,
            parent_commit,
            is_cover_letter,
            labels,
        )
        .await?;
    Ok(result.to_hex())
}
/// Update PR status by event ID string
pub async fn update_pr_status_by_id(
    event_ref: &str,
    status: IssueStatus,
) -> Result<String, String> {
    let event_id = decode_event_id(event_ref)
        .map_err(|e| format!("Invalid event reference: {}", e))?;
    let result = update_pr_status(event_id, status).await?;
    Ok(result.to_hex())
}
/// Publish comment on PR by event ID string
pub async fn publish_pr_comment_by_id(
    event_ref: &str,
    author_hex: &str,
    content: &str,
) -> Result<String, String> {
    use nostr_sdk::prelude::PublicKey;
    let event_id = decode_event_id(event_ref)
        .map_err(|e| format!("Invalid event reference: {}", e))?;
    let author = PublicKey::from_hex(author_hex)
        .map_err(|e| format!("Invalid author pubkey: {}", e))?;
    let result = publish_pr_comment(event_id, author, None, content).await?;
    Ok(result.to_hex())
}
/// Fetch comments for PR by event ID string
pub async fn fetch_pr_comments_by_id(
    event_ref: &str,
) -> Result<Vec<GitComment>, String> {
    let event_id = decode_event_id(event_ref)
        .map_err(|e| format!("Invalid event reference: {}", e))?;
    fetch_pr_comments(event_id).await
}
