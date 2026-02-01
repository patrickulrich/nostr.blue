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
use crate::stores::nostr_client::get_client;
use crate::stores::signer::SIGNER_INFO;
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use futures::join;
use instant::{Duration, Instant};
use lru::LruCache;
use nostr_relay_pool::{SyncDirection, SyncOptions};
use nostr_sdk::{
    Event, EventId, Filter, Kind, RelayPoolNotification, SubscriptionId, TagStandard,
    Timestamp,
};
use std::collections::{HashMap, HashSet};
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
}
/// Cache entry with TTL tracking
#[derive(Clone, Debug)]
struct CachedCounts {
    counts: InteractionCounts,
    cached_at: Instant,
}
impl CachedCounts {
    fn new(counts: InteractionCounts) -> Self {
        Self {
            counts,
            cached_at: Instant::now(),
        }
    }
    /// Check if cache entry is still valid (within TTL)
    fn is_valid(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() < ttl
    }
}
/// L2 cache for interaction counts (Phase 3.5)
///
/// In-memory LRU cache that sits between database and UI:
/// - Reduces redundant queries for recently-viewed events
/// - Automatic TTL-based freshness control
/// - LRU eviction prevents unbounded growth
struct CountsCache {
    cache: LruCache<String, CachedCounts>,
    ttl: Duration,
}
impl CountsCache {
    fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(capacity).unwrap()),
            ttl,
        }
    }
    /// Get cached counts if they exist and are still valid
    fn get(&mut self, event_id: &str) -> Option<InteractionCounts> {
        if let Some(cached) = self.cache.get(event_id) {
            if cached.is_valid(self.ttl) {
                return Some(cached.counts.clone());
            }
        }
        None
    }
    /// Cache counts for an event
    fn insert(&mut self, event_id: String, counts: InteractionCounts) {
        self.cache.put(event_id, CachedCounts::new(counts));
    }
    /// Get multiple counts from cache, returning only valid entries
    fn get_batch(
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
    fn insert_batch(&mut self, counts_map: HashMap<String, InteractionCounts>) {
        for (event_id, counts) in counts_map {
            self.insert(event_id, counts);
        }
    }
    /// Invalidate (remove) cached counts for an event
    ///
    /// Useful when user publishes a new interaction (like, repost, etc.)
    #[allow(dead_code)]
    fn invalidate(&mut self, event_id: &str) {
        self.cache.pop(event_id);
    }
    /// Increment a specific count type for an event (for negentropy sync)
    ///
    /// Updates an existing cache entry with new interaction data.
    /// If the event isn't cached, this is a no-op.
    #[allow(dead_code)]
    fn increment(
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
    fn get_or_create_mut(&mut self, event_id: &str) -> &mut InteractionCounts {
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
fn get_counts_cache() -> &'static Mutex<CountsCache> {
    COUNTS_CACHE
        .get_or_init(|| { Mutex::new(CountsCache::new(1000, Duration::from_secs(300))) })
}
/// NIP-45 support status for a relay
#[derive(Clone)]
struct Nip45SupportStatus {
    /// Whether relay supports COUNT
    supported: bool,
    /// When this status was recorded
    checked_at: Instant,
}
impl Nip45SupportStatus {
    fn new(supported: bool) -> Self {
        Self {
            supported,
            checked_at: Instant::now(),
        }
    }
    /// Negative results expire after 10 minutes (relay may have been updated)
    /// Positive results don't expire (once confirmed, unlikely to change)
    fn is_valid(&self) -> bool {
        if self.supported {
            true
        } else {
            self.checked_at.elapsed() < Duration::from_secs(600)
        }
    }
}
/// Cache for tracking which relays support NIP-45 COUNT
///
/// - `Nip45SupportStatus { supported: true }`: Relay supports COUNT (permanent)
/// - `Nip45SupportStatus { supported: false }`: Relay failed COUNT (TTL: 10 minutes)
/// - Not present: Unknown, needs testing
static NIP45_SUPPORT: OnceLock<Mutex<HashMap<String, Nip45SupportStatus>>> = OnceLock::new();
/// Get or initialize the NIP-45 support cache
fn get_nip45_cache() -> &'static Mutex<HashMap<String, Nip45SupportStatus>> {
    NIP45_SUPPORT.get_or_init(|| Mutex::new(HashMap::new()))
}
/// Attempt to get COUNT from relays that support NIP-45
///
/// This is a best-effort optimization - if no relays support COUNT or
/// all COUNT requests fail, returns None and caller should fall back
/// to full event fetch.
///
/// # Arguments
/// * `event_id` - The event to count interactions for
/// * `kind` - The interaction kind to count (Reaction, Repost, etc.)
/// * `timeout` - Short timeout for COUNT request (should be quick)
///
/// # Returns
/// * `Some(count)` - COUNT succeeded on at least one relay
/// * `None` - COUNT not supported or failed on all relays
#[allow(dead_code)]
async fn try_count_from_relays(
    event_id: &EventId,
    kind: Kind,
    timeout: Duration,
) -> Option<usize> {
    let client = get_client()?;
    let filter = Filter::new().kind(kind).event(*event_id);
    let relays = client.relays().await;
    for (url, relay) in relays.iter() {
        let url_str = url.to_string();
        let should_try = {
            let cache = get_nip45_cache()
                .lock()
                .unwrap_or_else(|poisoned| {
                    log::warn!("NIP-45 cache mutex was poisoned, recovering");
                    poisoned.into_inner()
                });
            match cache.get(&url_str) {
                Some(status) if status.is_valid() => status.supported,
                Some(_) => true,
                None => true,
            }
        };
        if !should_try {
            continue;
        }
        let count_timeout = Duration::from_millis(timeout.as_millis().min(2000) as u64);
        match relay.count_events(filter.clone(), count_timeout).await {
            Ok(count) => {
                {
                    let mut cache = get_nip45_cache()
                        .lock()
                        .unwrap_or_else(|poisoned| {
                            log::warn!("NIP-45 cache mutex was poisoned, recovering");
                            poisoned.into_inner()
                        });
                    cache.insert(url_str, Nip45SupportStatus::new(true));
                }
                log::debug!("COUNT from {}: {} events", url, count);
                return Some(count);
            }
            Err(e) => {
                {
                    let mut cache = get_nip45_cache()
                        .lock()
                        .unwrap_or_else(|poisoned| {
                            log::warn!("NIP-45 cache mutex was poisoned, recovering");
                            poisoned.into_inner()
                        });
                    cache.insert(url_str, Nip45SupportStatus::new(false));
                }
                log::debug!("COUNT failed on {}: {}", url, e);
            }
        }
    }
    None
}
/// Get interaction counts using COUNT when available, with fallback to full fetch
///
/// This is the COUNT-first strategy with silent fallback:
/// 1. Try COUNT on supporting relays (fast, low bandwidth)
/// 2. If COUNT unavailable, fall back to full event fetch
///
/// Note: COUNT only returns totals, not user's own reaction state.
/// User reaction state is determined separately via full fetch or cache.
#[allow(dead_code)]
pub async fn get_counts_with_count_fallback(
    event_id: &EventId,
    timeout: Duration,
) -> InteractionCounts {
    let mut counts = InteractionCounts::default();
    let (reactions, reposts, replies, zaps) = join!(
        try_count_from_relays(event_id, Kind::Reaction, timeout),
        try_count_from_relays(event_id, Kind::Repost, timeout),
        try_count_from_relays(event_id, Kind::TextNote, timeout),
        try_count_from_relays(event_id, Kind::ZapReceipt, timeout),
    );
    let mut needs_fallback = false;
    if let Some(count) = reactions {
        counts.likes = count;
    } else {
        needs_fallback = true;
    }
    if let Some(count) = reposts {
        counts.reposts = count;
    } else {
        needs_fallback = true;
    }
    if let Some(count) = replies {
        counts.replies = count;
    } else {
        needs_fallback = true;
    }
    if let Some(count) = zaps {
        counts.zaps = count;
    }
    if needs_fallback {
        log::debug!("COUNT incomplete for {}, using full fetch", event_id.to_hex());
        if let Ok(batch_counts) = fetch_interaction_counts_batch(
                vec![*event_id],
                timeout,
            )
            .await
        {
            if let Some(fetched) = batch_counts.get(&event_id.to_hex()) {
                return fetched.clone();
            }
        }
    }
    counts
}
/// Batch fetch interaction counts for multiple events
///
/// # Arguments
/// * `event_ids` - Vector of event IDs to fetch interactions for
/// * `timeout` - Query timeout duration
///
/// # Returns
/// HashMap mapping event_id (hex) to its interaction counts
///
/// # Example
/// ```
/// let event_ids = feed_events.iter().map(|e| e.id).collect();
/// let counts = fetch_interaction_counts_batch(event_ids, Duration::from_secs(5)).await?;
///
/// // Pass to NoteCard
/// NoteCard { event, counts: counts.get(&event.id.to_hex()) }
/// ```
pub async fn fetch_interaction_counts_batch(
    event_ids: Vec<EventId>,
    timeout: Duration,
) -> Result<HashMap<String, InteractionCounts>, String> {
    if event_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let (cached_counts, cache_hits, uncached_ids) = {
        let mut cache = get_counts_cache()
            .lock()
            .unwrap_or_else(|poisoned| {
                log::warn!("Counts cache mutex was poisoned, recovering");
                poisoned.into_inner()
            });
        let cached_counts = cache.get_batch(&event_ids);
        let cache_hits = cached_counts.len();
        let uncached_ids: Vec<EventId> = event_ids
            .iter()
            .filter(|id| !cached_counts.contains_key(&id.to_hex()))
            .cloned()
            .collect();
        (cached_counts, cache_hits, uncached_ids)
    };
    log::info!(
        "Batch fetching interaction counts for {} events ({} cache hits, {} cache misses)",
        event_ids.len(), cache_hits, uncached_ids.len()
    );
    if uncached_ids.is_empty() {
        log::info!("All counts served from cache!");
        return Ok(cached_counts);
    }
    let client = get_client().ok_or("Client not initialized")?;
    const MAX_RELAY_LIMIT: usize = 5000;
    let requested_limit = uncached_ids.len() * 100;
    let capped_limit = requested_limit.min(MAX_RELAY_LIMIT);
    let filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Reaction, Kind::Repost, Kind::ZapReceipt])
        .events(uncached_ids.clone())
        .limit(capped_limit);
    let db_events: Vec<Event> = match client.database().query(filter.clone()).await {
        Ok(events) => {
            let count = events.len();
            if count > 0 {
                log::info!("Found {} interaction events in local database", count);
            }
            events.into_iter().collect()
        }
        Err(e) => {
            log::debug!("Database query for interactions failed: {}", e);
            Vec::new()
        }
    };
    let relay_events: Vec<Event> = match client.fetch_events(filter, timeout).await {
        Ok(events) => {
            log::info!("Fetched {} interaction events from relays", events.len());
            events.into_iter().collect()
        }
        Err(e) => {
            if !db_events.is_empty() {
                log::warn!(
                    "Relay fetch failed but using {} cached events: {}", db_events.len(),
                    e
                );
                Vec::new()
            } else {
                return Err(format!("Failed to fetch interactions: {}", e));
            }
        }
    };
    let mut event_map: HashMap<EventId, Event> = HashMap::new();
    for event in db_events {
        event_map.insert(event.id, event);
    }
    for event in relay_events {
        event_map.insert(event.id, event);
    }
    let events: Vec<Event> = event_map.into_values().collect();
    log::info!(
        "Processing {} total interaction events (DB + relay, deduplicated)", events.len()
    );
    let mut freshly_fetched: HashMap<String, InteractionCounts> = HashMap::new();
    let requested_ids: std::collections::HashSet<String> = uncached_ids
        .iter()
        .map(|id| id.to_hex())
        .collect();
    for event_id in &uncached_ids {
        freshly_fetched.insert(event_id.to_hex(), InteractionCounts::default());
    }
    let current_user_pk: Option<nostr_sdk::PublicKey> = SIGNER_INFO
        .read()
        .as_ref()
        .and_then(|info| nostr_sdk::PublicKey::from_hex(&info.public_key).ok());
    let mut user_reactions_seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for event in events {
        let referenced_event_id = match extract_referenced_event(
            &event,
            &requested_ids,
        ) {
            Some(id) => id,
            None => continue,
        };
        let event_key = referenced_event_id.to_hex();
        let counts = freshly_fetched.entry(event_key.clone()).or_default();
        let is_current_user = current_user_pk
            .map(|pk| event.pubkey == pk)
            .unwrap_or(false);
        match event.kind {
            Kind::TextNote => counts.replies += 1,
            Kind::Reaction => {
                let content = event.content.trim();
                if content != "-" {
                    counts.likes += 1;
                }
                if is_current_user && !user_reactions_seen.contains(&event_key) {
                    user_reactions_seen.insert(event_key.clone());
                    if content == "-" {
                        counts.user_liked = Some(false);
                        counts.user_reaction = None;
                        counts.user_reaction_url = None;
                    } else {
                        counts.user_liked = Some(true);
                        counts.user_reaction = Some(content.to_string());
                        if content.starts_with(':') && content.ends_with(':')
                            && content.len() > 2
                        {
                            let shortcode = &content[1..content.len() - 1];
                            let emoji_url = event
                                .tags
                                .iter()
                                .find_map(|tag| {
                                    let tag_slice = tag.as_slice();
                                    if tag_slice.len() >= 3
                                        && tag_slice.first().map(|s| s.as_str()) == Some("emoji")
                                        && tag_slice.get(1).map(|s| s.as_str()) == Some(shortcode)
                                    {
                                        tag_slice.get(2).map(|s| s.to_string())
                                    } else {
                                        None
                                    }
                                });
                            counts.user_reaction_url = emoji_url;
                        } else {
                            counts.user_reaction_url = None;
                        }
                    }
                }
            }
            Kind::Repost => counts.reposts += 1,
            Kind::ZapReceipt => {
                counts.zaps += 1;
                if let Some(amount) = extract_zap_amount(&event) {
                    counts.zap_amount_sats += amount;
                }
            }
            _ => {}
        }
    }
    {
        let mut cache = get_counts_cache()
            .lock()
            .unwrap_or_else(|poisoned| {
                log::warn!("Counts cache mutex was poisoned, recovering");
                poisoned.into_inner()
            });
        cache.insert_batch(freshly_fetched.clone());
    }
    let mut final_counts = cached_counts;
    final_counts.extend(freshly_fetched);
    log::info!(
        "Returning {} interaction counts ({} from cache, {} freshly fetched)",
        final_counts.len(), cache_hits, uncached_ids.len()
    );
    Ok(final_counts)
}
/// Sync interaction counts using negentropy set reconciliation
///
/// This is more efficient than full fetch for subsequent refreshes:
/// - Uses negentropy to determine which events are missing locally
/// - Only fetches new events that appeared since last sync
/// - Incrementally updates cached counts without refetching everything
///
/// # When to use
/// - First load: Use `fetch_interaction_counts_batch` (no local data to reconcile)
/// - Subsequent refreshes: Use `sync_interaction_counts` (incremental updates)
///
/// # Fallback
/// If sync fails, silently falls back to full fetch behavior.
pub async fn sync_interaction_counts(
    event_ids: Vec<EventId>,
    timeout: Duration,
) -> Result<HashMap<String, InteractionCounts>, String> {
    if event_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let client = get_client().ok_or("Client not initialized")?;
    let filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Reaction, Kind::Repost, Kind::ZapReceipt])
        .events(event_ids.clone());
    let sync_opts = SyncOptions::default()
        .direction(SyncDirection::Down)
        .initial_timeout(timeout);
    let sync_result = client.sync(filter.clone(), &sync_opts).await;
    match sync_result {
        Ok(output) => {
            let reconciliation = output.val;
            let new_event_count = reconciliation.received.len();
            if new_event_count == 0 {
                log::info!("Negentropy sync: no new interaction events found");
                let mut cache = get_counts_cache()
                    .lock()
                    .unwrap_or_else(|poisoned| {
                        log::warn!("Counts cache mutex was poisoned, recovering");
                        poisoned.into_inner()
                    });
                return Ok(cache.get_batch(&event_ids));
            }
            log::info!(
                "Negentropy sync: {} new interaction events to process", new_event_count
            );
            let mut new_events = Vec::new();
            for event_id in &reconciliation.received {
                if let Ok(Some(event)) = client.database().event_by_id(event_id).await {
                    new_events.push(event);
                }
            }
            let mut result = {
                let mut cache = get_counts_cache()
                    .lock()
                    .unwrap_or_else(|poisoned| {
                        log::warn!("Counts cache mutex was poisoned, recovering");
                        poisoned.into_inner()
                    });
                cache.get_batch(&event_ids)
            };
            for event_id in &event_ids {
                let hex = event_id.to_hex();
                result.entry(hex).or_insert_with(InteractionCounts::default);
            }
            let current_user_pk: Option<nostr_sdk::PublicKey> = SIGNER_INFO
                .read()
                .as_ref()
                .and_then(|info| nostr_sdk::PublicKey::from_hex(&info.public_key).ok());
            let requested_ids: std::collections::HashSet<String> = event_ids
                .iter()
                .map(|id| id.to_hex())
                .collect();
            for event in new_events {
                let referenced_event_id = match extract_referenced_event(
                    &event,
                    &requested_ids,
                ) {
                    Some(id) => id,
                    None => continue,
                };
                let event_key = referenced_event_id.to_hex();
                let counts = result.entry(event_key.clone()).or_default();
                let is_current_user = current_user_pk
                    .map(|pk| event.pubkey == pk)
                    .unwrap_or(false);
                match event.kind {
                    Kind::TextNote => counts.replies += 1,
                    Kind::Reaction => {
                        let content = event.content.trim();
                        if content != "-" {
                            counts.likes += 1;
                        }
                        if is_current_user {
                            if content == "-" {
                                counts.user_liked = Some(false);
                                counts.user_reaction = None;
                                counts.user_reaction_url = None;
                            } else {
                                counts.user_liked = Some(true);
                                counts.user_reaction = Some(content.to_string());
                                if content.starts_with(':') && content.ends_with(':')
                                    && content.len() > 2
                                {
                                    let shortcode = &content[1..content.len() - 1];
                                    let emoji_url = event
                                        .tags
                                        .iter()
                                        .find_map(|tag| {
                                            let tag_slice = tag.as_slice();
                                            if tag_slice.len() >= 3
                                                && tag_slice.first().map(|s| s.as_str()) == Some("emoji")
                                                && tag_slice.get(1).map(|s| s.as_str()) == Some(shortcode)
                                            {
                                                tag_slice.get(2).map(|s| s.to_string())
                                            } else {
                                                None
                                            }
                                        });
                                    counts.user_reaction_url = emoji_url;
                                } else {
                                    counts.user_reaction_url = None;
                                }
                            }
                        }
                    }
                    Kind::Repost => counts.reposts += 1,
                    Kind::ZapReceipt => {
                        counts.zaps += 1;
                        if let Some(amount) = extract_zap_amount(&event) {
                            counts.zap_amount_sats += amount;
                        }
                    }
                    _ => {}
                }
            }
            {
                let mut cache = get_counts_cache()
                    .lock()
                    .unwrap_or_else(|poisoned| {
                        log::warn!("Counts cache mutex was poisoned, recovering");
                        poisoned.into_inner()
                    });
                cache.insert_batch(result.clone());
            }
            log::info!(
                "Negentropy sync complete: updated {} interaction counts", result.len()
            );
            Ok(result)
        }
        Err(e) => {
            log::debug!("Negentropy sync failed, falling back to full fetch: {}", e);
            fetch_interaction_counts_batch(event_ids, timeout).await
        }
    }
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
/// Extract the event ID being referenced by an interaction event
/// Only returns the event ID if it matches one of the requested IDs
/// If requested_ids is empty, returns the first 'e' tag found (for trending/all events)
fn extract_referenced_event(
    event: &Event,
    requested_ids: &std::collections::HashSet<String>,
) -> Option<EventId> {
    for tag in event.tags.iter() {
        if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
            if requested_ids.is_empty() {
                return Some(*event_id);
            }
            if requested_ids.contains(&event_id.to_hex()) {
                return Some(*event_id);
            }
        }
    }
    None
}
/// Extract zap amount in satoshis from a zap event (kind 9735)
fn extract_zap_amount(event: &Event) -> Option<u64> {
    if let Some(bolt11_tag) = event
        .tags
        .iter()
        .find(|tag| {
            tag.as_slice().first().map(|k| k.as_str() == "bolt11").unwrap_or(false)
        })
    {
        if let Some(bolt11) = bolt11_tag.as_slice().get(1) {
            if let Some(amount) = parse_bolt11_amount(bolt11.as_str()) {
                return Some(amount);
            }
        }
    }
    if let Some(description_tag) = event
        .tags
        .iter()
        .find(|tag| {
            tag.as_slice().first().map(|k| k.as_str() == "description").unwrap_or(false)
        })
    {
        if let Some(desc) = description_tag.as_slice().get(1) {
            return parse_amount_from_description(desc.as_str());
        }
    }
    None
}
/// Parse amount from bolt11 invoice string
/// This is a simplified parser - a full implementation would use a bolt11 crate
fn parse_bolt11_amount(_bolt11: &str) -> Option<u64> {
    None
}
/// Parse amount from zap request description
fn parse_amount_from_description(description: &str) -> Option<u64> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(description) {
        if let Some(tags) = json.get("tags").and_then(|t| t.as_array()) {
            for tag in tags {
                if let Some(tag_vals) = tag.as_array() {
                    if tag_vals.first().and_then(|v| v.as_str()) == Some("amount") {
                        if let Some(amount_str) = tag_vals
                            .get(1)
                            .and_then(|v| v.as_str())
                        {
                            if let Ok(millisats) = amount_str.parse::<u64>() {
                                return Some(millisats / 1000);
                            }
                        }
                    }
                }
            }
        }
        if let Some(amount) = json.get("amount") {
            if let Some(amount_str) = amount.as_str() {
                if let Ok(millisats) = amount_str.parse::<u64>() {
                    return Some(millisats / 1000);
                }
            } else if let Some(amount_num) = amount.as_u64() {
                return Some(amount_num / 1000);
            }
        }
    }
    None
}
/// Fetch interaction counts for a time range (useful for trending/popular feeds)
///
/// This fetches all interactions in a given time period and groups by event.
/// Useful for "trending" or "popular" feeds that want to rank by recent engagement.
#[allow(dead_code)]
pub async fn fetch_trending_interactions(
    since: Timestamp,
    limit: usize,
) -> Result<HashMap<String, InteractionCounts>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    log::info!("Fetching trending interactions since {}", since);
    let filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Reaction, Kind::Repost, Kind::ZapReceipt])
        .since(since)
        .limit(limit);
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch trending interactions: {}", e))?;
    let mut counts_map: HashMap<String, InteractionCounts> = HashMap::new();
    let empty_filter = std::collections::HashSet::new();
    for event in events {
        let referenced_event_id = match extract_referenced_event(&event, &empty_filter) {
            Some(id) => id,
            None => continue,
        };
        let event_key = referenced_event_id.to_hex();
        let counts = counts_map.entry(event_key).or_default();
        match event.kind {
            Kind::TextNote => counts.replies += 1,
            Kind::Reaction => {
                if event.content.trim() != "-" {
                    counts.likes += 1;
                }
            }
            Kind::Repost => counts.reposts += 1,
            Kind::ZapReceipt => {
                counts.zaps += 1;
                if let Some(amount) = extract_zap_amount(&event) {
                    counts.zap_amount_sats += amount;
                }
            }
            _ => {}
        }
    }
    Ok(counts_map)
}
/// Subscription handle for cleanup
#[derive(Clone, Debug)]
pub struct InteractionStreamHandle {
    pub subscription_id: SubscriptionId,
    /// Task handle for cancellation (Dioxus pattern)
    task: Option<dioxus::dioxus_core::Task>,
}
impl InteractionStreamHandle {
    /// Cancel the background notification handler and unsubscribe
    /// nostr-sdk pattern: graceful shutdown via signal, then cleanup
    pub async fn unsubscribe(mut self) {
        if let Some(task) = self.task.take() {
            task.cancel();
            log::debug!(
                "Cancelled interaction stream task for {:?}", self.subscription_id
            );
        }
        if let Some(client) = crate::stores::nostr_client::get_client() {
            crate::stores::subscription_manager::unsubscribe(
                    &client,
                    &self.subscription_id,
                )
                .await;
        }
    }
}
/// Increment cached counts from streaming update
///
/// Updates the L2 cache with a new interaction event.
/// Returns the updated counts if the event is in cache, None otherwise.
///
/// # Arguments
/// * `event_id` - The event ID being interacted with (hex string)
/// * `kind` - The interaction kind (TextNote, Reaction, Repost, ZapReceipt)
/// * `content` - The event content (used for reactions to detect "-" unlikes)
/// * `is_current_user` - Whether this interaction is from the current user
/// * `zap_amount` - Optional zap amount in satoshis
pub fn increment_cached_counts(
    event_id: &str,
    kind: Kind,
    content: Option<&str>,
    is_current_user: bool,
    zap_amount: Option<u64>,
) -> Option<InteractionCounts> {
    let mut cache = get_counts_cache()
        .lock()
        .unwrap_or_else(|poisoned| {
            log::warn!("Counts cache mutex was poisoned, recovering");
            poisoned.into_inner()
        });
    let cache_ttl = cache.ttl;
    if let Some(cached) = cache.cache.get_mut(event_id) {
        if cached.cached_at.elapsed() > cache_ttl {
            cache.cache.pop(event_id);
            return None;
        }
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
        Some(cached.counts.clone())
    } else {
        None
    }
}
/// Extract the event ID being interacted with from an interaction event
///
/// Only returns an event ID if it's in the set of tracked IDs.
fn extract_referenced_event_for_streaming(
    event: &Event,
    tracked_ids: &HashSet<String>,
) -> Option<String> {
    for tag in event.tags.iter() {
        if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
            let id_hex = event_id.to_hex();
            if tracked_ids.contains(&id_hex) {
                return Some(id_hex);
            }
        }
    }
    None
}
/// Start streaming interactions for a set of events
///
/// Opens a subscription for interaction events (replies, reactions, reposts, zaps)
/// that reference the given event IDs. Uses the handle_notifications() pattern
/// from nostr-sdk for event processing.
///
/// # Arguments
/// * `event_ids` - Vector of event IDs to track interactions for
/// * `interaction_counts` - Signal to update with new counts (Dioxus reactive state)
/// * `post_eose_timeout_secs` - Optional timeout in seconds after EOSE before closing subscription (default: 600)
///
/// # Returns
/// * `Ok(InteractionStreamHandle)` - Handle containing subscription ID for cleanup
/// * `Err(String)` - Error message if subscription fails
///
/// # Deduplication
/// - nostr-sdk automatically deduplicates events (RelayPoolNotification::Event only fires once per event)
/// - Uses `since: Timestamp::now()` to only receive new events
/// - Only updates existing cache entries (no-op if event not in cache)
///
/// # Important: Event Gap Risk
/// Because the filter uses `since: Timestamp::now()`, events that occur between the last fetch
/// and when the subscription starts will be missed. Callers should start streaming before or
/// concurrently with their first fetch to avoid gaps. The nostr-sdk deduplication ensures
/// overlapping fetches don't cause double-counting.
pub async fn stream_interaction_counts(
    event_ids: Vec<EventId>,
    interaction_counts: Signal<HashMap<String, InteractionCounts>>,
    post_eose_timeout_secs: Option<u64>,
) -> Result<InteractionStreamHandle, String> {
    use nostr_relay_pool::relay::ReqExitPolicy;
    use nostr_relay_pool::{RelayStatus as PoolRelayStatus, SubscribeAutoCloseOptions};
    if event_ids.is_empty() {
        return Err("No event IDs to stream".to_string());
    }
    let client = get_client().ok_or("Client not initialized")?;
    let tracked_ids: HashSet<String> = event_ids.iter().map(|id| id.to_hex()).collect();
    let filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Reaction, Kind::Repost, Kind::ZapReceipt])
        .events(event_ids)
        .since(Timestamp::now());
    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 5;
    let connected_urls = loop {
        let relays = client.relays().await;
        let urls: Vec<nostr_sdk::RelayUrl> = relays
            .iter()
            .filter(|(_, r)| r.status() == PoolRelayStatus::Connected)
            .filter_map(|(url, _)| nostr_sdk::RelayUrl::parse(url.as_str()).ok())
            .collect();
        if !urls.is_empty() || attempts >= MAX_ATTEMPTS {
            break urls;
        }
        attempts += 1;
        log::debug!(
            "Waiting for relay connections (attempt {}/{})", attempts, MAX_ATTEMPTS
        );
        gloo_timers::future::TimeoutFuture::new(500).await;
    };
    if connected_urls.is_empty() {
        return Err(
            "No connected relays for interaction streaming after retries".to_string(),
        );
    }
    log::info!(
        "Fast interaction streaming: subscribing to {} connected relays (bypassing gossip)",
        connected_urls.len()
    );
    let timeout = Duration::from_secs(post_eose_timeout_secs.unwrap_or(600));
    let auto_close = SubscribeAutoCloseOptions::default()
        .exit_policy(ReqExitPolicy::WaitDurationAfterEOSE(timeout));
    let subscription_id = client
        .subscribe_to(connected_urls, filter, Some(auto_close))
        .await
        .map(|output| output.val)
        .map_err(|e| format!("Failed to subscribe: {}", e))?;
    log::info!(
        "Started interaction stream subscription {:?} for {} events", subscription_id,
        tracked_ids.len()
    );
    let current_user_pk: Option<nostr_sdk::PublicKey> = SIGNER_INFO
        .read()
        .as_ref()
        .and_then(|info| nostr_sdk::PublicKey::from_hex(&info.public_key).ok());
    let tracked_ids = std::sync::Arc::new(tracked_ids);
    let sub_id = std::sync::Arc::new(subscription_id.clone());
    let task = dioxus::prelude::spawn(async move {
        let client = match get_client() {
            Some(c) => c,
            None => {
                log::error!("Client not available for interaction stream handler");
                return;
            }
        };
        if let Err(e) = client
            .handle_notifications(|notification| {
                let tracked_ids = std::sync::Arc::clone(&tracked_ids);
                let sub_id = std::sync::Arc::clone(&sub_id);
                let mut interaction_counts = interaction_counts;
                async move {
                    if let RelayPoolNotification::Event {
                        subscription_id: event_sub_id,
                        event,
                        ..
                    } = notification {
                        if event_sub_id != *sub_id {
                            return Ok(false);
                        }
                        let referenced_id = match extract_referenced_event_for_streaming(
                            &event,
                            &tracked_ids,
                        ) {
                            Some(id) => id,
                            None => return Ok(false),
                        };
                        let is_current_user = current_user_pk
                            .map(|pk| event.pubkey == pk)
                            .unwrap_or(false);
                        let zap_amount = if event.kind == Kind::ZapReceipt {
                            extract_zap_amount(&event)
                        } else {
                            None
                        };
                        let content = if event.kind == Kind::Reaction {
                            Some(event.content.trim())
                        } else {
                            None
                        };
                        if let Some(updated_counts) = increment_cached_counts(
                            &referenced_id,
                            event.kind,
                            content,
                            is_current_user,
                            zap_amount,
                        ) {
                            interaction_counts
                                .write()
                                .insert(referenced_id.clone(), updated_counts);
                            log::debug!(
                                "Streamed interaction update for {}: kind={}", &
                                referenced_id[..8.min(referenced_id.len())], event.kind
                                .as_u16()
                            );
                        }
                    }
                    Ok(false)
                }
            })
            .await
        {
            log::debug!("Interaction stream handler ended: {}", e);
        }
    });
    Ok(InteractionStreamHandle {
        subscription_id,
        task: Some(task),
    })
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
