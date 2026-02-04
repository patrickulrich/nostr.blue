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
pub fn is_addressable_video(kind: u16) -> bool {
    kind == VIDEO_HORIZONTAL_ADDR || kind == VIDEO_VERTICAL_ADDR
}

/// Extract the primary video URL from an event for deduplication
/// Returns the URL from the first imeta tag, or falls back to content field
pub fn get_video_url(event: &nostr_sdk::Event) -> Option<String> {
    // Try imeta tag first (NIP-71 standard)
    for tag in event.tags.iter() {
        let tag_slice = tag.as_slice();
        if tag_slice.first().map(|s| s.as_str()) == Some("imeta") {
            for field in tag_slice.iter().skip(1) {
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

/// Deduplicate video events by URL within a batch
/// When the same video exists as both regular (21/22) and addressable (34235/34236),
/// keeps only one instance. Prefers addressable kinds as they have more metadata.
/// Events without extractable URLs are kept (deduplicated by event ID only).
///
/// Optimized to use O(1) HashMap index lookups and swap_remove for replacements,
/// avoiding O(n) position() and remove() operations.
pub fn dedupe_videos_by_url(events: Vec<nostr_sdk::Event>) -> Vec<nostr_sdk::Event> {
    use std::collections::{HashMap, HashSet};

    let mut seen_url_index: HashMap<String, usize> = HashMap::new();
    let mut seen_ids: HashSet<nostr_sdk::EventId> = HashSet::new();
    let mut result: Vec<nostr_sdk::Event> = Vec::new();

    for event in events {
        // Skip if we've seen this exact event ID
        if seen_ids.contains(&event.id) {
            continue;
        }

        if let Some(url) = get_video_url(&event) {
            if let Some(&existing_idx) = seen_url_index.get(&url) {
                let existing = &result[existing_idx];
                // Prefer addressable kinds (34235/34236) over regular (21/22)
                // as they typically have more metadata
                let keep_existing = is_addressable_video(existing.kind.as_u16())
                    && !is_addressable_video(event.kind.as_u16());
                if keep_existing {
                    continue;
                }
                // Replace: swap_remove the existing and update indices
                let removed_id = result[existing_idx].id;
                result.swap_remove(existing_idx);
                seen_ids.remove(&removed_id);
                // Update index of the element that was swapped in (if any)
                if existing_idx < result.len() {
                    if let Some(swapped_url) = get_video_url(&result[existing_idx]) {
                        seen_url_index.insert(swapped_url, existing_idx);
                    }
                }
            }
            seen_url_index.insert(url, result.len());
        }

        seen_ids.insert(event.id);
        result.push(event);
    }

    result
}
