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
//! - Cache size: 10000 events
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
use nostr_sdk::prelude::{EventId, Kind};
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

impl InteractionCounts {
    /// Merge another (fresher or partial) observation into this one.
    ///
    /// Counter fields take the max — both sides are under-counts of relay
    /// reality, so the larger observation is the better estimate. This
    /// protects freshly painted counts (e.g. local-DB rows landing after a
    /// thread's author-inbox sweep) from being wiped by a later batch
    /// result whose L2-cached rows lag behind (cached zeros).
    ///
    /// User-state fields prefer `other` when set: an explicit `Some` —
    /// including `Some(false)` from a `-` unlike — is a stronger signal
    /// than a stale `None`.
    pub fn merge_max(&mut self, other: &Self) {
        self.replies = self.replies.max(other.replies);
        self.likes = self.likes.max(other.likes);
        self.reposts = self.reposts.max(other.reposts);
        self.zaps = self.zaps.max(other.zaps);
        self.zap_amount_sats = self.zap_amount_sats.max(other.zap_amount_sats);
        if other.user_liked.is_some() {
            self.user_liked = other.user_liked;
        }
        if other.user_reaction.is_some() {
            self.user_reaction = other.user_reaction.clone();
        }
        if other.user_reaction_url.is_some() {
            self.user_reaction_url = other.user_reaction_url.clone();
        }
        if other.user_reposted.is_some() {
            self.user_reposted = other.user_reposted;
        }
        if other.user_repost_id.is_some() {
            self.user_repost_id = other.user_repost_id.clone();
        }
        if other.user_zapped.is_some() {
            self.user_zapped = other.user_zapped;
        }
    }

    /// Add another observation's counters into this one (R4 review).
    ///
    /// Used to re-apply post-partition stream deltas on top of a
    /// relay-truth baseline (see [`merge_counts_batch_into`]). User-state
    /// fields are ignored — stream deltas never carry them.
    pub fn add_counters(&mut self, other: &Self) {
        self.replies += other.replies;
        self.likes += other.likes;
        self.reposts += other.reposts;
        self.zaps += other.zaps;
        self.zap_amount_sats += other.zap_amount_sats;
    }
}

/// R3 review merge policy for a counts batch landing on a painted signal
/// map (the note viewer's flow): rows the batch re-observed from relays
/// (`complete` + not L2-cache-served) are relay truth and REPLACE their
/// painted counters — NIP-25 deletions and un-likes are honored by relays
/// but never pruned from the local DB, so max-merging them left painted
/// counts stuck high forever. Cache-served rows (and every row of a
/// truncated observation) still max-merge: both sides are under-counts of
/// relay reality there, and the larger observation is the better estimate.
///
/// R4 review — `stream_deltas`: relay truth is bounded `until = T-1`
/// (the partition the note viewer hands the live stream everything ≥ T
/// of), while the live stream may already have merged +1 increments for
/// post-partition events into the painted row during the multi-second
/// backfill window. A bare replace would discard those increments (badge
/// flicker + a lasting under-count — nothing re-observes them). Relay
/// truth therefore replaces the *baseline* and the recorded post-partition
/// deltas are re-applied on top.
pub fn merge_counts_batch_into(
    map: &mut HashMap<String, InteractionCounts>,
    batch: InteractionCountsBatch,
    stream_deltas: &HashMap<String, InteractionCounts>,
) {
    for (id, incoming) in batch.counts {
        let prefer_incoming = batch.complete && !batch.cached_ids.contains(&id);
        match map.get_mut(&id) {
            Some(existing) => {
                if prefer_incoming {
                    let mut row = incoming;
                    if let Some(delta) = stream_deltas.get(&id) {
                        row.add_counters(delta);
                    }
                    *existing = row;
                } else {
                    existing.merge_max(&incoming);
                }
            }
            None => {
                // Absent key: neither the paint nor the stream produced a
                // row, but apply any recorded delta defensively anyway so
                // seeded rows match the replace path's semantics.
                let mut row = incoming;
                if let Some(delta) = stream_deltas.get(&id) {
                    row.add_counters(delta);
                }
                map.insert(id, row);
            }
        }
    }
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
        // Pop expired entry outside the borrow
        self.cache.pop(event_id);
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
                kind if is_reply_kind(kind) => cached.counts.replies += 1,
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
                Kind::Repost => {
                    cached.counts.reposts += 1;
                    if is_current_user {
                        cached.counts.user_reposted = Some(true);
                    }
                }
                Kind::ZapReceipt => {
                    cached.counts.zaps += 1;
                    if let Some(amount) = zap_amount {
                        cached.counts.zap_amount_sats += amount;
                    }
                    if is_current_user {
                        cached.counts.user_zapped = Some(true);
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
            self.cache.put(
                event_id.to_string(),
                CachedCounts::new(InteractionCounts::default()),
            );
        }
        &mut self.cache.get_mut(event_id).unwrap().counts
    }
}

/// Cache configuration:
/// - Capacity: 10000 events
/// - TTL: 5 minutes (balance freshness vs performance)
static COUNTS_CACHE: OnceLock<Mutex<CountsCache>> = OnceLock::new();

/// Get or initialize the counts cache
pub(crate) fn get_counts_cache() -> &'static Mutex<CountsCache> {
    COUNTS_CACHE.get_or_init(|| Mutex::new(CountsCache::new(10000, Duration::from_secs(300))))
}

pub(crate) fn is_reply_kind(kind: Kind) -> bool {
    matches!(kind, Kind::TextNote | Kind::Comment)
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
        let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
            log::warn!("Counts cache mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        cache.invalidate(event_id);
    }
    log::debug!("Invalidated interaction counts cache for {}", event_id);
}

/// Invalidate cached counts for multiple events at once
pub fn invalidate_interaction_counts_batch(event_ids: &[String]) {
    {
        let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
            log::warn!("Counts cache mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        for event_id in event_ids {
            cache.invalidate(event_id);
        }
    }
    log::debug!(
        "Invalidated interaction counts cache for {} events",
        event_ids.len()
    );
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

    /// Cached zeros must not wipe counts painted from a fresher source:
    /// counters merge to the max (both sides are under-counts).
    #[test]
    fn test_merge_max_keeps_larger_counters() {
        let mut current = InteractionCounts {
            replies: 3,
            likes: 12,
            reposts: 2,
            zaps: 1,
            zap_amount_sats: 5_000,
            ..InteractionCounts::default()
        };
        let incoming = InteractionCounts::default(); // all-zero L2 row
        current.merge_max(&incoming);
        assert_eq!(current.replies, 3);
        assert_eq!(current.likes, 12);
        assert_eq!(current.reposts, 2);
        assert_eq!(current.zaps, 1);
        assert_eq!(current.zap_amount_sats, 5_000);
    }

    /// The batch result may carry larger counters than the painted local
    /// counts (e.g. after an author-inbox sweep dropped events into the
    /// DB): merging must raise, not keep, the smaller side.
    #[test]
    fn test_merge_max_raises_to_incoming() {
        let mut current = InteractionCounts {
            likes: 1,
            ..InteractionCounts::default()
        };
        let incoming = InteractionCounts {
            likes: 7,
            reposts: 4,
            zap_amount_sats: 2_100,
            ..InteractionCounts::default()
        };
        current.merge_max(&incoming);
        assert_eq!(current.likes, 7);
        assert_eq!(current.reposts, 4);
        assert_eq!(current.zap_amount_sats, 2_100);
    }

    /// User-state fields prefer the incoming `Some` when set — including
    /// `Some(false)` (a `-` unlike is a stronger signal than a stale
    /// `Some(true)`) — and keep the current value when incoming is `None`.
    #[test]
    fn test_merge_max_user_state_prefers_incoming_some() {
        let mut current = InteractionCounts {
            user_liked: Some(true),
            user_reaction: Some("+".to_string()),
            user_zapped: Some(true),
            ..InteractionCounts::default()
        };
        let incoming = InteractionCounts {
            user_liked: Some(false),
            user_reaction: None,
            user_reposted: Some(true),
            user_repost_id: Some("abc".to_string()),
            ..InteractionCounts::default()
        };
        current.merge_max(&incoming);
        assert_eq!(current.user_liked, Some(false));
        assert_eq!(current.user_reaction, Some("+".to_string()), "None keeps current");
        assert_eq!(current.user_zapped, Some(true));
        assert_eq!(current.user_reposted, Some(true));
        assert_eq!(current.user_repost_id, Some("abc".to_string()));
    }

    fn counts_with(likes: usize) -> InteractionCounts {
        InteractionCounts {
            likes,
            ..InteractionCounts::default()
        }
    }

    fn batch(
        counts: Vec<(&str, InteractionCounts)>,
        cached: &[&str],
        complete: bool,
    ) -> super::InteractionCountsBatch {
        super::InteractionCountsBatch {
            counts: counts
                .into_iter()
                .map(|(id, c)| (id.to_string(), c))
                .collect(),
            cached_ids: cached.iter().map(|id| id.to_string()).collect(),
            complete,
        }
    }

    /// R3 review: a fresh, untruncated relay observation may correct a
    /// painted count DOWNWARD (deletion / un-like convergence).
    #[test]
    fn merge_batch_fresh_relay_truth_replaces_painted_counts() {
        let mut painted = HashMap::from([
            ("a".to_string(), counts_with(5)), // fresh row, likes deleted since
            ("b".to_string(), counts_with(5)), // cache-served row
        ]);
        let b = batch(
            vec![("a", counts_with(2)), ("b", counts_with(2))],
            &["b"],
            true,
        );
        merge_counts_batch_into(&mut painted, b, &HashMap::new());
        assert_eq!(painted["a"].likes, 2, "fresh relay truth replaces");
        assert_eq!(painted["b"].likes, 5, "cache-served rows max-merge");
    }

    /// A truncated observation is never authoritative: every row
    /// max-merges, even the freshly-fetched ones.
    #[test]
    fn merge_batch_truncated_observation_max_merges_everything() {
        let mut painted = HashMap::from([("a".to_string(), counts_with(5))]);
        let b = batch(vec![("a", counts_with(2))], &[], false);
        merge_counts_batch_into(&mut painted, b, &HashMap::new());
        assert_eq!(painted["a"].likes, 5, "truncated batches never lower counts");
    }

    /// Fresh rows still seed absent keys.
    #[test]
    fn merge_batch_inserts_missing_ids() {
        let mut painted = HashMap::new();
        let b = batch(vec![("a", counts_with(3))], &[], true);
        merge_counts_batch_into(&mut painted, b, &HashMap::new());
        assert_eq!(painted["a"].likes, 3);
    }

    /// R4 review: the relay-truth replace must re-apply the post-partition
    /// stream deltas — the batch is bounded `until = T-1` while the live
    /// stream may have merged increments for events ≥ T into the painted
    /// row during the backfill window. Replace the baseline, keep the
    /// streamed increment.
    #[test]
    fn merge_batch_relay_truth_reapplies_stream_deltas() {
        let mut painted = HashMap::from([("a".to_string(), counts_with(5))]);
        let deltas = HashMap::from([("a".to_string(), counts_with(1))]);
        // Relay recount says 2 likes below the partition; 1 streamed like
        // landed after it. Final: 3, not 2 (dropping the stream) and not
        // 5 (stale painted over-count).
        let b = batch(vec![("a", counts_with(2))], &[], true);
        merge_counts_batch_into(&mut painted, b, &deltas);
        assert_eq!(painted["a"].likes, 3, "baseline replaced, delta re-applied");
    }

    /// R4 review: cache-served rows never replace, so their painted value
    /// (which already embeds the streamed increment) must stand — the
    /// delta is NOT added again on top of a max-merge.
    #[test]
    fn merge_batch_cached_rows_do_not_double_apply_deltas() {
        let mut painted = HashMap::from([("b".to_string(), counts_with(5))]);
        let deltas = HashMap::from([("b".to_string(), counts_with(1))]);
        let b = batch(vec![("b", counts_with(2))], &["b"], true);
        merge_counts_batch_into(&mut painted, b, &deltas);
        assert_eq!(painted["b"].likes, 5, "max-merge keeps painted; no delta add");
    }

    /// R4 review: a row the paint never produced but the stream did
    /// (recorded only in the ledger) seeds with the delta applied.
    #[test]
    fn merge_batch_seeds_absent_row_with_delta() {
        let mut painted = HashMap::new();
        let deltas = HashMap::from([("a".to_string(), counts_with(1))]);
        let b = batch(vec![("a", counts_with(2))], &[], true);
        merge_counts_batch_into(&mut painted, b, &deltas);
        assert_eq!(painted["a"].likes, 3, "seeded row includes the stream delta");
    }

    #[test]
    fn test_parse_zap_description() {
        let desc = r#"{"amount":"5000","content":"Great post!"}"#;
        let amount = parse_amount_from_description(desc);
        assert_eq!(amount, Some(5));
    }

    #[test]
    fn test_parse_amount_from_tags() {
        let desc = r#"{"tags":[["amount","5000"]]}"#;
        let amount = parse_amount_from_description(desc);
        assert_eq!(amount, Some(5));
    }

    #[test]
    fn test_parse_amount_from_numeric() {
        let desc = r#"{"amount":3000}"#;
        let amount = parse_amount_from_description(desc);
        assert_eq!(amount, Some(3));
    }

    #[test]
    fn test_parse_amount_invalid_json() {
        let amount = parse_amount_from_description("not json");
        assert_eq!(amount, None);
    }

    #[test]
    fn test_parse_amount_missing_amount() {
        let desc = r#"{"content":"hello"}"#;
        let amount = parse_amount_from_description(desc);
        assert_eq!(amount, None);
    }

    #[test]
    fn test_cached_counts_is_valid() {
        let cached = CachedCounts::new(InteractionCounts::default());
        assert!(cached.is_valid(Duration::from_secs(60)));
    }

    #[test]
    fn test_cached_counts_expired() {
        let cached = CachedCounts::new(InteractionCounts::default());
        assert!(!cached.is_valid(Duration::ZERO));
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = CountsCache::new(10, Duration::from_secs(60));
        let counts = InteractionCounts {
            likes: 5,
            reposts: 2,
            ..Default::default()
        };
        cache.insert("abc".to_string(), counts.clone());
        assert_eq!(cache.get("abc"), Some(counts));
        assert_eq!(cache.get("missing"), None);
    }

    #[test]
    fn test_cache_ttl_expiration() {
        let mut cache = CountsCache::new(10, Duration::from_secs(300));
        cache.insert("key".to_string(), InteractionCounts::default());
        // Set cached_at to the past so the entry is expired
        cache.cache.get_mut("key").unwrap().cached_at = Instant::now() - Duration::from_secs(600);
        assert!(cache.get("key").is_none());
        // Verify the expired entry was removed from the cache
        assert!(cache.cache.peek("key").is_none());
    }

    #[test]
    fn test_cache_eviction() {
        let mut cache = CountsCache::new(2, Duration::from_secs(60));
        cache.insert("a".to_string(), InteractionCounts::default());
        cache.insert("b".to_string(), InteractionCounts::default());
        cache.insert("c".to_string(), InteractionCounts::default());
        // "a" should be evicted as LRU
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn test_cache_invalidate() {
        let mut cache = CountsCache::new(10, Duration::from_secs(60));
        cache.insert("key".to_string(), InteractionCounts::default());
        cache.invalidate("key");
        assert!(cache.get("key").is_none());
    }

    #[test]
    fn test_is_reply_kind_includes_comments() {
        assert!(is_reply_kind(Kind::TextNote));
        assert!(is_reply_kind(Kind::Comment));
        assert!(!is_reply_kind(Kind::Reaction));
    }

    #[test]
    fn test_cache_increment_counts_comment_as_reply() {
        let mut cache = CountsCache::new(10, Duration::from_secs(60));
        cache.insert("key".to_string(), InteractionCounts::default());
        cache.increment("key", Kind::Comment, None, false, None);

        let counts = cache.get("key").expect("cached counts should exist");
        assert_eq!(counts.replies, 1);
    }
}
