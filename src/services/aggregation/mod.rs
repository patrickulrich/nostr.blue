//! Event interaction aggregation service
//!
//! Provides batch fetching of interaction counts (replies, likes, reposts, zaps)
//! for multiple events in a single query. This dramatically reduces database
//! queries compared to fetching counts per-event.
//!
//! # Performance Impact
//! - Before: N queries (one per event in feed)
//! - After: 1 query (batched for all events)
//! - Example: 100 notes → 99% reduction in queries (100 → 1)
//!
//! # L2 Caching (Phase 3.5)
//! Implements in-memory LRU cache for computed interaction counts:
//! - Cache size: 1000 events
//! - TTL: 5 minutes per entry
//! - Automatic eviction of stale/excess entries
//! - Reduces redundant database queries for recently-viewed events
//!
//! # Submodules
//! - `counting`: Count computation, batch fetching, NIP-45 support, sync
//! - `streaming`: Real-time interaction streaming via subscriptions
#![allow(unused_imports)]

mod counting;
mod streaming;

pub use counting::*;
pub use streaming::*;

use instant::{Duration, Instant};
use lru::LruCache;
use nostr_sdk::{EventId, Kind};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};

/// Aggregated interaction counts for a single event
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InteractionCounts {
    pub replies: usize,
    pub likes: usize,
    pub reposts: usize,
    pub zaps: usize,
    pub zap_amount_sats: u64,
    /// Whether the current user has liked this event (None if not checked)
    pub user_liked: Option<bool>,
    /// The current user's reaction emoji if they reacted (None if not checked or no reaction)
    pub user_reaction: Option<String>,
    /// The URL for custom emoji reactions (NIP-30) - only set if user_reaction is a custom emoji
    pub user_reaction_url: Option<String>,
    /// Whether the current user has reposted this event (None if not checked)
    pub user_reposted: Option<bool>,
    /// The event ID of the current user's repost (for undo)
    pub user_repost_id: Option<String>,
    /// Whether the current user has zapped this event (None if not checked)
    pub user_zapped: Option<bool>,
}

/// Cache entry with TTL tracking
#[derive(Clone, Debug)]
pub(crate) struct CachedCounts {
    pub(crate) counts: InteractionCounts,
    pub(crate) cached_at: Instant,
}

impl CachedCounts {
    pub(crate) fn new(counts: InteractionCounts) -> Self {
        Self {
            counts,
            cached_at: Instant::now(),
        }
    }

    /// Check if cache entry is still valid (within TTL)
    pub(crate) fn is_valid(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() < ttl
    }
}

/// L2 cache for interaction counts (Phase 3.5)
///
/// In-memory LRU cache that sits between database and UI:
/// - Reduces redundant queries for recently-viewed events
/// - Automatic TTL-based freshness control
/// - LRU eviction prevents unbounded growth
pub(crate) struct CountsCache {
    pub(crate) cache: LruCache<String, CachedCounts>,
    pub(crate) ttl: Duration,
}

impl CountsCache {
    fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(capacity.max(1)).unwrap()),
            ttl,
        }
    }

    /// Get cached counts if they exist and are still valid
    pub(crate) fn get(&mut self, event_id: &str) -> Option<InteractionCounts> {
        if let Some(cached) = self.cache.get(event_id) {
            if cached.is_valid(self.ttl) {
                return Some(cached.counts.clone());
            }
        }
        None
    }

    /// Cache counts for an event
    pub(crate) fn insert(&mut self, event_id: String, counts: InteractionCounts) {
        self.cache.put(event_id, CachedCounts::new(counts));
    }

    /// Get multiple counts from cache, returning only valid entries
    pub(crate) fn get_batch(
        &mut self,
        event_ids: &[EventId],
    ) -> HashMap<String, InteractionCounts> {
        let mut result = HashMap::new();
        for event_id in event_ids {
            let event_id_hex = event_id.to_hex();
            if let Some(counts) = self.get(&event_id_hex) {
                result.insert(event_id_hex, counts);
            }
        }
        result
    }

    /// Cache multiple counts at once
    pub(crate) fn insert_batch(&mut self, counts_map: HashMap<String, InteractionCounts>) {
        for (event_id, counts) in counts_map {
            self.insert(event_id, counts);
        }
    }

    /// Invalidate (remove) cached counts for an event
    ///
    /// Useful when user publishes a new interaction (like, repost, etc.)
    #[allow(dead_code)]
    pub(crate) fn invalidate(&mut self, event_id: &str) {
        self.cache.pop(event_id);
    }

    /// Increment a specific count type for an event (for negentropy sync)
    ///
    /// Updates an existing cache entry with new interaction data.
    /// If the event isn't cached, this is a no-op.
    #[allow(dead_code)]
    pub(crate) fn increment(
        &mut self,
        event_id: &str,
        kind: Kind,
        content: Option<&str>,
        is_current_user: bool,
        zap_amount: Option<u64>,
    ) {
        if let Some(cached) = self.cache.get_mut(event_id) {
            cached.cached_at = Instant::now();
            match kind {
                Kind::TextNote => cached.counts.replies += 1,
                Kind::Reaction => {
                    let content = content.unwrap_or("+");
                    if content != "-" {
                        cached.counts.likes += 1;
                    }
                    if is_current_user {
                        if content == "-" {
                            cached.counts.user_liked = Some(false);
                            cached.counts.user_reaction = None;
                            cached.counts.user_reaction_url = None;
                        } else {
                            cached.counts.user_liked = Some(true);
                            cached.counts.user_reaction = Some(content.to_string());
                        }
                    }
                }
                Kind::Repost => cached.counts.reposts += 1,
                Kind::ZapReceipt => {
                    cached.counts.zaps += 1;
                    if let Some(amount) = zap_amount {
                        cached.counts.zap_amount_sats += amount;
                    }
                }
                _ => {}
            }
        }
    }

    /// Get mutable counts for incremental update during sync
    #[allow(dead_code)]
    pub(crate) fn get_or_create_mut(&mut self, event_id: &str) -> &mut InteractionCounts {
        let needs_create = self
            .cache
            .get(event_id)
            .map(|c| !c.is_valid(self.ttl))
            .unwrap_or(true);
        if needs_create {
            self.cache
                .put(
                    event_id.to_string(),
                    CachedCounts::new(InteractionCounts::default()),
                );
        }
        &mut self.cache.get_mut(event_id).unwrap().counts
    }
}

/// Global L2 cache for interaction counts
///
/// Cache configuration:
/// - Capacity: 1000 events (enough for ~10 full feeds)
/// - TTL: 5 minutes (balance freshness vs performance)
static COUNTS_CACHE: OnceLock<Mutex<CountsCache>> = OnceLock::new();

/// Get or initialize the counts cache
pub(crate) fn get_counts_cache() -> &'static Mutex<CountsCache> {
    COUNTS_CACHE
        .get_or_init(|| { Mutex::new(CountsCache::new(1000, Duration::from_secs(300))) })
}

/// Invalidate cached counts for an event
///
/// Call this when the user publishes a new interaction (like, repost, reply)
/// to ensure the next fetch gets fresh counts from the database.
///
/// # Example
/// ```
/// // After user likes a note
/// publish_reaction(event_id, content).await?;
/// invalidate_interaction_counts(&event_id);
/// ```
#[allow(dead_code)]
pub fn invalidate_interaction_counts(event_id: &str) {
    {
        let mut cache = get_counts_cache()
            .lock()
            .unwrap_or_else(|poisoned| {
                log::warn!("Counts cache mutex was poisoned, recovering");
                poisoned.into_inner()
            });
        cache.invalidate(event_id);
    }
    log::debug!("Invalidated interaction counts cache for {}", event_id);
}

/// Invalidate cached counts for multiple events at once
#[allow(dead_code)]
pub fn invalidate_interaction_counts_batch(event_ids: &[String]) {
    {
        let mut cache = get_counts_cache()
            .lock()
            .unwrap_or_else(|poisoned| {
                log::warn!("Counts cache mutex was poisoned, recovering");
                poisoned.into_inner()
            });
        for event_id in event_ids {
            cache.invalidate(event_id);
        }
    }
    log::debug!("Invalidated interaction counts cache for {} events", event_ids.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interaction_counts_default() {
        let counts = InteractionCounts::default();
        assert_eq!(counts.replies, 0);
        assert_eq!(counts.likes, 0);
        assert_eq!(counts.reposts, 0);
        assert_eq!(counts.zaps, 0);
        assert_eq!(counts.zap_amount_sats, 0);
    }

    #[test]
    fn test_parse_zap_description() {
        let desc = r#"{"amount":"5000","content":"Great post!"}"#;
        let amount = parse_amount_from_description(desc);
        assert_eq!(amount, Some(5));
    }
}
