//! Encrypted NIP-78 application-data helpers.
//!
//! Mostro-related NIP-78 events (trades, node config) carry cryptographic
//! material that must not be visible on public relays:
//!
//! - `Trade::my_trade_pubkey`, `counterparty_pubkey`, `solver_pubkey`,
//!   `pending_hold_invoice`, `my_payout_invoice`, `payment_failed_*`,
//!   `bond_slashed_at`, `bond_payout_deadline`, `next_trade_*`
//! - `MostroNodeConfig::pubkey` (which Mostro daemon the user is on)
//!
//! NIP-78 has no encryption layer of its own — events are broadcast to all
//! the user's write relays and any Nostr client can read them. This module
//! wraps the kind 30078 publish/load paths with NIP-44 encrypt-to-self using
//! the **Mostro identity key** (a separate NIP-06-derived keypair, stored in
//! `platform::storage`, independent of the user's main auth method).
//!
//! ## Why the Mostro identity key, not the main auth signer?
//!
//! 1. **Always available locally as raw keys** — the Mostro mnemonic lives in
//!    `platform::storage`, so we can use the sync `nip44::encrypt`/`decrypt`
//!    API directly without a `NostrSigner` round-trip (which would be slow
//!    for NIP-07/46/55 users).
//! 2. **Works across auth methods** — NIP-07 browser extension, NIP-46
//!    bunker, NIP-55 Android signer all work because we never touch the
//!    user's main identity for this encryption.
//! 3. **Cross-device portable** — users who import their Mostro mnemonic on
//!    a new device can decrypt their own NIP-78 events there.
//! 4. **Naturally aligned** — trades only exist when Mostro keys exist;
//!    `node_config` falls back to local-only persistence when Mostro keys
//!    are absent (see Phase 1.2 / `node_config::save_config`).

use base64::Engine;
use nostr::nips::nip44::{self, Version};
use nostr::secp256k1::rand::rngs::OsRng;
use nostr::{EventBuilder, Kind, PublicKey, SecretKey, Tag};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::stores::mostro::keys;

/// Encrypt a string to self using an explicit secret/public keypair.
///
/// This is the core primitive — production callers should use
/// [`encrypt_to_self`] which pulls the Mostro identity key from the
/// global signal; tests can call this directly with explicit keys to
/// avoid the Dioxus runtime requirement.
pub fn encrypt_with_keys(
    plaintext: &str,
    sk: &SecretKey,
    pk: &PublicKey,
) -> Result<String, String> {
    nip44::encrypt_with_rng(&mut OsRng, sk, pk, plaintext.as_bytes(), Version::V2)
        .map_err(|e| format!("NIP-44 encrypt: {e}"))
}

/// Encrypt a string to self (the Mostro identity key).
///
/// Returns `Ok(ciphertext)` suitable for placement as a kind 30078 `content`
/// field. Returns `Err` if Mostro keys are unavailable or NIP-44 fails.
pub fn encrypt_to_self(plaintext: &str) -> Result<String, String> {
    let mostro_keys = keys::try_get().ok_or("Mostro keys not initialized")?;
    let sk = mostro_keys.identity_keys.secret_key();
    let pk = mostro_keys.identity_keys.public_key();
    encrypt_with_keys(plaintext, sk, &pk)
}

/// Decrypt a string using explicit keys, with legacy plaintext fallback.
///
/// Tries NIP-44 decrypt first. If NIP-44 fails AND the content parses as
/// valid JSON of the expected type, treats it as a legacy plaintext event
/// (pre-upgrade) and returns the parsed value.
pub fn decrypt_with_keys_or_legacy<T: DeserializeOwned>(
    content: &str,
    sk: &SecretKey,
    pk: &PublicKey,
) -> Result<T, String> {
    match nip44::decrypt(sk, pk, content) {
        Ok(plaintext) => serde_json::from_str(&plaintext)
            .map_err(|e| format!("decrypted JSON parse: {e}")),
        Err(nip44_err) => {
            // Fall through to legacy plaintext path, but remember the
            // NIP-44 error in case plaintext parse also fails.
            match serde_json::from_str::<T>(content) {
                Ok(v) => {
                    log::info!(
                        "Falling back to plaintext NIP-78 parse (pre-upgrade event): \
                         NIP-44 error was {nip44_err}"
                    );
                    Ok(v)
                }
                Err(parse_err) => Err(format!(
                    "NIP-44 decrypt failed ({nip44_err}) and plaintext parse failed ({parse_err})"
                )),
            }
        }
    }
}

/// Decrypt a string from self.
///
/// Tries NIP-44 decrypt first. If NIP-44 fails AND the content parses as
/// valid JSON of the expected type, treats it as a legacy plaintext event
/// (pre-upgrade) and returns the parsed value. The caller is responsible
/// for triggering a re-publish in encrypted form (see `looks_encrypted`).
pub fn decrypt_from_self_or_legacy<T: DeserializeOwned>(content: &str) -> Result<T, String> {
    if let Some(mostro_keys) = keys::try_get() {
        let sk = mostro_keys.identity_keys.secret_key();
        let pk = mostro_keys.identity_keys.public_key();
        return decrypt_with_keys_or_legacy(content, sk, &pk);
    }
    // No Mostro keys — only legacy plaintext path is available.
    serde_json::from_str(content).map_err(|e| format!("plaintext parse (no Mostro keys): {e}"))
}

/// True if the content looks like NIP-44 ciphertext.
///
/// NIP-44 v2 payloads are base64 of a byte sequence whose first byte is the
/// version marker `0x02`. We decode the base64 prefix and check the version
/// byte. This is used to decide whether a legacy re-publish is needed after
/// a successful `decrypt_from_self_or_legacy` fallback.
#[allow(dead_code)]
pub fn looks_encrypted(content: &str) -> bool {
    // Trim whitespace; NIP-78 content shouldn't have leading/trailing space
    // but be defensive.
    let trimmed = content.trim();
    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(trimmed) {
        return matches!(bytes.first(), Some(0x02));
    }
    false
}

/// Build a kind 30078 event with encrypted content.
///
/// Serializes `payload` to JSON, encrypts the JSON to self via NIP-44 using
/// the Mostro identity key, and returns an `EventBuilder` ready for
/// `publish_queue::signing::sign_event_builder` + `publish_queue::enqueue`.
///
/// Returns `Err("Mostro keys not initialized")` when keys are absent —
/// callers should handle that case gracefully (typically: persist locally
/// only, defer the relay publish until keys become available).
pub fn build_encrypted_event_builder<T: Serialize>(
    d_tag: &str,
    payload: &T,
) -> Result<EventBuilder, String> {
    let json = serde_json::to_string(payload).map_err(|e| format!("serialize: {e}"))?;
    let encrypted = encrypt_to_self(&json)?;
    Ok(EventBuilder::new(Kind::from(30078), encrypted).tag(Tag::identifier(d_tag)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::Keys;
    use serde::{Deserialize, Serialize};

    /// A small test struct that mimics the shape of `Trade` /
    /// `MostroNodeConfig` (a heterogeneous mix of public + sensitive fields).
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    struct TestPayload {
        pub order_id: String,
        pub status: String,
        pub my_trade_pubkey: Option<String>,
        pub pending_invoice: Option<String>,
    }

    /// Generate a fresh keypair for tests (doesn't require Dioxus runtime).
    fn test_keypair() -> Keys {
        Keys::generate()
    }

    /// Phase 1.2 round-trip: encrypt then decrypt a payload using the
    /// Mostro identity key. Verifies the ciphertext is base64 NIP-44 v2
    /// and the original plaintext is recovered.
    #[test]
    fn roundtrip_encrypt_decrypt_to_self() {
        let keys = test_keypair();
        let sk = keys.secret_key();
        let pk = keys.public_key();
        let payload = TestPayload {
            order_id: "123e4567-e89b-12d3-a456-426614174000".to_string(),
            status: "active".to_string(),
            my_trade_pubkey: Some("deadbeef".to_string()),
            pending_invoice: Some("lnbc1stub".to_string()),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let encrypted = encrypt_with_keys(&json, sk, &pk).expect("encrypt_with_keys");
        assert!(looks_encrypted(&encrypted), "ciphertext must look encrypted");

        // The ciphertext must NOT contain the plaintext strings.
        assert!(!encrypted.contains("deadbeef"));
        assert!(!encrypted.contains("lnbc1stub"));
        assert!(!encrypted.contains("order_id"));

        // Round-trip: decrypt_with_keys_or_legacy should recover the struct.
        let recovered: TestPayload =
            decrypt_with_keys_or_legacy(&encrypted, sk, &pk).expect("decrypt");
        assert_eq!(recovered, payload);
    }

    /// Phase 1.2 migration: legacy plaintext content (pre-upgrade events)
    /// must still parse via `decrypt_with_keys_or_legacy`.
    #[test]
    fn legacy_plaintext_still_parses() {
        let keys = test_keypair();
        let sk = keys.secret_key();
        let pk = keys.public_key();
        let payload = TestPayload {
            order_id: "legacy-order".to_string(),
            status: "pending".to_string(),
            my_trade_pubkey: None,
            pending_invoice: None,
        };
        let plaintext = serde_json::to_string(&payload).unwrap();
        assert!(!looks_encrypted(&plaintext));

        let recovered: TestPayload =
            decrypt_with_keys_or_legacy(&plaintext, sk, &pk).expect("legacy parse");
        assert_eq!(recovered, payload);
    }

    /// `looks_encrypted` distinguishes ciphertext from plaintext correctly.
    #[test]
    fn looks_encrypted_distinguishes_formats() {
        let keys = test_keypair();
        let plaintext = r#"{"order_id":"abc"}"#;
        assert!(!looks_encrypted(plaintext));

        let ciphertext = encrypt_with_keys(plaintext, keys.secret_key(), &keys.public_key())
            .expect("encrypt");
        assert!(looks_encrypted(&ciphertext));
    }

    /// Wrong key cannot decrypt (cross-user isolation check). Encrypts with
    /// one keypair, attempts decrypt with a different keypair — NIP-44
    /// should fail AND the ciphertext should NOT parse as plaintext.
    #[test]
    fn wrong_key_cannot_decrypt() {
        let keys_a = test_keypair();
        let keys_b = test_keypair();
        let payload = TestPayload {
            order_id: "secret".to_string(),
            status: "active".to_string(),
            my_trade_pubkey: Some("pk_a_only".to_string()),
            pending_invoice: None,
        };
        let json = serde_json::to_string(&payload).unwrap();
        let encrypted = encrypt_with_keys(&json, keys_a.secret_key(), &keys_a.public_key()).unwrap();

        // Decrypt with keys_b should fail (NIP-44 ECDH mismatch) AND the
        // ciphertext should not parse as plaintext JSON.
        let result: Result<TestPayload, String> =
            decrypt_with_keys_or_legacy(&encrypted, keys_b.secret_key(), &keys_b.public_key());
        assert!(result.is_err(), "decryption with wrong key must fail");
    }
}
