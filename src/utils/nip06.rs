//! NIP-06: Basic key derivation from mnemonic seed phrase
//!
//! This is a generic BIP-39 / NIP-06 helper used by:
//! - Mostro P2P exchange (separate trade mnemonic)
//! - Cloud backup (Google Drive key backup)
//!
//! Path: `m/44'/{COIN}'/{account}'/{type}/{index}` where COIN = 1237 for Nostr.
//!
//! Reference: <https://github.com/nostr-protocol/nips/blob/master/06.md>

use bip39::Mnemonic;
use nostr::Keys;
use nostr::nips::nip06::FromMnemonic;
use rand::Rng;

/// Generate a fresh 12-word BIP-39 mnemonic using 128 bits of entropy.
///
/// Returns the space-separated words. To derive keys, pass this string to
/// [`keys_from_mnemonic`] or [`derive_at`].
pub fn generate_mnemonic() -> Result<String, String> {
    let entropy: [u8; 16] = rand::thread_rng().gen();
    let mnemonic = Mnemonic::from_entropy(&entropy).map_err(|e| e.to_string())?;
    Ok(mnemonic.to_string())
}

/// Derive a Nostr `Keys` from a BIP-39 mnemonic and optional passphrase.
///
/// Uses the default NIP-06 path `m/44'/1237'/0'/0/0`.
#[allow(dead_code)]
pub fn keys_from_mnemonic(words: &str, passphrase: Option<&str>) -> Result<Keys, String> {
    Keys::from_mnemonic(words, passphrase).map_err(|e| e.to_string())
}

/// Derive a Nostr `Keys` at a specific NIP-06 path.
///
/// Path: `m/44'/1237'/{account}'/{type}/{index}` (the last three components are
/// caller-controlled). Pass `Some(0)` explicitly for any of `account`/`type`/`index`
/// to make the path obvious and robust against future rust-nostr changes.
pub fn derive_at(
    mnemonic: &str,
    passphrase: Option<&str>,
    account: u32,
    type_index: u32,
    index: u32,
) -> Result<Keys, String> {
    Keys::from_mnemonic_advanced(
        mnemonic,
        passphrase,
        Some(account),
        Some(type_index),
        Some(index),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_mnemonic_is_twelve_words() {
        let words = generate_mnemonic().unwrap();
        let count = words.split_whitespace().count();
        assert_eq!(count, 12, "expected 12-word mnemonic, got {} words", count);
    }

    #[test]
    fn test_keys_from_mnemonic_deterministic() {
        // Fixed test vector (well-known BIP-39 mnemonic)
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let k1 = keys_from_mnemonic(words, None).unwrap();
        let k2 = keys_from_mnemonic(words, None).unwrap();
        assert_eq!(k1.public_key().to_hex(), k2.public_key().to_hex());
    }

    #[test]
    fn test_derive_at_explicit_zero_path() {
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        // m/44'/1237'/0'/0/0 (default NIP-06)
        let k_default = keys_from_mnemonic(words, None).unwrap();
        let k_explicit = derive_at(words, None, 0, 0, 0).unwrap();
        assert_eq!(
            k_default.public_key().to_hex(),
            k_explicit.public_key().to_hex(),
            "explicit (0,0,0) must match default NIP-06 path"
        );
    }

    #[test]
    fn test_derive_at_different_index_produces_different_key() {
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let k0 = derive_at(words, None, 38383, 0, 0).unwrap();
        let k1 = derive_at(words, None, 38383, 0, 1).unwrap();
        assert_ne!(
            k0.public_key().to_hex(),
            k1.public_key().to_hex(),
            "different indices must produce different keys"
        );
    }

    #[test]
    fn test_passphrase_changes_key() {
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let k_empty = keys_from_mnemonic(words, None).unwrap();
        let k_pass = keys_from_mnemonic(words, Some("secret")).unwrap();
        assert_ne!(k_empty.public_key().to_hex(), k_pass.public_key().to_hex());
    }
}
