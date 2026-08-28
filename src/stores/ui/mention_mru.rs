//! Most-recently-mentioned users (MRU) store for bare-`@` suggestions.
//!
//! Mirrors the `search_history.rs` pattern: a process-wide OnceLock<Mutex>
//! cache hydrated from localStorage on first use and persisted on mutation.

use crate::stores::profiles::get_cached_profile;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const MENTION_MRU_KEY: &str = "nostr_blue_mention_mru";
const MAX_MRU: usize = 10;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MentionMru {
    pubkeys: Vec<String>,
}

static MRU_CACHE: std::sync::OnceLock<std::sync::Mutex<MentionMru>> =
    std::sync::OnceLock::new();

fn get_cache() -> &'static std::sync::Mutex<MentionMru> {
    MRU_CACHE.get_or_init(|| {
        let mru = crate::platform::storage::get::<MentionMru>(MENTION_MRU_KEY)
            .unwrap_or_default();
        std::sync::Mutex::new(mru)
    })
}

fn save_to_storage(mru: &MentionMru) {
    if let Err(e) = crate::platform::storage::set(MENTION_MRU_KEY, mru) {
        log::warn!("Failed to save mention MRU: {}", e);
    }
}

/// Record that `pubkey` was mentioned. Moves it to the front of the MRU list.
pub fn record_mention(pubkey: &PublicKey) {
    let hex = pubkey.to_hex();
    let cache = get_cache();
    let mut mru = cache.lock().unwrap_or_else(|e| e.into_inner());
    mru.pubkeys.retain(|p| p != &hex);
    mru.pubkeys.insert(0, hex);
    mru.pubkeys.truncate(MAX_MRU);
    save_to_storage(&mru);
}

/// Read the MRU list as search results, skipping users with no cached profile.
pub fn get_mru_results(limit: usize) -> Vec<crate::services::profile_search::ProfileSearchResult> {
    let hexes: Vec<String> = {
        let cache = get_cache();
        let mru = cache.lock().unwrap_or_else(|e| e.into_inner());
        mru.pubkeys.clone()
    };
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    for hex in hexes.iter().take(limit) {
        if !seen.insert(hex.clone()) {
            continue;
        }
        let Ok(pk) = PublicKey::from_hex(hex) else {
            continue;
        };
        if let Some(profile) = get_cached_profile(hex) {
            results.push(crate::services::profile_search::ProfileSearchResult {
                pubkey: pk,
                name: profile.name.clone(),
                display_name: profile.display_name.clone(),
                picture: profile.picture.clone(),
                nip05: profile.nip05.clone(),
                is_contact: false,
                is_thread_participant: false,
                relevance: 50,
            });
        }
    }
    results
}

/// Session-scoped mention relay hints learned from NIP-05 resolutions (the
/// nostr.json `relays` map). Merged with the outbox coverage map when
/// building nprofile mention hints.
static HINTS_CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, Vec<String>>>> =
    std::sync::OnceLock::new();

fn get_hints_cache(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, Vec<String>>> {
    HINTS_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Record relay hints for a pubkey (e.g. from a resolved nostr.json document).
pub fn record_hints(pubkey_hex: &str, relays: &[RelayUrl]) {
    if relays.is_empty() {
        return;
    }
    let cache = get_hints_cache();
    let mut hints = cache.lock().unwrap_or_else(|e| e.into_inner());
    let entry = hints.entry(pubkey_hex.to_string()).or_default();
    for relay in relays {
        let url = relay.to_string();
        if !entry.contains(&url) {
            entry.push(url);
        }
    }
    entry.truncate(5);
}

/// Retrieve recorded hints for a pubkey (may be empty).
pub fn get_hints(pubkey_hex: &str) -> Vec<String> {
    let cache = get_hints_cache();
    let hints = cache.lock().unwrap_or_else(|e| e.into_inner());
    hints.get(pubkey_hex).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mru_serialization_round_trip() {
        let mut mru = MentionMru::default();
        mru.pubkeys.push("aa".to_string());
        mru.pubkeys.push("bb".to_string());
        let json = serde_json::to_string(&mru).unwrap();
        let back: MentionMru = serde_json::from_str(&json).unwrap();
        assert_eq!(back.pubkeys, vec!["aa".to_string(), "bb".to_string()]);
    }
}
