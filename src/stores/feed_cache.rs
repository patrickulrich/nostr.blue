//! # Feed Cache Service
//!
//! High-level API for caching and retrieving feed items.
//!
//! ## Usage Pattern
//!
//! ```ignore
//! // On page load:
//! // 1. Load cached feed instantly
//! let cached = feed_cache::load_cached_feed(&key, 100).await?;
//! feed_state.set(DataState::Loaded(cached.clone()));
//!
//! // 2. Stream from network, merge progressively
//! let network_items = load_from_network().await;
//! let merged = feed_cache::merge_feed_items(cached, network_items);
//! feed_state.set(DataState::Loaded(merged.clone()));
//!
//! // 3. Persist to cache
//! feed_cache::store_feed_items(&key, &merged).await?;
//! feed_cache::run_eviction_if_needed().await?;
//! ```

use crate::utils::FeedItem;
use std::collections::HashSet;

#[cfg(target_arch = "wasm32")]
use super::feed_cache_db::{CachedFeedItem, CachedFeedItemType, FeedCacheMetadata, LruEntry, FeedCacheDb};

#[cfg(target_arch = "wasm32")]
use std::sync::OnceLock;

#[cfg(target_arch = "wasm32")]
use nostr_sdk::Event;

// ============================================================================
// Cache Configuration
// ============================================================================

/// Maximum items per feed type
pub const MAX_ITEMS_PER_FEED: usize = 500;

/// Maximum total items across all feeds
pub const MAX_TOTAL_ITEMS: usize = 5000;

/// Number of items to evict when over limit
const EVICTION_BATCH_SIZE: usize = 100;

/// Minimum age (in seconds) before item can be evicted
const MIN_AGE_BEFORE_EVICTION_SECS: u64 = 3600; // 1 hour

// ============================================================================
// Feed Cache Key
// ============================================================================

/// Identifies a specific feed for caching
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FeedCacheKey {
    /// Home feed - posts from followed users (top-level only)
    Following { pubkey: String },
    /// Home feed with replies
    FollowingWithReplies { pubkey: String },
    /// Global feed
    Global,
    /// Photos feed from followed users
    Photos { pubkey: String },
    /// Photos global feed
    PhotosGlobal,
    /// Videos feed from followed users
    Videos { pubkey: String },
    /// Videos global feed
    VideosGlobal,
    /// Articles feed from followed users
    Articles { pubkey: String },
    /// Articles global feed
    ArticlesGlobal,
    /// People list feed
    PeopleList { pubkey: String, list_id: String },
}

impl FeedCacheKey {
    /// Convert to string key for IndexedDB storage
    pub fn to_string_key(&self) -> String {
        match self {
            FeedCacheKey::Following { pubkey } => format!("following:{}", pubkey),
            FeedCacheKey::FollowingWithReplies { pubkey } => format!("following_replies:{}", pubkey),
            FeedCacheKey::Global => "global".to_string(),
            FeedCacheKey::Photos { pubkey } => format!("photos:{}", pubkey),
            FeedCacheKey::PhotosGlobal => "photos_global".to_string(),
            FeedCacheKey::Videos { pubkey } => format!("videos:{}", pubkey),
            FeedCacheKey::VideosGlobal => "videos_global".to_string(),
            FeedCacheKey::Articles { pubkey } => format!("articles:{}", pubkey),
            FeedCacheKey::ArticlesGlobal => "articles_global".to_string(),
            FeedCacheKey::PeopleList { pubkey, list_id } => format!("list:{}:{}", pubkey, list_id),
        }
    }
}

// ============================================================================
// Native stub implementation
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
pub async fn init_feed_cache() -> Result<(), String> {
    log::warn!("Feed cache not available on native targets");
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn load_cached_feed(_key: &FeedCacheKey, _limit: usize) -> Result<Vec<FeedItem>, String> {
    // Return empty on native - cache not available
    Ok(Vec::new())
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn store_feed_items(_key: &FeedCacheKey, _items: &[FeedItem]) -> Result<(), String> {
    // No-op on native
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn merge_feed_items(cached: Vec<FeedItem>, network: Vec<FeedItem>) -> Vec<FeedItem> {
    // Simple merge - deduplicate by event ID and sort by timestamp
    let mut seen = HashSet::new();
    let mut merged = Vec::new();

    // Add network items first (fresher)
    for item in network {
        let id = item.event().id.to_string();
        if seen.insert(id) {
            merged.push(item);
        }
    }

    // Add cached items that aren't duplicates
    for item in cached {
        let id = item.event().id.to_string();
        if seen.insert(id) {
            merged.push(item);
        }
    }

    // Sort by timestamp descending
    merged.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));

    merged
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn run_eviction_if_needed() -> Result<usize, String> {
    // No-op on native
    Ok(0)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn touch_items(_event_ids: &[String]) -> Result<(), String> {
    // No-op on native
    Ok(())
}

// ============================================================================
// WASM implementation
// ============================================================================

#[cfg(target_arch = "wasm32")]
static FEED_CACHE_DB: OnceLock<FeedCacheDb> = OnceLock::new();

/// Initialize the feed cache database
/// Should be called once at application startup
#[cfg(target_arch = "wasm32")]
pub async fn init_feed_cache() -> Result<(), String> {
    if FEED_CACHE_DB.get().is_some() {
        return Ok(()); // Already initialized
    }

    let db = FeedCacheDb::new().await?;
    FEED_CACHE_DB.set(db).map_err(|_| "Feed cache already initialized".to_string())?;

    log::info!("Feed cache initialized");
    Ok(())
}

/// Get the feed cache database instance
#[cfg(target_arch = "wasm32")]
fn get_db() -> Option<&'static FeedCacheDb> {
    FEED_CACHE_DB.get()
}

/// Load cached feed items for instant display
#[cfg(target_arch = "wasm32")]
pub async fn load_cached_feed(key: &FeedCacheKey, limit: usize) -> Result<Vec<FeedItem>, String> {
    let db = match get_db() {
        Some(db) => db,
        None => return Ok(Vec::new()), // Cache not initialized
    };

    let feed_key = key.to_string_key();

    // Get feed metadata
    let metadata = match db.get_feed_metadata(&feed_key).await? {
        Some(m) => m,
        None => return Ok(Vec::new()), // No cached data
    };

    // Get event IDs up to limit
    let event_ids: Vec<String> = metadata.event_ids
        .into_iter()
        .take(limit)
        .collect();

    if event_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Fetch cached items
    let cached_items = db.get_feed_items_by_ids(&event_ids).await?;

    // Convert to FeedItems
    let mut feed_items = Vec::new();
    for cached in cached_items {
        if let Ok(event) = serde_json::from_str::<Event>(&cached.event_json) {
            let feed_item = match cached.item_type {
                CachedFeedItemType::OriginalPost => FeedItem::OriginalPost(event),
                CachedFeedItemType::Repost { reposted_by, repost_timestamp } => {
                    // For reposts, reconstruct the FeedItem::Repost variant
                    use nostr_sdk::{PublicKey, Timestamp};
                    let pubkey = PublicKey::parse(&reposted_by).unwrap_or(event.pubkey);
                    FeedItem::Repost {
                        original: event,
                        reposted_by: pubkey,
                        repost_timestamp: Timestamp::from(repost_timestamp),
                    }
                }
            };
            feed_items.push(feed_item);
        }
    }

    // Sort by timestamp descending
    feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));

    // Update LRU timestamps for accessed items
    let now = current_timestamp();
    for item in &feed_items {
        let event_id = item.event().id.to_string();
        let _ = db.put_lru_entry(&event_id, &LruEntry { last_access: now }).await;
    }

    log::info!("Loaded {} items from cache for {}", feed_items.len(), feed_key);

    Ok(feed_items)
}

/// Store feed items in cache
#[cfg(target_arch = "wasm32")]
pub async fn store_feed_items(key: &FeedCacheKey, items: &[FeedItem]) -> Result<(), String> {
    let db = match get_db() {
        Some(db) => db,
        None => return Ok(()), // Cache not initialized
    };

    if items.is_empty() {
        return Ok(());
    }

    let feed_key = key.to_string_key();
    let now = current_timestamp();

    // Store each item
    let mut event_ids = Vec::new();
    let mut newest_ts: Option<u64> = None;
    let mut oldest_ts: Option<u64> = None;

    for item in items.iter().take(MAX_ITEMS_PER_FEED) {
        let event = item.event();
        let event_id = event.id.to_string();
        let sort_ts = item.sort_timestamp().as_secs();

        // Track timestamps
        newest_ts = Some(newest_ts.map_or(sort_ts, |t| t.max(sort_ts)));
        oldest_ts = Some(oldest_ts.map_or(sort_ts, |t| t.min(sort_ts)));

        // Create cached item
        let cached_item = CachedFeedItem {
            event_json: serde_json::to_string(event).map_err(|e| format!("Serialize error: {}", e))?,
            item_type: match item {
                FeedItem::OriginalPost(_) => CachedFeedItemType::OriginalPost,
                FeedItem::Repost { reposted_by, repost_timestamp, .. } => CachedFeedItemType::Repost {
                    reposted_by: reposted_by.to_string(),
                    repost_timestamp: repost_timestamp.as_secs(),
                },
            },
            sort_timestamp: sort_ts,
            cached_at: now,
            feed_keys: vec![feed_key.clone()],
        };

        // Store item
        db.put_feed_item(&event_id, &cached_item).await?;

        // Update LRU
        db.put_lru_entry(&event_id, &LruEntry { last_access: now }).await?;

        event_ids.push(event_id);
    }

    // Store metadata
    let metadata = FeedCacheMetadata {
        feed_key: feed_key.clone(),
        event_ids,
        newest_timestamp: newest_ts,
        oldest_timestamp: oldest_ts,
        last_sync: now,
    };

    db.put_feed_metadata(&feed_key, &metadata).await?;

    log::info!("Stored {} items to cache for {}", items.len().min(MAX_ITEMS_PER_FEED), feed_key);

    Ok(())
}

/// Merge cached items with network items, deduplicating by event ID
#[cfg(target_arch = "wasm32")]
pub fn merge_feed_items(cached: Vec<FeedItem>, network: Vec<FeedItem>) -> Vec<FeedItem> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();

    // Add network items first (fresher data takes precedence)
    for item in network {
        let id = item.event().id.to_string();
        if seen.insert(id) {
            merged.push(item);
        }
    }

    // Add cached items that aren't duplicates
    for item in cached {
        let id = item.event().id.to_string();
        if seen.insert(id) {
            merged.push(item);
        }
    }

    // Sort by timestamp descending
    merged.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));

    merged
}

/// Run LRU eviction if cache exceeds limits
#[cfg(target_arch = "wasm32")]
pub async fn run_eviction_if_needed() -> Result<usize, String> {
    let db = match get_db() {
        Some(db) => db,
        None => return Ok(0),
    };

    let total_items = db.count_feed_items().await?;

    if (total_items as usize) <= MAX_TOTAL_ITEMS {
        return Ok(0); // Under limit
    }

    log::info!("Feed cache has {} items, running eviction", total_items);

    // Get all LRU entries
    let mut lru_entries = db.get_all_lru_entries().await?;

    // Sort by last_access ascending (oldest first)
    lru_entries.sort_by_key(|(_, entry)| entry.last_access);

    // Don't evict recently accessed items
    let now = current_timestamp();
    let min_age = now.saturating_sub(MIN_AGE_BEFORE_EVICTION_SECS);

    let eviction_candidates: Vec<_> = lru_entries
        .into_iter()
        .filter(|(_, entry)| entry.last_access < min_age)
        .take(EVICTION_BATCH_SIZE)
        .collect();

    let mut evicted = 0;
    for (event_id, _) in eviction_candidates {
        // Delete from all stores
        db.delete_feed_item(&event_id).await?;
        db.delete_lru_entry(&event_id).await?;
        evicted += 1;
    }

    log::info!("Evicted {} items from feed cache", evicted);

    Ok(evicted)
}

/// Update LRU timestamps for displayed items
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)] // Part of cache API, will be used for LRU optimization
pub async fn touch_items(event_ids: &[String]) -> Result<(), String> {
    let db = match get_db() {
        Some(db) => db,
        None => return Ok(()),
    };

    let now = current_timestamp();

    for event_id in event_ids {
        let _ = db.put_lru_entry(event_id, &LruEntry { last_access: now }).await;
    }

    Ok(())
}

// ============================================================================
// Helper functions
// ============================================================================

#[cfg(target_arch = "wasm32")]
fn current_timestamp() -> u64 {
    use web_sys::js_sys::Date;
    (Date::now() / 1000.0) as u64
}

