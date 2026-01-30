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
    // First check cache
    if let Some(repo) = get_cached_repo(naddr) {
        return Ok(repo);
    }

    // Decode naddr to coordinate
    let (coordinate, _relay_hints) =
        decode_repo_naddr(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;

    // Build filter
    let filter = Filter::new()
        .kind(Kind::GitRepoAnnouncement)
        .author(coordinate.public_key)
        .identifier(&coordinate.identifier);

    // Fetch from relays
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch repository: {}", e))?;

    // Parse and cache
    cache_repo_events(&events);

    // Find the most recent event
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
pub async fn fetch_recent_repositories(limit: usize) -> Result<Vec<Repository>, String> {
    let filter = Filter::new().kind(Kind::GitRepoAnnouncement).limit(limit);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch repositories: {}", e))?;

    cache_repo_events(&events);

    Ok(events.iter().filter_map(Repository::from_event).collect())
}

/// Search repositories by text
pub async fn search_repositories(query: &str, limit: usize) -> Result<Vec<Repository>, String> {
    let filter = Filter::new()
        .kind(Kind::GitRepoAnnouncement)
        .search(query)
        .limit(limit);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to search repositories: {}", e))?;

    cache_repo_events(&events);

    Ok(events.iter().filter_map(Repository::from_event).collect())
}

/// Publish a new repository announcement
pub async fn publish_repository(
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
    clone_urls: &[&str],
    web_urls: &[&str],
    relays: &[&str],
    maintainers: &[PublicKey],
) -> Result<EventId, String> {
    use nostr::nips::nip34::GitRepositoryAnnouncement;

    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    // Build repository announcement using SDK type
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

    // Parse URLs
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

    // Build event using EventBuilder's public method
    let builder = EventBuilder::git_repository_announcement(repo)
        .map_err(|e| format!("Failed to build event: {}", e))?;

    // Sign and publish
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;

    let event_id = *output.id();

    // Fetch the event back for caching
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

    // Remove from cache
    let coord_str = format!(
        "{}:{}:{}",
        coordinate.kind.as_u16(),
        coordinate.public_key.to_hex(),
        coordinate.identifier
    );
    crate::stores::code_store::CODE_REPOS_CACHE
        .write()
        .pop(&coord_str);

    Ok(())
}
