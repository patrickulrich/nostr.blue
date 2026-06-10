#![allow(dead_code)]

use base64::Engine;

const NONCE_SIZE: usize = 12;
const MAX_FILE_SIZE: usize = 25 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AttachmentMeta {
    pub url: String,
    pub nonce: String,
    pub mime_type: String,
    pub size: u64,
}

pub fn encrypt_attachment(data: &[u8], key: &[u8; 32]) -> Result<(Vec<u8>, [u8; NONCE_SIZE]), String> {
    if data.len() > MAX_FILE_SIZE {
        return Err(format!(
            "File too large: {} bytes (max {} bytes)",
            data.len(),
            MAX_FILE_SIZE
        ));
    }
    use chacha20poly1305::{
        ChaCha20Poly1305, KeyInit, Nonce,
        aead::Aead,
    };
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| format!("cipher init failed: {e}"))?;
    let nonce_bytes = {
        let mut n = [0u8; NONCE_SIZE];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut n);
        n
    };
    let nonce = Nonce::from_slice(&nonce_bytes);
    let encrypted = cipher
        .encrypt(nonce, data)
        .map_err(|e| format!("encryption failed: {e}"))?;
    Ok((encrypted, nonce_bytes))
}

pub fn decrypt_attachment(data: &[u8], nonce: &[u8; NONCE_SIZE], key: &[u8; 32]) -> Result<Vec<u8>, String> {
    use chacha20poly1305::{
        ChaCha20Poly1305, KeyInit, Nonce,
        aead::Aead,
    };
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| format!("cipher init failed: {e}"))?;
    let nonce = Nonce::from_slice(nonce);
    cipher
        .decrypt(nonce, data)
        .map_err(|e| format!("decryption failed: {e}"))
}

pub fn attachment_key_from_shared_secret(secret: &[u8; 32]) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(b"mostro-chat-attachment-key-v1");
    hasher.update(secret);
    let hash = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&hash);
    key
}

impl AttachmentMeta {
    pub fn parse_nonce(&self) -> Result<[u8; NONCE_SIZE], String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&self.nonce)
            .map_err(|e| format!("invalid nonce base64: {e}"))?;
        if bytes.len() != NONCE_SIZE {
            return Err(format!("nonce must be {} bytes, got {}", NONCE_SIZE, bytes.len()));
        }
        let mut arr = [0u8; NONCE_SIZE];
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }
}

pub fn encode_nonce(nonce: &[u8; NONCE_SIZE]) -> String {
    base64::engine::general_purpose::STANDARD.encode(nonce)
}
