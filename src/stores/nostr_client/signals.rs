//! Nostr client global signals
//!
//! Dioxus GlobalSignal definitions for client state management.
//! Following Dioxus patterns with section headers and doc comments.

use dioxus::prelude::*;
use nostr_sdk::Client;
use std::sync::{Arc, Mutex, OnceLock};

use crate::stores::signer::SignerType;

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
pub(crate) struct CachedContacts {
    pub pubkey: String,
    pub contacts: Vec<String>,
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
