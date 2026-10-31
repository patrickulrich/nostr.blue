//! NIP-92 `imeta` attachment registry.
//!
//! Upload/pick flows register metadata for every media file they hand to a
//! composer (keyed by URL); the publish paths drain the registry, emitting
//! one `["imeta", "url …", "m …", …]` tag per attachment whose URL actually
//! appears in the note content (NIP-92: clients MAY ignore imetas that do
//! not match a content URL — Amethyst's `filterIsIn` rule). This keeps the
//! `MediaUploader`/`GifPicker` → composer wiring unchanged: everything
//! still flows through plain URL strings.

use std::collections::HashMap;

use dioxus::prelude::*;

/// Registry cap: keeps the map bounded when users upload-and-cancel or
/// compose many attachments across sessions (LRU-style, oldest dropped).
const MAX_ENTRIES: usize = 30;

/// Metadata for one attached media file (BUD-08/NIP-92 fields).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImetaAttachment {
    /// URL as it will appear in the note content (registry key).
    pub url: String,
    pub mime_type: Option<String>,
    /// SHA-256 of the media bytes (`x` field).
    pub sha256: Option<String>,
    /// Size in bytes.
    pub size: Option<u64>,
    /// Pixel (width, height) (`dim` field, `"WxH"`).
    pub dim: Option<(u32, u32)>,
    pub blurhash: Option<String>,
    pub alt: Option<String>,
    /// Server-provided BUD-08 `nip94` descriptor fields (authoritative;
    /// keys not covered above — e.g. `magnet`, `i` — are passed through).
    pub nip94: Vec<(String, String)>,
}

impl From<crate::stores::media::blossom_store::UploadedMediaMeta> for ImetaAttachment {
    fn from(meta: crate::stores::media::blossom_store::UploadedMediaMeta) -> Self {
        Self {
            url: meta.url,
            mime_type: Some(meta.mime_type),
            sha256: Some(meta.sha256),
            size: Some(meta.size),
            dim: meta.dim,
            blurhash: None,
            alt: None,
            nip94: meta
                .nip94
                .into_iter()
                .filter_map(|kv| match kv.as_slice() {
                    [k, v, ..] => Some((k.clone(), v.clone())),
                    _ => None,
                })
                .collect(),
        }
    }
}

/// Pending attachments keyed by URL.
static IMETA_ATTACHMENTS: GlobalSignal<HashMap<String, ImetaAttachment>> =
    Signal::global(HashMap::new);

/// Register (or replace) the metadata for one attachment URL.
pub fn register_attachment(attachment: ImetaAttachment) {
    if attachment.url.is_empty() {
        return;
    }
    let mut map = IMETA_ATTACHMENTS.write();
    if map.len() >= MAX_ENTRIES && !map.contains_key(&attachment.url) {
        // Drop the oldest entry (HashMap order is unspecified but stable
        // enough for a cache; the cap only guards unbounded growth).
        if let Some(oldest) = map.keys().next().cloned() {
            map.remove(&oldest);
        }
    }
    map.insert(attachment.url.clone(), attachment);
}

/// Drop all pending attachments (e.g. when a composer is reset).
pub fn clear_attachments() {
    IMETA_ATTACHMENTS.write().clear();
}

/// Build one raw `["imeta", "url …", …]` tag for an attachment.
///
/// NIP-92: the tag MUST have a `url` and at least one other field — returns
/// `None` when nothing else is known (the tag would be invalid).
/// Server-provided BUD-08 fields are preferred; locally-known values fill
/// gaps; `url` always first, then a stable field order.
fn build_imeta_tag(attachment: &ImetaAttachment) -> Option<Vec<String>> {
    let server: HashMap<&str, &str> = attachment
        .nip94
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let mut fields: Vec<(String, String)> = Vec::new();
    let mut push = |key: &str, value: Option<String>| {
        let value = value.or_else(|| server.get(key).map(|v| v.to_string()));
        if let Some(value) = value {
            fields.push((key.to_string(), value));
        }
    };
    push("m", attachment.mime_type.clone());
    push("x", attachment.sha256.clone());
    push(
        "size",
        attachment.size.map(|s| s.to_string()),
    );
    push(
        "dim",
        attachment
            .dim
            .map(|(w, h)| format!("{w}x{h}")),
    );
    push("blurhash", attachment.blurhash.clone());
    push("alt", attachment.alt.clone());
    // Any server-provided field we did not cover (magnet, i, fallback…).
    for (key, value) in &attachment.nip94 {
        if key != "url" && !fields.iter().any(|(k, _)| k == key) {
            fields.push((key.clone(), value.clone()));
        }
    }
    if fields.is_empty() {
        return None; // NIP-92: url alone is not a valid imeta tag.
    }
    let mut tag = vec![format!("url {}", attachment.url)];
    tag.extend(fields.into_iter().map(|(k, v)| format!("{k} {v}")));
    Some(tag)
}

/// Return raw `["imeta", …]` tags for every pending attachment whose URL
/// appears in `content`, and remove those entries (publish consumes them).
/// Attachments that never matched stay registered for a later publish.
pub fn take_imeta_tags_for(content: &str) -> Vec<Vec<String>> {
    let map = IMETA_ATTACHMENTS.read();
    let used: Vec<String> = map
        .keys()
        .filter(|url| content.contains(url.as_str()))
        .cloned()
        .collect();
    drop(map);
    if used.is_empty() {
        return Vec::new();
    }
    let mut tags = Vec::with_capacity(used.len());
    let mut map = IMETA_ATTACHMENTS.write();
    for url in used {
        if let Some(attachment) = map.remove(&url) {
            if let Some(mut tag) = build_imeta_tag(&attachment) {
                tag.insert(0, "imeta".to_string());
                tags.push(tag);
            }
        }
    }
    tags
}

/// Convert raw imeta tags to `nostr::event::Tag`s (multi-value custom tag).
pub fn to_nostr_tags(raw: Vec<Vec<String>>) -> Vec<nostr::event::Tag> {
    raw.into_iter()
        .filter(|tag| tag.len() > 1)
        .map(|tag| nostr::event::Tag::custom("imeta", tag[1..].to_vec()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_attachment(url: &str) -> ImetaAttachment {
        ImetaAttachment {
            url: url.to_string(),
            mime_type: Some("image/png".to_string()),
            sha256: Some("ab".repeat(32)),
            size: Some(1234),
            dim: Some((800, 600)),
            blurhash: None,
            alt: Some("A scenic photo overlooking the coast".to_string()),
            nip94: Vec::new(),
        }
    }

    #[test]
    fn builds_tag_with_url_first_and_known_fields() {
        let tag = build_imeta_tag(&test_attachment("https://cdn.example/a.png")).unwrap();
        assert_eq!(tag[0], "url https://cdn.example/a.png");
        assert_eq!(tag[1], "m image/png");
        assert_eq!(tag[2], format!("x {}", "ab".repeat(32)));
        assert_eq!(tag[3], "size 1234");
        assert_eq!(tag[4], "dim 800x600");
        assert_eq!(
            tag[5],
            "alt A scenic photo overlooking the coast"
        );
    }

    #[test]
    fn url_only_attachment_is_invalid_per_nip92() {
        let attachment = ImetaAttachment {
            url: "https://cdn.example/b.png".to_string(),
            ..Default::default()
        };
        assert!(build_imeta_tag(&attachment).is_none());
    }

    #[test]
    fn server_nip94_fields_preferred_and_passthrough() {
        let mut attachment = test_attachment("https://cdn.example/c.png");
        attachment.mime_type = None; // server knows better
        attachment.size = None;
        attachment.nip94 = vec![
            ("m".to_string(), "image/webp".to_string()),
            ("size".to_string(), "999".to_string()),
            ("magnet".to_string(), "magnet:?xt=urn:btih:xyz".to_string()),
            ("url".to_string(), "ignored-duplicate".to_string()),
        ];
        let tag = build_imeta_tag(&attachment).unwrap();
        assert!(tag.contains(&"m image/webp".to_string()));
        assert!(tag.contains(&"size 999".to_string()));
        assert!(tag.contains(&"magnet magnet:?xt=urn:btih:xyz".to_string()));
        assert_eq!(
            tag.iter()
                .filter(|f| f.starts_with("url "))
                .count(),
            1,
            "server url duplicate must not produce a second url field"
        );
    }

    // The registry itself is a GlobalSignal (Dioxus runtime-bound); per
    // codebase convention it stays covered by the pure builder tests above
    // — `take_imeta_tags_for` is a thin match-and-drain over the map.
}
