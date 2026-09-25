//! Vendored NIP-57 private/anonymous zap helpers.
//!
//! nostr 0.45 removed the private/anonymous zap helpers from the SDK
//! (`ZapType` was reduced to `Public` only). DIP-03 is nostr.blue's own
//! extension, so we own this crypto now. Ported from nostr 0.44.3 (MIT),
//! adapted to 0.45 APIs.

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use bech32::Bech32;
use nostr::event::{Event, EventBuilder, FinalizeEvent, Tag};
use nostr::key::{Keys, PublicKey, SecretKey};
use nostr::nips::nip57::ZapRequestData;
use sha2::{Digest, Sha256};

type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

const PRIVATE_ZAP_MSG_BECH32_HRP: &str = "pzap";
const PRIVATE_ZAP_IV_BECH32_HRP: &str = "iv";

/// Error type for the vendored helpers.
#[derive(Debug)]
pub enum PrivateZapCryptoError {
    Crypto(String),
    Sign(String),
    InvalidPrivateZapMessage,
    PrivateZapMessageNotFound,
}

impl core::fmt::Display for PrivateZapCryptoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Crypto(e) => write!(f, "private zap crypto error: {e}"),
            Self::Sign(e) => write!(f, "private zap sign error: {e}"),
            Self::InvalidPrivateZapMessage => write!(f, "invalid private zap message"),
            Self::PrivateZapMessageNotFound => write!(f, "private zap message not found"),
        }
    }
}

impl std::error::Error for PrivateZapCryptoError {}

/// Create an **anonymous** zap request (kind 9734 signed with an ephemeral
/// keypair, carrying an empty `["anon"]` tag).
pub fn anonymous_zap_request(data: ZapRequestData) -> Result<Event, PrivateZapCryptoError> {
    let keys = Keys::generate();
    let message: String = data.message.clone();
    let mut tags: Vec<Tag> = data.into();
    // 0.44 serialized `Anon { msg: None }` as the bare `["anon"]` tag.
    tags.push(Tag::custom("anon", [""]));
    EventBuilder::new(nostr::event::Kind::ZapRequest, message)
        .tags(tags)
        .finalize(&keys)
        .map_err(|e| PrivateZapCryptoError::Sign(e.to_string()))
}

/// Create the NIP-57 encryption key for a **private** zap
/// (`sha256(secret_hex || recipient_pubkey || created_at)`).
#[allow(dead_code)]
pub fn create_encryption_key(
    secret_key: &SecretKey,
    public_key: &PublicKey,
    created_at: nostr::types::time::Timestamp,
) -> Result<SecretKey, PrivateZapCryptoError> {
    let mut unhashed = secret_key.to_secret_hex();
    unhashed.push_str(&public_key.to_string());
    unhashed.push_str(&created_at.to_string());
    let hash = Sha256::digest(unhashed.as_bytes());
    SecretKey::from_slice(&hash).map_err(|e| PrivateZapCryptoError::Crypto(e.to_string()))
}

/// ECDH shared secret (x-coordinate of the shared point), matching the
/// 0.44 `nostr::util::generate_shared_key`.
fn generate_shared_key(
    secret_key: &SecretKey,
    public_key: &PublicKey,
) -> Result<[u8; 32], PrivateZapCryptoError> {
    use secp256k1::{ecdh, PublicKey as SecpPublicKey, Secp256k1, SecretKey as SecpSecretKey};
    let _secp = Secp256k1::new();
    let sk = SecpSecretKey::from_slice(secret_key.as_secret_bytes())
        .map_err(|e| PrivateZapCryptoError::Crypto(e.to_string()))?;
    // nostr PublicKey is x-only; lift to secp256k1 PublicKey with even parity.
    let xonly = secp256k1::XOnlyPublicKey::from_slice(&public_key.to_bytes())
        .map_err(|e| PrivateZapCryptoError::Crypto(e.to_string()))?;
    let pk = SecpPublicKey::from_x_only_public_key(xonly, secp256k1::Parity::Even);
    let secret_point = ecdh::shared_secret_point(&pk, &sk);
    let mut key = [0u8; 32];
    key.copy_from_slice(&secret_point[..32]);
    Ok(key)
}

/// Encrypt a private zap message: AES-256-CBC under the ECDH shared key,
/// bech32-encoded as `pzap1..._iv1...`.
pub fn encrypt_private_zap_message(
    rng: &mut impl rand::RngCore,
    secret_key: &SecretKey,
    public_key: &PublicKey,
    msg: impl AsRef<[u8]>,
) -> Result<String, PrivateZapCryptoError> {
    let key: [u8; 32] = generate_shared_key(secret_key, public_key)?;
    let mut iv: [u8; 16] = [0u8; 16];
    rng.fill_bytes(&mut iv);

    let cipher = Aes256CbcEnc::new(&key.into(), &iv.into());
    let msg: Vec<u8> = cipher.encrypt_padded_vec_mut::<Pkcs7>(msg.as_ref());

    let msg_hrp = bech32::Hrp::parse(PRIVATE_ZAP_MSG_BECH32_HRP)
        .map_err(bech32_hrp_err)?;
    let encrypted_bech32_msg =
        bech32::encode::<Bech32>(msg_hrp, &msg).map_err(bech32_err)?;
    let iv_hrp =
        bech32::Hrp::parse(PRIVATE_ZAP_IV_BECH32_HRP).map_err(bech32_hrp_err)?;
    let iv_bech32 = bech32::encode::<Bech32>(iv_hrp, &iv).map_err(bech32_err)?;

    Ok(format!("{encrypted_bech32_msg}_{iv_bech32}"))
}

/// Decrypt a **private** zap message that was received by the owner of
/// `secret_key` (the outer event's pubkey is the sender).
pub fn decrypt_received_private_zap_message(
    secret_key: &SecretKey,
    private_zap_event: &Event,
) -> Result<Event, PrivateZapCryptoError> {
    let key: [u8; 32] = generate_shared_key(secret_key, &private_zap_event.pubkey)?;
    decrypt_private_zap_message(key, private_zap_event)
}

/// Decrypt a **private** zap message that was sent by the owner of
/// `secret_key` to `public_key`.
#[allow(dead_code)]
pub fn decrypt_sent_private_zap_message(
    secret_key: &SecretKey,
    public_key: &PublicKey,
    private_zap_event: &Event,
) -> Result<Event, PrivateZapCryptoError> {
    let secret_key = create_encryption_key(secret_key, public_key, private_zap_event.created_at)?;
    let key: [u8; 32] = generate_shared_key(&secret_key, public_key)?;
    decrypt_private_zap_message(key, private_zap_event)
}

fn extract_anon_tag_message(event: &Event) -> Result<&str, PrivateZapCryptoError> {
    for tag in event.tags.iter() {
        if tag.kind() == "anon" {
            if let Some(msg) = tag.content() {
                if !msg.is_empty() {
                    return Ok(msg);
                }
            }
        }
    }
    Err(PrivateZapCryptoError::PrivateZapMessageNotFound)
}

fn decrypt_private_zap_message(
    key: [u8; 32],
    private_zap_event: &Event,
) -> Result<Event, PrivateZapCryptoError> {
    let msg = extract_anon_tag_message(private_zap_event)?;
    let mut splitted = msg.split('_');

    let msg = splitted
        .next()
        .ok_or(PrivateZapCryptoError::InvalidPrivateZapMessage)?;
    let iv = splitted
        .next()
        .ok_or(PrivateZapCryptoError::InvalidPrivateZapMessage)?;

    let (hrp, iv) = bech32::decode(iv).map_err(bech32_err)?;
    if hrp.as_str() != PRIVATE_ZAP_IV_BECH32_HRP {
        return Err(PrivateZapCryptoError::InvalidPrivateZapMessage);
    }

    let (_, data) = bech32::decode(msg).map_err(bech32_err)?;
    if data.len() % 16 != 0 {
        return Err(PrivateZapCryptoError::InvalidPrivateZapMessage);
    }

    let iv_bytes: [u8; 16] = iv[..16].try_into().unwrap();
    let decipher = Aes256CbcDec::new(&key.into(), &iv_bytes.into());
    let decrypted: Vec<u8> = decipher
        .decrypt_padded_vec_mut::<Pkcs7>(&data)
        .map_err(|_| PrivateZapCryptoError::InvalidPrivateZapMessage)?;

    serde_json::from_slice::<Event>(&decrypted)
        .map_err(|_| PrivateZapCryptoError::InvalidPrivateZapMessage)
}

fn bech32_err<E: core::fmt::Display>(e: E) -> PrivateZapCryptoError {
    PrivateZapCryptoError::Crypto(e.to_string())
}

fn bech32_hrp_err(e: bech32::primitives::hrp::Error) -> PrivateZapCryptoError {
    PrivateZapCryptoError::Crypto(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        use rand::rngs::OsRng;
        let bob = Keys::generate();
        let ephemeral = Keys::generate();
        let inner_event = nostr::event::EventBuilder::new(
            nostr::event::Kind::ZapPrivateMessage,
            "inner event json",
        )
        .finalize(&ephemeral)
        .unwrap();
        let payload = encrypt_private_zap_message(
            &mut OsRng,
            ephemeral.secret_key(),
            &bob.public_key(),
            inner_event.as_json(),
        )
        .unwrap();

        // Build a fake zap request event carrying the payload.
        let tags = vec![Tag::public_key(bob.public_key()), Tag::custom("anon", [payload])];
        let event = nostr::event::EventBuilder::new(nostr::event::Kind::ZapRequest, "")
            .tags(tags)
            .finalize(&ephemeral)
            .unwrap();

        let inner = decrypt_received_private_zap_message(bob.secret_key(), &event).unwrap();
        assert_eq!(inner.content, "inner event json");
        assert_eq!(inner.pubkey, ephemeral.public_key());
    }
}
