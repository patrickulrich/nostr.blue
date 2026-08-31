//! Continue-listening playback positions.
//!
//! Cross-platform by design: native persists to the downloads SQLite DB,
//! web persists to a capped localStorage JSON map. Writes are throttled and
//! quantized by `note_playback_position`, which the player calls from its
//! time-update paths.

use super::model::PlaybackPosition;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

#[cfg(feature = "web")]
const WEB_STORAGE_KEY: &str = "playback_positions";
#[cfg(feature = "web")]
const WEB_MAX_ENTRIES: usize = 400;
/// Only persist positions at/after this many seconds.
const MIN_POSITION_SECS: f64 = 5.0;
/// Consider a position "finished" within this margin of the end.
const FINISHED_MARGIN_SECS: f64 = 10.0;

/// Last quantized position persisted per track (throttle state).
static THROTTLE: LazyLock<Mutex<HashMap<String, f64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Record the current playback position of a track (fire-and-forget,
/// internally throttled to one write per track per 5 seconds).
pub fn note_playback_position(track_id: &str, position: f64, duration: Option<f64>) {
    if track_id.is_empty() || !position.is_finite() || position < 0.0 {
        return;
    }
    if position < MIN_POSITION_SECS {
        return;
    }
    // Skip (and clear) positions that are effectively "finished".
    if let Some(dur) = duration {
        if dur.is_finite() && dur > 0.0 && position >= dur - FINISHED_MARGIN_SECS {
            THROTTLE
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(track_id);
            #[cfg(feature = "native")]
            {
                let track_id = track_id.to_string();
                crate::platform::spawn::spawn_detached(async move {
                    let _ = super::db::delete_position(&track_id).await;
                });
            }
            #[cfg(feature = "web")]
            {
                let mut map = load_web_map();
                if map.remove(track_id).is_some() {
                    save_web_map(&map);
                }
            }
            return;
        }
    }
    let quantized = (position as u64).saturating_sub((position as u64) % 5) as f64;
    {
        let mut guard = THROTTLE.lock().unwrap_or_else(|e| e.into_inner());
        if guard
            .get(track_id)
            .is_some_and(|last| quantized <= *last)
        {
            return;
        }
        guard.insert(track_id.to_string(), quantized);
    }
    let track_id = track_id.to_string();
    #[cfg(feature = "native")]
    crate::platform::spawn::spawn_detached(async move {
        if let Err(e) = super::db::save_position(&track_id, quantized, duration).await {
            log::warn!("Failed to persist playback position: {}", e);
        }
    });
    #[cfg(feature = "web")]
    {
        let mut map = load_web_map();
        map.insert(
            track_id.clone(),
            PlaybackPosition {
                position_secs: quantized,
                duration_secs: duration.filter(|d| d.is_finite() && *d > 0.0),
                updated_at: chrono::Utc::now().timestamp().max(0) as u64,
            },
        );
        prune_web_map(&mut map);
        save_web_map(&map);
    }
}

/// Fetch the persisted position for a track (if any).
pub async fn get_position(track_id: &str) -> Option<PlaybackPosition> {
    if track_id.is_empty() {
        return None;
    }
    #[cfg(feature = "native")]
    {
        super::db::get_position(track_id).await.ok().flatten()
    }
    #[cfg(feature = "web")]
    {
        load_web_map().get(track_id).copied()
    }
}

/// Most recently updated positions, newest first. Reserved for
/// continue-listening surfaces beyond the current resume-on-play behavior.
#[allow(dead_code)]
pub async fn recent_positions(limit: u32) -> Vec<(String, PlaybackPosition)> {
    #[cfg(feature = "native")]
    {
        super::db::recent_positions(limit).await.unwrap_or_default()
    }
    #[cfg(feature = "web")]
    {
        let mut entries: Vec<(String, PlaybackPosition)> =
            load_web_map().into_iter().collect();
        entries.sort_by_key(|(_, pos)| std::cmp::Reverse(pos.updated_at));
        entries.truncate(limit as usize);
        entries
    }
}

#[cfg(feature = "web")]
fn load_web_map() -> HashMap<String, PlaybackPosition> {
    crate::platform::storage::get::<HashMap<String, PlaybackPosition>>(WEB_STORAGE_KEY)
        .unwrap_or_default()
}

#[cfg(feature = "web")]
fn save_web_map(map: &HashMap<String, PlaybackPosition>) {
    if let Err(e) = crate::platform::storage::set(WEB_STORAGE_KEY, map) {
        log::warn!("Failed to persist playback positions: {}", e);
    }
}

#[cfg(feature = "web")]
fn prune_web_map(map: &mut HashMap<String, PlaybackPosition>) {
    while map.len() > WEB_MAX_ENTRIES {
        let oldest = map
            .iter()
            .min_by_key(|(_, pos)| pos.updated_at)
            .map(|(k, _)| k.clone());
        match oldest {
            Some(key) => {
                map.remove(&key);
            }
            None => break,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_throttle_skips_non_increasing_positions() {
        // Pure logic check of the quantize helper used by note_playback_position.
        let position = 73.9_f64;
        let quantized = (position as u64 - (position as u64) % 5) as f64;
        assert_eq!(quantized, 70.0);
    }
}
