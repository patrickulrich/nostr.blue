//! NIP-49 Private Key Encryption Utilities
//!
//! Helpers for encrypting and decrypting private keys with passwords.
//! Uses scrypt key derivation and XChaCha20Poly1305 encryption.
use nostr::nips::nip19::{FromBech32, ToBech32};
use nostr::nips::nip49::{EncryptedSecretKey, KeySecurity};
use nostr::{Keys, SecretKey};
use rand::rngs::OsRng;
/// Default scrypt log_n parameter (16 = 65536 iterations)
/// Higher values increase security but also decryption time
pub const DEFAULT_LOG_N: u8 = 16;
/// Error types for NIP-49 operations
#[derive(Debug, Clone)]
pub enum EncryptionError {
    /// Invalid password (wrong password or corrupted data)
    InvalidPassword,
    /// Invalid ncryptsec format
    InvalidFormat(String),
    /// Encryption failed
    EncryptionFailed(String),
}
impl std::fmt::Display for EncryptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPassword => write!(f, "Invalid password"),
            Self::InvalidFormat(e) => write!(f, "Invalid ncryptsec format: {}", e),
            Self::EncryptionFailed(e) => write!(f, "Encryption failed: {}", e),
        }
    }
}
/// Encrypt a secret key with a password
///
/// Returns the ncryptsec bech32 string for storage.
/// Uses KeySecurity::Unknown since we don't know the key's history.
pub fn encrypt_secret_key(
    secret_key: &SecretKey,
    password: &str,
) -> Result<String, EncryptionError> {
    encrypt_secret_key_with_security(secret_key, password, KeySecurity::Unknown)
}
/// Encrypt a secret key with a password and specified security level
///
/// Use KeySecurity::Weak for keys that were previously stored unencrypted.
/// Use KeySecurity::Medium for keys that have always been encrypted.
/// Use KeySecurity::Unknown if the security history is not tracked.
pub fn encrypt_secret_key_with_security(
    secret_key: &SecretKey,
    password: &str,
    key_security: KeySecurity,
) -> Result<String, EncryptionError> {
    let encrypted = EncryptedSecretKey::new_with_rng(
            &mut OsRng,
            secret_key,
            password,
            DEFAULT_LOG_N,
            key_security,
        )
        .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;
    encrypted.to_bech32().map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))
}
/// Decrypt an ncryptsec string with a password
///
/// Returns the Keys struct on success.
pub fn decrypt_ncryptsec(
    ncryptsec: &str,
    password: &str,
) -> Result<Keys, EncryptionError> {
    let encrypted = EncryptedSecretKey::from_bech32(ncryptsec)
        .map_err(|e| EncryptionError::InvalidFormat(e.to_string()))?;
    let secret_key = encrypted
        .decrypt(password)
        .map_err(|_| EncryptionError::InvalidPassword)?;
    Ok(Keys::new(secret_key))
}
/// Check if a string is an ncryptsec (encrypted) format
pub fn is_ncryptsec(s: &str) -> bool {
    s.starts_with("ncryptsec1")
}
/// Validate password strength
///
/// Returns error message if password is too weak.
pub fn validate_password(password: &str) -> Option<String> {
    if password.len() < 8 {
        return Some("Password must be at least 8 characters".to_string());
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    const TEST_NCRYPTSEC: &str = "ncryptsec1qgg9947rlpvqu76pj5ecreduf9jxhselq2nae2kghhvd5g7dgjtcxfqtd67p9m0w57lspw8gsq6yphnm8623nsl8xn9j4jdzz84zm3frztj3z7s35vpzmqf6ksu8r89qk5z2zxfmu5gv8th8wclt0h4p";
    const TEST_SECRET_KEY: &str = "3501454135014541350145413501453fefb02227e449e57cf4d3a3ce05378683";
    const TEST_PASSWORD: &str = "nostr";
    #[test]
    fn test_decrypt_ncryptsec() {
        let keys = decrypt_ncryptsec(TEST_NCRYPTSEC, TEST_PASSWORD).unwrap();
        assert_eq!(keys.secret_key().to_secret_hex(), TEST_SECRET_KEY);
    }
    #[test]
    fn test_decrypt_wrong_password() {
        let result = decrypt_ncryptsec(TEST_NCRYPTSEC, "wrong");
        assert!(matches!(result, Err(EncryptionError::InvalidPassword)));
    }
    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let original = SecretKey::from_hex(TEST_SECRET_KEY).unwrap();
        let password = "test_password_123";
        let encrypted = encrypt_secret_key(&original, password).unwrap();
        assert!(is_ncryptsec(&encrypted));
        let keys = decrypt_ncryptsec(&encrypted, password).unwrap();
        assert_eq!(keys.secret_key().to_secret_hex(), TEST_SECRET_KEY);
    }
    #[test]
    fn test_is_ncryptsec() {
        assert!(is_ncryptsec(TEST_NCRYPTSEC));
        assert!(!is_ncryptsec("nsec1abc"));
        assert!(!is_ncryptsec("npub1abc"));
    }
    #[test]
    fn test_validate_password() {
        assert!(validate_password("short").is_some());
        assert!(validate_password("1234567").is_some());
        assert!(validate_password("12345678").is_none());
        assert!(validate_password("a_good_password").is_none());
    }
}
