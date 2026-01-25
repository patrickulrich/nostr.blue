//! Mute/block/report (kind 10000, 1984)
//!
//! Functions for muting posts, blocking users, and reporting content (NIP-51, NIP-56).

use std::time::Duration;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

use super::fetching::{get_client, fetch_events_aggregated};
use super::signals::HAS_SIGNER;
use super::types::{MuteListTags, extract_mute_list_tags, rebuild_mute_list_tags};

// =============================================================================
// Mute List Fetching
// =============================================================================

/// Fetch the mute list (kind 10000) from relays
/// NIP-51: https://github.com/nostr-protocol/nips/blob/master/51.md
async fn fetch_mute_list() -> std::result::Result<Option<nostr::Event>, String> {
    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    let pubkey = nostr::PublicKey::from_hex(&current_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Note: No .limit(1) - let aggregation collect all versions from multiple relays.
    // Relays may return stale versions; max_by_key(created_at) selects the newest.
    let filter = nostr::Filter::new()
        .author(pubkey)
        .kind(nostr::Kind::from(10000));

    // Fetch from database/relays
    match fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        // Select latest event by timestamp (nostr-database pattern)
        Ok(events) => Ok(events.into_iter().max_by_key(|e| e.created_at)),
        Err(e) => {
            log::error!("Failed to fetch mute list: {}", e);
            Ok(None)
        }
    }
}

// =============================================================================
// Muted Posts
// =============================================================================

/// Get all muted event IDs
pub async fn get_muted_posts() -> std::result::Result<Vec<String>, String> {
    match fetch_mute_list().await? {
        Some(event) => {
            // Use SDK's event_ids() method to extract e-tags
            let muted_posts: Vec<String> = event.tags.event_ids()
                .map(|id| id.to_string())
                .collect();
            Ok(muted_posts)
        }
        None => Ok(Vec::new()),
    }
}

/// Normalize event ID to hex format (handles both hex and bech32 note1... inputs)
fn normalize_event_id(event_id: &str) -> Result<String, String> {
    // Try hex first
    if let Ok(id) = nostr::EventId::from_hex(event_id) {
        return Ok(id.to_hex());
    }
    // Try bech32 (note1...)
    nostr::EventId::parse(event_id)
        .map(|id| id.to_hex())
        .map_err(|e| format!("Invalid event ID: {}", e))
}

/// Check if a post is muted
pub async fn is_post_muted(event_id: String) -> std::result::Result<bool, String> {
    let normalized = normalize_event_id(&event_id)?;
    let muted_posts = get_muted_posts().await?;
    Ok(muted_posts.contains(&normalized))
}

/// Mute a post (add to mute list kind 10000)
/// NIP-51: https://github.com/nostr-protocol/nips/blob/master/51.md
pub async fn mute_post(event_id: String) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Muting post: {}", event_id);

    // Use parse() to support both hex and bech32 (note1...) formats
    let target_event_id = nostr::EventId::parse(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;

    // Fetch current mute list and extract tags
    let mute_event = fetch_mute_list().await?;
    let (mut tags, existing_content) = match mute_event {
        Some(event) => {
            let content = event.content.clone();
            (extract_mute_list_tags(&event), content)
        }
        None => (MuteListTags::default(), String::new())
    };

    // Add new muted post if not already present
    if !tags.event_ids.contains(&target_event_id) {
        tags.event_ids.push(target_event_id);
    }

    let all_tags = rebuild_mute_list_tags(&tags);
    let builder = nostr::EventBuilder::new(nostr::Kind::from(10000), existing_content).tags(all_tags);

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish mute list: {}", e))?;

    log::info!("Post muted successfully");
    Ok(())
}

/// Unmute a post (remove from mute list)
pub async fn unmute_post(event_id: String) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Unmuting post: {}", event_id);

    // Use parse() to support both hex and bech32 (note1...) formats
    let target_event_id = nostr::EventId::parse(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;

    // Fetch current mute list and extract tags
    let mute_event = fetch_mute_list().await?
        .ok_or("No mute list found")?;
    let existing_content = mute_event.content.clone();
    let mut tags = extract_mute_list_tags(&mute_event);

    // Remove the target post
    tags.event_ids.retain(|eid| *eid != target_event_id);

    let all_tags = rebuild_mute_list_tags(&tags);
    let builder = nostr::EventBuilder::new(nostr::Kind::from(10000), existing_content).tags(all_tags);

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish mute list: {}", e))?;

    log::info!("Post unmuted successfully");
    Ok(())
}

// =============================================================================
// Blocked Users
// =============================================================================

/// Get all blocked user pubkeys
pub async fn get_blocked_users() -> std::result::Result<Vec<String>, String> {
    match fetch_mute_list().await? {
        Some(event) => {
            // Use SDK's public_keys() method to extract p-tags
            let blocked_users: Vec<String> = event.tags.public_keys()
                .map(|pk| pk.to_string())
                .collect();
            Ok(blocked_users)
        }
        None => Ok(Vec::new()),
    }
}

/// Check if a user is blocked
pub async fn is_user_blocked(pubkey: String) -> std::result::Result<bool, String> {
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey)?;
    let blocked_users = get_blocked_users().await?;
    Ok(blocked_users.contains(&normalized_pubkey))
}

/// Block a user (add to mute list kind 10000)
/// NIP-51: https://github.com/nostr-protocol/nips/blob/master/51.md
pub async fn block_user(pubkey: String) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey)?;
    log::info!("Blocking user: {}", normalized_pubkey);

    let target_pubkey = nostr::PublicKey::from_hex(&normalized_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Fetch current mute list and extract tags
    let mute_event = fetch_mute_list().await?;
    let (mut tags, existing_content) = match mute_event {
        Some(event) => {
            let content = event.content.clone();
            (extract_mute_list_tags(&event), content)
        }
        None => (MuteListTags::default(), String::new())
    };

    // Add new blocked user if not already present
    if !tags.pubkeys.contains(&target_pubkey) {
        tags.pubkeys.push(target_pubkey);
    }

    let all_tags = rebuild_mute_list_tags(&tags);
    let builder = nostr::EventBuilder::new(nostr::Kind::from(10000), existing_content).tags(all_tags);

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish mute list: {}", e))?;

    log::info!("User blocked successfully");
    Ok(())
}

/// Unblock a user (remove from mute list)
pub async fn unblock_user(pubkey: String) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey)?;
    log::info!("Unblocking user: {}", normalized_pubkey);

    let target_pubkey = nostr::PublicKey::from_hex(&normalized_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Fetch current mute list and extract tags
    let mute_event = fetch_mute_list().await?
        .ok_or("No mute list found")?;
    let existing_content = mute_event.content.clone();
    let mut tags = extract_mute_list_tags(&mute_event);

    // Remove the target user
    tags.pubkeys.retain(|pk| *pk != target_pubkey);

    let all_tags = rebuild_mute_list_tags(&tags);
    let builder = nostr::EventBuilder::new(nostr::Kind::from(10000), existing_content).tags(all_tags);

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish mute list: {}", e))?;

    log::info!("User unblocked successfully");
    Ok(())
}

// =============================================================================
// Reporting
// =============================================================================

/// Report a post (publish kind 1984 event)
/// NIP-56: https://github.com/nostr-protocol/nips/blob/master/56.md
pub async fn report_post(
    event_id: String,
    author_pubkey: String,
    report_type: String,
    details: Option<String>,
) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Reporting post: {} for: {}", event_id, report_type);

    // Parse event ID and pubkey
    // Use parse() to support both hex and bech32 (note1...) formats
    use nostr::{EventId, PublicKey, Tag};
    let target_event_id = EventId::parse(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    // Use parse() to support both hex and bech32 formats (nostr-sdk pattern)
    let target_pubkey = PublicKey::parse(&author_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Build report event (kind 1984)
    // NIP-56: Required 'p' tag for user, 'e' tag for event, report type as 3rd entry
    let tags = vec![
        Tag::public_key(target_pubkey),
        Tag::custom(
            nostr::TagKind::e(),
            vec![target_event_id.to_hex(), String::new(), report_type],
        ),
    ];

    let content = details.unwrap_or_default();
    let builder = nostr::EventBuilder::new(nostr::Kind::from(1984), content).tags(tags);

    match client.send_event_builder(builder).await {
        Ok(output) => {
            let report_id = output.id().to_hex();
            log::info!("Report published successfully: {}", report_id);
            Ok(report_id)
        }
        Err(e) => {
            log::error!("Failed to publish report: {}", e);
            Err(format!("Failed to publish report: {}", e))
        }
    }
}
