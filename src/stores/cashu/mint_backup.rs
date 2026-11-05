//! NUT-27 mint-list backup.
//!
//! Publishes the user's mint list as a NIP-78 addressable event (kind
//! 30078, `d`-tag `mint-list`) encrypted with NIP-44 to self under
//! deterministic keys derived per NUT-27 — the same scheme as cashu's
//! `nut27` module and cashu.me's `nostrMintBackup`.
//!
//! ## Key derivation
//! NUT-27 derives its keys from `SHA256(bip39_seed || "cashu-mint-backup")`
//! where `bip39_seed = mnemonic.to_seed("")`. nostr.blue mediates that
//! through BIP-39: the wallet seed is hashed into 32 bytes of entropy,
//! rendered as a 24-word mnemonic (`backup_mnemonic_words` reveals it), and
//! re-expanded with an empty passphrase. The interop claim therefore holds
//! *given the revealed phrase*: any NUT-27 client fed the same words
//! derives the same keys and can read the backup, independently of the
//! nostr identity key. Restores try the legacy direct-wallet-seed keys as
//! a fallback so backup events published before the BIP-39 mediation still
//! resolve.
//!
//! Ported from `cashu::nuts::nut27` (its event builder is typed against
//! cashu's nostr-sdk 0.44 — a different nostr instance than ours), so the
//! derivation, event construction, and parsing live here against nostr
//! 0.45 and the shared publish queue. The `d`-tag namespace (`mint-list`)
//! is disjoint from our NIP-78 preference blobs (`nostr.blue/*`), and both
//! our prefs subscriptions and fetches filter by `d` server-side, so the
//! two event families never interact.

use cdk::mint_url::MintUrl;
use nostr::event::{Event, EventBuilder, FinalizeEvent, Kind, Tag};
use nostr::key::Keys;
use nostr::nips::nip44;
use nostr_sdk::prelude::Filter;

use crate::stores::nostr_client;

/// NIP-78 application-specific addressable kind used by NUT-27.
const KIND_MINT_BACKUP: u16 = 30078;
/// NUT-27 `d`-tag identifier (distinct from `nostr.blue/prefs` etc.).
const MINT_LIST_IDENTIFIER: &str = "mint-list";
/// NUT-27 key-derivation domain separator.
const DOMAIN_SEPARATOR: &[u8] = b"cashu-mint-backup";
/// Domain separator for the BIP-39 entropy that feeds the NUT-27 mnemonic.
const NUT27_ENTROPY_DOMAIN: &[u8] = b"nut27-mnemonic-entropy";

/// NUT-27 backup payload (the NIP-44 plaintext).
///
/// Ported from `cashu::nuts::nut27::MintBackup` (identical serde shape)
/// because the upstream module is feature-gated behind cashu's nostr-sdk
/// 0.44 dependency — a different nostr instance than ours.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MintBackup {
    /// List of mint URLs.
    pub mints: Vec<MintUrl>,
    /// Unix timestamp of when the backup was created.
    pub timestamp: u64,
}

impl MintBackup {
    /// Create a new mint backup with the current timestamp.
    pub fn new(mints: Vec<MintUrl>) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self { mints, timestamp }
    }
}

/// Deterministically derive the NUT-27 backup keys from the BIP-39 seed:
/// `SHA256(seed || "cashu-mint-backup")` → nostr secret key (NUT-27).
pub fn derive_backup_keys(seed: &[u8; 64]) -> Result<Keys, String> {
    let mut combined = Vec::with_capacity(seed.len() + DOMAIN_SEPARATOR.len());
    combined.extend_from_slice(seed);
    combined.extend_from_slice(DOMAIN_SEPARATOR);
    let private_key_bytes = bitcoin_hashes_1::sha256::Hash::hash(&combined).to_byte_array();
    let secret_key =
        nostr::key::SecretKey::from_slice(&private_key_bytes).map_err(|e| e.to_string())?;
    Ok(Keys::new(secret_key))
}

/// BIP-39 entropy for the NUT-27 mnemonic:
/// `SHA256(wallet_seed || "nut27-mnemonic-entropy")` — 32 bytes → 24 words.
fn nut27_entropy(wallet_seed: &[u8; 64]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(wallet_seed);
    hasher.update(NUT27_ENTROPY_DOMAIN);
    hasher.finalize().into()
}

/// The NUT-27 mnemonic deterministically derived from the wallet seed
/// (empty passphrase when expanded). Published backups are only portable
/// to other NUT-27 clients if this phrase is revealed to the user.
fn nut27_mnemonic(wallet_seed: &[u8; 64]) -> Result<bip39::Mnemonic, String> {
    bip39::Mnemonic::from_entropy(&nut27_entropy(wallet_seed)).map_err(|e| e.to_string())
}

/// BIP-39 seed the NUT-27 backup keys are derived from:
/// `nut27_mnemonic(wallet_seed).to_seed("")`.
fn nut27_seed(wallet_seed: &[u8; 64]) -> Result<[u8; 64], String> {
    Ok(nut27_mnemonic(wallet_seed)?.to_seed(""))
}

/// NUT-27 backup keys under the current (BIP-39-mediated) scheme.
fn nut27_backup_keys(wallet_seed: &[u8; 64]) -> Result<Keys, String> {
    derive_backup_keys(&nut27_seed(wallet_seed)?)
}

/// The 24 BIP-39 words behind the NUT-27 backup keys, for the "reveal
/// backup phrase" UI. Anyone with these words can read the mint-list
/// backup on any NUT-27 client; losing them (and the nostr login) makes
/// the backup unreadable elsewhere.
pub async fn backup_mnemonic_words() -> Result<Vec<String>, String> {
    let wallet_seed = super::internal::derive_wallet_seed().await?;
    Ok(nut27_mnemonic(&wallet_seed)?
        .words()
        .map(str::to_string)
        .collect())
}

/// Build the signed kind-30078 mint-list backup event (NIP-44
/// self-encrypted under the seed-derived keys).
pub fn build_backup_event(seed: &[u8; 64], mints: Vec<MintUrl>) -> Result<Event, String> {
    let keys = derive_backup_keys(seed)?;
    let backup = MintBackup::new(mints);
    let plaintext = serde_json::to_string(&backup).map_err(|e| e.to_string())?;
    let content = nip44::encrypt(
        keys.secret_key(),
        &keys.public_key(),
        plaintext,
        nip44::Version::V2,
    )
    .map_err(|e| e.to_string())?;
    EventBuilder::new(Kind::from(KIND_MINT_BACKUP), content)
        .tag(Tag::identifier(MINT_LIST_IDENTIFIER))
        .tag(Tag::custom("client", ["nostr.blue"]))
        .finalize(&keys)
        .map_err(|e| e.to_string())
}

/// Decrypt and parse a mint backup event's content.
fn parse_backup_event(event: &Event, keys: &Keys) -> Result<MintBackup, String> {
    let plaintext = nip44::decrypt(
        keys.secret_key(),
        &keys.public_key(),
        event.content.as_str(),
    )
    .map_err(|e| format!("NUT-27 decrypt failed: {e}"))?;
    serde_json::from_str(&plaintext).map_err(|e| format!("NUT-27 parse failed: {e}"))
}

/// Publish the current mint list as a NUT-27 backup event through the
/// shared publish queue (pre-signed: the seed-derived keys, not the
/// identity, sign the event).
pub async fn publish_mint_backup() -> Result<String, String> {
    let mints: Vec<MintUrl> = super::mint_mgmt::get_mints()
        .into_iter()
        .filter_map(|url| url.parse().ok())
        .collect();
    if mints.is_empty() {
        return Err("No mints to back up".to_string());
    }
    // BIP-39-mediated keys (see module docs): the mnemonic-derived seed,
    // not the raw wallet seed, feeds the NUT-27 derivation.
    let wallet_seed = super::internal::derive_wallet_seed().await?;
    let seed = nut27_seed(&wallet_seed)?;
    let event = build_backup_event(&seed, mints)?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("nut27_mint_backup".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(event_id)
}

/// Fetch the latest NUT-27 backup event published under `keys`.
async fn fetch_backup_event(keys: &Keys) -> Result<Option<Event>, String> {
    let filter = Filter::new()
        .author(keys.public_key())
        .kind(Kind::from(KIND_MINT_BACKUP))
        .identifier(MINT_LIST_IDENTIFIER)
        .limit(1);
    let events = nostr_client::fetch(filter, nostr_client::FetchProfile::Std).await?;
    Ok(events.into_iter().next())
}

/// Fetch and decrypt the latest NUT-27 mint backup for this wallet's seed.
/// Returns `Ok(None)` when no backup event exists.
///
/// Tries the current BIP-39-mediated keys first, then the legacy
/// direct-wallet-seed keys, so backups published before the BIP-39
/// mediation still resolve.
pub async fn restore_mint_backup() -> Result<Option<Vec<String>>, String> {
    let wallet_seed = super::internal::derive_wallet_seed().await?;
    let primary = nut27_backup_keys(&wallet_seed)?;
    let legacy = derive_backup_keys(&wallet_seed)
        .ok()
        .filter(|k| k.public_key() != primary.public_key());
    for keys in [Some(primary), legacy].into_iter().flatten() {
        // No event under these keys → try the other derivation; a decrypt
        // failure likewise means "wrong keyset", not a hard error.
        let Some(event) = fetch_backup_event(&keys).await? else {
            continue;
        };
        if let Ok(backup) = parse_backup_event(&event, &keys) {
            return Ok(Some(
                backup.mints.iter().map(|m| m.to_string()).collect(),
            ));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_mints() -> Vec<MintUrl> {
        vec![
            "https://mint.example.com".parse().unwrap(),
            "https://other.example.com".parse().unwrap(),
        ]
    }

    #[test]
    fn derivation_is_deterministic_and_seed_scoped() {
        let seed_a = [7u8; 64];
        let seed_b = [8u8; 64];
        let keys_a1 = derive_backup_keys(&seed_a).unwrap();
        let keys_a2 = derive_backup_keys(&seed_a).unwrap();
        let keys_b = derive_backup_keys(&seed_b).unwrap();
        assert_eq!(keys_a1.public_key(), keys_a2.public_key());
        assert_ne!(keys_a1.public_key(), keys_b.public_key());
    }

    #[test]
    fn backup_event_round_trips() {
        let seed = [42u8; 64];
        let event = build_backup_event(&seed, test_mints()).unwrap();
        assert_eq!(event.kind, Kind::from(30078));
        assert_eq!(
            event.tags.iter().find(|t| t.kind() == "d").and_then(|t| t.content()),
            Some(MINT_LIST_IDENTIFIER)
        );
        // Decrypt with the same seed's keys and recover the mint list.
        let keys = derive_backup_keys(&seed).unwrap();
        let backup = parse_backup_event(&event, &keys).unwrap();
        assert_eq!(
            backup.mints.iter().map(|m| m.to_string()).collect::<Vec<_>>(),
            vec![
                "https://mint.example.com".to_string(),
                "https://other.example.com".to_string(),
            ]
        );
    }

    #[test]
    fn wrong_seed_cannot_decrypt() {
        let seed = [42u8; 64];
        let event = build_backup_event(&seed, test_mints()).unwrap();
        let wrong_keys = derive_backup_keys(&[43u8; 64]).unwrap();
        assert!(parse_backup_event(&event, &wrong_keys).is_err());
    }

    #[test]
    fn nut27_derivation_is_deterministic() {
        let seed_a = [7u8; 64];
        let seed_b = [8u8; 64];
        // Same wallet seed → same words → same BIP-39 seed → same keys.
        let words_a1: Vec<String> = nut27_mnemonic(&seed_a)
            .unwrap()
            .words()
            .map(str::to_string)
            .collect();
        let words_a2: Vec<String> = nut27_mnemonic(&seed_a)
            .unwrap()
            .words()
            .map(str::to_string)
            .collect();
        assert_eq!(words_a1, words_a2);
        assert_eq!(words_a1.len(), 24);
        assert_eq!(nut27_seed(&seed_a).unwrap(), nut27_seed(&seed_a).unwrap());
        assert_eq!(
            nut27_backup_keys(&seed_a).unwrap().public_key(),
            nut27_backup_keys(&seed_a).unwrap().public_key()
        );
        // Different wallet seed → different words and keys.
        let words_b: Vec<String> = nut27_mnemonic(&seed_b)
            .unwrap()
            .words()
            .map(str::to_string)
            .collect();
        assert_ne!(words_a1, words_b);
        assert_ne!(
            nut27_backup_keys(&seed_a).unwrap().public_key(),
            nut27_backup_keys(&seed_b).unwrap().public_key()
        );
    }

    #[test]
    fn nut27_entropy_differs_from_raw_wallet_seed() {
        let seed = [42u8; 64];
        // The BIP-39 mediation must not be the identity function: entropy,
        // mnemonic seed, and the legacy direct-wallet-seed keys all differ.
        let entropy = nut27_entropy(&seed);
        assert_ne!(&entropy[..], &seed[..32]);
        assert_ne!(nut27_seed(&seed).unwrap(), seed);
        assert_ne!(
            nut27_backup_keys(&seed).unwrap().public_key(),
            derive_backup_keys(&seed).unwrap().public_key()
        );
    }

    #[test]
    fn backup_event_round_trips_under_nut27_scheme() {
        // A backup published with the current scheme decrypts with the
        // BIP-39-mediated keys (what restore tries first).
        let wallet_seed = [99u8; 64];
        let event = build_backup_event(&nut27_seed(&wallet_seed).unwrap(), test_mints()).unwrap();
        let keys = nut27_backup_keys(&wallet_seed).unwrap();
        let backup = parse_backup_event(&event, &keys).unwrap();
        assert_eq!(
            backup.mints.iter().map(|m| m.to_string()).collect::<Vec<_>>(),
            vec![
                "https://mint.example.com".to_string(),
                "https://other.example.com".to_string(),
            ]
        );
        // Legacy keys cannot read it (and vice versa) — restore must try
        // both derivations.
        let legacy_keys = derive_backup_keys(&wallet_seed).unwrap();
        assert!(parse_backup_event(&event, &legacy_keys).is_err());
    }
}
