//! Centralized mute/block cache hook with automatic pubkey-tracking invalidation
//!
//! Automatically refetches when user logs in/out, switches accounts, or client initializes.
//! Uses Dioxus reactive subscription pattern: reading AUTH_STATE.read() subscribes to changes.

use dioxus::prelude::*;
use std::collections::HashSet;
use std::rc::Rc;

/// Cache signal type for muted posts / blocked users
pub type MuteBlockCache = Signal<Option<Rc<HashSet<String>>>>;

/// Returns (cached_muted_posts, cached_blocked_users) signals
/// Both are `MuteBlockCache` (Signal<Option<Rc<HashSet<String>>>>) for O(1) lookups
#[allow(clippy::type_complexity)]
pub fn use_mute_block_cache() -> (MuteBlockCache, MuteBlockCache) {
    let mut cached_muted_posts: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);
    let mut cached_blocked_users: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);

    use_effect(move || {
        let is_authenticated = crate::stores::auth_store::is_authenticated();
        let client_initialized = *crate::stores::nostr_client::CLIENT_INITIALIZED.read();

        // Dioxus pattern: reading .read() auto-subscribes to pubkey changes
        // Effect re-runs when AUTH_STATE.pubkey changes (account switch)
        let current_pubkey = crate::stores::auth_store::AUTH_STATE.read().pubkey.clone();

        // Clear caches on logout to prevent stale data
        if !is_authenticated {
            cached_muted_posts.set(None);
            cached_blocked_users.set(None);
            return;
        }

        if !client_initialized {
            return;
        }

        // Skip fetch if caches already populated for this session
        if cached_muted_posts.peek().is_some() && cached_blocked_users.peek().is_some() {
            return;
        }

        // Capture pubkey before spawn to guard against account switch during fetch
        let auth_pubkey_snapshot = current_pubkey.clone();

        spawn(async move {
            // Single fetch for both - avoids double fetch_mute_list() call
            // Only set caches on success; leave as None on error so we can retry
            if let Ok(data) = crate::stores::nostr_client::get_mute_list_data().await {
                // Guard: only write if same user still logged in (prevents stale data)
                let current = crate::stores::auth_store::AUTH_STATE.peek().pubkey.clone();
                if current == auth_pubkey_snapshot && auth_pubkey_snapshot.is_some() {
                    cached_muted_posts.set(Some(Rc::new(data.muted_posts)));
                    cached_blocked_users.set(Some(Rc::new(data.blocked_users)));
                }
            }
        });
    });

    (cached_muted_posts, cached_blocked_users)
}
