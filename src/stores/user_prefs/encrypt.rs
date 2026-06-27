//! Encrypt/decrypt helpers for the unified preference blobs.
//!
//! Two tracks, forced by the SDK's NIP-44 API surface:
//!
//! - **Main signer** (async): `signer.nip44_encrypt(&pubkey, json).await`.
//!   Required for NIP-07/46/55 users whose secret key is not local.
//! - **Mostro identity key** (sync): `nip44::encrypt_with_rng(sk, pk, ...)`.
//!   The Mostro mnemonic is always-local, enabling sync encryption without
//!   a signer round-trip.
//!
//! ## Legacy fallback
//!
//! [`decrypt_blob_from_content`] tries NIP-44 first. If that fails AND the
//! content parses as valid JSON of type `T`, treats it as a legacy plaintext
//! event (pre-encryption-upgrade) and returns the parsed value. This is
//! the same pattern as `private_app_data::decrypt_from_self_or_legacy`.

use nostr::EventId;
use serde::de::DeserializeOwned;
use serde::Serialize;

// ─── Main signer track (async) ──────────────────────────────────────────

/// Encrypt a JSON string to self using the user's main signer.
///
/// This is an async operation because NIP-07 (browser extension), NIP-46
/// (bunker), and NIP-55 (Android signer) all require round-trips to an
/// external signing device/app. The debounce in `save.rs` coalesces rapid
/// edits to minimize signer prompts.
pub async fn encrypt_to_self_signer(json: &str) -> Result<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("No signer: {e}"))?;
    let pubkey = crate::stores::nostr_client::get_cached_pubkey()?;
    signer
        .nip44_encrypt(&pubkey, json)
        .await
        .map_err(|e| format!("NIP-44 encrypt: {e}"))
}

/// Decrypt content from self using the user's main signer, with legacy
/// plaintext fallback for pre-encryption-upgrade events.
pub async fn decrypt_from_self_signer<T: DeserializeOwned>(
    content: &str,
    _event_id: EventId,
) -> Result<T, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("No signer: {e}"))?;
    let pubkey = crate::stores::nostr_client::get_cached_pubkey()?;
    match signer.nip44_decrypt(&pubkey, content).await {
        Ok(plaintext) => serde_json::from_str(&plaintext)
            .map_err(|e| format!("decrypted JSON parse: {e}")),
        Err(nip44_err) => {
            // Fall through to legacy plaintext path.
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

// ─── Mostro identity key track (sync) ───────────────────────────────────

/// Encrypt a JSON string to self using the Mostro identity key (sync).
/// Delegates to `private_app_data::encrypt_to_self`.
pub fn encrypt_to_self_mostro(json: &str) -> Result<String, String> {
    crate::stores::private_app_data::encrypt_to_self(json)
}

/// Decrypt content from self using the Mostro identity key, with legacy
/// plaintext fallback. Delegates to `private_app_data::decrypt_from_self_or_legacy`.
pub fn decrypt_from_self_mostro<T: DeserializeOwned>(content: &str) -> Result<T, String> {
    crate::stores::private_app_data::decrypt_from_self_or_legacy(content)
}

// ─── Low-level helpers (testable without Dioxus runtime) ────────────────

/// Encrypt JSON to self using explicit keys (for tests).
#[cfg(test)]
pub(crate) fn encrypt_with_keys(
    plaintext: &str,
    sk: &nostr::SecretKey,
    pk: &nostr::PublicKey,
) -> Result<String, String> {
    use nostr::nips::nip44::{self, Version};
    use nostr::secp256k1::rand::rngs::OsRng;
    nip44::encrypt_with_rng(&mut OsRng, sk, pk, plaintext.as_bytes(), Version::V2)
        .map_err(|e| format!("NIP-44 encrypt: {e}"))
}

/// Build a kind 30078 event builder with the given d-tag and encrypted
/// content.
pub fn build_event_builder(
    d_tag: &str,
    encrypted_content: String,
) -> nostr::EventBuilder {
    nostr::EventBuilder::new(nostr::Kind::from(30078), encrypted_content)
        .tag(nostr::Tag::identifier(d_tag))
}

/// Serialize a payload, encrypt it, and return the event builder.
/// Uses the Mostro identity key (sync track).
pub fn build_encrypted_mostro_event_builder<T: Serialize>(
    d_tag: &str,
    payload: &T,
) -> Result<nostr::EventBuilder, String> {
    let json = serde_json::to_string(payload).map_err(|e| format!("serialize: {e}"))?;
    let encrypted = encrypt_to_self_mostro(&json)?;
    Ok(build_event_builder(d_tag, encrypted))
}

/// Serialize a payload and encrypt it via the main signer (async track).
pub async fn build_encrypted_signer_event_builder<T: Serialize>(
    d_tag: &str,
    payload: &T,
) -> Result<nostr::EventBuilder, String> {
    let json = serde_json::to_string(payload).map_err(|e| format!("serialize: {e}"))?;
    let encrypted = encrypt_to_self_signer(&json).await?;
    Ok(build_event_builder(d_tag, encrypted))
}

/// Generic decrypt helper that picks the right track based on content.
/// Tries NIP-44 with explicit keys first; falls back to plaintext.
#[cfg(test)]
pub(crate) fn decrypt_with_keys_or_legacy<T: DeserializeOwned>(
    content: &str,
    sk: &nostr::SecretKey,
    pk: &nostr::PublicKey,
) -> Result<T, String> {
    use nostr::nips::nip44;
    match nip44::decrypt(sk, pk, content) {
        Ok(plaintext) => {
            serde_json::from_str(&plaintext).map_err(|e| format!("decrypted JSON parse: {e}"))
        }
        Err(nip44_err) => match serde_json::from_str::<T>(content) {
            Ok(v) => {
                log::info!(
                    "Falling back to plaintext NIP-78 parse: NIP-44 error was {nip44_err}"
                );
                Ok(v)
            }
            Err(parse_err) => Err(format!(
                "NIP-44 decrypt failed ({nip44_err}) and plaintext parse failed ({parse_err})"
            )),
        },
    }
}
