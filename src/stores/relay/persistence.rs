//! Disk persistence + boot-time seeding for relay lists.
//!
//! Implements an emit-backup-on-start pattern: the user's
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
//! | 1 — localStorage mirror | Per-pubkey keys `nostr.blue/relays/{pubkey}/{list}` | `RelayListMetadata` (10002+10050), search, blocked, outbox, favorites, indexer, proxy, trusted, broadcast (Nostr portion) | Survives SDK DB eviction (the 50k-cap on web) |
//! | 2 — SDK DB | `client.database().query(...)` | All of the above (supplemental; public-tag lists only) | Instant; backed by IndexedDB/NDB |
//!
//! The NIP-51 private lists (indexer/proxy/trusted/broadcast, kinds
//! 10086/10087/10089/10088) are mirrored as the **decrypted** URL lists —
//! same trust domain as the existing plaintext search/blocked mirrors, and
//! it keeps the boot seed free of any signer dependency (decrypting
//! ciphertext at boot would risk a disruptive NIP-07 prompt / NIP-46
//! timeout). They therefore have no tier-2 DB component. For broadcast,
//! only the Nostr kind-10088 portion is mirrored; the local-only broadcast
//! list keeps its own legacy key so the two union at boot.
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
    BROADCAST_RELAYS, FAVORITE_RELAYS, INDEXER_RELAYS, OUTBOX_RELAYS, PROXY_RELAYS,
    SEARCH_RELAYS, TRUSTED_RELAYS, USER_RELAY_METADATA,
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

/// Whether the localStorage metadata mirror should be overwritten with
/// `incoming`, given the currently-mirrored list (if any).
///
/// `updated_at == 0` is the defaults sentinel: the `init_user_relay_lists`
/// failure fallback stores the DEFAULT relay set in `USER_RELAY_METADATA`
/// with `updated_at: 0` (real network fetches always carry the event's
/// `created_at`). A defaults snapshot must never durably overwrite a real
/// mirrored list — the mirror wins the boot seed
/// (`mirror_metadata.or(db_metadata)`), so one offline boot would otherwise
/// replace the user's relay list with defaults across sessions.
fn should_persist_metadata(
    incoming: &RelayListMetadata,
    existing: Option<&RelayListMetadata>,
) -> bool {
    if incoming.updated_at > 0 {
        return true;
    }
    match existing {
        Some(existing) => existing.updated_at == 0,
        None => true,
    }
}

/// Persist all public relay lists to the localStorage mirror.
///
/// Reads the current signal state via `.peek()` (non-subscribing) and writes it
/// to per-pubkey keys. Synchronous — safe to call after any batch of signal
/// writes. The NIP-51 private lists (indexer/proxy/trusted) are mirrored as
/// decrypted URL lists; the broadcast mirror holds only the Nostr kind-10088
/// portion and is written at apply/publish time via [`persist_list_mirror`].
pub fn persist_public_relay_lists() {
    let Ok(pubkey) = crate::stores::nostr_client::get_cached_pubkey() else {
        return;
    };
    if let Some(metadata) = USER_RELAY_METADATA.peek().as_ref() {
        let existing = load_metadata(&pubkey);
        if should_persist_metadata(metadata, existing.as_ref()) {
            let _ = storage::set(&metadata_key(&pubkey), metadata);
        }
    }
    let search = SEARCH_RELAYS.peek().clone();
    let _ = storage::set(&list_key(&pubkey, "search"), &search);
    let blocked = BLOCKED_RELAYS.peek().clone();
    let _ = storage::set(&list_key(&pubkey, "blocked"), &blocked);
    let outbox = OUTBOX_RELAYS.peek().clone();
    let _ = storage::set(&list_key(&pubkey, "outbox"), &outbox);
    let favorites = FAVORITE_RELAYS.peek().clone();
    let _ = storage::set(&list_key(&pubkey, "favorites"), &favorites);
    let indexer = INDEXER_RELAYS.peek().clone();
    let _ = storage::set(&list_key(&pubkey, "indexer"), &indexer);
    let proxy = PROXY_RELAYS.peek().clone();
    let _ = storage::set(&list_key(&pubkey, "proxy"), &proxy);
    let trusted = TRUSTED_RELAYS.peek().clone();
    let _ = storage::set(&list_key(&pubkey, "trusted"), &trusted);
}

/// Persist a single list mirror under `nostr.blue/relays/{pubkey}/{name}`.
///
/// Used for lists whose mirrored value differs from the signal (the broadcast
/// signal is a union, but only the Nostr portion belongs in the mirror).
pub fn persist_list_mirror(pubkey: &PublicKey, name: &str, urls: &[String]) {
    let _ = storage::set(&list_key(pubkey, name), &urls.to_vec());
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

/// Select the newest kind 10002 / 10050 events from a database query result.
///
/// `database().query()` ordering is not a documented newest-first guarantee
/// across backends (IndexedDB vs NDB), so fold explicitly instead of taking
/// the last iteration: newest `created_at` wins, and same-second ties break
/// on the smaller event id — matching the SDK's `Ord for Event` (descending
/// `created_at`, then ascending id), i.e. exactly the event
/// `Events::into_iter().next()` would yield.
fn newest_relay_list_events(
    events: Vec<nostr_sdk::Event>,
) -> (Option<nostr_sdk::Event>, Option<nostr_sdk::Event>) {
    let mut best_10002: Option<nostr_sdk::Event> = None;
    let mut best_10050: Option<nostr_sdk::Event> = None;
    for event in events {
        let slot = match event.kind.as_u16() {
            10002 => &mut best_10002,
            10050 => &mut best_10050,
            _ => continue,
        };
        let replace = slot.as_ref().is_none_or(|current| event < *current);
        if replace {
            *slot = Some(event);
        }
    }
    (best_10002, best_10050)
}

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
    let (best_10002, best_10050) = newest_relay_list_events(events.to_vec());
    let relays = best_10002
        .as_ref()
        .map(parse_relay_list_event)
        .filter(|parsed| !parsed.is_empty())
        .unwrap_or_default();
    let dm_relays = best_10050
        .as_ref()
        .map(parse_dm_relay_list)
        .filter(|parsed| !parsed.is_empty())
        .unwrap_or_default();
    let updated_at = best_10002
        .as_ref()
        .map(|event| event.created_at.as_secs())
        .unwrap_or(0);
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
///
/// Folds newest-wins (SDK `Ord for Event` semantics) rather than taking the
/// first iteration — the module documents `database().query()` ordering as
/// not a guarantee we rely on anywhere else, so we don't here either (the
/// current `Events` collection iterates newest-first by construction, but
/// this keeps the file self-consistent if that ever changes).
async fn collect_list_from_db(client: &Client, pubkey: &PublicKey, kind: Kind) -> Vec<String> {
    let filter = Filter::new().author(*pubkey).kind(kind).limit(10);
    let Ok(events) = client.database().query(filter).await else {
        return Vec::new();
    };
    let newest = events
        .into_iter()
        .fold(None::<nostr_sdk::Event>, |best, event| match best {
            // SDK Ord: `a < b` ⇒ a is newer. Keep the current best when it
            // is newer than the incoming event; otherwise take the event.
            Some(current) if current < event => Some(current),
            _ => Some(event),
        });
    newest
        .as_ref()
        .map(extract_relay_tag_urls)
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
    /// NIP-51 private lists (decrypted mirrors; no tier-2 DB component —
    /// their `.content` is ciphertext that would need a signer at boot).
    pub indexer: Vec<String>,
    pub proxy: Vec<String>,
    pub trusted: Vec<String>,
    /// Nostr kind-10088 portion of the broadcast set (unions with the local
    /// list at seed time).
    pub broadcast: Vec<String>,
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
    let mirror_indexer = load_list(&pubkey, "indexer");
    let mirror_proxy = load_list(&pubkey, "proxy");
    let mirror_trusted = load_list(&pubkey, "trusted");
    let mirror_broadcast = load_list(&pubkey, "broadcast");

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
        indexer: mirror_indexer,
        proxy: mirror_proxy,
        trusted: mirror_trusted,
        broadcast: mirror_broadcast,
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
    if !seeded.indexer.is_empty() {
        *INDEXER_RELAYS.write() = seeded.indexer.clone();
    }
    if !seeded.proxy.is_empty() {
        *PROXY_RELAYS.write() = seeded.proxy.clone();
    }
    if !seeded.trusted.is_empty() {
        *TRUSTED_RELAYS.write() = seeded.trusted.clone();
    }
    if !seeded.broadcast.is_empty() {
        // Effective broadcast set = local (browser) list ∪ Nostr kind-10088
        // list. The local portion was already loaded into the signal by
        // `init_local_relays_from_cache`; union in the mirrored Nostr part.
        let union = super::nip51_lists::merge_urls(
            BROADCAST_RELAYS.peek().clone(),
            seeded.broadcast.clone(),
        );
        *BROADCAST_RELAYS.write() = union;
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

    fn signed_relay_list(keys: &nostr_sdk::Keys, created_at: u64, url: &str) -> nostr_sdk::Event {
        let url: nostr_sdk::RelayUrl = nostr_sdk::RelayUrl::parse(url).unwrap();
        nostr_sdk::EventBuilder::relay_list(vec![(url, None)])
            .custom_created_at(nostr_sdk::Timestamp::from(created_at))
            .sign_with_keys(keys)
            .unwrap()
    }

    fn signed_dm_relay_list(keys: &nostr_sdk::Keys, created_at: u64, url: &str) -> nostr_sdk::Event {
        let tag = nostr_sdk::Tag::custom(
            nostr_sdk::TagKind::Custom("relay".into()),
            vec![url.to_string()],
        );
        nostr_sdk::EventBuilder::new(nostr_sdk::Kind::from(10050), "")
            .tag(tag)
            .custom_created_at(nostr_sdk::Timestamp::from(created_at))
            .sign_with_keys(keys)
            .unwrap()
    }

    #[test]
    fn test_newest_relay_list_events_prefers_newest_not_last() {
        let keys = nostr_sdk::Keys::generate();
        let older = signed_relay_list(&keys, 100, "wss://relay.old");
        let newer = signed_relay_list(&keys, 200, "wss://relay.new");
        let dm = signed_dm_relay_list(&keys, 300, "wss://dm.relay");
        // Iterate with the OLDER 10002 last — the fold must be newest-wins,
        // not last-iteration-wins.
        let (best_10002, best_10050) =
            newest_relay_list_events(vec![dm.clone(), newer.clone(), older]);
        assert_eq!(best_10002.unwrap().id, newer.id);
        assert_eq!(best_10050.unwrap().id, dm.id);
    }

    #[test]
    fn test_newest_relay_list_events_tie_breaks_on_smaller_id() {
        let keys = nostr_sdk::Keys::generate();
        let a = signed_relay_list(&keys, 100, "wss://a.relay");
        let b = signed_relay_list(&keys, 100, "wss://b.relay");
        let (smaller, larger) = if a.id < b.id { (a, b) } else { (b, a) };
        // Same created_at: the later-arriving larger-id event must NOT
        // displace the smaller-id winner (SDK `Ord for Event` semantics).
        let (best_10002, _) = newest_relay_list_events(vec![smaller.clone(), larger]);
        assert_eq!(best_10002.unwrap().id, smaller.id);
    }

    fn metadata_with(updated_at: u64) -> RelayListMetadata {
        RelayListMetadata {
            relays: vec![],
            dm_relays: vec![],
            updated_at,
        }
    }

    #[test]
    fn test_defaults_never_overwrite_real_mirror() {
        let real = metadata_with(1700000000);
        let defaults = metadata_with(0);
        // Real data always persists.
        assert!(should_persist_metadata(&real, None));
        assert!(should_persist_metadata(&real, Some(&defaults)));
        // Defaults may seed an empty/stale mirror but never clobber a real one.
        assert!(should_persist_metadata(&defaults, None));
        assert!(should_persist_metadata(&defaults, Some(&defaults)));
        assert!(!should_persist_metadata(&defaults, Some(&real)));
    }
}
