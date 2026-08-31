//! Data model for the on-device media downloads layer.
//!
//! These types are cross-platform so the web build can compile the store and
//! progress helpers (continue-listening works on web too); the download
//! engine itself is native-only (Android + Linux desktop). Many helpers are
//! therefore only consumed on native builds.
#![cfg_attr(feature = "web", allow(dead_code))]

use crate::stores::audio::music_player::MusicTrack;
use crate::stores::audio::nostr_music::TrackSource;
use serde::{Deserialize, Serialize};

/// Which library a downloaded file belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Podcast,
    Music,
}

impl MediaKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            MediaKind::Podcast => "podcast",
            MediaKind::Music => "music",
        }
    }

    pub fn from_str_value(value: &str) -> Self {
        if value == "podcast" {
            MediaKind::Podcast
        } else {
            MediaKind::Music
        }
    }

    pub fn for_track(track: &MusicTrack) -> Self {
        if track.is_podcast {
            return MediaKind::Podcast;
        }
        match track.source {
            TrackSource::NostrPodcast { .. } | TrackSource::RssPodcast { .. } => {
                MediaKind::Podcast
            }
            _ => MediaKind::Music,
        }
    }
}

/// Lifecycle status of a download item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
}

impl DownloadStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            DownloadStatus::Queued => "queued",
            DownloadStatus::Downloading => "downloading",
            DownloadStatus::Paused => "paused",
            DownloadStatus::Completed => "completed",
            DownloadStatus::Failed => "failed",
        }
    }

    pub fn from_str_value(value: &str) -> Self {
        match value {
            "downloading" => DownloadStatus::Downloading,
            "paused" => DownloadStatus::Paused,
            "completed" => DownloadStatus::Completed,
            "failed" => DownloadStatus::Failed,
            _ => DownloadStatus::Queued,
        }
    }
}

/// A single downloadable unit, keyed by `MusicTrack.id`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: String,
    pub kind: MediaKind,
    pub status: DownloadStatus,
    pub remote_url: String,
    /// File path relative to the media root dir (posix separators).
    pub file_path: Option<String>,
    pub bytes_downloaded: u64,
    pub bytes_total: Option<u64>,
    pub error: Option<String>,
    /// Whether this was auto-downloaded (subject to LRU eviction).
    pub auto: bool,
    pub created_at: u64,
    pub completed_at: Option<u64>,
    /// Snapshot of the track for offline hydration.
    pub track: MusicTrack,
}

impl DownloadItem {
    /// Progress in `[0.0, 1.0]` when computable.
    pub fn progress(&self) -> Option<f64> {
        self.bytes_total
            .filter(|total| *total > 0)
            .map(|total| (self.bytes_downloaded as f64 / total as f64).clamp(0.0, 1.0))
    }
}

/// Device-local download management settings. Persisted via
/// `platform::storage` (localStorage on web, settings.json on native) —
/// deliberately NOT part of the synced NIP-78 preference blobs because
/// storage policy is per-device.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DownloadSettings {
    #[serde(default = "default_wifi_only")]
    pub wifi_only: bool,
    /// How many of the newest episodes to auto-download per show that has
    /// auto-download enabled (show-page toggle). 0 disables enqueuing.
    #[serde(default = "default_episodes_per_show")]
    pub episodes_per_show: u32,
    /// Retention: keep at most this many auto-downloaded episodes per show;
    /// older ones are evicted during sync. Manual downloads are never
    /// evicted.
    #[serde(default = "default_keep_per_show")]
    pub keep_per_show: u32,
    /// Master switch; when false no new download starts.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_wifi_only() -> bool {
    true
}

fn default_episodes_per_show() -> u32 {
    3
}

fn default_keep_per_show() -> u32 {
    3
}

fn default_enabled() -> bool {
    true
}

impl Default for DownloadSettings {
    fn default() -> Self {
        Self {
            wifi_only: default_wifi_only(),
            episodes_per_show: default_episodes_per_show(),
            keep_per_show: default_keep_per_show(),
            enabled: default_enabled(),
        }
    }
}

/// Cached show (podcast feed) record for offline browsing and auto-download.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ShowCacheEntry {
    /// Stable key: `rss:{guid}`, `rss:{feed_url}`, or `nostr:{coordinate}`.
    pub show_key: String,
    pub title: Option<String>,
    pub image: Option<String>,
    pub feed_url: Option<String>,
    pub auto_download: bool,
    pub last_synced_at: Option<u64>,
    /// High-water mark: newest episode publish date already seen (unix secs).
    pub last_episode_date: Option<u64>,
}

/// A persisted playback position for continue-listening.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaybackPosition {
    pub position_secs: f64,
    pub duration_secs: Option<f64>,
    pub updated_at: u64,
}

/// Derive the stable cache key for a show from a track's source routing.
/// Always keyed by feed URL when available (never the Podcast Index numeric
/// id) so PI-sourced and directly-RSS-sourced episodes of the same show map
/// to the same key — the sync pass keys shows by `rss:{feed_url}`.
pub fn show_key_for_track(track: &MusicTrack) -> Option<String> {
    match &track.source {
        TrackSource::RssPodcast { feed_url, .. } => Some(format!("rss:{feed_url}")),
        TrackSource::NostrPodcast { addr, .. } => Some(match addr {
            crate::stores::audio::nostr_music::PodcastAddr::Legacy { coordinate, .. } => {
                format!("nostr:{coordinate}")
            }
            crate::stores::audio::nostr_music::PodcastAddr::F4 { event_id } => {
                format!("nostr:{event_id}")
            }
        }),
        TrackSource::RssMusic { feed_url, .. } => Some(format!("rssmusic:{feed_url}")),
        _ => None,
    }
}

/// Sanitize an arbitrary identifier into a filesystem-safe path component.
/// A short deterministic hash suffix guards against collisions after
/// character replacement.
pub fn safe_component(raw: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut hasher);
    let hash = hasher.finish();
    let sanitized: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    format!("{sanitized}-{hash:016x}")
}

/// Guess a file extension from a remote media URL.
pub fn extension_for_url(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "mp3" | "m4a" | "aac" | "ogg" | "oga" | "opus" | "wav" | "flac" | "webm" | "mp4" => ext,
        _ => "mp3".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_component_is_deterministic_and_fs_safe() {
        let a = safe_component("rss-podcast:https://example.com/feed.xml:guid#1");
        let b = safe_component("rss-podcast:https://example.com/feed.xml:guid#1");
        assert_eq!(a, b);
        assert!(!a.contains(':') && !a.contains('/') && !a.contains('#'));
        assert!(a.len() > 16);
    }

    #[test]
    fn test_extension_for_url() {
        assert_eq!(extension_for_url("https://x.com/a/b.mp3?token=1"), "mp3");
        assert_eq!(extension_for_url("https://x.com/audio.Enclosure.M4A"), "m4a");
        assert_eq!(extension_for_url("https://x.com/no-ext"), "mp3");
    }

    #[test]
    fn test_status_roundtrip() {
        for status in [
            DownloadStatus::Queued,
            DownloadStatus::Downloading,
            DownloadStatus::Paused,
            DownloadStatus::Completed,
            DownloadStatus::Failed,
        ] {
            assert_eq!(
                DownloadStatus::from_str_value(status.as_str()),
                status,
                "roundtrip failed for {status:?}"
            );
        }
    }

    #[test]
    fn test_settings_defaults_deserialize_minimal() {
        let parsed: DownloadSettings = serde_json::from_str("{}").unwrap();
        assert!(parsed.wifi_only);
        assert_eq!(parsed.episodes_per_show, 3);
        assert_eq!(parsed.keep_per_show, 3);
        assert!(parsed.enabled);
    }

    #[test]
    fn test_settings_ignores_legacy_max_storage_field() {
        // Settings persisted before the storage cap was removed still
        // contain max_storage_mb; serde must ignore it.
        let json = r#"{"wifi_only":false,"max_storage_mb":4096,"episodes_per_show":5,"keep_per_show":2,"enabled":true}"#;
        let parsed: DownloadSettings = serde_json::from_str(json).unwrap();
        assert!(!parsed.wifi_only);
        assert_eq!(parsed.episodes_per_show, 5);
        assert_eq!(parsed.keep_per_show, 2);
    }
}
