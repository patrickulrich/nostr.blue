//! Pull Requests Service
//!
//! Handles fetching and publishing NIP-34 Git patch events (Kind 1617).
#![allow(dead_code)]
use crate::stores::code_store::{cache_pr_events, get_cached_pr, update_pr_statuses};
use crate::stores::nostr_client::{fetch_events_aggregated, get_client, HAS_SIGNER};
use crate::utils::nip34::{decode_event_id, GitComment, IssueStatus, PullRequest};
use crate::utils::relay_output::ensure_publish_accepted;
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;
/// Default timeout for fetching events
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

async fn send_and_ensure_published(
    client: &Client,
    builder: EventBuilder,
    action: &str,
) -> Result<EventId, String> {
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("{action}: {e}"))?;
    ensure_publish_accepted(&output, action)?;
    Ok(*output.id())
}

/// Fetch status events for a set of PR event IDs and apply them to the cache
async fn fetch_and_apply_pr_statuses(event_ids: &[EventId]) {
    if event_ids.is_empty() {
        return;
    }
    let status_filter = Filter::new()
        .kinds(vec![
            Kind::GitStatusOpen,
            Kind::GitStatusApplied,
            Kind::GitStatusClosed,
            Kind::GitStatusDraft,
        ])
        .events(event_ids.to_vec());
    if let Ok(status_events) = fetch_events_aggregated(status_filter, FETCH_TIMEOUT).await {
        update_pr_statuses(&status_events);
    }
}
/// Fetch a pull request by its event ID (note1 or nevent1)
pub async fn fetch_pull_request(event_ref: &str) -> Result<PullRequest, String> {
    if let Some(pr) = get_cached_pr(event_ref) {
        return Ok(pr);
    }
    let event_id =
        decode_event_id(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
    let filter = Filter::new().id(event_id).kind(Kind::GitPatch);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch pull request: {}", e))?;
    cache_pr_events(&events);
    let status_filter = Filter::new()
        .kinds(vec![
            Kind::GitStatusOpen,
            Kind::GitStatusApplied,
            Kind::GitStatusClosed,
            Kind::GitStatusDraft,
        ])
        .event(event_id);
    if let Ok(status_events) = fetch_events_aggregated(status_filter, FETCH_TIMEOUT).await {
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
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::A),
            coordinate.to_string(),
        )
        .limit(limit);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch pull requests: {}", e))?;
    cache_pr_events(&events);
    let event_ids: Vec<EventId> = events.iter().map(|e| e.id).collect();
    fetch_and_apply_pr_statuses(&event_ids).await;
    Ok(events.iter().filter_map(PullRequest::from_event).collect())
}
/// Fetch PRs assigned to a user (tagged with #p)
pub async fn fetch_prs_assigned_to(
    pubkey: &PublicKey,
    limit: usize,
) -> Result<Vec<PullRequest>, String> {
    let filter = Filter::new()
        .kind(Kind::GitPatch)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::P), pubkey.to_hex())
        .limit(limit);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch assigned PRs: {}", e))?;
    cache_pr_events(&events);
    let event_ids: Vec<EventId> = events.iter().map(|e| e.id).collect();
    fetch_and_apply_pr_statuses(&event_ids).await;
    Ok(events.iter().filter_map(PullRequest::from_event).collect())
}
/// Fetches PRs mentioning the given pubkey.
///
/// Note: NIP-34 uses p-tags for both assignment and mentions with no protocol-level
/// distinction, so this delegates to `fetch_prs_assigned_to`.
pub async fn fetch_prs_mentioning(
    pubkey: &PublicKey,
    limit: usize,
) -> Result<Vec<PullRequest>, String> {
    fetch_prs_assigned_to(pubkey, limit).await
}
/// Search PRs by text (NIP-50)
///
/// When `query` is `None`, no `.search()` filter is applied — the relay returns
/// all PRs up to `limit`, which is useful for filter-only queries.
pub async fn search_prs(query: Option<&str>, limit: usize) -> Result<Vec<PullRequest>, String> {
    let mut filter = Filter::new().kind(Kind::GitPatch).limit(limit);
    if let Some(q) = query {
        filter = filter.search(q);
    }
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to search pull requests: {}", e))?;
    cache_pr_events(&events);
    let event_ids: Vec<EventId> = events.iter().map(|e| e.id).collect();
    fetch_and_apply_pr_statuses(&event_ids).await;
    Ok(events.iter().filter_map(PullRequest::from_event).collect())
}
/// Fetch pull requests by author
pub async fn fetch_user_prs(pubkey: &PublicKey, limit: usize) -> Result<Vec<PullRequest>, String> {
    let filter = Filter::new()
        .kind(Kind::GitPatch)
        .author(*pubkey)
        .limit(limit);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch pull requests: {}", e))?;
    cache_pr_events(&events);
    let event_ids: Vec<EventId> = events.iter().map(|e| e.id).collect();
    fetch_and_apply_pr_statuses(&event_ids).await;
    Ok(events.iter().filter_map(PullRequest::from_event).collect())
}
/// Publish a new patch/pull request
#[allow(clippy::too_many_arguments)]
pub async fn publish_patch(
    repository: &Coordinate,
    content: &str,
    commit: Option<&str>,
    parent_commit: Option<&str>,
    is_cover_letter: bool,
    labels: &[&str],
    closes_issues: &[&str],
    branch_name: Option<&str>,
) -> Result<EventId, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let mut builder = EventBuilder::new(Kind::GitPatch, content)
        .tag(Tag::coordinate(repository.clone(), None))
        .tag(Tag::public_key(repository.public_key));
    if let Some(hash) = commit {
        builder = builder.tag(Tag::custom(
            TagKind::Custom(std::borrow::Cow::Borrowed("commit")),
            [hash],
        ));
    }
    if let Some(hash) = parent_commit {
        builder = builder.tag(Tag::custom(
            TagKind::Custom(std::borrow::Cow::Borrowed("parent-commit")),
            [hash],
        ));
    }
    if is_cover_letter {
        builder = builder.tag(Tag::hashtag("cover-letter"));
    }
    for label in labels {
        builder = builder.tag(Tag::hashtag(*label));
    }
    // Validate all issue IDs upfront (supports hex, note1, NIP-21, and nevent1)
    let mut invalid_ids = Vec::new();
    let mut parsed_issue_ids = Vec::new();
    for issue_id in closes_issues {
        if let Ok(eid) = EventId::parse(issue_id) {
            parsed_issue_ids.push(eid);
        } else if let Ok(eid) = decode_event_id(issue_id) {
            parsed_issue_ids.push(eid);
        } else {
            invalid_ids.push(*issue_id);
        }
    }
    if !invalid_ids.is_empty() {
        return Err(format!("Invalid issue IDs: {}", invalid_ids.join(", ")));
    }
    // Add linked issue tags with "closes" marker
    for eid in &parsed_issue_ids {
        builder = builder.tag(Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)),
            [
                eid.to_hex(),
                String::new(),
                String::new(),
                "closes".to_string(),
            ],
        ));
    }
    // Add branch name tag
    if let Some(name) = branch_name {
        builder = builder.tag(Tag::custom(
            TagKind::Custom(std::borrow::Cow::Borrowed("branch")),
            [name],
        ));
    }
    let event_id = send_and_ensure_published(&client, builder, "Failed to publish").await?;
    let filter = Filter::new().id(event_id);
    if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(2)).await {
        cache_pr_events(&events);
    }
    Ok(event_id)
}
/// Publish a PR update event (Kind 1619)
///
/// Used when the PR author pushes new commits or updates the patch content.
/// References the original PR event.
pub async fn publish_pr_update(
    pr_id: EventId,
    repository: &Coordinate,
    content: &str,
    commit: Option<&str>,
    parent_commit: Option<&str>,
) -> Result<EventId, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let mut builder = EventBuilder::new(Kind::from(1619), content)
        .tag(Tag::event(pr_id))
        .tag(Tag::coordinate(repository.clone(), None))
        .tag(Tag::public_key(repository.public_key));
    if let Some(hash) = commit {
        builder = builder.tag(Tag::custom(
            TagKind::Custom(std::borrow::Cow::Borrowed("commit")),
            [hash],
        ));
    }
    if let Some(hash) = parent_commit {
        builder = builder.tag(Tag::custom(
            TagKind::Custom(std::borrow::Cow::Borrowed("parent-commit")),
            [hash],
        ));
    }
    send_and_ensure_published(&client, builder, "Failed to publish PR update").await
}

/// Publish a PR update by event ID and naddr strings
pub async fn publish_pr_update_by_id(
    event_ref: &str,
    naddr: &str,
    content: &str,
    commit: Option<&str>,
    parent_commit: Option<&str>,
) -> Result<String, String> {
    use crate::utils::nip34::decode_naddr;
    let event_id =
        decode_event_id(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
    let coord = decode_naddr(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    let result = publish_pr_update(event_id, &coord, content, commit, parent_commit).await?;
    Ok(result.to_hex())
}

/// Update PR status (Kind 1630-1633)
pub async fn update_pr_status(pr_id: EventId, status: IssueStatus) -> Result<EventId, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let kind = status.to_kind();
    let builder = EventBuilder::new(kind, "").tag(Tag::event(pr_id));
    let event_id = send_and_ensure_published(&client, builder, "Failed to publish status").await?;
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
    send_and_ensure_published(&client, builder, "Failed to publish comment").await
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
#[allow(clippy::too_many_arguments)]
pub async fn publish_patch_by_naddr(
    naddr: &str,
    content: &str,
    commit: Option<&str>,
    parent_commit: Option<&str>,
    is_cover_letter: bool,
    labels: &[&str],
    closes_issues: &[&str],
    branch_name: Option<&str>,
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
        closes_issues,
        branch_name,
    )
    .await?;
    Ok(result.to_hex())
}
/// Update PR status by event ID string
pub async fn update_pr_status_by_id(
    event_ref: &str,
    status: IssueStatus,
) -> Result<String, String> {
    let event_id =
        decode_event_id(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
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
    let event_id =
        decode_event_id(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
    let author =
        PublicKey::from_hex(author_hex).map_err(|e| format!("Invalid author pubkey: {}", e))?;
    let result = publish_pr_comment(event_id, author, None, content).await?;
    Ok(result.to_hex())
}
/// Fetch comments for PR by event ID string
pub async fn fetch_pr_comments_by_id(event_ref: &str) -> Result<Vec<GitComment>, String> {
    let event_id =
        decode_event_id(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
    fetch_pr_comments(event_id).await
}

/// A comment anchored to a specific file and line in a diff
#[derive(Clone, Debug, PartialEq)]
pub struct LineComment {
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// Comment content
    pub content: String,
    /// Created timestamp (unix seconds)
    pub created_at: u64,
    /// File path the comment is anchored to
    pub file_path: String,
    /// Line number within the file the comment is anchored to
    pub line_number: usize,
}

/// Publish a line-level comment on a PR
///
/// Uses Kind::Comment (1111) like regular PR comments, with additional
/// `["file", "{file_path}"]` and `["line", "{line_number}"]` tags to
/// anchor the comment to a specific location in the diff.
pub async fn publish_line_comment(
    pr_id: EventId,
    pr_author: PublicKey,
    content: &str,
    file_path: &str,
    line_number: usize,
) -> Result<EventId, String> {
    use nostr::nips::nip22::CommentTarget;
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let comment_to = CommentTarget::event(pr_id, Kind::GitPatch, Some(pr_author), None);
    let builder = EventBuilder::comment(content, comment_to, None)
        .tag(Tag::custom(
            TagKind::Custom(std::borrow::Cow::Borrowed("file")),
            [file_path.to_string()],
        ))
        .tag(Tag::custom(
            TagKind::Custom(std::borrow::Cow::Borrowed("line")),
            [line_number.to_string()],
        ));
    send_and_ensure_published(&client, builder, "Failed to publish line comment").await
}

/// Fetch line comments for a PR
///
/// Fetches all Kind 1111 comments for the PR and filters for those
/// that have both "file" and "line" tags, parsing them into LineComment structs.
pub async fn fetch_line_comments(pr_id: EventId) -> Result<Vec<LineComment>, String> {
    let filter = Filter::new().kind(Kind::Comment).event(pr_id);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch line comments: {}", e))?;
    let mut comments = Vec::new();
    for event in &events {
        let mut file_path = None;
        let mut line_number = None;
        for tag in event.tags.iter() {
            let kind = tag.kind();
            if kind == TagKind::Custom(std::borrow::Cow::Borrowed("file")) {
                if let Some(f) = tag.content() {
                    file_path = Some(f.to_string());
                }
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("line")) {
                if let Some(l) = tag.content() {
                    line_number = l.parse::<usize>().ok();
                }
            }
        }
        // Only include comments that have both file and line tags
        if let (Some(fp), Some(ln)) = (file_path, line_number) {
            comments.push(LineComment {
                event_id: event.id.to_hex(),
                pubkey: event.pubkey.to_hex(),
                content: event.content.clone(),
                created_at: event.created_at.as_secs(),
                file_path: fp,
                line_number: ln,
            });
        }
    }
    // Sort by creation time ascending
    comments.sort_by_key(|c| c.created_at);
    Ok(comments)
}

/// Publish a line-level comment on a PR by event ID and author hex strings
pub async fn publish_line_comment_by_id(
    event_ref: &str,
    author_hex: &str,
    content: &str,
    file_path: &str,
    line_number: usize,
) -> Result<String, String> {
    let event_id =
        decode_event_id(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
    let author =
        PublicKey::from_hex(author_hex).map_err(|e| format!("Invalid author pubkey: {}", e))?;
    let result = publish_line_comment(event_id, author, content, file_path, line_number).await?;
    Ok(result.to_hex())
}

/// Fetch line comments for a PR by event ID string
pub async fn fetch_line_comments_by_id(event_ref: &str) -> Result<Vec<LineComment>, String> {
    let event_id =
        decode_event_id(event_ref).map_err(|e| format!("Invalid event reference: {}", e))?;
    fetch_line_comments(event_id).await
}
