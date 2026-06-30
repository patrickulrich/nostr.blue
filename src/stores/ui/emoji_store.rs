use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_stores::Store;
use nostr::nips::nip51::Emojis;
use nostr::FromBech32;
use nostr_sdk::{EventBuilder, Filter, Kind, PublicKey, Timestamp, Url};
use std::collections::HashSet;
use std::time::Duration;
/// Custom emoji from Nostr (NIP-30 format)
#[derive(Clone, Debug, PartialEq)]
pub struct CustomEmoji {
    pub shortcode: String,
    pub image_url: String,
}
/// Emoji set (kind 30030) from Nostr
#[derive(Clone, Debug, PartialEq)]
pub struct EmojiSet {
    pub identifier: String,
    pub name: Option<String>,
    pub emojis: Vec<CustomEmoji>,
    pub author: String,
    pub picture: Option<String>,
    pub about: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmojiPackReference {
    pub coordinate: String,
    pub identifier: String,
    pub author: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiscoverableEmojiPack {
    pub coordinate: String,
    pub identifier: String,
    pub name: String,
    pub author: String,
    pub picture: Option<String>,
    pub about: Option<String>,
    pub emojis: Vec<CustomEmoji>,
    pub created_at: u64,
}
/// Global state for custom emojis from Nostr
/// Store for custom emojis with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct CustomEmojisStore {
    pub data: Vec<CustomEmoji>,
}
/// Store for emoji sets with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct EmojiSetsStore {
    pub data: Vec<EmojiSet>,
}
#[derive(Clone, Debug, Default, Store)]
pub struct EmojiPackRefsStore {
    pub data: Vec<EmojiPackReference>,
}
#[derive(Clone, Debug, Default, Store)]
pub struct DiscoverableEmojiPacksStore {
    pub data: Vec<DiscoverableEmojiPack>,
}
pub static CUSTOM_EMOJIS: GlobalSignal<Store<CustomEmojisStore>> =
    Signal::global(|| Store::new(CustomEmojisStore::default()));
pub static EMOJI_SETS: GlobalSignal<Store<EmojiSetsStore>> =
    Signal::global(|| Store::new(EmojiSetsStore::default()));
pub static EMOJI_PACK_REFS: GlobalSignal<Store<EmojiPackRefsStore>> =
    Signal::global(|| Store::new(EmojiPackRefsStore::default()));
pub static DISCOVERABLE_EMOJI_PACKS: GlobalSignal<Store<DiscoverableEmojiPacksStore>> =
    Signal::global(|| Store::new(DiscoverableEmojiPacksStore::default()));
pub static EMOJI_FETCH_TIME: GlobalSignal<Option<Timestamp>> = Signal::global(|| None);
pub static DISCOVERABLE_EMOJI_PACKS_FETCH_TIME: GlobalSignal<Option<Timestamp>> =
    Signal::global(|| None);
pub static DISCOVERABLE_EMOJI_PACKS_LOADING: GlobalSignal<bool> = Signal::global(|| false);
#[cfg(feature = "web")]
const RECENT_EMOJIS_KEY: &str = "nostr_blue_recent_emojis";
const MAX_RECENT: usize = 14;
const DEFAULT_RECENT: &[&str] = &["❤️", "👍", "😂", "🔥", "😮", "😢", "🎉"];
pub static RECENT_EMOJIS: GlobalSignal<Vec<String>> = Signal::global(|| {
    load_recent_emojis().unwrap_or_else(|| DEFAULT_RECENT.iter().map(|s| s.to_string()).collect())
});
/// Load recent emojis from localStorage
pub fn load_recent_emojis() -> Option<Vec<String>> {
    #[cfg(feature = "web")]
    {
        use web_sys::window;
        let storage = window()?.local_storage().ok()??;
        let value = storage.get_item(RECENT_EMOJIS_KEY).ok()??;
        serde_json::from_str(&value).ok()
    }
    #[cfg(not(feature = "web"))]
    {
        None
    }
}
/// Save an emoji to recents (moves to front if already exists)
pub fn save_recent_emoji(emoji: String) {
    let mut recent = RECENT_EMOJIS.write();
    recent.retain(|e| e != &emoji);
    recent.insert(0, emoji);
    recent.truncate(MAX_RECENT);
    #[cfg(feature = "web")]
    {
        use web_sys::window;
        if let Some(storage) = window().and_then(|w| w.local_storage().ok()).flatten() {
            let _ = storage.set_item(
                RECENT_EMOJIS_KEY,
                &serde_json::to_string(&*recent).unwrap_or_default(),
            );
        }
    }
}

fn parse_discoverable_pack(event: &nostr_sdk::Event) -> Option<DiscoverableEmojiPack> {
    if event.kind != Kind::EmojiSet {
        return None;
    }

    let identifier = event.tags.identifier()?.to_string();
    let author = event.pubkey.to_hex();
    let coordinate = format!("30030:{}:{}", author, identifier);

    let mut name = None;
    let mut picture = None;
    let mut about = None;
    let mut emojis = Vec::new();

    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        if slice.len() >= 3 && slice[0] == "emoji" {
            emojis.push(CustomEmoji {
                shortcode: slice[1].to_string(),
                image_url: slice[2].to_string(),
            });
        } else if slice.len() >= 2 {
            match slice[0].as_str() {
                "name" => name = Some(slice[1].to_string()),
                "picture" => picture = Some(slice[1].to_string()),
                "about" => about = Some(slice[1].to_string()),
                _ => {}
            }
        }
    }

    if emojis.is_empty() {
        return None;
    }

    Some(DiscoverableEmojiPack {
        coordinate,
        identifier: identifier.clone(),
        name: name.unwrap_or(identifier),
        author,
        picture,
        about,
        emojis,
        created_at: event.created_at.as_secs(),
    })
}

fn parse_emoji_collection(event: &nostr_sdk::Event) -> (Vec<CustomEmoji>, Vec<EmojiPackReference>) {
    let mut custom_emojis = Vec::new();
    let mut emoji_pack_refs = Vec::new();

    for tag in event.tags.iter() {
        let tag_slice = tag.as_slice();
        if tag_slice.len() >= 3 && tag_slice[0] == "emoji" {
            custom_emojis.push(CustomEmoji {
                shortcode: tag_slice[1].to_string(),
                image_url: tag_slice[2].to_string(),
            });
        } else if tag_slice.len() >= 2 && tag_slice[0] == "a" {
            let coordinate = tag_slice[1].to_string();
            let parts: Vec<&str> = coordinate.splitn(3, ':').collect();
            if parts.len() >= 3 && parts[0] == "30030" {
                emoji_pack_refs.push(EmojiPackReference {
                    coordinate: coordinate.clone(),
                    identifier: parts[2].to_string(),
                    author: parts[1].to_string(),
                });
            }
        }
    }

    (custom_emojis, emoji_pack_refs)
}

pub fn parse_emoji_set(event: &nostr_sdk::Event) -> Option<EmojiSet> {
    if event.kind != Kind::EmojiSet {
        return None;
    }

    let identifier = event.tags.identifier()?.to_string();
    let mut emojis = Vec::new();
    let mut title = None;
    let mut name = None;
    let mut picture = None;
    let mut about = None;

    for tag in event.tags.iter() {
        let tag_slice = tag.as_slice();
        if tag_slice.len() >= 3 && tag_slice[0] == "emoji" {
            emojis.push(CustomEmoji {
                shortcode: tag_slice[1].to_string(),
                image_url: tag_slice[2].to_string(),
            });
        } else if tag_slice.len() >= 2 {
            match tag_slice[0].as_str() {
                "title" => title = Some(tag_slice[1].to_string()),
                "name" => name = Some(tag_slice[1].to_string()),
                "picture" => picture = Some(tag_slice[1].to_string()),
                "about" => about = Some(tag_slice[1].to_string()),
                _ => {}
            }
        }
    }

    if emojis.is_empty() {
        return None;
    }

    Some(EmojiSet {
        identifier,
        // NIP-51 prefers the newer `title` tag; fall back to legacy `name`.
        name: title.or(name),
        emojis,
        author: event.pubkey.to_hex(),
        picture,
        about,
    })
}

/// Fetch a single emoji set (kind 30030) by its `naddr` coordinate.
///
/// Mirrors `packs_store::fetch_pack_by_naddr`: decode the bech32 coordinate,
/// build a kind/author/identifier filter, fetch via the aggregated path with a
/// DB fallback, and parse with [`parse_emoji_set`].
pub async fn fetch_emoji_set_by_naddr(
    naddr: &str,
) -> std::result::Result<Option<EmojiSet>, String> {
    let coord = nostr::nips::nip01::Coordinate::from_bech32(naddr)
        .map_err(|e| format!("Invalid naddr: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::EmojiSet)
        .author(coord.public_key)
        .identifier(&coord.identifier);

    let events = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(10),
    )
    .await?;

    Ok(events.into_iter().next().and_then(|e| parse_emoji_set(&e)))
}

async fn load_latest_event(
    client: &std::sync::Arc<nostr_sdk::Client>,
    filter: Filter,
    label: &str,
) -> Option<nostr_sdk::Event> {
    match crate::stores::nostr_client::fetch_events_aggregated(
        filter.clone(),
        Duration::from_secs(5),
    )
    .await
    {
        Ok(events) if !events.is_empty() => {
            log::info!("Loaded {} {} event(s) from fetch path", events.len(), label);
            return events.first().cloned();
        }
        Ok(_) => {}
        Err(e) => {
            log::warn!(
                "Failed to fetch {} from relays: {}, falling back to DB",
                label,
                e
            );
        }
    }

    match client.database().query(filter).await {
        Ok(events) => {
            log::info!("Loaded {} {} event(s) from database", events.len(), label);
            events.first().cloned()
        }
        Err(e) => {
            log::error!("Failed to query {} from database: {}", label, e);
            None
        }
    }
}

/// Fetch user's custom emojis (kind 10030) and emoji sets (kind 30030)
pub async fn fetch_custom_emojis(pubkey: String) {
    log::info!("Fetching custom emojis for pubkey: {}", pubkey);
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => {
            log::warn!("Client not initialized, skipping emoji fetch");
            return;
        }
    };
    let public_key = match PublicKey::parse(&pubkey) {
        Ok(pk) => pk,
        Err(e) => {
            log::error!("Failed to parse pubkey: {}", e);
            return;
        }
    };
    let emoji_list_filter = Filter::new()
        .kind(Kind::from(10030))
        .author(public_key)
        .limit(1);
    let mut custom_emojis = Vec::new();
    let mut parsed_pack_refs = Vec::new();

    if let Some(emoji_list) = load_latest_event(&client, emoji_list_filter, "emoji list").await {
        let (emojis, pack_refs) = parse_emoji_collection(&emoji_list);
        custom_emojis = emojis;
        parsed_pack_refs = pack_refs;
    }

    log::info!(
        "Found {} direct emojis and {} emoji set references",
        custom_emojis.len(),
        parsed_pack_refs.len()
    );

    let mut emoji_sets = Vec::new();
    for pack_ref in &parsed_pack_refs {
        let author_pk = match PublicKey::parse(&pack_ref.author) {
            Ok(pk) => pk,
            Err(e) => {
                log::warn!("Failed to parse author pubkey {}: {}", pack_ref.author, e);
                continue;
            }
        };
        let set_filter = Filter::new()
            .kind(Kind::from(30030))
            .author(author_pk)
            .identifier(pack_ref.identifier.clone())
            .limit(1);

        if let Some(set_event) = load_latest_event(&client, set_filter, "emoji set").await {
            if let Some(set) = parse_emoji_set(&set_event) {
                emoji_sets.push(set);
            }
        }
    }

    log::info!("Loaded {} emoji sets with emojis", emoji_sets.len());
    *CUSTOM_EMOJIS.read().data().write() = custom_emojis;
    *EMOJI_SETS.read().data().write() = emoji_sets;
    *EMOJI_PACK_REFS.read().data().write() = parsed_pack_refs;
    *EMOJI_FETCH_TIME.write() = Some(Timestamp::now());
}
/// Initialize emoji fetching for the authenticated user
pub fn init_emoji_fetch() {
    let auth_state = crate::stores::auth_store::AUTH_STATE.read();
    if let Some(pubkey) = &auth_state.pubkey {
        let pubkey = pubkey.clone();
        spawn(async move {
            fetch_custom_emojis(pubkey).await;
        });
    }
}
/// Check if we should refresh emojis (older than 5 minutes)
#[allow(dead_code)]
pub fn should_refresh_emojis() -> bool {
    if let Some(last_fetch) = *EMOJI_FETCH_TIME.read() {
        let now = Timestamp::now();
        let diff = now.as_secs() - last_fetch.as_secs();
        diff > 300
    } else {
        true
    }
}

pub fn is_pack_installed(coordinate: &str) -> bool {
    EMOJI_PACK_REFS
        .read()
        .data()
        .read()
        .iter()
        .any(|pack| pack.coordinate == coordinate)
}

pub async fn publish_emoji_collection(
    inline_emojis: Vec<CustomEmoji>,
    pack_coordinates: Vec<String>,
) -> std::result::Result<(), String> {
    let _client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;

    let emojis = inline_emojis
        .into_iter()
        .filter_map(|emoji| match Url::parse(&emoji.image_url) {
            Ok(url) => Some((emoji.shortcode, url)),
            Err(e) => {
                log::warn!("Skipping invalid emoji URL {}: {}", emoji.image_url, e);
                None
            }
        })
        .collect();

    let coordinate = pack_coordinates
        .into_iter()
        .filter_map(|pack| match nostr::nips::nip01::Coordinate::parse(&pack) {
            Ok(coord) => Some(coord),
            Err(e) => {
                log::warn!("Skipping invalid emoji pack coordinate {}: {}", pack, e);
                None
            }
        })
        .collect();

    let builder = EventBuilder::emojis(Emojis { emojis, coordinate });
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("emoji".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;

    Ok(())
}

pub async fn toggle_emoji_pack(coordinate: String) -> std::result::Result<(), String> {
    let pubkey = crate::stores::auth_store::AUTH_STATE.read()
        .pubkey
        .clone()
        .ok_or("No authenticated user found")?;

    let inline_emojis = CUSTOM_EMOJIS.read().data().read().clone();
    let current_refs = EMOJI_PACK_REFS.read().data().read().clone();
    let mut next_refs: Vec<String> = current_refs
        .iter()
        .map(|pack| pack.coordinate.clone())
        .collect();

    if next_refs.iter().any(|pack| pack == &coordinate) {
        next_refs.retain(|pack| pack != &coordinate);
    } else {
        next_refs.push(coordinate);
        next_refs.sort();
        next_refs.dedup();
    }

    publish_emoji_collection(inline_emojis, next_refs).await?;
    fetch_custom_emojis(pubkey).await;

    Ok(())
}

pub async fn fetch_discoverable_emoji_packs(limit: usize) -> std::result::Result<(), String> {
    *DISCOVERABLE_EMOJI_PACKS_LOADING.write() = true;

    let filter = Filter::new().kind(Kind::EmojiSet).limit(limit);
    let events =
        match crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await
        {
            Ok(events) => events,
            Err(e) => {
                *DISCOVERABLE_EMOJI_PACKS_LOADING.write() = false;
                return Err(e);
            }
        };

    let mut seen = HashSet::new();
    let mut packs: Vec<DiscoverableEmojiPack> = events
        .iter()
        .filter_map(parse_discoverable_pack)
        .filter(|pack| seen.insert(pack.coordinate.clone()))
        .collect();

    packs.sort_by_key(|b| std::cmp::Reverse(b.created_at));

    *DISCOVERABLE_EMOJI_PACKS.read().data().write() = packs;
    *DISCOVERABLE_EMOJI_PACKS_FETCH_TIME.write() = Some(Timestamp::now());
    *DISCOVERABLE_EMOJI_PACKS_LOADING.write() = false;

    Ok(())
}

pub fn should_refresh_discoverable_emoji_packs() -> bool {
    if let Some(last_fetch) = *DISCOVERABLE_EMOJI_PACKS_FETCH_TIME.read() {
        let now = Timestamp::now();
        now.as_secs().saturating_sub(last_fetch.as_secs()) > 300
    } else {
        true
    }
}
