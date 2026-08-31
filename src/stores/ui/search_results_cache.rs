//! Cross-tab search result cache (~2min TTL).
//!
//! Tab switches re-render the previous tab's results instantly while a fresh
//! search refreshes in the background. Session-scoped (GlobalSignal, not
//! persisted) and bounded to the most recent `MAX_ENTRIES` (query, tab)
//! entries.

use crate::services::content_search::ContentSearchResult;
use crate::services::profile_search::ProfileSearchResult;
use dioxus::prelude::*;
use std::collections::HashMap;

/// Cache TTL in seconds.
const TTL_SECS: u64 = 120;
/// Maximum number of cached (query, tab) entries (simple recency bound).
const MAX_ENTRIES: usize = 20;

#[derive(Clone, Debug, Default)]
pub struct CachedTabResults {
    pub content: Vec<ContentSearchResult>,
    pub profiles: Vec<ProfileSearchResult>,
    pub cached_at: u64,
}

pub static SEARCH_RESULTS_CACHE: GlobalSignal<HashMap<String, CachedTabResults>> =
    Signal::global(HashMap::new);

fn cache_key(query: &str, tab: &str) -> String {
    format!("{}\u{1}{}", query.to_lowercase(), tab)
}

fn now_secs() -> u64 {
    crate::platform::timestamp::now_secs()
}

/// Fetch fresh cached results for (query, tab), if any.
pub fn get_cached(query: &str, tab: &str) -> Option<CachedTabResults> {
    let cache = SEARCH_RESULTS_CACHE.peek();
    let entry = cache.get(&cache_key(query, tab))?;
    if now_secs().saturating_sub(entry.cached_at) > TTL_SECS {
        return None;
    }
    Some(entry.clone())
}

/// Store results for (query, tab), evicting beyond `MAX_ENTRIES` (oldest
/// `cached_at` first).
pub fn store(query: &str, tab: &str, content: Vec<ContentSearchResult>, profiles: Vec<ProfileSearchResult>) {
    let mut cache = SEARCH_RESULTS_CACHE.write();
    if cache.len() >= MAX_ENTRIES && !cache.contains_key(&cache_key(query, tab)) {
        if let Some(oldest_key) = cache
            .iter()
            .min_by_key(|(_, entry)| entry.cached_at)
            .map(|(k, _)| k.clone())
        {
            cache.remove(&oldest_key);
        }
    }
    cache.insert(
        cache_key(query, tab),
        CachedTabResults {
            content,
            profiles,
            cached_at: now_secs(),
        },
    );
}

/// Invalidate all cached results (e.g. on logout).
pub fn invalidate_all() {
    SEARCH_RESULTS_CACHE.write().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_case_insensitive_and_tab_scoped() {
        assert_eq!(cache_key("Hello", "posts"), cache_key("hello", "posts"));
        assert_ne!(cache_key("hello", "posts"), cache_key("hello", "people"));
    }
}
