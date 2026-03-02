//! Pinned Communities Store
//! Manages user's pinned communities with NIP-51 kind 10004 (Communities list)
//!
//! Features:
//! - Optimistic UI updates
//! - Debounced publishing to relays
//! - Automatic retry with exponential backoff
//! - Rollback on publish failure
use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_stores::Store;
use nostr_sdk::nips::nip01::Coordinate;
use nostr_sdk::{EventBuilder, Filter, Kind, PublicKey};
use std::collections::HashSet;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

#[allow(dead_code)]
static GENERATION_COUNTER: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "web")]
use gloo_timers::callback::Timeout;
#[cfg(feature = "web")]
use std::cell::RefCell;
#[cfg(feature = "web")]
use wasm_bindgen_futures::spawn_local;
use super::community_store::KIND_COMMUNITY_DEFINITION;
/// Store for pinned community a_tags with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct PinnedCommunitiesStore {
    pub data: Vec<String>,
}
/// Store for pinned communities rollback state with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct PinnedCommunitiesRollbackStore {
    pub data: Option<Vec<String>>,
}
/// Global signal to track pinned community a_tags (current user's pins)
pub static PINNED_COMMUNITIES: GlobalSignal<Store<PinnedCommunitiesStore>> = Signal::global(||
Store::new(PinnedCommunitiesStore::default()));
/// Sync status for pinned communities publishing
#[derive(Clone, Debug, PartialEq)]
pub enum PinnedCommunitiesSyncStatus {
    /// No pending operations
    Idle,
    /// Publishing to relays in progress
    Syncing,
    /// Publish failed with error message and retry count
    Failed { error: String, retry_count: u32 },
}
/// Global signal to track pinned communities sync status
pub static PINNED_COMMUNITIES_SYNC_STATUS: GlobalSignal<PinnedCommunitiesSyncStatus> = Signal::global(||
PinnedCommunitiesSyncStatus::Idle);
/// Previous pinned communities state for rollback on failure
pub static PINNED_COMMUNITIES_ROLLBACK: GlobalSignal<
    Store<PinnedCommunitiesRollbackStore>,
> = Signal::global(|| Store::new(PinnedCommunitiesRollbackStore::default()));
#[cfg(feature = "web")]
thread_local! {
    /// Pending pinned communities publish timeout (for debouncing)
    static PINNED_COMMUNITIES_TIMEOUT: RefCell<Option<Timeout>> = const {
        RefCell::new(None)
    };
}
/// Initialize pinned communities by fetching from relays for the current user
pub async fn init_pinned_communities() -> Result<(), String> {
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    log::info!("Loading pinned communities for {}", pubkey_str);
    let filter = Filter::new().author(pubkey).kind(Kind::Communities).limit(1);
    nostr_client::ensure_relays_ready(&client).await;
    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                let pinned: Vec<String> = event
                    .tags
                    .iter()
                    .filter(|t| t.kind() == nostr_sdk::TagKind::a())
                    .filter_map(|t| t.content().map(|s| s.to_string()))
                    .collect();
                log::info!("Loaded {} pinned communities", pinned.len());
                *PINNED_COMMUNITIES.read().data().write() = pinned;
                Ok(())
            } else {
                log::info!("No pinned communities found");
                *PINNED_COMMUNITIES.read().data().write() = Vec::new();
                Ok(())
            }
        }
        Err(e) => {
            log::error!("Failed to fetch pinned communities: {}", e);
            Err(format!("Failed to fetch pinned communities: {}", e))
        }
    }
}
/// Check if a community is pinned by the current user
pub fn is_community_pinned(a_tag: &str) -> bool {
    PINNED_COMMUNITIES.read().data().read().contains(&a_tag.to_string())
}
/// Get all pinned community a_tags
pub fn get_pinned_communities() -> Vec<String> {
    PINNED_COMMUNITIES.read().data().read().clone()
}
/// Get pinned communities as a HashSet for efficient lookup
pub fn get_pinned_communities_set() -> HashSet<String> {
    PINNED_COMMUNITIES.read().data().read().iter().cloned().collect()
}
fn schedule_debounced_publish(pins: Vec<String>) {
    #[cfg(feature = "web")]
    {
        use std::sync::atomic::Ordering;
        // fetch_add returns previous value, so add 1 to get the new generation
        let captured_gen = GENERATION_COUNTER.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        PINNED_COMMUNITIES_TIMEOUT
            .with(|timeout| {
                *timeout.borrow_mut() = None;
                let timeout_handle = Timeout::new(
                    1000,
                    move || {
                        spawn_local(async move {
                            publish_with_retry(pins, captured_gen, 0).await;
                        });
                    },
                );
                *timeout.borrow_mut() = Some(timeout_handle);
            });
    }
    #[cfg(not(feature = "web"))]
    {
        use std::sync::atomic::Ordering;
        // fetch_add returns previous value, so add 1 to get the new generation
        let captured_gen = GENERATION_COUNTER.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        let delay_ms = 1000;
        crate::platform::spawn::spawn_detached(async move {
            crate::platform::timer::sleep_ms(delay_ms).await;
            // Only publish if generation hasn't changed (debounce protects against stale calls)
            if captured_gen == GENERATION_COUNTER.load(Ordering::SeqCst) {
                publish_with_retry(pins, captured_gen, 0).await;
            }
        });
    }
}
/// Pin a community
pub async fn pin_community(a_tag: String) -> Result<(), String> {
    let mut pins = PINNED_COMMUNITIES.read().data().read().clone();
    if pins.contains(&a_tag) {
        return Ok(());
    }
    if PINNED_COMMUNITIES_ROLLBACK.read().data().read().is_none() {
        *PINNED_COMMUNITIES_ROLLBACK.read().data().write() = Some(pins.clone());
    }
    pins.push(a_tag);
    *PINNED_COMMUNITIES.read().data().write() = pins.clone();
    schedule_debounced_publish(pins);
    Ok(())
}
/// Unpin a community
pub async fn unpin_community(a_tag: String) -> Result<(), String> {
    let mut pins = PINNED_COMMUNITIES.read().data().read().clone();
    if PINNED_COMMUNITIES_ROLLBACK.read().data().read().is_none() {
        *PINNED_COMMUNITIES_ROLLBACK.read().data().write() = Some(pins.clone());
    }
    pins.retain(|tag| tag != &a_tag);
    *PINNED_COMMUNITIES.read().data().write() = pins.clone();
    schedule_debounced_publish(pins);
    Ok(())
}
/// Publish pinned communities with retry and exponential backoff
#[cfg(not(target_arch = "wasm32"))]
fn publish_with_retry(
    pins: Vec<String>,
    captured_gen: u64,
    retry_count: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
    Box::pin(async move {
        use std::sync::atomic::Ordering;
        // Short-circuit if this generation is stale
        if captured_gen != GENERATION_COUNTER.load(Ordering::SeqCst) {
            return;
        }

        const MAX_RETRIES: u32 = 3;
        *PINNED_COMMUNITIES_SYNC_STATUS.write() = PinnedCommunitiesSyncStatus::Syncing;
        match publish_pinned_communities(pins.clone()).await {
            Ok(_) => {
                *PINNED_COMMUNITIES_ROLLBACK.read().data().write() = None;
                *PINNED_COMMUNITIES_SYNC_STATUS.write() = PinnedCommunitiesSyncStatus::Idle;
                log::info!("Pinned communities published successfully");
            }
            Err(e) => {
                log::error!(
                    "Failed to publish pinned communities (attempt {}): {}", retry_count
                    + 1, e
                );
                if retry_count < MAX_RETRIES {
                    let delay_ms = 1000u32 * (1 << retry_count);
                    log::info!(
                        "Retrying pinned communities publish in {}ms (attempt {}/{})",
                        delay_ms, retry_count + 1, MAX_RETRIES
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms as u64)).await;
                    publish_with_retry(pins, captured_gen, retry_count + 1).await;
                } else {
                    log::error!(
                        "Pinned communities publish failed after {} retries: {}",
                        MAX_RETRIES, e
                    );
                    if let Some(previous_state) = PINNED_COMMUNITIES_ROLLBACK
                        .read()
                        .data()
                        .read()
                        .clone()
                    {
                        log::warn!(
                            "Automatically rolling back pinned communities to previous state due to publish failure"
                        );
                        *PINNED_COMMUNITIES.read().data().write() = previous_state;
                    }
                    *PINNED_COMMUNITIES_ROLLBACK.read().data().write() = None;
                    *PINNED_COMMUNITIES_SYNC_STATUS.write() = PinnedCommunitiesSyncStatus::Failed {
                        error: e.clone(),
                        retry_count,
                    };
                }
            }
        }
    })
}

/// Publish pinned communities with retry and exponential backoff (WASM version - no Send bound)
#[cfg(target_arch = "wasm32")]
fn publish_with_retry(
    pins: Vec<String>,
    captured_gen: u64,
    retry_count: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'static>> {
    Box::pin(async move {
        use std::sync::atomic::Ordering;
        // Short-circuit if this generation is stale
        if captured_gen != GENERATION_COUNTER.load(Ordering::SeqCst) {
            return;
        }

        const MAX_RETRIES: u32 = 3;
        *PINNED_COMMUNITIES_SYNC_STATUS.write() = PinnedCommunitiesSyncStatus::Syncing;
        match publish_pinned_communities(pins.clone()).await {
            Ok(_) => {
                *PINNED_COMMUNITIES_ROLLBACK.read().data().write() = None;
                *PINNED_COMMUNITIES_SYNC_STATUS.write() = PinnedCommunitiesSyncStatus::Idle;
                log::info!("Pinned communities published successfully");
            }
            Err(e) => {
                log::error!(
                    "Failed to publish pinned communities (attempt {}): {}", retry_count
                    + 1, e
                );
                if retry_count < MAX_RETRIES {
                    let delay_ms = 1000u32 * (1 << retry_count);
                    log::info!(
                        "Retrying pinned communities publish in {}ms (attempt {}/{})",
                        delay_ms, retry_count + 1, MAX_RETRIES
                    );
                    let timeout_handle = Timeout::new(
                        delay_ms,
                        move || {
                            spawn_local(publish_with_retry(pins, captured_gen, retry_count + 1));
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
                        "Pinned communities publish failed after {} retries: {}",
                        MAX_RETRIES, e
                    );
                    if let Some(previous_state) = PINNED_COMMUNITIES_ROLLBACK
                        .read()
                        .data()
                        .read()
                        .clone()
                    {
                        log::warn!(
                            "Automatically rolling back pinned communities to previous state due to publish failure"
                        );
                        *PINNED_COMMUNITIES.read().data().write() = previous_state;
                    }
                    *PINNED_COMMUNITIES_ROLLBACK.read().data().write() = None;
                    *PINNED_COMMUNITIES_SYNC_STATUS.write() = PinnedCommunitiesSyncStatus::Failed {
                        error: e.clone(),
                        retry_count,
                    };
                }
            }
        }
})
}
/// Publish pinned communities list to relays (NIP-51 kind 10004)
async fn publish_pinned_communities(pins: Vec<String>) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }
    log::info!("Publishing {} pinned communities", pins.len());
    let coordinates: Vec<Coordinate> = pins
        .iter()
        .filter_map(|a_tag| parse_a_tag_to_coordinate(a_tag))
        .collect();
    let builder = EventBuilder::communities(coordinates);
    match client.send_event_builder(builder).await {
        Ok(_) => {
            log::info!("Pinned communities published successfully");
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to publish pinned communities: {}", e);
            Err(format!("Failed to publish pinned communities: {}", e))
        }
    }
}
/// Parse a_tag string to Coordinate
/// Format: "34550:pubkey:d_tag"
fn parse_a_tag_to_coordinate(a_tag: &str) -> Option<Coordinate> {
    let parts: Vec<&str> = a_tag.splitn(3, ':').collect();
    if parts.len() != 3 {
        return None;
    }
    let kind_num: u16 = parts[0].parse().ok()?;
    if kind_num != KIND_COMMUNITY_DEFINITION {
        return None;
    }
    let pubkey = PublicKey::from_hex(parts[1]).ok()?;
    let identifier = parts[2];
    Some(Coordinate::new(Kind::Custom(kind_num), pubkey).identifier(identifier))
}
/// Rollback pinned communities to previous state after failed publish
#[allow(dead_code)]
pub fn rollback_pinned_communities() {
    if let Some(previous_state) = PINNED_COMMUNITIES_ROLLBACK
        .read()
        .data()
        .read()
        .clone()
    {
        log::info!("Rolling back pinned communities to previous state");
        *PINNED_COMMUNITIES.read().data().write() = previous_state;
        *PINNED_COMMUNITIES_ROLLBACK.read().data().write() = None;
        *PINNED_COMMUNITIES_SYNC_STATUS.write() = PinnedCommunitiesSyncStatus::Idle;
    } else {
        log::warn!("No rollback state available");
    }
}
/// Manually retry failed pinned communities publish
#[allow(dead_code)]
pub async fn retry_pinned_communities_publish() {
    use std::sync::atomic::Ordering;
    let current_pins = PINNED_COMMUNITIES.read().data().read().clone();
    log::info!("Manually retrying pinned communities publish");
    // fetch_add returns previous value, so add 1 to get the new generation
    let captured_gen = GENERATION_COUNTER.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    publish_with_retry(current_pins, captured_gen, 0).await;
}
/// Dismiss failed status and keep local changes
#[allow(dead_code)]
pub fn dismiss_pinned_communities_error() {
    log::info!("Dismissing pinned communities sync error, keeping local changes");
    *PINNED_COMMUNITIES_ROLLBACK.read().data().write() = None;
    *PINNED_COMMUNITIES_SYNC_STATUS.write() = PinnedCommunitiesSyncStatus::Idle;
}
/// Get the total number of pinned communities
#[allow(dead_code)]
pub fn get_pinned_communities_count() -> usize {
    PINNED_COMMUNITIES.read().data().read().len()
}
