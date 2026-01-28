//! Centralized mute/block cache hook with automatic pubkey-tracking invalidation
//!
//! Automatically refetches when user logs in/out, switches accounts, or client initializes.
//! Uses Dioxus reactive subscription pattern: reading AUTH_STATE.read() subscribes to changes.

use dioxus::prelude::*;
use std::collections::HashSet;
use std::rc::Rc;

/// Cooldown period after fetch error before retrying (prevents immediate retry loops)
const FETCH_ERROR_COOLDOWN_SECS: u64 = 30;

fn now_secs() -> u64 {
    chrono::Utc::now().timestamp() as u64
}

/// Cache signal type for muted posts / blocked users
pub type MuteBlockCache = Signal<Option<Rc<HashSet<String>>>>;

/// Returns (cached_muted_posts, cached_blocked_users) signals
/// Both are `MuteBlockCache` (Signal<Option<Rc<HashSet<String>>>>) for O(1) lookups
#[allow(clippy::type_complexity)]
pub fn use_mute_block_cache() -> (MuteBlockCache, MuteBlockCache) {
    let mut cached_muted_posts: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);
    let mut cached_blocked_users: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);
    let mut last_fetch_error_at: Signal<Option<u64>> = use_signal(|| None);
    // Track previous pubkey to detect account switches (Dioxus pattern)
    let mut last_pubkey: Signal<Option<String>> = use_signal(|| None);

    use_effect(move || {
        let is_authenticated = crate::stores::auth_store::is_authenticated();
        let client_initialized = *crate::stores::nostr_client::CLIENT_INITIALIZED.read();

        // Dioxus pattern: reading .read() auto-subscribes to pubkey changes
        // Effect re-runs when AUTH_STATE.pubkey changes (account switch)
        let current_pubkey = crate::stores::auth_store::AUTH_STATE.read().pubkey.clone();

        // Dioxus pattern: reading .read() subscribes to changes
        // Effect re-runs when mute/block mutation occurs
        let _ = *crate::stores::nostr_client::MUTE_BLOCK_INVALIDATE.read();

        // Clear caches on logout to prevent stale data
        if !is_authenticated {
            cached_muted_posts.set(None);
            cached_blocked_users.set(None);
            last_pubkey.set(None);
            last_fetch_error_at.set(None);  // Clear error cooldown on logout
            return;
        }

        // Detect account switch (both Some but different) and clear stale caches
        if let (Some(ref last), Some(ref current)) = (last_pubkey.peek().as_ref(), current_pubkey.as_ref()) {
            if last != current {
                log::debug!("Account switch detected, clearing mute/block cache");
                cached_muted_posts.set(None);
                cached_blocked_users.set(None);
                last_fetch_error_at.set(None);
            }
        }
        last_pubkey.set(current_pubkey.clone());

        if !client_initialized {
            return;
        }

        // Skip fetch if caches already populated for this session
        // (cache cleared by invalidation or pubkey change triggers refetch)
        if cached_muted_posts.peek().is_some() && cached_blocked_users.peek().is_some() {
            return;
        }

        // Skip fetch if in error cooldown period
        if let Some(error_at) = *last_fetch_error_at.peek() {
            if now_secs().saturating_sub(error_at) < FETCH_ERROR_COOLDOWN_SECS {
                return;
            }
        }

        // Capture pubkey before spawn to guard against account switch during fetch
        let auth_pubkey_snapshot = current_pubkey.clone();

        spawn(async move {
            // Single fetch for both - avoids double fetch_mute_list() call
            match crate::stores::nostr_client::get_mute_list_data().await {
                Ok(data) => {
                    // Guard: only write if same user still logged in (prevents stale data)
                    let current = crate::stores::auth_store::AUTH_STATE.peek().pubkey.clone();
                    if current == auth_pubkey_snapshot && auth_pubkey_snapshot.is_some() {
                        cached_muted_posts.set(Some(Rc::new(data.muted_posts)));
                        cached_blocked_users.set(Some(Rc::new(data.blocked_users)));
                        last_fetch_error_at.set(None); // Reset on success
                    }
                }
                Err(e) => {
                    // nostr-sdk pattern: structured logging with truncated IDs
                    let snapshot_short = auth_pubkey_snapshot.as_ref()
                        .map(|s| &s[..8.min(s.len())]);
                    log::error!(
                        "Failed to fetch mute list: {} (snapshot={:?})",
                        e, snapshot_short
                    );
                    last_fetch_error_at.set(Some(now_secs())); // Set error timestamp
                }
            }
        });
    });

    (cached_muted_posts, cached_blocked_users)
}
