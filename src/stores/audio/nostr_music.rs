use crate::stores::{auth_store, nostr_client, profiles};
use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use lru::LruCache;
use nostr::nips::nip19::{Nip19Coordinate, Nip19Event, ToBech32};
use nostr_sdk::client::Client;
use nostr_sdk::prelude::{
    EventBuilder, EventId, Filter, FromBech32, IntoEventBuilder, Kind, PublicKey,
    SingleLetterTag, Tag, Timestamp,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::time::Duration;
use nostr::event::Event;
/// Kind number for Music Track events
pub const KIND_MUSIC_TRACK: u16 = 36787;
/// Kind number for Playlist events
pub const KIND_PLAYLIST: u16 = 34139;
/// How a Nostr podcast episode is addressed.
///
/// Legacy custom-NIP episodes are addressable (kind 30054, referenced by an
/// `naddr` coordinate); NIP-F4 episodes are regular (kind 54, referenced by an
/// event id / `nevent`). This enum lets routing/share logic branch cleanly.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PodcastAddr {
    /// Legacy custom-NIP episode: addressable kind 30054.
    Legacy {
        /// Event coordinate: "30054:pubkey:d-tag"
        coordinate: String,
        /// d-tag identifier
        d_tag: String,
    },
    /// NIP-F4 episode: regular kind 54, referenced by event id.
    F4 {
        /// Event id (hex)
        event_id: String,
    },
}

impl PodcastAddr {
    /// Whether this is a NIP-F4 (kind 54) episode.
    pub fn is_f4(&self) -> bool {
        matches!(self, PodcastAddr::F4 { .. })
    }
}

/// Source of the music track for routing and display purposes
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackSource {
    /// Track from Wavlake API
    Wavlake { artist_id: String, album_id: String },
    /// Track from Nostr Kind 36787 event
    Nostr {
        /// Event coordinate: "36787:pubkey:d-tag"
        coordinate: String,
        /// Author pubkey (hex)
        pubkey: String,
        /// d-tag identifier
        d_tag: String,
    },
    /// Podcast episode from Nostr (legacy kind 30054 or NIP-F4 kind 54)
    NostrPodcast {
        /// Author pubkey (hex)
        pubkey: String,
        /// Podcast show title
        podcast_title: String,
        /// Episode addressing (legacy coordinate naddr or NIP-F4 event id)
        addr: PodcastAddr,
    },
    /// Podcast episode from RSS feed
    RssPodcast {
        /// RSS feed URL
        feed_url: String,
        /// Podcast Index feed ID (for routing)
        podcast_id: Option<u64>,
        /// Episode GUID
        episode_guid: String,
        /// Podcast show title
        podcast_title: String,
    },
    /// Music track from RSS feed (Podcast Index medium="music")
    RssMusic {
        /// Podcast Index feed ID
        feed_id: u64,
        /// RSS feed URL
        feed_url: String,
        /// Episode/track ID from Podcast Index
        episode_id: u64,
        /// Album title (from feed title)
        album_title: String,
        /// Artist name (from feed author)
        artist: Option<String>,
    },
    /// Live internet radio station from Kind 31237
    Radio {
        /// Event coordinate: "31237:pubkey:d-tag"
        coordinate: String,
        /// Station owner pubkey (hex)
        pubkey: String,
        /// d-tag identifier
        d_tag: String,
        /// Station name
        station_name: String,
    },
    /// Bible audio chapter
    Bible {
        translation: String,
        book: String,
        chapter: u32,
        reader: String,
    },
    /// Quran audio surah
    Quran {
        reciter: String,
        surah: u32,
    },
}
impl Default for TrackSource {
    fn default() -> Self {
        TrackSource::Wavlake {
            artist_id: String::new(),
            album_id: String::new(),
        }
    }
}
/// Parsed Kind 36787 music track event
#[derive(Clone, Debug, PartialEq)]
pub struct NostrTrack {
    /// Event ID
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// d-tag identifier
    pub d_tag: String,
    /// Coordinate string: "36787:pubkey:d-tag"
    pub coordinate: String,
    /// Track title
    pub title: String,
    /// Audio URL
    pub url: String,
    /// Cover image URL
    pub image: Option<String>,
    /// CSS gradient fallback
    pub gradient: Option<String>,
    /// Duration in seconds
    pub duration: Option<u32>,
    /// Genre tags
    pub genres: Vec<String>,
    /// AI-generated flag
    pub ai_generated: bool,
    /// Created timestamp
    pub created_at: u64,
}
impl NostrTrack {
    /// Get the track source for this nostr track
    #[allow(dead_code)]
    pub fn get_source(&self) -> TrackSource {
        TrackSource::Nostr {
            coordinate: self.coordinate.clone(),
            pubkey: self.pubkey.clone(),
            d_tag: self.d_tag.clone(),
        }
    }
}
/// Parsed music playlist event — kind 34139 (NIP-WW), or a playlist-shaped
/// NIP-51 bookmark set (kind 30003 with `t: playlist` and `a` tags resolving
/// to kind-36787 tracks, the pattern used by Plume and nostr.blue's own
/// music library).
#[derive(Clone, Debug, PartialEq)]
pub struct NostrPlaylist {
    /// Event ID
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// d-tag identifier
    pub d_tag: String,
    /// Source event kind (34139 or 30003) — used for coordinates/nadrs
    pub kind: u16,
    /// Coordinate string: "kind:pubkey:d-tag"
    pub coordinate: String,
    /// NIP-19 naddr for this playlist (bech32-encoded)
    pub naddr: Option<String>,
    /// Playlist title
    pub title: String,
    /// Description
    pub description: Option<String>,
    /// Cover image URL
    pub image: Option<String>,
    /// CSS gradient fallback
    pub gradient: Option<String>,
    /// Track coordinates (ordered) - "36787:pubkey:d-tag"
    pub track_refs: Vec<String>,
    /// Per-track relay hints from `a`-tag third elements
    /// (coordinate -> relay URLs). Keyed by track coordinate.
    pub track_hints: HashMap<String, Vec<String>>,
    /// Category/genre tags
    pub categories: Vec<String>,
    /// Public visibility (Quartz/NIP-WW rule: `private` is authoritative —
    /// an event with no visibility tags defaults to public).
    pub is_public: bool,
    /// Collaborative editing
    pub is_collaborative: bool,
    /// Created timestamp
    pub created_at: u64,
}
/// Cached track with fetch timestamp
#[derive(Clone, Debug)]
pub struct CachedTrack {
    pub track: NostrTrack,
    pub fetched_at: DateTime<Utc>,
}
/// Cache TTL in seconds (5 minutes, same as profiles)
const CACHE_TTL_SECONDS: i64 = 300;
/// Maximum tracks in cache
const TRACK_CACHE_CAPACITY: usize = 2000;
/// Maximum playlists in cache
const PLAYLIST_CACHE_CAPACITY: usize = 500;
/// Global LRU cache for nostr tracks (coordinate -> CachedTrack)
pub static NOSTR_TRACK_CACHE: GlobalSignal<LruCache<String, CachedTrack>> =
    Signal::global(|| LruCache::new(NonZeroUsize::new(TRACK_CACHE_CAPACITY).unwrap()));
/// Global LRU cache for nostr playlists (coordinate -> NostrPlaylist)
pub static NOSTR_PLAYLIST_CACHE: GlobalSignal<LruCache<String, NostrPlaylist>> =
    Signal::global(|| LruCache::new(NonZeroUsize::new(PLAYLIST_CACHE_CAPACITY).unwrap()));
/// Filter for music feed source selection
#[derive(Clone, Debug, PartialEq, Default)]
#[allow(dead_code)]
pub enum MusicFeedFilter {
    #[default]
    All,
    Wavlake,
    Nostr,
    Following,
}
/// Parse a Kind 36787 event into a NostrTrack
pub fn parse_track_event(event: &Event) -> Result<NostrTrack, String> {
    let pubkey = event.pubkey.to_hex();
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;
    let title = get_tag_value(event, "title").ok_or("Missing required 'title' tag")?;
    let url = get_tag_value(event, "url").ok_or("Missing required 'url' tag")?;
    let coordinate = format!("{}:{}:{}", KIND_MUSIC_TRACK, pubkey, d_tag);
    let image = get_tag_value(event, "image");
    let gradient = event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("gradient"))
        .and_then(|t| {
            let slice = t.as_slice();
            slice.get(2).or(slice.get(1)).map(|s| s.to_string())
        });
    let duration = get_tag_value(event, "duration").and_then(|d| d.parse::<u32>().ok());
    let genres: Vec<String> = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("t"))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
        .filter(|g| g != "music")
        .collect();
    let ai_generated = get_tag_value(event, "ai-generated")
        .map(|v| v == "true")
        .unwrap_or(false);
    Ok(NostrTrack {
        event_id: event.id.to_hex(),
        pubkey,
        d_tag,
        coordinate,
        title,
        url,
        image,
        gradient,
        duration,
        genres,
        ai_generated,
        created_at: event.created_at.as_secs(),
    })
}
/// Does a NIP-51 bookmark-set event carry a music playlist? (Plume pattern:
/// a `t: playlist` tag plus at least one `a` tag resolving to a kind-36787
/// track.)
pub fn is_playlist_shaped_event(event: &Event) -> bool {
    let kind = event.kind.as_u16();
    if kind != crate::utils::list_kinds::NAMED_BOOKMARKS {
        return false;
    }
    let has_playlist_tag = event
        .tags
        .iter()
        .any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("t")
            && t.as_slice().get(1).map(|s| s.as_str()) == Some("playlist"));
    if !has_playlist_tag {
        return false;
    }
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("a"))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.as_str()))
        .any(|r| r.starts_with(&format!("{}:", KIND_MUSIC_TRACK)))
}

/// Heuristic: does the `content` field look like encrypted payload
/// (NIP-44 base64 or NIP-04 base64+"?iv=") rather than a plain-text
/// description? Encrypted payloads are a single whitespace-free token;
/// real descriptions virtually always contain spaces.
pub fn content_looks_encrypted(content: &str) -> bool {
    if content.len() < 33 {
        return false;
    }
    !content.contains(char::is_whitespace)
}

/// Parse a music playlist event into a [NostrPlaylist].
///
/// Accepts kind 34139 (NIP-WW) and playlist-shaped kind 30003 bookmark sets
/// ([is_playlist_shaped_event]). Visibility follows the Quartz/NIP-WW rule:
/// the `private` tag is authoritative and absent flags default to public.
pub fn parse_playlist_event(event: &Event) -> Result<NostrPlaylist, String> {
    let kind = event.kind.as_u16();
    if kind != KIND_PLAYLIST && !is_playlist_shaped_event(event) {
        return Err(format!(
            "Not a music playlist event (kind {})",
            kind
        ));
    }
    let pubkey = event.pubkey.to_hex();
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;
    let title = get_tag_value(event, "title")
        .or_else(|| {
            // NIP-51 bookmark sets use `name` (deprecated in favor of `title`).
            if kind == crate::utils::list_kinds::NAMED_BOOKMARKS {
                get_tag_value(event, "name")
            } else {
                None
            }
        })
        .ok_or("Missing required 'title' tag")?;
    let coordinate = format!("{}:{}:{}", kind, pubkey, d_tag);
    let naddr = build_naddr_for(kind, &pubkey, &d_tag);
    let description = get_tag_value(event, "description").or_else(|| {
        // Only fall back to `content` when it is not an encrypted payload:
        // kind-30003 sets may carry NIP-44/NIP-04 ciphertext there, which
        // must never be surfaced as a description.
        if !event.content.is_empty() && !content_looks_encrypted(&event.content) {
            Some(event.content.clone())
        } else {
            None
        }
    });
    let image = get_tag_value(event, "image");
    let gradient = event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("gradient"))
        .and_then(|t| {
            let slice = t.as_slice();
            slice.get(2).or(slice.get(1)).map(|s| s.to_string())
        });
    let mut track_refs: Vec<String> = Vec::new();
    let mut track_hints: HashMap<String, Vec<String>> = HashMap::new();
    for t in event.tags.iter() {
        let slice = t.as_slice();
        if slice.first().map(|s| s.as_str()) != Some("a") {
            continue;
        }
        if let Some(coord) = slice.get(1).map(|s| s.as_str()) {
            if coord.starts_with(&format!("{}:", KIND_MUSIC_TRACK)) {
                track_refs.push(coord.to_string());
                if let Some(hint) = slice.get(2).map(|s| s.to_string()) {
                    track_hints
                        .entry(coord.to_string())
                        .or_default()
                        .push(hint);
                }
            }
        }
    }
    let categories: Vec<String> = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("t"))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
        .collect();
    // Quartz/NIP-WW visibility rule: `private` wins when asserted; absent or
    // contradictory flags fall back to public. Matches MusicPlaylistEvent
    // in quartz (`isPrivate() ?: false`, `isPublic() = !isPrivate()`).
    let is_private = get_tag_value(event, "private")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let is_collaborative = get_tag_value(event, "collaborative")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    Ok(NostrPlaylist {
        event_id: event.id.to_hex(),
        pubkey,
        d_tag,
        kind,
        coordinate,
        naddr,
        title,
        description,
        image,
        gradient,
        track_refs,
        track_hints,
        categories,
        is_public: !is_private,
        is_collaborative,
        created_at: event.created_at.as_secs(),
    })
}

/// Backwards-compatible wrapper for kind-34139 playlists.
#[allow(dead_code)]
pub fn build_playlist_naddr(pubkey: &str, d_tag: &str) -> Option<String> {    build_naddr_for(KIND_PLAYLIST, pubkey, d_tag)
}

/// Build a bech32 `naddr` for an addressable event of any kind
/// ("kind:pubkey:d-tag" coordinate). This is the canonical NIP-19
/// reference for addressable events like playlists (34139) and
/// playlist-shaped bookmark sets (30003).
pub fn build_naddr_for(kind: u16, pubkey: &str, d_tag: &str) -> Option<String> {
    let pk = PublicKey::from_hex(pubkey).ok()?;
    let coordinate =
        nostr::nips::nip01::Coordinate::new(Kind::Custom(kind), pk)
            .identifier(d_tag);
    let nip19 = Nip19Coordinate::new(coordinate, vec![]);
    nip19.to_bech32().ok()
}

/// Build a bech32 share id for a Nostr podcast EPISODE.
///
/// Returns an `naddr` (kind 30054) for legacy episodes, or a `nevent` (kind 54,
/// with author + kind hint) for NIP-F4 episodes.
pub fn episode_share_bech32(pubkey: &str, addr: &PodcastAddr) -> Option<String> {
    let pk = PublicKey::from_hex(pubkey).ok()?;
    match addr {
        PodcastAddr::Legacy { d_tag, .. } => {
            let coord = nostr::nips::nip01::Coordinate::new(
                Kind::Custom(crate::utils::podcast::KIND_PODCAST_EPISODE),
                pk,
            )
            .identifier(d_tag);
            Nip19Coordinate::new(coord, vec![]).to_bech32().ok()
        }
        PodcastAddr::F4 { event_id } => {
            let id = EventId::from_hex(event_id).ok()?;
            Nip19Event::new(id)
                .author(pk)
                .kind(Kind::Custom(crate::utils::podcast::KIND_F4_EPISODE))
                .to_bech32()
                .ok()
        }
    }
}

/// Build a bech32 share id for a Nostr podcast SHOW (metadata event).
///
/// Returns an `naddr` for kind 30078 (legacy, d="podcast-metadata") or kind
/// 10154 (NIP-F4, replaceable with empty d-tag), depending on the episode's scheme.
pub fn show_share_bech32(pubkey: &str, addr: &PodcastAddr) -> Option<String> {
    let pk = PublicKey::from_hex(pubkey).ok()?;
    let coord = if addr.is_f4() {
        // NIP-F4 metadata: replaceable kind 10154, no d-tag.
        nostr::nips::nip01::Coordinate::new(
            Kind::Custom(crate::utils::podcast::KIND_F4_PODCAST_META),
            pk,
        )
    } else {
        nostr::nips::nip01::Coordinate::new(
            Kind::Custom(crate::utils::podcast::KIND_APP_DATA),
            pk,
        )
        .identifier("podcast-metadata")
    };
    Nip19Coordinate::new(coord, vec![]).to_bech32().ok()
}

/// Helper to get a tag value by name
fn get_tag_value(event: &Event, tag_name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}
/// Ensure music specialty relays (Basspistol/nostria — the primary hosts of
/// kind-36787 events) and, for signed-in users, the user's NIP-65 read relays
/// are connected before a music fetch. Idempotent; no-op-safe when logged out.
///
/// The NIP-65 wait is short (1s): the primary gating is reactive at call
/// sites (read `HAS_SIGNER` + `USER_RELAYS_APPLIED` in the effect — see
/// `routes/music/music_home.rs`), so this is only a tail chance for callers
/// without reactive gating; a long block here used to stall the /music page
/// by up to 5s.
async fn ensure_music_ready() {
    if let Some(client) = nostr_client::get_client() {
        let _ = crate::stores::relay::specialty::ensure_music_relays(&client).await;
    }
    let _ = crate::stores::relay::wait_for_user_relays(Duration::from_secs(1), "music").await;
}
/// Fetch nostr tracks with optional filter, genre and backward-pagination cursor.
///
/// The local SDK database (nostrdb/IndexedDB) is queried FIRST, before any
/// relay-connect waits: a warm DB returns instantly, and the background
/// relay refresh inside `fetch_events_aggregated` keeps it fresh. Music
/// relay connects only run when the DB misses and we go to network.
pub async fn fetch_nostr_tracks(
    filter: MusicFeedFilter,
    limit: usize,
    genre: Option<&str>,
    until: Option<Timestamp>,
) -> Result<Vec<NostrTrack>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let mut nostr_filter = Filter::new()
        .kind(Kind::from(KIND_MUSIC_TRACK))
        .limit(limit);
    if let Some(g) = genre {
        nostr_filter = nostr_filter.hashtag(g.to_lowercase());
    }
    if let Some(ts) = until {
        nostr_filter = nostr_filter.until(ts);
    }
    if let MusicFeedFilter::Following = filter {
        let current_pubkey = auth_store::get_pubkey().ok_or("Not logged in")?;
        let contacts = nostr_client::fetch_contacts(current_pubkey)
            .await
            .map_err(|e| format!("Failed to fetch contacts: {}", e))?;
        if contacts.is_empty() {
            return Ok(Vec::new());
        }
        let authors: Vec<PublicKey> = contacts
            .iter()
            .filter_map(|pk| PublicKey::from_hex(pk).ok())
            .collect();
        nostr_filter = nostr_filter.authors(authors);
    }
    // DB-first fast path: if the local database has tracks for this filter,
    // return them immediately (relay sync continues in the background via
    // fetch_events_aggregated's spawned refresh). This mirrors the home
    // feed's nostrdb pre-step and avoids blocking on relay connects.
    if let Ok(db_events) = client.database().query(nostr_filter.clone()).await {
        let mut tracks: Vec<NostrTrack> = db_events
            .iter()
            .filter_map(|e| parse_track_event(e).ok())
            .collect();
        if !tracks.is_empty() {
            tracks.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            tracks.truncate(limit);
            for track in &tracks {
                NOSTR_TRACK_CACHE.write().put(
                    track.coordinate.clone(),
                    CachedTrack {
                        track: track.clone(),
                        fetched_at: Utc::now(),
                    },
                );
            }
            // Fire-and-forget network refresh so the next visit is fresh.
            spawn_db_refresh(client.clone(), nostr_filter);
            return Ok(tracks);
        }
    }
    // Cold path: DB empty — ensure music relays + user relays, then fetch.
    ensure_music_ready().await;
    let events = nostr_client::fetch_events_aggregated(nostr_filter, Duration::from_secs(6))
        .await
        .map_err(|e| format!("Failed to fetch tracks: {}", e))?;
    let mut tracks: Vec<NostrTrack> = events
        .iter()
        .filter_map(|e| parse_track_event(e).ok())
        .collect();
    tracks.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    for track in &tracks {
        NOSTR_TRACK_CACHE.write().put(
            track.coordinate.clone(),
            CachedTrack {
                track: track.clone(),
                fetched_at: Utc::now(),
            },
        );
    }
    Ok(tracks)
}

/// Background relay refresh after a DB-fast-path hit (non-blocking).
fn spawn_db_refresh(client: std::sync::Arc<Client>, filter: Filter) {
    dioxus::prelude::spawn(async move {
        crate::stores::relay::specialty::ensure_music_relays(&client).await;
        let _ = crate::stores::relay::wait_for_user_relays(Duration::from_secs(1), "music").await;
        if let Err(e) = nostr_client::fetch_events_quorum(
            &client,
            filter,
            Duration::from_secs(6),
        )
        .await
        {
            log::debug!("Music background refresh failed: {}", e);
        }
    });
}
/// Fetch a single track by coordinate
pub async fn fetch_nostr_track_by_coordinate(
    pubkey: &str,
    d_tag: &str,
    relay_hints: Vec<String>,
) -> Result<Option<NostrTrack>, String> {
    let cache_key = format!("{}:{}:{}", KIND_MUSIC_TRACK, pubkey, d_tag);
    if let Some(cached) = NOSTR_TRACK_CACHE.read().peek(&cache_key) {
        let age = Utc::now().signed_duration_since(cached.fetched_at);
        if age.num_seconds() < CACHE_TTL_SECONDS {
            return Ok(Some(cached.track.clone()));
        }
    }
    let event = nostr_client::fetch_event_by_coordinate_with_relays(
        KIND_MUSIC_TRACK,
        pubkey.to_string(),
        d_tag.to_string(),
        relay_hints,
    )
    .await
    .map_err(|e| format!("Failed to fetch track: {}", e))?;
    if let Some(event) = event {
        let track = parse_track_event(&event)?;
        NOSTR_TRACK_CACHE.write().put(
            track.coordinate.clone(),
            CachedTrack {
                track: track.clone(),
                fetched_at: Utc::now(),
            },
        );
        Ok(Some(track))
    } else {
        Ok(None)
    }
}
/// Zap totals for the same track set + window are re-requested on every
/// filter toggle of /music; a 60s TTL collapses that into one relay round
/// trip per distinct (coords, days) pair.
type ZapTotalsCacheMap = HashMap<String, (instant::Instant, HashMap<String, u64>)>;
static ZAP_TOTALS_CACHE: std::sync::Mutex<Option<ZapTotalsCacheMap>> =
    std::sync::Mutex::new(None);
const ZAP_TOTALS_TTL: std::time::Duration = std::time::Duration::from_secs(60);

fn zap_cache_key(coords: &[String], days: Option<u32>) -> String {
    let mut sorted = coords.to_vec();
    sorted.sort();
    format!("{days:?}:{}", sorted.join(","))
}

/// Fetch zap totals for multiple tracks in one query
/// If `since_days` is provided, only counts zaps from that time period
pub async fn fetch_track_zap_totals(
    track_coordinates: Vec<String>,
    since_days: Option<u32>,
) -> Result<HashMap<String, u64>, String> {
    if track_coordinates.is_empty() {
        return Ok(HashMap::new());
    }
    let cache_key = zap_cache_key(&track_coordinates, since_days);
    {
        let cache = ZAP_TOTALS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, totals)) = cache
            .as_ref()
            .and_then(|map| map.get(&cache_key))
        {
            if at.elapsed() < ZAP_TOTALS_TTL {
                return Ok(totals.clone());
            }
        }
    }
    let _client = nostr_client::get_client().ok_or("Client not initialized")?;
    let mut filter = Filter::new().kind(Kind::ZapReceipt);
    if let Some(days) = since_days {
        let since = nostr::types::time::Timestamp::now() - Duration::from_secs(days as u64 * 24 * 60 * 60);
        filter = filter.since(since);
    }
    for coord in &track_coordinates {
        filter = filter.custom_tag(SingleLetterTag::from_char('a').unwrap(), coord.clone());
    }
    // 6s ceiling: the quorum helper early-exits in ~1-2s on a healthy pool;
    // this only caps the worst case when relays are flaky. Zap totals now
    // merge asynchronously into the rendered list, so this fetch no longer
    // gates first paint either.
    let zap_events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(6))
        .await
        .map_err(|e| format!("Failed to fetch zap receipts: {}", e))?;
    let mut totals: HashMap<String, u64> = HashMap::new();
    for zap in zap_events {
        let a_tag = zap
            .tags
            .iter()
            .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("a"))
            .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()));
        let amount = zap
            .tags
            .iter()
            .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("bolt11"))
            .and_then(|t| t.as_slice().get(1))
            .and_then(|bolt11| crate::utils::bolt11::parse_bolt11_amount_msats(bolt11));
        if let (Some(coord), Some(msats)) = (a_tag, amount) {
            *totals.entry(coord).or_default() += msats;
        }
    }
    {
        let mut cache = ZAP_TOTALS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        // Bounded: only distinct (coords, days) pairs are retained, each
        // expiring after 60s.
        cache
            .get_or_insert_with(HashMap::new)
            .retain(|_, (at, _)| at.elapsed() < ZAP_TOTALS_TTL);
        cache
            .get_or_insert_with(HashMap::new)
            .insert(cache_key, (instant::Instant::now(), totals.clone()));
    }
    Ok(totals)
}
/// Search nostr music tracks by title or artist name
///
/// This performs a hybrid search:
/// 1. Fetches recent Kind 36787 tracks from relays
/// 2. Batch fetches artist profiles for the tracks
/// 3. Filters client-side by title or artist name match
pub async fn search_nostr_tracks(query: &str, limit: usize) -> Result<Vec<NostrTrack>, String> {
    ensure_music_ready().await;
    let filter = Filter::new()
        .kind(Kind::from(KIND_MUSIC_TRACK))
        .limit(limit);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch tracks: {}", e))?;
    let mut tracks: Vec<NostrTrack> = events
        .iter()
        .filter_map(|e| parse_track_event(e).ok())
        .collect();
    if query.trim().is_empty() {
        tracks.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        return Ok(tracks);
    }
    let pubkeys: Vec<String> = tracks
        .iter()
        .map(|t| t.pubkey.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let profiles_map = profiles::fetch_profiles_batch(pubkeys)
        .await
        .unwrap_or_default();
    let query_lower = query.to_lowercase();
    tracks.retain(|track| {
        if track.title.to_lowercase().contains(&query_lower) {
            return true;
        }
        if track
            .genres
            .iter()
            .any(|g| g.to_lowercase().contains(&query_lower))
        {
            return true;
        }
        if let Some(profile) = profiles_map.get(&track.pubkey) {
            if let Some(name) = &profile.display_name {
                if name.to_lowercase().contains(&query_lower) {
                    return true;
                }
            }
            if let Some(name) = &profile.name {
                if name.to_lowercase().contains(&query_lower) {
                    return true;
                }
            }
        }
        false
    });
    tracks.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(tracks)
}
/// Search for nostr music artists by name
/// Returns unique artists (profiles) who have published Kind 36787 tracks
pub async fn search_nostr_artists(
    query: &str,
    limit: usize,
) -> Result<Vec<(String, profiles::Profile)>, String> {
    ensure_music_ready().await;
    let filter = Filter::new().kind(Kind::from(KIND_MUSIC_TRACK)).limit(200);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch tracks: {}", e))?;
    let pubkeys: Vec<String> = events
        .iter()
        .map(|e| e.pubkey.to_hex())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if pubkeys.is_empty() {
        return Ok(Vec::new());
    }
    let profiles_map = profiles::fetch_profiles_batch(pubkeys)
        .await
        .unwrap_or_default();
    let query_lower = query.to_lowercase();
    let results: Vec<(String, profiles::Profile)> = profiles_map
        .into_iter()
        .filter(|(_, profile)| {
            if query.is_empty() {
                return true;
            }
            profile
                .display_name
                .as_ref()
                .map(|n| n.to_lowercase().contains(&query_lower))
                .unwrap_or(false)
                || profile
                    .name
                    .as_ref()
                    .map(|n| n.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
        })
        .take(limit)
        .collect();
    Ok(results)
}
/// Fetch all tracks by a specific pubkey (for artist page)
pub async fn fetch_artist_tracks(pubkey: &str, limit: usize) -> Result<Vec<NostrTrack>, String> {
    ensure_music_ready().await;
    let public_key = PublicKey::from_hex(pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::from(KIND_MUSIC_TRACK))
        .author(public_key)
        .limit(limit);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch artist tracks: {}", e))?;
    let mut tracks: Vec<NostrTrack> = events
        .iter()
        .filter_map(|e| parse_track_event(e).ok())
        .collect();
    tracks.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    for track in &tracks {
        NOSTR_TRACK_CACHE.write().put(
            track.coordinate.clone(),
            CachedTrack {
                track: track.clone(),
                fetched_at: Utc::now(),
            },
        );
    }
    Ok(tracks)
}
/// Fetch playlists, optionally by author, with a backward-pagination cursor.
pub async fn fetch_playlists(
    author: Option<&str>,
    limit: usize,
    until: Option<Timestamp>,
) -> Result<Vec<NostrPlaylist>, String> {
    let mut filter = Filter::new().kind(Kind::from(KIND_PLAYLIST)).limit(limit);
    if let Some(author_pk) = author {
        let public_key = PublicKey::from_hex(author_pk)
            .or_else(|_| PublicKey::from_bech32(author_pk))
            .map_err(|e| format!("Invalid pubkey: {}", e))?;
        filter = filter.author(public_key);
    }
    if let Some(ts) = until {
        filter = filter.until(ts);
    }
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch playlists: {}", e))?;
    let mut playlists: Vec<NostrPlaylist> = events
        .iter()
        .filter_map(|e| parse_playlist_event(e).ok())
        .collect();
    playlists.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    for playlist in &playlists {
        NOSTR_PLAYLIST_CACHE
            .write()
            .put(playlist.coordinate.clone(), playlist.clone());
    }
    Ok(playlists)
}
/// Fetch a single playlist by author pubkey and d-tag (more efficient than fetching all and filtering)
#[allow(dead_code)]
pub async fn fetch_playlist_by_coordinate(
    author: &str,
    d_tag: &str,
    relay_hints: Vec<String>,
) -> Result<Option<NostrPlaylist>, String> {
    fetch_playlist_by_coordinate_with_kind(KIND_PLAYLIST, author, d_tag, relay_hints).await
}

/// Fetch a playlist of any supported kind (34139 or playlist-shaped 30003)
/// by coordinate. The kind comes from the parsed naddr so links stay
/// kind-correct.
pub async fn fetch_playlist_by_coordinate_with_kind(
    kind: u16,
    author: &str,
    d_tag: &str,
    relay_hints: Vec<String>,
) -> Result<Option<NostrPlaylist>, String> {
    let event = nostr_client::fetch_event_by_coordinate_with_relays(
        kind,
        author.to_string(),
        d_tag.to_string(),
        relay_hints,
    )
    .await
    .map_err(|e| format!("Failed to fetch playlist: {}", e))?;
    if let Some(event) = event {
        let playlist = parse_playlist_event(&event)?;
        NOSTR_PLAYLIST_CACHE
            .write()
            .put(playlist.coordinate.clone(), playlist.clone());
        Ok(Some(playlist))
    } else {
        Ok(None)
    }
}
/// Resolve playlist tracks from track references.
///
/// Cache-first, then fetches every missing coordinate **in parallel**
/// (hints from the playlist's per-`a`-tag relay URLs are passed through so
/// tracks that live only on niche relays — e.g. Plume sets pointing at
/// theforest.nostr1.com — are reachable).
pub async fn resolve_playlist_tracks(playlist: &NostrPlaylist) -> Result<Vec<NostrTrack>, String> {
    if playlist.track_refs.is_empty() {
        return Ok(Vec::new());
    }
    let mut tracks: Vec<(String, Option<NostrTrack>)> = Vec::new();
    let mut missing_refs: Vec<String> = Vec::new();
    for track_ref in &playlist.track_refs {
        if let Some(cached) = NOSTR_TRACK_CACHE.read().peek(track_ref) {
            let age = Utc::now().signed_duration_since(cached.fetched_at);
            if age.num_seconds() < CACHE_TTL_SECONDS {
                tracks.push((track_ref.clone(), Some(cached.track.clone())));
                continue;
            }
        }
        missing_refs.push(track_ref.clone());
        tracks.push((track_ref.clone(), None));
    }
    if !missing_refs.is_empty() {
        let fetches: Vec<_> = missing_refs
            .iter()
            .map(|track_ref| {
                let track_ref = track_ref.clone();
                let relay_hints = playlist
                    .track_hints
                    .get(track_ref.as_str())
                    .cloned()
                    .unwrap_or_default();
                async move {
                    let parts: Vec<&str> = track_ref.splitn(3, ':').collect();
                    if parts.len() < 3 {
                        return None;
                    }
                    fetch_nostr_track_by_coordinate(parts[1], parts[2], relay_hints)
                        .await
                        .ok()
                        .flatten()
                }
            })
            .collect();
        let fetched = futures::future::join_all(fetches).await;
        for (ref_key, track_opt) in &mut tracks {
            if track_opt.is_none() {
                if let Some(idx) = missing_refs.iter().position(|r| r == ref_key) {
                    *track_opt = fetched[idx].clone();
                }
            }
        }
    }
    Ok(tracks
        .into_iter()
        .filter_map(|(_, track_opt)| track_opt)
        .collect())
}
/// Publish a new music track (Kind 36787)
#[allow(clippy::too_many_arguments)]
pub async fn publish_track(
    d_tag: String,
    title: String,
    url: String,
    image: Option<String>,
    gradient: Option<String>,
    duration: Option<u32>,
    genres: Vec<String>,
    ai_generated: bool,
) -> Result<String, String> {
    let _client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !nostr_client::has_signer() {
        return Err("No signer attached".to_string());
    }
    let mut tags = vec![
        Tag::identifier(d_tag),
        Tag::custom("title", vec![title.clone()]),
        Tag::custom("url", vec![url]),
        // NIP-31: human-readable summary for clients that don't know
        // kind 36787.
        Tag::custom(
            "alt",
            vec![format!("Music track: {}", title)],
        ),
    ];
    if let Some(img) = image {
        tags.push(Tag::custom("image", vec![img]));
    }
    if let Some(grad) = gradient {
        tags.push(Tag::custom("gradient", vec![grad]));
    }
    if let Some(dur) = duration {
        tags.push(Tag::custom("duration",
            vec![dur.to_string()],
        ));
    }
    for genre in genres {
        tags.push(Tag::hashtag(genre));
    }
    if ai_generated {
        tags.push(Tag::custom("ai-generated",
            vec!["true".to_string()],
        ));
    }
    let builder = EventBuilder::new(Kind::from(KIND_MUSIC_TRACK), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("music".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok(event_id)
}
/// Publish a new music playlist (Kind 34139)
#[allow(clippy::too_many_arguments)]
pub async fn publish_playlist(
    d_tag: String,
    title: String,
    description: Option<String>,
    image: Option<String>,
    track_refs: Vec<String>,
    categories: Vec<String>,
    is_public: bool,
    is_collaborative: bool,
) -> Result<String, String> {
    let _client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !nostr_client::has_signer() {
        return Err("No signer attached".to_string());
    }
    let mut tags = vec![
        Tag::identifier(d_tag),
        Tag::custom("title", vec![title.clone()]),
        // NIP-31: human-readable summary for clients that don't know
        // kind 34139.
        Tag::custom(
            "alt",
            vec![format!("Music playlist: {}", title)],
        ),
        // Category marker used by Quartz/Amethyst's music feeds.
        Tag::hashtag("playlist"),
    ];
    if let Some(desc) = description {
        tags.push(Tag::custom("description",
            vec![desc],
        ));
    }
    if let Some(img) = image {
        tags.push(Tag::custom("image", vec![img]));
    }
    for track_ref in track_refs {
        tags.push(Tag::custom("a", vec![track_ref]));
    }
    let mut seen_categories = std::collections::HashSet::new();
    for category in categories {
        let cat = category.trim().to_lowercase();
        if cat.is_empty() || cat == "playlist" || !seen_categories.insert(cat.clone()) {
            continue;
        }
        tags.push(Tag::hashtag(cat));
    }
    // Quartz/NIP-WW rule: emit exactly ONE visibility flag so clients that
    // read either `public` or `private` agree. A private playlist without a
    // `private` tag renders as public in Quartz-based clients.
    if is_public {
        tags.push(Tag::custom("public",
            vec!["true".to_string()],
        ));
    } else {
        tags.push(Tag::custom("private",
            vec!["true".to_string()],
        ));
    }
    if is_collaborative {
        tags.push(Tag::custom("collaborative",
            vec!["true".to_string()],
        ));
    }
    let builder = EventBuilder::new(Kind::from(KIND_PLAYLIST), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("music".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;
    Ok(event_id)
}
// ---------------------------------------------------------------------------
// Playlist mutation (NIP-WW lifecycle)
//
// Modeled on the music library's republish flow and Quartz's
// `MusicPlaylistEvent.edit/addTrack/removeTrack`: re-fetch the latest event
// version by coordinate, transform tags (preserving every tag the composer
// doesn't own — including non-track `a` refs and unknown tags), re-sign and
// enqueue through the publish queue (which coalesces replaceable events).
// ---------------------------------------------------------------------------

/// Append a track `a` tag if not already present. Idempotent; preserves tag
/// order and any relay-hint third element on the appended tag.
pub fn playlist_tags_with_track_added(
    tags: &[Tag],
    coordinate: &str,
    relay_hint: Option<&str>,
) -> Vec<Tag> {
    let already = tags.iter().any(|t| {
        let slice = t.as_slice();
        slice.first().map(|s| s.as_str()) == Some("a")
            && slice.get(1).map(|s| s.as_str()) == Some(coordinate)
    });
    let mut out = tags.to_vec();
    if !already {
        let mut values = vec![coordinate.to_string()];
        if let Some(hint) = relay_hint {
            values.push(hint.to_string());
        }
        out.push(Tag::custom("a", values));
    }
    out
}

/// Remove every `a` tag matching the coordinate. Preserves all other tags.
pub fn playlist_tags_with_track_removed(tags: &[Tag], coordinate: &str) -> Vec<Tag> {
    tags.iter()
        .filter(|t| {
            let slice = t.as_slice();
            !(slice.first().map(|s| s.as_str()) == Some("a")
                && slice.get(1).map(|s| s.as_str()) == Some(coordinate))
        })
        .cloned()
        .collect()
}

/// Reset the track list to `ordered_coordinates`, preserving each track's
/// original tag (relay hints included) and every non-track tag (including
/// non-track `a` refs, in their original relative order). The tracks are
/// appended after the retained tags in the given order — Quartz `edit()`
/// semantics where the editor's order is authoritative for the track list.
pub fn playlist_tags_with_tracks_reordered(
    tags: &[Tag],
    ordered_coordinates: &[String],
) -> Vec<Tag> {
    let track_prefix = format!("{}:", KIND_MUSIC_TRACK);
    let is_track_a = |t: &Tag| {
        let slice = t.as_slice();
        slice.first().map(|s| s.as_str()) == Some("a")
            && slice
                .get(1)
                .map(|s| s.as_str())
                .map(|v| v.starts_with(&track_prefix))
                .unwrap_or(false)
    };
    // Latest original tag per coordinate (last write wins for dupes).
    let mut by_coord: HashMap<String, Tag> = HashMap::new();
    for t in tags.iter().filter(|t| is_track_a(t)) {
        if let Some(coord) = t.as_slice().get(1) {
            by_coord.insert(coord.to_string(), t.clone());
        }
    }
    // Retain every non-track tag in original order, then append the tracks
    // in the editor's order.
    let mut out: Vec<Tag> = tags.iter().filter(|t| !is_track_a(t)).cloned().collect();
    for coord in ordered_coordinates {
        if let Some(tag) = by_coord.get(coord) {
            out.push(tag.clone());
        } else {
            out.push(Tag::custom("a", vec![coord.clone()]));
        }
    }
    out
}

/// Drop all `title`, `image`, `description`, `public`, `private` and
/// `collaborative` tags, then re-add the composer-owned ones. Exactly one
/// visibility flag is re-asserted (Quartz rule). The `d` tag, track refs,
/// hashtags, and unknown tags are preserved.
#[allow(clippy::too_many_arguments)]
pub fn playlist_tags_with_metadata_replaced(
    tags: &[Tag],
    title: &str,
    description: Option<&str>,
    image: Option<&str>,
    is_public: bool,
    is_collaborative: bool,
) -> Vec<Tag> {
    const COMPOSER_OWNED: [&str; 6] =
        ["title", "description", "image", "public", "private", "collaborative"];
    let mut out: Vec<Tag> = tags
        .iter()
        .filter(|t| {
            t.as_slice()
                .first()
                .map(|s| !COMPOSER_OWNED.contains(&s.as_str()))
                .unwrap_or(true)
        })
        .cloned()
        .collect();
    out.push(Tag::custom("title", vec![title.to_string()]));
    if let Some(desc) = description {
        out.push(Tag::custom("description", vec![desc.to_string()]));
    }
    if let Some(img) = image {
        out.push(Tag::custom("image", vec![img.to_string()]));
    }
    if is_public {
        out.push(Tag::custom("public", vec!["true".to_string()]));
    } else {
        out.push(Tag::custom("private", vec!["true".to_string()]));
    }
    if is_collaborative {
        out.push(Tag::custom("collaborative", vec!["true".to_string()]));
    }
    out
}

/// Ensure a republished playlist carries the NIP-31 `alt` tag and the
/// `t: playlist` category marker (added to any position; idempotent).
pub fn playlist_tags_ensure_conventions(tags: &[Tag], title: &str) -> Vec<Tag> {
    let mut out = tags.to_vec();
    let has_alt = tags
        .iter()
        .any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("alt"));
    if !has_alt {
        out.push(Tag::custom("alt", vec![format!("Music playlist: {}", title)]));
    }
    let has_playlist_tag = tags.iter().any(|t| {
        let slice = t.as_slice();
        slice.first().map(|s| s.as_str()) == Some("t")
            && slice.get(1).map(|s| s.as_str()) == Some("playlist")
    });
    if !has_playlist_tag {
        out.push(Tag::hashtag("playlist"));
    }
    out
}

/// Fetch the user's own playlists (kind 34139, newest first).
pub async fn fetch_own_playlists() -> Result<Vec<NostrPlaylist>, String> {
    let pubkey = auth_store::get_pubkey().ok_or("Not logged in")?;
    fetch_playlists(Some(&pubkey), 100, None).await
}

/// Re-fetch the latest version of a playlist event by coordinate. Mutations
/// must operate on the freshest version so concurrent edits aren't clobbered
/// (best-effort: last-write-wins by `created_at`, matching Quartz).
async fn fetch_latest_playlist_event(
    playlist: &NostrPlaylist,
) -> Result<Event, String> {
    let event = nostr_client::fetch_event_by_coordinate_with_relays(
        playlist.kind,
        playlist.pubkey.clone(),
        playlist.d_tag.clone(),
        Vec::new(),
    )
    .await
    .map_err(|e| format!("Failed to fetch playlist: {}", e))?;
    event.ok_or_else(|| "Playlist not found on relays".to_string())
}

/// Ownership check: mutations and deletion are restricted to the author.
fn ensure_playlist_owned(playlist: &NostrPlaylist) -> Result<(), String> {
    let Some(user_pk) = auth_store::get_pubkey() else {
        return Err("Not logged in".to_string());
    };
    if user_pk != playlist.pubkey {
        return Err("You can only modify your own playlists".to_string());
    }
    if !nostr_client::has_signer() {
        return Err("No signer attached".to_string());
    }
    Ok(())
}

/// Sign and enqueue a replacement playlist event with the given tags.
/// Content is preserved from the earlier version.
async fn republish_playlist_event(earlier: &Event, tags: Vec<Tag>) -> Result<String, String> {
    let builder =
        EventBuilder::new(earlier.kind, earlier.content.to_string()).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("music".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(event_id)
}

/// Collect an event's tags into a `Vec<Tag>` without moving out of the event.
fn event_tags_vec(event: &Event) -> Vec<Tag> {
    event.tags.iter().cloned().collect()
}

/// Append a track to one of the user's playlists (idempotent).
pub async fn add_track_to_playlist(
    playlist: &NostrPlaylist,
    track: &NostrTrack,
) -> Result<String, String> {
    ensure_playlist_owned(playlist)?;
    let latest = fetch_latest_playlist_event(playlist).await?;
    let mut tags =
        playlist_tags_with_track_added(&event_tags_vec(&latest), &track.coordinate, None);
    tags = playlist_tags_ensure_conventions(&tags, &playlist.title);
    republish_playlist_event(&latest, tags).await
}

/// Remove a track from one of the user's playlists.
pub async fn remove_track_from_playlist(
    playlist: &NostrPlaylist,
    track: &NostrTrack,
) -> Result<String, String> {
    ensure_playlist_owned(playlist)?;
    let latest = fetch_latest_playlist_event(playlist).await?;
    let mut tags = playlist_tags_with_track_removed(&event_tags_vec(&latest), &track.coordinate);
    tags = playlist_tags_ensure_conventions(&tags, &playlist.title);
    republish_playlist_event(&latest, tags).await
}

/// Update playlist metadata and track order (Quartz `edit` semantics):
/// title always replaces; `None` image/description removes those tags;
/// the track list is reset to `track_refs` order; unknown tags, non-track
/// `a` refs, the `d` tag and `content` are preserved.
#[allow(clippy::too_many_arguments)]
pub async fn update_playlist(
    playlist: &NostrPlaylist,
    title: String,
    description: Option<String>,
    image: Option<String>,
    track_refs: Vec<String>,
    is_public: bool,
    is_collaborative: bool,
) -> Result<String, String> {
    ensure_playlist_owned(playlist)?;
    let latest = fetch_latest_playlist_event(playlist).await?;
    let tags = playlist_tags_with_metadata_replaced(
        &event_tags_vec(&latest),
        &title,
        description.as_deref(),
        image.as_deref(),
        is_public,
        is_collaborative,
    );
    let tags = playlist_tags_with_tracks_reordered(&tags, &track_refs);
    let tags = playlist_tags_ensure_conventions(&tags, &title);
    republish_playlist_event(&latest, tags).await
}

/// Delete a playlist via NIP-09 coordinate deletion (removes all versions).
pub async fn delete_playlist(playlist: &NostrPlaylist) -> Result<String, String> {
    ensure_playlist_owned(playlist)?;
    use nostr::nips::nip01::Coordinate;
    use nostr::nips::nip09::EventDeletionRequest;
    let author = PublicKey::from_hex(&playlist.pubkey)
        .map_err(|e| format!("Invalid author pubkey: {}", e))?;
    let coord = Coordinate::new(Kind::Custom(playlist.kind), author)
        .identifier(&playlist.d_tag);
    let builder = EventDeletionRequest::new().coordinate(coord).into_event_builder();
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign deletion: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("music".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(event_id)
}

/// Get a cached track if available and not expired
#[allow(dead_code)]
pub fn get_cached_track(coordinate: &str) -> Option<NostrTrack> {
    NOSTR_TRACK_CACHE
        .read()
        .peek(coordinate)
        .and_then(|cached| {
            let age = Utc::now().signed_duration_since(cached.fetched_at);
            if age.num_seconds() < CACHE_TTL_SECONDS {
                Some(cached.track.clone())
            } else {
                None
            }
        })
}
/// Get a cached playlist if available
#[allow(dead_code)]
pub fn get_cached_playlist(coordinate: &str) -> Option<NostrPlaylist> {
    NOSTR_PLAYLIST_CACHE.read().peek(coordinate).cloned()
}
/// Clear all caches (useful for logout)
#[allow(dead_code)]
pub fn clear_caches() {
    NOSTR_TRACK_CACHE.write().clear();
    NOSTR_PLAYLIST_CACHE.write().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_event(kind: u16, tags: Vec<Tag>, content: &str) -> Event {
        use nostr::event::FinalizeEvent;
        let keys = nostr::key::Keys::generate();
        EventBuilder::new(Kind::Custom(kind), content)
            .tags(tags)
            .finalize(&keys)
            .unwrap()
    }

    fn tag(name: &str, values: &[&str]) -> Tag {
        Tag::custom(
            name,
            values
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<String>>(),
        )
    }

    // ------------------------------------------------------------------
    // Visibility semantics (Quartz/NIP-WW: `private` authoritative)
    // ------------------------------------------------------------------

    #[test]
    fn visibility_no_tags_defaults_to_public() {
        let ev = signed_event(
            KIND_PLAYLIST,
            vec![tag("d", &["pl"]), tag("title", &["T"])],
            "",
        );
        let pl = parse_playlist_event(&ev).unwrap();
        assert!(pl.is_public);
    }

    #[test]
    fn visibility_private_tag_wins_over_public() {
        let ev = signed_event(
            KIND_PLAYLIST,
            vec![
                tag("d", &["pl"]),
                tag("title", &["T"]),
                tag("public", &["true"]),
                tag("private", &["true"]),
            ],
            "",
        );
        let pl = parse_playlist_event(&ev).unwrap();
        assert!(!pl.is_public);
    }

    #[test]
    fn visibility_private_only_is_private() {
        let ev = signed_event(
            KIND_PLAYLIST,
            vec![
                tag("d", &["pl"]),
                tag("title", &["T"]),
                tag("private", &["true"]),
            ],
            "",
        );
        let pl = parse_playlist_event(&ev).unwrap();
        assert!(!pl.is_public);
    }

    #[test]
    fn visibility_case_insensitive_and_other_values_ignored() {
        let ev = signed_event(
            KIND_PLAYLIST,
            vec![
                tag("d", &["pl"]),
                tag("title", &["T"]),
                tag("private", &["TRUE"]),
            ],
            "",
        );
        assert!(!parse_playlist_event(&ev).unwrap().is_public);
    }

    // ------------------------------------------------------------------
    // Playlist-shaped NIP-51 bookmark sets (real Plume event shape)
    // ------------------------------------------------------------------

    /// Mirrors the real kind-30003 playlist published by a Plume user
    /// (title + t:playlist + a-tags with relay hints + encrypted content).
    fn plume_shaped_bookmark_set() -> Event {
        let track = format!("{}:{}", KIND_MUSIC_TRACK, "aaaa1111aaaa");
        let track2 = format!("{}:{}", KIND_MUSIC_TRACK, "bbbb2222bbbb");
        signed_event(
            30003,
            vec![
                tag("d", &["my-playlist-oct-26"]),
                tag("title", &["My playlist Oct 26"]),
                tag("t", &["playlist"]),
                tag("a", &[&track, "wss://theforest.nostr1.com"]),
                tag("a", &[&track2, "wss://theforest.nostr1.com"]),
            ],
            // NIP-44 ciphertext-shaped blob (no whitespace, >32 chars).
            "AuPrXEa9K234dCzuoCt8TdeOhGisYBZNlfSA2Q08MSn7LhFKlvk5p5JLVV",
        )
    }

    #[test]
    fn playlist_shaped_bookmark_set_parses() {
        let ev = plume_shaped_bookmark_set();
        assert!(is_playlist_shaped_event(&ev));
        let pl = parse_playlist_event(&ev).unwrap();
        assert_eq!(pl.kind, 30003);
        assert_eq!(pl.title, "My playlist Oct 26");
        assert_eq!(pl.track_refs.len(), 2);
        // Relay hints captured from the a-tag third element.
        assert_eq!(
            pl.track_hints.get(&pl.track_refs[0]).map(|v| v.as_slice()),
            Some(&["wss://theforest.nostr1.com".to_string()][..])
        );
        // Encrypted content must NOT become the description.
        assert!(pl.description.is_none());
        // Coordinate/naddr carry the true kind.
        assert!(pl.coordinate.starts_with("30003:"));
        let naddr = pl.naddr.unwrap();
        assert!(naddr.starts_with("naddr1"));
    }

    #[test]
    fn bookmark_set_without_playlist_tag_is_not_playlist_shaped() {
        let ev = signed_event(
            30003,
            vec![
                tag("d", &["reading"]),
                tag("title", &["Reading list"]),
                tag("a", &["30023:aa:art"]),
            ],
            "",
        );
        assert!(!is_playlist_shaped_event(&ev));
        assert!(parse_playlist_event(&ev).is_err());
    }

    #[test]
    fn name_tag_fallback_for_bookmark_sets() {
        let track = format!("{}:{}", KIND_MUSIC_TRACK, "cccc3333");
        let ev = signed_event(
            30003,
            vec![
                tag("d", &["pl"]),
                tag("name", &["Named list"]),
                tag("t", &["playlist"]),
                tag("a", &[&track]),
            ],
            "",
        );
        assert_eq!(parse_playlist_event(&ev).unwrap().title, "Named list");
    }

    #[test]
    fn encrypted_content_heuristic() {
        assert!(content_looks_encrypted(
            "AuPrXEa9K234dCzuoCt8TdeOhGisYBZNlfSA2Q08MSn7"
        ));
        // Plain descriptions (whitespace, short) are not encrypted.
        assert!(!content_looks_encrypted("My favourite driving tunes"));
        assert!(!content_looks_encrypted("short"));
        assert!(!content_looks_encrypted(""));
        // NIP-04 style with ?iv= also has no whitespace.
        assert!(content_looks_encrypted(
            "ciphertextciphertextciphertextciphertext?iv=1234567890abcdef"
        ));
    }

    // ------------------------------------------------------------------
    // Tag transforms (Quartz edit/addTrack/removeTrack semantics)
    // ------------------------------------------------------------------

    fn sample_tags() -> Vec<Tag> {
        let track_a = format!("{}:pk:track-a", KIND_MUSIC_TRACK);
        let track_b = format!("{}:pk:track-b", KIND_MUSIC_TRACK);
        vec![
            tag("d", &["pl1"]),
            tag("title", &["Old Title"]),
            tag("t", &["playlist"]),
            tag("t", &["custom-genre"]),
            tag("a", &[&track_a, "wss://relay.example"]),
            tag("a", &["30023:pk:some-article"]),
            tag("a", &[&track_b]),
            tag("public", &["true"]),
            tag("mystery", &["keep-me"]),
        ]
    }

    fn tag_values(tags: &[Tag], name: &str) -> Vec<Vec<String>> {
        tags.iter()
            .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some(name))
            .map(|t| {
                t.as_slice()[1..]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn add_track_is_idempotent_and_preserves_tags() {
        let tags = sample_tags();
        let track_a = format!("{}:pk:track-a", KIND_MUSIC_TRACK);
        let out = playlist_tags_with_track_added(&tags, &track_a, None);
        assert_eq!(out.len(), tags.len(), "duplicate add must be a no-op");

        let new_track = format!("{}:pk:track-c", KIND_MUSIC_TRACK);
        let out = playlist_tags_with_track_added(&tags, &new_track, Some("wss://hint"));
        assert_eq!(out.len(), tags.len() + 1);
        let added = out.last().unwrap().as_slice();
        assert_eq!(added[1], new_track);
        assert_eq!(added[2], "wss://hint");
    }

    #[test]
    fn remove_track_only_removes_matching_a_tag() {
        let tags = sample_tags();
        let track_a = format!("{}:pk:track-a", KIND_MUSIC_TRACK);
        let track_b = format!("{}:pk:track-b", KIND_MUSIC_TRACK);
        let out = playlist_tags_with_track_removed(&tags, &track_a);
        assert_eq!(out.len(), tags.len() - 1);
        // Non-track a tag survives, track-b survives.
        assert_eq!(
            tag_values(&out, "a"),
            vec![
                vec!["30023:pk:some-article".to_string()],
                vec![track_b],
            ]
        );
    }

    #[test]
    fn reorder_preserves_non_track_refs_hints_and_unknown_tags() {
        let tags = sample_tags();
        let track_a = format!("{}:pk:track-a", KIND_MUSIC_TRACK);
        let track_b = format!("{}:pk:track-b", KIND_MUSIC_TRACK);
        let out = playlist_tags_with_tracks_reordered(&tags, &[track_b.clone(), track_a.clone()]);
        // Non-track `a` refs come first, then tracks in the given order.
        assert_eq!(
            tag_values(&out, "a"),
            vec![
                vec!["30023:pk:some-article".to_string()],
                vec![track_b],
                vec![track_a, "wss://relay.example".to_string()],
            ]
        );
        // Unknown tags and hashtags survive untouched.
        assert_eq!(tag_values(&out, "mystery"), vec![vec!["keep-me".to_string()]]);
        assert_eq!(tag_values(&out, "t").len(), 2);
    }

    #[test]
    fn metadata_replacement_reasserts_exactly_one_visibility_flag() {
        let tags = sample_tags();
        let out = playlist_tags_with_metadata_replaced(
            &tags,
            "New Title",
            Some("new desc"),
            None,
            false, // private
            true,
        );
        assert!(tag_values(&out, "public").is_empty(), "stale public flag must be dropped");
        assert_eq!(tag_values(&out, "private"), vec![vec!["true".to_string()]]);
        assert_eq!(tag_values(&out, "title"), vec![vec!["New Title".to_string()]]);
        assert_eq!(tag_values(&out, "description"), vec![vec!["new desc".to_string()]]);
        assert!(tag_values(&out, "image").is_empty());
        // Unknown tags survive.
        assert_eq!(tag_values(&out, "mystery"), vec![vec!["keep-me".to_string()]]);
        // Track a-tags untouched by metadata replacement.
        assert_eq!(tag_values(&out, "a").len(), 3);
    }

    #[test]
    fn conventions_add_alt_and_playlist_tag_once() {
        let tags = sample_tags(); // already has t:playlist, no alt
        let out = playlist_tags_ensure_conventions(&tags, "T");
        assert_eq!(tag_values(&out, "alt").len(), 1);
        assert_eq!(
            tag_values(&out, "t").iter().filter(|v| v[0] == "playlist").count(),
            1
        );
        // Idempotent.
        let again = playlist_tags_ensure_conventions(&out, "T");
        assert_eq!(tag_values(&again, "alt").len(), 1);
    }
}

