use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_stores::Store;
use nostr_sdk::{Event, Filter, Kind, EventBuilder, PublicKey};
use crate::stores::{auth_store, nostr_client};
use std::time::Duration;

#[cfg(target_arch = "wasm32")]
use gloo_timers::callback::Timeout;
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;

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
pub static BOOKMARKED_EVENTS: GlobalSignal<Store<BookmarkedEventsStore>> =
    Signal::global(|| Store::new(BookmarkedEventsStore::default()));

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
pub static BOOKMARK_SYNC_STATUS: GlobalSignal<BookmarkSyncStatus> =
    Signal::global(|| BookmarkSyncStatus::Idle);

/// Previous bookmark state for rollback on failure
pub static BOOKMARK_ROLLBACK_STATE: GlobalSignal<Store<BookmarkRollbackStore>> =
    Signal::global(|| Store::new(BookmarkRollbackStore::default()));

#[cfg(target_arch = "wasm32")]
thread_local! {
    /// Pending bookmark publish timeout (for debouncing)
    static BOOKMARK_PUBLISH_TIMEOUT: RefCell<Option<Timeout>> = RefCell::new(None);
}

/// Initialize bookmarks by fetching from relays
pub async fn init_bookmarks() -> Result<(), String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Loading bookmarks for {}", pubkey_str);

    // Fetch bookmark list (kind 30001 with d tag "bookmark")
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(30001)) // Bookmarks list
        .identifier("bookmark") // NIP-51 bookmark identifier
        .limit(1);

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                // Extract event IDs from 'e' tags
                let bookmarked: Vec<String> = event.tags.iter()
                    .filter(|tag| tag.kind() == nostr_sdk::TagKind::e())
                    .filter_map(|tag| tag.content().map(|s| s.to_string()))
                    .collect();

                log::info!("Loaded {} bookmarks", bookmarked.len());
                *BOOKMARKED_EVENTS.read().data().write() = bookmarked;
                Ok(())
            } else {
                log::info!("No bookmarks found");
                *BOOKMARKED_EVENTS.read().data().write() = Vec::new();
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
pub fn is_bookmarked(event_id: &str) -> bool {
    BOOKMARKED_EVENTS.read().data().read().contains(&event_id.to_string())
}

/// Add event to bookmarks
pub async fn bookmark_event(event_id: String) -> Result<(), String> {
    // Validate event ID early to prevent invalid IDs from being stored
    use nostr_sdk::EventId;
    EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID '{}': {}", event_id, e))?;

    let mut bookmarks = BOOKMARKED_EVENTS.read().data().read().clone();

    // Don't add if already bookmarked
    if bookmarks.contains(&event_id) {
        return Ok(());
    }

    // Store rollback state before making changes (preserve initial state for batch)
    if BOOKMARK_ROLLBACK_STATE.read().data().read().is_none() {
        *BOOKMARK_ROLLBACK_STATE.read().data().write() = Some(bookmarks.clone());
    }

    bookmarks.push(event_id);

    // Update local state immediately for UI responsiveness
    *BOOKMARKED_EVENTS.read().data().write() = bookmarks.clone();

    // Debounce relay publish (batches rapid bookmarks into one publish)
    #[cfg(target_arch = "wasm32")]
    {
        BOOKMARK_PUBLISH_TIMEOUT.with(|timeout| {
            // Cancel any existing timeout
            *timeout.borrow_mut() = None;

            // Schedule new publish after 1 second
            let timeout_handle = Timeout::new(1000, move || {
                spawn(async move {
                    publish_with_retry(bookmarks, 0).await;
                });
            });

            *timeout.borrow_mut() = Some(timeout_handle);
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // Non-WASM: publish immediately with retry
        publish_with_retry(bookmarks, 0).await;
    }

    Ok(())
}

/// Remove event from bookmarks
pub async fn unbookmark_event(event_id: String) -> Result<(), String> {
    let mut bookmarks = BOOKMARKED_EVENTS.read().data().read().clone();

    // Store rollback state before making changes (preserve initial state for batch)
    if BOOKMARK_ROLLBACK_STATE.read().data().read().is_none() {
        *BOOKMARK_ROLLBACK_STATE.read().data().write() = Some(bookmarks.clone());
    }

    // Remove the event ID
    bookmarks.retain(|id| id != &event_id);

    // Update local state immediately for UI responsiveness
    *BOOKMARKED_EVENTS.read().data().write() = bookmarks.clone();

    // Debounce relay publish (batches rapid unbookmarks into one publish)
    #[cfg(target_arch = "wasm32")]
    {
        BOOKMARK_PUBLISH_TIMEOUT.with(|timeout| {
            // Cancel any existing timeout
            *timeout.borrow_mut() = None;

            // Schedule new publish after 1 second
            let timeout_handle = Timeout::new(1000, move || {
                spawn(async move {
                    publish_with_retry(bookmarks, 0).await;
                });
            });

            *timeout.borrow_mut() = Some(timeout_handle);
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // Non-WASM: publish immediately with retry
        publish_with_retry(bookmarks, 0).await;
    }

    Ok(())
}

/// Publish bookmarks with retry and exponential backoff
fn publish_with_retry(bookmarks: Vec<String>, retry_count: u32) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'static>> {
    Box::pin(async move {
        const MAX_RETRIES: u32 = 3;

        // Set status to syncing
        *BOOKMARK_SYNC_STATUS.write() = BookmarkSyncStatus::Syncing;

        match publish_bookmarks(bookmarks.clone()).await {
            Ok(_) => {
                // Success - clear rollback state and set status to idle
                *BOOKMARK_ROLLBACK_STATE.read().data().write() = None;
                *BOOKMARK_SYNC_STATUS.write() = BookmarkSyncStatus::Idle;
                log::info!("Bookmarks published successfully");
            }
            Err(e) => {
                log::error!("Failed to publish bookmarks (attempt {}): {}", retry_count + 1, e);

                if retry_count < MAX_RETRIES {
                    // Calculate exponential backoff delay: 1s, 2s, 4s
                    let delay_ms = 1000u32 * (1 << retry_count); // 2^retry_count seconds

                    log::info!("Retrying bookmark publish in {}ms (attempt {}/{})",
                        delay_ms, retry_count + 1, MAX_RETRIES);

                    // Schedule retry with exponential backoff
                    #[cfg(target_arch = "wasm32")]
                    {
                        let timeout_handle = Timeout::new(delay_ms, move || {
                            spawn(publish_with_retry(bookmarks, retry_count + 1));
                        });
                        // Note: We let the timeout run and don't store it since it's a retry
                        std::mem::forget(timeout_handle);
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        tokio::time::sleep(Duration::from_millis(delay_ms as u64)).await;
                        publish_with_retry(bookmarks, retry_count + 1).await;
                    }
                } else {
                    // Max retries exceeded - rollback local state and set failed status
                    log::error!("Bookmark publish failed after {} retries: {}", MAX_RETRIES, e);

                    // Rollback local state to match persisted state
                    if let Some(previous_state) = BOOKMARK_ROLLBACK_STATE.read().data().read().clone() {
                        log::warn!("Automatically rolling back bookmarks to previous state due to publish failure");
                        *BOOKMARKED_EVENTS.read().data().write() = previous_state;
                    }

                    // Set failed status (rollback state is cleared here)
                    *BOOKMARK_ROLLBACK_STATE.read().data().write() = None;
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
    if let Some(previous_state) = BOOKMARK_ROLLBACK_STATE.read().data().read().clone() {
        log::info!("Rolling back bookmarks to previous state");
        *BOOKMARKED_EVENTS.read().data().write() = previous_state;
        *BOOKMARK_ROLLBACK_STATE.read().data().write() = None;
        *BOOKMARK_SYNC_STATUS.write() = BookmarkSyncStatus::Idle;
    } else {
        log::warn!("No rollback state available");
    }
}

/// Manually retry failed bookmark publish
#[allow(dead_code)]
pub async fn retry_bookmark_publish() {
    let current_bookmarks = BOOKMARKED_EVENTS.read().data().read().clone();
    log::info!("Manually retrying bookmark publish");
    publish_with_retry(current_bookmarks, 0).await;
}

/// Dismiss failed status and keep local changes
#[allow(dead_code)]
pub fn dismiss_bookmark_error() {
    log::info!("Dismissing bookmark sync error, keeping local changes");
    *BOOKMARK_ROLLBACK_STATE.read().data().write() = None;
    *BOOKMARK_SYNC_STATUS.write() = BookmarkSyncStatus::Idle;
}

/// Publish bookmarks list to relays (NIP-51)
async fn publish_bookmarks(bookmarks: Vec<String>) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    log::info!("Publishing {} bookmarks", bookmarks.len());

    // Parse event IDs with better error messages
    use nostr_sdk::EventId;
    let mut event_ids = Vec::new();
    for id in bookmarks.into_iter() {
        match EventId::from_hex(&id) {
            Ok(event_id) => event_ids.push(event_id),
            Err(e) => {
                return Err(format!("Invalid event ID '{}': {}", id, e));
            }
        }
    }

    // Use NIP-51 Bookmarks struct for type-safe bookmark list construction
    use nostr_sdk::nips::nip51::Bookmarks;
    let bookmarks_list = Bookmarks {
        event_ids,
        coordinate: Vec::new(),
        hashtags: Vec::new(),
        urls: Vec::new(),
    };

    // Use EventBuilder::bookmarks_set() for proper NIP-51 compliance
    // This automatically adds the 'd' tag and properly formats all bookmark entries
    let builder = EventBuilder::bookmarks_set("bookmark", bookmarks_list);

    match client.send_event_builder(builder).await {
        Ok(_) => {
            log::info!("Bookmarks published successfully");
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
pub async fn fetch_bookmarked_events_paginated(skip: usize, limit: Option<usize>) -> Result<Vec<Event>, String> {
    let bookmarks = BOOKMARKED_EVENTS.read().data().read().clone();

    if bookmarks.is_empty() {
        return Ok(Vec::new());
    }

    // Apply skip and limit to bookmark IDs
    let bookmarks_slice = if skip >= bookmarks.len() {
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

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Create filter for bookmarked events
    let event_ids: Result<Vec<nostr_sdk::EventId>, _> = bookmarks_slice
        .iter()
        .map(|id| nostr_sdk::EventId::from_hex(id))
        .collect();

    let event_ids = event_ids.map_err(|e| format!("Invalid event ID: {}", e))?;

    let filter = Filter::new().ids(event_ids);

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    match client.fetch_events(filter, Duration::from_secs(15)).await {
        Ok(events) => {
            let mut event_vec: Vec<Event> = events.into_iter().collect();
            // Sort by created_at descending (newest first)
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            log::info!("Fetched {} bookmarked events (skip: {}, limit: {:?})", event_vec.len(), skip, limit);
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
    BOOKMARKED_EVENTS.read().data().read().len()
}
