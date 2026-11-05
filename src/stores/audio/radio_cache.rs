//! Session-level radio list cache — stale-while-revalidate.
//!
//! /radio's effect re-runs on every genre/search/favorites switch and used
//! to refetch from zero each time (skeletons → full relay round trip).
//! This cache keys the completed lists by mode so a switch paints the
//! previous result instantly and refreshes in the background when stale
//! (no skeleton flash). In-memory only: cross-session persistence comes
//! free from the SDK database (see `utils::radio::fetch_radio_stations_from_db`).
//!
//! Consume with [`peek`] — deliberately non-subscribing. A reactive read in
//! the radio home effect would make fetch-completion writes re-run the
//! effect (write → effect → refetch → write …). `peek` opts out of
//! subscription, the pattern the Dioxus docs prescribe for exactly this.

use crate::utils::radio::RadioStation;
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};

/// Millisecond clock. Gated on target arch (not the `web` feature) so host
/// unit tests with the default feature set don't hit wasm-bindgen panics —
/// same pattern as `nostr_client::fetching::now_ms`.
fn now_millis() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// How long a cached list is served without any network refresh.
const FRESH_TTL_MS: u64 = 2 * 60 * 1000;

/// A completed /radio list page.
#[derive(Clone, Debug, PartialEq)]
pub struct CachedRadioList {
    /// The rendered page (sorted newest-first, truncated to the page size).
    pub stations: Vec<RadioStation>,
    /// `platform::timestamp::now_millis()` when the page was fetched.
    pub fetched_at_ms: u64,
    /// Favorites coordinate snapshot the list was resolved from (favorites
    /// key only): a mismatch against the current favorites set marks the
    /// entry stale without any cross-module invalidation hookups — toggles
    /// and logout (which clears the whole cache via `reset` alongside
    /// `radio_favorites::reset`) are covered.
    pub favorites_snapshot: Option<HashSet<String>>,
}

impl CachedRadioList {
    /// Whether the TTL is still unexpired.
    pub fn fresh(&self) -> bool {
        now_millis().saturating_sub(self.fetched_at_ms) < FRESH_TTL_MS
    }

    /// Whether the entry may be served without a refresh: within the TTL
    /// and (favorites lists) resolved from the same favorites set.
    pub fn usable_without_refresh(&self, current_favorites: Option<&HashSet<String>>) -> bool {
        let snapshot_matches = match (&self.favorites_snapshot, current_favorites) {
            (Some(snapshot), Some(current)) => snapshot == current,
            (None, None) => true,
            // A favorites entry compared against no favorites signal, or a
            // non-favorites entry carrying a snapshot: treat as stale.
            _ => false,
        };
        self.fresh() && snapshot_matches
    }
}

/// Cached pages by list key (`genre:{g}` / `search:{q}` / `favorites`).
pub static RADIO_LIST_CACHE: GlobalSignal<HashMap<String, CachedRadioList>> =
    Signal::global(HashMap::new);

/// Cache key for a genre page ("all" included).
pub fn genre_key(genre: &str) -> String {
    format!("genre:{genre}")
}

/// Cache key for a search results page.
pub fn search_key(query: &str) -> String {
    format!("search:{query}")
}

/// Cache key for the resolved favorites page.
pub const FAVORITES_KEY: &str = "favorites";

/// Non-subscribing cache read (see the module doc for why not `.read()`).
pub fn peek(key: &str) -> Option<CachedRadioList> {
    RADIO_LIST_CACHE.peek().get(key).cloned()
}

/// Store a completed page.
pub fn store(key: &str, stations: Vec<RadioStation>, favorites_snapshot: Option<HashSet<String>>) {
    RADIO_LIST_CACHE.write().insert(
        key.to_string(),
        CachedRadioList {
            stations,
            fetched_at_ms: now_millis(),
            favorites_snapshot,
        },
    );
}

/// Extend an existing entry with a pagination page (deduped by coordinate).
/// Entries that don't exist (e.g. the list was painted DB-first and the
/// network page hasn't landed yet) are left alone — the next completed
/// fetch stores the full page.
pub fn append_page(key: &str, more: &[RadioStation]) {
    let mut cache = RADIO_LIST_CACHE.write();
    let Some(entry) = cache.get_mut(key) else {
        return;
    };
    for station in more {
        if !entry
            .stations
            .iter()
            .any(|s| s.coordinate == station.coordinate)
        {
            entry.stations.push(station.clone());
        }
    }
}

/// Clear everything (logout/user switch — called from
/// `radio_favorites::reset`).
pub fn clear_all() {
    RADIO_LIST_CACHE.write().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(fetched_at_ms: u64, favorites_snapshot: Option<HashSet<String>>) -> CachedRadioList {
        CachedRadioList {
            stations: Vec::new(),
            fetched_at_ms,
            favorites_snapshot,
        }
    }

    #[test]
    fn test_fresh_within_ttl_and_stale_after() {
        let now = now_millis();
        assert!(entry(now, None).fresh());
        assert!(entry(now.saturating_sub(FRESH_TTL_MS - 1), None).fresh());
        assert!(!entry(now.saturating_sub(FRESH_TTL_MS), None).fresh());
    }

    #[test]
    fn test_usable_requires_snapshot_match_for_favorites() {
        let now = now_millis();
        let mut set = HashSet::new();
        set.insert("31237:aa:st-1".to_string());

        // Favorites entry matching the current set: usable.
        assert!(entry(now, Some(set.clone())).usable_without_refresh(Some(&set)));
        // Different favorites set: stale (a toggle happened).
        let mut other = HashSet::new();
        other.insert("31237:bb:st-2".to_string());
        assert!(!entry(now, Some(set.clone())).usable_without_refresh(Some(&other)));
        // No current set available: stale, don't serve blind.
        assert!(!entry(now, Some(set)).usable_without_refresh(None));
        // Non-favorites entries carry no snapshot and match nothing.
        assert!(entry(now, None).usable_without_refresh(None));
        assert!(!entry(now, None).usable_without_refresh(Some(&HashSet::new())));
    }
}
