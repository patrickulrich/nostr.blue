use base64::Engine;
use nostr::nips::nip06::FromMnemonic;
use nostr::nips::nip44::v2::{ConversationKey, decrypt_to_bytes, encrypt_to_bytes};
use nostr::Keys;

use super::types::BackupBundle;

const BACKUP_KEY_SALT: &[u8] = b"nostrblue-backup-v1";

pub fn derive_backup_key(sub: &str) -> [u8; 32] {
    use bitcoin_hashes::hmac::{Hmac, HmacEngine};
    use bitcoin_hashes::{sha256, Hash, HashEngine};
    let mut engine = HmacEngine::<sha256::Hash>::new(BACKUP_KEY_SALT);
    engine.input(sub.as_bytes());
    let hmac: Hmac<sha256::Hash> = Hmac::from_engine(engine);
    hmac.to_byte_array()
}

pub fn encrypt_bundle(bundle: &BackupBundle, key: &[u8; 32]) -> Result<String, String> {
    let json = serde_json::to_vec(bundle).map_err(|e| e.to_string())?;
    let conv_key = ConversationKey::new(*key);
    let ciphertext = encrypt_to_bytes(&conv_key, &json).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&ciphertext))
}

pub fn decrypt_bundle(payload_b64: &str, key: &[u8; 32]) -> Result<BackupBundle, String> {
    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(payload_b64)
        .map_err(|e| e.to_string())?;
    let conv_key = ConversationKey::new(*key);
    let json = decrypt_to_bytes(&conv_key, &ciphertext).map_err(|e| e.to_string())?;
    serde_json::from_slice(&json).map_err(|e| e.to_string())
}

pub fn generate_mnemonic_and_keys() -> Result<(String, Keys), String> {
    use rand::Rng;
    let entropy: [u8; 16] = rand::thread_rng().gen();
    let mnemonic = bip39::Mnemonic::from_entropy(&entropy).map_err(|e| e.to_string())?;
    let words = mnemonic.to_string();
    let keys = Keys::from_mnemonic(&words, None).map_err(|e| e.to_string())?;
    Ok((words, keys))
}

pub fn generate_auto_password() -> String {
    use rand::Rng;
    let bytes: [u8; 32] = rand::thread_rng().gen();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_backup_key_stable() {
        let key1 = derive_backup_key("user123");
        let key2 = derive_backup_key("user123");
        assert_eq!(key1, key2);
        let key3 = derive_backup_key("user456");
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let sub = "test-google-sub-12345";
        let key = derive_backup_key(sub);
        let bundle = BackupBundle {
            nsec_hex: "0".repeat(64),
            nwc_uri: Some("nostr+walletconnect://test".to_string()),
            account_label: Some("test account".to_string()),
            created_at: 1234567890,
        };
        let encrypted = encrypt_bundle(&bundle, &key).unwrap();
        let decrypted = decrypt_bundle(&encrypted, &key).unwrap();
        assert_eq!(decrypted.nsec_hex, bundle.nsec_hex);
        assert_eq!(decrypted.nwc_uri, bundle.nwc_uri);
        assert_eq!(decrypted.account_label, bundle.account_label);
        assert_eq!(decrypted.created_at, bundle.created_at);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = derive_backup_key("user1");
        let key2 = derive_backup_key("user2");
        let bundle = BackupBundle {
            nsec_hex: "a".repeat(64),
            nwc_uri: None,
            account_label: None,
            created_at: 0,
        };
        let encrypted = encrypt_bundle(&bundle, &key1).unwrap();
        assert!(decrypt_bundle(&encrypted, &key2).is_err());
    }

    #[test]
    fn test_generate_mnemonic() {
        let (words, keys) = generate_mnemonic_and_keys().unwrap();
        let word_count = words.split_whitespace().count();
        assert_eq!(word_count, 12);
        assert!(!keys.secret_key().to_secret_hex().is_empty());
    }
}
