//! Pin Preview Metadata Fetching
//!
//! Utilities for fetching metadata (title, image, description) from URLs and Nostr events
//! for pin creation previews.
use crate::stores::nostr_client;
use crate::utils::url_metadata::fetch_url_metadata;
use nostr_sdk::prelude::*;
use once_cell::sync::Lazy;
use regex::Regex;
use std::time::Duration;
/// Unified metadata result for pin previews
#[derive(Clone, Debug, Default)]
pub struct PinPreviewMetadata {
    /// Title of the content
    pub title: Option<String>,
    /// Description or summary
    pub description: Option<String>,
    /// Primary image URL
    pub image: Option<String>,
    /// Source type label (e.g., "Link", "Note", "Article")
    pub source_type: String,
}
/// Fetch metadata for a URL (OpenGraph/Twitter Card)
pub async fn fetch_url_preview(url: &str) -> Result<PinPreviewMetadata, String> {
    let meta = fetch_url_metadata(url.to_string()).await?;
    Ok(PinPreviewMetadata {
        title: meta.title,
        description: meta.description,
        image: meta.image,
        source_type: "Link".to_string(),
    })
}
/// Fetch metadata for a Nostr event by ID (note1, nevent1, or hex)
pub async fn fetch_event_preview(event_id_str: &str) -> Result<PinPreviewMetadata, String> {
    let event_id = parse_event_id(event_id_str)?;
    let filter = Filter::new().id(event_id).limit(1);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;
    let event = events.first().ok_or("Event not found on relays")?;
    Ok(extract_event_metadata(event))
}
/// Fetch metadata for an addressable event (naddr1)
pub async fn fetch_address_preview(naddr: &str) -> Result<PinPreviewMetadata, String> {
    let normalized = naddr.trim().strip_prefix("nostr:").unwrap_or(naddr.trim());
    let coord = Coordinate::from_bech32(normalized).map_err(|e| format!("Invalid naddr: {}", e))?;
    let filter = Filter::new()
        .kind(coord.kind)
        .author(coord.public_key)
        .identifier(&coord.identifier)
        .limit(1);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;
    let event = events.first().ok_or("Event not found on relays")?;
    Ok(extract_addressable_metadata(event, coord.kind.as_u16()))
}
/// Parse an event ID from various formats (note1, nevent1, hex)
fn parse_event_id(input: &str) -> Result<EventId, String> {
    let trimmed = input.trim();
    let normalized = trimmed.strip_prefix("nostr:").unwrap_or(trimmed);
    if normalized.starts_with("nevent1") {
        let nip19 = Nip19::from_bech32(normalized).map_err(|e| format!("Invalid nevent: {}", e))?;
        match nip19 {
            Nip19::Event(nip19_event) => Ok(nip19_event.event_id),
            _ => Err("Not an event reference".to_string()),
        }
    } else if normalized.starts_with("note1") {
        EventId::from_bech32(normalized).map_err(|e| format!("Invalid note: {}", e))
    } else {
        EventId::from_hex(normalized).map_err(|e| format!("Invalid event ID: {}", e))
    }
}
/// Extract metadata from a regular note (Kind 1)
fn extract_event_metadata(event: &Event) -> PinPreviewMetadata {
    let kind = event.kind.as_u16();
    if (30000..40000).contains(&kind) {
        return extract_addressable_metadata(event, kind);
    }
    let image =
        extract_imeta_image(event).or_else(|| extract_first_image_from_content(&event.content));
    let title = event
        .content
        .lines()
        .next()
        .map(|s| {
            let cleaned = s.trim();
            if cleaned.len() > 100 {
                format!("{}...", &cleaned.chars().take(100).collect::<String>())
            } else {
                cleaned.to_string()
            }
        })
        .filter(|s| !s.is_empty());
    let description = if event.content.len() > 500 {
        Some(format!(
            "{}...",
            &event.content.chars().take(500).collect::<String>()
        ))
    } else {
        Some(event.content.clone())
    };
    let source_type = match kind {
        1 => "Note",
        6 => "Repost",
        7 => "Reaction",
        20 => "Picture",
        21 | 22 => "Video",
        _ => "Event",
    }
    .to_string();
    PinPreviewMetadata {
        title,
        description,
        image,
        source_type,
    }
}
/// Extract metadata from addressable events (Kind 30000+)
fn extract_addressable_metadata(event: &Event, kind: u16) -> PinPreviewMetadata {
    let title = extract_tag_value(&event.tags, "title");
    let image = extract_tag_value(&event.tags, "image")
        .or_else(|| extract_imeta_image(event))
        .or_else(|| extract_first_image_from_content(&event.content));
    let summary = extract_tag_value(&event.tags, "summary");
    let description = summary.or_else(|| {
        if event.content.len() > 500 {
            Some(format!(
                "{}...",
                &event.content.chars().take(500).collect::<String>(),
            ))
        } else if !event.content.is_empty() {
            Some(event.content.clone())
        } else {
            None
        }
    });
    let source_type = match kind {
        30023 => {
            let is_recipe = event.tags.iter().any(|t| {
                let slice = t.as_slice();
                slice.first().map(|s| s.as_str()) == Some("t")
                    && slice
                        .get(1)
                        .map(|s| s.to_lowercase().contains("nostrcooking"))
                        .unwrap_or(false)
            });
            if is_recipe {
                "Recipe"
            } else {
                "Article"
            }
        }
        30078 => "Recipe",
        30067 => "Pinboard",
        34550 => "Community",
        30617 => "Repository",
        31922 | 31923 => "Event",
        30311 => "Live Stream",
        30009 => "Badge",
        31337 | 32267 => "Music",
        30818 => "Wiki",
        30040 | 30041 => "Publication",
        _ => "Event",
    }
    .to_string();
    PinPreviewMetadata {
        title,
        description,
        image,
        source_type,
    }
}
/// Extract image URL from imeta tags (NIP-92)
fn extract_imeta_image(event: &Event) -> Option<String> {
    for tag in event.tags.iter() {
        let tag_slice = tag.as_slice();
        if tag_slice.first().map(|s| s.as_str()) == Some("imeta") {
            for field in tag_slice.iter().skip(1) {
                if let Some(url) = field.strip_prefix("url ") {
                    if is_image_url(url) {
                        return Some(url.to_string());
                    }
                }
            }
        }
    }
    None
}
/// Extract first image URL from note content using regex
fn extract_first_image_from_content(content: &str) -> Option<String> {
    static URL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"https?://[^\s\)\]]+").unwrap());
    for mat in URL_REGEX.find_iter(content) {
        let url = mat
            .as_str()
            .trim_end_matches(['.', ',', ')', ']', '>', '"', '\'']);
        if is_image_url(url) {
            return Some(url.to_string());
        }
    }
    None
}
/// Check if a URL points to an image based on extension
fn is_image_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    let extensions = [
        ".jpg", ".jpeg", ".png", ".gif", ".webp", ".svg", ".bmp", ".avif",
    ];
    extensions
        .iter()
        .any(|ext| lower.ends_with(ext) || lower.contains(&format!("{ext}?")))
}
/// Extract a tag value by name (first match)
fn extract_tag_value(tags: &Tags, name: &str) -> Option<String> {
    for tag in tags.iter() {
        let vec = tag.as_slice();
        if vec.first().map(|s| s.as_str()) == Some(name) {
            return vec.get(1).map(|s| s.to_string()).filter(|s| !s.is_empty());
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_is_image_url() {
        assert!(is_image_url("https://example.com/image.jpg"));
        assert!(is_image_url("https://example.com/image.PNG"));
        assert!(is_image_url("https://example.com/image.webp?w=400"));
        assert!(!is_image_url("https://example.com/page.html"));
        assert!(!is_image_url("https://example.com/video.mp4"));
    }
    #[test]
    fn test_extract_first_image_from_content() {
        let content = "Check out this cool image https://example.com/photo.jpg and more text";
        assert_eq!(
            extract_first_image_from_content(content),
            Some("https://example.com/photo.jpg".to_string()),
        );
        let no_image = "Just text without any images";
        assert_eq!(extract_first_image_from_content(no_image), None);
    }
}
