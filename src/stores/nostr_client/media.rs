//! Picture/video/voice
//!
//! Functions for publishing media content (NIP-68, NIP-71, NIP-A0).
use super::fetching::get_client;
use super::signals::HAS_SIGNER;
use super::types::{detect_mime_type, PublishResult};
use crate::utils::video_kinds::{VIDEO_HORIZONTAL_ADDR, VIDEO_VERTICAL_ADDR};
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
/// Extract root event information from a reply event (NIP-10/NIP-22)
/// Returns (root_event_id, root_pubkey, root_relay_url)
/// Note: Kind is not available from standard e-tags, so caller should use parent's kind as fallback
fn extract_root_from_event(
    event: &nostr::Event,
) -> (Option<String>, Option<PublicKey>, Option<RelayUrl>) {
    if let Some(result) = event.tags.iter().find_map(|tag| {
        if let Some(nostr::TagStandard::Event {
            event_id,
            relay_url,
            marker,
            public_key,
            ..
        }) = tag.as_standardized()
        {
            if marker == &Some(nostr_sdk::nips::nip10::Marker::Root) {
                return Some((Some(event_id.to_hex()), *public_key, relay_url.clone()));
            }
        }
        None
    }) {
        return result;
    }
    if let Some(result) = event.tags.iter().find_map(|tag| {
        if let Some(nostr::TagStandard::Event {
            event_id,
            relay_url,
            marker,
            public_key,
            ..
        }) = tag.as_standardized()
        {
            if marker.is_none() {
                return Some((Some(event_id.to_hex()), *public_key, relay_url.clone()));
            }
        }
        None
    }) {
        return result;
    }
    let uppercase_e_tags: Vec<_> = event
        .tags
        .iter()
        .filter_map(|tag| {
            let tag_slice = tag.as_slice();
            if tag_slice.len() >= 2 && tag_slice[0] == "E" {
                let relay = tag_slice
                    .get(2)
                    .filter(|r| !r.is_empty())
                    .and_then(|r| RelayUrl::parse(r).ok());
                Some((tag_slice[1].to_string(), relay))
            } else {
                None
            }
        })
        .collect();
    if let Some((root_id, relay)) = uppercase_e_tags.first() {
        let root_pubkey = event.tags.iter().find_map(|tag| {
            let v = tag.as_slice();
            if v.len() >= 2 && v[0] == "P" {
                PublicKey::from_hex(&v[1]).ok()
            } else {
                None
            }
        });
        return (Some(root_id.clone()), root_pubkey, relay.clone());
    }
    (None, None, None)
}
/// Maximum waveform samples (256 is reasonable for display)
const MAX_WAVEFORM_LEN: usize = 256;
/// Validate voice message parameters (Dioxus pattern: early returns, single responsibility)
///
/// Returns (duration_u64, capped_waveform) on success.
fn validate_voice_params(duration: f64, waveform: &[u8]) -> Result<(u64, Vec<u8>), String> {
    if duration.is_nan() {
        return Err("Duration must be a valid number".to_string());
    }
    if !duration.is_finite() {
        return Err("Duration must be finite".to_string());
    }
    if duration < 0.0 {
        return Err("Duration must be non-negative".to_string());
    }
    const MAX_SAFE_DURATION: f64 = (u64::MAX as f64) - 0.5;
    if duration > MAX_SAFE_DURATION {
        return Err("Duration too large".to_string());
    }
    let duration_u64 = duration.round() as u64;
    let capped_waveform = if waveform.len() > MAX_WAVEFORM_LEN {
        log::warn!(
            "Waveform truncated from {} to {} samples",
            waveform.len(),
            MAX_WAVEFORM_LEN
        );
        waveform[..MAX_WAVEFORM_LEN].to_vec()
    } else {
        waveform.to_vec()
    };
    Ok((duration_u64, capped_waveform))
}
/// Publish a picture post (Kind 20) with relay feedback
/// NIP-68: https://github.com/nostr-protocol/nips/blob/master/68.md
pub async fn publish_picture_tracked(
    title: String,
    caption: String,
    image_urls: Vec<String>,
    hashtags: Vec<String>,
    location: String,
) -> std::result::Result<PublishResult, String> {
    let _client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    if image_urls.is_empty() {
        return Err("At least one image is required".to_string());
    }
    log::info!("Publishing picture post: {}", title);
    use nostr::Tag;
    let mut tags = vec![Tag::title(title.clone())];
    for url in &image_urls {
        let url = url.trim();
        if url.is_empty() {
            return Err("Image URLs cannot be empty".to_string());
        }
        nostr::Url::parse(url).map_err(|e| format!("Invalid image URL '{}': {}", url, e))?;
        let mut imeta_fields = vec![format!("url {}", url)];
        if let Some(mime_type) = detect_mime_type(url) {
            imeta_fields.push(format!("m {}", mime_type));
        }
        tags.push(Tag::custom(
            nostr::TagKind::Custom("imeta".into()),
            imeta_fields,
        ));
    }
    if !location.is_empty() {
        tags.push(Tag::custom(
            nostr::TagKind::Custom("location".into()),
            vec![location],
        ));
    }
    for hashtag in hashtags {
        tags.push(Tag::hashtag(hashtag));
    }
    let builder = nostr::EventBuilder::new(nostr::Kind::from(20), caption).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign picture: {}", e))?;
    let event_id = event.id.to_hex();
    let queue_id = crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Media,
        None,
        std::collections::HashMap::new(),
    ).await;
    let result = PublishResult::queued(queue_id, event_id);
    log::info!("Picture '{}' queued: {}", title, result.event_id);
    Ok(result)
}
/// Publish a picture post (Kind 20)
/// For relay feedback, use publish_picture_tracked instead
pub async fn publish_picture(
    title: String,
    caption: String,
    image_urls: Vec<String>,
    hashtags: Vec<String>,
    location: String,
) -> std::result::Result<String, String> {
    publish_picture_tracked(title, caption, image_urls, hashtags, location)
        .await
        .map(|result| result.event_id)
}
/// Publish a video post as addressable event (Kind 34235 for landscape, Kind 34236 for portrait)
/// NIP-71: https://github.com/nostr-protocol/nips/blob/master/71.md
///
/// Videos are published as addressable events with a d-tag identifier, allowing them to be
/// updated by republishing with the same d-tag.
pub async fn publish_video_tracked(
    title: String,
    description: String,
    video_url: String,
    thumbnail_url: String,
    hashtags: Vec<String>,
    is_portrait: bool,
) -> std::result::Result<PublishResult, String> {
    let _client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let video_url = video_url.trim();
    if video_url.is_empty() {
        return Err("Video URL is required".to_string());
    }
    nostr::Url::parse(video_url).map_err(|e| format!("Invalid video URL: {}", e))?;
    let thumbnail_url = thumbnail_url.trim();
    if !thumbnail_url.is_empty() {
        nostr::Url::parse(thumbnail_url).map_err(|e| format!("Invalid thumbnail URL: {}", e))?;
    }
    if title.trim().is_empty() {
        return Err("Title is required".to_string());
    }
    // Use addressable video kinds (NIP-71)
    let kind = if is_portrait {
        VIDEO_VERTICAL_ADDR
    } else {
        VIDEO_HORIZONTAL_ADDR
    };
    log::info!("Publishing addressable video (kind {}): {}", kind, title);
    use nostr::Tag;

    // Generate d-tag identifier: slug from title + timestamp for uniqueness
    // Use millisecond precision to prevent collisions on rapid publishes
    let timestamp_ms = crate::platform::timestamp::now_millis();
    let slug = crate::utils::slugify(&title);
    let identifier = format!("{}-{}", slug, timestamp_ms);

    // d-tag must be first for addressable events
    let mut tags = vec![
        Tag::identifier(identifier.clone()),
        Tag::title(title.clone()),
    ];

    let mut imeta_fields = vec![format!("url {}", video_url)];
    let video_mime: Option<&str> = {
        let url_lower = video_url.to_lowercase();
        let path = url_lower
            .split('#')
            .next()
            .unwrap_or(&url_lower)
            .split('?')
            .next()
            .unwrap_or(&url_lower);
        let ext = path.split('.').next_back().unwrap_or("");
        match ext {
            "mp4" | "m4v" => Some("video/mp4"),
            "webm" => Some("video/webm"),
            "mov" => Some("video/quicktime"),
            "avi" => Some("video/x-msvideo"),
            "mkv" => Some("video/x-matroska"),
            "m3u8" => Some("application/x-mpegURL"),
            "ts" => Some("video/MP2T"),
            _ => None,
        }
    };
    if let Some(mime) = video_mime {
        imeta_fields.push(format!("m {}", mime));
    }
    if !thumbnail_url.is_empty() {
        imeta_fields.push(format!("image {}", thumbnail_url));
    }
    tags.push(Tag::custom(
        nostr::TagKind::Custom("imeta".into()),
        imeta_fields,
    ));
    for hashtag in hashtags {
        tags.push(Tag::hashtag(hashtag));
    }
    let content = description;
    let builder = nostr::EventBuilder::new(nostr::Kind::from(kind), content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign video: {}", e))?;
    let event_id = event.id.to_hex();
    let queue_id = crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Media,
        None,
        std::collections::HashMap::new(),
    ).await;
    let result = PublishResult::queued(queue_id, event_id);
    log::info!("Video '{}' queued: {} (d-tag: {})", title, result.event_id, identifier);
    Ok(result)
}
/// Publish a video post as addressable event (Kind 34235 for landscape, Kind 34236 for portrait)
/// For relay feedback, use publish_video_tracked instead
pub async fn publish_video(
    title: String,
    description: String,
    video_url: String,
    thumbnail_url: String,
    hashtags: Vec<String>,
    is_portrait: bool,
) -> std::result::Result<String, String> {
    publish_video_tracked(
        title,
        description,
        video_url,
        thumbnail_url,
        hashtags,
        is_portrait,
    )
    .await
    .map(|result| result.event_id)
}
/// Publish a voice message (Kind 1222) with relay feedback
/// NIP-A0: https://github.com/nostr-protocol/nips/blob/master/A0.md
pub async fn publish_voice_message_tracked(
    audio_url: String,
    duration: f64,
    waveform: Vec<u8>,
    hashtags: Vec<String>,
    mime_type: Option<String>,
) -> std::result::Result<PublishResult, String> {
    let _client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    log::info!("Publishing voice message: {}", audio_url);
    let (duration_u64, waveform) = validate_voice_params(duration, &waveform)?;
    let url = nostr::Url::parse(&audio_url).map_err(|e| format!("Invalid audio URL: {}", e))?;
    let mut builder = nostr::EventBuilder::voice_message(url);
    use nostr::Tag;
    let mut tags = Vec::new();
    let waveform_str = waveform
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let mut imeta_fields = vec![
        format!("url {}", audio_url),
        format!("duration {}", duration_u64),
        format!("waveform {}", waveform_str),
    ];
    let final_mime_type = mime_type
        .or_else(|| detect_mime_type(&audio_url))
        .unwrap_or_else(|| "audio/webm".to_string());
    imeta_fields.push(format!("m {}", final_mime_type));
    tags.push(Tag::custom(
        nostr::TagKind::Custom("imeta".into()),
        imeta_fields,
    ));
    for hashtag in hashtags {
        tags.push(Tag::hashtag(hashtag));
    }
    builder = builder.tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign voice message: {}", e))?;
    let event_id = event.id.to_hex();
    let queue_id = crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Media,
        None,
        std::collections::HashMap::new(),
    ).await;
    let result = PublishResult::queued(queue_id, event_id);
    log::info!("Voice message queued: {}", result.event_id);
    Ok(result)
}
/// Publish a voice message (Kind 1222)
/// For relay feedback, use publish_voice_message_tracked instead
pub async fn publish_voice_message(
    audio_url: String,
    duration: f64,
    waveform: Vec<u8>,
    hashtags: Vec<String>,
    mime_type: Option<String>,
) -> std::result::Result<String, String> {
    publish_voice_message_tracked(audio_url, duration, waveform, hashtags, mime_type)
        .await
        .map(|result| result.event_id)
}
/// Publish a voice message reply (Kind 1244) with relay feedback
/// NIP-A0: https://github.com/nostr-protocol/nips/blob/master/A0.md
/// NIP-22: https://github.com/nostr-protocol/nips/blob/master/22.md
pub async fn publish_voice_message_reply_tracked(
    audio_url: String,
    duration: f64,
    waveform: Vec<u8>,
    reply_to: nostr::Event,
    mime_type: Option<String>,
) -> std::result::Result<PublishResult, String> {
    let _client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    log::info!(
        "Publishing voice message reply to: {}",
        reply_to.id.to_hex()
    );
    let (duration_u64, waveform) = validate_voice_params(duration, &waveform)?;
    let url = nostr::Url::parse(&audio_url).map_err(|e| format!("Invalid audio URL: {}", e))?;
    let (root_event_id, root_pubkey, root_relay_url) = extract_root_from_event(&reply_to);
    let parent_id = reply_to.id.to_hex();
    let parent_pubkey = reply_to.pubkey;
    let parent_kind = reply_to.kind;
    use nostr::prelude::*;
    let parent_target = if parent_kind.as_u16() == 1222 || parent_kind.as_u16() == 1244 {
        let event_id = EventId::parse(&parent_id)
            .map_err(|e| format!("Failed to parse parent event ID: {}", e))?;
        CommentTarget::event(event_id, parent_kind, Some(parent_pubkey), None)
    } else {
        return Err("Can only reply to voice messages (Kind 1222 or 1244)".to_string());
    };
    let root_target = if let Some(root_id) = root_event_id {
        if root_id != parent_id {
            let event_id = EventId::parse(&root_id)
                .map_err(|e| format!("Failed to parse root event ID: {}", e))?;
            use std::borrow::Cow;
            Some(CommentTarget::event(
                event_id,
                reply_to.kind,
                root_pubkey,
                root_relay_url.as_ref().map(Cow::Borrowed),
            ))
        } else {
            None
        }
    } else {
        None
    };
    let mut builder = nostr::EventBuilder::voice_message_reply(url, root_target, parent_target);
    use nostr::Tag;
    let mut tags = Vec::new();
    let waveform_str = waveform
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let mut imeta_fields = vec![
        format!("url {}", audio_url),
        format!("duration {}", duration_u64),
        format!("waveform {}", waveform_str),
    ];
    let final_mime_type = mime_type
        .or_else(|| detect_mime_type(&audio_url))
        .unwrap_or_else(|| "audio/webm".to_string());
    imeta_fields.push(format!("m {}", final_mime_type));
    tags.push(Tag::custom(
        nostr::TagKind::Custom("imeta".into()),
        imeta_fields,
    ));
    tags.push(Tag::public_key(parent_pubkey));
    if let Some(root_pk) = root_pubkey {
        if root_pk != parent_pubkey {
            let already_tagged = reply_to.tags.public_keys().any(|pk| pk == &root_pk);
            if !already_tagged {
                tags.push(Tag::public_key(root_pk));
            }
        }
    }
    for public_key in reply_to.tags.public_keys() {
        if public_key != &parent_pubkey {
            tags.push(Tag::public_key(*public_key));
        }
    }
    builder = builder.tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign voice message reply: {}", e))?;
    let event_id = event.id.to_hex();
    let queue_id = crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Media,
        None,
        std::collections::HashMap::new(),
    ).await;
    let result = PublishResult::queued(queue_id, event_id);
    log::info!("Voice message reply queued: {}", result.event_id);
    Ok(result)
}
/// Publish a voice message reply (Kind 1244) following NIP-22
/// For relay feedback, use publish_voice_message_reply_tracked instead
pub async fn publish_voice_message_reply(
    audio_url: String,
    duration: f64,
    waveform: Vec<u8>,
    reply_to: nostr::Event,
    mime_type: Option<String>,
) -> std::result::Result<String, String> {
    publish_voice_message_reply_tracked(audio_url, duration, waveform, reply_to, mime_type)
        .await
        .map(|result| result.event_id)
}
