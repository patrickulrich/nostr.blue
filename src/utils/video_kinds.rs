//! Video kind constants for NIP-71 support
//!
//! Regular videos (non-addressable):
//! - Kind 21: Horizontal video
//! - Kind 22: Vertical video (shorts/verts)
//!
//! Addressable videos (NIP-71):
//! - Kind 34235: Horizontal video (addressable)
//! - Kind 34236: Vertical video (addressable)

use nostr_sdk::Kind;

// Regular video kinds
pub const VIDEO_HORIZONTAL: u16 = 21;
pub const VIDEO_VERTICAL: u16 = 22;

// Addressable video kinds (NIP-71)
pub const VIDEO_HORIZONTAL_ADDR: u16 = 34235;
pub const VIDEO_VERTICAL_ADDR: u16 = 34236;

/// All horizontal video kinds (regular + addressable)
pub fn horizontal_kinds() -> Vec<Kind> {
    vec![
        Kind::Custom(VIDEO_HORIZONTAL),
        Kind::Custom(VIDEO_HORIZONTAL_ADDR),
    ]
}

/// All vertical video kinds (regular + addressable)
pub fn vertical_kinds() -> Vec<Kind> {
    vec![
        Kind::Custom(VIDEO_VERTICAL),
        Kind::Custom(VIDEO_VERTICAL_ADDR),
    ]
}

/// All video kinds
pub fn all_video_kinds() -> Vec<Kind> {
    vec![
        Kind::Custom(VIDEO_HORIZONTAL),
        Kind::Custom(VIDEO_VERTICAL),
        Kind::Custom(VIDEO_HORIZONTAL_ADDR),
        Kind::Custom(VIDEO_VERTICAL_ADDR),
    ]
}

/// Check if a kind represents a vertical/portrait video
pub fn is_vertical_video(kind: u16) -> bool {
    kind == VIDEO_VERTICAL || kind == VIDEO_VERTICAL_ADDR
}

/// Check if a kind represents an addressable video
#[allow(dead_code)]
pub fn is_addressable_video(kind: u16) -> bool {
    kind == VIDEO_HORIZONTAL_ADDR || kind == VIDEO_VERTICAL_ADDR
}

/// Extract the primary video URL from an event for deduplication
/// Returns the URL from the first imeta tag, or falls back to content field
pub fn get_video_url(event: &nostr_sdk::Event) -> Option<String> {
    // Try imeta tag first (NIP-71 standard)
    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("imeta") {
            for field in tag_vec.iter().skip(1) {
                if let Some(url) = field.strip_prefix("url ") {
                    return Some(url.to_string());
                }
            }
        }
    }
    // Fallback: check if content is a URL
    let content = event.content.trim();
    if content.starts_with("http://") || content.starts_with("https://") {
        return Some(content.to_string());
    }
    None
}
