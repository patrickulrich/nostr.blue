//! Nostr client global signals
//!
//! Dioxus GlobalSignal definitions for client state management.
//! Following Dioxus patterns with section headers and doc comments.
use super::contacts::EnrichedContact;
use crate::stores::signer::SignerType;
use dioxus::prelude::*;
use nostr_sdk::Client;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
/// Global Nostr client instance (None until initialized)
pub static NOSTR_CLIENT: GlobalSignal<Option<Arc<Client>>> = Signal::global(|| None);
/// Whether the client has finished initializing
pub static CLIENT_INITIALIZED: GlobalSignal<bool> = Signal::global(|| false);
/// Whether the client has a signer attached (can publish events)
pub static HAS_SIGNER: GlobalSignal<bool> = Signal::global(|| false);
/// The current signer type (if any)
pub static CURRENT_SIGNER: GlobalSignal<Option<SignerType>> = Signal::global(|| None);
static CONTACTS_CACHE_GENERATION: AtomicU64 = AtomicU64::new(0);
/// Get the current cache generation (for staleness checks)
pub(crate) fn get_cache_generation() -> u64 {
    CONTACTS_CACHE_GENERATION.load(Ordering::SeqCst)
}
/// Increment cache generation and return the new value
fn increment_cache_generation() -> u64 {
    CONTACTS_CACHE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}
/// Cached contacts with 5-minute TTL for feed optimization
/// Uses EnrichedContact to preserve relay hints and petnames (NIP-02)
pub(crate) struct CachedContacts {
    pub pubkey: String,
    pub contacts: Vec<EnrichedContact>,
    pub cached_at: instant::Instant,
    /// nostr-sdk pattern: Track last refresh spawn to prevent spam
    pub last_refresh_spawned: Option<instant::Instant>,
    /// Dioxus pattern: Track cache generation for staleness (freshness.rs)
    /// Stored for debugging; staleness checks use get_cache_generation()
    #[allow(dead_code)]
    pub generation: u64,
}
static CONTACTS_CACHE: OnceLock<Mutex<Option<CachedContacts>>> = OnceLock::new();
/// Get the contacts cache mutex
pub fn get_contacts_cache() -> &'static Mutex<Option<CachedContacts>> {
    CONTACTS_CACHE.get_or_init(|| Mutex::new(None))
}
/// Invalidate the contacts cache (call after follow/unfollow)
pub fn invalidate_contacts_cache() {
    increment_cache_generation();
    let mut cache = get_contacts_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *cache = None;
    log::debug!("Contacts cache invalidated (generation incremented)");
}
/// Trigger for mute/block cache invalidation (Dioxus GlobalSignal pattern)
/// Incremented after each mute/block mutation to trigger effect re-runs
pub static MUTE_BLOCK_INVALIDATE: GlobalSignal<u32> = Signal::global(|| 0);
/// Invalidate mute/block caches across all components
/// Call after mute_post, unmute_post, block_user, unblock_user succeed
pub fn invalidate_mute_block_cache() {
    *MUTE_BLOCK_INVALIDATE.write() = MUTE_BLOCK_INVALIDATE.peek().wrapping_add(1);
    log::debug!("Mute/block cache invalidated");
}
