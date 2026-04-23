//! Metadata (kind 0)
//!
//! Functions for publishing profile metadata (NIP-01).
use super::types::PublishResult;
use super::{get_client, HAS_SIGNER};
use dioxus::prelude::ReadableExt;
use nostr::Url;
use nostr_sdk::prelude::*;
/// Publish profile metadata (Kind 0) with relay feedback
///
/// Updates the user's Nostr profile with the provided metadata
pub async fn publish_metadata_tracked(
    metadata: Metadata,
) -> std::result::Result<PublishResult, String> {
    let _client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer available".to_string());
    }
    log::info!("Publishing profile metadata");
    let builder = EventBuilder::metadata(&metadata);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign metadata: {}", e))?;
    let event_id = event.id.to_hex();
    let queue_id = crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Profile,
        None,
        std::collections::HashMap::new(),
    ).await;
    let result = PublishResult::queued(queue_id, event_id);
    log::info!("Metadata queued: {}", result.event_id);
    Ok(result)
}
/// Publish profile metadata (Kind 0)
/// For relay feedback, use publish_metadata_tracked instead
pub async fn publish_metadata(metadata: Metadata) -> std::result::Result<String, String> {
    publish_metadata_tracked(metadata)
        .await
        .map(|result| result.event_id)
}
/// Validate a picture/banner URL (http/https only, must have host)
fn validate_picture_url(url: &str) -> std::result::Result<Url, String> {
    let validated_url = Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
    match validated_url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "Invalid URL scheme '{}': only http/https allowed",
                scheme
            ));
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
async fn update_profile_field(field: &str, url: String) -> std::result::Result<(), String> {
    let pubkey_str = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let cached_profile = crate::stores::profiles::PROFILE_CACHE
        .read()
        .peek(&pubkey_str)
        .cloned()
        .ok_or("Profile not loaded; fetch metadata first")?;
    validate_picture_url(&url)?;
    let updated_metadata = if let Some(json) = cached_profile.raw_metadata_json {
        let mut value: serde_json::Value =
            serde_json::from_str(&json).map_err(|e| format!("Invalid metadata JSON: {}", e))?;
        value[field] = serde_json::Value::String(url);
        serde_json::from_value(value)
            .map_err(|e| format!("Failed to parse updated metadata: {}", e))?
    } else {
        let current_metadata =
            crate::stores::profiles::get_profile(&pubkey_str).ok_or("Profile not loaded")?;
        match field {
            "picture" => Metadata {
                picture: Some(url),
                ..current_metadata
            },
            "banner" => Metadata {
                banner: Some(url),
                ..current_metadata
            },
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
