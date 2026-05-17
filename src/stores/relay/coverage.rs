//! Relay coverage map and three-tier relay resolver
//!
//! Builds a pubkey → relay URLs mapping from observed NIP-65 events, database queries,
//! and network fetches. Supplements the SDK's gossip model for targeted fetches.
//!
//! # Three-tier resolution
//!
//! 1. In-memory `RELAY_COVERAGE` cache (instant, session-only)
//! 2. SDK database via `relay_list(pubkey)` (instant, persistent across sessions)
//! 3. Network fetch of kind 10002 (slow, populates both tiers above)
//!
//! # Fallback
//!
//! When no NIP-65 data exists, uses event provenance (which relay delivered a user's
//! events) and p-tag relay hints as fallback relay sources.
use crate::stores::nostr_client;
use crate::stores::relay::nip65::{parse_relay_list_event, USER_RELAY_METADATA, DEFAULT_NIP65_RELAYS};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct RelayCoverageMap {
    user_relays: std::collections::HashMap<String, Vec<String>>,
    provenance: std::collections::HashMap<String, Vec<String>>,
    hints: std::collections::HashMap<String, Vec<String>>,
}

pub static RELAY_COVERAGE: GlobalSignal<RelayCoverageMap> =
    Signal::global(RelayCoverageMap::default);

/// Which relay purpose to filter for when resolving a user's relays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayPurpose {
    #[allow(dead_code)]
    Read,
    Write,
    #[allow(dead_code)]
    All,
}

fn extract_relays_from_map(
    relays_map: &std::collections::HashMap<RelayUrl, Option<RelayMetadata>>,
    purpose: RelayPurpose,
) -> Vec<String> {
    relays_map
        .iter()
        .filter(|(_, marker)| match purpose {
            RelayPurpose::Read => marker.is_none() || matches!(marker, Some(RelayMetadata::Read)),
            RelayPurpose::Write => {
                marker.is_none() || matches!(marker, Some(RelayMetadata::Write))
            }
            RelayPurpose::All => true,
        })
        .map(|(url, _)| url.to_string())
        .collect()
}

fn fallback_relays_for(pubkey: &str) -> Vec<String> {
    let coverage = RELAY_COVERAGE.peek();
    let mut relays = Vec::new();
    if let Some(prov) = coverage.provenance.get(pubkey) {
        relays.extend(prov.clone());
    }
    if let Some(hints) = coverage.hints.get(pubkey) {
        for h in hints {
            if !relays.contains(h) {
                relays.push(h.clone());
            }
        }
    }
    if relays.is_empty() {
        DEFAULT_NIP65_RELAYS.iter().map(|s| s.to_string()).collect()
    } else {
        relays
    }
}

/// Three-tier relay resolver.
///
/// 1. In-memory cache → instant
/// 2. SDK database `relay_list()` → instant, persistent
/// 3. Network fetch → slow, populates both tiers
///
/// Falls back to event provenance + p-tag hints when no NIP-65 data exists.
pub async fn resolve_user_relays(pubkey: &str, purpose: RelayPurpose) -> Vec<String> {
    {
        let coverage = RELAY_COVERAGE.peek();
        if let Some(relays) = coverage.user_relays.get(pubkey) {
            if !relays.is_empty() {
                return relays.clone();
            }
        }
    }

    if let Some(client) = nostr_client::get_client() {
        if let Ok(pk) = PublicKey::from_hex(pubkey) {
            if let Ok(relays_map) = client.database().relay_list(pk).await {
                if !relays_map.is_empty() {
                    let urls = extract_relays_from_map(&relays_map, purpose);
                    if !urls.is_empty() {
                        record_user_relays(pubkey, &urls);
                        let pk_bg = pubkey.to_string();
                        dioxus::prelude::spawn(async move {
                            let _ = refresh_10002_from_network(&pk_bg).await;
                        });
                        return urls;
                    }
                }
            }
        }
    }

    match fetch_10002_from_network(pubkey).await {
        Some(urls) => {
            record_user_relays(pubkey, &urls);
            urls
        }
        None => fallback_relays_for(pubkey),
    }
}

/// Fetch kind 10002 from network for a single pubkey.
/// Returns parsed relay URLs (all, unfiltered by purpose).
async fn fetch_10002_from_network(pubkey: &str) -> Option<Vec<String>> {
    let client = nostr_client::get_client()?;
    let pk = PublicKey::from_hex(pubkey).ok()?;
    let filter = Filter::new().author(pk).kind(Kind::RelayList).limit(1);
    let result = client.fetch_events(filter, Duration::from_secs(5)).await;
    match result {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                let relays = parse_relay_list_event(&event);
                let urls: Vec<String> = relays.iter().map(|r| r.url.clone()).collect();
                if !urls.is_empty() {
                    record_user_relays(pubkey, &urls);
                    return Some(urls);
                }
            }
            None
        }
        Err(e) => {
            log::debug!("Failed to fetch 10002 for {}: {}", pubkey, e);
            None
        }
    }
}

/// Background refresh: fetch kind 10002 from network to update cache.
async fn refresh_10002_from_network(pubkey: &str) -> Option<Vec<String>> {
    let client = nostr_client::get_client()?;
    let pk = PublicKey::from_hex(pubkey).ok()?;
    let filter = Filter::new().author(pk).kind(Kind::RelayList).limit(1);
    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                let relays = parse_relay_list_event(&event);
                let urls: Vec<String> = relays.iter().map(|r| r.url.clone()).collect();
                if !urls.is_empty() {
                    record_user_relays(pubkey, &urls);
                    return Some(urls);
                }
            }
            None
        }
        Err(_) => None,
    }
}

/// Record which relays a user publishes to (from NIP-65 kind 10002 events).
pub fn record_user_relays(pubkey: &str, relay_urls: &[String]) {
    if relay_urls.is_empty() {
        return;
    }
    RELAY_COVERAGE
        .write()
        .user_relays
        .insert(pubkey.to_string(), relay_urls.to_vec());
}

/// Get relay URLs for a pubkey ONLY if we have actual NIP-65 data for them.
/// Returns None if no specific relay data exists (no fallback to generic relays).
/// Use this to decide whether to generate nprofile vs npub.
pub fn get_known_user_relays(pubkey: &str) -> Option<Vec<String>> {
    let coverage = RELAY_COVERAGE.peek();
    coverage.user_relays.get(pubkey).and_then(|relays| {
        if relays.is_empty() {
            None
        } else {
            Some(relays.clone())
        }
    })
}

/// Get the best relay URLs for fetching a given user's events.
/// Falls back to the current user's own read relays, then defaults.
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
        None => DEFAULT_NIP65_RELAYS
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

/// Clear the coverage map (on logout).
#[allow(dead_code)]
pub fn clear_coverage() {
    let mut coverage = RELAY_COVERAGE.write();
    coverage.user_relays.clear();
    coverage.provenance.clear();
    coverage.hints.clear();
}

/// Number of users tracked in the coverage map.
#[allow(dead_code)]
pub fn coverage_size() -> usize {
    RELAY_COVERAGE.peek().user_relays.len()
}

/// Record event provenance: which relay delivered an event by this author.
pub fn record_provenance(pubkey: &str, relay_url: &str) {
    let mut coverage = RELAY_COVERAGE.write();
    let entry = coverage.provenance.entry(pubkey.to_string()).or_default();
    if !entry.contains(&relay_url.to_string()) {
        entry.push(relay_url.to_string());
        if entry.len() > 5 {
            entry.remove(0);
        }
    }
}

/// Record a relay hint from a p-tag (index 2) for a pubkey.
pub fn record_relay_hint(pubkey: &str, relay_url: &str) {
    let url = relay_url.trim();
    if !url.starts_with("wss://") && !url.starts_with("ws://") {
        return;
    }
    let mut coverage = RELAY_COVERAGE.write();
    let entry = coverage.hints.entry(pubkey.to_string()).or_default();
    if !entry.contains(&url.to_string()) {
        entry.push(url.to_string());
        if entry.len() > 5 {
            entry.remove(0);
        }
    }
}

/// Record a kind 10002 event into the coverage map (eager population).
pub fn record_relay_list_from_event(event: &nostr::Event) {
    if event.kind != Kind::RelayList {
        return;
    }
    let relays = parse_relay_list_event(event);
    if !relays.is_empty() {
        let urls: Vec<String> = relays.iter().map(|r| r.url.clone()).collect();
        record_user_relays(&event.pubkey.to_hex(), &urls);
    }
}

/// Record relay list from a RelaysMap (e.g., from batch DB query) into the coverage map.
pub fn record_relay_list_from_event_by_map(
    pk: &PublicKey,
    relays_map: &std::collections::HashMap<RelayUrl, Option<RelayMetadata>>,
) {
    if relays_map.is_empty() {
        return;
    }
    let urls: Vec<String> = relays_map.keys().map(|u| u.to_string()).collect();
    record_user_relays(&pk.to_hex(), &urls);
}

/// Extract and record relay hints from p-tags in an event.
fn extract_and_record_hints(event: &nostr::Event) {
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        if slice.first().map(|s| s.as_str()) == Some("p") && slice.len() >= 3 {
            if let Some(url) = slice.get(2) {
                let url_str = url.as_str();
                if url_str.starts_with("wss://") || url_str.starts_with("ws://") {
                    let pk = slice[1].as_str();
                    record_relay_hint(pk, url_str);
                }
            }
        }
    }
}

/// Result of connecting to relays temporarily for a targeted fetch.
///
/// Separates pre-existing connected relays from newly-added ones so that
/// cleanup only removes relays we actually added, avoiding disconnecting
/// the user's own persistent relays.
#[derive(Clone, Debug, Default)]
pub struct EphemeralResult {
    /// All connected URLs (pre-existing + newly added) — use for querying.
    pub connected: Vec<String>,
    /// Only the URLs we added to the pool — use for cleanup.
    pub newly_added: Vec<String>,
}

/// Connect to relays temporarily for a targeted fetch.
///
/// Two-phase batch approach to avoid pool RwLock contention:
/// - Phase A: Batch `add_relay` for all URLs (write locks, ~1ms each)
/// - Phase B: Parallel `try_connect_relay` with bounded concurrency (read locks coexist)
///
/// Uses `reconnect(false)` so failed connections are not auto-retried.
/// Returns an `EphemeralResult` distinguishing pre-existing relays from newly-added ones.
pub async fn connect_ephemeral_relays(client: &Client, urls: &[String]) -> EphemeralResult {
    use futures::stream::{self, StreamExt};

    let relays = client.relays().await;
    let mut already_connected: Vec<String> = Vec::new();
    let mut to_add: Vec<nostr::Url> = Vec::new();

    for url in urls {
        let Ok(parsed) = nostr::Url::parse(url) else {
            continue;
        };
        let is_connected = relays
            .iter()
            .any(|(u, r)| u.as_str() == url && r.is_connected());
        if is_connected {
            already_connected.push(url.clone());
        } else {
            to_add.push(parsed);
        }
    }

    if to_add.is_empty() {
        return EphemeralResult {
            connected: already_connected,
            newly_added: Vec::new(),
        };
    }

    let opts = RelayOptions::new()
        .reconnect(false)
        .sleep_when_idle(true)
        .idle_timeout(Duration::from_secs(300));
    for url in &to_add {
        let _ = client.pool().add_relay(url.clone(), opts.clone()).await;
    }

    const MAX_CONCURRENT: usize = 5;
    let newly_connected: Vec<String> = stream::iter(to_add)
        .map(|url| {
            let client = client.clone();
            async move {
                if client
                    .pool()
                    .try_connect_relay(url.clone(), Duration::from_secs(3))
                    .await
                    .is_ok()
                {
                    Some(url.to_string())
                } else {
                    None
                }
            }
        })
        .buffer_unordered(MAX_CONCURRENT)
        .filter_map(|r| async { r })
        .collect()
        .await;

    let mut connected = already_connected;
    connected.extend(newly_connected.clone());
    EphemeralResult {
        connected,
        newly_added: newly_connected,
    }
}

/// Remove ephemeral relays from the pool after a targeted fetch.
pub async fn cleanup_ephemeral_relays(client: &Client, urls: &[String]) {
    for url in urls {
        let _ = client.force_remove_relay(url.as_str()).await;
    }
}

/// Remove gossip-only relays from the pool.
///
/// Preserves user-configured relays (which have READ/WRITE flags) and
/// discovery relays (which have DISCOVERY flag). Only removes relays
/// that have GOSSIP flag with no READ, WRITE, or DISCOVERY flags.
#[allow(dead_code)]
pub async fn cleanup_gossip_relays(client: &Client) {
    let all = client.pool().all_relays().await;
    for (url, relay) in all {
        let flags = relay.flags();
        if flags.has_gossip()
            && !flags.has_read()
            && !flags.has_write()
            && !flags.has_discovery()
        {
            let _ = client.force_remove_relay(url.as_str()).await;
        }
    }
}

/// Start a provenance recorder that listens to all relay notifications.
///
/// Records:
/// - Event provenance (which relay delivered events for each author)
/// - p-tag relay hints
/// - Eager kind 10002 recording
///
/// Must be called from within a Dioxus component (uses `spawn_forever`).
pub fn start_provenance_recorder(client: Arc<Client>) {
    dioxus_core::spawn_forever(async move {
        let mut notifications = client.notifications();
        while let Ok(notification) = notifications.recv().await {
            if let RelayPoolNotification::Event {
                relay_url,
                event,
                ..
            } = notification
            {
                let pubkey_hex = event.pubkey.to_hex();
                let relay_str = relay_url.to_string();

                record_provenance(&pubkey_hex, &relay_str);
                extract_and_record_hints(&event);

                if event.kind == Kind::RelayList {
                    record_relay_list_from_event(&event);
                }
            }
        }
    });
}

/// Bulk prefetch relay lists for followed users at startup.
///
/// Queries the SDK database (instant) for all followed users' kind 10002 events,
/// then spawns a background task to fetch missing ones from the network.
pub async fn prefetch_relay_lists_for_follows() {
    let pubkey_str = match crate::stores::auth_store::get_pubkey() {
        Some(pk) => pk,
        None => return,
    };
    let contacts = match nostr_client::fetch_contacts(pubkey_str).await {
        Ok(c) => c,
        Err(_) => return,
    };
    if contacts.is_empty() {
        return;
    }

    let pubkeys: Vec<PublicKey> = contacts
        .iter()
        .filter_map(|p| PublicKey::from_hex(p).ok())
        .collect();
    if pubkeys.is_empty() {
        return;
    }

    let client = match nostr_client::get_client() {
        Some(c) => c,
        None => return,
    };

    if let Ok(relay_maps) = client.database().relay_lists(pubkeys).await {
        let mut count = 0usize;
        {
            let mut coverage = RELAY_COVERAGE.write();
            for (pk, relays_map) in &relay_maps {
                if !relays_map.is_empty() {
                    let urls: Vec<String> = relays_map.keys().map(|u| u.to_string()).collect();
                    coverage.user_relays.insert(pk.to_hex(), urls);
                    count += 1;
                }
            }
        }
        log::info!("Preloaded relay lists for {count} followed users from DB");

        let cached: std::collections::HashSet<String> =
            RELAY_COVERAGE.peek().user_relays.keys().cloned().collect();
        let missing: Vec<String> = contacts
            .into_iter()
            .filter(|p| !cached.contains(p))
            .collect();

        if !missing.is_empty() {
            log::info!(
                "Fetching relay lists from network for {} uncached follows",
                missing.len()
            );
            dioxus::prelude::spawn(async move {
                for chunk in missing.chunks(50) {
                    let chunk_pubkeys: Vec<PublicKey> = chunk
                        .iter()
                        .filter_map(|p| PublicKey::from_hex(p).ok())
                        .collect();
                    if chunk_pubkeys.is_empty() {
                        continue;
                    }
                    let filter =
                        Filter::new().authors(chunk_pubkeys).kind(Kind::RelayList);
                    if let Ok(events) = client.fetch_events(filter, Duration::from_secs(10)).await {
                        for event in events {
                            record_relay_list_from_event(&event);
                        }
                    }
                }
            });
        }
    }
}
