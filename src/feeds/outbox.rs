//! Outbox routing: builds per-relay filter targets for `subscribe_targeted`.
//!
//! Ports wisp's `OutboxRouter` pattern using the SDK's pool-level
//! `subscribe_targeted(id, HashMap<RelayUrl, Vec<Filter>>, opts)`.
//!
//! ## Algorithm
//!
//! 1. For each author, look up their write relays from local DB (kind 10002).
//! 2. Select up to `MIN_REDUNDANCY` (3) write relays per author.
//! 3. Group: `relay_url → set of authors`.
//! 4. Pre-add missing relays to the pool (SDK's `subscribe_targeted` ERRORS
//!    on unknown relay URLs — verified at `pool/mod.rs:941`).
//! 5. Chunk authors per relay to `MAX_AUTHORS_PER_FILTER` (200).
//! 6. Authors with no eligible write relays → `INDEXER_RELAYS` fallback
//!    with the full author list as safety net.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nostr::nips::nip65::{extract_relay_list, RelayMetadata};
use nostr_sdk::{Client, Filter, Kind, PublicKey, RelayUrl, SubscriptionId};

use super::repository::FeedDatabase;
use super::cursor::DEFAULT_LIVE_TAIL_SECS;

/// Maximum write relays to use per author (wisp pattern: 3 relays so one
/// going down doesn't lose the author).
pub const MIN_REDUNDANCY: usize = 3;

/// Maximum authors per relay filter (avoid relay "filter items too large"
/// rejections — wisp pattern).
pub const MAX_AUTHORS_PER_FILTER: usize = 200;

/// Indexer relays that get the full author list as a safety net.
pub const INDEXER_RELAYS: &[&str] = &["wss://relay.nostr.band", "wss://relay.damus.io"];

/// Cache TTL for author relay-list lookups (matches contacts cache: 5 min).
const RELAY_LIST_CACHE_TTL: Duration = Duration::from_secs(300);

/// Maximum time budget for outbox target construction.
const OUTBOX_BUILD_TIMEOUT: Duration = Duration::from_secs(10);

/// Cached relay list for a single author.
#[derive(Clone, Debug)]
struct CachedRelayList {
    write_relays: Vec<RelayUrl>,
    fetched_at: Instant,
}

impl CachedRelayList {
    fn is_stale(&self) -> bool {
        self.fetched_at.elapsed() > RELAY_LIST_CACHE_TTL
    }
}

/// Per-relay filter targets for outbox routing.
#[derive(Debug, Clone)]
pub struct OutboxTargets {
    /// Per-relay filter lists for `subscribe_targeted`.
    /// Each relay gets one or more filters (chunked to MAX_AUTHORS_PER_FILTER).
    pub targets: HashMap<RelayUrl, Vec<Filter>>,
    /// Authors that couldn't be routed to any pool relay.
    pub unrouted_authors: Vec<PublicKey>,
    /// Indexer safety-net filters (full author list to INDEXER_RELAYS).
    pub indexer_filters: Vec<Filter>,
    /// All relay URLs that need to be in the pool (for pre-adding).
    pub required_relay_urls: HashSet<RelayUrl>,
}

impl OutboxTargets {
    /// Check if there are any targets to subscribe to.
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty() && self.indexer_filters.is_empty()
    }

    /// Total number of authors across all targets (with overlap from redundancy).
    pub fn total_author_slots(&self) -> usize {
        self.targets
            .values()
            .flatten()
            .filter_map(|f| f.authors.as_ref().map(|a| a.len()))
            .sum()
    }
}

/// Builds per-relay filter targets for outbox-routed subscriptions.
///
/// One instance per feed scope. Uses a 5-minute TTL cache for author relay
/// lists to avoid re-querying the database on every pagination.
pub struct OutboxRouter {
    client: Arc<Client>,
    db: Arc<dyn FeedDatabase>,
    relay_list_cache: Mutex<HashMap<PublicKey, CachedRelayList>>,
}

impl OutboxRouter {
    /// Construct with the SDK client and a FeedDatabase.
    pub fn new(client: Arc<Client>, db: Arc<dyn FeedDatabase>) -> Self {
        Self {
            client,
            db,
            relay_list_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Construct with the production SdkDatabase from a Client.
    pub fn from_client(client: Arc<Client>) -> Self {
        let db = Arc::new(super::repository::SdkDatabase::from_client(&client));
        Self::new(client, db)
    }

    /// Look up an author's write relays from the local DB (kind 10002).
    /// Uses a 5-minute TTL cache.
    pub async fn get_write_relays(&self, author: &PublicKey) -> Vec<RelayUrl> {
        // Check cache first
        {
            let cache = self.relay_list_cache.lock().unwrap();
            if let Some(cached) = cache.get(author) {
                if !cached.is_stale() {
                    return cached.write_relays.clone();
                }
            }
        }

        // Query local DB for the author's kind 10002 relay list event
        let filter = Filter::new()
            .author(*author)
            .kind(Kind::RelayList)
            .limit(1);
        let write_relays = match self.db.query(filter).await {
            Ok(events) => {
                if let Some(event) = events.into_iter().next() {
                    // Extract write relays from the NIP-65 tags
                    let relays: Vec<RelayUrl> = extract_relay_list(&event)
                        .filter_map(|(url, metadata)| {
                            // Include relays marked as Write, or with no marker
                            // (no marker = both read and write per NIP-65)
                            match metadata {
                                Some(RelayMetadata::Write) | None => Some(url.clone()),
                                Some(RelayMetadata::Read) => None,
                            }
                        })
                        .take(MIN_REDUNDANCY)
                        .collect();
                    relays
                } else {
                    Vec::new()
                }
            }
            Err(e) => {
                log::warn!("Failed to query relay list for {}: {}", author, e);
                Vec::new()
            }
        };

        // Cache the result
        {
            let mut cache = self.relay_list_cache.lock().unwrap();
            cache.insert(
                *author,
                CachedRelayList {
                    write_relays: write_relays.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }

        write_relays
    }

    /// Build per-relay filter targets for a set of authors.
    ///
    /// This is the main entry point for outbox routing. The caller provides
    /// the base filter (kinds, since/until, limit); the router distributes
    /// authors across relays based on their NIP-65 write relay lists.
    pub async fn build_targets(
        &self,
        authors: &[PublicKey],
        base_filter: Filter,
    ) -> OutboxTargets {
        if authors.is_empty() {
            return OutboxTargets {
                targets: HashMap::new(),
                unrouted_authors: Vec::new(),
                indexer_filters: Vec::new(),
                required_relay_urls: HashSet::new(),
            };
        }

        // Step 1-2: For each author, get up to MIN_REDUNDANCY write relays
        let mut relay_to_authors: HashMap<RelayUrl, HashSet<PublicKey>> = HashMap::new();
        let mut unrouted: Vec<PublicKey> = Vec::new();

        for author in authors {
            let write_relays = self.get_write_relays(author).await;
            if write_relays.is_empty() {
                unrouted.push(*author);
            } else {
                for relay_url in write_relays {
                    relay_to_authors
                        .entry(relay_url)
                        .or_default()
                        .insert(*author);
                }
            }
        }

        // Step 3: Get connected relay URLs from the pool
        let _connected_relays: HashSet<RelayUrl> = {
            let pool_relays = self.client.relays().await;
            pool_relays
                .into_iter()
                .filter(|(_, relay)| relay.is_connected())
                .map(|(url, _)| url)
                .collect()
        };

        // Step 4: Build per-relay filter lists, chunked to MAX_AUTHORS_PER_FILTER
        let mut targets: HashMap<RelayUrl, Vec<Filter>> = HashMap::new();
        let mut required_relay_urls: HashSet<RelayUrl> = HashSet::new();

        for (relay_url, author_set) in &relay_to_authors {
            // Include this relay even if not yet connected (we'll pre-add it)
            required_relay_urls.insert(relay_url.clone());

            // If relay is not connected AND not in the pool, we still include
            // it — the caller is responsible for pre-adding. But if it IS
            // connected, we definitely want it.
            // (Wisp filters to pool URLs only, but we pre-add first so the
            // pool check is more lenient.)

            let sorted_authors: Vec<PublicKey> = {
                let mut auths: Vec<PublicKey> = author_set.iter().copied().collect();
                auths.sort();
                auths
            };

            // Chunk authors to MAX_AUTHORS_PER_FILTER
            for chunk in sorted_authors.chunks(MAX_AUTHORS_PER_FILTER) {
                let filter = base_filter.clone().authors(chunk.to_vec());
                targets
                    .entry(relay_url.clone())
                    .or_default()
                    .push(filter);
            }
        }

        // Step 5: Build indexer safety-net filters for unrouted authors
        // AND as a general safety net (wisp sends full list to indexers)
        let mut indexer_filters = Vec::new();
        if !authors.is_empty() {
            // Send the full author list to indexer relays as a safety net
            for chunk in authors.chunks(MAX_AUTHORS_PER_FILTER) {
                let filter = base_filter.clone().authors(chunk.to_vec());
                indexer_filters.push(filter);
            }
            // Add indexer relays to required set
            for url_str in INDEXER_RELAYS {
                if let Ok(url) = RelayUrl::parse(url_str) {
                    required_relay_urls.insert(url);
                }
            }
        }

        OutboxTargets {
            targets,
            unrouted_authors: unrouted,
            indexer_filters,
            required_relay_urls,
        }
    }

    /// Ensure all required relay URLs are in the pool.
    ///
    /// Must be called BEFORE `subscribe_targeted` — the SDK errors with
    /// `RelayNotFound` if any URL is missing from the pool (verified at
    /// `pool/mod.rs:941`).
    pub async fn ensure_relays_in_pool(&self, urls: &HashSet<RelayUrl>) {
        let existing: HashSet<RelayUrl> = self
            .client
            .relays()
            .await
            .into_keys()
            .collect();

        for url in urls {
            if !existing.contains(url) {
                match self.client.add_relay(url.clone()).await {
                    Ok(_) => {
                        log::debug!("Added relay {} to pool for outbox routing", url);
                    }
                    Err(e) => {
                        log::warn!("Failed to add relay {} to pool: {}", url, e);
                    }
                }
            }
        }
    }

    /// Issue outbox-routed subscriptions via `pool().subscribe_targeted`.
    ///
    /// Pre-adds all required relays, then subscribes with the given options.
    /// Returns the subscription ID used and the set of relay URLs that were
    /// actually targeted.
    pub async fn subscribe(
        &self,
        id: SubscriptionId,
        targets: &OutboxTargets,
    ) -> Result<HashSet<RelayUrl>, super::repository::FeedError> {
        // Pre-add missing relays
        self.ensure_relays_in_pool(&targets.required_relay_urls).await;

        // Build the (RelayUrl, Vec<Filter>) iterator for subscribe_targeted
        let targeted: Vec<(RelayUrl, Vec<Filter>)> = targets
            .targets
            .iter()
            .map(|(url, filters)| (url.clone(), filters.clone()))
            .collect();

        if targeted.is_empty() {
            return Ok(HashSet::new());
        }

        // Use pool-level subscribe_targeted which accepts Vec<Filter> per URL.
        // The SDK-level Client::subscribe_targeted only accepts single Filter per URL.
        let pool = self.client.pool();
        let opts = nostr_relay_pool::relay::options::SubscribeOptions::default();
        pool.subscribe_targeted(id, targeted, opts)
            .await
            .map_err(|e| super::repository::FeedError::Client(e.to_string()))?;

        Ok(targets.targets.keys().cloned().collect())
    }

    /// Invalidate the relay-list cache for a specific author (e.g. when
    /// their kind 10002 event changes).
    pub fn invalidate_cache(&self, author: &PublicKey) {
        let mut cache = self.relay_list_cache.lock().unwrap();
        cache.remove(author);
    }

    /// Clear the entire relay-list cache.
    pub fn clear_cache(&self) {
        let mut cache = self.relay_list_cache.lock().unwrap();
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feeds::repository::InMemoryDatabase;
    use nostr_sdk::{EventBuilder, Keys, Kind, Timestamp};

    fn make_relay_list_event(author: &Keys, relays: &[(&str, Option<RelayMetadata>)]) -> nostr_sdk::Event {
        let tags: Vec<nostr_sdk::Tag> = relays
            .iter()
            .map(|(url, metadata)| {
                nostr_sdk::Tag::from_standardized_without_cell(
                    nostr_sdk::TagStandard::RelayMetadata {
                        relay_url: RelayUrl::parse(url).unwrap(),
                        metadata: *metadata,
                    },
                )
            })
            .collect();
        EventBuilder::new(Kind::RelayList, "")
            .tags(tags)
            .custom_created_at(Timestamp::now())
            .sign_with_keys(author)
            .unwrap()
    }

    #[tokio::test]
    async fn get_write_relays_extracts_from_kind_10002() {
        let db = Arc::new(InMemoryDatabase::new());
        let keys = Keys::generate();
        let event = make_relay_list_event(
            &keys,
            &[
                ("wss://relay1.example.com", Some(RelayMetadata::Write)),
                ("wss://relay2.example.com", Some(RelayMetadata::Read)),
                ("wss://relay3.example.com", None), // both read+write
            ],
        );
        db.insert(event);

        // We can't easily construct a Client in tests, but we can test the
        // DB query + relay-list extraction logic directly.
        let filter = Filter::new()
            .author(keys.public_key())
            .kind(Kind::RelayList)
            .limit(1);
        let results = db.query(filter).await.unwrap();
        assert_eq!(results.len(), 1);

        let event = &results[0];
        let write_relays: Vec<RelayUrl> = extract_relay_list(event)
            .filter_map(|(url, metadata)| match metadata {
                Some(RelayMetadata::Write) | None => Some(url.clone()),
                Some(RelayMetadata::Read) => None,
            })
            .collect();
        assert_eq!(write_relays.len(), 2); // relay1 (Write) + relay3 (None=both)
        assert!(write_relays.contains(&RelayUrl::parse("wss://relay1.example.com").unwrap()));
        assert!(write_relays.contains(&RelayUrl::parse("wss://relay3.example.com").unwrap()));
    }

    #[test]
    fn outbox_targets_is_empty_when_no_authors() {
        let targets = OutboxTargets {
            targets: HashMap::new(),
            unrouted_authors: Vec::new(),
            indexer_filters: Vec::new(),
            required_relay_urls: HashSet::new(),
        };
        assert!(targets.is_empty());
    }

    #[test]
    fn outbox_targets_not_empty_with_targets() {
        let mut targets_map = HashMap::new();
        targets_map.insert(
            RelayUrl::parse("wss://relay.example.com").unwrap(),
            vec![Filter::new().kind(Kind::TextNote)],
        );
        let targets = OutboxTargets {
            targets: targets_map,
            unrouted_authors: Vec::new(),
            indexer_filters: Vec::new(),
            required_relay_urls: HashSet::new(),
        };
        assert!(!targets.is_empty());
    }

    #[test]
    fn min_redundancy_is_three() {
        assert_eq!(MIN_REDUNDANCY, 3);
    }

    #[test]
    fn max_authors_per_filter_is_200() {
        assert_eq!(MAX_AUTHORS_PER_FILTER, 200);
    }
}
