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
use std::sync::{Arc, LazyLock, Mutex as StdMutex};
use std::time::Duration;

/// Tracks relay URLs that are being connected or have been connected by
/// `connect_ephemeral_relays`. Prevents concurrent callers (e.g. the profile
/// metadata fetch and the profile tab-events stream both connecting to the
/// same author's write relays) from each tracking the URL as "newly added"
/// and then racing to clean it up while the other is still using it.
static EPHEMERAL_IN_USE: LazyLock<StdMutex<std::collections::HashSet<String>>> =
    LazyLock::new(|| StdMutex::new(std::collections::HashSet::new()));

#[derive(Clone, Debug, Default)]
pub struct RelayCoverageMap {
    user_relays: std::collections::HashMap<String, Vec<String>>,
    provenance: std::collections::HashMap<String, Vec<String>>,
    hints: std::collections::HashMap<String, Vec<String>>,
}

pub static RELAY_COVERAGE: GlobalSignal<RelayCoverageMap> =
    Signal::global(RelayCoverageMap::default);

impl RelayCoverageMap {
    /// Rank relay URLs by how many of `authors` they cover, descending.
    ///
    /// Coverage sources: NIP-65 lists (`user_relays`), event provenance, and
    /// p-tag relay hints. Each relay counts at most once per author, so a
    /// relay appearing in several sources for the same author doesn't inflate
    /// its rank. Ties break by URL ascending so the output is deterministic.
    ///
    /// Used to pick a bounded set of negentropy sync targets for the
    /// following feed — see `feed_loaders::sync_following_feed_page`.
    pub fn top_relays_by_author_coverage(
        &self,
        authors: &[PublicKey],
        limit: usize,
    ) -> Vec<String> {
        if limit == 0 || authors.is_empty() {
            return Vec::new();
        }
        let wanted: std::collections::HashSet<String> =
            authors.iter().map(|p| p.to_hex()).collect();
        let mut counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for (pubkey, relays) in self
            .user_relays
            .iter()
            .chain(self.provenance.iter())
            .chain(self.hints.iter())
        {
            if !wanted.contains(pubkey.as_str()) {
                continue;
            }
            // Count each relay at most once per author.
            let mut per_author: std::collections::HashSet<&str> =
                std::collections::HashSet::new();
            for url in relays {
                if per_author.insert(url.as_str()) {
                    *counts.entry(url.as_str()).or_default() += 1;
                }
            }
        }
        let mut ranked: Vec<(&str, usize)> = counts.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        ranked
            .into_iter()
            .take(limit)
            .map(|(url, _)| url.to_string())
            .collect()
    }
}

/// Which relay purpose to filter for when resolving a user's relays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayPurpose {
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
            let filter = Filter::new().kind(Kind::RelayList).author(pk);
            if let Ok(events) = client.database().query(filter).await {
                // Kind 10002 is replaceable: keep the newest event if several exist
                let newest = events
                    .into_iter()
                    .max_by_key(|event| event.created_at.as_secs());
                if let Some(event) = newest {
                    let relays_map: std::collections::HashMap<
                        RelayUrl,
                        Option<RelayMetadata>,
                    > = nostr::nips::nip65::extract_relay_list(&event).collect();
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
///
/// Uses `fetch_events_from_indexers` (the only sanctioned way to read from
/// DISCOVERY-only indexers). `client.fetch_events` targets only strict-READ
/// relays and would silently miss indexers where kind 10002 lists are
/// aggregated — see AGENTS.md "Relay Routing & Discovery".
async fn fetch_10002_from_network(pubkey: &str) -> Option<Vec<String>> {
    let client = nostr_client::get_client()?;
    let pk = PublicKey::from_hex(pubkey).ok()?;
    let filter = Filter::new().author(pk).kind(Kind::RelayList).limit(1);
    let result = super::nip65::fetch_events_from_indexers(&client, filter, Duration::from_secs(5)).await;
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
///
/// Same indexer-routed query as [`fetch_10002_from_network`].
async fn refresh_10002_from_network(pubkey: &str) -> Option<Vec<String>> {
    let client = nostr_client::get_client()?;
    let pk = PublicKey::from_hex(pubkey).ok()?;
    let filter = Filter::new().author(pk).kind(Kind::RelayList).limit(1);
    match super::nip65::fetch_events_from_indexers(&client, filter, Duration::from_secs(5)).await {
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

/// Rank relay URLs by followed-author coverage from the live
/// [`RELAY_COVERAGE`] signal. See
/// [`RelayCoverageMap::top_relays_by_author_coverage`].
pub fn top_relays_by_author_coverage(authors: &[PublicKey], limit: usize) -> Vec<String> {
    RELAY_COVERAGE
        .peek()
        .top_relays_by_author_coverage(authors, limit)
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

/// All relay URLs known to the coverage map (user NIP-65 lists, event
/// provenance, and relay hints), deduplicated. Used for autocomplete
/// suggestion sourcing.
pub fn known_coverage_urls() -> Vec<String> {
    let coverage = RELAY_COVERAGE.peek();
    let mut seen = std::collections::HashSet::new();
    let mut urls = Vec::new();
    for list in coverage
        .user_relays
        .values()
        .chain(coverage.provenance.values())
        .chain(coverage.hints.values())
    {
        for url in list {
            if seen.insert(url.clone()) {
                urls.push(url.clone());
            }
        }
    }
    urls
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
    let url = crate::utils::relay::upgrade_to_secure_relay_url(url);
    let mut coverage = RELAY_COVERAGE.write();
    let entry = coverage.hints.entry(pubkey.to_string()).or_default();
    if !entry.contains(&url) {
        entry.push(url);
        if entry.len() > 5 {
            entry.remove(0);
        }
    }
}

/// Record a kind 10002 event into the coverage map (eager population).
pub fn record_relay_list_from_event(event: &nostr::event::Event) {
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
fn extract_and_record_hints(event: &nostr::event::Event) {
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
///
/// **Concurrent dedup**: if another caller has already added/connecting a URL
/// (tracked in `EPHEMERAL_IN_USE`), it is treated as "already connected" and
/// excluded from `newly_added`. This prevents two concurrent callers (e.g. the
/// profile metadata fetch and the profile tab-events stream) from each tracking
/// the same relay for cleanup and then racing to remove it while the other is
/// still using it.
pub async fn connect_ephemeral_relays(client: &Client, urls: &[String]) -> EphemeralResult {
    use futures::stream::{self, StreamExt};

    // Default `relays()` snapshot (READ|WRITE members) — same filter as 0.44.
    // GOSSIP-only ephemeral members are invisible here by design; the
    // EPHEMERAL_IN_USE set below is what dedups concurrent callers.
    let relays = client.relays().await;
    let mut already_connected: Vec<String> = Vec::new();
    let mut to_add: Vec<nostr::types::url::Url> = Vec::new();

    // Atomic check-and-claim: check pool connected + EPHEMERAL_IN_USE and claim
    // in one lock scope so concurrent callers on different threads (desktop)
    // can't both see in_use=false and both claim the same URL (TOCTOU).
    {
        let mut in_use = EPHEMERAL_IN_USE.lock().unwrap();
        for raw_url in urls {
            let url = crate::utils::relay::upgrade_to_secure_relay_url(raw_url);
            let Ok(parsed) = nostr::types::url::Url::parse(&url) else {
                continue;
            };
            let is_connected = relays
                .iter()
                .any(|(u, r)| u.as_str() == url && r.status().is_connected());
            // insert() returns false if already present = atomic check-and-claim.
            // Normalize so insert/remove/lookup all use the same string form —
            // Url::parse adds a trailing "/" to bare host URLs (e.g. "wss://nos.lol"
            // → "wss://nos.lol/"), matching the normalized strings produced by
            // url.to_string() in newly_connected and pool RelayUrl::as_str().
            let normalized = crate::utils::relay::normalize_known_relay_url(&url);
            if is_connected || !in_use.insert(normalized) {
                already_connected.push(url);
            } else {
                to_add.push(parsed);
            }
        }
    } // lock released — before any .await

    if to_add.is_empty() {
        return EphemeralResult {
            connected: already_connected,
            newly_added: Vec::new(),
        };
    }

    // GOSSIP-only capability: the default capability set (READ|WRITE) makes
    // these relays visible in `client.relays()` snapshots — and
    // `stream_events_immediate` / `fetch_events_from_connected_relays`
    // snapshot then target that set, so a `remove_relay(url).force()` from
    // `cleanup_ephemeral_relays` landing between snapshot and use makes the
    // SDK's targeted stream fail the WHOLE call with a "relay not found"
    // error (one removed relay poisons all targets).
    // GOSSIP-only relays are excluded from those snapshots
    // (default `relays()` filters to READ|WRITE) while remaining reachable
    // by targeted URL calls (`can_read()` includes GOSSIP) — they also stop
    // inheriting pool broadcast subscriptions (a READ behavior).
    // `pool.connect()`/`try_connect_relay` connect every member regardless
    // of capabilities, and our cleanup uses `remove_relay(url).force()`
    // (which bypasses the SDK's re-insert-GOSSIP guard on non-force
    // removal), so lifecycle is unchanged.
    //
    // Idle eviction: 0.45 replaced `RelayOptions::idle_timeout` with
    // `sleep_when_idle(SleepWhenIdle::Enabled { timeout })`; relays that
    // never connect are reclaimed by the failed-connect path below, and
    // dormant connected ones by `cleanup_gossip_relays`.
    let opts = RelayOptions::new()
        .reconnect(false)
        .sleep_when_idle(SleepWhenIdle::Enabled {
            timeout: Duration::from_secs(300),
        });
    for url in &to_add {
        let _ = client
            .add_relay(url.as_str())
            .capabilities(RelayCapabilities::GOSSIP)
            .opts(opts.clone())
            .await;
    }

    const MAX_CONCURRENT: usize = 5;
    let newly_connected: Vec<String> = stream::iter(to_add.clone())
        .map(|url| {
            let client = client.clone();
            async move {
                if client
                    .try_connect_relay(url.as_str(), Duration::from_secs(3))
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

    // Reclaim failed connections: URLs we claimed in EPHEMERAL_IN_USE and
    // added to the pool but whose try_connect_relay failed. Without this,
    // the URL stays claimed forever (never reaches cleanup_ephemeral_relays)
    // and lingers in the pool as a dormant entry, silently breaking all
    // future connect attempts to that URL.
    let newly_set: std::collections::HashSet<&String> = newly_connected.iter().collect();
    for url in &to_add {
        let url_str = url.to_string();
        if !newly_set.contains(&url_str) {
            EPHEMERAL_IN_USE.lock().unwrap().remove(&url_str);
            let _ = client.remove_relay(url.as_str()).force().await;
        }
    }

    let mut connected = already_connected;
    connected.extend(newly_connected.clone());
    EphemeralResult {
        connected,
        newly_added: newly_connected,
    }
}

/// Remove ephemeral relays from the pool after a targeted fetch.
/// Also clears the `EPHEMERAL_IN_USE` claim so future calls can re-add.
pub async fn cleanup_ephemeral_relays(client: &Client, urls: &[String]) {
    for url in urls {
        let _ = client.remove_relay(url.as_str()).force().await;
        let normalized = crate::utils::relay::normalize_known_relay_url(url);
        EPHEMERAL_IN_USE.lock().unwrap().remove(&normalized);
    }
}

/// Check whether a relay URL was added by `connect_ephemeral_relays` and is
/// still tracked (not yet cleaned up). Used by the profile-viewer reset effect
/// to identify which relays to remove on profile change.
pub fn is_ephemeral_relay(url: &str) -> bool {
    let normalized = crate::utils::relay::normalize_known_relay_url(url);
    EPHEMERAL_IN_USE.lock().unwrap().contains(&normalized)
}

/// Remove gossip-only relays from the pool.
///
/// Preserves user-configured relays (which have READ/WRITE capabilities) and
/// discovery relays (which have the DISCOVERY capability). Only removes
/// relays that have the GOSSIP capability with no READ, WRITE, or DISCOVERY
/// capabilities.
///
/// Returns the number of relays removed.
pub async fn cleanup_gossip_relays(client: &Client) -> usize {
    let all = client.relays().all().await;
    let mut removed = 0;
    for (url, relay) in all {
        let caps = relay.capabilities();
        if caps.has_gossip()
            && !caps.has_read()
            && !caps.has_write()
            && !caps.has_discovery()
        {
            let _ = client.remove_relay(url.as_str()).force().await;
            removed += 1;
        }
    }
    if removed > 0 {
        log::info!("Cleaned up {} dormant gossip-only relays", removed);
    }
    removed
}

/// Start a provenance recorder that listens to all relay notifications.
///
/// Records:
/// - Event provenance (which relay delivered events for each author)
/// - p-tag relay hints
/// - Eager kind 10002 recording
///
/// Must be called from within a Dioxus component (uses `spawn_forever`).
pub fn start_provenance_recorder(_client: Arc<Client>) {
    crate::platform::spawn::spawn_forever_catch_unwind("provenance", async move {
        // Outer re-read loop: the client is rebuilt on login/logout. When the
        // old client shuts down its notification stream ends; re-read
        // `get_client()` and continue with the new client instead of exiting.
        loop {
            let Some(client) = nostr_client::get_client() else {
                nostr_client::platform_sleep_ms(1000).await;
                continue;
            };
            let mut notifications = client.notifications();
            loop {
                match notifications.next().await {
                    Some(ClientNotification::Event {
                        relay_url,
                        event,
                        ..
                    }) => {
                        let pubkey_hex = event.pubkey.to_hex();
                        let relay_str = relay_url.to_string();

                        record_provenance(&pubkey_hex, &relay_str);
                        extract_and_record_hints(&event);

                        if event.kind == Kind::RelayList {
                            record_relay_list_from_event(&event);
                        }
                    }
                    Some(ClientNotification::Shutdown) => break,
                    // Stream ended (client shut down / dropped).
                    None => {
                        log::info!("coverage provenance: notification stream ended, re-reading client");
                        break;
                    }
                    Some(_) => {}
                }
            }
            // Brief backoff so a shut-down-but-still-present client
            // (empty stream) can't hot-spin this loop.
            nostr_client::platform_sleep_ms(1000).await;
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

    let filter = Filter::new().kind(Kind::RelayList).authors(pubkeys);
    if let Ok(events) = client.database().query(filter).await {
        // Kind 10002 is replaceable: keep only the newest list per author
        let mut newest: std::collections::HashMap<PublicKey, nostr::event::Event> =
            std::collections::HashMap::new();
        for event in events {
            match newest.entry(event.pubkey) {
                std::collections::hash_map::Entry::Occupied(mut occupied) => {
                    if occupied.get().created_at < event.created_at {
                        occupied.insert(event);
                    }
                }
                std::collections::hash_map::Entry::Vacant(vacant) => {
                    vacant.insert(event);
                }
            }
        }
        let mut count = 0usize;
        {
            let mut coverage = RELAY_COVERAGE.write();
            for (pk, event) in &newest {
                let relays_map: std::collections::HashMap<RelayUrl, Option<RelayMetadata>> =
                    nostr::nips::nip65::extract_relay_list(event).collect();
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
                    if let Ok(events) = client
                        .fetch_events(filter)
                        .timeout(Duration::from_secs(10))
                        .await
                    {
                        for event in events {
                            record_relay_list_from_event(&event);
                        }
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(hex: &str) -> PublicKey {
        PublicKey::parse(hex).unwrap()
    }

    fn coverage_fixture() -> RelayCoverageMap {
        let author_a = "11".repeat(32);
        let author_b = "22".repeat(32);
        let author_c = "33".repeat(32);
        let mut map = RelayCoverageMap::default();
        map.user_relays.insert(
            author_a.clone(),
            vec!["wss://r1".to_string(), "wss://r2".to_string()],
        );
        map.user_relays.insert(
            author_b.clone(),
            vec!["wss://r1".to_string(), "wss://r3".to_string()],
        );
        // author_c is NOT part of the sync set; its relays must never rank.
        map.user_relays
            .insert(author_c, vec!["wss://r9".to_string()]);
        // Provenance for author_a repeats r1 (dedup per author) and adds r4.
        map.provenance.insert(
            author_a,
            vec!["wss://r1".to_string(), "wss://r4".to_string()],
        );
        // Hint puts r2 on author_b's coverage too.
        map.hints
            .insert(author_b, vec!["wss://r2".to_string()]);
        map
    }

    #[test]
    fn ranks_by_author_coverage_with_deterministic_ties() {
        let map = coverage_fixture();
        let authors = vec![pk(&"11".repeat(32)), pk(&"22".repeat(32))];

        // r1 (A+B) and r2 (A user_relays + B hint) both cover 2 authors;
        // tie breaks by URL ascending. r3/r4 cover 1 each, r9 is excluded.
        assert_eq!(
            map.top_relays_by_author_coverage(&authors, 4),
            vec![
                "wss://r1".to_string(),
                "wss://r2".to_string(),
                "wss://r3".to_string(),
                "wss://r4".to_string(),
            ]
        );
        // Budget respected…
        assert_eq!(
            map.top_relays_by_author_coverage(&authors, 2),
            vec!["wss://r1".to_string(), "wss://r2".to_string()]
        );
        // …and edge cases.
        assert!(map.top_relays_by_author_coverage(&authors, 0).is_empty());
        assert!(map.top_relays_by_author_coverage(&[], 5).is_empty());
    }

    #[test]
    fn counts_each_relay_once_per_author() {
        let map = coverage_fixture();
        let authors = vec![pk(&"11".repeat(32))];
        // Only author A: r1 appears in both user_relays and provenance but
        // must count once; ranking among single-author relays is by URL.
        assert_eq!(
            map.top_relays_by_author_coverage(&authors, 5),
            vec![
                "wss://r1".to_string(),
                "wss://r2".to_string(),
                "wss://r4".to_string(),
            ]
        );
    }
}
