/// Event interaction aggregation service
///
/// Provides batch fetching of interaction counts (replies, likes, reposts, zaps)
/// for multiple events in a single query. This dramatically reduces database
/// queries compared to fetching counts per-event.
///
/// # Performance Impact
/// - Before: N queries (one per event in feed)
/// - After: 1 query (batched for all events)
/// - Example: 100 notes → 99% reduction in queries (100 → 1)
///
/// # L2 Caching (Phase 3.5)
/// Implements in-memory LRU cache for computed interaction counts:
/// - Cache size: 1000 events
/// - TTL: 5 minutes per entry
/// - Automatic eviction of stale/excess entries
/// - Reduces redundant database queries for recently-viewed events

use lru::LruCache;
use nostr_sdk::{Event, EventId, Filter, Kind, Timestamp, TagStandard};
use crate::stores::nostr_client::get_client;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};
use instant::{Duration, Instant};

/// Aggregated interaction counts for a single event
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InteractionCounts {
    pub replies: usize,
    pub likes: usize,
    pub reposts: usize,
    pub zaps: usize,
    pub zap_amount_sats: u64,
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
            // Entry expired, will be overwritten on next insert
        }
        None
    }

    /// Cache counts for an event
    fn insert(&mut self, event_id: String, counts: InteractionCounts) {
        self.cache.put(event_id, CachedCounts::new(counts));
    }

    /// Get multiple counts from cache, returning only valid entries
    fn get_batch(&mut self, event_ids: &[EventId]) -> HashMap<String, InteractionCounts> {
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
}

/// Global L2 cache for interaction counts
///
/// Cache configuration:
/// - Capacity: 1000 events (enough for ~10 full feeds)
/// - TTL: 5 minutes (balance freshness vs performance)
static COUNTS_CACHE: OnceLock<Mutex<CountsCache>> = OnceLock::new();

/// Get or initialize the counts cache
fn get_counts_cache() -> &'static Mutex<CountsCache> {
    COUNTS_CACHE.get_or_init(|| {
        Mutex::new(CountsCache::new(
            1000,
            Duration::from_secs(300), // 5 minutes
        ))
    })
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

    // Phase 3.5: Check L2 cache first
    let (cached_counts, cache_hits, uncached_ids) = {
        let mut cache = get_counts_cache().lock().unwrap();
        let cached_counts = cache.get_batch(&event_ids);
        let cache_hits = cached_counts.len();

        // Identify cache misses - events we need to fetch from database
        let uncached_ids: Vec<EventId> = event_ids
            .iter()
            .filter(|id| !cached_counts.contains_key(&id.to_hex()))
            .cloned()
            .collect();

        (cached_counts, cache_hits, uncached_ids)
        // Cache lock is dropped here
    };

    log::info!(
        "Batch fetching interaction counts for {} events ({} cache hits, {} cache misses)",
        event_ids.len(),
        cache_hits,
        uncached_ids.len()
    );

    // If all counts are cached, return early
    if uncached_ids.is_empty() {
        log::info!("All counts served from cache!");
        return Ok(cached_counts);
    }

    let client = get_client().ok_or("Client not initialized")?;

    // Create filter for ONLY uncached events (cache-aware query)
    // Cap the limit to avoid exceeding relay max limits
    const MAX_RELAY_LIMIT: usize = 5000;
    let requested_limit = uncached_ids.len() * 100;
    let capped_limit = requested_limit.min(MAX_RELAY_LIMIT);

    let filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,   // kind 1 - replies
            Kind::Reaction,   // kind 7 - likes
            Kind::Repost,     // kind 6 - reposts
            Kind::from(9735), // kind 9735 - zaps
        ])
        .events(uncached_ids.clone())
        .limit(capped_limit); // Capped to avoid relay limit issues

    // Fetch all interactions in one query
    let events = client
        .fetch_events(filter, timeout)
        .await
        .map_err(|e| format!("Failed to fetch interactions: {}", e))?;

    log::info!("Fetched {} total interaction events", events.len());

    // Aggregate counts by event_id for uncached events only
    let mut freshly_fetched: HashMap<String, InteractionCounts> = HashMap::new();

    // Build set of requested event IDs for filtering
    let requested_ids: std::collections::HashSet<String> = uncached_ids.iter()
        .map(|id| id.to_hex())
        .collect();

    // Initialize uncached event IDs with zero counts
    for event_id in &uncached_ids {
        freshly_fetched.insert(event_id.to_hex(), InteractionCounts::default());
    }

    // Count interactions
    for event in events {
        // Get the event this interaction is referencing, only if it's one we requested
        let referenced_event_id = match extract_referenced_event(&event, &requested_ids) {
            Some(id) => id,
            None => continue,
        };

        let event_key = referenced_event_id.to_hex();

        // Get or create counts entry (should already exist from initialization)
        let counts = freshly_fetched.entry(event_key).or_default();

        // Increment appropriate counter
        match event.kind {
            Kind::TextNote => counts.replies += 1,
            Kind::Reaction => {
                // Per NIP-25, only count reactions with content != "-" as likes
                if event.content.trim() != "-" {
                    counts.likes += 1;
                }
            },
            Kind::Repost => counts.reposts += 1,
            Kind::Custom(9735) => {
                counts.zaps += 1;
                // Extract zap amount from bolt11 tag
                if let Some(amount) = extract_zap_amount(&event) {
                    counts.zap_amount_sats += amount;
                }
            }
            _ => {}
        }
    }

    // Update L2 cache with freshly fetched counts
    {
        let mut cache = get_counts_cache().lock().unwrap();
        cache.insert_batch(freshly_fetched.clone());
        // Cache lock is dropped here
    }

    // Combine cached and freshly fetched results
    let mut final_counts = cached_counts;
    final_counts.extend(freshly_fetched);

    log::info!(
        "Returning {} interaction counts ({} from cache, {} freshly fetched)",
        final_counts.len(),
        cache_hits,
        uncached_ids.len()
    );

    Ok(final_counts)
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
        let mut cache = get_counts_cache().lock().unwrap();
        cache.invalidate(event_id);
    }
    log::debug!("Invalidated interaction counts cache for {}", event_id);
}

/// Invalidate cached counts for multiple events at once
#[allow(dead_code)]
pub fn invalidate_interaction_counts_batch(event_ids: &[String]) {
    {
        let mut cache = get_counts_cache().lock().unwrap();
        for event_id in event_ids {
            cache.invalidate(event_id);
        }
    }
    log::debug!("Invalidated interaction counts cache for {} events", event_ids.len());
}

/// Extract the event ID being referenced by an interaction event
/// Only returns the event ID if it matches one of the requested IDs
/// If requested_ids is empty, returns the first 'e' tag found (for trending/all events)
fn extract_referenced_event(event: &Event, requested_ids: &std::collections::HashSet<String>) -> Option<EventId> {
    // Check for 'e' tags (most interactions use this)
    for tag in event.tags.iter() {
        if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
            // If no filter set (empty), return first tag
            if requested_ids.is_empty() {
                return Some(*event_id);
            }
            // Only return if this event ID was requested
            if requested_ids.contains(&event_id.to_hex()) {
                return Some(*event_id);
            }
        }
    }
    None
}

/// Extract zap amount in satoshis from a zap event (kind 9735)
fn extract_zap_amount(event: &Event) -> Option<u64> {
    // Look for 'bolt11' tag first
    if let Some(bolt11_tag) = event.tags.iter().find(|tag| {
        let vec = (*tag).clone().to_vec();
        vec.get(0).map(|k| k.as_str() == "bolt11").unwrap_or(false)
    }) {
        let tag_vec = bolt11_tag.clone().to_vec();
        if let Some(bolt11) = tag_vec.get(1) {
            // Parse bolt11 invoice to extract amount
            // For now, try to extract from description tag as fallback
            // Full bolt11 parsing would require additional dependency
            if let Some(amount) = parse_bolt11_amount(bolt11.as_str()) {
                return Some(amount);
            }
        }
    }

    // Fallback: check description tag for amount
    if let Some(description_tag) = event.tags.iter().find(|tag| {
        let vec = (*tag).clone().to_vec();
        vec.get(0).map(|k| k.as_str() == "description").unwrap_or(false)
    }) {
        let tag_vec = description_tag.clone().to_vec();
        if let Some(desc) = tag_vec.get(1) {
            // Description contains the zap request which has amount
            return parse_amount_from_description(desc.as_str());
        }
    }

    None
}

/// Parse amount from bolt11 invoice string
/// This is a simplified parser - a full implementation would use a bolt11 crate
fn parse_bolt11_amount(_bolt11: &str) -> Option<u64> {
    // TODO: Implement proper bolt11 parsing
    // For now, return None and rely on description parsing
    None
}

/// Parse amount from zap request description
fn parse_amount_from_description(description: &str) -> Option<u64> {
    // Try to parse the description as JSON to extract amount
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(description) {
        if let Some(amount) = json.get("amount") {
            if let Some(amount_str) = amount.as_str() {
                // Amount in description is in millisats
                if let Ok(millisats) = amount_str.parse::<u64>() {
                    return Some(millisats / 1000); // Convert to sats
                }
            } else if let Some(amount_num) = amount.as_u64() {
                return Some(amount_num / 1000); // Convert to sats
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
        .kinds(vec![
            Kind::TextNote,
            Kind::Reaction,
            Kind::Repost,
            Kind::from(9735),
        ])
        .since(since)
        .limit(limit);

    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch trending interactions: {}", e))?;

    let mut counts_map: HashMap<String, InteractionCounts> = HashMap::new();
    // Use empty set to accept all event IDs (trending mode)
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
                // Per NIP-25, only count reactions with content != "-" as likes
                if event.content.trim() != "-" {
                    counts.likes += 1;
                }
            },
            Kind::Repost => counts.reposts += 1,
            Kind::Custom(9735) => {
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
        assert_eq!(amount, Some(5)); // 5000 millisats = 5 sats
    }
}
