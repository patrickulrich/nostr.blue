//! Starter Packs Store (Kind 39089)
//!
//! Reusable store for Nostr starter packs — curated lists of profiles for discovery.
//! Designed for use by /packs and other features (e.g., /news).
#![allow(dead_code)]

use dioxus::prelude::*;
use lru::LruCache;
use nostr_sdk::prelude::*;
use std::num::NonZeroUsize;
use std::time::Duration;

type StdResult<T, E> = std::result::Result<T, E>;

/// Starter pack kind (addressable event)
pub const STARTER_PACK_KIND: u16 = 39089;

/// Cache size for packs
const PACK_CACHE_SIZE: usize = 100;

/// A parsed starter pack
#[derive(Clone, Debug, PartialEq)]
pub struct StarterPack {
    pub event: nostr::Event,
    pub event_id: String,
    pub name: String,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub author_pubkey: String,
    pub members: Vec<PackMember>,
    pub d_tag: String,
    pub naddr: String,
    pub created_at: u64,
}

/// A member of a starter pack
#[derive(Clone, Debug, PartialEq)]
pub struct PackMember {
    pub pubkey: String,
    pub relay_hint: Option<String>,
}

/// Pack cache keyed by naddr
pub static PACKS_CACHE: GlobalSignal<LruCache<String, StarterPack>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(PACK_CACHE_SIZE).unwrap()));

/// Loading state
pub static LOADING_PACKS: GlobalSignal<bool> = GlobalSignal::new(|| false);

// ── Parsing ──────────────────────────────────────────────────────────

impl StarterPack {
    /// Parse a kind 39089 event into a StarterPack
    pub fn from_event(event: &nostr::Event) -> Option<Self> {
        if event.kind.as_u16() != STARTER_PACK_KIND {
            return None;
        }

        let d_tag = event.tags.identifier()?.to_string();

        let name = event
            .tags
            .iter()
            .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("title"))
            .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
            .unwrap_or_else(|| "Untitled Pack".to_string());

        let description = event
            .tags
            .iter()
            .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("description"))
            .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
            .filter(|s| !s.is_empty());

        let image_url = event
            .tags
            .iter()
            .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("image"))
            .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
            .filter(|s| !s.is_empty());

        let members: Vec<PackMember> = event
            .tags
            .iter()
            .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p"))
            .filter_map(|t| {
                let slice = t.as_slice();
                let pk_str = slice.get(1)?;
                // Validate as proper hex public key
                PublicKey::from_hex(pk_str).ok()?;
                let pubkey = pk_str.to_string();
                let relay_hint = slice
                    .get(2)
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty());
                Some(PackMember { pubkey, relay_hint })
            })
            .collect();

        let naddr = Coordinate::new(Kind::Custom(STARTER_PACK_KIND), event.pubkey)
            .identifier(&d_tag)
            .to_bech32()
            .ok()?;

        Some(StarterPack {
            event: event.clone(),
            event_id: event.id.to_hex(),
            name,
            description,
            image_url,
            author_pubkey: event.pubkey.to_hex(),
            members,
            d_tag,
            naddr,
            created_at: event.created_at.as_secs(),
        })
    }
}

// ── Cache ────────────────────────────────────────────────────────────

/// Get a pack from cache by naddr
pub fn get_cached_pack(naddr: &str) -> Option<StarterPack> {
    PACKS_CACHE.read().peek(naddr).cloned()
}

/// Cache a pack (only if newer than existing)
pub fn cache_pack(pack: &StarterPack) {
    let mut cache = PACKS_CACHE.write();
    let should_insert = cache
        .peek(&pack.naddr)
        .is_none_or(|existing| pack.created_at > existing.created_at);
    if should_insert {
        cache.put(pack.naddr.clone(), pack.clone());
    }
}

/// Cache multiple packs (only if newer than existing)
fn cache_packs(packs: &[StarterPack]) {
    let mut cache = PACKS_CACHE.write();
    for pack in packs {
        let should_insert = cache
            .peek(&pack.naddr)
            .is_none_or(|existing| pack.created_at > existing.created_at);
        if should_insert {
            cache.put(pack.naddr.clone(), pack.clone());
        }
    }
}

// ── Fetching ─────────────────────────────────────────────────────────

/// Fetch recent starter packs (global discovery)
pub async fn fetch_starter_packs(limit: usize) -> StdResult<Vec<StarterPack>, String> {
    *LOADING_PACKS.write() = true;
    let filter = Filter::new()
        .kind(Kind::Custom(STARTER_PACK_KIND))
        .limit(limit);

    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await;
    *LOADING_PACKS.write() = false;

    match result {
        Ok(events) => {
            let mut packs: Vec<StarterPack> =
                events.iter().filter_map(StarterPack::from_event).collect();
            packs.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            packs.truncate(limit);
            cache_packs(&packs);
            log::info!("Fetched {} starter packs", packs.len());
            Ok(packs)
        }
        Err(e) => {
            log::error!("Failed to fetch starter packs: {}", e);
            Err(e)
        }
    }
}

/// Fetch packs by a specific author
pub async fn fetch_packs_by_author(
    pubkey_hex: &str,
    limit: usize,
) -> StdResult<Vec<StarterPack>, String> {
    let pubkey = PublicKey::from_hex(pubkey_hex).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(STARTER_PACK_KIND))
        .author(pubkey)
        .limit(limit);

    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;

    let mut packs: Vec<StarterPack> = events.iter().filter_map(StarterPack::from_event).collect();
    packs.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    packs.truncate(limit);
    cache_packs(&packs);
    Ok(packs)
}

/// Fetch packs that contain a specific pubkey as a member
pub async fn fetch_packs_containing_pubkey(
    pubkey_hex: &str,
    limit: usize,
) -> StdResult<Vec<StarterPack>, String> {
    let filter = Filter::new()
        .kind(Kind::Custom(STARTER_PACK_KIND))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::P), pubkey_hex)
        .limit(limit);

    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;

    let mut packs: Vec<StarterPack> = events.iter().filter_map(StarterPack::from_event).collect();
    packs.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    packs.truncate(limit);
    cache_packs(&packs);
    Ok(packs)
}

/// Fetch packs created by users the current user follows
pub async fn fetch_packs_from_following(
    contact_pubkeys: &[String],
    limit: usize,
) -> StdResult<Vec<StarterPack>, String> {
    if contact_pubkeys.is_empty() {
        return Ok(Vec::new());
    }

    let authors: Vec<PublicKey> = contact_pubkeys
        .iter()
        .filter_map(|pk| PublicKey::from_hex(pk).ok())
        .collect();

    if authors.is_empty() {
        return Ok(Vec::new());
    }

    let filter = Filter::new()
        .kind(Kind::Custom(STARTER_PACK_KIND))
        .authors(authors)
        .limit(limit);

    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15))
            .await?;

    let mut packs: Vec<StarterPack> = events.iter().filter_map(StarterPack::from_event).collect();
    packs.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    packs.truncate(limit);
    cache_packs(&packs);
    Ok(packs)
}

/// Fetch a single pack by naddr (cache-first)
pub async fn fetch_pack_by_naddr(naddr: &str) -> StdResult<Option<StarterPack>, String> {
    if let Some(cached) = get_cached_pack(naddr) {
        return Ok(Some(cached));
    }

    let coord = Coordinate::from_bech32(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::Custom(STARTER_PACK_KIND))
        .author(coord.public_key)
        .identifier(&coord.identifier);

    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;

    if let Some(event) = events.first() {
        if let Some(pack) = StarterPack::from_event(event) {
            cache_pack(&pack);
            return Ok(Some(pack));
        }
    }

    Ok(None)
}

// ── Publishing ───────────────────────────────────────────────────────

/// Generate a 12-character random alphanumeric d-tag
pub fn generate_d_tag() -> String {
    use nostr_sdk::prelude::rand::{self, Rng};
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..12)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Publish a starter pack (create or update)
///
/// For create: pass `None` for `d_tag` to auto-generate.
/// For update: pass the existing d-tag.
/// Returns the naddr of the published pack.
pub async fn publish_starter_pack(
    name: &str,
    description: Option<&str>,
    image_url: Option<&str>,
    members: &[PackMember],
    d_tag: Option<&str>,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let d = d_tag.map(|s| s.to_string()).unwrap_or_else(generate_d_tag);

    let mut tags: Vec<Tag> = vec![Tag::identifier(&d), Tag::title(name), Tag::alt("Starter pack")];

    if let Some(desc) = description {
        if !desc.is_empty() {
            tags.push(Tag::description(desc));
        }
    }

    if let Some(img) = image_url {
        if !img.is_empty() {
            if let Ok(url) = Url::parse(img) {
                tags.push(Tag::image(url, None));
            }
        }
    }

    for member in members {
        let pk = PublicKey::from_hex(&member.pubkey)
            .map_err(|e| format!("Invalid member pubkey {}: {}", member.pubkey, e))?;
        if let Some(ref hint) = member.relay_hint {
            let relay_url = nostr::RelayUrl::parse(hint).ok();
            tags.push(Tag::from_standardized_without_cell(
                TagStandard::PublicKey {
                    public_key: pk,
                    relay_url,
                    alias: None,
                    uppercase: false,
                },
            ));
        } else {
            tags.push(Tag::public_key(pk));
        }
    }

    let builder = EventBuilder::new(Kind::Custom(STARTER_PACK_KIND), "")
        .tags(tags)
        .allow_self_tagging();
    let builder = crate::utils::nips::nip89::tag_event_builder(builder);

    let event = client
        .sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign starter pack: {}", e))?;

    client
        .pool()
        .send_event(&event)
        .await
        .map_err(|e| format!("Failed to publish starter pack: {}", e))?;

    if let Some(pack) = StarterPack::from_event(&event) {
        cache_pack(&pack);
        log::info!(
            "Starter pack published: {} ({} members)",
            pack.naddr,
            pack.members.len(),
        );
        return Ok(pack.naddr);
    }

    let pubkey = crate::stores::nostr_client::get_cached_pubkey()?;
    let naddr =
        crate::stores::nostr_client::make_naddr_with_hints(STARTER_PACK_KIND, &pubkey, &d).await?;
    Ok(naddr)
}

/// Delete a starter pack (NIP-09)
pub async fn delete_starter_pack(pack: &StarterPack) -> StdResult<(), String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;

    if !*crate::stores::nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if pack.author_pubkey != current_pubkey {
        return Err("You can only delete your own packs".to_string());
    }

    let author_pk = PublicKey::from_hex(&pack.author_pubkey)
        .map_err(|e| format!("Invalid author pubkey: {}", e))?;

    let coord = Coordinate::new(Kind::Custom(STARTER_PACK_KIND), author_pk).identifier(&pack.d_tag);

    let deletion_request = EventDeletionRequest::new().coordinate(coord);
    let builder = EventBuilder::delete(deletion_request);

    client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to delete starter pack: {}", e))?;

    PACKS_CACHE.write().pop(&pack.naddr);
    log::info!("Starter pack deleted: {}", pack.naddr);
    Ok(())
}

/// Fetch recent posts from pack members (reusable for /news and /packs detail).
/// Returns raw events — caller converts to FeedItems via `process_events_to_feed_items`.
pub async fn fetch_pack_member_posts(
    pack: &StarterPack,
    limit: usize,
    until: Option<u64>,
) -> StdResult<Vec<nostr::Event>, String> {
    let authors: Vec<PublicKey> = pack
        .members
        .iter()
        .filter_map(|m| PublicKey::from_hex(&m.pubkey).ok())
        .collect();

    if authors.is_empty() {
        return Ok(Vec::new());
    }

    let mut filter = Filter::new()
        .kinds([Kind::TextNote, Kind::Repost, Kind::GenericRepost])
        .authors(authors)
        .limit(limit);

    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts.saturating_sub(1)));
    }

    let mut events = crate::stores::nostr_client::fetch_events_from_connected_relays(
        filter,
        Duration::from_secs(10),
    )
    .await?;

    events.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(events)
}

/// Follow all members of a pack that the user isn't already following.
/// Returns the count of newly followed users.
pub async fn follow_all_pack_members(pack: &StarterPack) -> StdResult<usize, String> {
    let member_pubkeys: Vec<String> = pack.members.iter().map(|m| m.pubkey.clone()).collect();
    if member_pubkeys.is_empty() {
        return Ok(0);
    }
    crate::stores::nostr_client::follow_users_batch(member_pubkeys).await
}
