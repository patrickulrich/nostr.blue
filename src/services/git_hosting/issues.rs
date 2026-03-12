//! Issues Service
//!
//! Handles fetching and publishing NIP-34 Git issue events (Kind 1621).
#![allow(dead_code)]
use crate::stores::code_store::{cache_issue_events, get_cached_issue, update_issue_statuses};
use crate::stores::nostr_client::{fetch_events_aggregated, get_client, HAS_SIGNER};
use crate::utils::nip34::{decode_event_id, GitComment, Issue, IssueStatus};
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;
/// Default timeout for fetching events
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// Fetch an issue by its event ID (note1 or nevent1)
pub async fn fetch_issue(event_ref: &str) -> Result<Issue, String> {
    if let Some(issue) = get_cached_issue(event_ref) {
        return Ok(issue);
    }
    let event_id =
        decode_event_id(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
    let filter = Filter::new().id(event_id).kind(Kind::GitIssue);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch issue: {}", e))?;
    cache_issue_events(&events);
    let status_filter = Filter::new()
        .kinds(vec![
            Kind::GitStatusOpen,
            Kind::GitStatusApplied,
            Kind::GitStatusClosed,
            Kind::GitStatusDraft,
        ])
        .event(event_id);
    if let Ok(status_events) = fetch_events_aggregated(status_filter, FETCH_TIMEOUT).await {
        update_issue_statuses(&status_events);
    }
    get_cached_issue(&event_id.to_hex()).ok_or_else(|| "Issue not found".to_string())
}
/// Fetch issues for a repository by naddr
pub async fn fetch_repo_issues(naddr: &str) -> Result<Vec<Issue>, String> {
    use crate::utils::nip34::decode_naddr;
    let coord = decode_naddr(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    fetch_repository_issues(&coord, 50).await
}
/// Fetch issues for a repository coordinate
pub async fn fetch_repository_issues(
    coordinate: &Coordinate,
    limit: usize,
) -> Result<Vec<Issue>, String> {
    let filter = Filter::new()
        .kind(Kind::GitIssue)
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::A),
            coordinate.to_string(),
        )
        .limit(limit);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch issues: {}", e))?;
    cache_issue_events(&events);
    let event_ids: Vec<EventId> = events.iter().map(|e| e.id).collect();
    if !event_ids.is_empty() {
        let status_filter = Filter::new()
            .kinds(vec![
                Kind::GitStatusOpen,
                Kind::GitStatusApplied,
                Kind::GitStatusClosed,
                Kind::GitStatusDraft,
            ])
            .events(event_ids);
        if let Ok(status_events) = fetch_events_aggregated(status_filter, FETCH_TIMEOUT).await {
            update_issue_statuses(&status_events);
        }
    }
    Ok(events.iter().filter_map(Issue::from_event).collect())
}
/// Fetch issues by author
pub async fn fetch_user_issues(pubkey: &PublicKey, limit: usize) -> Result<Vec<Issue>, String> {
    let filter = Filter::new()
        .kind(Kind::GitIssue)
        .author(*pubkey)
        .limit(limit);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch issues: {}", e))?;
    cache_issue_events(&events);
    Ok(events.iter().filter_map(Issue::from_event).collect())
}
/// Fetch issues assigned to a user (tagged with #p)
pub async fn fetch_issues_assigned_to(
    pubkey: &PublicKey,
    limit: usize,
) -> Result<Vec<Issue>, String> {
    let filter = Filter::new()
        .kind(Kind::GitIssue)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::P), pubkey.to_hex())
        .limit(limit);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch assigned issues: {}", e))?;
    cache_issue_events(&events);
    Ok(events.iter().filter_map(Issue::from_event).collect())
}
/// Fetches issues mentioning the given pubkey.
///
/// Note: NIP-34 uses p-tags for both assignment and mentions with no protocol-level
/// distinction, so this delegates to `fetch_issues_assigned_to`.
pub async fn fetch_issues_mentioning(
    pubkey: &PublicKey,
    limit: usize,
) -> Result<Vec<Issue>, String> {
    fetch_issues_assigned_to(pubkey, limit).await
}
/// Search issues by text (NIP-50)
///
/// When `query` is `None`, no `.search()` filter is applied — the relay returns
/// all issues up to `limit`, which is useful for filter-only queries.
pub async fn search_issues(query: Option<&str>, limit: usize) -> Result<Vec<Issue>, String> {
    let mut filter = Filter::new().kind(Kind::GitIssue).limit(limit);
    if let Some(q) = query {
        filter = filter.search(q);
    }
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to search issues: {}", e))?;
    cache_issue_events(&events);
    Ok(events.iter().filter_map(Issue::from_event).collect())
}
/// Publish a new issue
pub async fn publish_issue(
    repository: &Coordinate,
    subject: Option<&str>,
    content: &str,
    labels: &[&str],
    assignees: &[&str],
) -> Result<EventId, String> {
    use nostr::nips::nip34::GitIssue;
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let issue = GitIssue {
        repository: repository.clone(),
        content: content.to_string(),
        subject: subject.map(|s| s.to_string()),
        labels: labels.iter().map(|l| l.to_string()).collect(),
    };
    let mut builder = EventBuilder::git_issue(issue)
        .map_err(|e| format!("Failed to build issue event: {}", e))?;
    // Validate all assignees upfront
    let mut invalid_assignees = Vec::new();
    let mut parsed_assignees = Vec::new();
    for assignee_hex in assignees {
        match PublicKey::parse(assignee_hex) {
            Ok(pk) => parsed_assignees.push(pk),
            Err(_) => invalid_assignees.push(*assignee_hex),
        }
    }
    if !invalid_assignees.is_empty() {
        return Err(format!(
            "Invalid assignee keys: {}",
            invalid_assignees.join(", ")
        ));
    }
    // Add assignee p tags
    for pk in &parsed_assignees {
        builder = builder.tag(Tag::public_key(*pk));
    }
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;
    let event_id = *output.id();
    let filter = Filter::new().id(event_id);
    if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(2)).await {
        cache_issue_events(&events);
    }
    Ok(event_id)
}
/// Update issue status (Kind 1630-1633)
pub async fn update_issue_status(
    issue_id: EventId,
    status: IssueStatus,
) -> Result<EventId, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let kind = status.to_kind();
    let builder = EventBuilder::new(kind, "").tag(Tag::event(issue_id));
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish status: {}", e))?;
    let event_id = *output.id();
    let filter = Filter::new().id(event_id);
    if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(2)).await {
        update_issue_statuses(&events);
    }
    Ok(event_id)
}
/// Publish a comment on an issue (NIP-22 Comment, Kind 1111)
pub async fn publish_issue_comment(
    issue_id: EventId,
    issue_author: PublicKey,
    repository: Option<&Coordinate>,
    content: &str,
) -> Result<EventId, String> {
    use nostr::nips::nip22::CommentTarget;
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let comment_to = CommentTarget::event(issue_id, Kind::GitIssue, Some(issue_author), None);
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
/// Fetch comments for an issue (by EventId)
pub async fn fetch_issue_comments(issue_id: EventId) -> Result<Vec<GitComment>, String> {
    let filter = Filter::new().kind(Kind::Comment).event(issue_id);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch comments: {}", e))?;
    Ok(events.iter().filter_map(GitComment::from_event).collect())
}
/// Publish issue by naddr string
pub async fn publish_issue_by_naddr(
    naddr: &str,
    subject: Option<&str>,
    content: &str,
    labels: &[&str],
    assignees: &[&str],
) -> Result<String, String> {
    use crate::utils::nip34::decode_naddr;
    let coord = decode_naddr(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    let result = publish_issue(&coord, subject, content, labels, assignees).await?;
    Ok(result.to_hex())
}
/// Update issue status by event ID string
pub async fn update_issue_status_by_id(
    event_ref: &str,
    status: IssueStatus,
) -> Result<String, String> {
    let event_id =
        decode_event_id(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
    let result = update_issue_status(event_id, status).await?;
    Ok(result.to_hex())
}
/// Publish comment on issue by event ID string
pub async fn publish_comment_by_id(
    event_ref: &str,
    author_hex: &str,
    content: &str,
) -> Result<String, String> {
    use nostr_sdk::prelude::PublicKey;
    let event_id =
        decode_event_id(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
    let author =
        PublicKey::from_hex(author_hex).map_err(|e| format!("Invalid author pubkey: {}", e))?;
    let result = publish_issue_comment(event_id, author, None, content).await?;
    Ok(result.to_hex())
}
/// Fetch comments for issue by event ID string
pub async fn fetch_comments_by_id(event_ref: &str) -> Result<Vec<GitComment>, String> {
    let event_id =
        decode_event_id(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
    fetch_issue_comments(event_id).await
}
