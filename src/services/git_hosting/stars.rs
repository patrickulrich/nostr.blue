//! Stars Service
//!
//! Handles repository stars using Kind 7 (Reaction) events.

#![allow(dead_code)]

use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;

use crate::stores::code_store::{is_repo_starred, star_repo, unstar_repo, STARRED_REPOS};
use crate::stores::nostr_client::{fetch_events_aggregated, get_client, HAS_SIGNER};
use crate::stores::auth_store;

/// Default timeout for fetching events
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Star a repository (publish reaction event)
pub async fn publish_star(coordinate: &Coordinate) -> Result<EventId, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    // Check if already starred locally
    let coord_str = format!(
        "{}:{}:{}",
        coordinate.kind.as_u16(),
        coordinate.public_key.to_hex(),
        coordinate.identifier
    );

    if is_repo_starred(&coord_str) {
        return Err("Already starred".to_string());
    }

    // Build reaction event with + content and a-tag
    let builder = EventBuilder::new(Kind::Reaction, "+")
        .tag(Tag::coordinate(coordinate.clone(), None));

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish star: {}", e))?;

    let event_id = *output.id();

    // Update local state
    star_repo(&coord_str);

    Ok(event_id)
}

/// Unstar a repository (publish delete event for the reaction)
pub async fn remove_star(coordinate: &Coordinate) -> Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    let coord_str = format!(
        "{}:{}:{}",
        coordinate.kind.as_u16(),
        coordinate.public_key.to_hex(),
        coordinate.identifier
    );

    if !is_repo_starred(&coord_str) {
        return Err("Not starred".to_string());
    }

    // Find the star event to delete
    let my_pubkey_str = auth_store::get_pubkey()
        .ok_or_else(|| "Not logged in".to_string())?;
    let my_pubkey = PublicKey::from_hex(&my_pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::Reaction)
        .author(my_pubkey)
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::A),
            coordinate.to_string(),
        );

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch star: {}", e))?;

    if let Some(star_event) = events.first() {
        // Publish deletion
        use nostr::nips::nip09::EventDeletionRequest;
        let request = EventDeletionRequest::new().id(star_event.id);
        let builder = EventBuilder::delete(request);

        client.send_event_builder(builder).await
            .map_err(|e| format!("Failed to publish delete: {}", e))?;
    }

    // Update local state
    unstar_repo(&coord_str);

    Ok(())
}

/// Fetch star count for a repository
pub async fn fetch_star_count(coordinate: &Coordinate) -> Result<u32, String> {
    let filter = Filter::new()
        .kind(Kind::Reaction)
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::A),
            coordinate.to_string(),
        );

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch stars: {}", e))?;

    // Count unique authors with + reactions
    let unique_stars: std::collections::HashSet<_> = events
        .iter()
        .filter(|e| e.content == "+" || e.content.is_empty())
        .map(|e| e.pubkey)
        .collect();

    Ok(unique_stars.len() as u32)
}

/// Check if the current user has starred a repository
pub async fn check_user_star(coordinate: &Coordinate) -> Result<bool, String> {
    let coord_str = format!(
        "{}:{}:{}",
        coordinate.kind.as_u16(),
        coordinate.public_key.to_hex(),
        coordinate.identifier
    );

    // Check local cache first
    if is_repo_starred(&coord_str) {
        return Ok(true);
    }

    // If not in cache, check from relays
    let my_pubkey_str = match auth_store::get_pubkey() {
        Some(pk) => pk,
        None => return Ok(false),
    };
    let my_pubkey = match PublicKey::from_hex(&my_pubkey_str) {
        Ok(pk) => pk,
        Err(_) => return Ok(false),
    };

    let filter = Filter::new()
        .kind(Kind::Reaction)
        .author(my_pubkey)
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::A),
            coordinate.to_string(),
        );

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to check star: {}", e))?;

    let is_starred = events.iter().any(|e| e.content == "+" || e.content.is_empty());

    // Update local cache
    if is_starred {
        star_repo(&coord_str);
    }

    Ok(is_starred)
}

/// Load user's starred repositories from relays
pub async fn load_user_stars() -> Result<(), String> {
    let my_pubkey_str = match auth_store::get_pubkey() {
        Some(pk) => pk,
        None => return Err("Not logged in".to_string()),
    };
    let my_pubkey = PublicKey::from_hex(&my_pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::Reaction)
        .author(my_pubkey)
        .limit(500);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch stars: {}", e))?;

    // Extract repository coordinates from star reactions
    let mut starred = STARRED_REPOS.write();
    starred.clear();

    for event in events {
        if event.content != "+" && !event.content.is_empty() {
            continue;
        }

        for tag in event.tags.iter() {
            if let Some(TagStandard::Coordinate { coordinate, .. }) = tag.as_standardized() {
                if coordinate.kind == Kind::GitRepoAnnouncement {
                    let coord_str = format!(
                        "{}:{}:{}",
                        coordinate.kind.as_u16(),
                        coordinate.public_key.to_hex(),
                        coordinate.identifier
                    );
                    starred.insert(coord_str);
                }
            }
        }
    }

    Ok(())
}
