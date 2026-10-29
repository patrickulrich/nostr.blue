//! nostr-sdk 0.45 compatibility helpers — the single module where removed-SDK
//! APIs are re-implemented (folded here from `utils/sdk_compat.rs` so the next
//! SDK migration touches exactly one module inside the client wrapper).
//!
//! The 0.45 SDK removed the high-level `Client::fetch_metadata` /
//! `NostrDatabaseExt::{metadata, contacts, contacts_public_keys}` helpers.
//! These functions reimplement them on top of the 0.45 builder APIs with
//! identical semantics (see nostr-database 0.44 `ext.rs`).

use std::time::Duration;

use nostr::key::PublicKey;
use nostr::nips::nip01::Metadata;
use nostr_sdk::client::Client;
use nostr_sdk::prelude::{Filter, Kind};

/// Fetch the latest kind 0 metadata for `pubkey` **from indexer relays**.
///
/// Replacement for the removed `Client::fetch_metadata(pubkey, timeout)`.
///
/// **Deprecated in favor of the batched profile pipeline** — UI-driven
/// profile loads must use `stores::profiles::fetch_profile` /
/// `queue_profile_request` (PROFILE_CACHE with LRU + TTL). This direct fetch
/// remains only for callers that need a single explicit lookup outside the
/// cache; it routes through the sanctioned indexer helper
/// (`relay::nip65::fetch_events_from_indexers`), per AGENTS.md's relay
/// taxonomy.
#[allow(dead_code)]
pub async fn fetch_metadata(
    client: &Client,
    pubkey: PublicKey,
    timeout: Duration,
) -> Result<Option<Metadata>, String> {
    let filter = Filter::new().author(pubkey).kind(Kind::Metadata).limit(1);
    let events =
        crate::stores::relay::nip65::fetch_events_from_indexers(client, filter, timeout).await?;
    Ok(latest_metadata(&events.into_iter().collect::<std::collections::BTreeSet<_>>()))
}

/// Read the latest kind 0 metadata for `pubkey` from the local database.
///
/// Replacement for the removed `NostrDatabaseExt::metadata`. Prefer the
/// `PROFILE_CACHE` pipeline (`stores::profiles::fetch_profile`, which is
/// already DB-first) — kept as the raw DB shim for that module.
#[allow(dead_code)]
pub async fn db_metadata(
    client: &Client,
    pubkey: PublicKey,
) -> Result<Option<Metadata>, nostr_database::error::Error> {
    let filter = Filter::new().author(pubkey).kind(Kind::Metadata).limit(1);
    let events = client.database().query(filter).await?;
    Ok(latest_metadata(&events))
}

fn latest_metadata(events: &std::collections::BTreeSet<nostr::event::Event>) -> Option<Metadata> {
    events
        .iter()
        .max_by_key(|e| e.created_at)
        .and_then(|e| Metadata::from_json(&e.content).ok())
}

/// Contact-list pubkeys for `pubkey` from the local database (kind 3 p-tags).
///
/// Replacement for the removed `NostrDatabaseExt::contacts_public_keys`.
pub async fn db_contacts_public_keys(
    client: &Client,
    pubkey: PublicKey,
) -> Result<std::collections::HashSet<PublicKey>, nostr_database::error::Error> {
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::ContactList)
        .limit(1);
    let events = client.database().query(filter).await?;
    Ok(events
        .iter()
        .max_by_key(|e| e.created_at)
        .map(|e| e.tags.public_keys().collect())
        .unwrap_or_default())
}

/// Contact pubkeys joined with their kind 0 metadata from the local database.
///
/// Contacts without a stored kind 0 get `Metadata::default()`, matching the
/// removed `NostrDatabaseExt::contacts`.
pub async fn db_contact_profiles(
    client: &Client,
    pubkey: PublicKey,
) -> Result<Vec<(PublicKey, Metadata)>, nostr_database::error::Error> {
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::ContactList)
        .limit(1);
    let events = client.database().query(filter).await?;
    let Some(latest) = events.iter().max_by_key(|e| e.created_at) else {
        return Ok(Vec::new());
    };
    let pubkeys: Vec<PublicKey> = latest.tags.public_keys().collect();
    if pubkeys.is_empty() {
        return Ok(Vec::new());
    }

    let metadata_filter = Filter::new().kind(Kind::Metadata).authors(pubkeys.clone());
    let metadata_events = client.database().query(metadata_filter).await?;

    // Newest-wins per pubkey: the DB may hold multiple kind-0 versions for
    // one author (the desktop ndb backend returns them all). Explicit
    // selection emits exactly one entry per contact, keeps stale versions
    // from double-counting in logs, and makes the missing-pubkey extension
    // an O(n) map lookup instead of an O(n²) scan.
    let mut newest: std::collections::HashMap<PublicKey, &nostr::event::Event> =
        std::collections::HashMap::with_capacity(pubkeys.len());
    for event in metadata_events.iter() {
        match newest.entry(event.pubkey) {
            std::collections::hash_map::Entry::Occupied(mut slot) => {
                if event.created_at > slot.get().created_at {
                    slot.insert(event);
                }
            }
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(event);
            }
        }
    }
    let mut out: Vec<(PublicKey, Metadata)> = Vec::with_capacity(pubkeys.len());
    for pk in pubkeys {
        match newest.get(&pk) {
            Some(event) => {
                let metadata = Metadata::from_json(&event.content).unwrap_or_default();
                out.push((pk, metadata));
            }
            None => out.push((pk, Metadata::default())),
        }
    }
    Ok(out)
}
