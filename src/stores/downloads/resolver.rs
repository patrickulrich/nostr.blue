//! Playback URL resolution: swap remote media URLs for local ones when a
//! completed download exists.
//!
//! - Android: `file://` URIs into app-internal storage (ExoPlayer reads
//!   these directly through the existing `NativeAudioBridge`).
//! - Linux desktop: `http://127.0.0.1:{port}/...` served by the embedded
//!   Range-capable media server (WebView `<audio>` cannot load `file://`).
//! - Web: no-op (no downloads).

use crate::stores::audio::music_player::MusicTrack;
use std::path::PathBuf;

/// Root directory for downloaded media (native only).
pub fn media_dir() -> PathBuf {
    crate::platform::storage::data_dir()
        .join("nostr-blue")
        .join("media")
}

/// Absolute path of a completed download for `id`, if the file exists.
#[cfg(feature = "native")]
pub fn local_path_for(id: &str) -> Option<PathBuf> {
    let item = super::store::get_item_from_store(id)?;
    if item.status != super::model::DownloadStatus::Completed {
        return None;
    }
    let rel = item.file_path.as_ref()?;
    let path = media_dir().join(rel);
    path.is_file().then_some(path)
}

/// Resolve the playable URL for a track: local URL when downloaded, `None`
/// when the remote URL should be used.
#[cfg(feature = "native")]
pub fn resolve_playable_url(track: &MusicTrack) -> Option<String> {
    if track.is_live_stream || track.media_url.is_empty() {
        return None;
    }
    let path = local_path_for(&track.id)?;
    #[cfg(feature = "mobile_platform")]
    {
        // App-internal storage: ExoPlayer plays this directly.
        let url = format!("file://{}", path.display());
        Some(url)
    }
    #[cfg(all(not(feature = "mobile_platform"), feature = "desktop"))]
    {
        let root = media_dir();
        let rel = path.strip_prefix(&root).ok()?;
        let rel_posix = rel.to_string_lossy().replace('\\', "/");
        let port = super::server::ensure_started()?;
        Some(format!(
            "http://127.0.0.1:{port}/{}",
            urlencoding::encode(&rel_posix)
        ))
    }
    #[cfg(all(not(feature = "mobile_platform"), not(feature = "desktop")))]
    {
        None
    }
}

/// Rewrite a track's `media_url` in place when a local copy exists.
#[cfg(feature = "native")]
pub fn rewrite_track(track: &mut MusicTrack) {
    if let Some(local) = resolve_playable_url(track) {
        log::info!("Using local media for {}: {}", track.id, local);
        track.media_url = local;
    }
}

/// Rewrite every track of a playlist in place.
#[cfg(feature = "native")]
pub fn rewrite_playlist(playlist: &mut [MusicTrack]) {
    for track in playlist.iter_mut() {
        rewrite_track(track);
    }
}
