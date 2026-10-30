//! NUT-27 mint-list backup.
//!
//! Publishes the user's mint list as a NIP-78 addressable event (kind
//! 30078, `d`-tag `mint-list`) encrypted with NIP-44 to self under
//! deterministic keys derived from the wallet seed — the same scheme as
//! cashu's `nut27` module and cashu.me's `nostrMintBackup`. This is
//! cross-wallet interop: any NUT-27 client can restore the mint list from
//! just the BIP39 seed, independently of the identity key.
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

/// Deterministically derive the NUT-27 backup keys from the wallet seed:
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
    let seed = super::internal::derive_wallet_seed().await?;
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

/// Fetch and decrypt the latest NUT-27 mint backup for this wallet's seed.
/// Returns `Ok(None)` when no backup event exists.
pub async fn restore_mint_backup() -> Result<Option<Vec<String>>, String> {
    let seed = super::internal::derive_wallet_seed().await?;
    let keys = derive_backup_keys(&seed)?;
    let filter = Filter::new()
        .author(keys.public_key())
        .kind(Kind::from(KIND_MINT_BACKUP))
        .identifier(MINT_LIST_IDENTIFIER)
        .limit(1);
    let events = nostr_client::fetch(filter, nostr_client::FetchProfile::Std).await?;
    let Some(event) = events.first() else {
        return Ok(None);
    };
    let backup = parse_backup_event(event, &keys)?;
    Ok(Some(
        backup.mints.iter().map(|m| m.to_string()).collect(),
    ))
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
}
