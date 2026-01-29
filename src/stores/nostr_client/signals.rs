//! Nostr client global signals
//!
//! Dioxus GlobalSignal definitions for client state management.
//! Following Dioxus patterns with section headers and doc comments.

use dioxus::prelude::*;
use nostr_sdk::Client;
use std::sync::{Arc, Mutex, OnceLock};

use crate::stores::signer::SignerType;
use super::contacts::EnrichedContact;

// =============================================================================
// Core Client Signals
// =============================================================================

/// Global Nostr client instance (None until initialized)
pub static NOSTR_CLIENT: GlobalSignal<Option<Arc<Client>>> = Signal::global(|| None);

/// Whether the client has finished initializing
pub static CLIENT_INITIALIZED: GlobalSignal<bool> = Signal::global(|| false);

// =============================================================================
// Signer Signals
// =============================================================================

/// Whether the client has a signer attached (can publish events)
pub static HAS_SIGNER: GlobalSignal<bool> = Signal::global(|| false);

/// The current signer type (if any)
pub static CURRENT_SIGNER: GlobalSignal<Option<SignerType>> = Signal::global(|| None);

// =============================================================================
// Contacts Cache
// =============================================================================

/// Cached contacts with 5-minute TTL for feed optimization
/// Uses EnrichedContact to preserve relay hints and petnames (NIP-02)
pub(crate) struct CachedContacts {
    pub pubkey: String,
    pub contacts: Vec<EnrichedContact>,
    pub cached_at: instant::Instant,
    /// nostr-sdk pattern: Track last refresh spawn to prevent spam
    pub last_refresh_spawned: Option<instant::Instant>,
}

static CONTACTS_CACHE: OnceLock<Mutex<Option<CachedContacts>>> = OnceLock::new();

/// Get the contacts cache mutex
pub(crate) fn get_contacts_cache() -> &'static Mutex<Option<CachedContacts>> {
    CONTACTS_CACHE.get_or_init(|| Mutex::new(None))
}

/// Invalidate the contacts cache (call after follow/unfollow)
pub fn invalidate_contacts_cache() {
    // Use unwrap_or_else to recover from poisoned mutex instead of silently ignoring
    let mut cache = get_contacts_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *cache = None;
    log::debug!("Contacts cache invalidated");
}

// =============================================================================
// Mute/Block Cache Invalidation
// =============================================================================

/// Trigger for mute/block cache invalidation (Dioxus GlobalSignal pattern)
/// Incremented after each mute/block mutation to trigger effect re-runs
pub static MUTE_BLOCK_INVALIDATE: GlobalSignal<u32> = Signal::global(|| 0);

/// Invalidate mute/block caches across all components
/// Call after mute_post, unmute_post, block_user, unblock_user succeed
pub fn invalidate_mute_block_cache() {
    // Dioxus pattern: write() triggers subscribers, wrapping_add handles overflow
    *MUTE_BLOCK_INVALIDATE.write() = MUTE_BLOCK_INVALIDATE.peek().wrapping_add(1);
    log::debug!("Mute/block cache invalidated");
}
