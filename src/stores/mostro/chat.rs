//! Mostro P2P chat envelope helpers (mostro-core 0.14.2 kind-14 format).
//!
//! The chat wire format migrated from NIP-59 gift wraps addressed to the raw
//! ECDH shared key (kind 1059) to a gift-wrap-free kind-14 envelope
//! (`mostro_core::chat`): the ECDH secret is HKDF-split into `K_conv`
//! (NIP-44 self-encryption, outer `p` tag) and `K_sign` (outer author).
//! Spec: <https://mostro.network/protocol/chat.html>.
//!
//! nostr.blue **sends the new format only** and **dual-reads** both formats:
//!
//! * live subscription: `chat_filter(pub(K_sign))` — kind 14, author-pinned
//!   (filtering by `#p` alone is flood-vulnerable);
//! * legacy hydration: `giftwrap_chat_filter(pub(shared))` — kind 1059, so
//!   history and messages from pre-migration clients (Mobile trade chat)
//!   still render.
//!
//! Attachment encryption is unchanged: keyed by the raw ECDH secret
//! (`SharedKey::secret_key`), which matches mostrix
//! (`order_chat_decryption_key_bytes`) and Mostro Mobile.
//!
//! Reference: mostrix `src/util/chat_utils.rs` (dual-read merge pattern).

use mostro_core::chat::{self, SharedKey};
use nostr::prelude::*;
use nostr_sdk::Event;

/// Current timestamp for the kind-14 clock-skew check.
///
/// Gated on target arch (not the `web` feature) so `cargo test` on a native
/// host with default features doesn't call `js_sys` — while the real wasm
/// runtime avoids `SystemTime` (panics on wasm32-unknown-unknown).
fn chat_now() -> Timestamp {
    #[cfg(target_arch = "wasm32")]
    {
        Timestamp::from(crate::platform::timestamp::now_secs())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Timestamp::from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        )
    }
}

/// Decode a chat event from either envelope format.
///
/// * kind 14 → `chat::unwrap_chat_message` — verifies outer author is
///   `pub(K_sign)`, exactly one `p` tag = `pub(K_conv)`, clock skew, content
///   size bound, and the inner kind-1 signer is one of `allowed_signers`.
/// * kind 1059 → legacy `chat::unwrap_giftwrap_chat_message` + explicit
///   `allowed_signers` check (mostrix `unwrap_giftwrap_with_shared_key`
///   pattern — the legacy helper itself does not gate the inner signer).
///
/// Other kinds are rejected (they are not chat events for this channel).
#[allow(clippy::result_large_err)]
pub async fn decode_chat_event(
    shared: &SharedKey,
    sign_pubkey: &PublicKey,
    allowed_signers: &[PublicKey],
    event: &Event,
) -> Result<chat::ChatMessage, String> {
    if event.kind == Kind::PrivateDirectMessage {
        let (conv, _) = shared
            .chat_keys()
            .map_err(|e| format!("chat key derivation failed: {e}"))?;
        chat::unwrap_chat_message(
            &conv,
            sign_pubkey,
            allowed_signers,
            event,
            chat_now(),
        )
        .map_err(|e| e.to_string())
    } else if event.kind == Kind::GiftWrap {
        let msg = chat::unwrap_giftwrap_chat_message(shared.keys(), event)
            .await
            .map_err(|e| e.to_string())?;
        if !allowed_signers.contains(&msg.sender) {
            return Err("legacy chat signer is not a party to this chat".to_string());
        }
        Ok(msg)
    } else {
        Err(format!("not a chat event: kind {}", event.kind.as_u16()))
    }
}

/// Wrap a chat message in the new kind-14 envelope (send-new-only policy).
#[allow(clippy::result_large_err)]
pub async fn encode_chat_event(
    sender: &Keys,
    shared: &SharedKey,
    content: &str,
) -> Result<nostr::Event, String> {
    let (conv, sign) = shared
        .chat_keys()
        .map_err(|e| format!("chat key derivation failed: {e}"))?;
    chat::wrap_chat_message(sender, &conv, &sign, content)
        .await
        .map_err(|e| e.to_string())
}

/// Live kind-14 subscription filter for a chat channel (`authors`-pinned).
#[allow(clippy::result_large_err)]
pub fn chat_filter_new(shared: &SharedKey) -> Result<nostr_sdk::Filter, String> {
    let (_, sign) = shared
        .chat_keys()
        .map_err(|e| format!("chat key derivation failed: {e}"))?;
    Ok(chat::chat_filter(sign.public_key()))
}

/// Legacy kind-1059 hydration filter for a chat channel (`#p` on the raw
/// shared key pubkey — the pre-migration wire address).
pub fn chat_filter_legacy(shared: &SharedKey) -> nostr_sdk::Filter {
    chat::giftwrap_chat_filter(shared.public_key())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (Keys, Keys, SharedKey) {
        let alice = Keys::generate();
        let bob = Keys::generate();
        let shared = SharedKey::derive(alice.secret_key(), &bob.public_key()).unwrap();
        (alice, bob, shared)
    }

    #[tokio::test]
    async fn kind14_round_trip() {
        let (alice, bob, shared) = pair();
        let event = encode_chat_event(&alice, &shared, "hello kind 14")
            .await
            .unwrap();
        assert_eq!(event.kind, Kind::PrivateDirectMessage);

        let (_, sign) = shared.chat_keys().unwrap();
        let sign_pk = sign.public_key();
        let allowed = [alice.public_key(), bob.public_key()];
        let decoded = decode_chat_event(&shared, &sign_pk, &allowed, &event)
            .await
            .unwrap();
        assert_eq!(decoded.content, "hello kind 14");
        assert_eq!(decoded.sender, alice.public_key());
    }

    #[tokio::test]
    async fn kind14_rejects_wrong_author() {
        let (alice, bob, shared) = pair();
        let event = encode_chat_event(&alice, &shared, "hi").await.unwrap();
        let impostor = Keys::generate().public_key();
        let allowed = [alice.public_key(), bob.public_key()];
        assert!(decode_chat_event(&shared, &impostor, &allowed, &event)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn legacy_giftwrap_still_decodes() {
        let (alice, bob, shared) = pair();
        let event = chat::wrap_giftwrap_chat_message(&alice, &shared.public_key(), "legacy msg")
            .await
            .unwrap();
        assert_eq!(event.kind, Kind::GiftWrap);

        let (_, sign) = shared.chat_keys().unwrap();
        let sign_pk = sign.public_key();
        let allowed = [alice.public_key(), bob.public_key()];
        let decoded = decode_chat_event(&shared, &sign_pk, &allowed, &event)
            .await
            .unwrap();
        assert_eq!(decoded.content, "legacy msg");
        assert_eq!(decoded.sender, alice.public_key());
    }

    #[tokio::test]
    async fn legacy_giftwrap_rejects_non_party_signer() {
        let (alice, bob, shared) = pair();
        let event = chat::wrap_giftwrap_chat_message(&alice, &shared.public_key(), "forged")
            .await
            .unwrap();
        let (_, sign) = shared.chat_keys().unwrap();
        let sign_pk = sign.public_key();
        // Only bob is allowed; alice (the actual signer) is not in the list.
        let allowed = [bob.public_key()];
        assert!(decode_chat_event(&shared, &sign_pk, &allowed, &event)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn new_and_legacy_are_isolated_channels() {
        // A kind-14 envelope must not decode via the legacy path and vice
        // versa: the two formats use different wire addresses.
        let (alice, bob, shared) = pair();
        let k14 = encode_chat_event(&alice, &shared, "new").await.unwrap();
        let k1059 = chat::wrap_giftwrap_chat_message(&alice, &shared.public_key(), "old")
            .await
            .unwrap();

        let (_, sign) = shared.chat_keys().unwrap();
        let sign_pk = sign.public_key();
        let allowed = [alice.public_key(), bob.public_key()];

        // Kind 14 rejected when the legacy filter feeds it (author pin fails
        // because K_sign != the ephemeral wrap author).
        let wrong_author = k1059.pubkey;
        assert!(decode_chat_event(&shared, &wrong_author, &allowed, &k14)
            .await
            .is_err());
        // Both decode via the canonical dual-read path.
        assert!(decode_chat_event(&shared, &sign_pk, &allowed, &k14)
            .await
            .is_ok());
        assert!(decode_chat_event(&shared, &sign_pk, &allowed, &k1059)
            .await
            .is_ok());
    }

    #[test]
    fn chat_key_derivation_is_symmetric() {
        let alice = Keys::generate();
        let bob = Keys::generate();
        let from_alice = SharedKey::derive(alice.secret_key(), &bob.public_key()).unwrap();
        let from_bob = SharedKey::derive(bob.secret_key(), &alice.public_key()).unwrap();
        let (ac, as_) = from_alice.chat_keys().unwrap();
        let (bc, bs) = from_bob.chat_keys().unwrap();
        assert_eq!(ac.public_key(), bc.public_key());
        assert_eq!(as_.public_key(), bs.public_key());
    }

    #[test]
    fn filters_target_the_right_channels() {
        let (_, _, shared) = pair();
        let new = chat_filter_new(&shared).unwrap();
        let legacy = chat_filter_legacy(&shared);
        let (_, sign) = shared.chat_keys().unwrap();
        let kinds: Vec<Kind> = new
            .kinds
            .as_ref()
            .map(|k| k.iter().copied().collect())
            .unwrap_or_default();
        let authors: Vec<PublicKey> = new
            .authors
            .as_ref()
            .map(|a| a.iter().copied().collect())
            .unwrap_or_default();
        assert_eq!(kinds, vec![Kind::PrivateDirectMessage]);
        assert_eq!(authors, vec![sign.public_key()]);
        let legacy_kinds: Vec<Kind> = legacy
            .kinds
            .as_ref()
            .map(|k| k.iter().copied().collect())
            .unwrap_or_default();
        assert_eq!(legacy_kinds, vec![Kind::GiftWrap]);
    }
}
