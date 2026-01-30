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
use super::feed_cache_db::{
    CachedFeedItem, CachedFeedItemType, FeedCacheDb, FeedCacheMetadata, LruEntry,
};
#[cfg(target_arch = "wasm32")]
use std::sync::OnceLock;
#[cfg(target_arch = "wasm32")]
use nostr_sdk::Event;
/// Maximum items per feed type
#[cfg(target_arch = "wasm32")]
pub const MAX_ITEMS_PER_FEED: usize = 500;
/// Maximum total items across all feeds
#[cfg(target_arch = "wasm32")]
pub const MAX_TOTAL_ITEMS: usize = 5000;
/// Number of items to evict when over limit
#[cfg(target_arch = "wasm32")]
const EVICTION_BATCH_SIZE: usize = 100;
/// Minimum age (in seconds) before item can be evicted
#[cfg(target_arch = "wasm32")]
const MIN_AGE_BEFORE_EVICTION_SECS: u64 = 3600;
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
    #[cfg(target_arch = "wasm32")]
    pub fn to_string_key(&self) -> String {
        match self {
            FeedCacheKey::Following { pubkey } => format!("following:{}", pubkey),
            FeedCacheKey::FollowingWithReplies { pubkey } => {
                format!("following_replies:{}", pubkey)
            }
            FeedCacheKey::Global => "global".to_string(),
            FeedCacheKey::Photos { pubkey } => format!("photos:{}", pubkey),
            FeedCacheKey::PhotosGlobal => "photos_global".to_string(),
            FeedCacheKey::Videos { pubkey } => format!("videos:{}", pubkey),
            FeedCacheKey::VideosGlobal => "videos_global".to_string(),
            FeedCacheKey::Articles { pubkey } => format!("articles:{}", pubkey),
            FeedCacheKey::ArticlesGlobal => "articles_global".to_string(),
            FeedCacheKey::PeopleList { pubkey, list_id } => {
                format!("list:{}:{}", pubkey, list_id)
            }
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub async fn init_feed_cache() -> Result<(), String> {
    log::warn!("Feed cache not available on native targets");
    Ok(())
}
#[cfg(not(target_arch = "wasm32"))]
pub async fn load_cached_feed(
    _key: &FeedCacheKey,
    _limit: usize,
) -> Result<Vec<FeedItem>, String> {
    Ok(Vec::new())
}
#[cfg(not(target_arch = "wasm32"))]
pub async fn store_feed_items(
    _key: &FeedCacheKey,
    _items: &[FeedItem],
) -> Result<(), String> {
    Ok(())
}
#[cfg(not(target_arch = "wasm32"))]
pub fn merge_feed_items(cached: Vec<FeedItem>, network: Vec<FeedItem>) -> Vec<FeedItem> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    for item in network {
        let id = item.event().id.to_string();
        if seen.insert(id) {
            merged.push(item);
        }
    }
    for item in cached {
        let id = item.event().id.to_string();
        if seen.insert(id) {
            merged.push(item);
        }
    }
    merged.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
    merged
}
#[cfg(not(target_arch = "wasm32"))]
pub async fn run_eviction_if_needed() -> Result<usize, String> {
    Ok(0)
}
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub async fn touch_items(_event_ids: &[String]) -> Result<(), String> {
    Ok(())
}
#[cfg(target_arch = "wasm32")]
static FEED_CACHE_DB: OnceLock<FeedCacheDb> = OnceLock::new();
/// Initialize the feed cache database
/// Should be called once at application startup
#[cfg(target_arch = "wasm32")]
pub async fn init_feed_cache() -> Result<(), String> {
    if FEED_CACHE_DB.get().is_some() {
        return Ok(());
    }
    let db = FeedCacheDb::new().await?;
    let _ = FEED_CACHE_DB.set(db);
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
pub async fn load_cached_feed(
    key: &FeedCacheKey,
    limit: usize,
) -> Result<Vec<FeedItem>, String> {
    let db = match get_db() {
        Some(db) => db,
        None => return Ok(Vec::new()),
    };
    let feed_key = key.to_string_key();
    let metadata = match db.get_feed_metadata(&feed_key).await? {
        Some(m) => m,
        None => return Ok(Vec::new()),
    };
    let event_ids: Vec<String> = metadata.event_ids.into_iter().take(limit).collect();
    if event_ids.is_empty() {
        return Ok(Vec::new());
    }
    let cached_result = db.get_feed_items_by_ids(&event_ids).await?;
    if cached_result.failed_count > 0 {
        log::warn!(
            "Feed cache: {} items failed deserialization (keys: {:?})", cached_result
            .failed_count, cached_result.failed_keys
        );
        if let Ok(Some(mut updated_metadata)) = db.get_feed_metadata(&feed_key).await {
            let failed_set: HashSet<&String> = cached_result
                .failed_keys
                .iter()
                .collect();
            updated_metadata.event_ids.retain(|id| !failed_set.contains(id));
            if let Err(e) = db.put_feed_metadata(&feed_key, &updated_metadata).await {
                log::error!("Failed to persist pruned feed metadata: {}", e);
            }
        }
    }
    let mut feed_items = Vec::new();
    for cached in cached_result.items {
        let event = match serde_json::from_str::<Event>(&cached.event_json) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let feed_item = match cached.item_type {
            CachedFeedItemType::OriginalPost => FeedItem::OriginalPost(event),
            CachedFeedItemType::Repost { reposted_by, repost_timestamp } => {
                use nostr_sdk::{PublicKey, Timestamp};
                match PublicKey::parse(&reposted_by) {
                    Ok(pubkey) => {
                        FeedItem::Repost {
                            original: event,
                            reposted_by: pubkey,
                            repost_timestamp: Timestamp::from(repost_timestamp),
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Invalid reposted_by pubkey '{}': {}, skipping", reposted_by,
                            e
                        );
                        continue;
                    }
                }
            }
        };
        feed_items.push(feed_item);
    }
    feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
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
pub async fn store_feed_items(
    key: &FeedCacheKey,
    items: &[FeedItem],
) -> Result<(), String> {
    let db = match get_db() {
        Some(db) => db,
        None => return Ok(()),
    };
    if items.is_empty() {
        return Ok(());
    }
    let feed_key = key.to_string_key();
    let now = current_timestamp();
    let mut event_ids = Vec::new();
    let mut newest_ts: Option<u64> = None;
    let mut oldest_ts: Option<u64> = None;
    for item in items.iter().take(MAX_ITEMS_PER_FEED) {
        let event = item.event();
        let event_id = event.id.to_string();
        let sort_ts = item.sort_timestamp().as_secs();
        newest_ts = Some(newest_ts.map_or(sort_ts, |t| t.max(sort_ts)));
        oldest_ts = Some(oldest_ts.map_or(sort_ts, |t| t.min(sort_ts)));
        let mut feed_keys = vec![feed_key.clone()];
        if let Ok(Some(existing)) = db.get_feed_item(&event_id).await {
            for key in existing.feed_keys {
                if !feed_keys.contains(&key) {
                    feed_keys.push(key);
                }
            }
        }
        let cached_item = CachedFeedItem {
            event_json: serde_json::to_string(event)
                .map_err(|e| format!("Serialize error: {}", e))?,
            item_type: match item {
                FeedItem::OriginalPost(_) => CachedFeedItemType::OriginalPost,
                FeedItem::Repost { reposted_by, repost_timestamp, .. } => {
                    CachedFeedItemType::Repost {
                        reposted_by: reposted_by.to_string(),
                        repost_timestamp: repost_timestamp.as_secs(),
                    }
                }
            },
            sort_timestamp: sort_ts,
            cached_at: now,
            feed_keys,
        };
        db.put_feed_item(&event_id, &cached_item).await?;
        db.put_lru_entry(&event_id, &LruEntry { last_access: now }).await?;
        event_ids.push(event_id);
    }
    let metadata = FeedCacheMetadata {
        feed_key: feed_key.clone(),
        event_ids,
        newest_timestamp: newest_ts,
        oldest_timestamp: oldest_ts,
        last_sync: now,
    };
    db.put_feed_metadata(&feed_key, &metadata).await?;
    log::info!(
        "Stored {} items to cache for {}", items.len().min(MAX_ITEMS_PER_FEED), feed_key
    );
    Ok(())
}
/// Merge cached items with network items, deduplicating by event ID
#[cfg(target_arch = "wasm32")]
pub fn merge_feed_items(cached: Vec<FeedItem>, network: Vec<FeedItem>) -> Vec<FeedItem> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    for item in network {
        let id = item.event().id.to_string();
        if seen.insert(id) {
            merged.push(item);
        }
    }
    for item in cached {
        let id = item.event().id.to_string();
        if seen.insert(id) {
            merged.push(item);
        }
    }
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
        return Ok(0);
    }
    log::info!("Feed cache has {} items, running eviction", total_items);
    let mut lru_entries = db.get_all_lru_entries().await?;
    lru_entries.sort_by_key(|(_, entry)| entry.last_access);
    let now = current_timestamp();
    let min_age = now.saturating_sub(MIN_AGE_BEFORE_EVICTION_SECS);
    let eviction_candidates: Vec<_> = lru_entries
        .iter()
        .filter(|(_, entry)| entry.last_access < min_age)
        .take(EVICTION_BATCH_SIZE)
        .cloned()
        .collect();
    let eviction_candidates = if eviction_candidates.is_empty()
        && total_items > MAX_TOTAL_ITEMS as u32
    {
        log::warn!(
            "All items are recent but over hard cap, evicting oldest {} items",
            EVICTION_BATCH_SIZE
        );
        lru_entries.iter().take(EVICTION_BATCH_SIZE).cloned().collect()
    } else {
        eviction_candidates
    };
    let mut evicted = 0;
    let mut evicted_ids: Vec<String> = Vec::new();
    for (event_id, _) in eviction_candidates {
        db.delete_feed_item(&event_id).await?;
        db.delete_lru_entry(&event_id).await?;
        evicted_ids.push(event_id);
        evicted += 1;
    }
    if !evicted_ids.is_empty() {
        if let Ok(all_metadata) = db.get_all_feed_metadata().await {
            for (feed_key, mut metadata) in all_metadata {
                let original_len = metadata.event_ids.len();
                metadata.event_ids.retain(|id| !evicted_ids.contains(id));
                if metadata.event_ids.len() != original_len {
                    let _ = db.put_feed_metadata(&feed_key, &metadata).await;
                }
            }
        }
    }
    log::info!("Evicted {} items from feed cache", evicted);
    Ok(evicted)
}
/// Update LRU timestamps for displayed items
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
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
#[cfg(target_arch = "wasm32")]
fn current_timestamp() -> u64 {
    use web_sys::js_sys::Date;
    (Date::now() / 1000.0) as u64
}
