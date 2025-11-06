/// Utilities for extracting metadata from NIP-23 article events
use nostr::Event;

/// Extract the article title from tags, with fallback
pub fn get_title(event: &Event) -> String {
    event
        .tags
        .iter()
        .find(|tag| tag.kind().to_string() == "title")
        .and_then(|tag| tag.content())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Untitled Article".to_string())
}

/// Extract the article summary/description from tags
pub fn get_summary(event: &Event) -> Option<String> {
    event
        .tags
        .iter()
        .find(|tag| tag.kind().to_string() == "summary")
        .and_then(|tag| tag.content())
        .map(|s| s.to_string())
}

/// Extract the article cover image URL from tags
pub fn get_image(event: &Event) -> Option<String> {
    event
        .tags
        .iter()
        .find(|tag| tag.kind().to_string() == "image")
        .and_then(|tag| tag.content())
        .map(|s| s.to_string())
}

/// Extract the published_at timestamp from tags
/// Falls back to event created_at if not present
pub fn get_published_at(event: &Event) -> u64 {
    event
        .tags
        .iter()
        .find(|tag| tag.kind().to_string() == "published_at")
        .and_then(|tag| tag.content())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| event.created_at.as_secs())
}

/// Extract hashtags (t tags) from the event
pub fn get_hashtags(event: &Event) -> Vec<String> {
    event
        .tags
        .hashtags()
        .map(|s| s.to_string())
        .collect()
}

/// Extract the article identifier (d tag)
/// This is required for NIP-23 articles
pub fn get_identifier(event: &Event) -> Option<String> {
    event
        .tags
        .identifier()
        .map(|s| s.to_string())
}

/// Calculate estimated read time in minutes based on word count
/// Uses 200 words per minute as the reading speed
pub fn calculate_read_time(content: &str) -> usize {
    let word_count = content.split_whitespace().count();
    let minutes = (word_count as f64 / 200.0).ceil() as usize;
    minutes.max(1) // Minimum 1 minute
}

/// Generate a preview of the content (first N characters, character-aware)
#[allow(dead_code)]
pub fn get_content_preview(content: &str, max_chars: usize) -> String {
    let char_count = content.chars().count();
    if char_count <= max_chars {
        content.to_string()
    } else {
        // Collect first max_chars characters
        let truncated: String = content.chars().take(max_chars).collect();
        // Try to truncate at word boundary
        if let Some(last_space) = truncated.rfind(' ') {
            format!("{}...", &truncated[..last_space])
        } else {
            format!("{}...", truncated)
        }
    }
}

/// Get the article coordinate in the format kind:pubkey:identifier
/// Returns None if the article doesn't have a valid identifier
#[allow(dead_code)]
pub fn get_coordinate(event: &Event) -> Option<String> {
    let identifier = get_identifier(event)?;
    Some(format!("{}:{}:{}",
        event.kind.as_u16(),
        event.pubkey.to_hex(),
        identifier
    ))
}

/// Convert coordinate to naddr (NIP-19 format)
/// This is used for creating shareable links with relay hints
#[allow(dead_code)]
pub fn coordinate_to_naddr(kind: u16, pubkey: &str, identifier: &str, relays: Vec<String>) -> Result<String, String> {
    use nostr::prelude::*;
    use nostr::types::url::RelayUrl;

    let pk = PublicKey::from_hex(pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let coord = Coordinate::new(Kind::from(kind), pk)
        .identifier(identifier);

    // Parse relay URLs
    let relay_urls: Result<Vec<RelayUrl>, _> = relays
        .into_iter()
        .map(|r| RelayUrl::parse(&r))
        .collect();

    let relay_urls = relay_urls
        .map_err(|e| format!("Invalid relay URL: {}", e))?;

    // Create Nip19Coordinate with relay hints
    let nip19_coord = Nip19Coordinate::new(coord, relay_urls);

    // Encode to bech32 naddr string
    nip19_coord.to_bech32()
        .map_err(|e| format!("Failed to encode naddr: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_read_time() {
        assert_eq!(calculate_read_time(""), 1); // Minimum 1 minute
        assert_eq!(calculate_read_time(&"word ".repeat(200)), 1); // 200 words = 1 min
        assert_eq!(calculate_read_time(&"word ".repeat(400)), 2); // 400 words = 2 min
        assert_eq!(calculate_read_time(&"word ".repeat(250)), 2); // 250 words = 2 min (ceil)
    }

    #[test]
    fn test_get_content_preview() {
        let short = "Short content";
        assert_eq!(get_content_preview(short, 100), "Short content");

        let long = "This is a very long piece of content that should be truncated";
        let preview = get_content_preview(long, 30);
        assert!(preview.len() <= 33); // 30 + "..."
        assert!(preview.ends_with("..."));
    }
}
