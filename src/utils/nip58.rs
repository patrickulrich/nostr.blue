//! NIP-58 Badge Event Parsing Utilities
//!
//! Handles parsing and creation of badge events:
//! - Kind 30009: Badge Definition (addressable)
//! - Kind 8: Badge Award (regular)
//! - Kind 30008: Profile Badges (addressable)
//!
//! NIP-58 defines a standard for badges that can be issued, awarded, and displayed
//! on user profiles.

use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::stores::nostr_client;

// ============================================================================
// Event Kind Constants
// ============================================================================

/// Badge definition event kind (addressable)
pub const KIND_BADGE_DEFINITION: u16 = 30009;

/// Badge award event kind (regular)
pub const KIND_BADGE_AWARD: u16 = 8;

/// Profile badges event kind (addressable)
pub const KIND_PROFILE_BADGES: u16 = 30008;

/// Standard d-tag for profile badges
pub const PROFILE_BADGES_D_TAG: &str = "profile_badges";

// ============================================================================
// Data Structures
// ============================================================================

/// Badge definition parsed from a Kind 30009 event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BadgeDefinition {
    /// Unique badge ID (d-tag)
    pub id: String,
    /// Badge name (name tag)
    pub name: Option<String>,
    /// Badge description (description tag)
    pub description: Option<String>,
    /// High-res image URL (image tag)
    pub image: Option<String>,
    /// Image dimensions (WxH from image tag)
    pub image_dimensions: Option<String>,
    /// Thumbnail image URL (thumb tag)
    pub thumb: Option<String>,
    /// Thumbnail dimensions (WxH from thumb tag)
    pub thumb_dimensions: Option<String>,
    /// Event ID (hex)
    pub event_id: String,
    /// Issuer pubkey (hex)
    pub pubkey: String,
    /// NIP-19 naddr for this badge
    pub naddr: String,
    /// Coordinate string: "30009:pubkey:d-tag"
    pub coordinate: String,
    /// Created timestamp
    pub created_at: u64,
}

impl BadgeDefinition {
    /// Get the best image URL (prefer thumb, fall back to image)
    pub fn get_thumbnail(&self) -> Option<&str> {
        self.thumb.as_deref().or(self.image.as_deref())
    }

    /// Get the full image URL (prefer image, fall back to thumb)
    pub fn get_image(&self) -> Option<&str> {
        self.image.as_deref().or(self.thumb.as_deref())
    }

    /// Get display name (prefer name, fall back to id)
    pub fn get_display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}

/// Badge award parsed from a Kind 8 event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BadgeAward {
    /// Event ID (hex)
    pub event_id: String,
    /// Issuer pubkey (hex)
    pub issuer: String,
    /// Badge definition coordinate (a-tag value: "30009:pubkey:d-tag")
    pub definition_coordinate: String,
    /// Awardee pubkeys (hex)
    pub awardees: Vec<String>,
    /// Created timestamp
    pub created_at: u64,
}

/// Entry in a user's profile badges (kind 30008)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileBadgeEntry {
    /// Badge definition coordinate (a-tag: "30009:issuer:badge_id")
    pub definition_coordinate: String,
    /// Award event ID (e-tag)
    pub award_event_id: String,
}

// ============================================================================
// Parsing Functions
// ============================================================================

/// Parse a badge definition from a Kind 30009 event
pub fn parse_badge_definition(event: &Event) -> Result<BadgeDefinition, String> {
    if event.kind.as_u16() != KIND_BADGE_DEFINITION {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_BADGE_DEFINITION,
            event.kind.as_u16()
        ));
    }

    let pubkey = event.pubkey.to_hex();

    // Extract required d-tag (badge ID)
    let id = get_tag_value(event, "d").ok_or("Missing required 'd' tag (badge ID)")?;

    // Build coordinate and naddr
    let coordinate = format!("{}:{}:{}", KIND_BADGE_DEFINITION, pubkey, id);
    let naddr = build_badge_naddr(&pubkey, &id).unwrap_or_default();

    // Optional fields
    let name = get_tag_value(event, "name");
    let description = get_tag_value(event, "description");

    // Image tag: ["image", "url", "WxH"] or just ["image", "url"]
    let (image, image_dimensions) = get_tag_with_dimensions(event, "image");

    // Thumb tag: ["thumb", "url", "WxH"] or just ["thumb", "url"]
    let (thumb, thumb_dimensions) = get_tag_with_dimensions(event, "thumb");

    Ok(BadgeDefinition {
        id,
        name,
        description,
        image,
        image_dimensions,
        thumb,
        thumb_dimensions,
        event_id: event.id.to_hex(),
        pubkey,
        naddr,
        coordinate,
        created_at: event.created_at.as_secs(),
    })
}

/// Parse a badge award from a Kind 8 event
pub fn parse_badge_award(event: &Event) -> Result<BadgeAward, String> {
    if event.kind.as_u16() != KIND_BADGE_AWARD {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_BADGE_AWARD,
            event.kind.as_u16()
        ));
    }

    // Extract required a-tag (badge definition coordinate)
    let definition_coordinate =
        get_tag_value(event, "a").ok_or("Missing required 'a' tag (badge definition)")?;

    // Validate it's a badge definition coordinate
    if !definition_coordinate.starts_with(&format!("{}:", KIND_BADGE_DEFINITION)) {
        return Err(format!(
            "Invalid badge definition coordinate: {}",
            definition_coordinate
        ));
    }

    // Extract all p-tags (awardees)
    let awardees = get_all_tag_values(event, "p");
    if awardees.is_empty() {
        return Err("Missing required 'p' tags (awardees)".to_string());
    }

    Ok(BadgeAward {
        event_id: event.id.to_hex(),
        issuer: event.pubkey.to_hex(),
        definition_coordinate,
        awardees,
        created_at: event.created_at.as_secs(),
    })
}

/// Parse profile badges from a Kind 30008 event
/// Returns pairs of (definition_coordinate, award_event_id)
pub fn parse_profile_badges(event: &Event) -> Result<Vec<ProfileBadgeEntry>, String> {
    if event.kind.as_u16() != KIND_PROFILE_BADGES {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_PROFILE_BADGES,
            event.kind.as_u16()
        ));
    }

    // Verify d-tag is "profile_badges"
    let d_tag = get_tag_value(event, "d");
    if d_tag.as_deref() != Some(PROFILE_BADGES_D_TAG) {
        return Err(format!(
            "Expected d-tag '{}', got {:?}",
            PROFILE_BADGES_D_TAG, d_tag
        ));
    }

    // Parse consecutive a/e tag pairs
    // Per NIP-58: tags should be ordered as [a, e, a, e, ...]
    let mut entries = Vec::new();
    let tags: Vec<_> = event.tags.iter().collect();

    let mut i = 0;
    while i < tags.len() {
        let tag = &tags[i];
        let tag_slice = tag.as_slice();

        // Look for a-tag
        if tag_slice.first().map(|s| s.as_str()) == Some("a") {
            if let Some(coordinate) = tag_slice.get(1) {
                // Check if next tag is e-tag
                if i + 1 < tags.len() {
                    let next_tag = &tags[i + 1];
                    let next_slice = next_tag.as_slice();
                    if next_slice.first().map(|s| s.as_str()) == Some("e") {
                        if let Some(event_id) = next_slice.get(1) {
                            entries.push(ProfileBadgeEntry {
                                definition_coordinate: coordinate.to_string(),
                                award_event_id: event_id.to_string(),
                            });
                            i += 2; // Skip both tags
                            continue;
                        }
                    }
                }
            }
        }
        i += 1;
    }

    Ok(entries)
}

/// Build a NIP-19 naddr for a badge definition
pub fn build_badge_naddr(pubkey: &str, badge_id: &str) -> Option<String> {
    let pk = PublicKey::from_hex(pubkey).ok()?;
    let coordinate = Coordinate::new(Kind::Custom(KIND_BADGE_DEFINITION), pk).identifier(badge_id);
    let nip19 = Nip19Coordinate::new(coordinate, vec![]);
    nip19.to_bech32().ok()
}

/// Parse a badge naddr back to (pubkey, badge_id)
pub fn parse_badge_naddr(naddr: &str) -> Result<(String, String), String> {
    let nip19 = Nip19Coordinate::from_bech32(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;

    if nip19.coordinate.kind.as_u16() != KIND_BADGE_DEFINITION {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_BADGE_DEFINITION,
            nip19.coordinate.kind.as_u16()
        ));
    }

    let pubkey = nip19.coordinate.public_key.to_hex();
    let badge_id = nip19.coordinate.identifier.clone();

    Ok((pubkey, badge_id))
}

// ============================================================================
// Fetching Functions
// ============================================================================

/// Fetch a badge definition by naddr
pub async fn fetch_badge_definition(naddr: &str) -> Result<BadgeDefinition, String> {
    let (pubkey, badge_id) = parse_badge_naddr(naddr)?;

    let pk = PublicKey::from_hex(&pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::Custom(KIND_BADGE_DEFINITION))
        .author(pk)
        .identifier(&badge_id)
        .limit(1);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;

    events
        .into_iter()
        .filter_map(|e| parse_badge_definition(&e).ok())
        .max_by_key(|b| b.created_at)
        .ok_or_else(|| "Badge not found".to_string())
}

/// Fetch a badge definition by coordinate string ("30009:pubkey:badge_id")
pub async fn fetch_badge_by_coordinate(coordinate: &str) -> Result<BadgeDefinition, String> {
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() != 3 {
        return Err(format!("Invalid coordinate format: {}", coordinate));
    }

    let kind_str = parts[0];
    let pubkey = parts[1];
    let badge_id = parts[2];

    if kind_str != KIND_BADGE_DEFINITION.to_string() {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_BADGE_DEFINITION, kind_str
        ));
    }

    let pk = PublicKey::from_hex(pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::Custom(KIND_BADGE_DEFINITION))
        .author(pk)
        .identifier(badge_id)
        .limit(1);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;

    events
        .into_iter()
        .filter_map(|e| parse_badge_definition(&e).ok())
        .max_by_key(|b| b.created_at)
        .ok_or_else(|| "Badge not found".to_string())
}

/// Fetch profile badges for a user (accepted badges)
pub async fn fetch_profile_badges(pubkey: &str) -> Result<Vec<ProfileBadgeEntry>, String> {
    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::Custom(KIND_PROFILE_BADGES))
        .author(pk)
        .identifier(PROFILE_BADGES_D_TAG)
        .limit(1);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;

    // Get the most recent profile badges event
    if let Some(event) = events.into_iter().max_by_key(|e| e.created_at.as_secs()) {
        parse_profile_badges(&event)
    } else {
        Ok(Vec::new()) // No profile badges set
    }
}

/// Fetch badge awards where the user is an awardee
pub async fn fetch_pending_badge_awards(pubkey: &str) -> Result<Vec<BadgeAward>, String> {
    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::Custom(KIND_BADGE_AWARD))
        .pubkey(pk) // p-tag filter
        .limit(100);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;

    Ok(events
        .into_iter()
        .filter_map(|e| parse_badge_award(&e).ok())
        .collect())
}

/// Fetch all badge definitions (for discovery)
pub async fn fetch_recent_badges(limit: usize) -> Result<Vec<BadgeDefinition>, String> {
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_BADGE_DEFINITION))
        .limit(limit);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;

    Ok(events
        .into_iter()
        .filter_map(|e| parse_badge_definition(&e).ok())
        .collect())
}

/// Fetch badge definitions created by a specific user
pub async fn fetch_badges_by_issuer(pubkey: &str) -> Result<Vec<BadgeDefinition>, String> {
    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::Custom(KIND_BADGE_DEFINITION))
        .author(pk)
        .limit(100);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;

    Ok(events
        .into_iter()
        .filter_map(|e| parse_badge_definition(&e).ok())
        .collect())
}

// ============================================================================
// Publishing Functions
// ============================================================================

/// Create and publish a badge definition (kind 30009)
pub async fn create_badge_definition(
    badge_id: &str,
    name: Option<&str>,
    description: Option<&str>,
    image_url: Option<&str>,
    image_dimensions: Option<&str>,
    thumb_url: Option<&str>,
    thumb_dimensions: Option<&str>,
) -> Result<String, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    if !nostr_client::has_signer() {
        return Err("No signer available".to_string());
    }

    let mut tags = vec![Tag::identifier(badge_id)];

    if let Some(name) = name {
        tags.push(Tag::custom(TagKind::Name, vec![name.to_string()]));
    }

    if let Some(description) = description {
        tags.push(Tag::custom(
            TagKind::Custom("description".into()),
            vec![description.to_string()],
        ));
    }

    if let Some(image) = image_url {
        let mut image_tag = vec![image.to_string()];
        if let Some(dims) = image_dimensions {
            image_tag.push(dims.to_string());
        }
        tags.push(Tag::custom(TagKind::Image, image_tag));
    }

    if let Some(thumb) = thumb_url {
        let mut thumb_tag = vec![thumb.to_string()];
        if let Some(dims) = thumb_dimensions {
            thumb_tag.push(dims.to_string());
        }
        tags.push(Tag::custom(
            TagKind::Custom("thumb".into()),
            thumb_tag,
        ));
    }

    let event = EventBuilder::new(Kind::Custom(KIND_BADGE_DEFINITION), "")
        .tags(tags);

    let output = client
        .send_event_builder(event)
        .await
        .map_err(|e| format!("Failed to publish badge: {}", e))?;

    Ok(output.id().to_hex())
}

/// Award a badge to one or more users (kind 8)
#[allow(dead_code)]
pub async fn award_badge(
    definition_coordinate: &str,
    awardees: Vec<&str>,
) -> Result<String, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    if !nostr_client::has_signer() {
        return Err("No signer available".to_string());
    }

    // Validate coordinate format
    if !definition_coordinate.starts_with(&format!("{}:", KIND_BADGE_DEFINITION)) {
        return Err(format!(
            "Invalid badge coordinate: {}",
            definition_coordinate
        ));
    }

    let mut tags = vec![Tag::custom(
        TagKind::Custom("a".into()),
        vec![definition_coordinate.to_string()],
    )];

    // Add p-tags for each awardee
    for awardee in awardees {
        let pk = PublicKey::from_hex(awardee)
            .or_else(|_| PublicKey::from_bech32(awardee))
            .map_err(|e| format!("Invalid awardee pubkey {}: {}", awardee, e))?;
        tags.push(Tag::public_key(pk));
    }

    let event = EventBuilder::new(Kind::Custom(KIND_BADGE_AWARD), "")
        .tags(tags);

    let output = client
        .send_event_builder(event)
        .await
        .map_err(|e| format!("Failed to award badge: {}", e))?;

    Ok(output.id().to_hex())
}

/// Accept a badge (add to profile badges)
pub async fn accept_badge(
    definition_coordinate: &str,
    award_event_id: &str,
) -> Result<String, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    if !nostr_client::has_signer() {
        return Err("No signer available".to_string());
    }

    // Get current user's pubkey
    let signer = client.signer().await.map_err(|e| format!("No signer: {}", e))?;
    let pubkey = signer
        .get_public_key()
        .await
        .map_err(|e| format!("Failed to get pubkey: {}", e))?;

    // Fetch existing profile badges
    let existing = fetch_profile_badges(&pubkey.to_hex()).await.unwrap_or_default();

    // Check if already accepted
    if existing
        .iter()
        .any(|e| e.definition_coordinate == definition_coordinate)
    {
        return Err("Badge already accepted".to_string());
    }

    // Build new profile badges event with all existing + new badge
    let mut tags = vec![Tag::identifier(PROFILE_BADGES_D_TAG)];

    // Add existing badges
    for entry in &existing {
        tags.push(Tag::custom(
            TagKind::Custom("a".into()),
            vec![entry.definition_coordinate.clone()],
        ));
        tags.push(Tag::custom(
            TagKind::Custom("e".into()),
            vec![entry.award_event_id.clone()],
        ));
    }

    // Add new badge
    tags.push(Tag::custom(
        TagKind::Custom("a".into()),
        vec![definition_coordinate.to_string()],
    ));
    tags.push(Tag::custom(
        TagKind::Custom("e".into()),
        vec![award_event_id.to_string()],
    ));

    let event = EventBuilder::new(Kind::Custom(KIND_PROFILE_BADGES), "")
        .tags(tags);

    let output = client
        .send_event_builder(event)
        .await
        .map_err(|e| format!("Failed to accept badge: {}", e))?;

    Ok(output.id().to_hex())
}

/// Reject/remove a badge from profile badges
pub async fn reject_badge(definition_coordinate: &str) -> Result<String, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    if !nostr_client::has_signer() {
        return Err("No signer available".to_string());
    }

    // Get current user's pubkey
    let signer = client.signer().await.map_err(|e| format!("No signer: {}", e))?;
    let pubkey = signer
        .get_public_key()
        .await
        .map_err(|e| format!("Failed to get pubkey: {}", e))?;

    // Fetch existing profile badges
    let existing = fetch_profile_badges(&pubkey.to_hex()).await.unwrap_or_default();

    // Filter out the badge to reject
    let remaining: Vec<_> = existing
        .iter()
        .filter(|e| e.definition_coordinate != definition_coordinate)
        .collect();

    // Build new profile badges event without the rejected badge
    let mut tags = vec![Tag::identifier(PROFILE_BADGES_D_TAG)];

    for entry in remaining {
        tags.push(Tag::custom(
            TagKind::Custom("a".into()),
            vec![entry.definition_coordinate.clone()],
        ));
        tags.push(Tag::custom(
            TagKind::Custom("e".into()),
            vec![entry.award_event_id.clone()],
        ));
    }

    let event = EventBuilder::new(Kind::Custom(KIND_PROFILE_BADGES), "")
        .tags(tags);

    let output = client
        .send_event_builder(event)
        .await
        .map_err(|e| format!("Failed to reject badge: {}", e))?;

    Ok(output.id().to_hex())
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get first value of a tag by name
fn get_tag_value(event: &Event, tag_name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}

/// Get all first values of tags with a given name
fn get_all_tag_values(event: &Event, tag_name: &str) -> Vec<String> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
        .collect()
}

/// Get tag value and optional dimensions (for image/thumb tags)
fn get_tag_with_dimensions(event: &Event, tag_name: &str) -> (Option<String>, Option<String>) {
    if let Some(tag) = event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
    {
        let slice = tag.as_slice();
        let url = slice.get(1).map(|s| s.to_string());
        let dims = slice.get(2).map(|s| s.to_string());
        (url, dims)
    } else {
        (None, None)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_badge_naddr() {
        let pubkey = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let badge_id = "bravery";

        let naddr = build_badge_naddr(pubkey, badge_id);
        assert!(naddr.is_some());
        assert!(naddr.unwrap().starts_with("naddr1"));
    }

    #[test]
    fn test_parse_badge_naddr() {
        let pubkey = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let badge_id = "bravery";

        let naddr = build_badge_naddr(pubkey, badge_id).unwrap();
        let (parsed_pubkey, parsed_id) = parse_badge_naddr(&naddr).unwrap();

        assert_eq!(parsed_pubkey, pubkey);
        assert_eq!(parsed_id, badge_id);
    }
}
