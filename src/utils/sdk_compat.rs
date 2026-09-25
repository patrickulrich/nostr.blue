//! nostr-sdk 0.45 compatibility helpers.
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

/// Fetch the latest kind 0 metadata for `pubkey` from READ relays.
///
/// Replacement for the removed `Client::fetch_metadata(pubkey, timeout)`.
pub async fn fetch_metadata(
    client: &Client,
    pubkey: PublicKey,
    timeout: Duration,
) -> Result<Option<Metadata>, nostr_sdk::error::Error> {
    let filter = Filter::new().author(pubkey).kind(Kind::Metadata).limit(1);
    let events = client.fetch_events(filter).timeout(timeout).await?;
    Ok(latest_metadata(&events))
}

/// Read the latest kind 0 metadata for `pubkey` from the local database.
///
/// Replacement for the removed `NostrDatabaseExt::metadata`.
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

    let mut out: Vec<(PublicKey, Metadata)> = metadata_events
        .iter()
        .map(|e| {
            let metadata = Metadata::from_json(&e.content).unwrap_or_default();
            (e.pubkey, metadata)
        })
        .collect();

    // Extend with missing public keys (default metadata).
    for pk in pubkeys {
        if !out.iter().any(|(p, _)| *p == pk) {
            out.push((pk, Metadata::default()));
        }
    }
    Ok(out)
}
