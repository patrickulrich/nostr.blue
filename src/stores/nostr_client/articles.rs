//! Long-form (kind 30023)
//!
//! Functions for fetching and publishing long-form articles (NIP-23).

use std::time::Duration;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

use crate::stores::relay;
use super::fetching::{get_client, fetch_events_aggregated};
use super::signals::HAS_SIGNER;
use super::types::PublishResult;

// =============================================================================
// Article Fetching
// =============================================================================

/// Fetch articles (kind 30023 - NIP-23 long-form content)
/// Returns events sorted by created_at descending (newest first)
pub async fn fetch_articles(
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<nostr::Event>, String> {
    log::info!("Fetching articles with limit: {}", limit);

    use nostr::{Filter, Kind, Timestamp};

    let mut filter = Filter::new()
        .kind(Kind::LongFormTextNote)
        .limit(limit);

    if let Some(until_timestamp) = until {
        filter = filter.until(Timestamp::from(until_timestamp));
    }

    // Use aggregated fetch pattern (DB cache first, background relay sync)
    match fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let mut sorted: Vec<_> = events.into_iter().collect();
            sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            // Enforce limit after sorting (API contract)
            sorted.truncate(limit);
            log::info!("Fetched {} articles", sorted.len());
            Ok(sorted)
        }
        Err(e) => {
            log::error!("Failed to fetch articles: {}", e);
            Err(format!("Failed to fetch articles: {}", e))
        }
    }
}

/// Fetch a specific article by coordinate (kind:pubkey:identifier)
/// Legacy function - use fetch_event_by_coordinate for new code
#[deprecated(since = "0.7.7", note = "Use fetch_event_by_coordinate instead")]
#[allow(dead_code)]
pub async fn fetch_article_by_coordinate(
    pubkey: String,
    identifier: String,
) -> std::result::Result<Option<nostr::Event>, String> {
    fetch_event_by_coordinate(30023, pubkey, identifier).await
}

/// Fetch any addressable event by coordinate (kind:pubkey:identifier)
/// Works for articles (30023), livestreams (30311), and other addressable events
/// Fetch addressable event by coordinate with two-phase loading (DB first, then relay)
/// Optionally uses relay hints for faster fetching
pub async fn fetch_event_by_coordinate(
    kind: u16,
    pubkey: String,
    identifier: String,
) -> std::result::Result<Option<nostr::Event>, String> {
    fetch_event_by_coordinate_with_relays(kind, pubkey, identifier, Vec::new()).await
}

/// Fetch addressable event by coordinate with relay hints
/// Two-phase loading: DB first (instant), then relay (if not found or for freshness)
/// Delegates to relay::connection::fetch_event_by_coordinate_with_relays
pub async fn fetch_event_by_coordinate_with_relays(
    kind: u16,
    pubkey: String,
    identifier: String,
    relay_hints: Vec<String>,
) -> std::result::Result<Option<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    relay::fetch_event_by_coordinate_with_relays(&client, kind, &pubkey, &identifier, relay_hints).await
}

// =============================================================================
// Article Publishing
// =============================================================================

/// Publish a long-form article (Kind 30023) with relay feedback
/// NIP-23: https://github.com/nostr-protocol/nips/blob/master/23.md
pub async fn publish_article_tracked(
    title: String,
    summary: String,
    content: String,
    identifier: String,
    cover_image: String,
    hashtags: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    // Trim all text inputs up front (shadow the parameters)
    let identifier = identifier.trim();
    let title = title.trim();
    let summary = summary.trim();
    let cover_image = cover_image.trim();

    // Validate trimmed values
    if identifier.is_empty() {
        return Err("Identifier cannot be empty".to_string());
    }

    if title.is_empty() {
        return Err("Title cannot be empty".to_string());
    }

    log::info!("Publishing article: {}", title);

    // Build tags using trimmed values
    // NIP-23: Addressable events are identified by kind+pubkey+d-tag
    // No self-referencing 'a' tag needed (would be redundant)
    use nostr::Tag;

    let mut tags = vec![
        Tag::identifier(identifier.to_string()),
        Tag::title(title.to_string()),
    ];

    // Add optional summary
    if !summary.is_empty() {
        tags.push(Tag::custom(
            nostr::TagKind::Custom("summary".into()),
            vec![summary.to_string()]
        ));
    }

    // Add optional cover image
    if !cover_image.is_empty() {
        tags.push(Tag::custom(
            nostr::TagKind::Custom("image".into()),
            vec![cover_image.to_string()]
        ));
    }

    // Add published_at timestamp (cross-platform using nostr_sdk)
    let timestamp = nostr_sdk::Timestamp::now().as_secs().to_string();

    tags.push(Tag::custom(
        nostr::TagKind::Custom("published_at".into()),
        vec![timestamp]
    ));

    // Add hashtags - sanitize: trim, filter empty, dedupe (Tag::hashtag already lowercases)
    use std::collections::HashSet;
    let sanitized: HashSet<String> = hashtags
        .into_iter()
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
        .collect();

    for hashtag in sanitized {
        tags.push(Tag::hashtag(hashtag));
    }

    // Build the event (Kind 30023 - LongFormTextNote)
    let builder = nostr::EventBuilder::new(nostr::Kind::LongFormTextNote, content)
        .tags(tags);

    // Publish
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish article: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Article '{}' published: {} ({}/{} relays succeeded)",
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

/// Publish a long-form article (Kind 30023)
/// For relay feedback, use publish_article_tracked instead
pub async fn publish_article(
    title: String,
    summary: String,
    content: String,
    identifier: String,
    cover_image: String,
    hashtags: Vec<String>,
) -> std::result::Result<String, String> {
    publish_article_tracked(title, summary, content, identifier, cover_image, hashtags)
        .await
        .map(|result| result.event_id)
}
