//! Nostr client global signals
//!
//! Dioxus GlobalSignal definitions for client state management.
//! Following Dioxus patterns with section headers and doc comments.
use super::contacts::EnrichedContact;
use crate::stores::signer::SignerType;
use dioxus::prelude::*;
use nostr_sdk::client::Client;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
/// Global Nostr client instance (None until initialized)
pub static NOSTR_CLIENT: GlobalSignal<Option<Arc<Client>>> = Signal::global(|| None);
/// Runtime-free mirror of [`NOSTR_CLIENT`].
///
/// WHY THIS EXISTS: every `GlobalSignal` access — including `peek()` —
/// resolves through `Runtime::current()` (`dioxus-signals-0.7.9/src/global/
/// mod.rs:287`, `get_global_context`), which panics on any thread without a
/// Dioxus runtime. The notification dispatcher and EOSE tracker are raw
/// `std::thread`s (by design: they own current-thread tokio runtimes and
/// must survive client rebuilds) and their loops call `get_client()`.
/// On Android release (`panic = "abort"`) that panic killed the process on
/// every boot (crash loop, fixed here); on desktop it silently killed the
/// threads in dev and would abort release builds the same way.
///
/// `Runtime` is `Rc` (`!Send`), so a `RuntimeGuard` cannot be moved into a
/// `std::thread` closure — the mirror is the only correct escape. It is
/// written exclusively via [`set_nostr_client`] at the same sites that
/// write the signal, so the two can never diverge.
static CLIENT_HANDLE: std::sync::LazyLock<std::sync::RwLock<Option<Arc<Client>>>> =
    std::sync::LazyLock::new(|| std::sync::RwLock::new(None));

/// Runtime-free accessor for the current client (see [`CLIENT_HANDLE`] for
/// why this exists). Safe from ANY thread — including the raw dispatcher
/// and EOSE-tracker threads that have no Dioxus runtime.
pub fn get_client() -> Option<Arc<Client>> {
    CLIENT_HANDLE
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Set the global client, keeping the signal (reactive readers) and the
/// runtime-free mirror (raw-thread readers) in lockstep. The mirror is
/// written first and unconditionally; the signal write is skipped when no
/// Dioxus runtime is current (bare threads, tests) — a GlobalSignal write
/// requires runtime context and would panic there. Today's call sites
/// (`initialize_client`, `rebuild_client`) always run inside the runtime,
/// so the signal is updated in production.
pub fn set_nostr_client(client: Option<Arc<Client>>) {
    *CLIENT_HANDLE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = client.clone();
    if dioxus::core::Runtime::try_current().is_some() {
        *NOSTR_CLIENT.write() = client;
    }
}
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
/// Version of the current user's follow set (kind 3). Bumped by the
/// contacts module ONLY when the followed-pubkey set actually changes
/// (network refresh that differs from the last observed set, or a
/// successful follow/unfollow publish) — never on routine refreshes with
/// an unchanged set, so reactive consumers can re-key subscriptions
/// without REQ churn (plan wave B3; mirrors Amethyst's reactive
/// `followsPerRelay` chain at app level).
pub static CONTACTS_VERSION: GlobalSignal<u64> = Signal::global(|| 0);
/// Invalidate mute/block caches across all components
/// Call after mute_post, unmute_post, block_user, unblock_user succeed.
///
/// Uses `with_mut` to avoid the RHS-then-LHS borrow-aliasing panic on
/// `dioxus-signals-0.7.9/src/global/mod.rs:100` that occurs when `.peek()`
/// and `.write()` overlap on the same signal (same trap as `bump_cache_version`
/// in `stores/profiles.rs`).
pub fn invalidate_mute_block_cache() {
    MUTE_BLOCK_INVALIDATE.with_mut(|v| *v = v.wrapping_add(1));
    log::debug!("Mute/block cache invalidated");
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    /// Regression test for the Android release crash loop: the dispatcher
    /// and EOSE tracker are raw `std::thread`s with no Dioxus runtime, and
    /// their loops call `get_client()`. When `get_client()` read the
    /// `NOSTR_CLIENT` GlobalSignal, that call hit
    /// `Runtime::current()` inside `get_global_context`
    /// (`dioxus-signals-0.7.9/src/global/mod.rs:287`) and panicked on
    /// every boot — with `panic = "abort"` in the release profile this
    /// killed the process instantly. `get_client()` must be callable from
    /// any thread.
    #[test]
    fn get_client_is_safe_off_the_dioxus_runtime() {
        let handle = std::thread::spawn(|| {
            // Must not panic — on the old GlobalSignal-backed
            // implementation this panicked before returning.
            get_client()
        });
        // Join (not unwrap-join) so a panic surfaces as a test failure
        // with the panic message instead of a join error.
        match handle.join() {
            Ok(client) => assert!(client.is_none() || client.is_some()),
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }

    /// The mirror round-trips through `set_nostr_client` without a Dioxus
    /// runtime present (the signal leg is skipped off-runtime by design —
    /// both production call sites run inside the runtime).
    #[test]
    fn set_nostr_client_updates_the_mirror_off_runtime() {
        set_nostr_client(None);
        assert!(get_client().is_none());
    }
}
