//! Watch/Subscribe Service
//!
//! Handles repository watch subscriptions using Kind 30003 (NIP-51 Bookmark Sets)
//! with d-tag "code-watched-repos". Each watched repo is stored as an `a` tag
//! referencing its Kind 30617 coordinate.
use nostr_sdk::prelude::*;
use std::time::Duration;

use crate::stores::{auth_store, nostr_client};

/// NIP-51 Kind 30003 - Bookmark Sets (addressable, uses d-tag)
const LIST_KIND: u16 = 30003;

/// D tag identifier for watched repositories
const D_TAG: &str = "code-watched-repos";

/// Timeout for relay fetches
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Fetch the list of watched repo coordinates for the current user.
/// Returns coordinate strings like "30617:<pubkey>:<identifier>".
pub async fn fetch_watched_repos() -> Result<Vec<String>, String> {
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(LIST_KIND))
        .identifier(D_TAG)
        .limit(1);

    let events = nostr_client::fetch_events_aggregated(filter, FETCH_TIMEOUT).await?;

    let Some(event) = events.first() else {
        return Ok(Vec::new());
    };

    let coords: Vec<String> = event
        .tags
        .iter()
        .filter_map(|tag| {
            if let Some(TagStandard::Coordinate { coordinate, .. }) = tag.as_standardized() {
                if coordinate.kind == Kind::GitRepoAnnouncement {
                    return Some(format!(
                        "{}:{}:{}",
                        coordinate.kind.as_u16(),
                        coordinate.public_key.to_hex(),
                        coordinate.identifier,
                    ));
                }
            }
            None
        })
        .collect();

    Ok(coords)
}

/// Toggle a repo's watch status. Fetches the current list, adds or removes
/// the coordinate, and republishes the Kind 30003 event.
pub async fn toggle_watch_repo(
    coordinate: &Coordinate,
    currently_watching: bool,
) -> Result<(), String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Fetch the current watched repos event
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(LIST_KIND))
        .identifier(D_TAG)
        .limit(1);

    let events = nostr_client::fetch_events_aggregated(filter, FETCH_TIMEOUT).await?;

    // Collect existing coordinate tags (excluding the one we're toggling)
    let target_str = format!(
        "{}:{}:{}",
        coordinate.kind.as_u16(),
        coordinate.public_key.to_hex(),
        coordinate.identifier,
    );

    let mut existing_coords: Vec<Coordinate> = Vec::new();
    if let Some(event) = events.first() {
        for tag in event.tags.iter() {
            if let Some(TagStandard::Coordinate {
                coordinate: c, ..
            }) = tag.as_standardized()
            {
                let c_str = format!(
                    "{}:{}:{}",
                    c.kind.as_u16(),
                    c.public_key.to_hex(),
                    c.identifier,
                );
                if c_str != target_str {
                    existing_coords.push(c.clone());
                }
            }
        }
    }

    // Add the coordinate if we're watching, skip if unwatching (already filtered out)
    if !currently_watching {
        existing_coords.push(coordinate.clone());
    }

    // Build and publish the updated event
    let mut builder = EventBuilder::new(Kind::from(LIST_KIND), "")
        .tag(Tag::identifier(D_TAG));

    for coord in &existing_coords {
        builder = builder.tag(Tag::coordinate(coord.clone(), None));
    }

    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish watch list: {}", e))?;

    Ok(())
}
