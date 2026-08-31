//! Kind-10133 payment-target cache (NIP-A3).
//!
//! Mirrors the two-signal pattern of `WORKOUT_TEMPLATE_CACHE`: the LruCache
//! mutation does not notify subscribers, so a companion version signal is
//! bumped on every insert for consumers to re-evaluate against. Entries
//! carry a fetched-at timestamp (10-minute freshness) so revisits within a
//! session don't re-query the author's relays.
use crate::utils::nips::nipa3::{self, PayToTarget};
use dioxus::prelude::*;
use lru::LruCache;
use nostr_sdk::prelude::*;
use std::num::NonZeroUsize;
use std::time::Duration;

/// Freshness window for cached targets.
const TTL_SECS: u64 = 600;

pub struct CacheEntry {
    targets: Vec<PayToTarget>,
    fetched_at: u64,
}

pub static PAYTO_TARGETS_CACHE: GlobalSignal<LruCache<String, CacheEntry>> =
    Signal::global(|| LruCache::new(NonZeroUsize::new(500).unwrap()));

/// Bumped on every cache insert (LruCache mutation alone is not reactive).
pub static PAYTO_TARGETS_VERSION: GlobalSignal<u64> = Signal::global(|| 0);

/// In-flight fetch dedup so concurrent mounts don't double-fetch.
static IN_FLIGHT: std::sync::LazyLock<tokio::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(|| {
        tokio::sync::Mutex::new(std::collections::HashSet::new())
    });

/// Cached targets for a hex pubkey, if fresh enough to display without a
/// refetch.
pub fn cached_targets(pubkey_hex: &str) -> Option<Vec<PayToTarget>> {
    let cache = PAYTO_TARGETS_CACHE.read();
    cache
        .peek(pubkey_hex)
        .filter(|entry| {
            crate::platform::timestamp::now_secs().saturating_sub(entry.fetched_at) < TTL_SECS
        })
        .map(|entry| entry.targets.clone())
}

/// Read the last-known targets regardless of freshness (for instant paint
/// while a refresh is in flight).
pub fn peek_targets(pubkey_hex: &str) -> Option<Vec<PayToTarget>> {
    let cache = PAYTO_TARGETS_CACHE.read();
    cache.peek(pubkey_hex).map(|entry| entry.targets.clone())
}

/// Fetch a user's kind-10133 payment targets (gossip-routed to the author's
/// write relays) and cache them. A user with no declared targets is cached
/// as an empty set; network errors leave the entry uncached so a later
/// mount can retry.
pub async fn fetch_targets(pubkey_hex: String) {
    if cached_targets(&pubkey_hex).is_some() {
        return;
    }
    {
        let mut in_flight = IN_FLIGHT.lock().await;
        if !in_flight.insert(pubkey_hex.clone()) {
            return;
        }
    }
    let result = match PublicKey::parse(&pubkey_hex) {
        Ok(pubkey) => {
            let filter = Filter::new()
                .kind(Kind::Custom(nipa3::KIND_PAYMENT_TARGETS))
                .author(pubkey)
                .limit(1);
            crate::stores::nostr_client::fetch_events_aggregated_outbox(
                filter,
                Duration::from_secs(10),
            )
            .await
        }
        Err(_) => Ok(Vec::new()),
    };
    match result {
        Ok(events) => {
            let targets = events
                .iter()
                .max_by_key(|e| e.created_at)
                .map(nipa3::parse_payto_targets)
                .unwrap_or_default();
            insert(pubkey_hex.clone(), targets).await;
        }
        Err(e) => {
            log::debug!("Payment targets fetch failed (will retry later): {}", e);
        }
    }
    IN_FLIGHT.lock().await.remove(&pubkey_hex);
}

/// Optimistically replace the cached targets for a pubkey (used after the
/// user saves their own targets, and to seed the editor).
pub async fn store_targets(pubkey_hex: String, targets: Vec<PayToTarget>) {
    insert(pubkey_hex, targets).await;
}

async fn insert(pubkey_hex: String, targets: Vec<PayToTarget>) {
    {
        let mut cache = PAYTO_TARGETS_CACHE.write();
        cache.put(
            pubkey_hex,
            CacheEntry {
                targets,
                fetched_at: crate::platform::timestamp::now_secs(),
            },
        );
    }
    PAYTO_TARGETS_VERSION.with_mut(|v| *v = v.wrapping_add(1));
}
