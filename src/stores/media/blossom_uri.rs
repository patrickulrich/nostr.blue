//! BUD-10 `blossom:` URI resolution.
//!
//! Content-addressed blob discovery per the Blossom URI schema: `xs` server
//! hints first (https preferred for bare domains), then `as`-authors' kind
//! 10063 server lists, then the user's configured servers. Verified URLs are
//! cached keyed by sha256 so the cache survives server flips, with a short
//! negative cache so dead URIs stay cheap (Amethyst's resolver pattern).

use std::collections::HashMap;

use dioxus::prelude::*;
use nostr_blossom::bud03;
use nostr_blossom::bud10::BlossomUri;
use nostr_blossom::client::BlossomPaymentRequest;
use nostr_sdk::prelude::{Filter, Kind, PublicKey, Timestamp};
use nostr::types::Url;

use crate::platform::http::http_client;
use crate::stores::media::blossom_store::{get_servers, KIND_USER_BLOSSOM_SERVERS};
use crate::stores::nostr_client;

/// How long a failed resolution stays memoized before re-probing.
const MISS_TTL_SECS: u64 = 30;

/// Verified working URL per blob hash: `sha256 -> (url, resolved_at)`.
/// Content-addressed (not URL-keyed) so entries survive server flips.
pub static BLOSSOM_URL_CACHE: GlobalSignal<HashMap<String, (String, u64)>> =
    Signal::global(HashMap::new);
/// Negative cache: `sha256 -> failed_at` (short TTL, see Amethyst's
/// `MISS_TTL_MS` — a brief outage must not permanently hide a blob).
static BLOSSOM_MISS_CACHE: GlobalSignal<HashMap<String, u64>> = Signal::global(HashMap::new);

/// What the renderer should do with a blob of a given file extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlossomMediaKind {
    Image,
    Video,
    Audio,
    File,
}

/// Classify a BUD-10 blob by its extension for renderer dispatch.
pub fn classify_extension(ext: &str) -> BlossomMediaKind {
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp" | "avif" | "heic" => {
            BlossomMediaKind::Image
        }
        "mp4" | "webm" | "mov" | "mkv" | "m4v" | "avi" => BlossomMediaKind::Video,
        "mp3" | "wav" | "ogg" | "m4a" | "flac" | "opus" | "aac" => BlossomMediaKind::Audio,
        _ => BlossomMediaKind::File,
    }
}

/// True when the page itself is served over HTTPS (web builds only): plain
/// `http://` candidates would be blocked as mixed content, so they are
/// skipped entirely there. Native/desktop keeps them as a last resort.
pub fn is_https_page() -> bool {
    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    {
        web_sys::window()
            .and_then(|w| w.location().protocol().ok())
            .is_some_and(|p| p.eq_ignore_ascii_case("https:"))
    }
    #[cfg(not(all(feature = "web", target_arch = "wasm32")))]
    {
        false
    }
}

/// Returns a cached working URL for the hash, unless the cached failure is
/// still within the miss TTL.
pub fn cached_url(sha256_hex: &str) -> Option<String> {
    let now = now_secs();
    let misses = BLOSSOM_MISS_CACHE.read();
    if let Some(&failed_at) = misses.get(sha256_hex) {
        if now.saturating_sub(failed_at) < MISS_TTL_SECS {
            return None;
        }
    }
    drop(misses);
    BLOSSOM_URL_CACHE
        .read()
        .get(sha256_hex)
        .map(|(url, _)| url.clone())
}

/// Record a verified working URL for the blob hash.
pub fn record_hit(sha256_hex: &str, url: &str) {
    let now = now_secs();
    BLOSSOM_URL_CACHE
        .write()
        .insert(sha256_hex.to_string(), (url.to_string(), now));
    BLOSSOM_MISS_CACHE.write().remove(sha256_hex);
}

/// Record that no server currently serves the blob (short-lived).
pub fn record_miss(sha256_hex: &str) {
    let now = now_secs();
    BLOSSOM_MISS_CACHE.write().insert(sha256_hex.to_string(), now);
}

fn now_secs() -> u64 {
    Timestamp::now().as_secs()
}

fn parse_server_url(s: &str) -> Option<Url> {
    let candidate = if s.contains("://") {
        s.to_string()
    } else {
        format!("https://{s}/")
    };
    Url::parse(&candidate).ok()
}

/// Phase-1 candidates: `xs` hints in URI order, then the user's configured
/// servers as fallback (BUD-10 resolution strategy). Bare domains get https
/// only (http would be mixed content on the web build and is rarely useful).
pub fn initial_candidates(uri: &BlossomUri) -> Vec<String> {
    initial_candidates_with(uri, &get_servers())
}

/// Pure core of [`initial_candidates`] (no signal reads — unit-testable).
pub fn initial_candidates_with(uri: &BlossomUri, fallback_servers: &[String]) -> Vec<String> {
    let fallback = fallback_servers
        .iter()
        .filter_map(|s| parse_server_url(s));
    let urls: Vec<String> = uri
        .candidate_urls([], fallback)
        .into_iter()
        .filter(|u| !is_https_page() || u.scheme() == "https")
        .map(|u| u.to_string())
        .collect();
    dedup_preserving_order(urls)
}

/// Phase-2 candidates: append the `as`-authors' kind 10063 servers (fetched
/// lazily only when phase 1 is exhausted, to avoid extra relay REQs).
pub fn candidates_with_authors(uri: &BlossomUri, author_servers: &[Url]) -> Vec<String> {
    candidates_with_authors_with(uri, author_servers, &get_servers())
}

/// Pure core of [`candidates_with_authors`] (no signal reads).
pub fn candidates_with_authors_with(
    uri: &BlossomUri,
    author_servers: &[Url],
    fallback_servers: &[String],
) -> Vec<String> {
    let existing = initial_candidates_with(uri, fallback_servers);
    let fallback = fallback_servers
        .iter()
        .filter_map(|s| parse_server_url(s));
    let mut all = existing;
    for url in uri.candidate_urls(author_servers.iter().cloned(), fallback) {
        if !is_https_page() || url.scheme() == "https" {
            let s = url.to_string();
            if !all.contains(&s) {
                all.push(s);
            }
        }
    }
    all
}

/// Dedup while keeping first occurrences (candidate order is meaningful).
fn dedup_preserving_order(urls: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    urls.into_iter()
        .filter(|u| seen.insert(u.clone()))
        .collect()
}

/// Fetch a pubkey's kind 10063 (BUD-03) Blossom server list: SDK database
/// first, then the readiness-gated fetch facade. Best effort — errors just
/// yield an empty list (the caller falls through to configured servers).
pub async fn fetch_author_servers(pubkey: &PublicKey) -> Vec<Url> {
    let filter = Filter::new()
        .author(*pubkey)
        .kind(Kind::from(KIND_USER_BLOSSOM_SERVERS))
        .limit(1);
    // DB-first, mirroring `blossom_store::fetch_user_servers`.
    if let Some(client) = nostr_client::get_client() {
        if let Ok(events) = client.database().query(filter.clone()).await {
            for event in events {
                let urls = bud03::server_urls(&event);
                if !urls.is_empty() {
                    return urls;
                }
            }
        }
    }
    match nostr_client::fetch(filter, nostr_client::FetchProfile::Fast).await {
        Ok(events) => events
            .iter()
            .find_map(|e| {
                let urls = bud03::server_urls(e);
                (!urls.is_empty()).then_some(urls)
            })
            .unwrap_or_default(),
        Err(e) => {
            log::debug!("kind 10063 fetch failed for {}: {e}", pubkey.to_hex());
            Vec::new()
        }
    }
}

/// Outcome of probing one candidate URL with `HEAD /<sha256>.<ext>` (BUD-01).
#[derive(Debug, Clone)]
pub enum ProbeOutcome {
    /// 200 OK — the server has the blob. `size_matches` is false when a
    /// readable `Content-Length` disagreed with the URI's `sz` (a mirror
    /// serving the wrong bytes), in which case the caller must skip it.
    Available { size_matches: bool },
    /// Definitively absent (404/410).
    NotFound,
    /// 402: the server wants payment. Requests may be empty on the web when
    /// the server did not expose the `X-*` payment headers cross-origin.
    PaymentRequired {
        requests: Vec<BlossomPaymentRequest>,
        reason: Option<String>,
    },
    /// Anything else (5xx, network error, CORS refusal): inconclusive.
    Unknown(String),
}

/// Best-effort `HEAD` probe of a candidate blob URL. On the web this is
/// subject to CORS (BUD-01 requires `Access-Control-Allow-Origin`; the
/// payment headers additionally need `Access-Control-Expose-Headers`), so
/// treat `Unknown` as "try anyway / try next", never as proof of absence.
pub async fn head_probe(url: &str, expected_size: Option<u64>) -> ProbeOutcome {
    let client = match http_client() {
        Ok(c) => c,
        Err(e) => return ProbeOutcome::Unknown(format!("http client: {e}")),
    };
    let resp = match client.head(url).send().await {
        Ok(r) => r,
        Err(e) => return ProbeOutcome::Unknown(format!("{e}")),
    };
    let status = resp.status();
    if status.is_success() {
        let size_matches = match expected_size {
            Some(expected) => resp
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .is_none_or(|len| len == expected),
            // No `sz` in the URI — nothing to check. (Content-Length may be
            // unreadable cross-origin; that maps to `None` here, not a fail.)
            None => true,
        };
        return ProbeOutcome::Available { size_matches };
    }
    match status.as_u16() {
        404 | 410 => ProbeOutcome::NotFound,
        402 => {
            let mut requests = Vec::new();
            let mut reason = None;
            for (name, value) in resp.headers() {
                let lower = name.as_str().to_ascii_lowercase();
                let Some(method) = lower.strip_prefix("x-") else {
                    continue;
                };
                let Ok(value) = value.to_str() else {
                    continue;
                };
                if method == "reason" {
                    reason = Some(sanitize_server_text(value));
                } else if !value.is_empty() {
                    requests.push(BlossomPaymentRequest {
                        method: method.to_owned(),
                        request: value.to_owned(),
                    });
                }
            }
            ProbeOutcome::PaymentRequired { requests, reason }
        }
        code => ProbeOutcome::Unknown(format!("HTTP {code}")),
    }
}

/// Server-controlled text (X-Reason) is display-only per BUD-05/12; replace
/// control characters (including RTL/bidi overrides used to spoof dialogs)
/// with spaces and clamp the length before rendering (Amethyst's
/// `sanitizedReason`).
pub fn sanitize_server_text(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(200).collect()
}

/// Extract blob hashes worth reporting from a note (BUD-09): `imeta` URL
/// fields, `blossom:` URIs, and any 64-hex path segment in a content URL.
pub fn extract_blob_hashes(event: &nostr::event::Event) -> Vec<String> {
    let mut hashes = Vec::new();
    for tag in event.tags.iter() {
        let values = tag.as_slice();
        if values.first().map(String::as_str) == Some("imeta") {
            for field in values.iter().skip(1) {
                if let Some(url) = field.strip_prefix("url ") {
                    if let Some(hash) = bud03::extract_sha256(url) {
                        hashes.push(hash.to_string());
                    }
                }
            }
        }
    }
    for token in crate::utils::content_parser::parse_content(&event.content, &event.tags) {
        if let crate::utils::content_parser::ContentToken::BlossomUri(uri) = token {
            hashes.push(uri.sha256.to_string());
        }
    }
    let mut seen = std::collections::HashSet::new();
    hashes
        .into_iter()
        .filter(|h| seen.insert(h.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "b1674191a88ec5cdd733e4240a81803105dc412d6c6708d53ab94fc248f4f553";

    #[test]
    fn classifies_extensions() {
        assert_eq!(classify_extension("jpg"), BlossomMediaKind::Image);
        assert_eq!(classify_extension("AVIF"), BlossomMediaKind::Image);
        assert_eq!(classify_extension("mp4"), BlossomMediaKind::Video);
        assert_eq!(classify_extension("opus"), BlossomMediaKind::Audio);
        assert_eq!(classify_extension("pdf"), BlossomMediaKind::File);
        assert_eq!(classify_extension("bin"), BlossomMediaKind::File);
    }

    #[test]
    fn initial_candidates_prefer_xs_order_then_https() {
        let uri: BlossomUri = format!("blossom:{HASH}.png?xs=a.example&xs=b.example")
            .parse()
            .unwrap();
        let candidates = initial_candidates_with(&uri, &["https://fallback.example".to_string()]);
        // Native (non-https page): each bare hint expands to https then http.
        assert_eq!(candidates[0], format!("https://a.example/{HASH}.png"));
        assert_eq!(candidates[1], format!("http://a.example/{HASH}.png"));
        assert_eq!(candidates[2], format!("https://b.example/{HASH}.png"));
        assert!(
            candidates.contains(&format!("https://fallback.example/{HASH}.png")),
            "configured fallbacks must be appended: {candidates:?}"
        );
    }

    #[test]
    fn scheme_hint_is_used_verbatim() {
        let uri: BlossomUri = format!("blossom:{HASH}.png?xs=http%3A%2F%2Fplain.example")
            .parse()
            .unwrap();
        // Percent-encoded scheme survives as a URL and is used as-is.
        let candidates = initial_candidates_with(&uri, &[]);
        assert!(candidates.contains(&format!("http://plain.example/{HASH}.png")));
    }

    #[test]
    fn candidates_dedup_across_phases() {
        let uri: BlossomUri = format!("blossom:{HASH}.png?xs=a.example").parse().unwrap();
        let author = Url::parse("https://a.example").unwrap();
        let candidates = candidates_with_authors_with(
            &uri,
            std::slice::from_ref(&author),
            &["https://fallback.example".to_string()],
        );
        assert_eq!(
            candidates
                .iter()
                .filter(|c| *c == &format!("https://a.example/{HASH}.png"))
                .count(),
            1
        );
    }

    #[test]
    fn sanitizes_reason_control_chars() {
        // Bidi override and control chars become spaces (word boundaries
        // preserved), whitespace collapses, length clamps.
        assert_eq!(sanitize_server_text("ok\u{202e}evil"), "ok evil");
        assert_eq!(sanitize_server_text("a\n\u{7}b   c"), "a b c");
        assert_eq!(sanitize_server_text(&"x".repeat(300)).len(), 200);
    }

    #[test]
    fn extracts_hashes_from_imeta_and_blossom_uris() {
        use nostr::event::{EventBuilder, FinalizeEvent, Tag};
        let keys = nostr::key::Keys::generate();
        let content = format!("pic blossom:{HASH}.jpg?xs=cdn.example&sz=1");
        let builder = EventBuilder::new(nostr::event::Kind::TextNote, content.clone()).tags(vec![
            Tag::custom(
                "imeta",
                vec![format!("url https://other.example/{HASH}.png")],
            ),
        ]);
        let event = builder.finalize(&keys).unwrap();
        assert_eq!(extract_blob_hashes(&event), vec![HASH.to_string()]);
    }
}
