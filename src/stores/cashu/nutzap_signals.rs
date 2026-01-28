//! Nutzap global signals (NIP-61)
//!
//! Global signal definitions for nutzap state management.

use dioxus::prelude::*;
use std::collections::HashMap;

use super::nutzap::{NutzapInfo, PendingNutzap};

// =============================================================================
// Core Nutzap Signals
// =============================================================================

/// My own nutzap info (kind:10019 data)
/// Contains P2PK pubkey, accepted mints, and delivery relays
pub static MY_NUTZAP_INFO: GlobalSignal<Option<NutzapInfo>> = Signal::global(|| None);

/// Cache of other users' nutzap info (pubkey -> NutzapInfo)
/// Used to look up recipient's accepted mints and P2PK pubkey
pub static NUTZAP_INFO_CACHE: GlobalSignal<HashMap<String, NutzapInfo>> =
    Signal::global(HashMap::new);

/// Pending nutzaps awaiting redemption
pub static PENDING_NUTZAPS: GlobalSignal<Vec<PendingNutzap>> = Signal::global(Vec::new);

/// Load nutzap auto-redeem setting from localStorage (Dioxus pattern - closure runs once on first access)
fn load_nutzap_auto_redeem() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        use gloo_storage::{LocalStorage, Storage};
        LocalStorage::get("nostr_nutzap_auto_redeem").unwrap_or(true)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        true
    }
}

/// Whether to automatically redeem incoming nutzaps
pub static NUTZAP_AUTO_REDEEM: GlobalSignal<bool> = Signal::global(load_nutzap_auto_redeem);

/// Whether nutzap subscription is currently active
pub static NUTZAP_SUBSCRIPTION_ACTIVE: GlobalSignal<bool> = Signal::global(|| false);

/// Whether nutzap receiving is enabled (kind:10019 published)
pub static NUTZAP_ENABLED: GlobalSignal<bool> = Signal::global(|| false);

// =============================================================================
// Helper Functions
// =============================================================================

/// Clear all nutzap state (for logout/reset)
/// Note: NUTZAP_AUTO_REDEEM is not reset here - it's persisted to localStorage
/// and should retain user preference across sessions
pub fn clear_nutzap_state() {
    *MY_NUTZAP_INFO.write() = None;
    NUTZAP_INFO_CACHE.write().clear();
    PENDING_NUTZAPS.write().clear();
    // Don't reset NUTZAP_AUTO_REDEEM - it's a persisted user preference
    *NUTZAP_SUBSCRIPTION_ACTIVE.write() = false;
    *NUTZAP_ENABLED.write() = false;
    log::info!("Cleared nutzap state (auto-redeem preference preserved)");
}

/// Add a nutzap info to the cache
pub fn cache_nutzap_info(pubkey: String, info: NutzapInfo) {
    NUTZAP_INFO_CACHE.write().insert(pubkey, info);
}

/// Get cached nutzap info for a pubkey
pub fn get_cached_nutzap_info(pubkey: &str) -> Option<NutzapInfo> {
    NUTZAP_INFO_CACHE.read().get(pubkey).cloned()
}

/// Add a pending nutzap
///
/// Returns `true` if the nutzap was added, `false` if it was a duplicate.
/// This allows callers to skip auto-redeem for duplicates.
pub fn add_pending_nutzap(nutzap: PendingNutzap) -> bool {
    let event_id = nutzap.event_id.clone();
    let mut pending = PENDING_NUTZAPS.write();

    if pending.iter().any(|n| n.event_id == event_id) {
        log::debug!("Skipped duplicate nutzap: {}", event_id);
        return false;
    }

    pending.push(nutzap);
    log::debug!("Added pending nutzap: {}", event_id);
    true
}

/// Remove a pending nutzap by event ID
pub fn remove_pending_nutzap(event_id: &str) {
    PENDING_NUTZAPS.write().retain(|n| n.event_id != event_id);
    log::debug!("Removed pending nutzap: {}", event_id);
}

/// Update a pending nutzap's status
pub fn update_pending_nutzap_status(event_id: &str, status: super::nutzap::NutzapStatus) {
    let mut pending = PENDING_NUTZAPS.write();
    if let Some(nutzap) = pending.iter_mut().find(|n| n.event_id == event_id) {
        nutzap.status = status;
        log::debug!("Updated nutzap {} status", event_id);
    }
}

/// Get count of pending nutzaps (only those with Pending status)
#[allow(dead_code)]
pub fn pending_nutzap_count() -> usize {
    PENDING_NUTZAPS.read()
        .iter()
        .filter(|n| matches!(n.status, super::nutzap::NutzapStatus::Pending))
        .count()
}

/// Get total value of pending nutzaps (only those with Pending status)
#[allow(dead_code)]
pub fn pending_nutzap_value() -> u64 {
    // Use sum() for clarity - nutzap amounts can't realistically overflow u64
    PENDING_NUTZAPS.read()
        .iter()
        .filter(|n| matches!(n.status, super::nutzap::NutzapStatus::Pending))
        .map(|n| n.amount)
        .sum()
}
