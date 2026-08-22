//! Session-level NIP-11 relay information cache (issue #359).
//!
//! Enriches relay rows in the settings screen (name, icon, paid badge)
//! without refetching the NIP-11 document on every visit. Fetch failures
//! (common on web, where many relays lack CORS headers for `Accept:
//! application/nostr+json`) are negative-cached with an attempt budget so
//! they don't cause refetch storms.
//!
//! Concurrency is bounded (see [`MAX_CONCURRENT_FETCHES`]) and duplicate
//! in-flight requests are coalesced through [`IN_FLIGHT`].

use crate::utils::relay::{fetch_nip11_body, normalize_known_relay_url, relay_http_url};
use dioxus::prelude::*;
use nostr_sdk::nips::nip11::RelayInformationDocument;
use nostr_sdk::prelude::JsonUtil;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Compact row-level view of a relay's NIP-11 document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Nip11RowInfo {
    pub name: Option<String>,
    pub icon: Option<String>,
    /// `limitations.payment_required == true`
    pub paid: bool,
}

impl Nip11RowInfo {
    pub fn from_document(doc: &RelayInformationDocument) -> Self {
        Self {
            name: doc.name.clone().filter(|n| !n.trim().is_empty()),
            icon: doc
                .icon
                .clone()
                .filter(|i| crate::utils::is_valid_http_url(i)),
            paid: doc
                .limitation
                .as_ref()
                .and_then(|l| l.payment_required)
                .unwrap_or(false),
        }
    }
}

/// Session cache: fetched documents plus failure counters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Nip11Cache {
    /// Successfully fetched row info, keyed by normalized relay URL.
    docs: HashMap<String, Nip11RowInfo>,
    /// Failed fetch attempts per normalized relay URL (negative cache).
    failed: HashMap<String, u32>,
}

/// Global NIP-11 row-info cache.
pub static NIP11_INFO: GlobalSignal<Nip11Cache> = Signal::global(Nip11Cache::default);

/// Give up on a relay's NIP-11 document after this many failed attempts
/// (per app session). Failures are usually permanent (no CORS / no doc).
const MAX_FAILURES: u32 = 2;
/// Bound concurrent NIP-11 HTTP fetches (mirrors `coverage.rs` patterns).
const MAX_CONCURRENT_FETCHES: usize = 5;

/// In-flight fetch claims (normalized URLs) for request coalescing.
/// The lock is never held across an await point. (A `Vec` rather than a
/// `HashSet` because `HashSet::new` is not a const fn for statics; the
/// claim list stays tiny.)
static IN_FLIGHT: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Parse a NIP-11 document body into [`Nip11RowInfo`].
fn parse_row_info(body: &str) -> Result<Nip11RowInfo, String> {
    let doc = RelayInformationDocument::from_json(body)
        .map_err(|e| format!("Failed to parse relay metadata: {}", e))?;
    Ok(Nip11RowInfo::from_document(&doc))
}

/// Fetch and parse the NIP-11 document for a single (normalized) URL.
async fn fetch_one(url: &str) -> Result<Nip11RowInfo, String> {
    let http_url = relay_http_url(url)?;
    let body = fetch_nip11_body(&http_url).await?;
    parse_row_info(&body)
}

/// Pure subset of the pending-selection logic: given cache state and
/// in-flight claims, return the URLs (normalized + deduped) that still need
/// fetching.
fn select_pending_from(
    cache: &Nip11Cache,
    in_flight: &[String],
    relay_urls: &[String],
    is_blocked: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut pending = Vec::new();
    for url in relay_urls {
        let normalized = normalize_known_relay_url(url);
        if !seen.insert(normalized.clone()) {
            continue;
        }
        if cache.docs.contains_key(&normalized)
            || cache.failed.get(&normalized).is_some_and(|n| *n >= MAX_FAILURES)
            || in_flight.contains(&normalized)
            || is_blocked(&normalized)
        {
            continue;
        }
        pending.push(normalized);
    }
    pending
}

/// Normalize + dedup the given relay URLs and kick off NIP-11 fetches for
/// any that are not cached, negative-cached, or already in flight.
/// Idempotent: safe to call repeatedly (e.g. from a `use_effect`).
///
/// Must be called from a Dioxus scope (component/effect/handler) since it
/// spawns via [`crate::platform::spawn::spawn_forever_catch_unwind`] so the
/// cache keeps filling even if the user navigates away mid-fetch.
pub fn ensure_nip11_for(relay_urls: Vec<String>) {
    let pending = {
        let Ok(mut in_flight) = IN_FLIGHT.lock() else {
            return;
        };
        let cache = NIP11_INFO.read();
        let pending = select_pending_from(&cache, &in_flight, &relay_urls, |url| {
            super::is_relay_blocked(url)
        });
        if pending.is_empty() {
            return;
        }
        for url in &pending {
            in_flight.push(url.clone());
        }
        pending
    };
    crate::platform::spawn::spawn_forever_catch_unwind("nip11-info-fetch", async move {
        use futures::StreamExt;
        futures::stream::iter(pending)
            .map(|url| async move {
                let outcome = fetch_one(&url).await;
                (url, outcome)
            })
            .buffer_unordered(MAX_CONCURRENT_FETCHES)
            .for_each(|(url, outcome)| {
                record_outcome(url, outcome);
                futures::future::ready(())
            })
            .await;
    });
}

/// Record a fetch outcome: success inserts into the cache; failure bumps the
/// negative cache. Always releases the in-flight claim.
fn record_outcome(url: String, outcome: Result<Nip11RowInfo, String>) {
    if let Err(e) = &outcome {
        log::debug!("NIP-11 fetch failed for {}: {}", url, e);
    }
    if let Ok(mut in_flight) = IN_FLIGHT.lock() {
        in_flight.retain(|claimed| claimed != &url);
    }
    let mut cache = NIP11_INFO.write();
    match outcome {
        Ok(info) => {
            cache.docs.insert(url, info);
        }
        Err(_) => {
            *cache.failed.entry(url).or_insert(0) += 1;
        }
    }
}

/// Look up cached row info for a relay URL. Subscribes to [`NIP11_INFO`],
/// so components re-render as documents trickle in.
pub fn lookup(relay_url: &str) -> Option<Nip11RowInfo> {
    let cache = NIP11_INFO.read();
    let key = normalize_known_relay_url(relay_url);
    cache.docs.get(&key).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_with(docs: &[(&str, Nip11RowInfo)], failed: &[(&str, u32)]) -> Nip11Cache {
        Nip11Cache {
            docs: docs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            failed: failed
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
        }
    }

    #[test]
    fn row_info_extracts_name_icon_and_paid() {
        let body = r#"{
            "name": "Example Relay",
            "icon": "https://example.com/icon.png",
            "limitation": { "payment_required": true }
        }"#;
        let info = parse_row_info(body).expect("should parse");
        assert_eq!(info.name.as_deref(), Some("Example Relay"));
        assert_eq!(info.icon.as_deref(), Some("https://example.com/icon.png"));
        assert!(info.paid);
    }

    #[test]
    fn row_info_defaults_paid_false_and_filters_bad_icon() {
        let body = r#"{ "name": "R", "icon": "not-a-url" }"#;
        let info = parse_row_info(body).expect("should parse");
        assert!(!info.paid);
        assert_eq!(info.icon, None);
    }

    #[test]
    fn row_info_rejects_invalid_json() {
        assert!(parse_row_info("not json").is_err());
    }

    #[test]
    fn select_pending_skips_cached_failed_blocked_and_duplicates() {
        let cache = cache_with(
            &[("wss://cached.relay/", Nip11RowInfo::default())],
            &[("wss://failed.relay/", MAX_FAILURES)],
        );
        let in_flight: Vec<String> = vec!["wss://inflight.relay/".to_string()];

        let pending = select_pending_from(
            &cache,
            &in_flight,
            &[
                "wss://cached.relay".to_string(),
                "wss://cached.relay/".to_string(),
                "wss://failed.relay".to_string(),
                "wss://inflight.relay".to_string(),
                "wss://blocked.relay".to_string(),
                "wss://new.relay".to_string(),
                "wss://new.relay".to_string(),
            ],
            |url| url.contains("blocked"),
        );

        assert_eq!(pending, vec!["wss://new.relay/".to_string()]);
    }

    #[test]
    fn select_pending_retries_under_failure_budget() {
        let cache = cache_with(&[], &[("wss://flaky.relay/", 1)]);
        let pending = select_pending_from(
            &cache,
            &[],
            &["wss://flaky.relay".to_string()],
            |_| false,
        );
        assert_eq!(pending, vec!["wss://flaky.relay/".to_string()]);
    }
}
