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
/// Uses SDK's DedupVal pattern with Vec<Option<T>> to preserve ordering.
/// Best variant is placed at first occurrence position, avoiding swap_remove corruption.
pub fn dedupe_videos_by_url(events: Vec<nostr_sdk::Event>) -> Vec<nostr_sdk::Event> {
    use std::collections::{HashMap, HashSet};

    if events.is_empty() {
        return events;
    }

    // Track first occurrence and best variant for each URL (SDK DedupVal pattern)
    struct DedupVal {
        first_idx: usize,
        best_idx: usize,
    }

    let mut url_map: HashMap<String, DedupVal> = HashMap::new();
    // Only track IDs of events that have URLs (fixes bug where non-URL events were skipped)
    let mut seen_url_ids: HashSet<nostr_sdk::EventId> = HashSet::new();

    for (idx, event) in events.iter().enumerate() {
        // Skip if we've already processed this exact event ID with a URL
        if seen_url_ids.contains(&event.id) {
            continue;
        }

        if let Some(url) = get_video_url(event) {
            // Only mark ID as seen when it HAS a URL
            seen_url_ids.insert(event.id);

            match url_map.entry(url) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    let val = entry.get_mut();
                    let existing = &events[val.best_idx];
                    // Prefer addressable kinds over regular kinds
                    let incoming_is_better = is_addressable_video(event.kind.as_u16())
                        && !is_addressable_video(existing.kind.as_u16());
                    if incoming_is_better {
                        val.best_idx = idx;
                    }
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(DedupVal {
                        first_idx: idx,
                        best_idx: idx,
                    });
                }
            }
        }
    }

    // Build result: place best variant at first occurrence position (SDK pattern)
    let mut new_list: Vec<Option<nostr_sdk::Event>> = vec![None; events.len()];
    let mut used_indices: HashSet<usize> = HashSet::new();

    for DedupVal {
        first_idx,
        best_idx,
    } in url_map.into_values()
    {
        new_list[first_idx] = Some(events[best_idx].clone());
        used_indices.insert(first_idx);
        used_indices.insert(best_idx);
    }

    // Add events without URLs at their original positions
    // Use separate tracking for non-URL event deduplication
    let mut seen_non_url_ids: HashSet<nostr_sdk::EventId> = HashSet::new();
    for (idx, event) in events.into_iter().enumerate() {
        // Skip if: already processed as URL event, or slot taken
        if used_indices.contains(&idx) || new_list[idx].is_some() {
            continue;
        }
        // Skip URL events (they were handled above)
        if get_video_url(&event).is_some() {
            continue;
        }
        // Skip duplicate non-URL events (SDK pattern: insert returns false if already present)
        if !seen_non_url_ids.insert(event.id) {
            continue;
        }
        used_indices.insert(idx);
        new_list[idx] = Some(event);
    }

    // Flatten, preserving order
    new_list.into_iter().flatten().collect()
}
