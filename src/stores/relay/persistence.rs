//! Disk persistence + boot-time seeding for relay lists.
//!
//! Implements an Amethyst-style `onStart { emit(backup) }` pattern: the user's
//! relay lists are persisted to a localStorage mirror and the SDK local DB on
//! every network refresh, then **seeded synchronously at boot** before the
//! first `connect()`. This eliminates the `USER_RELAYS_APPLIED` readiness race
//! (the architectural root cause of issue #351): the gate flips immediately
//! because the pool already contains the user's relays.
//!
//! # Two-tier seed
//!
//! | Tier | Source | Lists | Durability |
//! |------|--------|-------|------------|
//! | 1 — localStorage mirror | Per-pubkey keys `nostr.blue/relays/{pubkey}/{list}` | `RelayListMetadata` (10002+10050), search, blocked, outbox, favorites | Survives SDK DB eviction (the 50k-cap on web) |
//! | 2 — SDK DB | `client.database().query(...)` | All of the above (supplemental) | Instant; backed by IndexedDB/NDB |
//!
//! Gift-wrapped lists (indexer/proxy/trusted, kinds 10086/10087/10089) are
//! **not** seeded here — unwrapping requires a signer round-trip (NIP-07 prompt
//! / NIP-46 timeout risk) that is disruptive at boot. Indexers get the default
//! list immediately via `add_indexer_relays_to_client` (which falls back to
//! `DEFAULT_INDEXER_RELAYS`), and custom lists load later via
//! `init_private_relay_lists` + pool reconciliation.
//!
//! # Async-safety (Dioxus `WritableRef`)
//!
//! The seeder is split into a **collect** phase (all `await`s — DB queries) and
//! a **write** phase (synchronous signal writes, no `await`s between them).
//! Holding a `GlobalSignal::write()` guard across an `.await` panics with
//! `BorrowMutError` if the render loop or a woken task touches the same signal.

use std::sync::Arc;

use dioxus::prelude::ReadableExt;
use nostr_sdk::{Client, Filter, Kind, PublicKey, RelayUrl, TagKind};

use super::nip65::{
    parse_dm_relay_list, parse_relay_list_event, RelayListMetadata, BLOCKED_RELAYS,
    FAVORITE_RELAYS, OUTBOX_RELAYS, SEARCH_RELAYS, USER_RELAY_METADATA,
};
use crate::platform::storage;

// ---------------------------------------------------------------------------
// Key helpers (per-pubkey namespacing)
// ---------------------------------------------------------------------------

fn metadata_key(pk: &PublicKey) -> String {
    format!("nostr.blue/relays/{}/metadata", pk.to_hex())
}

fn list_key(pk: &PublicKey, list: &str) -> String {
    format!("nostr.blue/relays/{}/{}", pk.to_hex(), list)
}

// ---------------------------------------------------------------------------
// Mirror persistence (call after every network refresh / settings change)
// ---------------------------------------------------------------------------

/// Persist all public relay lists to the localStorage mirror.
///
/// Reads the current signal state via `.peek()` (non-subscribing) and writes it
/// to per-pubkey keys. Synchronous — safe to call after any batch of signal
/// writes. Gift-wrapped lists are intentionally excluded (privacy: kept
/// encrypted at rest in the SDK DB only).
pub fn persist_public_relay_lists() {
    let Ok(pubkey) = crate::stores::nostr_client::get_cached_pubkey() else {
        return;
    };
    if let Some(metadata) = USER_RELAY_METADATA.peek().as_ref() {
        let _ = storage::set(&metadata_key(&pubkey), metadata);
    }
    let search = SEARCH_RELAYS.peek().clone();
    let _ = storage::set(&list_key(&pubkey, "search"), &search);
    let blocked = BLOCKED_RELAYS.peek().clone();
    let _ = storage::set(&list_key(&pubkey, "blocked"), &blocked);
    let outbox = OUTBOX_RELAYS.peek().clone();
    let _ = storage::set(&list_key(&pubkey, "outbox"), &outbox);
    let favorites = FAVORITE_RELAYS.peek().clone();
    let _ = storage::set(&list_key(&pubkey, "favorites"), &favorites);
}

// ---------------------------------------------------------------------------
// Mirror load (tier 1)
// ---------------------------------------------------------------------------

fn load_metadata(pk: &PublicKey) -> Option<RelayListMetadata> {
    storage::get::<RelayListMetadata>(&metadata_key(pk)).ok()
}

fn load_list(pk: &PublicKey, name: &str) -> Vec<String> {
    storage::get::<Vec<String>>(&list_key(pk, name))
        .ok()
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// SDK DB load (tier 2 — supplemental)
// ---------------------------------------------------------------------------

/// Query the user's kind 10002 + 10050 from the SDK local DB and assemble a
/// `RelayListMetadata`. Reuses nostr.blue's existing parsers
/// (`parse_relay_list_event` / `parse_dm_relay_list`) so ws→wss upgrade and
/// marker semantics stay consistent.
async fn collect_metadata_from_db(client: &Client, pubkey: &PublicKey) -> Option<RelayListMetadata> {
    let filter = Filter::new()
        .author(*pubkey)
        .kinds(vec![Kind::RelayList, Kind::from(10050)])
        .limit(10);
    let events = client.database().query(filter).await.ok()?;
    let mut relays = Vec::new();
    let mut dm_relays = Vec::new();
    let mut updated_at = 0u64;
    for event in events.into_iter() {
        match event.kind.as_u16() {
            10002 => {
                let parsed = parse_relay_list_event(&event);
                if !parsed.is_empty() {
                    relays = parsed;
                    updated_at = event.created_at.as_secs();
                }
            }
            10050 => {
                let parsed = parse_dm_relay_list(&event);
                if !parsed.is_empty() {
                    dm_relays = parsed;
                }
            }
            _ => {}
        }
    }
    if relays.is_empty() && dm_relays.is_empty() {
        None
    } else {
        Some(RelayListMetadata {
            relays,
            dm_relays,
            updated_at,
        })
    }
}

/// Query a single NIP-51 relay-list kind from the SDK DB.
/// All four kinds (10006/10007/10012/10013) use `["relay", "url"]` tags.
async fn collect_list_from_db(client: &Client, pubkey: &PublicKey, kind: Kind) -> Vec<String> {
    let filter = Filter::new().author(*pubkey).kind(kind).limit(1);
    let Ok(events) = client.database().query(filter).await else {
        return Vec::new();
    };
    events
        .into_iter()
        .next()
        .map(|event| extract_relay_tag_urls(&event))
        .unwrap_or_default()
}

fn extract_relay_tag_urls(event: &nostr_sdk::Event) -> Vec<String> {
    event
        .tags
        .iter()
        .filter_map(|tag| {
            if tag.kind() == TagKind::Custom("relay".into()) {
                tag.content().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Seeder
// ---------------------------------------------------------------------------

/// Relay-list data collected from disk (mirror + SDK DB), ready to be written
/// to signals synchronously.
#[derive(Default)]
pub struct SeededRelays {
    pub metadata: Option<RelayListMetadata>,
    pub search: Vec<String>,
    pub blocked: Vec<String>,
    pub outbox: Vec<String>,
    pub favorites: Vec<String>,
}

/// Collect relay lists from the localStorage mirror (tier 1) and SDK local DB
/// (tier 2). Prefers the mirror (durable against the 50k-cap DB eviction);
/// falls back to the DB for any empty list.
///
/// **All `await`s live here.** Callers must not hold a `GlobalSignal::write()`
/// guard across this call.
pub async fn collect_relay_lists_from_disk(client: &Client, pubkey: PublicKey) -> SeededRelays {
    // Tier 1: localStorage mirror (sync I/O but logically part of the collect phase)
    let mirror_metadata = load_metadata(&pubkey);
    let mirror_search = load_list(&pubkey, "search");
    let mirror_blocked = load_list(&pubkey, "blocked");
    let mirror_outbox = load_list(&pubkey, "outbox");
    let mirror_favorites = load_list(&pubkey, "favorites");

    // Tier 2: SDK DB (supplemental — only queried to fill gaps the mirror misses)
    let db_metadata = collect_metadata_from_db(client, &pubkey).await;
    let db_search = collect_list_from_db(client, &pubkey, Kind::SearchRelays).await;
    let db_blocked = collect_list_from_db(client, &pubkey, Kind::BlockedRelays).await;
    let db_outbox = collect_list_from_db(client, &pubkey, Kind::Custom(10013)).await;
    let db_favorites = collect_list_from_db(client, &pubkey, Kind::Custom(10012)).await;

    // Prefer mirror (durable), fall back to DB
    SeededRelays {
        metadata: mirror_metadata.or(db_metadata),
        search: if !mirror_search.is_empty() {
            mirror_search
        } else {
            db_search
        },
        blocked: if !mirror_blocked.is_empty() {
            mirror_blocked
        } else {
            db_blocked
        },
        outbox: if !mirror_outbox.is_empty() {
            mirror_outbox
        } else {
            db_outbox
        },
        favorites: if !mirror_favorites.is_empty() {
            mirror_favorites
        } else {
            db_favorites
        },
    }
}

/// Add seeded user relays (kind 10002 + 10050) to the pool so they connect in
/// the first `connect()` wave. Skips relays that are in the blocked list.
pub async fn apply_seeded_relays_to_pool(client: Arc<Client>, seeded: &SeededRelays) {
    let Some(metadata) = &seeded.metadata else {
        return;
    };
    let blocked: Vec<String> = seeded
        .blocked
        .iter()
        .map(|b| b.trim_end_matches('/').to_string())
        .collect();
    for rc in &metadata.relays {
        let normalized = rc.url.trim_end_matches('/');
        if blocked.iter().any(|b| b == normalized) {
            log::info!("Skipping blocked seeded relay: {}", rc.url);
            continue;
        }
        if let Ok(url) = RelayUrl::parse(&rc.url) {
            match client.add_relay(url).await {
                Ok(added) => log::debug!("Seeded relay {} (new={})", rc.url, added),
                Err(e) => log::debug!("Seeded relay {} skipped: {}", rc.url, e),
            }
        }
    }
    for dm_relay in &metadata.dm_relays {
        let normalized = dm_relay.trim_end_matches('/');
        if blocked.iter().any(|b| b == normalized) {
            log::info!("Skipping blocked seeded DM relay: {}", dm_relay);
            continue;
        }
        if let Ok(url) = RelayUrl::parse(dm_relay) {
            match client.add_relay(url).await {
                Ok(added) => log::debug!("Seeded DM relay {} (new={})", dm_relay, added),
                Err(e) => log::debug!("Seeded DM relay {} skipped: {}", dm_relay, e),
            }
        }
    }
}

/// Write seeded data to the global signals synchronously.
///
/// **No `await`s here** — all signal writes happen in a burst to avoid holding
/// a `WritableRef` across a yield point (Dioxus `BorrowMutError` panic risk).
///
/// Returns `true` if the user's relay metadata was seeded (caller flips
/// `USER_RELAYS_APPLIED` early in that case).
pub fn write_seeded_relay_lists_to_signals(seeded: &SeededRelays) -> bool {
    let mut metadata_seeded = false;
    if let Some(m) = &seeded.metadata {
        *USER_RELAY_METADATA.write() = Some(m.clone());
        metadata_seeded = true;
    }
    if !seeded.search.is_empty() {
        *SEARCH_RELAYS.write() = seeded.search.clone();
    }
    if !seeded.blocked.is_empty() {
        *BLOCKED_RELAYS.write() = seeded.blocked.clone();
    }
    if !seeded.outbox.is_empty() {
        *OUTBOX_RELAYS.write() = seeded.outbox.clone();
    }
    if !seeded.favorites.is_empty() {
        *FAVORITE_RELAYS.write() = seeded.favorites.clone();
    }
    metadata_seeded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_relay_tag_urls() {
        // Build a kind 10007 event with a relay tag using EventBuilder.
        let keys = nostr_sdk::Keys::generate();
        let url: nostr_sdk::RelayUrl = nostr_sdk::RelayUrl::parse("wss://relay.damus.io").unwrap();
        let event = nostr_sdk::EventBuilder::search_relays(vec![url])
            .sign_with_keys(&keys)
            .unwrap();
        let urls = extract_relay_tag_urls(&event);
        assert_eq!(urls, vec!["wss://relay.damus.io"]);
    }

    #[test]
    fn test_extract_relay_tag_urls_empty() {
        let keys = nostr_sdk::Keys::generate();
        let event = nostr_sdk::EventBuilder::text_note("")
            .sign_with_keys(&keys)
            .unwrap();
        let urls = extract_relay_tag_urls(&event);
        assert!(urls.is_empty());
    }

    #[test]
    fn test_key_namespacing() {
        let keys = nostr_sdk::Keys::generate();
        let pk = keys.public_key();
        let mk = metadata_key(&pk);
        let lk = list_key(&pk, "search");
        assert!(mk.starts_with("nostr.blue/relays/"));
        assert!(mk.ends_with("/metadata"));
        assert!(lk.ends_with("/search"));
        assert_ne!(mk, lk);
    }
}
