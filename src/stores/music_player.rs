use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
use crate::services::wavlake::WavlakeTrack;
use crate::stores::{auth_store, nostr_client};
use crate::stores::nostr_music::{TrackSource, NostrTrack, KIND_MUSIC_TRACK};
use nostr_sdk::{EventBuilder, Timestamp, Kind, Tag, TagKind};
use nostr_sdk::nips::nip01::Coordinate;
use nostr_sdk::nips::nip38::{LiveStatus, StatusType};

/// Kind number for Music Vote events (addressable, one per user)
pub const KIND_MUSIC_VOTE: u16 = 33169;

/// Music track for the player (unified for Wavlake and Nostr sources)
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
}

impl From<WavlakeTrack> for MusicTrack {
    fn from(track: WavlakeTrack) -> Self {
        // Parse msatTotal from string to u64
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
            created_at: None, // Wavlake tracks don't have created_at in rankings
        }
    }
}

impl From<NostrTrack> for MusicTrack {
    fn from(track: NostrTrack) -> Self {
        Self {
            id: track.event_id.clone(),
            title: track.title,
            artist: String::new(), // Will be filled in by profile lookup
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
            msat_total: None, // Will be filled in by zap totals fetch
            created_at: Some(track.created_at),
        }
    }
}

/// Music player state
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
        }
    }
}

/// Global music player state
pub static MUSIC_PLAYER: GlobalSignal<MusicPlayerState> =
    Signal::global(MusicPlayerState::default);

const STORAGE_KEY_VOLUME: &str = "music_player_volume";
const STORAGE_KEY_MUTED: &str = "music_player_muted";

/// Initialize music player from localStorage
pub fn init_player() {
    let mut state = MusicPlayerState::default();

    // Load volume setting
    if let Ok(volume) = LocalStorage::get::<f64>(STORAGE_KEY_VOLUME) {
        state.volume = volume.clamp(0.0, 1.0);
    }

    // Load muted setting
    if let Ok(is_muted) = LocalStorage::get::<bool>(STORAGE_KEY_MUTED) {
        state.is_muted = is_muted;
    }

    *MUSIC_PLAYER.write() = state;
    log::info!("Music player initialized");
}

/// Publish NIP-38 music status (Kind 30315)
async fn publish_music_status(track: &MusicTrack) {
    // Only publish if user is authenticated
    if !auth_store::is_authenticated() {
        return;
    }

    let client = match nostr_client::get_client() {
        Some(c) => c,
        None => {
            log::warn!("Nostr client not initialized, skipping music status");
            return;
        }
    };

    // Format content: "Track Title - Artist Name"
    let content = format!("{} - {}", track.title, track.artist);

    // Build track URL reference based on source
    let track_reference = match &track.source {
        TrackSource::Wavlake { .. } => {
            // Link to Wavlake track on our site
            format!("https://nostr.blue/music/track/{}", track.id)
        }
        TrackSource::Nostr { pubkey, d_tag, .. } => {
            // Create naddr bech32 for nostr tracks (NIP-19)
            // Format: nostr:naddr1... where naddr encodes kind:pubkey:d-tag
            // For simplicity, we'll use the coordinate format which can be parsed
            format!("https://nostr.blue/music/playlist/{}:{}:{}",
                crate::stores::nostr_music::KIND_MUSIC_TRACK, pubkey, d_tag)
        }
    };

    // Create LiveStatus with expiration based on track duration
    let mut status = LiveStatus {
        status_type: StatusType::Music,
        expiration: None,
        reference: Some(track_reference),
    };

    // Set expiration if track has duration
    if let Some(duration) = track.duration {
        if duration > 0 {
            status.expiration = Some(Timestamp::now() + std::time::Duration::from_secs(duration as u64));
        }
    }

    // Create Kind 30315 (User Status) event using NIP-38 helper
    let builder = EventBuilder::live_status(status, content);

    match client.send_event_builder(builder).await {
        Ok(event_id) => {
            log::info!("Music status published: {} (event: {})", track.title, event_id.to_hex());
        }
        Err(e) => {
            log::error!("Failed to publish music status: {}", e);
        }
    }
}

/// Clear music status (publish empty status)
async fn clear_music_status() {
    // Only publish if user is authenticated
    if !auth_store::is_authenticated() {
        return;
    }

    let client = match nostr_client::get_client() {
        Some(c) => c,
        None => return,
    };

    // Create empty status to clear (per NIP-38: empty content clears the status)
    let status = LiveStatus::new(StatusType::Music);
    let builder = EventBuilder::live_status(status, "");

    match client.send_event_builder(builder).await {
        Ok(_) => {
            log::info!("Music status cleared");
        }
        Err(e) => {
            log::error!("Failed to clear music status: {}", e);
        }
    }
}

/// Play a track
pub fn play_track(track: MusicTrack, playlist: Option<Vec<MusicTrack>>, index_override: Option<usize>) {
    let mut state = MUSIC_PLAYER.write();

    let playlist = playlist.unwrap_or_else(|| vec![track.clone()]);
    let index = index_override.unwrap_or_else(|| {
        playlist.iter().position(|t| t.id == track.id).unwrap_or(0)
    });

    state.current_track = Some(track.clone());
    state.playlist = playlist;
    state.current_index = index;
    state.is_playing = true;
    state.is_visible = true;
    state.current_time = 0.0;

    log::info!("Playing track: {}", track.title);

    // Publish NIP-38 music status
    spawn(async move {
        publish_music_status(&track).await;
    });
}

/// Toggle play/pause
pub fn toggle_play() {
    let mut state = MUSIC_PLAYER.write();
    state.is_playing = !state.is_playing;

    // Clear status when pausing
    if !state.is_playing {
        spawn(async move {
            clear_music_status().await;
        });
    } else {
        // Republish status when resuming
        if let Some(track) = state.current_track.clone() {
            spawn(async move {
                publish_music_status(&track).await;
            });
        }
    }
}

/// Play next track in playlist
pub fn next_track() {
    let mut state = MUSIC_PLAYER.write();

    if state.playlist.is_empty() {
        return;
    }

    state.current_index = (state.current_index + 1) % state.playlist.len();
    state.current_track = state.playlist.get(state.current_index).cloned();
    state.is_playing = true;
    state.current_time = 0.0;

    if let Some(track) = state.current_track.clone() {
        log::info!("Next track: {}", track.title);

        // Publish NIP-38 music status for the new track
        spawn(async move {
            publish_music_status(&track).await;
        });
    }
}

/// Play previous track in playlist
pub fn previous_track() {
    let mut state = MUSIC_PLAYER.write();

    if state.playlist.is_empty() {
        return;
    }

    // If more than 3 seconds into the track, restart it
    if state.current_time > 3.0 {
        state.current_time = 0.0;
        // Publish status again for restarted track
        if let Some(track) = state.current_track.clone() {
            spawn(async move {
                publish_music_status(&track).await;
            });
        }
        return;
    }

    state.current_index = if state.current_index == 0 {
        state.playlist.len() - 1
    } else {
        state.current_index - 1
    };

    state.current_track = state.playlist.get(state.current_index).cloned();
    state.is_playing = true;
    state.current_time = 0.0;

    if let Some(track) = state.current_track.clone() {
        log::info!("Previous track: {}", track.title);

        // Publish NIP-38 music status for the new track
        spawn(async move {
            publish_music_status(&track).await;
        });
    }
}

/// Set volume (0.0 - 1.0)
pub fn set_volume(volume: f64) {
    let clamped = volume.clamp(0.0, 1.0);
    let mut state = MUSIC_PLAYER.write();
    state.volume = clamped;
    LocalStorage::set(STORAGE_KEY_VOLUME, clamped).ok();
}

/// Toggle mute
pub fn toggle_mute() {
    let mut state = MUSIC_PLAYER.write();
    state.is_muted = !state.is_muted;
    LocalStorage::set(STORAGE_KEY_MUTED, state.is_muted).ok();
}

/// Set current time
pub fn set_current_time(time: f64) {
    let mut state = MUSIC_PLAYER.write();
    state.current_time = time;
}

/// Set duration
pub fn set_duration(duration: f64) {
    let mut state = MUSIC_PLAYER.write();
    state.duration = duration;
}

/// Close/hide the player
pub fn close_player() {
    let mut state = MUSIC_PLAYER.write();
    state.is_visible = false;
    state.is_playing = false;

    // Clear NIP-38 music status
    spawn(async move {
        clear_music_status().await;
    });
}

/// Show the player
#[allow(dead_code)]
pub fn show_player() {
    let mut state = MUSIC_PLAYER.write();
    if state.current_track.is_some() {
        state.is_visible = true;
    }
}

/// Clear the player and stop playback
#[allow(dead_code)]
pub fn clear_player() {
    let mut state = MUSIC_PLAYER.write();
    state.current_track = None;
    state.playlist.clear();
    state.current_index = 0;
    state.is_playing = false;
    state.is_visible = false;
    state.current_time = 0.0;
    state.duration = 0.0;

    // Clear NIP-38 music status
    spawn(async move {
        clear_music_status().await;
    });
}

/// Get current track
#[allow(dead_code)]
pub fn get_current_track() -> Option<MusicTrack> {
    MUSIC_PLAYER.read().current_track.clone()
}

/// Get playing status
#[allow(dead_code)]
pub fn is_playing() -> bool {
    MUSIC_PLAYER.read().is_playing
}

/// Get player visibility
#[allow(dead_code)]
pub fn is_visible() -> bool {
    MUSIC_PLAYER.read().is_visible
}

/// Get volume
#[allow(dead_code)]
pub fn get_volume() -> f64 {
    MUSIC_PLAYER.read().volume
}

/// Check if muted
#[allow(dead_code)]
pub fn is_muted() -> bool {
    MUSIC_PLAYER.read().is_muted
}

/// Show zap dialog for a specific track (or current track if None)
pub fn show_zap_dialog_for_track(track: Option<MusicTrack>) {
    let mut state = MUSIC_PLAYER.write();
    state.zap_track = track.or_else(|| state.current_track.clone());
    state.show_zap_dialog = true;
}

/// Show zap dialog for current track
pub fn show_zap_dialog() {
    show_zap_dialog_for_track(None);
}

/// Hide zap dialog
pub fn hide_zap_dialog() {
    let mut state = MUSIC_PLAYER.write();
    state.show_zap_dialog = false;
    state.zap_track = None;
}

/// Vote for a track using Kind 33169 (Music Vote - addressable, one per user)
/// Supports both Wavlake and Nostr tracks via TrackSource
pub async fn vote_for_music(track: &MusicTrack) -> Result<(), String> {
    // Check if user is authenticated
    if !auth_store::is_authenticated() {
        return Err("You must be logged in to vote".to_string());
    }

    let client = nostr_client::get_client()
        .ok_or("Nostr client not initialized")?;

    // Build tags based on track source
    let mut tags = vec![
        // Fixed d-tag ensures one vote per user (replaces previous vote)
        Tag::identifier("music-vote"),
        // Cached metadata for leaderboard display
        Tag::custom(TagKind::custom("title"), vec![track.title.clone()]),
        Tag::custom(TagKind::custom("artist"), vec![track.artist.clone()]),
    ];

    // Add image if available
    if let Some(ref image) = track.album_art_url {
        tags.push(Tag::custom(TagKind::custom("image"), vec![image.clone()]));
    }

    // Add track reference based on source
    match &track.source {
        TrackSource::Nostr { coordinate, pubkey, d_tag } => {
            // Parse pubkey for Coordinate
            let pk = nostr_sdk::PublicKey::from_hex(pubkey)
                .map_err(|e| format!("Invalid pubkey: {}", e))?;

            // Create coordinate for the track (Kind 36787)
            let coord = Coordinate::new(Kind::from(KIND_MUSIC_TRACK), pk)
                .identifier(d_tag);

            tags.push(Tag::coordinate(coord, None));
            tags.push(Tag::custom(TagKind::custom("k"), vec![KIND_MUSIC_TRACK.to_string()]));

            log::debug!("Voting for Nostr track: {}", coordinate);
        }
        TrackSource::Wavlake { .. } => {
            // Use r-tag with Wavlake URL
            let track_url = format!("https://wavlake.com/track/{}", track.id);
            tags.push(Tag::custom(TagKind::custom("r"), vec![track_url]));
            tags.push(Tag::custom(TagKind::custom("k"), vec!["wavlake".to_string()]));
            // Also store the track ID for easier aggregation
            tags.push(Tag::custom(TagKind::custom("track_id"), vec![track.id.clone()]));

            log::debug!("Voting for Wavlake track: {}", track.id);
        }
    }

    let builder = EventBuilder::new(Kind::from(KIND_MUSIC_VOTE), "").tags(tags);

    match client.send_event_builder(builder).await {
        Ok(output) => {
            log::info!(
                "Vote submitted for '{}' by {} (event: {})",
                track.title,
                track.artist,
                output.id().to_hex()
            );
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to publish vote: {}", e);
            Err(format!("Failed to publish vote: {}", e))
        }
    }
}
