//! Global reactive state for the downloads layer.
//!
//! Mutation helpers are consumed by the native download engine and settings
//! UI; the web build only reads settings + keeps the (empty) item list.
#![cfg_attr(feature = "web", allow(dead_code))]

use super::model::{DownloadItem, DownloadSettings};
#[cfg(feature = "native")]
use crate::stores::audio::music_player::MusicTrack;
use dioxus::prelude::*;
use dioxus_stores::Store;

const SETTINGS_KEY: &str = "downloads_settings";

#[derive(Clone, Debug, PartialEq, Store, Default)]
pub struct DownloadsState {
    pub items: Vec<DownloadItem>,
    pub settings: DownloadSettings,
    /// Total bytes occupied under the media dir (native only).
    pub storage_used_bytes: u64,
    /// True while a library sync pass is running.
    pub sync_running: bool,
    pub last_sync_at: Option<u64>,
}

pub static DOWNLOADS: GlobalStore<DownloadsState> = Global::new(DownloadsState::default);

/// Boot-time initialization. On native this opens the DB and hydrates the
/// store; on web only settings are loaded (progress store is lazy).
pub fn init_downloads() {
    load_settings_into_store();
    #[cfg(feature = "native")]
    {
        if let Err(e) = super::db::init_db() {
            log::error!("Failed to initialize downloads database: {}", e);
        }
        super::manager::spawn_loader();
    }
}

fn load_settings_into_store() {
    let settings = crate::platform::storage::get::<DownloadSettings>(SETTINGS_KEY)
        .unwrap_or_default();
    DOWNLOADS.write().settings = settings;
}

pub fn save_settings(settings: DownloadSettings) {
    if let Err(e) = crate::platform::storage::set(SETTINGS_KEY, &settings) {
        log::warn!("Failed to persist download settings: {}", e);
    }
    DOWNLOADS.write().settings = settings;
    #[cfg(feature = "native")]
    super::manager::wake_worker();
}

pub fn update_settings(mut apply: impl FnMut(&mut DownloadSettings)) {
    let mut state = DOWNLOADS.write();
    apply(&mut state.settings);
    let settings = state.settings.clone();
    drop(state);
    save_settings(settings);
}

/// Replace or insert an item in the reactive store.
pub fn upsert_item_in_store(item: DownloadItem) {
    let mut state = DOWNLOADS.write();
    match state.items.iter().position(|i| i.id == item.id) {
        Some(idx) => state.items[idx] = item,
        None => state.items.push(item),
    }
}

pub fn remove_item_from_store(id: &str) {
    DOWNLOADS.write().items.retain(|i| i.id != id);
}

pub fn get_item_from_store(id: &str) -> Option<DownloadItem> {
    DOWNLOADS
        .read()
        .items
        .iter()
        .find(|i| i.id == id)
        .cloned()
}

/// Original remote URL for a track whose in-memory `media_url` may have been
/// rewritten to a local one by the resolver. Falls back to the given URL.
#[cfg(feature = "native")]
pub fn original_track_url(track: &MusicTrack) -> String {
    if !track.media_url.starts_with("file://")
        && !track.media_url.starts_with("http://127.0.0.1:")
    {
        return track.media_url.clone();
    }
    DOWNLOADS
        .read()
        .items
        .iter()
        .find(|i| i.id == track.id)
        .map(|i| i.remote_url.clone())
        .unwrap_or_else(|| track.media_url.clone())
}

pub fn is_downloaded(id: &str) -> bool {
    DOWNLOADS
        .read()
        .items
        .iter()
        .any(|i| i.id == id && i.status == super::model::DownloadStatus::Completed)
}

pub fn set_storage_used(bytes: u64) {
    DOWNLOADS.write().storage_used_bytes = bytes;
}

pub fn set_sync_running(running: bool) {
    DOWNLOADS.write().sync_running = running;
}

pub fn set_last_sync(now: u64) {
    DOWNLOADS.write().last_sync_at = Some(now);
}
