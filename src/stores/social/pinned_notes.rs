use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_stores::Store;
use nostr_sdk::{Event, EventBuilder, EventId, Filter, Kind, PublicKey};
use std::time::Duration;
#[cfg(feature = "web")]
use gloo_timers::callback::Timeout;
#[cfg(feature = "web")]
use std::cell::RefCell;
#[cfg(feature = "web")]
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
pub static PINNED_EVENTS: GlobalSignal<Store<PinnedEventsStore>> = Signal::global(|| Store::new(
    PinnedEventsStore::default(),
));
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
pub static PINNED_SYNC_STATUS: GlobalSignal<PinnedSyncStatus> = Signal::global(|| {
    PinnedSyncStatus::Idle
});
/// Previous pinned notes state for rollback on failure
pub static PINNED_ROLLBACK_STATE: GlobalSignal<Store<PinnedRollbackStore>> = Signal::global(||
Store::new(PinnedRollbackStore::default()));
#[cfg(feature = "web")]
thread_local! {
    /// Pending pinned notes publish timeout (for debouncing)
    static PINNED_PUBLISH_TIMEOUT: RefCell<Option<Timeout>> = const {
        RefCell::new(None)
    };
}
/// Initialize pinned notes by fetching from relays for the current user
pub async fn init_pinned_notes() -> Result<(), String> {
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    log::info!("Loading pinned notes for {}", pubkey_str);
    let filter = Filter::new().author(pubkey).kind(Kind::PinList).limit(1);
    nostr_client::ensure_relays_ready(&client).await;
    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                let pinned: Vec<String> = event
                    .tags
                    .event_ids()
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
pub async fn fetch_pinned_notes_for_user(
    pubkey_str: &str,
) -> Result<(Vec<String>, Vec<Event>), String> {
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let pubkey = PublicKey::parse(pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new().author(pubkey).kind(Kind::PinList).limit(1);
    nostr_client::ensure_relays_ready(&client).await;
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch pinned notes list: {}", e))?;
    let pin_list = events.into_iter().next();
    if let Some(list_event) = pin_list {
        let pin_ids: Vec<String> = list_event
            .tags
            .event_ids()
            .map(|id| id.to_hex())
            .collect();
        if pin_ids.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let event_ids: Vec<EventId> = pin_ids
            .iter()
            .filter_map(|id| EventId::from_hex(id).ok())
            .collect();
        let events_filter = Filter::new().ids(event_ids).limit(20);
        let pinned_events = client
            .fetch_events(events_filter, Duration::from_secs(10))
            .await
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
    EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID '{}': {}", event_id, e))?;
    let mut pins = PINNED_EVENTS.read().data().read().clone();
    if pins.contains(&event_id) {
        return Ok(());
    }
    if PINNED_ROLLBACK_STATE.read().data().read().is_none() {
        *PINNED_ROLLBACK_STATE.read().data().write() = Some(pins.clone());
    }
    pins.push(event_id);
    *PINNED_EVENTS.read().data().write() = pins.clone();
    #[cfg(feature = "web")]
    {
        PINNED_PUBLISH_TIMEOUT
            .with(|timeout| {
                *timeout.borrow_mut() = None;
                let timeout_handle = Timeout::new(
                    1000,
                    move || {
                        spawn_local(async move {
                            publish_with_retry(pins, 0).await;
                        });
                    },
                );
                *timeout.borrow_mut() = Some(timeout_handle);
            });
    }
    #[cfg(feature = "native")]
    {
        publish_with_retry(pins, 0).await;
    }
    Ok(())
}
/// Remove event from pinned notes
pub async fn unpin_event(event_id: String) -> Result<(), String> {
    let mut pins = PINNED_EVENTS.read().data().read().clone();
    if PINNED_ROLLBACK_STATE.read().data().read().is_none() {
        *PINNED_ROLLBACK_STATE.read().data().write() = Some(pins.clone());
    }
    pins.retain(|id| id != &event_id);
    *PINNED_EVENTS.read().data().write() = pins.clone();
    #[cfg(feature = "web")]
    {
        let pins_for_timeout = pins.clone();
        PINNED_PUBLISH_TIMEOUT
            .with(|timeout| {
                *timeout.borrow_mut() = None;
                let timeout_handle = Timeout::new(
                    1000,
                    move || {
                        spawn_local(async move {
                            publish_with_retry(pins_for_timeout, 0).await;
                        });
                    },
                );
                *timeout.borrow_mut() = Some(timeout_handle);
            });
    }
    if cfg!(not(feature = "web")) {
        publish_with_retry(pins, 0).await;
    }
    Ok(())
}
/// Publish pinned notes with retry and exponential backoff (native - requires Send)
#[cfg(feature = "native")]
fn publish_with_retry(
    pins: Vec<String>,
    retry_count: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
    Box::pin(async move {
        const MAX_RETRIES: u32 = 3;
        *PINNED_SYNC_STATUS.write() = PinnedSyncStatus::Syncing;
        match publish_pinned_notes(pins.clone()).await {
            Ok(_) => {
                *PINNED_ROLLBACK_STATE.read().data().write() = None;
                *PINNED_SYNC_STATUS.write() = PinnedSyncStatus::Idle;
                log::info!("Pinned notes published successfully");
            }
            Err(e) => {
                log::error!(
                    "Failed to publish pinned notes (attempt {}): {}", retry_count + 1, e
                );
                if retry_count < MAX_RETRIES {
                    let delay_ms = 1000u32 * (1 << retry_count);
                    log::info!(
                        "Retrying pinned notes publish in {}ms (attempt {}/{})",
                        delay_ms, retry_count + 1, MAX_RETRIES
                    );
                    crate::platform::timer::sleep_ms(delay_ms).await;
                    publish_with_retry(pins, retry_count + 1).await;
                } else {
                    log::error!(
                        "Pinned notes publish failed after {} retries: {}", MAX_RETRIES,
                        e
                    );
                    if let Some(previous_state) = PINNED_ROLLBACK_STATE
                        .read()
                        .data()
                        .read()
                        .clone()
                    {
                        log::warn!(
                            "Automatically rolling back pinned notes to previous state due to publish failure"
                        );
                        *PINNED_EVENTS.read().data().write() = previous_state;
                    }
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

/// Publish pinned notes with retry and exponential backoff (WASM - no Send bound)
#[cfg(feature = "web")]
fn publish_with_retry(
    pins: Vec<String>,
    retry_count: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'static>> {
    Box::pin(async move {
        const MAX_RETRIES: u32 = 3;
        *PINNED_SYNC_STATUS.write() = PinnedSyncStatus::Syncing;
        match publish_pinned_notes(pins.clone()).await {
            Ok(_) => {
                *PINNED_ROLLBACK_STATE.read().data().write() = None;
                *PINNED_SYNC_STATUS.write() = PinnedSyncStatus::Idle;
                log::info!("Pinned notes published successfully");
            }
            Err(e) => {
                log::error!(
                    "Failed to publish pinned notes (attempt {}): {}", retry_count + 1, e
                );
                if retry_count < MAX_RETRIES {
                    let delay_ms = 1000u32 * (1 << retry_count);
                    log::info!(
                        "Retrying pinned notes publish in {}ms (attempt {}/{})",
                        delay_ms, retry_count + 1, MAX_RETRIES
                    );
                    let timeout_handle = Timeout::new(
                        delay_ms,
                        move || {
                            spawn_local(publish_with_retry(pins, retry_count + 1));
                        },
                    );
                    // Intentionally forget the timeout handle to prevent the scheduled retry
                    // from being cancelled. When a Timeout is dropped, it cancels the callback.
                    // We use fire-and-forget here so the retry runs even after this scope ends.
                    // This is a deliberate WASM pattern - the small memory leak is acceptable
                    // because the callback runs once and the module lifetime is the app lifetime.
                    std::mem::forget(timeout_handle);
                } else {
                    log::error!(
                        "Pinned notes publish failed after {} retries: {}", MAX_RETRIES,
                        e
                    );
                    if let Some(previous_state) = PINNED_ROLLBACK_STATE
                        .read()
                        .data()
                        .read()
                        .clone()
                    {
                        log::warn!(
                            "Automatically rolling back pinned notes to previous state due to publish failure"
                        );
                        *PINNED_EVENTS.read().data().write() = previous_state;
                    }
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
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    log::info!("Publishing {} pinned notes", pins.len());
    let event_ids: Result<Vec<EventId>, _> = pins
        .iter()
        .map(|id| EventId::from_hex(id))
        .collect();
    let event_ids = event_ids.map_err(|e| format!("Invalid event ID: {}", e))?;
    let builder = EventBuilder::pinned_notes(event_ids);
    match client.send_event_builder(builder).await {
        Ok(output) => {
            let success_count = output.success.len();
            let failed_count = output.failed.len();
            let total = success_count + failed_count;
            log::info!(
                "Pinned notes published: {} ({}/{} relays succeeded)", output.id()
                .to_hex(), success_count, total
            );
            if !output.failed.is_empty() {
                for (relay, error) in &output.failed {
                    log::warn!("Relay {} failed: {}", relay, error);
                }
            }
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
