//! Metadata (kind 0)
//!
//! Functions for publishing profile metadata (NIP-01).
use dioxus::prelude::ReadableExt;
use nostr::Url;
use nostr_sdk::prelude::*;
use super::types::PublishResult;
use super::{get_client, HAS_SIGNER};
/// Publish profile metadata (Kind 0) with relay feedback
///
/// Updates the user's Nostr profile with the provided metadata
pub async fn publish_metadata_tracked(
    metadata: Metadata,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer available".to_string());
    }
    log::info!("Publishing profile metadata");
    let builder = EventBuilder::metadata(&metadata);
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish metadata: {}", e))?;
    let result = PublishResult::from_output(output);
    log::info!(
        "Metadata published: {} ({}/{} relays succeeded)", result.event_id, result
        .success_count(), result.total_attempted()
    );
    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }
    if result.success_count() == 0 {
        return Err(
            format!(
                "Failed to publish metadata: no relays accepted the event (attempted {})",
                result.total_attempted(),
            ),
        );
    }
    Ok(result)
}
/// Publish profile metadata (Kind 0)
/// For relay feedback, use publish_metadata_tracked instead
pub async fn publish_metadata(
    metadata: Metadata,
) -> std::result::Result<String, String> {
    publish_metadata_tracked(metadata).await.map(|result| result.event_id)
}
/// Validate a picture/banner URL (http/https only, must have host)
fn validate_picture_url(url: &str) -> std::result::Result<Url, String> {
    let validated_url = Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
    match validated_url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(
                format!("Invalid URL scheme '{}': only http/https allowed", scheme),
            );
        }
    }
    if validated_url.host().is_none() {
        return Err("Invalid URL: missing host".to_string());
    }
    Ok(validated_url)
}
/// Update a single profile field while preserving custom fields
///
/// Uses raw_metadata_json from cached Profile to preserve unknown fields during update.
async fn update_profile_field(
    field: &str,
    url: String,
) -> std::result::Result<(), String> {
    let pubkey_str = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let cached_profile = crate::stores::profiles::PROFILE_CACHE
        .read()
        .peek(&pubkey_str)
        .cloned()
        .ok_or("Profile not loaded; fetch metadata first")?;
    validate_picture_url(&url)?;
    let updated_metadata = if let Some(json) = cached_profile.raw_metadata_json {
        let mut value: serde_json::Value = serde_json::from_str(&json)
            .map_err(|e| format!("Invalid metadata JSON: {}", e))?;
        value[field] = serde_json::Value::String(url);
        serde_json::from_value(value)
            .map_err(|e| format!("Failed to parse updated metadata: {}", e))?
    } else {
        let current_metadata = crate::stores::profiles::get_profile(&pubkey_str)
            .ok_or("Profile not loaded")?;
        match field {
            "picture" => {
                Metadata {
                    picture: Some(url),
                    ..current_metadata
                }
            }
            "banner" => {
                Metadata {
                    banner: Some(url),
                    ..current_metadata
                }
            }
            _ => return Err(format!("Unknown profile field: {}", field)),
        }
    };
    publish_metadata(updated_metadata).await?;
    Ok(())
}
/// Update just the profile picture
///
/// Uses raw_metadata_json from cached Profile to preserve custom fields during update.
#[allow(dead_code)]
pub async fn update_profile_picture(url: String) -> std::result::Result<(), String> {
    update_profile_field("picture", url).await
}
/// Update just the profile banner
///
/// Uses raw_metadata_json from cached Profile to preserve custom fields during update.
#[allow(dead_code)]
pub async fn update_profile_banner(url: String) -> std::result::Result<(), String> {
    update_profile_field("banner", url).await
}
