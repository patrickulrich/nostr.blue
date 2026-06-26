//! Mostro chat attachment encryption — spec-compatible format.
//!
//! Phase 5.1 (C7): migrated from nostr.blue's bespoke format to the
//! wire format used by mostro-cli, mostrix, and mostro/mobile:
//!
//! - **Nonce encoding**: hex (24 chars = 12 bytes). Was base64.
//! - **Blob layout**: `[nonce(12) || ciphertext(N) || tag(16)]` uploaded as
//!   a single blob. The nonce is extracted from the first 12 bytes on
//!   decrypt — the `nonce` field in `AttachmentMeta` is redundant for
//!   spec-format blobs but still required for legacy nostr.blue blobs.
//!   Was: nonce-less blob + nonce in JSON.
//! - **Key derivation**: raw ECDH shared secret (no domain-separation hash).
//!   Was: SHA-256("mostro-chat-attachment-key-v1" || shared_secret).
//!
//! See `mostro-cli/src/util/messaging.rs` and `mostro/mobile/lib/services/
//! encryption_service.dart` for the reference implementations.

#![allow(dead_code)]

use chacha20poly1305::{
    aead::Aead, ChaCha20Poly1305, KeyInit, Nonce,
};
use base64::Engine;

pub const NONCE_SIZE: usize = 12;
const MAX_FILE_SIZE: usize = 25 * 1024 * 1024;
const TAG_SIZE: usize = 16; // ChaCha20-Poly1305 auth tag

/// Spec-compatible attachment metadata carried inside the chat text
/// message as JSON. Matches the format used by mostro-cli, mostrix, and
/// mostro/mobile for cross-client interop.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AttachmentMeta {
    /// Discriminator: `"file_encrypted"` or `"image_encrypted"`.
    #[serde(rename = "type")]
    pub kind: AttachmentKind,
    /// Blossom blob URL of the encrypted blob.
    #[serde(rename = "blossom_url", alias = "url")]
    pub blossom_url: String,
    /// 24-char hex encoding of the 12-byte nonce.
    pub nonce: String,
    /// Original (plaintext) MIME type.
    pub mime_type: String,
    /// Original (plaintext) file size in bytes.
    #[serde(rename = "original_size", alias = "size")]
    pub original_size: u64,
    /// Original filename.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Encrypted blob size in bytes (original_size + TAG_SIZE + NONCE_SIZE).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_size: Option<u64>,
    /// Category: `"document"`, `"image"`, etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
    /// Image width in pixels (images only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Image height in pixels (images only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

/// Attachment type discriminator matching the reference clients.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AttachmentKind {
    #[serde(rename = "file_encrypted")]
    File,
    #[serde(rename = "image_encrypted")]
    Image,
}

/// Encrypt attachment data using ChaCha20-Poly1305 with the raw ECDH
/// shared secret as the key.
///
/// Returns a single blob: `[nonce(12) || ciphertext(N) || tag(16)]`.
/// The nonce is prepended so the blob is self-contained — the recipient
/// extracts it from the first 12 bytes on decrypt.
pub fn encrypt_attachment(data: &[u8], key: &[u8; 32]) -> Result<(Vec<u8>, [u8; NONCE_SIZE]), String> {
    if data.len() > MAX_FILE_SIZE {
        return Err(format!(
            "File too large: {} bytes (max {} bytes)",
            data.len(),
            MAX_FILE_SIZE
        ));
    }

    // Generate a random 12-byte nonce.
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce_bytes);

    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| format!("cipher init failed: {e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    // encrypt() appends the 16-byte Poly1305 auth tag to the ciphertext.
    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| format!("encryption failed: {e}"))?;

    // Build the spec blob: nonce(12) || ciphertext(N) || tag(16)
    let mut blob = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);

    Ok((blob, nonce_bytes))
}

/// Decrypt a spec-format attachment blob.
///
/// Expects: `[nonce(12) || ciphertext(N) || tag(16)]`. Extracts the nonce
/// from the first 12 bytes, then decrypts the remaining bytes.
///
/// Also supports legacy nostr.blue blobs that lack the leading nonce —
/// in that case, the caller must supply the nonce separately (see
/// `decrypt_attachment_legacy`). Detection is based on blob length: if
/// `blob.len() > 12 + 16` and the first 12 bytes don't look like a
/// valid nonce prefix, the function returns an error and the caller can
/// try the legacy path.
pub fn decrypt_attachment(blob: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, String> {
    if blob.len() < NONCE_SIZE + TAG_SIZE {
        return Err(format!(
            "blob too short: {} bytes (min {} for nonce + tag)",
            blob.len(),
            NONCE_SIZE + TAG_SIZE
        ));
    }

    // Extract nonce from the first 12 bytes.
    let nonce_bytes: [u8; NONCE_SIZE] = blob[..NONCE_SIZE]
        .try_into()
        .map_err(|_| "nonce extraction failed".to_string())?;
    let ciphertext = &blob[NONCE_SIZE..];

    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| format!("cipher init failed: {e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("decryption failed: {e}"))
}

/// Decrypt a legacy nostr.blue blob (no leading nonce) using an
/// explicitly-supplied nonce. Kept for backward compat during the
/// migration window.
pub fn decrypt_attachment_legacy(
    ciphertext: &[u8],
    nonce: &[u8; NONCE_SIZE],
    key: &[u8; 32],
) -> Result<Vec<u8>, String> {
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| format!("cipher init failed: {e}"))?;
    let nonce = Nonce::from_slice(nonce);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("legacy decryption failed: {e}"))
}

/// Phase 5.1 (C7): return the raw ECDH shared secret as the key.
///
/// Previously this applied a domain-separation hash
/// (`SHA-256("mostro-chat-attachment-key-v1" || shared_secret)`), which
/// was incompatible with mostro-cli/mostrix/mobile. The spec uses the
/// raw ECDH shared secret directly.
pub fn attachment_key_from_shared_secret(secret: &[u8; 32]) -> [u8; 32] {
    *secret
}

/// Encode a 12-byte nonce as a 24-char hex string (spec format).
pub fn encode_nonce(nonce: &[u8; NONCE_SIZE]) -> String {
    hex::encode(nonce)
}

impl AttachmentMeta {
    /// Parse the nonce field as a 12-byte array.
    ///
    /// Tries hex first (spec format, 24 chars). Falls back to base64
    /// (legacy nostr.blue format) for backward compat.
    pub fn parse_nonce(&self) -> Result<[u8; NONCE_SIZE], String> {
        // Spec: hex-encoded 24 chars.
        if let Ok(bytes) = hex::decode(&self.nonce) {
            if bytes.len() == NONCE_SIZE {
                let mut arr = [0u8; NONCE_SIZE];
                arr.copy_from_slice(&bytes);
                return Ok(arr);
            }
        }

        // Legacy: base64-encoded 16 chars.
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&self.nonce) {
            if bytes.len() == NONCE_SIZE {
                let mut arr = [0u8; NONCE_SIZE];
                arr.copy_from_slice(&bytes);
                return Ok(arr);
            }
        }

        Err(format!(
            "nonce must be 24 hex chars or base64 of 12 bytes, got {:?}",
            self.nonce
        ))
    }

    /// Classify the attachment as image or file based on MIME type.
    pub fn classify(mime_type: &str) -> AttachmentKind {
        if mime_type.starts_with("image/") {
            AttachmentKind::Image
        } else {
            AttachmentKind::File
        }
    }

    /// Human-readable file type label.
    pub fn file_type_label(mime_type: &str) -> &'static str {
        if mime_type.starts_with("image/") {
            "image"
        } else if mime_type.starts_with("video/") {
            "video"
        } else if mime_type.starts_with("audio/") {
            "audio"
        } else {
            "document"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip_spec_format() {
        let key = [0x42u8; 32];
        let plaintext = b"Hello Mostro attachment encryption!";
        let (blob, nonce) = encrypt_attachment(plaintext, &key).unwrap();

        // Spec: blob must start with the nonce.
        assert_eq!(&blob[..NONCE_SIZE], &nonce);
        // Blob must be nonce + ciphertext + tag.
        assert_eq!(blob.len(), NONCE_SIZE + plaintext.len() + TAG_SIZE);

        // Decrypt must recover the original.
        let recovered = decrypt_attachment(&blob, &key).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn test_decrypt_rejects_short_blob() {
        let key = [0x42u8; 32];
        let short_blob = vec![0u8; 10]; // Too short.
        assert!(decrypt_attachment(&short_blob, &key).is_err());
    }

    #[test]
    fn test_wrong_key_fails() {
        let key_a = [0x42u8; 32];
        let key_b = [0x99u8; 32];
        let (blob, _) = encrypt_attachment(b"secret", &key_a).unwrap();
        assert!(decrypt_attachment(&blob, &key_b).is_err());
    }

    #[test]
    fn test_encode_nonce_is_hex() {
        let nonce = [0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01];
        let encoded = encode_nonce(&nonce);
        assert_eq!(encoded.len(), 24, "hex nonce must be 24 chars");
        assert!(encoded.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_parse_nonce_hex_spec_format() {
        let nonce = [0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01];
        let meta = AttachmentMeta {
            kind: AttachmentKind::File,
            blossom_url: "https://blossom.test/blob".to_string(),
            nonce: encode_nonce(&nonce),
            mime_type: "application/pdf".to_string(),
            original_size: 100,
            filename: None,
            encrypted_size: None,
            file_type: None,
            width: None,
            height: None,
        };
        let parsed = meta.parse_nonce().unwrap();
        assert_eq!(parsed, nonce);
    }

    #[test]
    fn test_parse_nonce_legacy_base64() {
        let nonce = [0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01];
        let legacy_encoded = base64::engine::general_purpose::STANDARD.encode(nonce);
        let meta = AttachmentMeta {
            kind: AttachmentKind::File,
            blossom_url: "https://blossom.test/blob".to_string(),
            nonce: legacy_encoded,
            mime_type: "application/pdf".to_string(),
            original_size: 100,
            filename: None,
            encrypted_size: None,
            file_type: None,
            width: None,
            height: None,
        };
        let parsed = meta.parse_nonce().unwrap();
        assert_eq!(parsed, nonce);
    }

    #[test]
    fn test_attachment_key_is_raw_secret() {
        let secret = [0x42u8; 32];
        let key = attachment_key_from_shared_secret(&secret);
        assert_eq!(key, secret, "key must be the raw ECDH shared secret");
    }

    #[test]
    fn test_classify_mime_types() {
        assert_eq!(AttachmentMeta::classify("image/png"), AttachmentKind::Image);
        assert_eq!(AttachmentMeta::classify("application/pdf"), AttachmentKind::File);
        assert_eq!(AttachmentMeta::classify("text/plain"), AttachmentKind::File);
    }

    #[test]
    fn test_attachment_meta_serde_roundtrip() {
        let meta = AttachmentMeta {
            kind: AttachmentKind::Image,
            blossom_url: "https://blossom.test/x".to_string(),
            nonce: "abcdef0123456789abcdef0123456789abcdef0123456789".to_string()[..24].to_string(),
            mime_type: "image/png".to_string(),
            original_size: 12345,
            filename: Some("photo.png".to_string()),
            encrypted_size: Some(12373),
            file_type: Some("image".to_string()),
            width: Some(800),
            height: Some(600),
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("\"type\":\"image_encrypted\""));
        assert!(json.contains("\"blossom_url\""));
        assert!(json.contains("\"original_size\""));
        let parsed: AttachmentMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.kind, AttachmentKind::Image);
        assert_eq!(parsed.blossom_url, meta.blossom_url);
    }

    #[test]
    fn test_attachment_meta_legacy_url_alias() {
        // Old nostr.blue format used "url" instead of "blossom_url".
        let legacy_json = r#"{"type":"file_encrypted","url":"https://x.test/b","nonce":"abcdef","mime_type":"text/plain","original_size":10}"#;
        let parsed: AttachmentMeta = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(parsed.blossom_url, "https://x.test/b");
    }

    #[test]
    fn test_attachment_meta_legacy_size_alias() {
        // Old nostr.blue format used "size" instead of "original_size".
        let legacy_json = r#"{"type":"file_encrypted","blossom_url":"https://x.test/b","nonce":"abcdef","mime_type":"text/plain","size":999}"#;
        let parsed: AttachmentMeta = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(parsed.original_size, 999);
    }
}
