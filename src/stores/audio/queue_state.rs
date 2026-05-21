use crate::stores::music_player::{LoopMode, MusicTrack};
use serde::{Deserialize, Serialize};

const STORAGE_KEY: &str = "music_player_queue";
const MAX_PERSISTED_TRACKS: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedQueueState {
    pub version: u8,
    pub playlist: Vec<MusicTrack>,
    pub current_index: usize,
    pub progress_secs: u64,
    #[serde(default)]
    pub loop_mode: LoopMode,
    #[serde(default)]
    pub shuffle_enabled: bool,
}

impl Default for PersistedQueueState {
    fn default() -> Self {
        Self {
            version: 2,
            playlist: Vec::new(),
            current_index: 0,
            progress_secs: 0,
            loop_mode: LoopMode::None,
            shuffle_enabled: false,
        }
    }
}

pub fn save_queue(state: &PersistedQueueState) {
    if state.playlist.is_empty() {
        return;
    }
    let mut to_save = state.clone();
    if to_save.playlist.len() > MAX_PERSISTED_TRACKS {
        let offset = to_save.current_index.saturating_sub(MAX_PERSISTED_TRACKS / 2);
        let end = (offset + MAX_PERSISTED_TRACKS).min(to_save.playlist.len());
        let start = end.saturating_sub(MAX_PERSISTED_TRACKS);
        to_save.current_index = to_save.current_index.saturating_sub(start);
        to_save.playlist = to_save.playlist[start..end].to_vec();
    }
    let _ = crate::platform::storage::set(STORAGE_KEY, &to_save);
}

pub fn load_queue() -> Option<PersistedQueueState> {
    let state: PersistedQueueState = crate::platform::storage::get(STORAGE_KEY).ok()?;
    if state.playlist.is_empty() {
        return None;
    }
    let idx = state.current_index.min(state.playlist.len() - 1);
    Some(PersistedQueueState {
        current_index: idx,
        ..state
    })
}

pub fn clear_queue() {
    let _ = crate::platform::storage::delete(STORAGE_KEY);
}
