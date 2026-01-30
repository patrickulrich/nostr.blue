//! # IndexedDB Feed Cache Database
//!
//! Browser-native implementation of feed caching using IndexedDB.
//!
//! ## Overview
//!
//! This module provides persistent storage for feed items (posts, photos, videos, articles)
//! in web browsers. It enables instant feed loading from cache while background network
//! requests fetch fresh data.
//!
//! ## Storage Model
//!
//! Data is stored as JSON strings in IndexedDB object stores:
//! - `feed_items` - Cached feed items (key: event_id)
//! - `feed_metadata` - Feed-level metadata (key: feed_key)
//! - `lru_order` - LRU access tracking (key: event_id)
//!
//! ## Platform Support
//!
//! This module only compiles on wasm32 targets. A stub type is provided for
//! native targets to allow type checking.

#![cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, unused_imports, unused_variables)
)]
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use serde::{Deserialize, Serialize};

// ============================================================================
// Common types (shared between native and wasm)
// ============================================================================

/// Cached feed item with metadata
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedFeedItem {
    /// Raw event JSON for deserialization
    pub event_json: String,
    /// Type of item (original post or repost)
    pub item_type: CachedFeedItemType,
    /// Timestamp used for sorting (created_at or repost time)
    pub sort_timestamp: u64,
    /// When this item was cached
    pub cached_at: u64,
    /// Feed keys this item belongs to (for multi-feed membership)
    pub feed_keys: Vec<String>,
}

/// Type of cached feed item
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CachedFeedItemType {
    /// Original post by author
    OriginalPost,
    /// Repost of another event
    Repost {
        reposted_by: String,
        repost_timestamp: u64,
    },
}

/// Metadata for a feed cache
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeedCacheMetadata {
    /// Feed key (e.g., "following:pubkey123")
    pub feed_key: String,
    /// Event IDs in this feed, ordered by sort_timestamp descending
    pub event_ids: Vec<String>,
    /// Newest timestamp in the feed
    pub newest_timestamp: Option<u64>,
    /// Oldest timestamp in the feed
    pub oldest_timestamp: Option<u64>,
    /// When this feed was last synced
    pub last_sync: u64,
}

/// LRU tracking entry
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LruEntry {
    /// Last access timestamp
    pub last_access: u64,
}

/// Result of a bulk cache read operation, including failed entries for cleanup
#[derive(Clone, Debug)]
pub struct BulkReadResult<T> {
    /// Successfully deserialized items
    pub items: Vec<T>,
    /// Number of entries that failed to deserialize
    pub failed_count: usize,
    /// Keys of entries that failed to deserialize (for cleanup)
    pub failed_keys: Vec<String>,
}

// ============================================================================
// Native stub (non-wasm32)
// ============================================================================
#[cfg(not(target_arch = "wasm32"))]
mod native_stub {
    use super::*;

    /// Stub type for native targets - cannot be instantiated
    #[derive(Clone, Debug)]
    pub struct FeedCacheDb {
        _private: (), // Prevents construction
    }

    unsafe impl Send for FeedCacheDb {}
    unsafe impl Sync for FeedCacheDb {}

    impl FeedCacheDb {
        fn make_error(msg: String) -> String {
            msg
        }

        pub async fn new() -> Result<Self, String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn get_feed_item(
            &self,
            _event_id: &str,
        ) -> Result<Option<CachedFeedItem>, String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn put_feed_item(
            &self,
            _event_id: &str,
            _item: &CachedFeedItem,
        ) -> Result<(), String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn delete_feed_item(&self, _event_id: &str) -> Result<(), String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn get_feed_metadata(
            &self,
            _feed_key: &str,
        ) -> Result<Option<FeedCacheMetadata>, String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn put_feed_metadata(
            &self,
            _feed_key: &str,
            _metadata: &FeedCacheMetadata,
        ) -> Result<(), String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn get_lru_entry(&self, _event_id: &str) -> Result<Option<LruEntry>, String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn put_lru_entry(
            &self,
            _event_id: &str,
            _entry: &LruEntry,
        ) -> Result<(), String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn delete_lru_entry(&self, _event_id: &str) -> Result<(), String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn get_all_lru_entries(&self) -> Result<Vec<(String, LruEntry)>, String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn count_feed_items(&self) -> Result<u32, String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn get_feed_items_by_ids(
            &self,
            _event_ids: &[String],
        ) -> Result<BulkReadResult<CachedFeedItem>, String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn get_all_feed_metadata_bulk(
            &self,
        ) -> Result<BulkReadResult<(String, FeedCacheMetadata)>, String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn get_all_lru_entries_bulk(
            &self,
        ) -> Result<BulkReadResult<(String, LruEntry)>, String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }

        pub async fn delete_bad_entries(
            &self,
            _keys: &[String],
            _store_name: &str,
        ) -> Result<usize, String> {
            Err(Self::make_error(
                "FeedCacheDb is only available on wasm32 targets".to_string(),
            ))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native_stub::FeedCacheDb;

// ============================================================================
// WASM implementation
// ============================================================================

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use super::*;
    use indexed_db_futures::prelude::*;
    use std::future::IntoFuture;
    use std::rc::Rc;
    use wasm_bindgen::JsValue;

    // Database constants
    const DB_NAME: &str = "nostr_blue_feed_cache";
    const DB_VERSION: u32 = 1;

    // Object store names
    const STORE_FEED_ITEMS: &str = "feed_items";
    const STORE_FEED_METADATA: &str = "feed_metadata";
    const STORE_LRU_ORDER: &str = "lru_order";

    /// IndexedDB-backed feed cache
    #[derive(Clone, Debug)]
    pub struct FeedCacheDb {
        db: Rc<IdbDatabase>,
    }

    // SAFETY: In WASM, there's only one thread, so Send + Sync are safe
    unsafe impl Send for FeedCacheDb {}
    unsafe impl Sync for FeedCacheDb {}

    impl FeedCacheDb {
        /// Create a new feed cache database instance
        pub async fn new() -> Result<Self, String> {
            let mut db_req: OpenDbRequest = IdbDatabase::open_u32(DB_NAME, DB_VERSION)
                .map_err(|e| format!("Failed to open feed cache database: {:?}", e))?;

            // Handle upgradeneeded event to create object stores
            db_req.set_on_upgrade_needed(Some(|evt: &IdbVersionChangeEvent| {
                log::info!("Feed cache IndexedDB upgrade needed, creating object stores");

                let db = evt.db();

                // Create object stores if they don't exist
                if !db.object_store_names().any(|n| n == STORE_FEED_ITEMS) {
                    db.create_object_store(STORE_FEED_ITEMS)?;
                }
                if !db.object_store_names().any(|n| n == STORE_FEED_METADATA) {
                    db.create_object_store(STORE_FEED_METADATA)?;
                }
                if !db.object_store_names().any(|n| n == STORE_LRU_ORDER) {
                    db.create_object_store(STORE_LRU_ORDER)?;
                }

                Ok(())
            }));

            // Wait for database to open
            let db: IdbDatabase = db_req
                .into_future()
                .await
                .map_err(|e| format!("Failed to open feed cache database: {:?}", e))?;

            log::info!("Feed cache IndexedDB initialized successfully");

            Ok(Self { db: Rc::new(db) })
        }

        /// Helper: Get a value from a store with JSON deserialization
        async fn get_value<T>(&self, store_name: &str, key: &str) -> Result<Option<T>, String>
        where
            T: for<'de> Deserialize<'de>,
        {
            let tx = self
                .db
                .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readonly)
                .map_err(|e| format!("Transaction error: {:?}", e))?;

            let store = tx
                .object_store(store_name)
                .map_err(|e| format!("Store error: {:?}", e))?;

            let js_key = JsValue::from_str(key);
            let value_opt = store
                .get(&js_key)
                .map_err(|e| format!("Get error: {:?}", e))?
                .await
                .map_err(|e| format!("Get await error: {:?}", e))?;

            if value_opt.is_none() {
                return Ok(None);
            }

            let value = value_opt.unwrap();

            // Deserialize from JSON string
            let json_str = value
                .as_string()
                .ok_or_else(|| "Value is not a string".to_string())?;

            let deserialized: T = serde_json::from_str(&json_str)
                .map_err(|e| format!("JSON deserialization error: {}", e))?;

            Ok(Some(deserialized))
        }

        /// Helper: Put a value into a store with JSON serialization
        async fn put_value<T>(&self, store_name: &str, key: &str, value: &T) -> Result<(), String>
        where
            T: Serialize,
        {
            let tx = self
                .db
                .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readwrite)
                .map_err(|e| format!("Transaction error: {:?}", e))?;

            let store = tx
                .object_store(store_name)
                .map_err(|e| format!("Store error: {:?}", e))?;

            // Serialize to JSON string
            let json_str = serde_json::to_string(value)
                .map_err(|e| format!("JSON serialization error: {}", e))?;

            let js_key = JsValue::from_str(key);
            let js_value = JsValue::from_str(&json_str);

            store
                .put_key_val(&js_key, &js_value)
                .map_err(|e| format!("Put error: {:?}", e))?;

            // Wait for transaction to complete
            tx.await
                .into_result()
                .map_err(|e| format!("Transaction commit error: {:?}", e))?;

            Ok(())
        }

        /// Helper: Delete a value from a store
        async fn delete_value(&self, store_name: &str, key: &str) -> Result<(), String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readwrite)
                .map_err(|e| format!("Transaction error: {:?}", e))?;

            let store = tx
                .object_store(store_name)
                .map_err(|e| format!("Store error: {:?}", e))?;

            let js_key = JsValue::from_str(key);
            store
                .delete(&js_key)
                .map_err(|e| format!("Delete error: {:?}", e))?;

            // Wait for transaction to complete
            tx.await
                .into_result()
                .map_err(|e| format!("Transaction commit error: {:?}", e))?;

            Ok(())
        }

        // ====================================================================
        // Feed Items API
        // ====================================================================

        /// Get a cached feed item by event ID
        pub async fn get_feed_item(
            &self,
            event_id: &str,
        ) -> Result<Option<CachedFeedItem>, String> {
            self.get_value(STORE_FEED_ITEMS, event_id).await
        }

        /// Store a cached feed item
        pub async fn put_feed_item(
            &self,
            event_id: &str,
            item: &CachedFeedItem,
        ) -> Result<(), String> {
            self.put_value(STORE_FEED_ITEMS, event_id, item).await
        }

        /// Delete a cached feed item
        pub async fn delete_feed_item(&self, event_id: &str) -> Result<(), String> {
            self.delete_value(STORE_FEED_ITEMS, event_id).await
        }

        /// Get multiple feed items by their IDs, tracking deserialization failures
        pub async fn get_feed_items_by_ids(
            &self,
            event_ids: &[String],
        ) -> Result<BulkReadResult<CachedFeedItem>, String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(STORE_FEED_ITEMS, IdbTransactionMode::Readonly)
                .map_err(|e| format!("Transaction error: {:?}", e))?;

            let store = tx
                .object_store(STORE_FEED_ITEMS)
                .map_err(|e| format!("Store error: {:?}", e))?;

            let mut items = Vec::new();
            let mut failed_keys = Vec::new();

            for event_id in event_ids {
                let js_key = JsValue::from_str(event_id);
                let value_opt = store
                    .get(&js_key)
                    .map_err(|e| format!("Get error: {:?}", e))?
                    .await
                    .map_err(|e| format!("Get await error: {:?}", e))?;

                if let Some(value) = value_opt {
                    if let Some(json_str) = value.as_string() {
                        match serde_json::from_str::<CachedFeedItem>(&json_str) {
                            Ok(item) => items.push(item),
                            Err(e) => {
                                log::error!(
                                    "Cache corruption: Failed to deserialize feed item '{}': {} - JSON preview: {}",
                                    event_id,
                                    e,
                                    &json_str[..json_str.len().min(200)]
                                );
                                failed_keys.push(event_id.clone());
                            }
                        }
                    }
                }
            }

            Ok(BulkReadResult {
                items,
                failed_count: failed_keys.len(),
                failed_keys,
            })
        }

        /// Count total feed items in cache
        pub async fn count_feed_items(&self) -> Result<u32, String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(STORE_FEED_ITEMS, IdbTransactionMode::Readonly)
                .map_err(|e| format!("Transaction error: {:?}", e))?;

            let store = tx
                .object_store(STORE_FEED_ITEMS)
                .map_err(|e| format!("Store error: {:?}", e))?;

            let count = store
                .count()
                .map_err(|e| format!("Count error: {:?}", e))?
                .await
                .map_err(|e| format!("Count await error: {:?}", e))?;

            Ok(count)
        }

        // ====================================================================
        // Feed Metadata API
        // ====================================================================

        /// Get feed metadata by feed key
        pub async fn get_feed_metadata(
            &self,
            feed_key: &str,
        ) -> Result<Option<FeedCacheMetadata>, String> {
            self.get_value(STORE_FEED_METADATA, feed_key).await
        }

        /// Store feed metadata
        pub async fn put_feed_metadata(
            &self,
            feed_key: &str,
            metadata: &FeedCacheMetadata,
        ) -> Result<(), String> {
            self.put_value(STORE_FEED_METADATA, feed_key, metadata)
                .await
        }

        /// Get all feed metadata entries (for eviction cleanup)
        pub async fn get_all_feed_metadata(
            &self,
        ) -> Result<Vec<(String, FeedCacheMetadata)>, String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(STORE_FEED_METADATA, IdbTransactionMode::Readonly)
                .map_err(|e| format!("Transaction error: {:?}", e))?;

            let store = tx
                .object_store(STORE_FEED_METADATA)
                .map_err(|e| format!("Store error: {:?}", e))?;

            let cursor = store
                .open_cursor()
                .map_err(|e| format!("Cursor error: {:?}", e))?
                .await
                .map_err(|e| format!("Cursor await error: {:?}", e))?;

            let mut entries = Vec::new();

            if let Some(cursor) = cursor {
                loop {
                    if let Some(key_js) = cursor.key() {
                        let value_js = cursor.value();
                        if let (Some(key_str), Some(value_str)) =
                            (key_js.as_string(), value_js.as_string())
                        {
                            match serde_json::from_str::<FeedCacheMetadata>(&value_str) {
                                Ok(metadata) => entries.push((key_str, metadata)),
                                Err(e) => {
                                    log::warn!(
                                        "Failed to deserialize feed metadata {}: {}",
                                        key_str,
                                        e
                                    );
                                }
                            }
                        }
                    }

                    let has_next = cursor
                        .continue_cursor()
                        .map_err(|e| format!("Cursor continue error: {:?}", e))?
                        .await
                        .map_err(|e| format!("Cursor continue await error: {:?}", e))?;

                    if !has_next {
                        break;
                    }
                }
            }

            Ok(entries)
        }

        // ====================================================================
        // LRU Tracking API
        // ====================================================================

        /// Get LRU entry for an event
        pub async fn get_lru_entry(&self, event_id: &str) -> Result<Option<LruEntry>, String> {
            self.get_value(STORE_LRU_ORDER, event_id).await
        }

        /// Update LRU entry for an event (touch on access)
        pub async fn put_lru_entry(&self, event_id: &str, entry: &LruEntry) -> Result<(), String> {
            self.put_value(STORE_LRU_ORDER, event_id, entry).await
        }

        /// Delete LRU entry
        pub async fn delete_lru_entry(&self, event_id: &str) -> Result<(), String> {
            self.delete_value(STORE_LRU_ORDER, event_id).await
        }

        /// Get all LRU entries for eviction processing
        pub async fn get_all_lru_entries(&self) -> Result<Vec<(String, LruEntry)>, String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(STORE_LRU_ORDER, IdbTransactionMode::Readonly)
                .map_err(|e| format!("Transaction error: {:?}", e))?;

            let store = tx
                .object_store(STORE_LRU_ORDER)
                .map_err(|e| format!("Store error: {:?}", e))?;

            // Get cursor to iterate all entries
            let cursor = store
                .open_cursor()
                .map_err(|e| format!("Cursor error: {:?}", e))?
                .await
                .map_err(|e| format!("Cursor await error: {:?}", e))?;

            let mut entries = Vec::new();

            if let Some(cursor) = cursor {
                loop {
                    // key() returns Option<JsValue>, value() returns JsValue
                    if let Some(key_js) = cursor.key() {
                        let value_js = cursor.value();
                        if let (Some(key_str), Some(value_str)) =
                            (key_js.as_string(), value_js.as_string())
                        {
                            match serde_json::from_str::<LruEntry>(&value_str) {
                                Ok(entry) => entries.push((key_str, entry)),
                                Err(e) => {
                                    log::warn!(
                                        "Failed to deserialize LRU entry {}: {}",
                                        key_str,
                                        e
                                    );
                                }
                            }
                        }
                    }

                    // Move to next entry
                    let has_next = cursor
                        .continue_cursor()
                        .map_err(|e| format!("Cursor continue error: {:?}", e))?
                        .await
                        .map_err(|e| format!("Cursor continue await error: {:?}", e))?;

                    if !has_next {
                        break;
                    }
                }
            }

            Ok(entries)
        }

        /// Get all feed metadata entries with failure tracking (for eviction cleanup)
        pub async fn get_all_feed_metadata_bulk(
            &self,
        ) -> Result<BulkReadResult<(String, FeedCacheMetadata)>, String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(STORE_FEED_METADATA, IdbTransactionMode::Readonly)
                .map_err(|e| format!("Transaction error: {:?}", e))?;

            let store = tx
                .object_store(STORE_FEED_METADATA)
                .map_err(|e| format!("Store error: {:?}", e))?;

            let cursor = store
                .open_cursor()
                .map_err(|e| format!("Cursor error: {:?}", e))?
                .await
                .map_err(|e| format!("Cursor await error: {:?}", e))?;

            let mut items = Vec::new();
            let mut failed_keys = Vec::new();

            if let Some(cursor) = cursor {
                loop {
                    if let Some(key_js) = cursor.key() {
                        let value_js = cursor.value();
                        if let (Some(key_str), Some(value_str)) =
                            (key_js.as_string(), value_js.as_string())
                        {
                            match serde_json::from_str::<FeedCacheMetadata>(&value_str) {
                                Ok(metadata) => items.push((key_str, metadata)),
                                Err(e) => {
                                    log::error!(
                                        "Cache corruption: Failed to deserialize feed metadata '{}': {} - JSON preview: {}",
                                        key_str,
                                        e,
                                        &value_str[..value_str.len().min(200)]
                                    );
                                    failed_keys.push(key_str);
                                }
                            }
                        }
                    }

                    let has_next = cursor
                        .continue_cursor()
                        .map_err(|e| format!("Cursor continue error: {:?}", e))?
                        .await
                        .map_err(|e| format!("Cursor continue await error: {:?}", e))?;

                    if !has_next {
                        break;
                    }
                }
            }

            Ok(BulkReadResult {
                items,
                failed_count: failed_keys.len(),
                failed_keys,
            })
        }

        /// Get all LRU entries with failure tracking (for eviction processing)
        pub async fn get_all_lru_entries_bulk(
            &self,
        ) -> Result<BulkReadResult<(String, LruEntry)>, String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(STORE_LRU_ORDER, IdbTransactionMode::Readonly)
                .map_err(|e| format!("Transaction error: {:?}", e))?;

            let store = tx
                .object_store(STORE_LRU_ORDER)
                .map_err(|e| format!("Store error: {:?}", e))?;

            let cursor = store
                .open_cursor()
                .map_err(|e| format!("Cursor error: {:?}", e))?
                .await
                .map_err(|e| format!("Cursor await error: {:?}", e))?;

            let mut items = Vec::new();
            let mut failed_keys = Vec::new();

            if let Some(cursor) = cursor {
                loop {
                    if let Some(key_js) = cursor.key() {
                        let value_js = cursor.value();
                        if let (Some(key_str), Some(value_str)) =
                            (key_js.as_string(), value_js.as_string())
                        {
                            match serde_json::from_str::<LruEntry>(&value_str) {
                                Ok(entry) => items.push((key_str, entry)),
                                Err(e) => {
                                    log::error!(
                                        "Cache corruption: Failed to deserialize LRU entry '{}': {} - JSON preview: {}",
                                        key_str,
                                        e,
                                        &value_str[..value_str.len().min(200)]
                                    );
                                    failed_keys.push(key_str);
                                }
                            }
                        }
                    }

                    let has_next = cursor
                        .continue_cursor()
                        .map_err(|e| format!("Cursor continue error: {:?}", e))?
                        .await
                        .map_err(|e| format!("Cursor continue await error: {:?}", e))?;

                    if !has_next {
                        break;
                    }
                }
            }

            Ok(BulkReadResult {
                items,
                failed_count: failed_keys.len(),
                failed_keys,
            })
        }

        /// Delete corrupted cache entries by their keys
        pub async fn delete_bad_entries(
            &self,
            keys: &[String],
            store_name: &str,
        ) -> Result<usize, String> {
            if keys.is_empty() {
                return Ok(0);
            }

            let tx = self
                .db
                .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readwrite)
                .map_err(|e| format!("Transaction error: {:?}", e))?;

            let store = tx
                .object_store(store_name)
                .map_err(|e| format!("Store error: {:?}", e))?;

            let mut deleted = 0;
            for key in keys {
                let js_key = JsValue::from_str(key);
                if store.delete(&js_key).is_ok() {
                    deleted += 1;
                }
            }

            tx.await
                .into_result()
                .map_err(|e| format!("Transaction commit error: {:?}", e))?;

            log::info!(
                "Cleaned up {} corrupted cache entries from {}",
                deleted,
                store_name
            );
            Ok(deleted)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::FeedCacheDb;
