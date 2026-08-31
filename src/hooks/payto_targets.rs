//! Reactive access to a user's NIP-A3 payment targets (kind 10133).
//!
//! Triple-gated like `use_user_lists`: the fetch effect waits for client
//! initialization, and for authenticated users until the signer has attached
//! and the user's NIP-65 relay list has been applied (the fetch is
//! gossip-routed, so an incomplete pool would silently miss the author's
//! relays). The returned memo re-evaluates whenever the global cache
//! version bumps.
use crate::stores::nostr_client;
use crate::stores::payto_targets_cache::{peek_targets, fetch_targets, PAYTO_TARGETS_VERSION};
use crate::utils::nips::nipa3::PayToTarget;
use dioxus::prelude::*;

/// Observe `pubkey_id` (hex, npub, or nprofile) and return a memo of the
/// user's payment targets. Empty when the id is unparsable or nothing has
/// been declared.
pub fn use_payto_targets(pubkey_id: &str) -> Memo<Vec<PayToTarget>> {
    let mut pubkey_hex = use_signal(String::new);
    let hex_for_fetch = pubkey_hex;
    let hex_for_memo = pubkey_hex;

    let id = pubkey_id.to_string();
    use_effect(use_reactive(&id, move |id| {
        let parsed = crate::utils::nip19_urls::parse_profile_id(&id)
            .map(|pk| pk.to_hex())
            .unwrap_or_default();
        pubkey_hex.set(parsed);
    }));

    use_effect(use_reactive(
        (
            &*hex_for_fetch.read(),
            &*nostr_client::CLIENT_INITIALIZED.read(),
            &*nostr_client::HAS_SIGNER.read(),
            &*crate::stores::relay::USER_RELAYS_APPLIED.read(),
        ),
        move |(hex, client_initialized, has_signer, relays_applied)| {
            if hex.is_empty() || !client_initialized {
                return;
            }
            // Authenticated users wait until their relay pool is complete;
            // logged-out users fetch against the default pool.
            if has_signer && !relays_applied {
                return;
            }
            spawn(async move {
                fetch_targets(hex).await;
            });
        },
    ));

    use_memo(move || {
        let _version = *PAYTO_TARGETS_VERSION.read();
        let hex = hex_for_memo.read().clone();
        if hex.is_empty() {
            return Vec::new();
        }
        peek_targets(&hex).unwrap_or_default()
    })
}
