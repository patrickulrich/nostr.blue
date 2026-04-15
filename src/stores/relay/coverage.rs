//! Relay coverage map
//!
//! Builds a pubkey → relay URLs mapping from observed NIP-65 events.
//! Used to determine which relays to query for a specific user's events,
//! supplementing the SDK's gossip model for targeted fetches.
use crate::stores::relay::nip65::USER_RELAY_METADATA;
use dioxus::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct RelayCoverageMap {
    user_relays: HashMap<String, Vec<String>>,
}

pub static RELAY_COVERAGE: GlobalSignal<RelayCoverageMap> =
    Signal::global(RelayCoverageMap::default);

/// Record which relays a user publishes to (from NIP-65 kind 10002 events).
#[allow(dead_code)]
pub fn record_user_relays(pubkey: &str, relay_urls: Vec<String>) {
    let mut coverage = RELAY_COVERAGE.write();
    if !relay_urls.is_empty() {
        coverage.user_relays.insert(pubkey.to_string(), relay_urls);
    }
}

/// Get the best relay URLs for fetching a given user's events.
/// Falls back to the current user's own read relays, then defaults.
#[allow(dead_code)]
pub fn get_relays_for_pubkey(pubkey: &str) -> Vec<String> {
    let coverage = RELAY_COVERAGE.peek();
    if let Some(relays) = coverage.user_relays.get(pubkey) {
        if !relays.is_empty() {
            return relays.clone();
        }
    }

    let metadata = USER_RELAY_METADATA.peek();
    match metadata.as_ref() {
        Some(m) => m
            .relays
            .iter()
            .filter(|r| r.read)
            .map(|r| r.url.clone())
            .collect(),
        None => crate::stores::relay::nip65::DEFAULT_NIP65_RELAYS
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

/// Clear the coverage map (on logout).
#[allow(dead_code)]
pub fn clear_coverage() {
    RELAY_COVERAGE.write().user_relays.clear();
}

/// Number of users tracked in the coverage map.
#[allow(dead_code)]
pub fn coverage_size() -> usize {
    RELAY_COVERAGE.peek().user_relays.len()
}
