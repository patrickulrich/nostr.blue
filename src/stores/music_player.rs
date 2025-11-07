use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
use crate::services::wavlake::WavlakeTrack;
use crate::stores::{auth_store, nostr_client};
use nostr_sdk::{EventBuilder, Timestamp};
use nostr_sdk::nips::nip38::{LiveStatus, StatusType};

/// Music track for the player (simplified from WavlakeTrack)
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
}

impl From<WavlakeTrack> for MusicTrack {
    fn from(track: WavlakeTrack) -> Self {
        Self {
            id: track.id,
            title: track.title,
            artist: track.artist,
            album: Some(track.album_title),
            media_url: track.media_url,
            album_art_url: Some(track.album_art_url),
            artist_art_url: Some(track.artist_art_url),
            duration: Some(track.duration),
            artist_id: Some(track.artist_id),
            album_id: Some(track.album_id),
            artist_npub: track.artist_npub,
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

    // Build track URL reference
    let track_url = format!("https://nostr.blue/music/track/{}", track.id);

    // Create LiveStatus with expiration based on track duration
    let mut status = LiveStatus {
        status_type: StatusType::Music,
        expiration: None,
        reference: Some(track_url),
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

/// Vote for a track using NIP-51 Kind 30003 (Bookmark Sets)
pub async fn vote_for_track(track_id: &str, title: &str, artist: &str) {
    // Check if user is authenticated
    if !auth_store::is_authenticated() {
        log::error!("User must be logged in to vote");
        // TODO: Show toast notification
        return;
    }

    let client = match nostr_client::get_client() {
        Some(c) => c,
        None => {
            log::error!("Nostr client not initialized");
            return;
        }
    };

    let track_url = format!("https://wavlake.com/track/{}", track_id);

    // Create Kind 30003 event with proper tags
    let tags = vec![
        nostr_sdk::Tag::custom(nostr_sdk::TagKind::Custom("d".into()), vec!["peachy-song-vote".to_string()]),
        nostr_sdk::Tag::custom(nostr_sdk::TagKind::Custom("title".into()), vec!["Weekly Song Vote".to_string()]),
        nostr_sdk::Tag::custom(nostr_sdk::TagKind::Custom("description".into()), vec!["My vote for the best song of the week".to_string()]),
        nostr_sdk::Tag::custom(nostr_sdk::TagKind::Custom("r".into()), vec![track_url]),
        nostr_sdk::Tag::custom(nostr_sdk::TagKind::Custom("track_title".into()), vec![title.to_string()]),
        nostr_sdk::Tag::custom(nostr_sdk::TagKind::Custom("track_artist".into()), vec![artist.to_string()]),
        nostr_sdk::Tag::custom(nostr_sdk::TagKind::Custom("track_id".into()), vec![track_id.to_string()]),
    ];

    let builder = EventBuilder::new(nostr_sdk::Kind::Custom(30003), "").tags(tags);

    match client.send_event_builder(builder).await {
        Ok(event_id) => {
            log::info!("Vote submitted for track: {} by {} (event: {})", title, artist, event_id.to_hex());
            // TODO: Show success toast
        }
        Err(e) => {
            log::error!("Failed to publish vote: {}", e);
            // TODO: Show error toast
        }
    }
}
