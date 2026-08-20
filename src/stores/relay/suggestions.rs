//! Relay URL autocomplete suggestions (issue #359).
//!
//! Aggregated from: live pool connections (with observed traffic), the
//! outbox coverage map, the app's curated `DEFAULT_*` lists, and NIP-66
//! relay-discovery monitor data (merged in from a one-shot background
//! fetch). The global list persists for the session; re-seeding merges and
//! keeps the best traffic/RTT info per relay.

use crate::stores::relay::RelayDisplayInfo;
use crate::utils::relay::{display_relay_url, normalize_known_relay_url};
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

/// One autocomplete candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct RelaySuggestion {
    /// Normalized relay URL (`wss://…/` or `ws://…/`).
    pub url: String,
    /// Compact display label (hostname[:port]).
    pub label: String,
    /// Bytes observed flowing from this relay in the live pool.
    pub bytes_received: u64,
    /// NIP-66 monitor RTT in milliseconds, when known.
    pub rtt_open: Option<u64>,
}

/// Global autocomplete suggestion pool, sorted by relevance
/// (traffic desc → RTT asc → URL).
pub static RELAY_SUGGESTIONS: GlobalSignal<Vec<RelaySuggestion>> = Signal::global(Vec::new);

/// Number of suggestions rendered in the dropdown.
pub const MAX_DROPDOWN_SUGGESTIONS: usize = 8;

/// Sort by observed traffic desc, then NIP-66 RTT asc, then URL.
pub fn sort_suggestions(list: &mut [RelaySuggestion]) {
    list.sort_by(|a, b| {
        b.bytes_received
            .cmp(&a.bytes_received)
            .then_with(|| match (a.rtt_open, b.rtt_open) {
                (Some(a), Some(b)) => a.cmp(&b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| a.url.cmp(&b.url))
    });
}

/// Pure: merge incoming suggestions into a list, deduping by normalized
/// URL and keeping the best traffic (max) / RTT (min) from either side.
/// The result is sorted.
pub fn merge_suggestions(
    existing: &[RelaySuggestion],
    incoming: Vec<RelaySuggestion>,
) -> Vec<RelaySuggestion> {
    let mut by_url: HashMap<String, RelaySuggestion> = HashMap::new();
    for mut suggestion in existing.iter().cloned().chain(incoming) {
        suggestion.url = normalize_known_relay_url(&suggestion.url);
        match by_url.get_mut(&suggestion.url) {
            Some(prev) => {
                prev.bytes_received = prev.bytes_received.max(suggestion.bytes_received);
                prev.rtt_open = match (prev.rtt_open, suggestion.rtt_open) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (Some(a), None) => Some(a),
                    (None, b) => b,
                };
            }
            None => {
                by_url.insert(suggestion.url.clone(), suggestion);
            }
        }
    }
    let mut merged: Vec<RelaySuggestion> = by_url.into_values().collect();
    sort_suggestions(&mut merged);
    merged
}

fn scheme_stripped(url: &str) -> String {
    url.trim_start_matches("wss://")
        .trim_start_matches("ws://")
        .to_lowercase()
}

/// Pure: substring-filter suggestions for a query, excluding URLs already
/// present in the section. Matches the full URL, the scheme-stripped URL,
/// and the display label.
pub fn filter_suggestions(
    suggestions: &[RelaySuggestion],
    query: &str,
    existing: &[String],
    limit: usize,
) -> Vec<RelaySuggestion> {
    let query_lower = query.trim().to_lowercase();
    if query_lower.is_empty() {
        return Vec::new();
    }
    let excluded: HashSet<String> = existing
        .iter()
        .map(|url| normalize_known_relay_url(url))
        .collect();
    suggestions
        .iter()
        .filter(|s| {
            !excluded.contains(&s.url)
                && !excluded.contains(&normalize_known_relay_url(&s.url))
        })
        .filter(|s| {
            s.url.to_lowercase().contains(&query_lower)
                || scheme_stripped(&s.url).contains(&query_lower)
                || s.label.to_lowercase().contains(&query_lower)
        })
        .take(limit)
        .cloned()
        .collect()
}

fn make_suggestion(url: &str, bytes_received: u64, rtt_open: Option<u64>) -> RelaySuggestion {
    RelaySuggestion {
        url: normalize_known_relay_url(url),
        label: display_relay_url(url),
        bytes_received,
        rtt_open,
    }
}

/// Curated defaults across every relay category.
fn curated_defaults() -> Vec<&'static str> {
    let mut urls: Vec<&'static str> = super::pool::DEFAULT_RELAYS.to_vec();
    urls.extend(super::nip65::DEFAULT_NIP65_RELAYS.iter().copied());
    urls.extend(super::nip65::DEFAULT_DM_RELAYS.iter().copied());
    urls.extend(super::nip65::DEFAULT_SEARCH_RELAYS.iter().copied());
    urls.extend(super::nip65::DEFAULT_INDEXER_RELAYS.iter().copied());
    urls.extend(super::nip65::DEFAULT_FAVORITE_RELAYS.iter().copied());
    urls
}

/// Seed the suggestion pool from live pool stats (traffic-sorted), the
/// outbox coverage map, and the curated defaults. Merges into whatever is
/// already cached; idempotent.
pub fn seed_base_suggestions(connection_info: &[RelayDisplayInfo]) {
    let mut incoming: Vec<RelaySuggestion> = connection_info
        .iter()
        .map(|info| make_suggestion(&info.url, info.bytes_received as u64, None))
        .collect();
    for url in super::coverage::known_coverage_urls() {
        incoming.push(make_suggestion(&url, 0, None));
    }
    for url in curated_defaults() {
        incoming.push(make_suggestion(url, 0, None));
    }
    let merged = merge_suggestions(&RELAY_SUGGESTIONS.read(), incoming);
    *RELAY_SUGGESTIONS.write() = merged;
}

/// One-shot-per-session guard for the NIP-66 background fetch.
static NIP66_FETCHED: Mutex<bool> = Mutex::new(false);

/// Fetch NIP-66 relay discovery events from connected relays in the
/// background and merge them (with monitor RTTs) into [`RELAY_SUGGESTIONS`].
/// Runs at most once per app session; safe to call repeatedly.
///
/// Must be called from a Dioxus scope (spawns via
/// [`crate::platform::spawn::spawn_forever_catch_unwind`]).
pub fn spawn_nip66_suggestions_fetch() {
    {
        let Ok(mut fetched) = NIP66_FETCHED.lock() else {
            return;
        };
        if *fetched {
            return;
        }
        *fetched = true;
    }
    crate::platform::spawn::spawn_forever_catch_unwind("relay-suggestions-nip66", async move {
        let filter = crate::utils::nip66::discovery_filter(300);
        let Ok(events) = crate::stores::nostr_client::fetch_events_from_connected_relays(
            filter,
            Duration::from_secs(15),
        )
        .await
        else {
            return;
        };
        let parsed: Vec<_> = events
            .iter()
            .filter_map(crate::utils::nip66::parse_relay_discovery)
            .collect();
        let aggregated = crate::utils::nip66::aggregate_discoveries(&parsed);
        let incoming: Vec<RelaySuggestion> = aggregated
            .into_iter()
            .filter_map(|discovery| {
                let url = normalize_known_relay_url(&discovery.relay_url);
                // Only suggest secure, clearnet-typable relays.
                if !url.starts_with("wss://") {
                    return None;
                }
                Some(make_suggestion(&url, 0, discovery.rtt_open))
            })
            .collect();
        if incoming.is_empty() {
            return;
        }
        let merged = merge_suggestions(&RELAY_SUGGESTIONS.read(), incoming);
        *RELAY_SUGGESTIONS.write() = merged;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suggestion(url: &str, traffic: u64, rtt: Option<u64>) -> RelaySuggestion {
        RelaySuggestion {
            url: url.to_string(),
            label: display_relay_url(url),
            bytes_received: traffic,
            rtt_open: rtt,
        }
    }

    #[test]
    fn merge_dedupes_and_keeps_best_metrics() {
        let existing = vec![suggestion("wss://a.relay/", 100, Some(300))];
        let incoming = vec![
            suggestion("wss://a.relay", 500, None), // same relay, no slash
            suggestion("wss://b.relay/", 0, Some(80)),
        ];
        let merged = merge_suggestions(&existing, incoming);
        assert_eq!(merged.len(), 2);
        let a = merged.iter().find(|s| s.label == "a.relay").unwrap();
        assert_eq!(a.bytes_received, 500);
        assert_eq!(a.rtt_open, Some(300));
    }

    #[test]
    fn sort_prefers_traffic_then_low_rtt() {
        let mut list = vec![
            suggestion("wss://c.relay/", 10, Some(50)),
            suggestion("wss://a.relay/", 100, Some(500)),
            suggestion("wss://b.relay/", 100, Some(90)),
            suggestion("wss://d.relay/", 100, None),
        ];
        sort_suggestions(&mut list);
        let urls: Vec<&str> = list.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(urls, ["b.relay", "a.relay", "d.relay", "c.relay"]);
    }

    #[test]
    fn filter_matches_scheme_stripped_and_excludes_existing() {
        let pool = vec![
            suggestion("wss://relay.damus.io/", 10, None),
            suggestion("wss://purplepag.es/", 5, None),
        ];
        let filtered = filter_suggestions(
            &pool,
            "damus",
            &["wss://purplepag.es".to_string()],
            8,
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].url, "wss://relay.damus.io/");

        // Empty query → no suggestions
        assert!(filter_suggestions(&pool, "  ", &[], 8).is_empty());

        // Exact existing relay is excluded even with slash-normalization
        assert!(filter_suggestions(&pool, "damus", &["wss://relay.damus.io".to_string()], 8).is_empty());
    }
}
