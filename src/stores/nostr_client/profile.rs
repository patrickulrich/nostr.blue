//! Metadata (kind 0)
//!
//! Functions for publishing profile metadata (NIP-01).

use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
use nostr::Url;

use super::{get_client, HAS_SIGNER};
use super::types::PublishResult;

// =============================================================================
// Metadata Publishing
// =============================================================================

/// Publish profile metadata (Kind 0) with relay feedback
///
/// Updates the user's Nostr profile with the provided metadata
pub async fn publish_metadata_tracked(metadata: Metadata) -> std::result::Result<PublishResult, String> {
    // Use get_client() helper to avoid holding RwLock across await
    let client = get_client().ok_or("Client not initialized")?;

    // Verify signer is available
    if !*HAS_SIGNER.read() {
        return Err("No signer available".to_string());
    }

    log::info!("Publishing profile metadata");

    // Build event and publish using gossip routing (client handles signing)
    let builder = EventBuilder::metadata(&metadata);
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish metadata: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Metadata published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    // nostr-sdk pattern: Total failure when no relays succeeded (pool/mod.rs)
    // Partial success is OK (some relays worked), but zero success is an error
    if result.success_count() == 0 {
        return Err(format!(
            "Failed to publish metadata: no relays accepted the event (attempted {})",
            result.total_attempted()
        ));
    }

    Ok(result)
}

/// Publish profile metadata (Kind 0)
/// For relay feedback, use publish_metadata_tracked instead
pub async fn publish_metadata(metadata: Metadata) -> std::result::Result<String, String> {
    publish_metadata_tracked(metadata)
        .await
        .map(|result| result.event_id)
}

// =============================================================================
// URL Validation Helper
// =============================================================================

/// Validate a picture/banner URL (http/https only, must have host)
fn validate_picture_url(url: &str) -> std::result::Result<Url, String> {
    let validated_url = Url::parse(url)
        .map_err(|e| format!("Invalid URL: {}", e))?;
    match validated_url.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("Invalid URL scheme '{}': only http/https allowed", scheme)),
    }
    if validated_url.host().is_none() {
        return Err("Invalid URL: missing host".to_string());
    }
    Ok(validated_url)
}

// =============================================================================
// Profile Picture/Banner Updates
// =============================================================================

/// Update just the profile picture
#[allow(dead_code)]
pub async fn update_profile_picture(url: String) -> std::result::Result<(), String> {
    // Fetch current metadata - require existing profile to avoid clobbering data
    let pubkey_str = crate::stores::auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let current_metadata = crate::stores::profiles::get_profile(&pubkey_str)
        .ok_or("Profile not loaded; fetch metadata first")?;

    // Validate URL using extracted helper
    validate_picture_url(&url)?;

    // Update picture field
    let updated_metadata = Metadata {
        picture: Some(url),
        ..current_metadata
    };

    publish_metadata(updated_metadata).await?;
    Ok(())
}

/// Update just the profile banner
#[allow(dead_code)]
pub async fn update_profile_banner(url: String) -> std::result::Result<(), String> {
    // Fetch current metadata - require existing profile to avoid clobbering data
    let pubkey_str = crate::stores::auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let current_metadata = crate::stores::profiles::get_profile(&pubkey_str)
        .ok_or("Profile not loaded; fetch metadata first")?;

    // Validate URL using extracted helper
    validate_picture_url(&url)?;

    // Update banner field
    let updated_metadata = Metadata {
        banner: Some(url),
        ..current_metadata
    };

    publish_metadata(updated_metadata).await?;
    Ok(())
}
