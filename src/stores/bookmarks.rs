use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_stores::Store;
use nostr_sdk::nips::nip51::Bookmarks;
use nostr_sdk::{Event, EventBuilder, EventId, Filter, Kind, PublicKey};
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use gloo_timers::callback::Timeout;
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;
/// Store for bookmarked event IDs with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct BookmarkedEventsStore {
    pub data: Vec<String>,
}
/// Store for bookmark rollback state with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct BookmarkRollbackStore {
    pub data: Option<Vec<String>>,
}
/// Global signal to track bookmarked event IDs
pub static BOOKMARKED_EVENTS: GlobalSignal<Store<BookmarkedEventsStore>> = Signal::global(||
Store::new(BookmarkedEventsStore::default()));
/// Sync status for bookmark publishing
#[derive(Clone, Debug, PartialEq)]
pub enum BookmarkSyncStatus {
    /// No pending operations
    Idle,
    /// Publishing to relays in progress
    Syncing,
    /// Publish failed with error message and retry count
    Failed { error: String, retry_count: u32 },
}
/// Global signal to track bookmark sync status
pub static BOOKMARK_SYNC_STATUS: GlobalSignal<BookmarkSyncStatus> = Signal::global(|| {
    BookmarkSyncStatus::Idle
});
/// Previous bookmark state for rollback on failure
pub static BOOKMARK_ROLLBACK_STATE: GlobalSignal<Store<BookmarkRollbackStore>> = Signal::global(||
Store::new(BookmarkRollbackStore::default()));
#[cfg(target_arch = "wasm32")]
thread_local! {
    /// Pending bookmark publish timeout (for debouncing)
    static BOOKMARK_PUBLISH_TIMEOUT: RefCell<Option<Timeout>> = const {
        RefCell::new(None)
    };
}
/// Initialize bookmarks by fetching from relays
pub async fn init_bookmarks() -> Result<(), String> {
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    log::info!("Loading bookmarks for {}", pubkey_str);
    let filter = Filter::new().author(pubkey).kind(Kind::Bookmarks).limit(1);
    nostr_client::ensure_relays_ready(&client).await;
    log::info!("Fetching bookmarks with filter: kind=10003, author={}", pubkey_str);
    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let events_vec: Vec<_> = events.into_iter().collect();
            log::info!("Received {} bookmark events from relays", events_vec.len());
            if let Some(event) = events_vec.into_iter().next() {
                log::info!(
                    "Bookmark event found: id={}, tags count={}", event.id.to_hex(),
                    event.tags.len()
                );
                for tag in event.tags.iter() {
                    log::debug!("  Tag: {:?}", tag);
                }
                let bookmarked: Vec<String> = {
                    let mut seen = std::collections::HashSet::new();
                    event
                        .tags
                        .iter()
                        .filter_map(|tag| {
                            let tag_vec = tag.clone().to_vec();
                            if tag_vec.first().map(|s| s.as_str()) == Some("e")
                                && tag_vec.len() >= 2
                            {
                                let id = tag_vec[1].clone();
                                if seen.insert(id.clone()) {
                                    Some(id)
                                } else {
                                    log::warn!("Skipping duplicate bookmark ID: {}", id);
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .collect()
                };
                log::info!(
                    "Extracted {} bookmark entries from 'e' tags", bookmarked.len()
                );
                for entry in &bookmarked {
                    log::debug!("  Bookmark: id={}", entry);
                }
                *BOOKMARKED_EVENTS.peek().data().write() = bookmarked;
                Ok(())
            } else {
                log::info!("No bookmark events found matching filter");
                *BOOKMARKED_EVENTS.peek().data().write() = Vec::new();
                Ok(())
            }
        }
        Err(e) => {
            log::error!("Failed to fetch bookmarks: {}", e);
            Err(format!("Failed to fetch bookmarks: {}", e))
        }
    }
}
/// Check if an event is bookmarked
/// Uses peek() to avoid creating subscriptions that could conflict with writes
pub fn is_bookmarked(event_id: &str) -> bool {
    BOOKMARKED_EVENTS.peek().data().peek().iter().any(|id| id == event_id)
}
/// Add event to bookmarks
///
/// # Arguments
/// * `event_id` - The event ID to bookmark
pub async fn bookmark_event(event_id: String) -> Result<(), String> {
    EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID '{}': {}", event_id, e))?;
    let mut bookmarks = BOOKMARKED_EVENTS.peek().data().peek().clone();
    if bookmarks.iter().any(|id| id == &event_id) {
        return Ok(());
    }
    if BOOKMARK_ROLLBACK_STATE.peek().data().peek().is_none() {
        *BOOKMARK_ROLLBACK_STATE.peek().data().write() = Some(bookmarks.clone());
    }
    bookmarks.push(event_id);
    *BOOKMARKED_EVENTS.peek().data().write() = bookmarks.clone();
    #[cfg(target_arch = "wasm32")]
    {
        BOOKMARK_PUBLISH_TIMEOUT
            .with(|timeout| {
                *timeout.borrow_mut() = None;
                let timeout_handle = Timeout::new(
                    1000,
                    move || {
                        spawn_local(async move {
                            publish_with_retry(bookmarks, 0).await;
                        });
                    },
                );
                *timeout.borrow_mut() = Some(timeout_handle);
            });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        publish_with_retry(bookmarks, 0).await;
    }
    Ok(())
}
/// Remove event from bookmarks
pub async fn unbookmark_event(event_id: String) -> Result<(), String> {
    let mut bookmarks = BOOKMARKED_EVENTS.peek().data().peek().clone();
    if BOOKMARK_ROLLBACK_STATE.peek().data().peek().is_none() {
        *BOOKMARK_ROLLBACK_STATE.peek().data().write() = Some(bookmarks.clone());
    }
    bookmarks.retain(|id| id != &event_id);
    *BOOKMARKED_EVENTS.peek().data().write() = bookmarks.clone();
    #[cfg(target_arch = "wasm32")]
    {
        BOOKMARK_PUBLISH_TIMEOUT
            .with(|timeout| {
                *timeout.borrow_mut() = None;
                let timeout_handle = Timeout::new(
                    1000,
                    move || {
                        spawn_local(async move {
                            publish_with_retry(bookmarks, 0).await;
                        });
                    },
                );
                *timeout.borrow_mut() = Some(timeout_handle);
            });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        publish_with_retry(bookmarks, 0).await;
    }
    Ok(())
}
/// Publish bookmarks with retry and exponential backoff
fn publish_with_retry(
    bookmarks: Vec<String>,
    retry_count: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'static>> {
    Box::pin(async move {
        const MAX_RETRIES: u32 = 3;
        *BOOKMARK_SYNC_STATUS.write() = BookmarkSyncStatus::Syncing;
        match publish_bookmarks(bookmarks.clone()).await {
            Ok(_) => {
                *BOOKMARK_ROLLBACK_STATE.peek().data().write() = None;
                *BOOKMARK_SYNC_STATUS.write() = BookmarkSyncStatus::Idle;
                log::info!("Bookmarks published successfully");
            }
            Err(e) => {
                log::error!(
                    "Failed to publish bookmarks (attempt {}): {}", retry_count + 1, e
                );
                if retry_count < MAX_RETRIES {
                    let delay_ms = 1000u32 * (1 << retry_count);
                    log::info!(
                        "Retrying bookmark publish in {}ms (attempt {}/{})", delay_ms,
                        retry_count + 1, MAX_RETRIES
                    );
                    #[cfg(target_arch = "wasm32")]
                    {
                        let timeout_handle = Timeout::new(
                            delay_ms,
                            move || {
                                spawn_local(async move {
                                    publish_with_retry(bookmarks, retry_count + 1).await;
                                });
                            },
                        );
                        std::mem::forget(timeout_handle);
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        tokio::time::sleep(Duration::from_millis(delay_ms as u64)).await;
                        publish_with_retry(bookmarks, retry_count + 1).await;
                    }
                } else {
                    log::error!(
                        "Bookmark publish failed after {} retries: {}", MAX_RETRIES, e
                    );
                    if let Some(previous_state) = BOOKMARK_ROLLBACK_STATE
                        .peek()
                        .data()
                        .peek()
                        .clone()
                    {
                        log::warn!(
                            "Automatically rolling back bookmarks to previous state due to publish failure"
                        );
                        *BOOKMARKED_EVENTS.peek().data().write() = previous_state;
                    }
                    *BOOKMARK_ROLLBACK_STATE.peek().data().write() = None;
                    *BOOKMARK_SYNC_STATUS.write() = BookmarkSyncStatus::Failed {
                        error: e.clone(),
                        retry_count,
                    };
                }
            }
        }
    })
}
/// Rollback bookmarks to previous state after failed publish
#[allow(dead_code)]
pub fn rollback_bookmarks() {
    if let Some(previous_state) = BOOKMARK_ROLLBACK_STATE.peek().data().peek().clone() {
        log::info!("Rolling back bookmarks to previous state");
        *BOOKMARKED_EVENTS.peek().data().write() = previous_state;
        *BOOKMARK_ROLLBACK_STATE.peek().data().write() = None;
        *BOOKMARK_SYNC_STATUS.write() = BookmarkSyncStatus::Idle;
    } else {
        log::warn!("No rollback state available");
    }
}
/// Manually retry failed bookmark publish
#[allow(dead_code)]
pub async fn retry_bookmark_publish() {
    let current_bookmarks = BOOKMARKED_EVENTS.peek().data().peek().clone();
    log::info!("Manually retrying bookmark publish");
    publish_with_retry(current_bookmarks, 0).await;
}
/// Dismiss failed status and keep local changes
#[allow(dead_code)]
pub fn dismiss_bookmark_error() {
    log::info!("Dismissing bookmark sync error, keeping local changes");
    *BOOKMARK_ROLLBACK_STATE.peek().data().write() = None;
    *BOOKMARK_SYNC_STATUS.write() = BookmarkSyncStatus::Idle;
}
/// Publish bookmarks list to relays (NIP-51)
async fn publish_bookmarks(bookmarks: Vec<String>) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    log::info!("Publishing {} bookmarks", bookmarks.len());
    let mut event_ids: Vec<EventId> = Vec::with_capacity(bookmarks.len());
    for (index, id) in bookmarks.iter().enumerate() {
        match EventId::from_hex(id) {
            Ok(event_id) => event_ids.push(event_id),
            Err(e) => {
                log::warn!(
                    "Skipping malformed bookmark ID at index {}: '{}' (error: {})",
                    index, id, e
                );
            }
        }
    }
    if event_ids.len() < bookmarks.len() {
        log::warn!(
            "publish_bookmarks: {} of {} bookmark IDs were invalid and skipped",
            bookmarks.len() - event_ids.len(), bookmarks.len()
        );
    }
    let bookmark_list = Bookmarks {
        event_ids,
        coordinate: Vec::new(),
        hashtags: Vec::new(),
        urls: Vec::new(),
    };
    let builder = EventBuilder::bookmarks(bookmark_list);
    match client.send_event_builder(builder).await {
        Ok(output) => {
            let success_count = output.success.len();
            let failed_count = output.failed.len();
            let total = success_count + failed_count;
            log::info!(
                "Bookmarks published: {} ({}/{} relays succeeded)", output.id().to_hex(),
                success_count, total
            );
            if !output.failed.is_empty() {
                for (relay, error) in &output.failed {
                    log::warn!("Relay {} failed: {}", relay, error);
                }
            }
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to publish bookmarks: {}", e);
            Err(format!("Failed to publish bookmarks: {}", e))
        }
    }
}
/// Fetch bookmarked events with pagination support
///
/// # Arguments
/// * `skip` - Number of bookmarks to skip (for pagination)
/// * `limit` - Maximum number of bookmarks to fetch (None = fetch all remaining)
pub async fn fetch_bookmarked_events_paginated(
    skip: usize,
    limit: Option<usize>,
) -> Result<Vec<Event>, String> {
    let bookmarks = BOOKMARKED_EVENTS.peek().data().peek().clone();
    if bookmarks.is_empty() {
        return Ok(Vec::new());
    }
    let bookmarks_slice: Vec<String> = if skip >= bookmarks.len() {
        Vec::new()
    } else {
        let end = if let Some(lim) = limit {
            (skip + lim).min(bookmarks.len())
        } else {
            bookmarks.len()
        };
        bookmarks[skip..end].to_vec()
    };
    if bookmarks_slice.is_empty() {
        return Ok(Vec::new());
    }
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let mut event_ids: Vec<EventId> = Vec::with_capacity(bookmarks_slice.len());
    for (index, id) in bookmarks_slice.iter().enumerate() {
        match EventId::from_hex(id) {
            Ok(event_id) => event_ids.push(event_id),
            Err(e) => {
                log::warn!(
                    "fetch_bookmarked_events: skipping malformed bookmark ID at index {}: '{}' (error: {})",
                    skip + index, id, e
                );
            }
        }
    }
    if event_ids.len() < bookmarks_slice.len() {
        log::warn!(
            "fetch_bookmarked_events: {} of {} bookmark IDs were invalid and skipped",
            bookmarks_slice.len() - event_ids.len(), bookmarks_slice.len()
        );
    }
    log::info!("Fetching {} bookmarked event IDs", event_ids.len());
    for id in &event_ids {
        log::info!("  Requesting event: {}", id.to_hex());
    }
    let filter = Filter::new().ids(event_ids.clone());
    nostr_client::ensure_relays_ready(&client).await;
    match client.fetch_events(filter.clone(), Duration::from_secs(15)).await {
        Ok(events) => {
            let mut event_vec: Vec<Event> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            log::info!(
                "Fetched {} bookmarked events (skip: {}, limit: {:?})", event_vec.len(),
                skip, limit
            );
            let found_ids: Vec<String> = event_vec
                .iter()
                .map(|e| e.id.to_hex())
                .collect();
            for id in &event_ids {
                let hex = id.to_hex();
                if found_ids.contains(&hex) {
                    log::info!("  ✓ Found event: {}", hex);
                } else {
                    log::warn!("  ✗ Missing event: {} (not found on relays)", hex);
                }
            }
            Ok(event_vec)
        }
        Err(e) => {
            log::error!("Failed to fetch bookmarked events: {}", e);
            Err(format!("Failed to fetch bookmarked events: {}", e))
        }
    }
}
/// Get the total number of bookmarks
pub fn get_bookmarks_count() -> usize {
    BOOKMARKED_EVENTS.peek().data().peek().len()
}
