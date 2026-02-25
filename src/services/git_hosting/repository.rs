//! Repository Service
//!
//! Handles fetching and publishing NIP-34 Git repository events (Kind 30617).
#![allow(dead_code)]
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;
use crate::stores::code_store::{cache_repo_events, get_cached_repo};
use crate::stores::nostr_client::{fetch_events_aggregated, get_client, HAS_SIGNER};
use crate::utils::nip34::{decode_repo_naddr, Repository};
/// Default timeout for fetching events
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// Fetch a repository by its naddr coordinate
pub async fn fetch_repository(naddr: &str) -> Result<Repository, String> {
    if let Some(repo) = get_cached_repo(naddr) {
        return Ok(repo);
    }
    let (coordinate, _relay_hints) = decode_repo_naddr(naddr)
        .map_err(|e| format!("Invalid naddr: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::GitRepoAnnouncement)
        .author(coordinate.public_key)
        .identifier(&coordinate.identifier);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch repository: {}", e))?;
    cache_repo_events(&events);
    events
        .into_iter()
        .max_by_key(|e| e.created_at)
        .and_then(|e| Repository::from_event(&e))
        .ok_or_else(|| "Repository not found".to_string())
}
/// Fetch repositories by author pubkey
pub async fn fetch_user_repositories(
    pubkey: &PublicKey,
    limit: usize,
) -> Result<Vec<Repository>, String> {
    let filter = Filter::new()
        .kind(Kind::GitRepoAnnouncement)
        .author(*pubkey)
        .limit(limit);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch repositories: {}", e))?;
    cache_repo_events(&events);
    Ok(events.iter().filter_map(Repository::from_event).collect())
}
/// Fetch recent/trending repositories
pub async fn fetch_recent_repositories(
    limit: usize,
    until: Option<u64>,
) -> Result<Vec<Repository>, String> {
    let mut filter = Filter::new().kind(Kind::GitRepoAnnouncement).limit(limit);
    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch repositories: {}", e))?;
    cache_repo_events(&events);
    Ok(events.iter().filter_map(Repository::from_event).collect())
}
/// Search repositories by text
///
/// When `query` is `None`, no `.search()` filter is applied — the relay returns
/// all repositories up to `limit`, which is useful for filter-only queries.
pub async fn search_repositories(
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<Repository>, String> {
    let mut filter = Filter::new().kind(Kind::GitRepoAnnouncement).limit(limit);
    if let Some(q) = query {
        filter = filter.search(q);
    }
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to search repositories: {}", e))?;
    cache_repo_events(&events);
    Ok(events.iter().filter_map(Repository::from_event).collect())
}
/// Publish a new repository announcement
#[allow(clippy::too_many_arguments)]
pub async fn publish_repository(
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
    clone_urls: &[&str],
    web_urls: &[&str],
    relays: &[&str],
    maintainers: &[PublicKey],
    topics: &[&str],
) -> Result<EventId, String> {
    let topic_tags: Vec<Tag> = topics.iter().map(|t| Tag::hashtag(*t)).collect();
    publish_repository_with_extras(id, name, description, clone_urls, web_urls, relays, maintainers, &topic_tags).await
}
/// Publish a repository announcement with additional custom tags (zap splits, milestones, etc.)
#[allow(clippy::too_many_arguments)]
pub async fn publish_repository_with_extras(
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
    clone_urls: &[&str],
    web_urls: &[&str],
    relays: &[&str],
    maintainers: &[PublicKey],
    extra_tags: &[Tag],
) -> Result<EventId, String> {
    use nostr::nips::nip34::GitRepositoryAnnouncement;
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let mut repo = GitRepositoryAnnouncement {
        id: id.to_string(),
        name: name.map(|s| s.to_string()),
        description: description.map(|s| s.to_string()),
        web: vec![],
        clone: vec![],
        relays: vec![],
        euc: None,
        maintainers: maintainers.to_vec(),
    };
    for url in clone_urls {
        if let Ok(u) = url::Url::parse(url) {
            repo.clone.push(u);
        }
    }
    for url in web_urls {
        if let Ok(u) = url::Url::parse(url) {
            repo.web.push(u);
        }
    }
    for relay in relays {
        if let Ok(r) = RelayUrl::parse(relay) {
            repo.relays.push(r);
        }
    }
    let mut builder = EventBuilder::git_repository_announcement(repo)
        .map_err(|e| format!("Failed to build event: {}", e))?;
    for tag in extra_tags {
        builder = builder.tag(tag.clone());
    }
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;
    let event_id = *output.id();
    let filter = Filter::new().id(event_id);
    if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(2)).await {
        cache_repo_events(&events);
    }
    Ok(event_id)
}
/// Publish a forked repository announcement
///
/// Creates a new repo announcement with an `e` tag referencing the original repo.
pub async fn publish_fork(
    original_event_id: &str,
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
    clone_urls: &[&str],
) -> Result<EventId, String> {
    use nostr::nips::nip34::GitRepositoryAnnouncement;
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let repo = GitRepositoryAnnouncement {
        id: id.to_string(),
        name: name.map(|s| s.to_string()),
        description: description.map(|s| s.to_string()),
        web: vec![],
        clone: clone_urls
            .iter()
            .filter_map(|u| url::Url::parse(u).ok())
            .collect(),
        relays: vec![],
        euc: None,
        maintainers: vec![],
    };
    let orig_id = EventId::from_hex(original_event_id)
        .map_err(|e| format!("Invalid original event ID: {}", e))?;
    let builder = EventBuilder::git_repository_announcement(repo)
        .map_err(|e| format!("Failed to build event: {}", e))?
        .tag(Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)),
            [orig_id.to_hex(), String::new(), String::new(), "fork".to_string()],
        ));
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish fork: {}", e))?;
    let event_id = *output.id();
    let filter = Filter::new().id(event_id);
    if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(2)).await {
        cache_repo_events(&events);
    }
    Ok(event_id)
}

/// Delete a repository (publish deletion event)
pub async fn delete_repository(coordinate: &Coordinate) -> Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    use nostr::nips::nip09::EventDeletionRequest;
    let request = EventDeletionRequest::new().reason("Repository deleted");
    let builder = EventBuilder::delete(request);
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish deletion: {}", e))?;
    let coord_str = format!(
        "{}:{}:{}",
        coordinate.kind.as_u16(),
        coordinate.public_key.to_hex(),
        coordinate.identifier,
    );
    crate::stores::code_store::CODE_REPOS_CACHE.write().pop(&coord_str);
    Ok(())
}
