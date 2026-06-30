//! Music Library — a NIP-51 kind-30003 bookmark set (d-tag "music-library")
//! holding tracks the user has saved. Mirrors `podcast_subscription.rs`.
//!
//! Supports all track sources: Nostr tracks/episodes/radio are stored as `a`
//! tags (coordinates); Wavlake/RSS tracks are stored as NIP-73 `i` tags.
//! Display metadata is NOT persisted (id-only tags) and is re-hydrated at
//! render time, matching the podcast-subscriptions convention.

use crate::stores::{auth_store, nostr_client};
use crate::stores::music_player::MusicTrack;
use crate::stores::nostr_music::TrackSource;
use dioxus::prelude::*;
use nostr_sdk::{EventBuilder, Filter, FromBech32, Kind, Tag};
use std::time::Duration;

const LIST_KIND: u16 = 30003;
const D_TAG: &str = "music-library";

/// A saved track in the user's Music Library. Carries the source identifier
/// (for tag round-tripping + re-hydration) plus optional cached display fields
/// (populated in-session from the live `MusicTrack`, `None` on cold relay load).
#[derive(Clone, Debug, PartialEq)]
pub struct MusicLibraryItem {
    /// The original `MusicTrack.id` (Wavlake track id / Nostr event id / RSS
    /// episode id). Used for source-specific re-hydration.
    pub track_id: String,
    pub source: TrackSource,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album_art_url: Option<String>,
}

impl MusicLibraryItem {
    /// Build a library item from a playable track (at "+" time).
    pub fn from_track(track: &MusicTrack) -> Self {
        Self {
            track_id: track.id.clone(),
            source: track.source.clone(),
            title: Some(track.title.clone()),
            artist: (!track.artist.is_empty()).then(|| track.artist.clone()),
            album_art_url: track.album_art_url.clone(),
        }
    }

    /// Stable unique key for dedup / `is_saved`. Equal to the persisted tag
    /// value so it round-trips through the relay.
    pub fn key(&self) -> String {
        match &self.source {
            TrackSource::Nostr { coordinate, .. }
            | TrackSource::Radio { coordinate, .. } => coordinate.clone(),
            TrackSource::NostrPodcast { addr, .. } => match addr {
                crate::stores::nostr_music::PodcastAddr::Legacy { coordinate, .. } => {
                    coordinate.clone()
                }
                crate::stores::nostr_music::PodcastAddr::F4 { event_id } => event_id.clone(),
            },
            TrackSource::Wavlake { .. } => format!("wavlake:{}", self.track_id),
            TrackSource::RssMusic { episode_id, .. } => format!("podcast:item:{}", episode_id),
            TrackSource::RssPodcast { episode_guid, .. } => {
                format!("podcast:item:guid:{}", episode_guid)
            }
            TrackSource::Bible { translation, book, chapter, .. } => {
                format!("bible:{}:{}:{}", translation, book, chapter)
            }
            TrackSource::Quran { reciter, surah, .. } => format!("quran:{}:{}", reciter, surah),
        }
    }

    /// The NIP-51/NIP-73 tag to persist for this item.
    fn to_tag(&self) -> Option<Tag> {
        match &self.source {
            TrackSource::Nostr { coordinate, .. } | TrackSource::Radio { coordinate, .. } => {
                Some(Tag::custom(nostr_sdk::TagKind::a(), vec![coordinate.clone()]))
            }
            TrackSource::NostrPodcast { addr, .. } => match addr {
                crate::stores::nostr_music::PodcastAddr::Legacy { coordinate, .. } => {
                    Some(Tag::custom(nostr_sdk::TagKind::a(), vec![coordinate.clone()]))
                }
                // NIP-F4 episodes are regular (event-id addressed); persist via `e` tag.
                crate::stores::nostr_music::PodcastAddr::F4 { event_id } => {
                    Some(Tag::custom(nostr_sdk::TagKind::e(), vec![event_id.clone()]))
                }
            },
            TrackSource::Wavlake { .. } => Some(Tag::custom(
                nostr_sdk::TagKind::i(),
                vec![format!("wavlake:{}", self.track_id)],
            )),
            TrackSource::RssMusic { episode_id, .. } => Some(Tag::custom(
                nostr_sdk::TagKind::i(),
                vec![format!("podcast:item:{}", episode_id)],
            )),
            TrackSource::RssPodcast { episode_guid, .. } => Some(Tag::custom(
                nostr_sdk::TagKind::i(),
                vec![format!("podcast:item:guid:{}", episode_guid)],
            )),
            // Bible/Quran are not persisted to the relay library.
            TrackSource::Bible { .. } | TrackSource::Quran { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Store, Default)]
pub struct MusicLibraryState {
    pub items: Vec<MusicLibraryItem>,
    pub loading: bool,
    pub error: Option<String>,
    pub loaded: bool,
}

pub static MUSIC_LIBRARY: GlobalStore<MusicLibraryState> = Global::new(MusicLibraryState::default);

/// Fetch the user's Music Library from Nostr (NIP-51 kind 30003, d "music-library").
pub async fn fetch_music_library() -> Result<Vec<MusicLibraryItem>, String> {
    log::info!("Fetching music library from Nostr (NIP-51 Kind 30003)...");
    {
        let mut state = MUSIC_LIBRARY.write();
        state.loading = true;
        state.error = None;
    }
    if !auth_store::is_authenticated() {
        let mut state = MUSIC_LIBRARY.write();
        state.loading = false;
        state.loaded = true;
        return Ok(Vec::new());
    }
    let client = match nostr_client::NOSTR_CLIENT.read().as_ref() {
        Some(c) => c.clone(),
        None => {
            let err = "Client not initialized".to_string();
            let mut state = MUSIC_LIBRARY.write();
            state.loading = false;
            state.loaded = true;
            state.error = Some(err.clone());
            return Err(err);
        }
    };
    let pubkey_str = auth_store::AUTH_STATE.read().pubkey.clone().ok_or_else(|| {
        let err = "No pubkey".to_string();
        {
            let mut state = MUSIC_LIBRARY.write();
            state.loading = false;
            state.loaded = true;
            state.error = Some(err.clone());
        }
        err
    })?;
    let pubkey = match nostr_sdk::PublicKey::from_bech32(&pubkey_str)
        .or_else(|_| nostr_sdk::PublicKey::from_hex(&pubkey_str))
    {
        Ok(pk) => pk,
        Err(e) => {
            let err = format!("Invalid pubkey: {}", e);
            let mut state = MUSIC_LIBRARY.write();
            state.loading = false;
            state.loaded = true;
            state.error = Some(err.clone());
            return Err(err);
        }
    };
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(LIST_KIND))
        .identifier(D_TAG)
        .limit(1);
    nostr_client::ensure_relays_ready(&client).await;
    let items = match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                log::info!("Found music library event: {}", event.id);
                parse_library_event(&event)
            } else {
                log::info!("No music library found on Nostr");
                Vec::new()
            }
        }
        Err(e) => {
            let error_msg = format!("Fetch error: {}", e);
            {
                let mut state = MUSIC_LIBRARY.write();
                state.error = Some(error_msg.clone());
                state.loading = false;
            }
            return Err(error_msg);
        }
    };
    {
        let mut state = MUSIC_LIBRARY.write();
        state.items = items.clone();
        state.loading = false;
        state.loaded = true;
    }
    Ok(items)
}

/// Parse a kind-30003 music-library event into items (id-only; display fields
/// are left `None` for runtime hydration).
fn parse_library_event(event: &nostr_sdk::Event) -> Vec<MusicLibraryItem> {
    let mut items = Vec::new();
    for tag in event.tags.iter() {
        let tag_vec: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
        match tag_vec.as_slice() {
            ["a", coordinate] | ["a", coordinate, _] => {
                if let Some(item) = item_from_coordinate(coordinate) {
                    items.push(item);
                }
            }
            ["i", identifier] | ["i", identifier, _] => {
                if let Some(item) = item_from_external_id(identifier) {
                    items.push(item);
                }
            }
            _ => {}
        }
    }
    items
}

/// Reconstruct a library item from an `a`-tag coordinate ("kind:pubkey:d-tag").
fn item_from_coordinate(coordinate: &str) -> Option<MusicLibraryItem> {
    let mut parts = coordinate.splitn(3, ':');
    let kind: u16 = parts.next()?.parse().ok()?;
    let pubkey = parts.next()?.to_string();
    let d_tag = parts.next()?.to_string();
    let (source, track_id) = match kind {
        36787 => (
            TrackSource::Nostr {
                coordinate: coordinate.to_string(),
                pubkey: pubkey.clone(),
                d_tag: d_tag.clone(),
            },
            coordinate.to_string(),
        ),
        30054 => (
            TrackSource::NostrPodcast {
                pubkey: pubkey.clone(),
                podcast_title: String::new(),
                addr: crate::stores::nostr_music::PodcastAddr::Legacy {
                    coordinate: coordinate.to_string(),
                    d_tag: d_tag.clone(),
                },
            },
            coordinate.to_string(),
        ),
        31237 => (
            TrackSource::Radio {
                coordinate: coordinate.to_string(),
                pubkey: pubkey.clone(),
                d_tag: d_tag.clone(),
                station_name: String::new(),
            },
            coordinate.to_string(),
        ),
        _ => return None,
    };
    Some(MusicLibraryItem {
        track_id,
        source,
        title: None,
        artist: None,
        album_art_url: None,
    })
}

/// Reconstruct a library item from an NIP-73 `i`-tag external id.
fn item_from_external_id(identifier: &str) -> Option<MusicLibraryItem> {
    if let Some(track_id) = identifier.strip_prefix("wavlake:") {
        return Some(MusicLibraryItem {
            track_id: track_id.to_string(),
            source: TrackSource::Wavlake {
                artist_id: String::new(),
                album_id: String::new(),
            },
            title: None,
            artist: None,
            album_art_url: None,
        });
    }
    if let Some(guid) = identifier.strip_prefix("podcast:item:guid:") {
        return Some(MusicLibraryItem {
            track_id: guid.to_string(),
            source: TrackSource::RssPodcast {
                feed_url: String::new(),
                podcast_id: None,
                episode_guid: guid.to_string(),
                podcast_title: String::new(),
            },
            title: None,
            artist: None,
            album_art_url: None,
        });
    }
    if let Some(episode_id_str) = identifier.strip_prefix("podcast:item:") {
        if let Ok(episode_id) = episode_id_str.parse::<u64>() {
            return Some(MusicLibraryItem {
                track_id: episode_id_str.to_string(),
                source: TrackSource::RssMusic {
                    feed_id: 0,
                    feed_url: String::new(),
                    episode_id,
                    album_title: String::new(),
                    artist: None,
                },
                title: None,
                artist: None,
                album_art_url: None,
            });
        }
    }
    None
}

/// Add a track to the library. Idempotent: returns early if already saved.
pub async fn add_track(track: &MusicTrack) -> Result<(), String> {
    let item = MusicLibraryItem::from_track(track);
    let key = item.key();
    if is_saved(&key) {
        return Ok(());
    }
    let mut items = MUSIC_LIBRARY.read().items.clone();
    items.push(item);
    publish_library(&items).await?;
    {
        let mut state = MUSIC_LIBRARY.write();
        state.items = items;
    }
    Ok(())
}

/// Remove a track from the library by its key.
pub async fn remove_track(key: &str) -> Result<(), String> {
    let mut items = MUSIC_LIBRARY.read().items.clone();
    let before = items.len();
    items.retain(|i| i.key() != key);
    if items.len() == before {
        return Ok(());
    }
    publish_library(&items).await?;
    {
        let mut state = MUSIC_LIBRARY.write();
        state.items = items;
    }
    Ok(())
}

async fn publish_library(items: &[MusicLibraryItem]) -> Result<(), String> {
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    if !nostr_client::has_signer() {
        return Err("No signer available".to_string());
    }
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    nostr_client::ensure_relays_ready(&client).await;
    let mut tags = vec![
        Tag::identifier(D_TAG),
        Tag::custom(
            nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("title")),
            vec!["My Music Library".to_string()],
        ),
        Tag::custom(
            nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("alt")),
            vec!["User's saved music tracks".to_string()],
        ),
    ];
    for item in items {
        if let Some(tag) = item.to_tag() {
            tags.push(tag);
        }
    }
    let builder = EventBuilder::new(Kind::from(LIST_KIND), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("music-library".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(())
}

pub fn get_items() -> Vec<MusicLibraryItem> {
    MUSIC_LIBRARY.read().items.clone()
}

/// Compute the stable library key for a track (used by card buttons).
pub fn track_key(track: &MusicTrack) -> String {
    MusicLibraryItem::from_track(track).key()
}

pub fn is_saved(key: &str) -> bool {
    MUSIC_LIBRARY
        .read()
        .items
        .iter()
        .any(|i| i.key() == key)
}

pub fn is_loading() -> bool {
    MUSIC_LIBRARY.read().loading
}

pub fn is_loaded() -> bool {
    MUSIC_LIBRARY.read().loaded
}
