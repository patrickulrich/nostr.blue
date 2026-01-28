//! Nostr client types
//!
//! Shared types used across the nostr_client module.

// =============================================================================
// PublishResult
// =============================================================================

/// Result of publishing an event, including relay success/failure tracking
/// Enables debugging which relays accepted/rejected events
#[derive(Clone, Debug)]
pub struct PublishResult {
    /// The event ID that was published
    pub event_id: String,
    /// URLs of relays that successfully accepted the event
    pub successful_relays: Vec<String>,
    /// URLs of relays that failed to accept the event (with error messages)
    pub failed_relays: Vec<(String, String)>,
}

impl PublishResult {
    /// Create from SDK Output
    pub fn from_output(output: nostr_relay_pool::Output<nostr::EventId>) -> Self {
        let successful: Vec<String> = output.success
            .iter()
            .map(|url| url.to_string())
            .collect();
        let failed: Vec<(String, String)> = output.failed
            .iter()
            .map(|(url, reason)| (url.to_string(), reason.clone()))
            .collect();

        Self {
            event_id: output.id().to_hex(),
            successful_relays: successful,
            failed_relays: failed,
        }
    }

    /// Get total number of relays attempted
    pub fn total_attempted(&self) -> usize {
        self.successful_relays.len() + self.failed_relays.len()
    }

    /// Get number of successful relays
    pub fn success_count(&self) -> usize {
        self.successful_relays.len()
    }

    /// Check if publish was at least partially successful
    pub fn is_success(&self) -> bool {
        !self.successful_relays.is_empty()
    }

    /// Check if any relays failed
    pub fn has_failures(&self) -> bool {
        !self.failed_relays.is_empty()
    }

    /// Get success rate as percentage (0.0 - 100.0)
    pub fn success_rate(&self) -> f32 {
        let total = self.total_attempted();
        if total == 0 {
            0.0
        } else {
            (self.successful_relays.len() as f32 / total as f32) * 100.0
        }
    }
}

// =============================================================================
// MuteListTags (NIP-51)
// =============================================================================

/// Extracted tag categories from a mute list event (kind 10000)
/// Used to reduce code duplication in mute/unmute/block/unblock operations
#[derive(Default)]
pub(crate) struct MuteListTags {
    pub event_ids: Vec<nostr::EventId>,   // Muted posts (e tags)
    pub pubkeys: Vec<nostr::PublicKey>,   // Blocked users (p tags)
    pub hashtags: Vec<String>,            // Muted hashtags (t tags)
    pub words: Vec<String>,               // Muted words (word tags)
    pub other_tags: Vec<nostr::Tag>,      // Preserve unknown tags
}

/// Extract categorized tags from a kind 10000 mute list event
pub(crate) fn extract_mute_list_tags(event: &nostr::Event) -> MuteListTags {
    let mut tags = MuteListTags::default();

    for tag in event.tags.iter() {
        if tag.kind() == nostr::TagKind::e() {
            if let Some(id) = tag.content() {
                if let Ok(eid) = nostr::EventId::from_hex(id) {
                    tags.event_ids.push(eid);
                }
            }
        } else if tag.kind() == nostr::TagKind::p() {
            if let Some(pk) = tag.content() {
                if let Ok(pubkey) = nostr::PublicKey::from_hex(pk) {
                    tags.pubkeys.push(pubkey);
                }
            }
        } else if tag.kind() == nostr::TagKind::t() {
            if let Some(hashtag) = tag.content() {
                tags.hashtags.push(hashtag.to_string());
            }
        } else if tag.kind() == nostr::TagKind::Custom("word".into()) {
            if let Some(word) = tag.content() {
                tags.words.push(word.to_string());
            }
        } else {
            // Preserve all other tags (e.g., 'a' address tags, future extensions)
            tags.other_tags.push(tag.clone());
        }
    }

    tags
}

/// Rebuild tags vec from categorized structure
pub(crate) fn rebuild_mute_list_tags(tags: &MuteListTags) -> Vec<nostr::Tag> {
    let mut all_tags = Vec::new();

    // Add e tags for muted posts
    for event_id in &tags.event_ids {
        all_tags.push(nostr::Tag::event(*event_id));
    }

    // Add p tags for blocked users
    for pubkey in &tags.pubkeys {
        all_tags.push(nostr::Tag::public_key(*pubkey));
    }

    // Add t tags for hashtags
    for hashtag in &tags.hashtags {
        all_tags.push(nostr::Tag::hashtag(hashtag.clone()));
    }

    // Add word tags
    for word in &tags.words {
        all_tags.push(nostr::Tag::custom(nostr::TagKind::Custom("word".into()), vec![word.clone()]));
    }

    // Re-attach preserved tags
    all_tags.extend(tags.other_tags.clone());

    all_tags
}

// =============================================================================
// Tag Conversion
// =============================================================================

/// Convert raw string tags to nostr::Tag with NIP-10 marker support
/// Follows nostr-sdk pattern: handles flexible field detection for backward compat
pub fn convert_raw_tags(tags: Vec<Vec<String>>) -> Vec<nostr::Tag> {
    use nostr_sdk::nips::nip10::Marker;

    tags.into_iter()
        .filter_map(|tag_vec| {
            if tag_vec.is_empty() {
                return None;
            }
            match tag_vec[0].as_str() {
                "e" if tag_vec.len() >= 4 && !tag_vec[3].is_empty() => {
                    // E-tag with marker (NIP-10)
                    let event_id = nostr::EventId::from_hex(&tag_vec[1]).ok()?;
                    let marker = match tag_vec[3].as_str() {
                        "root" => Some(Marker::Root),
                        "reply" => Some(Marker::Reply),
                        _ => None,
                    };
                    if let Some(m) = marker {
                        let relay_url = if !tag_vec[2].is_empty() {
                            nostr_sdk::RelayUrl::parse(&tag_vec[2]).ok()
                        } else {
                            None
                        };
                        Some(nostr::Tag::from(nostr::TagStandard::Event {
                            event_id,
                            relay_url,
                            marker: Some(m),
                            public_key: None,
                            uppercase: false,
                        }))
                    } else {
                        // Unknown marker (e.g., "mention") - preserve full tag using Tag::custom
                        // This follows nostr-sdk pattern where Tag::custom preserves raw data
                        Some(nostr::Tag::custom(nostr::TagKind::e(), tag_vec[1..].to_vec()))
                    }
                }
                "e" if tag_vec.len() >= 2 => {
                    // Simple e-tag - preserve trailing fields if present
                    if tag_vec.len() > 2 {
                        Some(nostr::Tag::custom(nostr::TagKind::e(), tag_vec[1..].to_vec()))
                    } else {
                        nostr::EventId::from_hex(&tag_vec[1]).ok()
                            .map(nostr::Tag::event)
                    }
                }
                "p" if tag_vec.len() >= 2 => {
                    // P-tag - preserve trailing fields if present
                    if tag_vec.len() > 2 {
                        Some(nostr::Tag::custom(nostr::TagKind::p(), tag_vec[1..].to_vec()))
                    } else {
                        nostr::PublicKey::from_hex(&tag_vec[1]).ok()
                            .map(nostr::Tag::public_key)
                    }
                }
                "t" if tag_vec.len() >= 2 => {
                    Some(nostr::Tag::hashtag(&tag_vec[1]))
                }
                _ => Some(nostr::Tag::custom(
                    nostr::TagKind::Custom(std::borrow::Cow::Owned(tag_vec[0].clone())),
                    tag_vec[1..].to_vec(),
                ))
            }
        })
        .collect()
}

// =============================================================================
// MIME Type Detection
// =============================================================================

/// Detect MIME type from URL file extension (images and audio)
/// For video types, use detect_video_mime_type()
pub(crate) fn detect_mime_type(url: &str) -> Option<String> {
    let url_lower = url.to_lowercase();

    // Extract extension from URL (handles query params and fragments)
    let path = url_lower
        .split('?').next()?  // Remove query string
        .split('#').next()?; // Remove fragment
    let extension = path.split('.').next_back()?;

    match extension {
        // Image types
        "jpg" | "jpeg" => Some("image/jpeg".to_string()),
        "png" => Some("image/png".to_string()),
        "gif" => Some("image/gif".to_string()),
        "webp" => Some("image/webp".to_string()),
        "svg" => Some("image/svg+xml".to_string()),
        "bmp" => Some("image/bmp".to_string()),
        "ico" => Some("image/x-icon".to_string()),
        "tiff" | "tif" => Some("image/tiff".to_string()),
        "avif" => Some("image/avif".to_string()),
        "heic" | "heif" => Some("image/heic".to_string()),

        // Audio types (audio-only extensions)
        "mp3" => Some("audio/mpeg".to_string()),
        "m4a" | "aac" => Some("audio/mp4".to_string()),
        "ogg" | "opus" => Some("audio/ogg".to_string()),
        "wav" => Some("audio/wav".to_string()),
        "weba" => Some("audio/webm".to_string()),
        "flac" => Some("audio/flac".to_string()),

        _ => None,
    }
}

/// Detect video MIME type from URL file extension
#[allow(dead_code)]
pub(crate) fn detect_video_mime_type(url: &str) -> Option<String> {
    let url_lower = url.to_lowercase();

    // Extract extension from URL (handles query params and fragments)
    let path = url_lower
        .split('?').next()?  // Remove query string
        .split('#').next()?; // Remove fragment
    let extension = path.split('.').next_back()?;

    match extension {
        "mp4" | "m4v" => Some("video/mp4".to_string()),
        "webm" => Some("video/webm".to_string()),
        "mov" => Some("video/quicktime".to_string()),
        "avi" => Some("video/x-msvideo".to_string()),
        "mkv" => Some("video/x-matroska".to_string()),
        "ogv" => Some("video/ogg".to_string()),
        "3gp" => Some("video/3gpp".to_string()),
        "flv" => Some("video/x-flv".to_string()),
        _ => None,
    }
}
