#[cfg(feature = "mobile_platform")]
use crate::platform::android_media::{self, AndroidPlaybackSnapshot};
use crate::platform::storage;
use crate::routes::Route;
use crate::services::podcast_index::{Episode as PodcastIndexEpisode, PodcastFeed};
use crate::services::wavlake::WavlakeTrack;
use crate::stores::nostr_music::{NostrTrack, TrackSource, KIND_MUSIC_TRACK};
use crate::stores::auth_store;
use crate::utils::radio::{select_best_stream, NowPlaying, RadioStation, KIND_RADIO_STATION};
use dioxus::prelude::*;
use dioxus_stores::Store;
use nostr_sdk::nips::nip01::Coordinate;
use nostr_sdk::nips::nip38::{LiveStatus, StatusType};
use nostr_sdk::{EventBuilder, Kind, Tag, TagKind, Timestamp};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
/// Kind number for Music Vote events (addressable, one per user)
pub const KIND_MUSIC_VOTE: u16 = 33169;
/// Music track for the player (unified for Wavlake, Nostr music, and Podcasts)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MusicTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub media_url: String,
    pub album_art_url: Option<String>,
    pub artist_art_url: Option<String>,
    pub duration: Option<u32>,
    pub artist_id: Option<String>,
    pub album_id: Option<String>,
    pub artist_npub: Option<String>,
    /// Track source for routing zaps and links
    #[serde(default)]
    pub source: TrackSource,
    /// Total millisats earned (for unified hotness ranking)
    pub msat_total: Option<u64>,
    /// Created timestamp (for chronological sorting)
    pub created_at: Option<u64>,
    /// Whether this is a podcast episode (affects player UI controls)
    #[serde(default)]
    pub is_podcast: bool,
    /// Whether this is a live stream (no seeking, no duration, no progress bar)
    #[serde(default)]
    pub is_live_stream: bool,
    /// V4V payment info (for RSS podcasts with podcast:value tag)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_block: Option<crate::utils::podcast::ValueBlock>,
    /// Chapters URL (for RSS podcasts with podcast:chapters tag)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapters_url: Option<String>,
    /// Transcripts (for accessibility)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transcripts: Vec<crate::utils::podcast::TranscriptRef>,
}
impl From<WavlakeTrack> for MusicTrack {
    fn from(track: WavlakeTrack) -> Self {
        let msat_total = track.msat_total.parse::<u64>().ok();
        Self {
            id: track.id.clone(),
            title: track.title,
            artist: track.artist,
            album: Some(track.album_title),
            media_url: track.media_url,
            album_art_url: Some(track.album_art_url),
            artist_art_url: Some(track.artist_art_url),
            duration: Some(track.duration),
            artist_id: Some(track.artist_id.clone()),
            album_id: Some(track.album_id.clone()),
            artist_npub: track.artist_npub,
            source: TrackSource::Wavlake {
                artist_id: track.artist_id,
                album_id: track.album_id,
            },
            msat_total,
            created_at: None,
            is_podcast: false,
            is_live_stream: false,
            value_block: None,
            chapters_url: None,
            transcripts: Vec::new(),
        }
    }
}
impl From<NostrTrack> for MusicTrack {
    fn from(track: NostrTrack) -> Self {
        Self {
            id: track.event_id.clone(),
            title: track.title,
            artist: String::new(),
            album: None,
            media_url: track.url,
            album_art_url: track.image.clone(),
            artist_art_url: track.image,
            duration: track.duration,
            artist_id: None,
            album_id: None,
            artist_npub: Some(track.pubkey.clone()),
            source: TrackSource::Nostr {
                coordinate: track.coordinate,
                pubkey: track.pubkey,
                d_tag: track.d_tag,
            },
            msat_total: None,
            created_at: Some(track.created_at),
            is_podcast: false,
            is_live_stream: false,
            value_block: None,
            chapters_url: None,
            transcripts: Vec::new(),
        }
    }
}
impl From<RadioStation> for MusicTrack {
    fn from(station: RadioStation) -> Self {
        let best_stream = select_best_stream(&station.streams);
        let media_url = best_stream.map(|s| s.url.clone()).unwrap_or_default();
        Self {
            id: station.coordinate.clone(),
            title: station.name.clone(),
            artist: station.name.clone(),
            album: None,
            media_url,
            album_art_url: station.thumbnail.clone(),
            artist_art_url: station.thumbnail,
            duration: None,
            artist_id: None,
            album_id: None,
            artist_npub: Some(station.pubkey.clone()),
            source: TrackSource::Radio {
                coordinate: station.coordinate,
                pubkey: station.pubkey,
                d_tag: station.d_tag,
                station_name: station.name,
            },
            msat_total: None,
            created_at: Some(station.created_at),
            is_podcast: false,
            is_live_stream: true,
            value_block: None,
            chapters_url: None,
            transcripts: Vec::new(),
        }
    }
}
impl MusicTrack {
    /// Get route to the podcast show page (for podcasts) or artist page (for music)
    pub fn get_show_route(&self) -> Option<Route> {
        match &self.source {
            TrackSource::NostrPodcast { pubkey, .. } => {
                use nostr::prelude::*;
                if let Ok(pk) = PublicKey::from_hex(pubkey) {
                    let coord = nostr::nips::nip01::Coordinate::new(nostr::Kind::from(30078), pk)
                        .identifier("podcast-metadata");
                    let nip19_coord = nostr::nips::nip19::Nip19Coordinate::new(coord, vec![]);
                    if let Ok(naddr) = nip19_coord.to_bech32() {
                        return Some(Route::PodcastNostrDetail { naddr });
                    }
                }
                None
            }
            TrackSource::RssPodcast { podcast_id, .. } => {
                podcast_id.map(|id| Route::PodcastRssFeedDetail {
                    podcast_id: id.to_string(),
                })
            }
            TrackSource::Wavlake { artist_id, .. } => Some(Route::MusicArtist {
                artist_id: artist_id.clone(),
            }),
            TrackSource::RssMusic { artist, .. } => artist.as_ref().map(|a| Route::MusicRssArtist {
                artist: a.clone(),
            }),
            _ => None,
        }
    }
    /// Get route to the episode/track detail page
    pub fn get_episode_route(&self) -> Option<Route> {
        match &self.source {
            TrackSource::NostrPodcast { pubkey, d_tag, .. } => {
                use nostr::prelude::*;
                if let Ok(pk) = PublicKey::from_hex(pubkey) {
                    let coord = nostr::nips::nip01::Coordinate::new(nostr::Kind::from(30054), pk)
                        .identifier(d_tag);
                    let nip19_coord = nostr::nips::nip19::Nip19Coordinate::new(coord, vec![]);
                    if let Ok(naddr) = nip19_coord.to_bech32() {
                        return Some(Route::PodcastNostrEpisodeDetail { naddr });
                    }
                }
                None
            }
            TrackSource::RssPodcast {
                podcast_id,
                episode_guid,
                ..
            } => podcast_id.map(|id| Route::PodcastRssEpisodeDetail {
                podcast_id: id.to_string(),
                episode_id: urlencoding::encode(episode_guid).to_string(),
            }),
            _ => None,
        }
    }
    /// Get route to the track detail page (for music tracks)
    pub fn get_track_route(&self) -> Option<Route> {
        match &self.source {
            TrackSource::Wavlake { .. } => Some(Route::MusicTrackDetail {
                track_id: self.id.clone(),
            }),
            TrackSource::Nostr { pubkey, d_tag, .. } => {
                use nostr::prelude::*;
                if let Ok(pk) = PublicKey::from_hex(pubkey) {
                    let coord = nostr::nips::nip01::Coordinate::new(
                        nostr::Kind::from(KIND_MUSIC_TRACK),
                        pk,
                    )
                    .identifier(d_tag);
                    let nip19_coord = nostr::nips::nip19::Nip19Coordinate::new(coord, vec![]);
                    if let Ok(naddr) = nip19_coord.to_bech32() {
                        return Some(Route::MusicTrackDetail { track_id: naddr });
                    }
                }
                None
            }
            TrackSource::RssMusic {
                feed_id,
                episode_id,
                ..
            } => Some(Route::MusicTrackDetail {
                track_id: format!("rss:{}:{}", feed_id, episode_id),
            }),
            TrackSource::Bible {
                translation,
                book,
                chapter,
                ..
            } => Some(Route::BibleChapter {
                translation: translation.clone(),
                book: book.clone(),
                chapter: *chapter,
            }),
            _ => None,
        }
    }
    pub fn share_url(&self) -> String {
        match &self.source {
            TrackSource::Wavlake { .. } => {
                format!("https://nostr.blue/music/track/{}", self.id)
            }
            TrackSource::Nostr { pubkey, d_tag, .. } => {
                Self::build_naddr_url(KIND_MUSIC_TRACK, pubkey, d_tag)
                    .map(|n| format!("https://nostr.blue/music/track/{}", n))
                    .unwrap_or_else(|| format!("https://nostr.blue/music/track/{}", self.id))
            }
            TrackSource::NostrPodcast { pubkey, d_tag, .. } => {
                Self::build_naddr_url(30054, pubkey, d_tag)
                    .map(|n| format!("https://nostr.blue/podcast/nostr/episode/{}", n))
                    .unwrap_or_else(|| format!("https://nostr.blue/podcast/nostr/episode/{}", self.id))
            }
            TrackSource::RssPodcast {
                podcast_id,
                episode_guid,
                feed_url,
                ..
            } => {
                let id_str = podcast_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| feed_url.clone());
                format!(
                    "https://nostr.blue/podcast/rss/episode/{}/{}",
                    urlencoding::encode(&id_str),
                    urlencoding::encode(episode_guid),
                )
            }
            TrackSource::RssMusic {
                feed_id, episode_id, ..
            } => {
                format!(
                    "https://nostr.blue/music/track/rss:{}:{}",
                    feed_id, episode_id,
                )
            }
            TrackSource::Radio { pubkey, d_tag, .. } => {
                Self::build_naddr_url(KIND_RADIO_STATION, pubkey, d_tag)
                    .map(|n| format!("https://nostr.blue/radio/{}", n))
                    .unwrap_or_else(|| format!("https://nostr.blue/radio/{}", self.id))
            }
            TrackSource::Bible {
                translation,
                book,
                chapter,
                ..
            } => {
                format!(
                    "https://nostr.blue/bible/{}/{}/{}",
                    translation, book, chapter
                )
            }
        }
    }
    fn build_naddr_url(kind: u16, pubkey: &str, d_tag: &str) -> Option<String> {
        let pk = nostr_sdk::PublicKey::from_hex(pubkey).ok()?;
        let coord = Coordinate::new(Kind::Custom(kind), pk).identifier(d_tag);
        use nostr_sdk::nips::nip19::ToBech32;
        coord.to_bech32().ok()
    }
    fn resolve_image_url<'a>(
        episode: &'a PodcastIndexEpisode,
        feed: &'a PodcastFeed,
        chart_image: Option<&'a str>,
    ) -> Option<&'a str> {
        episode
            .image
            .as_deref()
            .filter(|s| !s.is_empty())
            .or(chart_image.filter(|s| !s.is_empty()))
            .or(episode.feed_image.as_deref().filter(|s| !s.is_empty()))
            .or(feed.get_image())
    }

    /// Create MusicTrack from RSS music album track (Podcast Index medium="music")
    pub fn from_rss_music_track(
        episode: &PodcastIndexEpisode,
        feed: &PodcastFeed,
        chart_image: Option<&str>,
    ) -> Self {
        let value_block = episode.value.as_ref().or(feed.value.as_ref()).map(|v| {
            let model = v.model.as_ref();
            crate::utils::podcast::ValueBlock {
                value_type: model
                    .and_then(|m| m.model_type.clone())
                    .unwrap_or_else(|| "lightning".to_string()),
                method: model
                    .and_then(|m| m.method.clone())
                    .unwrap_or_else(|| "keysend".to_string()),
                suggested: model
                    .and_then(|m| m.suggested.as_ref())
                    .and_then(|s| s.parse().ok()),
                recipients: v
                    .destinations
                    .iter()
                    .map(|d| crate::utils::podcast::ValueRecipient {
                        name: d.name.clone(),
                        recipient_type: d.dest_type.clone().unwrap_or_else(|| "node".to_string()),
                        address: d.address.clone().unwrap_or_default(),
                        custom_key: None,
                        custom_value: None,
                        split: d.split.unwrap_or(100),
                        fee: None,
                    })
                    .collect(),
            }
        });
        Self {
            id: episode.id.to_string(),
            title: episode.title.clone(),
            artist: feed.author.clone().unwrap_or_else(|| feed.title.clone()),
            album: Some(feed.title.clone()),
            media_url: episode.enclosure_url.clone().unwrap_or_default(),
            album_art_url: Self::resolve_image_url(episode, feed, chart_image).map(String::from),
            artist_art_url: feed.get_image().filter(|s| !s.is_empty()).map(String::from),
            duration: episode.duration.map(|d| d as u32),
            artist_id: None,
            album_id: None,
            artist_npub: None,
            source: TrackSource::RssMusic {
                feed_id: feed.id,
                feed_url: feed.url.clone(),
                episode_id: episode.id,
                album_title: feed.title.clone(),
                artist: feed.author.clone(),
            },
            msat_total: None,
            created_at: episode.date_published.map(|d| d as u64),
            is_podcast: false,
            is_live_stream: false,
            value_block,
            chapters_url: episode.chapters_url.clone(),
            transcripts: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum PlayerViewMode {
    #[default]
    Bar,
    Expanded,
    Floating,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoopMode {
    #[default]
    None,
    Queue,
    Track,
}

impl LoopMode {
    pub fn next(self) -> Self {
        match self {
            LoopMode::None => LoopMode::Queue,
            LoopMode::Queue => LoopMode::Track,
            LoopMode::Track => LoopMode::None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Store)]
pub struct MusicPlayerState {
    pub current_track: Option<MusicTrack>,
    pub playlist: Vec<MusicTrack>,
    pub current_index: usize,
    pub is_playing: bool,
    pub is_visible: bool,
    pub volume: f64,
    pub is_muted: bool,
    pub current_time: f64,
    pub duration: f64,
    #[serde(skip)]
    pub show_zap_dialog: bool,
    #[serde(skip)]
    pub zap_track: Option<MusicTrack>,
    /// Playback speed for podcasts (0.5x to 3.0x)
    #[serde(default = "default_playback_speed")]
    pub playback_speed: f64,
    /// Error message if stream playback failed
    #[serde(skip)]
    pub playback_error: Option<String>,
    /// Whether stream is buffering
    #[serde(skip)]
    pub is_buffering: bool,
    /// Current stream index for fallback logic
    #[serde(skip)]
    pub current_stream_index: usize,
    /// All available streams for current track (for fallback)
    #[serde(skip)]
    pub available_streams: Vec<String>,
    /// Now playing metadata from HLS ID3 tags (for radio streams)
    #[serde(skip)]
    pub now_playing: Option<NowPlaying>,
    /// If true, stop playback when reaching the end of the playlist instead of wrapping
    #[serde(default)]
    pub stop_at_end: bool,
    /// Repeat mode: None (stop at end / wrap), Queue (loop all), Track (loop one)
    #[serde(default)]
    pub loop_mode: LoopMode,
    /// Whether shuffle is enabled
    #[serde(default)]
    pub shuffle_enabled: bool,
    /// Shuffle permutation order (maps logical position -> actual playlist index)
    #[serde(default)]
    pub shuffle_order: Vec<usize>,
    /// Current player view mode (bar / expanded / floating)
    #[serde(skip)]
    pub view_mode: PlayerViewMode,
    /// Position of the floating player pill (x, y) in viewport coordinates
    #[serde(skip)]
    pub floating_pos: (f64, f64),
}
fn default_playback_speed() -> f64 {
    1.0
}
impl Default for MusicPlayerState {
    fn default() -> Self {
        Self {
            current_track: None,
            playlist: Vec::new(),
            current_index: 0,
            is_playing: false,
            is_visible: false,
            volume: 1.0,
            is_muted: false,
            current_time: 0.0,
            duration: 0.0,
            show_zap_dialog: false,
            zap_track: None,
            playback_speed: 1.0,
            playback_error: None,
            is_buffering: false,
            current_stream_index: 0,
            available_streams: Vec::new(),
            now_playing: None,
            stop_at_end: false,
            loop_mode: LoopMode::None,
            shuffle_enabled: false,
            shuffle_order: Vec::new(),
            view_mode: PlayerViewMode::Bar,
            floating_pos: (16.0, 16.0),
        }
    }
}
/// Global music player state
pub static MUSIC_PLAYER: GlobalStore<MusicPlayerState> = Global::new(MusicPlayerState::default);
const STORAGE_KEY_VOLUME: &str = "music_player_volume";
const STORAGE_KEY_MUTED: &str = "music_player_muted";
const STORAGE_KEY_PLAYBACK_SPEED: &str = "music_player_playback_speed";
static VOLUME_PERSIST_GEN: AtomicU64 = AtomicU64::new(0);
static QUEUE_SAVE_GEN: AtomicU64 = AtomicU64::new(0);
const PROGRESS_STEP_SECS: u64 = 5;
/// Initialize music player from localStorage
pub fn init_player() {
    let mut state = MUSIC_PLAYER.write();
    if let Ok(volume) = storage::get::<f64>(STORAGE_KEY_VOLUME) {
        state.volume = volume.clamp(0.0, 1.0);
    }
    if let Ok(is_muted) = storage::get::<bool>(STORAGE_KEY_MUTED) {
        state.is_muted = is_muted;
    }
    if let Ok(speed) = storage::get::<f64>(STORAGE_KEY_PLAYBACK_SPEED) {
        state.playback_speed = speed.clamp(0.5, 3.0);
    }
    if let Some(queue) = super::queue_state::load_queue() {
        if !queue.playlist.is_empty() {
            let idx = queue.current_index.min(queue.playlist.len() - 1);
            state.playlist = queue.playlist;
            state.current_index = idx;
            state.current_track = state.playlist.get(idx).cloned();
            state.current_time = queue.progress_secs as f64;
            state.is_visible = true;
            state.loop_mode = queue.loop_mode;
            state.shuffle_enabled = queue.shuffle_enabled;
            if state.shuffle_enabled {
                state.shuffle_order = rebuild_shuffle_order(state.playlist.len(), idx);
            }
            log::info!(
                "Restored queue: {} tracks, index {}, position {}s, loop {:?}, shuffle {}",
                state.playlist.len(),
                idx,
                queue.progress_secs,
                state.loop_mode,
                state.shuffle_enabled
            );
        }
    }
    log::info!("Music player initialized");
}
/// Publish NIP-38 music status (Kind 30315)
async fn publish_music_status(track: &MusicTrack) {
    if matches!(track.source, TrackSource::Bible { .. }) {
        return;
    }
    if !auth_store::is_authenticated() {
        return;
    }
    let content = format!("{} - {}", track.title, track.artist);
    let track_reference = match &track.source {
        TrackSource::Wavlake { .. } => {
            format!("https://nostr.blue/music/track/{}", track.id)
        }
        TrackSource::Nostr { pubkey, d_tag, .. } => {
            use nostr::prelude::*;
            if let Ok(pk) = PublicKey::from_hex(pubkey) {
                let coord = nostr::nips::nip01::Coordinate::new(
                    nostr::Kind::from(crate::stores::nostr_music::KIND_MUSIC_TRACK),
                    pk,
                )
                .identifier(d_tag);
                let nip19_coord = nostr::nips::nip19::Nip19Coordinate::new(coord, vec![]);
                if let Ok(naddr) = nip19_coord.to_bech32() {
                    format!("nostr:{}", naddr)
                } else {
                    format!(
                        "https://nostr.blue/music/playlist/{}:{}:{}",
                        crate::stores::nostr_music::KIND_MUSIC_TRACK,
                        pubkey,
                        d_tag,
                    )
                }
            } else {
                format!(
                    "https://nostr.blue/music/playlist/{}:{}:{}",
                    crate::stores::nostr_music::KIND_MUSIC_TRACK,
                    pubkey,
                    d_tag,
                )
            }
        }
        TrackSource::NostrPodcast { pubkey, d_tag, .. } => {
            use nostr::prelude::*;
            if let Ok(pk) = PublicKey::from_hex(pubkey) {
                let coord = nostr::nips::nip01::Coordinate::new(nostr::Kind::from(30054), pk)
                    .identifier(d_tag);
                let nip19_coord = nostr::nips::nip19::Nip19Coordinate::new(coord, vec![]);
                if let Ok(naddr) = nip19_coord.to_bech32() {
                    format!("https://nostr.blue/podcast/nostr/episode/{}", naddr)
                } else {
                    format!(
                        "https://nostr.blue/podcast/nostr/episode/30054:{}:{}",
                        pubkey, d_tag,
                    )
                }
            } else {
                format!(
                    "https://nostr.blue/podcast/nostr/episode/30054:{}:{}",
                    pubkey, d_tag,
                )
            }
        }
        TrackSource::RssPodcast {
            podcast_id,
            feed_url,
            episode_guid,
            ..
        } => match podcast_id {
            Some(id) => format!(
                "https://nostr.blue/podcast/rss/episode/{}/{}",
                id,
                urlencoding::encode(episode_guid),
            ),
            None => format!(
                "https://nostr.blue/podcast/rss/episode/{}/{}",
                urlencoding::encode(feed_url),
                urlencoding::encode(episode_guid),
            ),
        },
        TrackSource::RssMusic { feed_id, .. } => {
            format!("https://nostr.blue/music/rss/album/{}", feed_id)
        }
        TrackSource::Radio { d_tag, .. } => {
            format!("https://nostr.blue/radio/{}", urlencoding::encode(d_tag))
        }
        TrackSource::Bible { .. } => String::new(),
    };
    let mut status = LiveStatus {
        status_type: StatusType::Music,
        expiration: None,
        reference: Some(track_reference),
    };
    if let Some(duration) = track.duration {
        if duration > 0 {
            status.expiration =
                Some(Timestamp::now() + std::time::Duration::from_secs(duration as u64));
        }
    }
    let builder = EventBuilder::live_status(status, content);
    match crate::stores::publish_queue::signing::sign_event_builder(builder).await {
        Ok(event) => {
            let event_id = event.id.to_hex();
            crate::stores::publish_queue::enqueue(
                event,
                crate::stores::publish_queue::types::QueueEventType::Other("music".to_string()),
                None,
                std::collections::HashMap::new(),
            ).await;
            log::info!(
                "Music status enqueued: {} (event: {})",
                track.title,
                event_id
            );
        }
        Err(e) => {
            log::error!("Failed to sign music status: {}", e);
        }
    }
}
/// Clear music status (publish empty status)
async fn clear_music_status() {
    if !auth_store::is_authenticated() {
        return;
    }
    let status = LiveStatus::new(StatusType::Music);
    let builder = EventBuilder::live_status(status, "");
    match crate::stores::publish_queue::signing::sign_event_builder(builder).await {
        Ok(event) => {
            crate::stores::publish_queue::enqueue(
                event,
                crate::stores::publish_queue::types::QueueEventType::Other("music".to_string()),
                None,
                std::collections::HashMap::new(),
            ).await;
            log::info!("Music status clear enqueued");
        }
        Err(e) => {
            log::error!("Failed to sign music status clear: {}", e);
        }
    }
}

pub fn mark_queue_dirty() {
    QUEUE_SAVE_GEN.fetch_add(1, Ordering::SeqCst);
}

fn build_queue_snapshot() -> Option<super::queue_state::PersistedQueueState> {
    let state = MUSIC_PLAYER.read();
    if state.playlist.is_empty() {
        return None;
    }
    let idx = state.current_index.min(state.playlist.len() - 1);
    let progress = if state.is_playing {
        let p = state.current_time as u64;
        p - (p % PROGRESS_STEP_SECS)
    } else {
        state.current_time as u64
    };
    Some(super::queue_state::PersistedQueueState {
        version: 2,
        playlist: state.playlist.iter().take(200).cloned().collect(),
        current_index: idx,
        progress_secs: progress,
        loop_mode: state.loop_mode,
        shuffle_enabled: state.shuffle_enabled,
    })
}

pub fn persist_queue_snapshot() {
    if let Some(snapshot) = build_queue_snapshot() {
        super::queue_state::save_queue(&snapshot);
    } else {
        super::queue_state::clear_queue();
    }
}

pub fn queue_save_gen() -> u64 {
    QUEUE_SAVE_GEN.load(Ordering::SeqCst)
}

/// Play a track
pub fn play_track(
    track: MusicTrack,
    playlist: Option<Vec<MusicTrack>>,
    index_override: Option<usize>,
) {
    let mut state = MUSIC_PLAYER.write();
    let playlist = playlist.unwrap_or_else(|| vec![track.clone()]);
    let index = index_override
        .unwrap_or_else(|| playlist.iter().position(|t| t.id == track.id).unwrap_or(0));
    state.current_track = Some(track.clone());
    state.playlist = playlist;
    state.current_index = index;
    state.is_playing = true;
    state.is_visible = true;
    state.view_mode = PlayerViewMode::Bar;
    state.current_time = 0.0;
    state.now_playing = None;
    if state.shuffle_enabled {
        state.shuffle_order = rebuild_shuffle_order(state.playlist.len(), index);
    }
    log::info!("Playing track: {}", track.title);
    #[cfg(feature = "mobile_platform")]
    if let Err(e) = android_media::set_queue(&state.playlist, state.current_index, true) {
        log::error!("Failed to start native Android playback queue: {}", e);
        state.playback_error = Some(format!("Android playback failed: {}", e));
    }
    #[cfg(feature = "mobile_platform")]
    {
        let recent_tracks: Vec<MusicTrack> = state.playlist.iter().take(21).cloned().collect();
        let track_id = track.id.clone();
        let track_json = serde_json::to_string(&track).unwrap_or_default();
        let recent_json = serde_json::to_string(&recent_tracks).unwrap_or_default();
        let item_key = format!("item:{}", track.id);
        spawn(async move {
            for t in &recent_tracks {
                let _ = android_media::save_browse_cache(
                    &format!("item:{}", t.id),
                    &serde_json::to_string(t).unwrap_or_default(),
                );
            }
            let _ = android_media::save_browse_cache("continue_listening", &recent_json);
            let _ = android_media::save_browse_position(&track_id, 0);
            let _ = android_media::save_browse_cache(&item_key, &track_json);
        });
    }
    let ms_track = track.clone();
    {
        let title = ms_track.title.clone();
        let artist = ms_track.artist.clone();
        let album = ms_track.album.clone().unwrap_or_default();
        let artwork = ms_track.album_art_url.clone();
        let duration = ms_track.duration.unwrap_or(0) as f64;
        crate::platform::mpris::update_now_playing(
            &title, &artist, &album, duration, 0.0, true, artwork.as_deref(),
        );
    }
    spawn(async move {
        publish_music_status(&track).await;
    });
    spawn(async move {
        crate::utils::media_session::update_metadata(
            &ms_track.title,
            &ms_track.artist,
            ms_track.album.as_deref().unwrap_or(""),
            ms_track.album_art_url.as_deref(),
        )
        .await;
        crate::utils::media_session::set_playback_state(true).await;
    });
    mark_queue_dirty();
}

pub fn play_or_toggle_track(
    track: MusicTrack,
    playlist: Option<Vec<MusicTrack>>,
    index_override: Option<usize>,
) {
    let should_toggle = {
        let state = MUSIC_PLAYER.read();
        state
            .current_track
            .as_ref()
            .map(|current| current.id == track.id)
            .unwrap_or(false)
    };
    if should_toggle {
        toggle_play();
    } else {
        play_track(track, playlist, index_override);
    }
}

pub fn append_to_playlist(tracks: Vec<MusicTrack>) {
    let mut state = MUSIC_PLAYER.write();
    state.playlist.extend(tracks);
    #[cfg(feature = "mobile_platform")]
    if !state.playlist.is_empty() {
        let _ = android_media::set_queue(&state.playlist, state.current_index, state.is_playing);
    }
}
/// Toggle play/pause
pub fn toggle_play() {
    let mut state = MUSIC_PLAYER.write();
    state.is_playing = !state.is_playing;
    #[cfg(feature = "mobile_platform")]
    {
        let result = if state.is_playing {
            android_media::play()
        } else {
            android_media::pause()
        };
        if let Err(e) = result {
            log::error!("Failed to toggle native Android playback: {}", e);
            state.playback_error = Some(format!("Android playback failed: {}", e));
        }
    }
    if !state.is_playing {
        spawn(async move {
            clear_music_status().await;
        });
        spawn(async move {
            crate::utils::media_session::set_playback_state(false).await;
        });
        crate::platform::mpris::update_now_playing("", "", "", 0.0, 0.0, false, None);
    } else if let Some(track) = state.current_track.clone() {
        crate::platform::mpris::update_now_playing(
            &track.title, &track.artist,
            track.album.as_deref().unwrap_or(""),
            track.duration.unwrap_or(0) as f64, 0.0, true,
            track.album_art_url.as_deref(),
        );
        spawn(async move {
            publish_music_status(&track).await;
        });
        spawn(async move {
            crate::utils::media_session::set_playback_state(true).await;
        });
    }
    mark_queue_dirty();
}
/// Play next track in playlist
pub fn next_track() {
    let mut state = MUSIC_PLAYER.write();
    if state.playlist.is_empty() {
        return;
    }
    match state.loop_mode {
        LoopMode::Track => {
            state.current_time = 0.0;
            if let Some(track) = state.current_track.clone() {
                spawn(async move {
                    publish_music_status(&track).await;
                });
            }
            mark_queue_dirty();
            return;
        }
        LoopMode::None if state.stop_at_end && state.current_index >= state.playlist.len() - 1 => {
            state.is_playing = false;
            state.is_visible = false;
            state.current_track = None;
            #[cfg(feature = "mobile_platform")]
            let _ = android_media::stop();
            mark_queue_dirty();
            return;
        }
        LoopMode::None | LoopMode::Queue => {}
    }
    let next_idx = if state.shuffle_enabled && !state.shuffle_order.is_empty() {
        let logical = state
            .shuffle_order
            .iter()
            .position(|&i| i == state.current_index)
            .unwrap_or(0);
        let next_logical = (logical + 1) % state.shuffle_order.len();
        if next_logical == 0 && state.loop_mode == LoopMode::None && !state.stop_at_end {
            if let Some(&actual) = state.shuffle_order.get(next_logical) {
                state.current_index = actual;
            }
        } else if let Some(&actual) = state.shuffle_order.get(next_logical) {
            state.current_index = actual;
        } else {
            state.current_index = (state.current_index + 1) % state.playlist.len();
        }
        state.current_index
    } else if state.loop_mode == LoopMode::Queue
        || !state.stop_at_end
        || state.current_index < state.playlist.len() - 1
    {
        state.current_index = (state.current_index + 1) % state.playlist.len();
        state.current_index
    } else {
        mark_queue_dirty();
        return;
    };
    state.current_track = state.playlist.get(next_idx).cloned();
    state.is_playing = true;
    state.current_time = 0.0;
    if let Some(track) = state.current_track.clone() {
        log::info!("Next track: {}", track.title);
        #[cfg(feature = "mobile_platform")]
        if let Err(e) = android_media::next_track() {
            log::error!("Failed to skip to next native Android track: {}", e);
            state.playback_error = Some(format!("Android playback failed: {}", e));
        }
        crate::platform::mpris::update_now_playing(
            &track.title, &track.artist,
            track.album.as_deref().unwrap_or(""),
            track.duration.unwrap_or(0) as f64, 0.0, true,
            track.album_art_url.as_deref(),
        );
        let t = track.clone();
        spawn(async move {
            publish_music_status(&t).await;
        });
        spawn(async move {
            crate::utils::media_session::update_metadata(
                &track.title,
                &track.artist,
                track.album.as_deref().unwrap_or(""),
                track.album_art_url.as_deref(),
            )
            .await;
            crate::utils::media_session::set_playback_state(true).await;
        });
    }
    mark_queue_dirty();
}
/// Play previous track in playlist
pub fn previous_track() {
    let mut state = MUSIC_PLAYER.write();
    if state.playlist.is_empty() {
        return;
    }
    if state.current_time > 3.0 {
        state.current_time = 0.0;
        #[cfg(feature = "mobile_platform")]
        if let Err(e) = android_media::seek_to(0.0) {
            log::error!("Failed to rewind native Android track: {}", e);
            state.playback_error = Some(format!("Android playback failed: {}", e));
        }
        if let Some(track) = state.current_track.clone() {
            spawn(async move {
                publish_music_status(&track).await;
            });
        }
        return;
    }
    if state.loop_mode == LoopMode::Track {
        state.current_time = 0.0;
        if let Some(track) = state.current_track.clone() {
            spawn(async move {
                publish_music_status(&track).await;
            });
        }
        mark_queue_dirty();
        return;
    }
    state.current_index = if state.shuffle_enabled && !state.shuffle_order.is_empty() {
        let logical = state
            .shuffle_order
            .iter()
            .position(|&i| i == state.current_index)
            .unwrap_or(0);
        if logical == 0 {
            if state.loop_mode == LoopMode::Queue {
                *state.shuffle_order.last().unwrap_or(&0)
            } else {
                state.current_index
            }
        } else {
            state.shuffle_order[logical - 1]
        }
    } else if state.current_index == 0 {
        state.playlist.len() - 1
    } else {
        state.current_index - 1
    };
    state.current_track = state.playlist.get(state.current_index).cloned();
    state.is_playing = true;
    state.current_time = 0.0;
    if let Some(track) = state.current_track.clone() {
        log::info!("Previous track: {}", track.title);
        #[cfg(feature = "mobile_platform")]
        if let Err(e) = android_media::previous_track() {
            log::error!("Failed to skip to previous native Android track: {}", e);
            state.playback_error = Some(format!("Android playback failed: {}", e));
        }
        crate::platform::mpris::update_now_playing(
            &track.title, &track.artist,
            track.album.as_deref().unwrap_or(""),
            track.duration.unwrap_or(0) as f64, 0.0, true,
            track.album_art_url.as_deref(),
        );
        let t = track.clone();
        spawn(async move {
            publish_music_status(&t).await;
        });
        spawn(async move {
            crate::utils::media_session::update_metadata(
                &track.title,
                &track.artist,
                track.album.as_deref().unwrap_or(""),
                track.album_art_url.as_deref(),
            )
            .await;
            crate::utils::media_session::set_playback_state(true).await;
        });
    }
    mark_queue_dirty();
}
/// Set volume (0.0 - 1.0)
pub fn set_volume(volume: f64) {
    let clamped = volume.clamp(0.0, 1.0);
    {
        let mut state = MUSIC_PLAYER.write();
        state.volume = clamped;
        #[cfg(feature = "mobile_platform")]
        if let Err(e) = android_media::set_volume(if state.is_muted { 0.0 } else { clamped }) {
            log::error!("Failed to set native Android volume: {}", e);
            state.playback_error = Some(format!("Android playback failed: {}", e));
        }
    }
    let gen = VOLUME_PERSIST_GEN
        .fetch_add(1, Ordering::SeqCst)
        .wrapping_add(1);
    crate::platform::spawn::spawn_detached(async move {
        crate::platform::timer::sleep_ms(300).await;
        if VOLUME_PERSIST_GEN.load(Ordering::SeqCst) != gen {
            return;
        }
        storage::set(STORAGE_KEY_VOLUME, &clamped).ok();
    });
}
/// Toggle mute
pub fn toggle_mute() {
    let is_muted = {
        let mut state = MUSIC_PLAYER.write();
        state.is_muted = !state.is_muted;
        #[cfg(feature = "mobile_platform")]
        if let Err(e) = android_media::set_volume(if state.is_muted { 0.0 } else { state.volume }) {
            log::error!("Failed to toggle native Android mute: {}", e);
            state.playback_error = Some(format!("Android playback failed: {}", e));
        }
        state.is_muted
    };
    storage::set(STORAGE_KEY_MUTED, &is_muted).ok();
}
pub fn toggle_loop() {
    let mut state = MUSIC_PLAYER.write();
    state.loop_mode = state.loop_mode.next();
    drop(state);
    mark_queue_dirty();
}

#[allow(dead_code)]
pub fn set_loop_mode(mode: LoopMode) {
    MUSIC_PLAYER.write().loop_mode = mode;
}

fn rebuild_shuffle_order(playlist_len: usize, current_index: usize) -> Vec<usize> {
    use rand::seq::SliceRandom;
    let mut ahead: Vec<usize> = (current_index..playlist_len).collect();
    ahead.shuffle(&mut rand::thread_rng());
    if let Some(pos) = ahead.iter().position(|&i| i == current_index) {
        ahead.swap(pos, 0);
    }
    let mut wrapped: Vec<usize> = (0..current_index).collect();
    wrapped.shuffle(&mut rand::thread_rng());
    ahead.extend(wrapped);
    ahead
}

pub fn toggle_shuffle() {
    let mut state = MUSIC_PLAYER.write();
    state.shuffle_enabled = !state.shuffle_enabled;
    if state.shuffle_enabled && !state.playlist.is_empty() {
        state.shuffle_order = rebuild_shuffle_order(state.playlist.len(), state.current_index);
    } else {
        state.shuffle_order = Vec::new();
    }
    drop(state);
    mark_queue_dirty();
}

#[allow(dead_code)]
pub fn play_next_track(track: MusicTrack) {
    let mut state = MUSIC_PLAYER.write();
    if state.playlist.is_empty() {
        drop(state);
        play_track(track, None, None);
        return;
    }
    let insert_at = state.current_index + 1;
    state.playlist.insert(insert_at, track);
    if state.shuffle_enabled {
        state.shuffle_order = rebuild_shuffle_order(state.playlist.len(), state.current_index);
    }
    mark_queue_dirty();
}
/// Set current time
#[cfg(not(feature = "mobile_platform"))]
pub fn set_current_time(time: f64) {
    *MUSIC_PLAYER.resolve().current_time().write() = time;
}
/// Seek to a specific time in the track (in seconds)
/// This sets the current time in state and triggers audio element seek via JS
pub fn seek_to(time: f64) {
    let mut state = MUSIC_PLAYER.write();
    state.current_time = time;
    #[cfg(feature = "mobile_platform")]
    if let Err(e) = android_media::seek_to(time) {
        log::error!("Failed to seek native Android playback: {}", e);
        state.playback_error = Some(format!("Android playback failed: {}", e));
    }
    drop(state);
    #[cfg(feature = "web")]
    spawn(async move {
        let script = format!(
            r#"
            let audio = document.getElementById("global-music-player-audio");
            if (audio) {{
                audio.currentTime = {time};
            }}
            "#,
        );
        if let Err(e) = document::eval(&script).await {
            log::warn!("Failed to sync seek via eval: {:?}", e);
        }
    });
    mark_queue_dirty();
}
/// Set duration
#[cfg(not(feature = "mobile_platform"))]
pub fn set_duration(duration: f64) {
    *MUSIC_PLAYER.resolve().duration().write() = duration;
}
/// Close/hide the player
pub fn close_player() {
    let mut state = MUSIC_PLAYER.write();
    state.is_visible = false;
    state.is_playing = false;
    state.view_mode = PlayerViewMode::Bar;
    #[cfg(feature = "mobile_platform")]
    {
        if let Err(e) = android_media::stop() {
            log::error!("Failed to stop native Android playback: {}", e);
        }
        if let Err(e) = android_media::clear_queue() {
            log::error!("Failed to clear native Android playback queue: {}", e);
        }
    }
    spawn(async move {
        clear_music_status().await;
    });
}
pub fn set_view_mode(mode: PlayerViewMode) {
    *MUSIC_PLAYER.resolve().view_mode().write() = mode;
}
pub fn set_floating_pos(x: f64, y: f64) {
    *MUSIC_PLAYER.resolve().floating_pos().write() = (x, y);
}
pub fn minimize_to_floating() {
    *MUSIC_PLAYER.resolve().view_mode().write() = PlayerViewMode::Floating;
}
pub fn restore_from_floating() {
    *MUSIC_PLAYER.resolve().view_mode().write() = PlayerViewMode::Bar;
}
/// Show zap dialog for a specific track (or current track if None)
pub fn show_zap_dialog_for_track(track: Option<MusicTrack>) {
    let store = MUSIC_PLAYER.resolve();
    let current = store.current_track().peek().clone();
    *store.zap_track().write() = track.or(current);
    *store.show_zap_dialog().write() = true;
}
/// Show zap dialog for current track
pub fn show_zap_dialog() {
    show_zap_dialog_for_track(None);
}
/// Hide zap dialog
pub fn hide_zap_dialog() {
    let store = MUSIC_PLAYER.resolve();
    *store.show_zap_dialog().write() = false;
    *store.zap_track().write() = None;
}
/// Vote for a track using Kind 33169 (Music Vote - addressable, one per user)
/// Supports both Wavlake and Nostr tracks via TrackSource
pub async fn vote_for_music(track: &MusicTrack) -> Result<(), String> {
    if matches!(track.source, TrackSource::Bible { .. }) {
        return Ok(());
    }
    if !auth_store::is_authenticated() {
        return Err("You must be logged in to vote".to_string());
    }
    let mut tags = vec![
        Tag::identifier("music-vote"),
        Tag::custom(TagKind::custom("title"), vec![track.title.clone()]),
        Tag::custom(TagKind::custom("artist"), vec![track.artist.clone()]),
    ];
    if let Some(ref image) = track.album_art_url {
        tags.push(Tag::custom(TagKind::custom("image"), vec![image.clone()]));
    }
    match &track.source {
        TrackSource::Nostr {
            coordinate,
            pubkey,
            d_tag,
        } => {
            let pk = nostr_sdk::PublicKey::from_hex(pubkey)
                .map_err(|e| format!("Invalid pubkey: {}", e))?;
            let coord = Coordinate::new(Kind::from(KIND_MUSIC_TRACK), pk).identifier(d_tag);
            tags.push(Tag::coordinate(coord, None));
            tags.push(Tag::custom(
                TagKind::custom("k"),
                vec![KIND_MUSIC_TRACK.to_string()],
            ));
            log::debug!("Voting for Nostr track: {}", coordinate);
        }
        TrackSource::Wavlake { .. } => {
            let track_url = format!("https://wavlake.com/track/{}", track.id);
            tags.push(Tag::custom(TagKind::custom("r"), vec![track_url]));
            tags.push(Tag::custom(
                TagKind::custom("k"),
                vec!["wavlake".to_string()],
            ));
            tags.push(Tag::custom(
                TagKind::custom("track_id"),
                vec![track.id.clone()],
            ));
            log::debug!("Voting for Wavlake track: {}", track.id);
        }
        TrackSource::NostrPodcast {
            coordinate,
            pubkey,
            d_tag,
            ..
        } => {
            let pk = nostr_sdk::PublicKey::from_hex(pubkey)
                .map_err(|e| format!("Invalid pubkey: {}", e))?;
            let coord =
                Coordinate::new(Kind::from(crate::utils::podcast::KIND_PODCAST_EPISODE), pk)
                    .identifier(d_tag);
            tags.push(Tag::coordinate(coord, None));
            tags.push(Tag::custom(
                TagKind::custom("k"),
                vec![crate::utils::podcast::KIND_PODCAST_EPISODE.to_string()],
            ));
            log::debug!("Voting for Nostr podcast: {}", coordinate);
        }
        TrackSource::RssPodcast {
            feed_url,
            episode_guid,
            ..
        } => {
            tags.push(Tag::custom(TagKind::custom("r"), vec![feed_url.clone()]));
            tags.push(Tag::custom(
                TagKind::custom("k"),
                vec!["podcast".to_string()],
            ));
            tags.push(Tag::custom(
                TagKind::custom("episode_guid"),
                vec![episode_guid.clone()],
            ));
            log::debug!("Voting for RSS podcast episode: {}", episode_guid);
        }
        TrackSource::RssMusic {
            feed_url,
            episode_id,
            ..
        } => {
            tags.push(Tag::custom(TagKind::custom("r"), vec![feed_url.clone()]));
            tags.push(Tag::custom(
                TagKind::custom("k"),
                vec!["rss-music".to_string()],
            ));
            tags.push(Tag::custom(
                TagKind::custom("episode_id"),
                vec![episode_id.to_string()],
            ));
            log::debug!("Voting for RSS music track: {}", episode_id);
        }
        TrackSource::Radio {
            coordinate,
            pubkey,
            d_tag,
            ..
        } => {
            let pk = nostr_sdk::PublicKey::from_hex(pubkey)
                .map_err(|e| format!("Invalid pubkey: {}", e))?;
            let coord = Coordinate::new(Kind::from(crate::utils::radio::KIND_RADIO_STATION), pk)
                .identifier(d_tag);
            tags.push(Tag::coordinate(coord, None));
            tags.push(Tag::custom(
                TagKind::custom("k"),
                vec![crate::utils::radio::KIND_RADIO_STATION.to_string()],
            ));
            log::debug!("Voting for radio station: {}", coordinate);
        }
        TrackSource::Bible { .. } => {}
    }
    let builder = EventBuilder::new(Kind::from(KIND_MUSIC_VOTE), "").tags(tags);
    match crate::stores::publish_queue::signing::sign_event_builder(builder).await {
        Ok(event) => {
            let event_id = event.id.to_hex();
            crate::stores::publish_queue::enqueue(
                event,
                crate::stores::publish_queue::types::QueueEventType::Other("music".to_string()),
                None,
                std::collections::HashMap::new(),
            ).await;
            log::info!(
                "Vote enqueued for '{}' by {} (event: {})",
                track.title,
                track.artist,
                event_id
            );
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to sign vote: {}", e);
            Err(format!("Failed to sign vote: {}", e))
        }
    }
}
/// Set playback speed (for podcasts)
pub fn set_playback_speed(speed: f64) {
    let speed = speed.clamp(0.5, 3.0);
    {
        let mut state = MUSIC_PLAYER.write();
        state.playback_speed = speed;
        #[cfg(feature = "mobile_platform")]
        if let Err(e) = android_media::set_playback_speed(speed) {
            log::error!("Failed to set native Android playback speed: {}", e);
            state.playback_error = Some(format!("Android playback failed: {}", e));
        }
    }
    storage::set(STORAGE_KEY_PLAYBACK_SPEED, &speed).ok();
    log::debug!("Playback speed set to {}x", speed);
}
/// Skip forward by specified seconds (for podcasts)
pub fn skip_forward(seconds: f64) {
    let state = MUSIC_PLAYER.read();
    let new_time = (state.current_time + seconds).min(state.duration);
    drop(state);
    seek_to(new_time);
    log::debug!("Skipped forward {} seconds to {}", seconds, new_time);
}
/// Skip backward by specified seconds (for podcasts)
pub fn skip_backward(seconds: f64) {
    let state = MUSIC_PLAYER.read();
    let new_time = (state.current_time - seconds).max(0.0);
    drop(state);
    seek_to(new_time);
    log::debug!("Skipped backward {} seconds to {}", seconds, new_time);
}
/// Set playback error message
pub fn set_playback_error(error: Option<String>) {
    *MUSIC_PLAYER.resolve().playback_error().write() = error;
}
/// Set buffering state
pub fn set_buffering(buffering: bool) {
    *MUSIC_PLAYER.resolve().is_buffering().write() = buffering;
}
/// Try next available stream when current one fails
/// Returns true if there's a fallback stream to try, false if all failed
#[cfg(not(feature = "mobile_platform"))]
pub fn try_next_stream() -> bool {
    let mut state = MUSIC_PLAYER.write();
    if state.available_streams.is_empty() {
        return false;
    }
    let next_idx = state.current_stream_index + 1;
    if next_idx >= state.available_streams.len() {
        state.playback_error = Some("All streams failed - station may be offline".to_string());
        return false;
    }
    let new_url = state.available_streams[next_idx].clone();
    let total_streams = state.available_streams.len();
    state.current_stream_index = next_idx;
    if let Some(ref mut track) = state.current_track {
        track.media_url = new_url;
    }
    log::info!("Falling back to stream {}/{}", next_idx + 1, total_streams);
    true
}
/// Try next available stream when current one fails (mobile/Android)
/// Returns true if a fallback stream was started, false if all failed
#[cfg(feature = "mobile_platform")]
pub fn try_next_stream_mobile() -> bool {
    let store = MUSIC_PLAYER.resolve();
    if store.available_streams().peek().is_empty() {
        return false;
    }
    let prev_idx = *store.current_stream_index().peek();
    let next_idx = prev_idx + 1;
    let stream_count = store.available_streams().peek().len();
    if next_idx >= stream_count {
        *store.is_buffering().write() = false;
        *store.playback_error().write() =
            Some("All streams failed - station may be offline".to_string());
        return false;
    }
    let new_url = store.available_streams().peek()[next_idx].clone();
    let total_streams = stream_count;
    *store.current_stream_index().write() = next_idx;
    *store.playback_error().write() = None;
    *store.is_buffering().write() = true;
    if let Some(ref mut track) = store.current_track().write().as_mut() {
        track.media_url = new_url;
    }
    let playlist = store.playlist().peek().clone();
    let index = *store.current_index().peek();
    log::info!(
        "Falling back to stream {}/{} on Android",
        next_idx + 1,
        total_streams
    );
    if let Err(e) = crate::platform::android_media::set_queue(&playlist, index, true) {
        log::error!("Failed to start Android fallback stream: {}", e);
        let store = MUSIC_PLAYER.resolve();
        *store.playback_error().write() = Some(format!("Fallback stream failed: {}", e));
        *store.is_buffering().write() = false;
        return false;
    }
    true
}

/// Set available streams for fallback logic
pub fn set_available_streams(streams: Vec<String>) {
    let mut state = MUSIC_PLAYER.write();
    state.available_streams = streams;
    state.current_stream_index = 0;
}
/// Set now playing metadata from HLS ID3 tags
#[cfg(not(feature = "mobile_platform"))]
pub fn set_now_playing(now_playing: Option<NowPlaying>) {
    *MUSIC_PLAYER.resolve().now_playing().write() = now_playing;
}
/// Clear now playing metadata
pub fn clear_now_playing() {
    *MUSIC_PLAYER.resolve().now_playing().write() = None;
}

#[cfg(feature = "mobile_platform")]
pub fn sync_native_playback_snapshot(snapshot: AndroidPlaybackSnapshot) {
    let store = MUSIC_PLAYER.resolve();
    let prev_index = *store.current_index().peek();
    if !store.playlist().peek().is_empty() && snapshot.current_index < store.playlist().peek().len()
        && prev_index != snapshot.current_index {
        *store.current_index().write() = snapshot.current_index;
        let track = store.playlist().peek().get(snapshot.current_index).cloned();
        *store.current_track().write() = track;
    }
    *store.is_playing().write() = snapshot.is_playing;
    *store.is_buffering().write() = snapshot.is_buffering;
    if snapshot.current_time.is_finite() && snapshot.current_time >= 0.0 {
        *store.current_time().write() = snapshot.current_time;
    }
    if snapshot.duration.is_finite() && snapshot.duration >= 0.0 {
        *store.duration().write() = snapshot.duration;
    }
    *store.playback_error().write() = snapshot.playback_error.clone();
    let is_live = store
        .current_track()
        .peek()
        .as_ref()
        .map(|t| t.is_live_stream)
        .unwrap_or(false);
    if !is_live {
        *store.now_playing().write() = None;
    }
    if snapshot.playback_error.is_some() {
        *store.is_buffering().write() = false;
        try_next_stream_mobile();
    }
}
