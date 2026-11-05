//! NIP-92 `imeta` attachment registry.
//!
//! Upload/pick flows register metadata for every media file they hand to a
//! composer (keyed by URL); the publish paths drain the registry, emitting
//! one `["imeta", "url …", "m …", …]` tag per attachment whose URL appears
//! in the note content as an **exact URL token** (NIP-92: clients MAY
//! ignore imetas that do not match a content URL — Amethyst's `filterIsIn`
//! rule matches tokens by equality, not by substring). This keeps the
//! `MediaUploader`/`GifPicker` → composer wiring unchanged: everything
//! still flows through plain URL strings.

use std::collections::HashMap;

use dioxus::prelude::*;

use crate::utils::parsing::content_parser::extract_content_urls;

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

/// Drop all pending attachments.
///
/// NOT used by composer resets: the registry is process-global and shared
/// by every coexisting composer (note/reply/edit/topic/community), so a
/// blanket clear from one composer would wipe another's pending
/// attachments — composers use [`clear_attachments_except`] instead.
/// Kept as a hard reset helper (logout, panic recovery); no production
/// caller remains, hence the allow.
#[allow(dead_code)]
pub fn clear_attachments() {
    IMETA_ATTACHMENTS.write().clear();
}

/// Remove the pending attachments that belong to `content`, keeping every
/// other entry.
///
/// Called from a composer reset with the composer's pre-clear content:
/// entries whose URL matches that content are this composer's (already
/// consumed by [`take_imeta_tags_for`] on the publish path), while the
/// remaining entries belong to OTHER coexisting composers and must survive
/// the reset. Matching uses the same exact-token rule as
/// [`take_imeta_tags_for`] — the two functions must agree.
pub fn clear_attachments_except(content: &str) {
    let tokens = extract_content_urls(content);
    IMETA_ATTACHMENTS
        .write()
        .retain(|url, _| !tokens.iter().any(|token| token == url));
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
/// appears in `content` as an exact URL token, and remove those entries
/// (publish consumes them). Substring containment is NOT enough: an
/// attachment for `https://cdn.example/a.png` must not be attached to a
/// note whose token is `https://cdn.example/a.png?w=64` (Amethyst's
/// `filterIsIn` token-equality rule). Attachments that never matched stay
/// registered for a later publish.
pub fn take_imeta_tags_for(content: &str) -> Vec<Vec<String>> {
    let tokens = extract_content_urls(content);
    let map = IMETA_ATTACHMENTS.read();
    let used: Vec<String> = map
        .keys()
        .filter(|url| tokens.iter().any(|token| token == url.as_str()))
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

    // --- Registry (GlobalSignal) tests --------------------------------
    //
    // The registry is process-global: GlobalSignal access needs a Dioxus
    // runtime on the thread (same pattern as `stores/deflock_store.rs`),
    // and the registry tests serialize on a mutex so parallel test threads
    // don't wipe each other's fixtures.

    /// Serializes registry-mutating tests (the harness runs tests on
    /// parallel threads; `IMETA_ATTACHMENTS` is shared per-process).
    static REGISTRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn take_imeta_tags_for_consumes_exact_token_matches() {
        let vdom = dioxus::prelude::VirtualDom::new(|| dioxus::prelude::rsx! { div {} });
        let _rt_guard = dioxus_core::RuntimeGuard::new(vdom.runtime());
        let _lock = REGISTRY_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let url = "https://imeta-take.example/a.png";
        register_attachment(test_attachment(url));

        // Exact token present → one tag, entry consumed.
        let tags = take_imeta_tags_for(&format!("look {url}!"));
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0][0], "imeta");
        assert_eq!(tags[0][1], format!("url {url}"));
        // Consumed by the publish: a second take finds nothing.
        assert!(take_imeta_tags_for(&format!("look {url}!")).is_empty());
    }

    #[test]
    fn take_imeta_tags_for_rejects_substring_matches() {
        let vdom = dioxus::prelude::VirtualDom::new(|| dioxus::prelude::rsx! { div {} });
        let _rt_guard = dioxus_core::RuntimeGuard::new(vdom.runtime());
        let _lock = REGISTRY_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let url = "https://imeta-substring.example/a.png";
        register_attachment(test_attachment(url));

        // Superstring token (url + extra query param): substring
        // containment holds, exact-token equality does not → no tag.
        let superstring = take_imeta_tags_for(&format!("look {url}?w=64"));
        assert!(
            superstring.is_empty(),
            "attachment must not match a superstring URL token"
        );
        // Neither does unrelated content.
        let unrelated = take_imeta_tags_for("no attachments here https://other.example/x.png");
        assert!(unrelated.is_empty());
        // The entry is still registered for a later publish with the
        // exact URL, and that later publish consumes it.
        let tags = take_imeta_tags_for(url);
        assert_eq!(tags.len(), 1);
        assert!(take_imeta_tags_for(url).is_empty());
    }

    #[test]
    fn clear_attachments_except_spares_other_composers_entries() {
        let vdom = dioxus::prelude::VirtualDom::new(|| dioxus::prelude::rsx! { div {} });
        let _rt_guard = dioxus_core::RuntimeGuard::new(vdom.runtime());
        let _lock = REGISTRY_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // Two coexisting composers: this one references `mine`, another
        // composer still pending on `other`.
        let mine = "https://imeta-except.example/mine.png";
        let other = "https://imeta-except.example/other.png";
        register_attachment(test_attachment(mine));
        register_attachment(test_attachment(other));

        clear_attachments_except(&format!("posting {mine}"));

        let map = IMETA_ATTACHMENTS.read();
        assert!(
            !map.contains_key(mine),
            "entry referenced by the cleared content is removed"
        );
        assert!(
            map.contains_key(other),
            "entry belonging to another composer's content must survive"
        );
    }
}
