use crate::stores::{auth_store, nostr_client, profiles};
use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use lru::LruCache;
use nostr_sdk::{
    Alphabet, Event, EventBuilder, Filter, FromBech32, Kind, PublicKey, SingleLetterTag,
    Tag, TagKind,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::time::Duration;
/// Kind number for Music Track events
pub const KIND_MUSIC_TRACK: u16 = 36787;
/// Kind number for Playlist events
pub const KIND_PLAYLIST: u16 = 34139;
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
    /// Podcast episode from Nostr Kind 30054 event
    NostrPodcast {
        /// Event coordinate: "30054:pubkey:d-tag"
        coordinate: String,
        /// Author pubkey (hex)
        pubkey: String,
        /// d-tag identifier
        d_tag: String,
        /// Podcast show title
        podcast_title: String,
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
/// Parsed Kind 34139 playlist event
#[derive(Clone, Debug, PartialEq)]
pub struct NostrPlaylist {
    /// Event ID
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// d-tag identifier
    pub d_tag: String,
    /// Coordinate string: "34139:pubkey:d-tag"
    pub coordinate: String,
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
    /// Category/genre tags
    pub categories: Vec<String>,
    /// Public visibility
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
pub static NOSTR_TRACK_CACHE: GlobalSignal<LruCache<String, CachedTrack>> = Signal::global(||
LruCache::new(NonZeroUsize::new(TRACK_CACHE_CAPACITY).unwrap()));
/// Global LRU cache for nostr playlists (coordinate -> NostrPlaylist)
pub static NOSTR_PLAYLIST_CACHE: GlobalSignal<LruCache<String, NostrPlaylist>> = Signal::global(||
LruCache::new(NonZeroUsize::new(PLAYLIST_CACHE_CAPACITY).unwrap()));
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
/// Parse a Kind 34139 event into a NostrPlaylist
pub fn parse_playlist_event(event: &Event) -> Result<NostrPlaylist, String> {
    let pubkey = event.pubkey.to_hex();
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;
    let title = get_tag_value(event, "title").ok_or("Missing required 'title' tag")?;
    let coordinate = format!("{}:{}:{}", KIND_PLAYLIST, pubkey, d_tag);
    let description = get_tag_value(event, "description")
        .or_else(|| {
            if !event.content.is_empty() { Some(event.content.clone()) } else { None }
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
    let track_refs: Vec<String> = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("a"))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
        .filter(|r| r.starts_with(&format!("{}:", KIND_MUSIC_TRACK)))
        .collect();
    let categories: Vec<String> = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("t"))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
        .collect();
    let is_public = get_tag_value(event, "public").map(|v| v == "true").unwrap_or(false);
    let is_collaborative = get_tag_value(event, "collaborative")
        .map(|v| v == "true")
        .unwrap_or(false);
    Ok(NostrPlaylist {
        event_id: event.id.to_hex(),
        pubkey,
        d_tag,
        coordinate,
        title,
        description,
        image,
        gradient,
        track_refs,
        categories,
        is_public,
        is_collaborative,
        created_at: event.created_at.as_secs(),
    })
}
/// Helper to get a tag value by name
fn get_tag_value(event: &Event, tag_name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}
/// Fetch nostr tracks with optional filter and genre
pub async fn fetch_nostr_tracks(
    filter: MusicFeedFilter,
    limit: usize,
    genre: Option<&str>,
) -> Result<Vec<NostrTrack>, String> {
    let _client = nostr_client::get_client().ok_or("Client not initialized")?;
    let mut nostr_filter = Filter::new().kind(Kind::from(KIND_MUSIC_TRACK)).limit(limit);
    if let Some(g) = genre {
        nostr_filter = nostr_filter.hashtag(g.to_lowercase());
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
    let events = nostr_client::fetch_events_aggregated(
            nostr_filter,
            Duration::from_secs(10),
        )
        .await
        .map_err(|e| format!("Failed to fetch tracks: {}", e))?;
    let mut tracks: Vec<NostrTrack> = events
        .iter()
        .filter_map(|e| parse_track_event(e).ok())
        .collect();
    tracks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    for track in &tracks {
        NOSTR_TRACK_CACHE
            .write()
            .put(
                track.coordinate.clone(),
                CachedTrack {
                    track: track.clone(),
                    fetched_at: Utc::now(),
                },
            );
    }
    Ok(tracks)
}
/// Fetch a single track by coordinate
pub async fn fetch_nostr_track_by_coordinate(
    pubkey: &str,
    d_tag: &str,
) -> Result<Option<NostrTrack>, String> {
    let cache_key = format!("{}:{}:{}", KIND_MUSIC_TRACK, pubkey, d_tag);
    if let Some(cached) = NOSTR_TRACK_CACHE.read().peek(&cache_key) {
        let age = Utc::now().signed_duration_since(cached.fetched_at);
        if age.num_seconds() < CACHE_TTL_SECONDS {
            return Ok(Some(cached.track.clone()));
        }
    }
    let public_key = PublicKey::from_hex(pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::from(KIND_MUSIC_TRACK))
        .author(public_key)
        .identifier(d_tag)
        .limit(1);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5))
        .await
        .map_err(|e| format!("Failed to fetch track: {}", e))?;
    if let Some(event) = events.into_iter().next() {
        let track = parse_track_event(&event)?;
        NOSTR_TRACK_CACHE
            .write()
            .put(
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
/// Fetch zap totals for multiple tracks in one query
/// If `since_days` is provided, only counts zaps from that time period
pub async fn fetch_track_zap_totals(
    track_coordinates: Vec<String>,
    since_days: Option<u32>,
) -> Result<HashMap<String, u64>, String> {
    if track_coordinates.is_empty() {
        return Ok(HashMap::new());
    }
    let _client = nostr_client::get_client().ok_or("Client not initialized")?;
    let mut filter = Filter::new().kind(Kind::ZapReceipt);
    if let Some(days) = since_days {
        let since = nostr_sdk::Timestamp::now()
            - Duration::from_secs(days as u64 * 24 * 60 * 60);
        filter = filter.since(since);
    }
    for coord in &track_coordinates {
        filter = filter
            .custom_tag(SingleLetterTag::lowercase(Alphabet::A), coord.clone());
    }
    let zap_events = nostr_client::fetch_events_aggregated(
            filter,
            Duration::from_secs(10),
        )
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
            .and_then(|bolt11| parse_bolt11_amount(bolt11));
        if let (Some(coord), Some(msats)) = (a_tag, amount) {
            *totals.entry(coord).or_default() += msats;
        }
    }
    Ok(totals)
}
/// Parse millisats amount from bolt11 invoice
fn parse_bolt11_amount(bolt11: &str) -> Option<u64> {
    let lower = bolt11.to_lowercase();
    if !lower.starts_with("lnbc") {
        return None;
    }
    let rest = &lower[4..];
    let sep_pos = rest.find('1')?;
    let amount_str = &rest[..sep_pos];
    if amount_str.is_empty() {
        return None;
    }
    let (num_str, multiplier) = if let Some(stripped) = amount_str.strip_suffix('m') {
        (stripped, 100_000_000_u64)
    } else if let Some(stripped) = amount_str.strip_suffix('u') {
        (stripped, 100_000_u64)
    } else if let Some(stripped) = amount_str.strip_suffix('n') {
        (stripped, 100_u64)
    } else if let Some(stripped) = amount_str.strip_suffix('p') {
        (stripped, 1_u64)
    } else {
        (amount_str, 100_000_000_000_u64)
    };
    let amount: u64 = num_str.parse().ok()?;
    Some(amount * multiplier)
}
/// Search nostr music tracks by title or artist name
///
/// This performs a hybrid search:
/// 1. Fetches recent Kind 36787 tracks from relays
/// 2. Batch fetches artist profiles for the tracks
/// 3. Filters client-side by title or artist name match
pub async fn search_nostr_tracks(
    query: &str,
    limit: usize,
) -> Result<Vec<NostrTrack>, String> {
    let filter = Filter::new().kind(Kind::from(KIND_MUSIC_TRACK)).limit(limit);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch tracks: {}", e))?;
    let mut tracks: Vec<NostrTrack> = events
        .iter()
        .filter_map(|e| parse_track_event(e).ok())
        .collect();
    if query.trim().is_empty() {
        tracks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        return Ok(tracks);
    }
    let pubkeys: Vec<String> = tracks
        .iter()
        .map(|t| t.pubkey.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let profiles_map = profiles::fetch_profiles_batch(pubkeys).await.unwrap_or_default();
    let query_lower = query.to_lowercase();
    tracks
        .retain(|track| {
            if track.title.to_lowercase().contains(&query_lower) {
                return true;
            }
            if track.genres.iter().any(|g| g.to_lowercase().contains(&query_lower)) {
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
    tracks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(tracks)
}
/// Search for nostr music artists by name
/// Returns unique artists (profiles) who have published Kind 36787 tracks
pub async fn search_nostr_artists(
    query: &str,
    limit: usize,
) -> Result<Vec<(String, profiles::Profile)>, String> {
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
    let profiles_map = profiles::fetch_profiles_batch(pubkeys).await.unwrap_or_default();
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
pub async fn fetch_artist_tracks(
    pubkey: &str,
    limit: usize,
) -> Result<Vec<NostrTrack>, String> {
    let public_key = PublicKey::from_hex(pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
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
    tracks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    for track in &tracks {
        NOSTR_TRACK_CACHE
            .write()
            .put(
                track.coordinate.clone(),
                CachedTrack {
                    track: track.clone(),
                    fetched_at: Utc::now(),
                },
            );
    }
    Ok(tracks)
}
/// Fetch playlists, optionally by author
pub async fn fetch_playlists(
    author: Option<&str>,
    limit: usize,
) -> Result<Vec<NostrPlaylist>, String> {
    let mut filter = Filter::new().kind(Kind::from(KIND_PLAYLIST)).limit(limit);
    if let Some(author_pk) = author {
        let public_key = PublicKey::from_hex(author_pk)
            .or_else(|_| PublicKey::from_bech32(author_pk))
            .map_err(|e| format!("Invalid pubkey: {}", e))?;
        filter = filter.author(public_key);
    }
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch playlists: {}", e))?;
    let mut playlists: Vec<NostrPlaylist> = events
        .iter()
        .filter_map(|e| parse_playlist_event(e).ok())
        .collect();
    playlists.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    for playlist in &playlists {
        NOSTR_PLAYLIST_CACHE.write().put(playlist.coordinate.clone(), playlist.clone());
    }
    Ok(playlists)
}
/// Fetch a single playlist by author pubkey and d-tag (more efficient than fetching all and filtering)
pub async fn fetch_playlist_by_coordinate(
    author: &str,
    d_tag: &str,
) -> Result<Option<NostrPlaylist>, String> {
    let public_key = PublicKey::from_hex(author)
        .or_else(|_| PublicKey::from_bech32(author))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::from(KIND_PLAYLIST))
        .author(public_key)
        .identifier(d_tag)
        .limit(1);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5))
        .await
        .map_err(|e| format!("Failed to fetch playlist: {}", e))?;
    if let Some(event) = events.into_iter().next() {
        let playlist = parse_playlist_event(&event)?;
        NOSTR_PLAYLIST_CACHE.write().put(playlist.coordinate.clone(), playlist.clone());
        Ok(Some(playlist))
    } else {
        Ok(None)
    }
}
/// Resolve playlist tracks from track references
pub async fn resolve_playlist_tracks(
    playlist: &NostrPlaylist,
) -> Result<Vec<NostrTrack>, String> {
    if playlist.track_refs.is_empty() {
        return Ok(Vec::new());
    }
    let mut tracks = Vec::new();
    let mut missing_refs = Vec::new();
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
        for track_ref in &missing_refs {
            let parts: Vec<&str> = track_ref.splitn(3, ':').collect();
            if parts.len() >= 3 {
                let pubkey = parts[1];
                let d_tag = parts[2];
                if let Ok(Some(track)) = fetch_nostr_track_by_coordinate(pubkey, d_tag)
                    .await
                {
                    for (ref_key, track_opt) in &mut tracks {
                        if ref_key == track_ref {
                            *track_opt = Some(track.clone());
                            break;
                        }
                    }
                }
            }
        }
    }
    Ok(tracks.into_iter().filter_map(|(_, track_opt)| track_opt).collect())
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
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !nostr_client::has_signer() {
        return Err("No signer attached".to_string());
    }
    let mut tags = vec![
        Tag::identifier(d_tag),
        Tag::custom(TagKind::Custom("title".into()), vec![title]),
        Tag::custom(TagKind::Custom("url".into()), vec![url]),
    ];
    if let Some(img) = image {
        tags.push(Tag::custom(TagKind::Custom("image".into()), vec![img]));
    }
    if let Some(grad) = gradient {
        tags.push(Tag::custom(TagKind::Custom("gradient".into()), vec![grad]));
    }
    if let Some(dur) = duration {
        tags.push(
            Tag::custom(TagKind::Custom("duration".into()), vec![dur.to_string()]),
        );
    }
    for genre in genres {
        tags.push(Tag::hashtag(genre));
    }
    if ai_generated {
        tags.push(
            Tag::custom(TagKind::Custom("ai-generated".into()), vec!["true".to_string()]),
        );
    }
    let builder = EventBuilder::new(Kind::from(KIND_MUSIC_TRACK), "").tags(tags);
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish track: {}", e))?;
    Ok(output.id().to_hex())
}
/// Publish a new playlist (Kind 34139)
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
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !nostr_client::has_signer() {
        return Err("No signer attached".to_string());
    }
    let mut tags = vec![
        Tag::identifier(d_tag),
        Tag::custom(TagKind::Custom("title".into()), vec![title]),
    ];
    if let Some(desc) = description {
        tags.push(Tag::custom(TagKind::Custom("description".into()), vec![desc]));
    }
    if let Some(img) = image {
        tags.push(Tag::custom(TagKind::Custom("image".into()), vec![img]));
    }
    for track_ref in track_refs {
        tags.push(Tag::custom(TagKind::a(), vec![track_ref]));
    }
    for category in categories {
        tags.push(Tag::hashtag(category));
    }
    if is_public {
        tags.push(
            Tag::custom(TagKind::Custom("public".into()), vec!["true".to_string()]),
        );
    }
    if is_collaborative {
        tags.push(
            Tag::custom(
                TagKind::Custom("collaborative".into()),
                vec!["true".to_string()],
            ),
        );
    }
    let builder = EventBuilder::new(Kind::from(KIND_PLAYLIST), "").tags(tags);
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish playlist: {}", e))?;
    Ok(output.id().to_hex())
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
