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
use std::time::Duration;

#[cfg(target_arch = "wasm32")]
use gloo_timers::callback::Timeout;
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
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
pub static PINNED_COMMUNITIES: GlobalSignal<Store<PinnedCommunitiesStore>> =
    Signal::global(|| Store::new(PinnedCommunitiesStore::default()));

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
pub static PINNED_COMMUNITIES_SYNC_STATUS: GlobalSignal<PinnedCommunitiesSyncStatus> =
    Signal::global(|| PinnedCommunitiesSyncStatus::Idle);

/// Previous pinned communities state for rollback on failure
pub static PINNED_COMMUNITIES_ROLLBACK: GlobalSignal<Store<PinnedCommunitiesRollbackStore>> =
    Signal::global(|| Store::new(PinnedCommunitiesRollbackStore::default()));

#[cfg(target_arch = "wasm32")]
thread_local! {
    /// Pending pinned communities publish timeout (for debouncing)
    static PINNED_COMMUNITIES_TIMEOUT: RefCell<Option<Timeout>> = const { RefCell::new(None) };
}

/// Initialize pinned communities by fetching from relays for the current user
pub async fn init_pinned_communities() -> Result<(), String> {
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Loading pinned communities for {}", pubkey_str);

    // Fetch communities list (kind 10004 - Communities)
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::Communities)
        .limit(1);

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                // Extract a_tags from coordinate tags
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
    PINNED_COMMUNITIES
        .read()
        .data()
        .read()
        .contains(&a_tag.to_string())
}

/// Get all pinned community a_tags
pub fn get_pinned_communities() -> Vec<String> {
    PINNED_COMMUNITIES.read().data().read().clone()
}

/// Get pinned communities as a HashSet for efficient lookup
pub fn get_pinned_communities_set() -> HashSet<String> {
    PINNED_COMMUNITIES
        .read()
        .data()
        .read()
        .iter()
        .cloned()
        .collect()
}

/// Pin a community
pub async fn pin_community(a_tag: String) -> Result<(), String> {
    let mut pins = PINNED_COMMUNITIES.read().data().read().clone();

    // Don't add if already pinned
    if pins.contains(&a_tag) {
        return Ok(());
    }

    // Store rollback state before making changes (preserve initial state for batch)
    if PINNED_COMMUNITIES_ROLLBACK.read().data().read().is_none() {
        *PINNED_COMMUNITIES_ROLLBACK.read().data().write() = Some(pins.clone());
    }

    pins.push(a_tag);

    // Update local state immediately for UI responsiveness
    *PINNED_COMMUNITIES.read().data().write() = pins.clone();

    // Debounce relay publish (batches rapid pins into one publish)
    #[cfg(target_arch = "wasm32")]
    {
        PINNED_COMMUNITIES_TIMEOUT.with(|timeout| {
            // Cancel any existing timeout
            *timeout.borrow_mut() = None;

            // Schedule new publish after 1 second
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

/// Unpin a community
pub async fn unpin_community(a_tag: String) -> Result<(), String> {
    let mut pins = PINNED_COMMUNITIES.read().data().read().clone();

    // Store rollback state before making changes (preserve initial state for batch)
    if PINNED_COMMUNITIES_ROLLBACK.read().data().read().is_none() {
        *PINNED_COMMUNITIES_ROLLBACK.read().data().write() = Some(pins.clone());
    }

    // Remove the a_tag
    pins.retain(|tag| tag != &a_tag);

    // Update local state immediately for UI responsiveness
    *PINNED_COMMUNITIES.read().data().write() = pins.clone();

    // Debounce relay publish (batches rapid unpins into one publish)
    #[cfg(target_arch = "wasm32")]
    {
        PINNED_COMMUNITIES_TIMEOUT.with(|timeout| {
            // Cancel any existing timeout
            *timeout.borrow_mut() = None;

            // Schedule new publish after 1 second
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

/// Publish pinned communities with retry and exponential backoff
fn publish_with_retry(
    pins: Vec<String>,
    retry_count: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'static>> {
    Box::pin(async move {
        const MAX_RETRIES: u32 = 3;

        // Set status to syncing
        *PINNED_COMMUNITIES_SYNC_STATUS.write() = PinnedCommunitiesSyncStatus::Syncing;

        match publish_pinned_communities(pins.clone()).await {
            Ok(_) => {
                // Success - clear rollback state and set status to idle
                *PINNED_COMMUNITIES_ROLLBACK.read().data().write() = None;
                *PINNED_COMMUNITIES_SYNC_STATUS.write() = PinnedCommunitiesSyncStatus::Idle;
                log::info!("Pinned communities published successfully");
            }
            Err(e) => {
                log::error!(
                    "Failed to publish pinned communities (attempt {}): {}",
                    retry_count + 1,
                    e
                );

                if retry_count < MAX_RETRIES {
                    // Calculate exponential backoff delay: 1s, 2s, 4s
                    let delay_ms = 1000u32 * (1 << retry_count);

                    log::info!(
                        "Retrying pinned communities publish in {}ms (attempt {}/{})",
                        delay_ms,
                        retry_count + 1,
                        MAX_RETRIES
                    );

                    // Schedule retry with exponential backoff
                    #[cfg(target_arch = "wasm32")]
                    {
                        let timeout_handle = Timeout::new(delay_ms, move || {
                            spawn_local(publish_with_retry(pins, retry_count + 1));
                        });
                        std::mem::forget(timeout_handle);
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        tokio::time::sleep(Duration::from_millis(delay_ms as u64)).await;
                        publish_with_retry(pins, retry_count + 1).await;
                    }
                } else {
                    // Max retries exceeded - rollback local state and set failed status
                    log::error!(
                        "Pinned communities publish failed after {} retries: {}",
                        MAX_RETRIES,
                        e
                    );

                    // Rollback local state to match persisted state
                    if let Some(previous_state) =
                        PINNED_COMMUNITIES_ROLLBACK.read().data().read().clone()
                    {
                        log::warn!("Automatically rolling back pinned communities to previous state due to publish failure");
                        *PINNED_COMMUNITIES.read().data().write() = previous_state;
                    }

                    // Set failed status (rollback state is cleared here)
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

    // Parse a_tags to Coordinates
    let coordinates: Vec<Coordinate> = pins
        .iter()
        .filter_map(|a_tag| parse_a_tag_to_coordinate(a_tag))
        .collect();

    // Use EventBuilder::communities() - the SDK's dedicated builder method
    // This creates Kind::Communities (10004) with coordinate tags
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
    // Use splitn(3, ':') to handle identifiers containing colons
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
    if let Some(previous_state) = PINNED_COMMUNITIES_ROLLBACK.read().data().read().clone() {
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
    let current_pins = PINNED_COMMUNITIES.read().data().read().clone();
    log::info!("Manually retrying pinned communities publish");
    publish_with_retry(current_pins, 0).await;
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
