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

use dioxus::prelude::ReadableExt;
use lru::LruCache;
use nostr_sdk::{Event, EventId, Filter, Kind, Timestamp, TagStandard};
use nostr_relay_pool::{SyncOptions, SyncDirection};
use crate::stores::nostr_client::get_client;
use crate::stores::signer::SIGNER_INFO;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};
use instant::{Duration, Instant};
use futures::join;

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

    /// Increment a specific count type for an event (for negentropy sync)
    ///
    /// Updates an existing cache entry with new interaction data.
    /// If the event isn't cached, this is a no-op.
    #[allow(dead_code)]
    fn increment(&mut self, event_id: &str, kind: Kind, content: Option<&str>, is_current_user: bool, zap_amount: Option<u64>) {
        if let Some(cached) = self.cache.get_mut(event_id) {
            // Refresh the timestamp since we're updating
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
                            // Note: emoji_url is not available in increment context
                            // This is only used for live updates, full data comes from fetch
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
        // First, check if we have a valid entry
        let needs_create = self.cache.get(event_id)
            .map(|c| !c.is_valid(self.ttl))
            .unwrap_or(true);

        if needs_create {
            self.cache.put(event_id.to_string(), CachedCounts::new(InteractionCounts::default()));
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
    COUNTS_CACHE.get_or_init(|| {
        Mutex::new(CountsCache::new(
            1000,
            Duration::from_secs(300), // 5 minutes
        ))
    })
}

// ============================================================================
// NIP-45 COUNT Support (Phase C)
// ============================================================================

/// Cache for tracking which relays support NIP-45 COUNT
///
/// - `true`: Relay supports COUNT (tested successfully)
/// - `false`: Relay does not support COUNT (timed out or error)
/// - Not present: Unknown, needs testing
static NIP45_SUPPORT: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();

/// Get or initialize the NIP-45 support cache
fn get_nip45_cache() -> &'static Mutex<HashMap<String, bool>> {
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

    let filter = Filter::new()
        .kind(kind)
        .event(event_id.clone());

    // Get connected relays
    let relays = client.relays().await;

    // Try COUNT on relays we know support it, or haven't tested yet
    for (url, relay) in relays.iter() {
        let url_str = url.to_string();

        // Check if we've cached this relay's NIP-45 support
        let support_status = {
            let cache = get_nip45_cache().lock().unwrap();
            cache.get(&url_str).cloned()
        };

        match support_status {
            Some(false) => continue, // Skip relays we know don't support COUNT
            Some(true) | None => {
                // Try COUNT - use short timeout since COUNT should be fast
                let count_timeout = Duration::from_millis(timeout.as_millis().min(2000) as u64);

                match relay.count_events(filter.clone(), count_timeout).await {
                    Ok(count) => {
                        // Cache successful result
                        {
                            let mut cache = get_nip45_cache().lock().unwrap();
                            cache.insert(url_str, true);
                        }
                        log::debug!("COUNT from {}: {} events", url, count);
                        return Some(count);
                    }
                    Err(e) => {
                        // Cache failure for this relay
                        {
                            let mut cache = get_nip45_cache().lock().unwrap();
                            cache.insert(url_str, false);
                        }
                        log::debug!("COUNT failed on {}: {}", url, e);
                        // Try next relay
                    }
                }
            }
        }
    }

    // No relay successfully returned COUNT
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

    // Try COUNT for each interaction type
    // These run in parallel for efficiency
    let (reactions, reposts, replies, zaps) = join!(
        try_count_from_relays(event_id, Kind::Reaction, timeout),
        try_count_from_relays(event_id, Kind::Repost, timeout),
        try_count_from_relays(event_id, Kind::TextNote, timeout),
        try_count_from_relays(event_id, Kind::from(9735), timeout),
    );

    // Use COUNT results if available
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
        // Note: zap_amount_sats requires full event fetch - COUNT doesn't provide this
    }

    // If any COUNT failed, fall back to batch fetch for complete data
    if needs_fallback {
        log::debug!("COUNT incomplete for {}, using full fetch", event_id.to_hex());
        if let Ok(batch_counts) = fetch_interaction_counts_batch(vec![event_id.clone()], timeout).await {
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

    // Phase 2.5: Query local IndexedDB first (instant)
    // This gives us immediate counts from cached interactions while we wait for relay
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

    // Fetch from relays (will also update local database for future queries)
    let relay_events: Vec<Event> = match client.fetch_events(filter, timeout).await {
        Ok(events) => {
            log::info!("Fetched {} interaction events from relays", events.len());
            events.into_iter().collect()
        }
        Err(e) => {
            // If relay fetch fails but we have DB data, continue with what we have
            if !db_events.is_empty() {
                log::warn!("Relay fetch failed but using {} cached events: {}", db_events.len(), e);
                Vec::new()
            } else {
                return Err(format!("Failed to fetch interactions: {}", e));
            }
        }
    };

    // Merge DB and relay events, deduplicating by event ID
    // Relay events take precedence (more recent)
    let mut event_map: HashMap<EventId, Event> = HashMap::new();
    for event in db_events {
        event_map.insert(event.id, event);
    }
    for event in relay_events {
        event_map.insert(event.id, event); // Overwrites if duplicate
    }
    let events: Vec<Event> = event_map.into_values().collect();

    log::info!("Processing {} total interaction events (DB + relay, deduplicated)", events.len());

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

    // Parse current user's pubkey once for efficient comparison (avoids string allocation per event)
    let current_user_pk: Option<nostr_sdk::PublicKey> = SIGNER_INFO
        .read()
        .as_ref()
        .and_then(|info| nostr_sdk::PublicKey::from_hex(&info.public_key).ok());

    // Track which events we've already processed a user reaction for.
    // Since fetch_events() returns events sorted descending by created_at (newest first),
    // we use "first seen wins" - the first reaction we see is the most recent one.
    let mut user_reactions_seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Count interactions
    for event in events {
        // Get the event this interaction is referencing, only if it's one we requested
        let referenced_event_id = match extract_referenced_event(&event, &requested_ids) {
            Some(id) => id,
            None => continue,
        };

        let event_key = referenced_event_id.to_hex();

        // Get or create counts entry (should already exist from initialization)
        let counts = freshly_fetched.entry(event_key.clone()).or_default();

        // Check if this event is from the current user (direct PublicKey comparison)
        let is_current_user = current_user_pk
            .map(|pk| event.pubkey == pk)
            .unwrap_or(false);

        // Increment appropriate counter
        match event.kind {
            Kind::TextNote => counts.replies += 1,
            Kind::Reaction => {
                let content = event.content.trim();
                // Per NIP-25, only count reactions with content != "-" as likes
                if content != "-" {
                    counts.likes += 1;
                }

                // Track current user's reaction (first seen wins - newest reaction)
                // Only process if we haven't already seen a reaction from this user for this event
                if is_current_user && !user_reactions_seen.contains(&event_key) {
                    user_reactions_seen.insert(event_key.clone());
                    if content == "-" {
                        // User unliked - they don't currently like this
                        counts.user_liked = Some(false);
                        counts.user_reaction = None;
                        counts.user_reaction_url = None;
                    } else {
                        // User reacted positively
                        counts.user_liked = Some(true);
                        counts.user_reaction = Some(content.to_string());

                        // Check for NIP-30 custom emoji - extract URL from emoji tag
                        if content.starts_with(':') && content.ends_with(':') && content.len() > 2 {
                            let shortcode = &content[1..content.len()-1];
                            // Find emoji tag with matching shortcode
                            let emoji_url = event.tags.iter().find_map(|tag| {
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
            },
            Kind::Repost => counts.reposts += 1,
            Kind::ZapReceipt => {
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

    // Build filter for interaction events
    let filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,   // kind 1 - replies
            Kind::Reaction,   // kind 7 - likes
            Kind::Repost,     // kind 6 - reposts
            Kind::from(9735), // kind 9735 - zaps
        ])
        .events(event_ids.clone());

    // Configure sync options - we only want to download new events
    let sync_opts = SyncOptions::default()
        .direction(SyncDirection::Down)
        .initial_timeout(timeout);

    // Attempt negentropy sync
    let sync_result = client.sync(filter.clone(), &sync_opts).await;

    match sync_result {
        Ok(output) => {
            let reconciliation = output.val;
            let new_event_count = reconciliation.received.len();

            if new_event_count == 0 {
                log::info!("Negentropy sync: no new interaction events found");
                // Return current cached counts
                let mut cache = get_counts_cache().lock().unwrap();
                return Ok(cache.get_batch(&event_ids));
            }

            log::info!("Negentropy sync: {} new interaction events to process", new_event_count);

            // Fetch the newly received events from database
            // (they were saved during sync)
            let mut new_events = Vec::new();
            for event_id in &reconciliation.received {
                if let Ok(Some(event)) = client.database().event_by_id(event_id).await {
                    new_events.push(event);
                }
            }

            // Get existing cached counts
            let mut result = {
                let mut cache = get_counts_cache().lock().unwrap();
                cache.get_batch(&event_ids)
            };

            // Initialize any missing entries
            for event_id in &event_ids {
                let hex = event_id.to_hex();
                result.entry(hex).or_insert_with(InteractionCounts::default);
            }

            // Parse current user's pubkey for reaction tracking
            let current_user_pk: Option<nostr_sdk::PublicKey> = SIGNER_INFO
                .read()
                .as_ref()
                .and_then(|info| nostr_sdk::PublicKey::from_hex(&info.public_key).ok());

            // Build set of requested event IDs
            let requested_ids: std::collections::HashSet<String> = event_ids.iter()
                .map(|id| id.to_hex())
                .collect();

            // Process new events and update counts
            for event in new_events {
                let referenced_event_id = match extract_referenced_event(&event, &requested_ids) {
                    Some(id) => id,
                    None => continue,
                };

                let event_key = referenced_event_id.to_hex();
                let counts = result.entry(event_key.clone()).or_default();
                let is_current_user = current_user_pk
                    .map(|pk| event.pubkey == pk)
                    .unwrap_or(false);

                // Increment appropriate counter
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

                                // Check for NIP-30 custom emoji - extract URL from emoji tag
                                if content.starts_with(':') && content.ends_with(':') && content.len() > 2 {
                                    let shortcode = &content[1..content.len()-1];
                                    let emoji_url = event.tags.iter().find_map(|tag| {
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

            // Update cache with new counts
            {
                let mut cache = get_counts_cache().lock().unwrap();
                cache.insert_batch(result.clone());
            }

            log::info!("Negentropy sync complete: updated {} interaction counts", result.len());
            Ok(result)
        }
        Err(e) => {
            // Negentropy not supported or failed - fall back to full fetch
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
    // Look for 'bolt11' tag first (use as_slice for zero-copy access)
    if let Some(bolt11_tag) = event.tags.iter().find(|tag| {
        tag.as_slice().first().map(|k| k.as_str() == "bolt11").unwrap_or(false)
    }) {
        if let Some(bolt11) = bolt11_tag.as_slice().get(1) {
            // Parse bolt11 invoice to extract amount
            // For now, try to extract from description tag as fallback
            // Full bolt11 parsing would require additional dependency
            if let Some(amount) = parse_bolt11_amount(bolt11.as_str()) {
                return Some(amount);
            }
        }
    }

    // Fallback: check description tag for amount (use as_slice for zero-copy access)
    if let Some(description_tag) = event.tags.iter().find(|tag| {
        tag.as_slice().first().map(|k| k.as_str() == "description").unwrap_or(false)
    }) {
        if let Some(desc) = description_tag.as_slice().get(1) {
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
    // Try to parse the description as JSON (zap request) to extract amount
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(description) {
        // NIP-57: Amount is in the tags array as ["amount", "millisats"]
        if let Some(tags) = json.get("tags").and_then(|t| t.as_array()) {
            for tag in tags {
                if let Some(tag_vals) = tag.as_array() {
                    if tag_vals.first().and_then(|v| v.as_str()) == Some("amount") {
                        if let Some(amount_str) = tag_vals.get(1).and_then(|v| v.as_str()) {
                            // Amount is in millisats, convert to sats
                            if let Ok(millisats) = amount_str.parse::<u64>() {
                                return Some(millisats / 1000);
                            }
                        }
                    }
                }
            }
        }
        // Fallback: check for amount at root level (some implementations)
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
