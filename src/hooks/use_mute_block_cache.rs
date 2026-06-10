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
    chrono::Utc::now().timestamp().try_into().unwrap_or(0)
}
/// Cache signal type for muted posts / blocked users
pub type MuteBlockCache = Signal<Option<Rc<HashSet<String>>>>;
/// Returns (cached_muted_posts, cached_blocked_users, cached_muted_words) signals
/// All are `MuteBlockCache` (Signal<Option<Rc<HashSet<String>>>>) for O(1) lookups
#[allow(clippy::type_complexity)]
pub fn use_mute_block_cache() -> (MuteBlockCache, MuteBlockCache, MuteBlockCache) {
    let mut cached_muted_posts: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);
    let mut cached_blocked_users: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);
    let mut cached_muted_words: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);
    let mut last_fetch_error_at: Signal<Option<u64>> = use_signal(|| None);
    let mut last_pubkey: Signal<Option<String>> = use_signal(|| None);
    let mut last_invalidate_token: Signal<u32> = use_signal(|| 0);
    use_effect(move || {
        let is_authenticated = crate::stores::auth_store::is_authenticated();
        let client_initialized = *crate::stores::nostr_client::CLIENT_INITIALIZED.read();
        let current_pubkey = crate::stores::auth_store::AUTH_STATE.read().pubkey.clone();
        let current_token = *crate::stores::nostr_client::MUTE_BLOCK_INVALIDATE.read();
        if current_token != *last_invalidate_token.peek() {
            log::debug!("Mute/block invalidation detected, clearing caches");
            cached_muted_posts.set(None);
            cached_blocked_users.set(None);
            cached_muted_words.set(None);
            last_fetch_error_at.set(None);
            last_invalidate_token.set(current_token);
        }
        if !is_authenticated {
            cached_muted_posts.set(None);
            cached_blocked_users.set(None);
            cached_muted_words.set(None);
            last_pubkey.set(None);
            last_fetch_error_at.set(None);
            return;
        }
        if current_pubkey.is_none() {
            cached_muted_posts.set(None);
            cached_blocked_users.set(None);
            cached_muted_words.set(None);
            last_fetch_error_at.set(None);
            last_pubkey.set(current_pubkey.clone());
            return;
        }
        if let (Some(ref last), Some(ref current)) =
            (last_pubkey.peek().as_ref(), current_pubkey.as_ref())
        {
            if last != current {
                log::debug!("Account switch detected, clearing mute/block cache");
                cached_muted_posts.set(None);
                cached_blocked_users.set(None);
                cached_muted_words.set(None);
                last_fetch_error_at.set(None);
            }
        }
        last_pubkey.set(current_pubkey.clone());
        if !client_initialized {
            return;
        }
        if cached_muted_posts.peek().is_some()
            && cached_blocked_users.peek().is_some()
            && cached_muted_words.peek().is_some()
        {
            return;
        }
        if let Some(error_at) = *last_fetch_error_at.peek() {
            if now_secs().saturating_sub(error_at) < FETCH_ERROR_COOLDOWN_SECS {
                return;
            }
        }
        let auth_pubkey_snapshot = current_pubkey.clone();
        let invalidate_token_snapshot = current_token;
        spawn(async move {
            match crate::stores::nostr_client::get_mute_list_data().await {
                Ok(data) => {
                    let current = crate::stores::auth_store::AUTH_STATE.peek().pubkey.clone();
                    let current_invalidate =
                        *crate::stores::nostr_client::MUTE_BLOCK_INVALIDATE.peek();
                    if current == auth_pubkey_snapshot
                        && current_invalidate == invalidate_token_snapshot
                        && auth_pubkey_snapshot.is_some()
                    {
                        cached_muted_posts.set(Some(Rc::new(data.muted_posts)));
                        cached_blocked_users.set(Some(Rc::new(data.blocked_users)));
                        cached_muted_words.set(Some(Rc::new(data.muted_words)));
                        last_fetch_error_at.set(None);
                    }
                }
                Err(e) => {
                    let snapshot_short =
                        auth_pubkey_snapshot.as_ref().map(|s| &s[..8.min(s.len())]);
                    log::error!(
                        "Failed to fetch mute list: {} (snapshot={:?})",
                        e,
                        snapshot_short
                    );
                    let current = crate::stores::auth_store::AUTH_STATE.peek().pubkey.clone();
                    let current_invalidate =
                        *crate::stores::nostr_client::MUTE_BLOCK_INVALIDATE.peek();
                    if current == auth_pubkey_snapshot
                        && current_invalidate == invalidate_token_snapshot
                    {
                        last_fetch_error_at.set(Some(now_secs()));
                    }
                }
            }
        });
    });
    (cached_muted_posts, cached_blocked_users, cached_muted_words)
}
