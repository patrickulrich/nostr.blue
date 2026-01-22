//! Code Snippets Service
//!
//! Handles fetching and publishing NIP-C0 Code Snippet events (Kind 1337).

#![allow(dead_code)]

use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;

use crate::stores::code_store::{cache_snippet_events, get_cached_snippet};
use crate::stores::nostr_client::{fetch_events_aggregated, get_client, HAS_SIGNER};
use crate::utils::nip34::{decode_event_id, DisplaySnippet};

/// Default timeout for fetching events
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Fetch a snippet by its event ID (note1, nevent1, or hex)
/// Alias: `fetch_snippet_by_id`
pub async fn fetch_snippet(event_ref: &str) -> Result<DisplaySnippet, String> {
    // First check cache
    if let Some(snippet) = get_cached_snippet(event_ref) {
        return Ok(snippet);
    }

    // Decode event reference
    let event_id = decode_event_id(event_ref)
        .map_err(|e| format!("Invalid event reference: {}", e))?;

    // Build filter
    let filter = Filter::new()
        .id(event_id)
        .kind(Kind::CodeSnippet);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch snippet: {}", e))?;

    // Parse and cache
    cache_snippet_events(&events);

    get_cached_snippet(&event_id.to_hex())
        .ok_or_else(|| "Snippet not found".to_string())
}

/// Alias for fetch_snippet - fetch by event ID
pub async fn fetch_snippet_by_id(event_ref: &str) -> Result<DisplaySnippet, String> {
    fetch_snippet(event_ref).await
}

/// Fetch snippets by author
pub async fn fetch_user_snippets(pubkey: &PublicKey, limit: usize) -> Result<Vec<DisplaySnippet>, String> {
    let filter = Filter::new()
        .kind(Kind::CodeSnippet)
        .author(*pubkey)
        .limit(limit);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch snippets: {}", e))?;

    cache_snippet_events(&events);

    Ok(events
        .iter()
        .filter_map(DisplaySnippet::from_event)
        .collect())
}

/// Fetch recent snippets
pub async fn fetch_recent_snippets(limit: usize) -> Result<Vec<DisplaySnippet>, String> {
    let filter = Filter::new()
        .kind(Kind::CodeSnippet)
        .limit(limit);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch snippets: {}", e))?;

    cache_snippet_events(&events);

    Ok(events
        .iter()
        .filter_map(DisplaySnippet::from_event)
        .collect())
}

/// Fetch snippets by language
pub async fn fetch_snippets_by_language(language: &str, limit: usize) -> Result<Vec<DisplaySnippet>, String> {
    let filter = Filter::new()
        .kind(Kind::CodeSnippet)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::L), language)
        .limit(limit);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch snippets: {}", e))?;

    cache_snippet_events(&events);

    Ok(events
        .iter()
        .filter_map(DisplaySnippet::from_event)
        .collect())
}

/// Search snippets by text
pub async fn search_snippets(query: &str, limit: usize) -> Result<Vec<DisplaySnippet>, String> {
    let filter = Filter::new()
        .kind(Kind::CodeSnippet)
        .search(query)
        .limit(limit);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to search snippets: {}", e))?;

    cache_snippet_events(&events);

    Ok(events
        .iter()
        .filter_map(DisplaySnippet::from_event)
        .collect())
}

/// Publish a new code snippet
#[allow(clippy::too_many_arguments)]
pub async fn publish_snippet(
    code: &str,
    language: Option<&str>,
    name: Option<&str>,
    extension: Option<&str>,
    description: Option<&str>,
    runtime: Option<&str>,
    license: Option<&str>,
    dependencies: &[&str],
    repo_url: Option<&str>,
) -> Result<EventId, String> {
    use nostr::nips::nipc0::CodeSnippet;

    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    // Build snippet using SDK type
    let mut snippet = CodeSnippet::new(code);

    if let Some(lang) = language {
        snippet = snippet.language(lang);
    }
    if let Some(n) = name {
        snippet = snippet.name(n);
    }
    if let Some(ext) = extension {
        snippet = snippet.extension(ext);
    }
    if let Some(desc) = description {
        snippet = snippet.description(desc);
    }
    if let Some(rt) = runtime {
        snippet = snippet.runtime(rt);
    }
    if let Some(lic) = license {
        snippet = snippet.license(lic);
    }
    for dep in dependencies {
        snippet = snippet.dependencies(*dep);
    }
    if let Some(repo) = repo_url {
        snippet = snippet.repo(repo);
    }

    // Build and publish using EventBuilder's public method
    let builder = EventBuilder::code_snippet(snippet);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish: {}", e))?;

    let event_id = *output.id();

    // Fetch back for caching
    let filter = Filter::new().id(event_id);
    if let Ok(events) = fetch_events_aggregated(filter, Duration::from_secs(2)).await {
        cache_snippet_events(&events);
    }

    Ok(event_id)
}

/// Delete a snippet (publish deletion event)
pub async fn delete_snippet(event_id: EventId) -> Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    use nostr::nips::nip09::EventDeletionRequest;
    let request = EventDeletionRequest::new().id(event_id).reason("Snippet deleted");
    let builder = EventBuilder::delete(request);

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish deletion: {}", e))?;

    // Remove from cache
    crate::stores::code_store::CODE_SNIPPETS_CACHE
        .write()
        .pop(&event_id.to_hex());

    Ok(())
}
