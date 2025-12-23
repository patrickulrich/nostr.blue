use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_stores::Store;
use nostr_sdk::{Event, Filter, Kind, EventBuilder, EventId, PublicKey};
use crate::stores::{auth_store, nostr_client};
use std::time::Duration;

#[cfg(target_arch = "wasm32")]
use gloo_timers::callback::Timeout;
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

/// Store for pinned event IDs with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct PinnedEventsStore {
    pub data: Vec<String>,
}

/// Store for pinned notes rollback state with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct PinnedRollbackStore {
    pub data: Option<Vec<String>>,
}

/// Global signal to track pinned event IDs (current user's pins)
pub static PINNED_EVENTS: GlobalSignal<Store<PinnedEventsStore>> =
    Signal::global(|| Store::new(PinnedEventsStore::default()));

/// Sync status for pinned notes publishing
#[derive(Clone, Debug, PartialEq)]
pub enum PinnedSyncStatus {
    /// No pending operations
    Idle,
    /// Publishing to relays in progress
    Syncing,
    /// Publish failed with error message and retry count
    Failed { error: String, retry_count: u32 },
}

/// Global signal to track pinned notes sync status
pub static PINNED_SYNC_STATUS: GlobalSignal<PinnedSyncStatus> =
    Signal::global(|| PinnedSyncStatus::Idle);

/// Previous pinned notes state for rollback on failure
pub static PINNED_ROLLBACK_STATE: GlobalSignal<Store<PinnedRollbackStore>> =
    Signal::global(|| Store::new(PinnedRollbackStore::default()));

#[cfg(target_arch = "wasm32")]
thread_local! {
    /// Pending pinned notes publish timeout (for debouncing)
    static PINNED_PUBLISH_TIMEOUT: RefCell<Option<Timeout>> = RefCell::new(None);
}

/// Initialize pinned notes by fetching from relays for the current user
pub async fn init_pinned_notes() -> Result<(), String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Loading pinned notes for {}", pubkey_str);

    // Fetch pinned notes list (kind 10001 - PinList)
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::PinList)
        .limit(1);

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                // Extract event IDs from 'e' tags using SDK helper
                let pinned: Vec<String> = event.tags.event_ids()
                    .map(|id| id.to_hex())
                    .collect();

                log::info!("Loaded {} pinned notes", pinned.len());
                *PINNED_EVENTS.read().data().write() = pinned;
                Ok(())
            } else {
                log::info!("No pinned notes found");
                *PINNED_EVENTS.read().data().write() = Vec::new();
                Ok(())
            }
        }
        Err(e) => {
            log::error!("Failed to fetch pinned notes: {}", e);
            Err(format!("Failed to fetch pinned notes: {}", e))
        }
    }
}

/// Fetch pinned notes for any user (returns pin IDs and the actual events)
pub async fn fetch_pinned_notes_for_user(pubkey_str: &str) -> Result<(Vec<String>, Vec<Event>), String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey = PublicKey::parse(pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Fetch pinned notes list (kind 10001 - PinList)
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::PinList)
        .limit(1);

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    let events = client.fetch_events(filter, Duration::from_secs(10)).await
        .map_err(|e| format!("Failed to fetch pinned notes list: {}", e))?;

    let pin_list = events.into_iter().next();

    if let Some(list_event) = pin_list {
        // Extract event IDs in order (preserving pin order)
        let pin_ids: Vec<String> = list_event.tags.event_ids()
            .map(|id| id.to_hex())
            .collect();

        if pin_ids.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }

        // Fetch the actual pinned events
        let event_ids: Vec<EventId> = pin_ids.iter()
            .filter_map(|id| EventId::from_hex(id).ok())
            .collect();

        let events_filter = Filter::new()
            .ids(event_ids)
            .limit(20);

        let pinned_events = client.fetch_events(events_filter, Duration::from_secs(10)).await
            .map_err(|e| format!("Failed to fetch pinned events: {}", e))?;

        Ok((pin_ids, pinned_events.into_iter().collect()))
    } else {
        Ok((Vec::new(), Vec::new()))
    }
}

/// Check if an event is pinned by the current user
pub fn is_pinned(event_id: &str) -> bool {
    PINNED_EVENTS.read().data().read().contains(&event_id.to_string())
}

/// Add event to pinned notes
pub async fn pin_event(event_id: String) -> Result<(), String> {
    // Validate event ID early to prevent invalid IDs from being stored
    EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID '{}': {}", event_id, e))?;

    let mut pins = PINNED_EVENTS.read().data().read().clone();

    // Don't add if already pinned
    if pins.contains(&event_id) {
        return Ok(());
    }

    // Store rollback state before making changes (preserve initial state for batch)
    if PINNED_ROLLBACK_STATE.read().data().read().is_none() {
        *PINNED_ROLLBACK_STATE.read().data().write() = Some(pins.clone());
    }

    pins.push(event_id);

    // Update local state immediately for UI responsiveness
    *PINNED_EVENTS.read().data().write() = pins.clone();

    // Debounce relay publish (batches rapid pins into one publish)
    #[cfg(target_arch = "wasm32")]
    {
        PINNED_PUBLISH_TIMEOUT.with(|timeout| {
            // Cancel any existing timeout
            *timeout.borrow_mut() = None;

            // Schedule new publish after 1 second
            // Use spawn_local instead of Dioxus spawn - timer callbacks don't have runtime context
            let timeout_handle = Timeout::new(1000, move || {
                spawn_local(async move {
                    publish_with_retry(pins, 0).await;
                });
            });

            *timeout.borrow_mut() = Some(timeout_handle);
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // Non-WASM: publish immediately with retry
        publish_with_retry(pins, 0).await;
    }

    Ok(())
}

/// Remove event from pinned notes
pub async fn unpin_event(event_id: String) -> Result<(), String> {
    let mut pins = PINNED_EVENTS.read().data().read().clone();

    // Store rollback state before making changes (preserve initial state for batch)
    if PINNED_ROLLBACK_STATE.read().data().read().is_none() {
        *PINNED_ROLLBACK_STATE.read().data().write() = Some(pins.clone());
    }

    // Remove the event ID
    pins.retain(|id| id != &event_id);

    // Update local state immediately for UI responsiveness
    *PINNED_EVENTS.read().data().write() = pins.clone();

    // Debounce relay publish (batches rapid unpins into one publish)
    #[cfg(target_arch = "wasm32")]
    {
        PINNED_PUBLISH_TIMEOUT.with(|timeout| {
            // Cancel any existing timeout
            *timeout.borrow_mut() = None;

            // Schedule new publish after 1 second
            // Use spawn_local instead of Dioxus spawn - timer callbacks don't have runtime context
            let timeout_handle = Timeout::new(1000, move || {
                spawn_local(async move {
                    publish_with_retry(pins, 0).await;
                });
            });

            *timeout.borrow_mut() = Some(timeout_handle);
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // Non-WASM: publish immediately with retry
        publish_with_retry(pins, 0).await;
    }

    Ok(())
}

/// Publish pinned notes with retry and exponential backoff
fn publish_with_retry(pins: Vec<String>, retry_count: u32) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'static>> {
    Box::pin(async move {
        const MAX_RETRIES: u32 = 3;

        // Set status to syncing
        *PINNED_SYNC_STATUS.write() = PinnedSyncStatus::Syncing;

        match publish_pinned_notes(pins.clone()).await {
            Ok(_) => {
                // Success - clear rollback state and set status to idle
                *PINNED_ROLLBACK_STATE.read().data().write() = None;
                *PINNED_SYNC_STATUS.write() = PinnedSyncStatus::Idle;
                log::info!("Pinned notes published successfully");
            }
            Err(e) => {
                log::error!("Failed to publish pinned notes (attempt {}): {}", retry_count + 1, e);

                if retry_count < MAX_RETRIES {
                    // Calculate exponential backoff delay: 1s, 2s, 4s
                    let delay_ms = 1000u32 * (1 << retry_count); // 2^retry_count seconds

                    log::info!("Retrying pinned notes publish in {}ms (attempt {}/{})",
                        delay_ms, retry_count + 1, MAX_RETRIES);

                    // Schedule retry with exponential backoff
                    // Use spawn_local instead of Dioxus spawn - timer callbacks don't have runtime context
                    #[cfg(target_arch = "wasm32")]
                    {
                        let timeout_handle = Timeout::new(delay_ms, move || {
                            spawn_local(publish_with_retry(pins, retry_count + 1));
                        });
                        // Note: We let the timeout run and don't store it since it's a retry
                        std::mem::forget(timeout_handle);
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        tokio::time::sleep(Duration::from_millis(delay_ms as u64)).await;
                        publish_with_retry(pins, retry_count + 1).await;
                    }
                } else {
                    // Max retries exceeded - rollback local state and set failed status
                    log::error!("Pinned notes publish failed after {} retries: {}", MAX_RETRIES, e);

                    // Rollback local state to match persisted state
                    if let Some(previous_state) = PINNED_ROLLBACK_STATE.read().data().read().clone() {
                        log::warn!("Automatically rolling back pinned notes to previous state due to publish failure");
                        *PINNED_EVENTS.read().data().write() = previous_state;
                    }

                    // Set failed status (rollback state is cleared here)
                    *PINNED_ROLLBACK_STATE.read().data().write() = None;
                    *PINNED_SYNC_STATUS.write() = PinnedSyncStatus::Failed {
                        error: e.clone(),
                        retry_count,
                    };
                }
            }
        }
    })
}

/// Publish pinned notes list to relays (NIP-51 kind 10001)
async fn publish_pinned_notes(pins: Vec<String>) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    log::info!("Publishing {} pinned notes", pins.len());

    // Parse event IDs
    let event_ids: Result<Vec<EventId>, _> = pins.iter()
        .map(|id| EventId::from_hex(id))
        .collect();

    let event_ids = event_ids.map_err(|e| format!("Invalid event ID: {}", e))?;

    // Use EventBuilder::pinned_notes() - the SDK's dedicated builder method
    // This handles Kind::PinList and Tag::event() internally
    let builder = EventBuilder::pinned_notes(event_ids);

    match client.send_event_builder(builder).await {
        Ok(_) => {
            log::info!("Pinned notes published successfully");
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to publish pinned notes: {}", e);
            Err(format!("Failed to publish pinned notes: {}", e))
        }
    }
}

/// Rollback pinned notes to previous state after failed publish
#[allow(dead_code)]
pub fn rollback_pinned_notes() {
    if let Some(previous_state) = PINNED_ROLLBACK_STATE.read().data().read().clone() {
        log::info!("Rolling back pinned notes to previous state");
        *PINNED_EVENTS.read().data().write() = previous_state;
        *PINNED_ROLLBACK_STATE.read().data().write() = None;
        *PINNED_SYNC_STATUS.write() = PinnedSyncStatus::Idle;
    } else {
        log::warn!("No rollback state available");
    }
}

/// Manually retry failed pinned notes publish
#[allow(dead_code)]
pub async fn retry_pinned_publish() {
    let current_pins = PINNED_EVENTS.read().data().read().clone();
    log::info!("Manually retrying pinned notes publish");
    publish_with_retry(current_pins, 0).await;
}

/// Dismiss failed status and keep local changes
#[allow(dead_code)]
pub fn dismiss_pinned_error() {
    log::info!("Dismissing pinned notes sync error, keeping local changes");
    *PINNED_ROLLBACK_STATE.read().data().write() = None;
    *PINNED_SYNC_STATUS.write() = PinnedSyncStatus::Idle;
}

/// Get the total number of pinned notes
#[allow(dead_code)]
pub fn get_pinned_count() -> usize {
    PINNED_EVENTS.read().data().read().len()
}
