//! Download engine (native only): DB-driven queue with a background worker.
//!
//! - Max 2 concurrent downloads (semaphore-free: an atomic active counter).
//! - Streaming reqwest → `.tmp` file → atomic rename on completion.
//! - Pause/resume with HTTP `Range` continuation when the server allows it.
//! - Cooperative cancellation via per-item atomic flags.
//! - Wi-Fi-only gating on Android (`ConnectivityManager.isActiveNetworkMetered`).
//! - Per-show retention during sync (Keep-per-show setting) evicts the
//!   oldest AUTO-downloaded episodes; storage is otherwise manually managed.
//! - Progress is throttled into the reactive store for UI, and periodically
//!   flushed to SQLite so restarts resume cleanly.

use super::model::{
    extension_for_url, safe_component, show_key_for_track, DownloadItem, DownloadStatus,
    MediaKind,
};
use crate::stores::audio::music_player::MusicTrack;
use dioxus::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const MAX_CONCURRENT: usize = 2;
/// Cancel flag values.
const CANCEL_RUN: u8 = 0;
const CANCEL_PAUSE: u8 = 1;
const CANCEL_DELETE: u8 = 2;
/// Throttle: minimum ms between store progress updates.
const STORE_UPDATE_MS: u64 = 400;
/// Throttle: minimum ms between DB progress flushes.
const DB_FLUSH_MS: u64 = 2500;
/// When metered, hold off re-picking queued items for this long.
const METERED_HOLD_MS: u64 = 60_000;

static WORKER_STARTED: OnceLock<()> = OnceLock::new();
static WAKE_TX: OnceLock<tokio::sync::mpsc::UnboundedSender<()>> = OnceLock::new();
static ACTIVE_COUNT: AtomicUsize = AtomicUsize::new(0);
static METERED_HOLD_UNTIL_MS: AtomicU64 = AtomicU64::new(0);

type CancelFlag = std::sync::Arc<AtomicU8>;
static CANCEL_FLAGS: Mutex<Option<HashMap<String, CancelFlag>>> = Mutex::new(None);

fn with_cancel_registry<R>(f: impl FnOnce(&mut HashMap<String, CancelFlag>) -> R) -> R {
    let mut guard = CANCEL_FLAGS.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

fn now_secs() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

fn now_ms() -> u64 {
    chrono::Utc::now()
        .timestamp_millis()
        .max(0)
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(all(target_os = "android", feature = "mobile_platform"))]
fn network_metered() -> bool {
    crate::platform::mobile::is_network_metered()
}

#[cfg(not(all(target_os = "android", feature = "mobile_platform")))]
fn network_metered() -> bool {
    false
}

fn download_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            // Per-read stall bound (not a total deadline): large media
            // transfers may legitimately run for minutes, but a server that
            // stops sending must not hold one of the MAX_CONCURRENT slots
            // forever.
            .read_timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build download client")
    })
}

/// Relative file path (posix separators) for a track under the media root.
pub fn file_path_for(track: &MusicTrack) -> String {
    let ext = extension_for_url(&track.media_url);
    let name = format!("{}.{}", safe_component(&track.id), ext);
    let kind = MediaKind::for_track(track);
    match (kind, show_key_for_track(track)) {
        (MediaKind::Podcast, Some(show)) => {
            format!("podcasts/{}/{}", safe_component(&show), name)
        }
        _ => format!("music/{name}"),
    }
}

fn absolute_media_path(rel: &str) -> PathBuf {
    super::resolver::media_dir().join(rel)
}

fn tmp_path_for(rel: &str) -> PathBuf {
    let mut s = std::path::PathBuf::from(rel).into_os_string();
    s.push(".tmp");
    super::resolver::media_dir().join(s)
}

// ---------------------------------------------------------------------------
// Boot + public API
// ---------------------------------------------------------------------------

/// Spawn an engine task on the Dioxus runtime.
///
/// Tasks that touch Dioxus `Global` signals MUST be polled with the Dioxus
/// runtime context (it is thread-local). `tokio::spawn` polls on worker
/// threads without it, panicking at `Runtime::current()` (verified against
/// an Android tombstone). `spawn_forever` attaches the task to the app
/// runtime — the documented pattern for long-lived background engines —
/// and the catch_unwind wrapper logs panics instead of aborting (same as
/// the user_prefs sidecar). Pure-IO closures (SQLite writes, file deletes)
/// stay on `spawn_detached`.
pub(crate) fn spawn_engine<F>(name: &'static str, future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    crate::platform::spawn::spawn_forever_catch_unwind(name, future);
}

/// Called once at boot: hydrate the store from the DB and start the worker.
pub fn spawn_loader() {
    spawn_engine("downloads-loader", async move {
        match super::db::get_items().await {
            Ok(mut items) => {
                // A "downloading" status at boot means we were interrupted.
                let mut restored = false;
                for item in items.iter_mut() {
                    if item.status == DownloadStatus::Downloading {
                        item.status = DownloadStatus::Queued;
                        restored = true;
                    }
                    // Drop entries whose files vanished (e.g. user cleared app data).
                    if item.status == DownloadStatus::Completed {
                        let exists = item
                            .file_path
                            .as_deref()
                            .is_some_and(|rel| absolute_media_path(rel).is_file());
                        if !exists {
                            item.status = DownloadStatus::Failed;
                            item.error = Some("File missing".to_string());
                            restored = true;
                        }
                    }
                }
                let used = compute_storage_used().await;
                {
                    let mut state = super::store::DOWNLOADS.write();
                    state.items = items;
                    state.storage_used_bytes = used;
                }
                // The boot-time queue restore (init_player) runs before this
                // loader finishes, so re-resolve local media for the restored
                // queue now (only meaningful while playback has not started).
                {
                    let player = crate::stores::audio::music_player::MUSIC_PLAYER.read();
                    if !player.is_playing && !player.playlist.is_empty() {
                        let mut playlist = player.playlist.clone();
                        super::resolver::rewrite_playlist(&mut playlist);
                        drop(player);
                        let mut player =
                            crate::stores::audio::music_player::MUSIC_PLAYER.write();
                        player.playlist = playlist;
                        let idx = player
                            .current_index
                            .min(player.playlist.len().saturating_sub(1));
                        player.current_track = player.playlist.get(idx).cloned();
                    }
                }
                if restored {
                    let snapshot = super::store::DOWNLOADS.read().items.clone();
                    for item in snapshot {
                        let _ = super::db::upsert_item(&item).await;
                    }
                }
            }
            Err(e) => log::error!("Failed to load downloads from DB: {}", e),
        }
        ensure_worker();
        wake_worker();
    });
}

/// Enqueue a track for download. Returns `false` when it cannot be enqueued
/// (live stream, empty URL, already known).
pub fn enqueue(track: &MusicTrack, auto: bool) -> bool {
    if track.is_live_stream || track.media_url.is_empty() {
        return false;
    }
    if !track.media_url.starts_with("http") {
        return false;
    }
    {
        let state = super::store::DOWNLOADS.read();
        if let Some(existing) = state.items.iter().find(|i| i.id == track.id) {
            // Allow re-enqueueing terminal states (retry path rewrites auto).
            if existing.status != DownloadStatus::Failed {
                return false;
            }
        }
    }
    let item = DownloadItem {
        id: track.id.clone(),
        kind: MediaKind::for_track(track),
        status: DownloadStatus::Queued,
        remote_url: track.media_url.clone(),
        file_path: Some(file_path_for(track)),
        bytes_downloaded: 0,
        bytes_total: None,
        error: None,
        auto,
        created_at: now_secs(),
        completed_at: None,
        track: track.clone(),
    };
    super::store::upsert_item_in_store(item.clone());
    let item_for_db = item;
    crate::platform::spawn::spawn_detached(async move {
        let _ = super::db::upsert_item(&item_for_db).await;
    });
    ensure_worker();
    wake_worker();
    true
}

pub fn pause(id: &str) {
    let active = request_cancel(id, CANCEL_PAUSE);
    if !active {
        // Not currently transferring: flip Queued (or a stale Downloading
        // marker) straight to Paused so the worker stops picking it up.
        let mut updated = None;
        {
            let mut state = super::store::DOWNLOADS.write();
            for item in state.items.iter_mut() {
                if item.id == id
                    && matches!(
                        item.status,
                        DownloadStatus::Queued | DownloadStatus::Downloading
                    )
                {
                    item.status = DownloadStatus::Paused;
                    updated = Some(item.clone());
                }
            }
        }
        if let Some(item) = updated {
            crate::platform::spawn::spawn_detached(async move {
                let _ = super::db::upsert_item(&item).await;
            });
        }
    }
}

pub fn retry(id: &str) {
    set_status(id, DownloadStatus::Queued, None);
}

pub fn resume(id: &str) {
    set_status(id, DownloadStatus::Queued, None);
}

pub fn delete(id: &str) {
    let active = request_cancel(id, CANCEL_DELETE);
    if !active {
        delete_now(id);
    }
}

pub fn pause_all() {
    let ids: Vec<String> = super::store::DOWNLOADS
        .read()
        .items
        .iter()
        .filter(|i| {
            matches!(
                i.status,
                DownloadStatus::Queued | DownloadStatus::Downloading
            )
        })
        .map(|i| i.id.clone())
        .collect();
    for id in ids {
        pause(&id);
    }
}

pub fn resume_all() {
    let ids: Vec<String> = super::store::DOWNLOADS
        .read()
        .items
        .iter()
        .filter(|i| i.status == DownloadStatus::Paused)
        .map(|i| i.id.clone())
        .collect();
    for id in ids {
        resume(&id);
    }
}

/// Delete every downloaded file and row (settings page "Clear all").
pub fn delete_all() {
    let ids: Vec<String> = super::store::DOWNLOADS
        .read()
        .items
        .iter()
        .map(|i| i.id.clone())
        .collect();
    for id in ids {
        delete(&id);
    }
}

/// Nudge the worker (e.g. after settings changed).
pub fn wake_worker() {
    ensure_worker();
    if let Some(tx) = WAKE_TX.get() {
        let _ = tx.send(());
    }
}

// ---------------------------------------------------------------------------
// Status plumbing
// ---------------------------------------------------------------------------

fn set_status(id: &str, status: DownloadStatus, error: Option<String>) {
    let mut updated = None;
    {
        let mut state = super::store::DOWNLOADS.write();
        for item in state.items.iter_mut() {
            if item.id == id {
                item.status = status;
                item.error = error.clone();
                if status == DownloadStatus::Queued {
                    item.error = None;
                }
                updated = Some(item.clone());
            }
        }
    }
    if let Some(item) = updated {
        crate::platform::spawn::spawn_detached(async move {
            let _ = super::db::upsert_item(&item).await;
        });
    }
    wake_worker();
}

/// Request cancellation of an active download. Returns whether it was active.
fn request_cancel(id: &str, mode: u8) -> bool {
    with_cancel_registry(|registry| match registry.get(id) {
        Some(flag) => {
            flag.store(mode, Ordering::SeqCst);
            true
        }
        None => false,
    })
}

fn delete_now(id: &str) {
    let removed = {
        let state = super::store::DOWNLOADS.read();
        state.items.iter().find(|i| i.id == id).cloned()
    };
    let Some(item) = removed else {
        return;
    };
    super::store::remove_item_from_store(id);
    let rel = item.file_path.clone();
    let id_for_db = id.to_string();
    // Touches signals at the tail (refresh_storage_used + browse cache), so
    // it must run on the Dioxus runtime.
    spawn_engine("downloads-delete", async move {
        let _ = super::db::delete_item(&id_for_db).await;
        let _ = super::db::mark_episode_downloaded(&id_for_db, false).await;
        if let Some(rel) = rel {
            let _ = tokio::fs::remove_file(absolute_media_path(&rel)).await;
            let _ = tokio::fs::remove_file(tmp_path_for(&rel)).await;
        }
        refresh_storage_used().await;
        refresh_downloads_browse_cache();
    });
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

fn ensure_worker() {
    if WORKER_STARTED.get().is_some() {
        return;
    }
    if WORKER_STARTED.set(()).is_err() {
        return;
    }
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let _ = WAKE_TX.set(tx);
    spawn_engine("downloads-worker", async move {
        loop {
            let tick = tokio::time::sleep(Duration::from_secs(5));
            tokio::select! {
                _ = rx.recv() => {}
                _ = tick => {}
            }
            run_tick().await;
        }
    });
}

async fn run_tick() {
    if !super::store::DOWNLOADS.read().settings.enabled {
        return;
    }
    if now_ms() < METERED_HOLD_UNTIL_MS.load(Ordering::SeqCst) {
        return;
    }
    // Wi-Fi gate: when metered and wifi_only, hold off entirely.
    let settings = super::store::DOWNLOADS.read().settings.clone();
    if settings.wifi_only && network_metered() {
        METERED_HOLD_UNTIL_MS
            .store(now_ms() + METERED_HOLD_MS, Ordering::SeqCst);
        return;
    }
    loop {
        let active = ACTIVE_COUNT.load(Ordering::SeqCst);
        if active >= MAX_CONCURRENT {
            return;
        }
        let next = {
            let state = super::store::DOWNLOADS.read();
            state
                .items
                .iter()
                .find(|i| i.status == DownloadStatus::Queued)
                .cloned()
        };
        let Some(item) = next else {
            return;
        };
        ACTIVE_COUNT.fetch_add(1, Ordering::SeqCst);
        let id = item.id.clone();
        spawn_engine("downloads-transfer", async move {
            run_download(&id).await;
            ACTIVE_COUNT.fetch_sub(1, Ordering::SeqCst);
            wake_worker();
        });
    }
}

/// How to proceed after a resume request response, classified from
/// (status, current offset, whether the no-range retry already happened).
#[derive(Debug, PartialEq, Eq)]
enum ResumeAction {
    /// Stream this response body, appending when `partial` (206).
    Stream { partial: bool },
    /// 2xx that ignored the `Range` header: discard the stale partial and
    /// stream this full-body response from byte 0.
    DiscardPartialAndStream,
    /// 416 with a resume offset: discard the partial and re-request once
    /// WITHOUT a `Range` header. Streaming the 416 error body into the
    /// media file (the old behavior) silently corrupted "completed"
    /// downloads.
    RetryWithoutRange,
    /// Fail the item with this message.
    Fail(String),
}

fn classify_resume_response(
    status: reqwest::StatusCode,
    start_offset: u64,
    retried_without_range: bool,
) -> ResumeAction {
    use reqwest::StatusCode;
    if status == StatusCode::PARTIAL_CONTENT {
        return ResumeAction::Stream { partial: true };
    }
    if status == StatusCode::RANGE_NOT_SATISFIABLE {
        // The server cannot serve from our offset (stale/changed upstream
        // file). Retry once from byte 0 without a Range header; a second
        // 416 — or one on a fresh start — is a hard failure.
        if start_offset > 0 && !retried_without_range {
            return ResumeAction::RetryWithoutRange;
        }
        return ResumeAction::Fail(format!("HTTP {status}"));
    }
    if status.is_success() {
        // Full-body 2xx: when a partial existed the server ignored the
        // range; truncate before streaming so the file only holds the new
        // body.
        if start_offset > 0 {
            return ResumeAction::DiscardPartialAndStream;
        }
        return ResumeAction::Stream { partial: false };
    }
    ResumeAction::Fail(format!("HTTP {status}"))
}

/// Delete the partial temp file and reopen it fresh (append mode), so a
/// restarted download only holds the new bytes. The caller must drop its
/// old file handle first.
async fn truncate_and_reopen(
    tmp_path: &std::path::Path,
) -> Result<tokio::fs::File, String> {
    let _ = tokio::fs::remove_file(tmp_path).await;
    tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(tmp_path)
        .await
        .map_err(|e| format!("Failed to reopen temp file: {e}"))
}

async fn run_download(id: &str) {
    let item = match super::store::get_item_from_store(id) {
        Some(item) => item,
        None => return,
    };
    let Some(rel_path) = item.file_path.clone() else {
        set_status(id, DownloadStatus::Failed, Some("No file path".into()));
        return;
    };
    let flag = std::sync::Arc::new(AtomicU8::new(CANCEL_RUN));
    with_cancel_registry(|registry| registry.insert(id.to_string(), flag.clone()));

    // A racing pause() may have flipped the item to Paused between the
    // worker pick and this point (before the cancel flag was registered).
    // The store is the source of truth: only proceed while still Queued.
    let still_queued = {
        let state = super::store::DOWNLOADS.read();
        state
            .items
            .iter()
            .any(|i| i.id == id && i.status == DownloadStatus::Queued)
    };
    if !still_queued {
        cleanup_cancel(id);
        return;
    }
    set_status(id, DownloadStatus::Downloading, None);

    let final_path = absolute_media_path(&rel_path);
    let tmp_path = tmp_path_for(&rel_path);
    if let Some(parent) = final_path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }

    let mut start_offset: u64;
    let mut file = match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&tmp_path)
        .await
    {
        Ok(file) => file,
        Err(e) => {
            finish_with_error(id, format!("Failed to open temp file: {e}"));
            cleanup_cancel(id);
            return;
        }
    };
    start_offset = file.metadata().await.map(|meta| meta.len()).unwrap_or(0);

    // Request phase: honor a resume `Range` when a partial exists, with
    // explicit 416 handling (retry once from byte 0 without the header) and
    // range-ignoring 2xx handling (truncate, then stream the full body).
    let mut retried_without_range = false;
    let (response, partial) = loop {
        let mut request = download_client().get(&item.remote_url);
        if start_offset > 0 {
            request = request.header("Range", format!("bytes={start_offset}-"));
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(e) => {
                finish_with_error(id, format!("Network error: {e}"));
                cleanup_cancel(id);
                return;
            }
        };
        match classify_resume_response(response.status(), start_offset, retried_without_range) {
            ResumeAction::Stream { partial } => break (response, partial),
            ResumeAction::DiscardPartialAndStream => {
                drop(file);
                file = match truncate_and_reopen(&tmp_path).await {
                    Ok(f) => f,
                    Err(e) => {
                        finish_with_error(id, e);
                        cleanup_cancel(id);
                        return;
                    }
                };
                start_offset = 0;
                break (response, false);
            }
            ResumeAction::RetryWithoutRange => {
                drop(file);
                file = match truncate_and_reopen(&tmp_path).await {
                    Ok(f) => f,
                    Err(e) => {
                        finish_with_error(id, e);
                        cleanup_cancel(id);
                        return;
                    }
                };
                start_offset = 0;
                retried_without_range = true;
            }
            ResumeAction::Fail(msg) => {
                finish_with_error(id, msg);
                cleanup_cancel(id);
                return;
            }
        }
    };
    let total_from_server: Option<u64> = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(|len| len + if partial { start_offset } else { 0 })
        .or(item.bytes_total);

    update_item(id, |stored| {
        stored.bytes_downloaded = start_offset;
        stored.bytes_total = total_from_server;
    });

    let mut downloaded = start_offset;
    let mut last_store_ms: u64 = 0;
    let mut last_db_ms: u64 = 0;
    let mut stream = response.bytes_stream();
    use futures::StreamExt;
    let mut io_error: Option<String> = None;
    let mut cancelled: Option<u8> = None;
    while let Some(chunk) = stream.next().await {
        let cancel = flag.load(Ordering::SeqCst);
        if cancel != CANCEL_RUN {
            cancelled = Some(cancel);
            break;
        }
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(e) => {
                io_error = Some(format!("Stream error: {e}"));
                break;
            }
        };
        if let Err(e) = file.write_all(&chunk).await {
            io_error = Some(format!("Disk write error: {e}"));
            break;
        }
        downloaded += chunk.len() as u64;
        let now = now_ms();
        if now.saturating_sub(last_store_ms) >= STORE_UPDATE_MS {
            last_store_ms = now;
            update_item(id, |stored| {
                stored.bytes_downloaded = downloaded;
                stored.bytes_total = total_from_server;
            });
        }
        if now.saturating_sub(last_db_ms) >= DB_FLUSH_MS {
            last_db_ms = now;
            flush_item_to_db(id);
        }
    }
    drop(file);

    if let Some(mode) = cancelled {
        match mode {
            CANCEL_DELETE => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                delete_now(id);
            }
            _ => {
                update_item(id, |stored| {
                    stored.bytes_downloaded = downloaded;
                    stored.status = DownloadStatus::Paused;
                });
                flush_item_to_db(id);
            }
        }
        cleanup_cancel(id);
        return;
    }
    if let Some(err) = io_error {
        update_item(id, |stored| {
            stored.bytes_downloaded = downloaded;
            stored.status = DownloadStatus::Failed;
            stored.error = Some(err.clone());
        });
        flush_item_to_db(id);
        cleanup_cancel(id);
        return;
    }

    if let Err(e) = tokio::fs::rename(&tmp_path, &final_path).await {
        finish_with_error(id, format!("Failed to finalize file: {e}"));
        let _ = tokio::fs::remove_file(&tmp_path).await;
        cleanup_cancel(id);
        return;
    }
    let completed_at = now_secs();
    update_item(id, |stored| {
        stored.status = DownloadStatus::Completed;
        stored.bytes_downloaded = downloaded;
        stored.completed_at = Some(completed_at);
        stored.error = None;
    });
    flush_item_to_db(id);
    let id_owned = id.to_string();
    crate::platform::spawn::spawn_detached(async move {
        let _ = super::db::mark_episode_downloaded(&id_owned, true).await;
    });
    refresh_storage_used().await;
    refresh_downloads_browse_cache();
    cleanup_cancel(id);
}

fn cleanup_cancel(id: &str) {
    with_cancel_registry(|registry| registry.remove(id));
}

fn finish_with_error(id: &str, message: String) {
    update_item(id, |stored| {
        stored.status = DownloadStatus::Failed;
        stored.error = Some(message.clone());
    });
    flush_item_to_db(id);
}

fn update_item(id: &str, mut apply: impl FnMut(&mut DownloadItem)) {
    let mut state = super::store::DOWNLOADS.write();
    for item in state.items.iter_mut() {
        if item.id == id {
            apply(item);
        }
    }
}

fn flush_item_to_db(id: &str) {
    if let Some(item) = super::store::get_item_from_store(id) {
        crate::platform::spawn::spawn_detached(async move {
            let _ = super::db::upsert_item(&item).await;
        });
    }
}

// ---------------------------------------------------------------------------
// Storage accounting + eviction
// ---------------------------------------------------------------------------

async fn compute_storage_used() -> u64 {
    let root = super::resolver::media_dir();
    tokio::task::spawn_blocking(move || dir_size(&root))
        .await
        .unwrap_or(0)
}

fn dir_size(dir: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            total += dir_size(&entry.path());
        } else {
            total += meta.len();
        }
    }
    total
}

async fn refresh_storage_used() {
    let used = compute_storage_used().await;
    super::store::set_storage_used(used);
}

// ---------------------------------------------------------------------------
// Android Auto browse cache mirror
// ---------------------------------------------------------------------------

/// Regenerate the "downloads" Android Auto browse cache from completed items.
fn refresh_downloads_browse_cache() {
    #[cfg(feature = "mobile_platform")]
    {
        let tracks: Vec<MusicTrack> = {
            let state = super::store::DOWNLOADS.read();
            state
                .items
                .iter()
                .filter(|i| i.status == DownloadStatus::Completed)
                .map(|i| {
                    let mut track = i.track.clone();
                    if let Some(rel) = &i.file_path {
                        track.media_url =
                            format!("file://{}", absolute_media_path(rel).display());
                    }
                    track
                })
                .collect()
        };
        if let Ok(json) = serde_json::to_string(&tracks) {
            crate::platform::spawn::spawn_detached(async move {
                let _ = crate::platform::android_media::save_browse_cache("downloads", &json);
            });
        }
    }
    #[cfg(not(feature = "mobile_platform"))]
    {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_206_streams_partial() {
        assert_eq!(
            classify_resume_response(reqwest::StatusCode::PARTIAL_CONTENT, 1024, false),
            ResumeAction::Stream { partial: true }
        );
        assert_eq!(
            classify_resume_response(reqwest::StatusCode::PARTIAL_CONTENT, 0, true),
            ResumeAction::Stream { partial: true }
        );
    }

    #[test]
    fn classify_200_with_offset_discards_partial() {
        assert_eq!(
            classify_resume_response(reqwest::StatusCode::OK, 500, false),
            ResumeAction::DiscardPartialAndStream
        );
    }

    #[test]
    fn classify_200_fresh_streams() {
        assert_eq!(
            classify_resume_response(reqwest::StatusCode::OK, 0, false),
            ResumeAction::Stream { partial: false }
        );
    }

    #[test]
    fn classify_416_retries_once_then_fails() {
        // First 416 with a resume offset: discard the partial and re-request
        // without the Range header (must not stream the error body).
        assert_eq!(
            classify_resume_response(reqwest::StatusCode::RANGE_NOT_SATISFIABLE, 500, false),
            ResumeAction::RetryWithoutRange
        );
        // A 416 after the no-range retry is a hard failure.
        assert_eq!(
            classify_resume_response(reqwest::StatusCode::RANGE_NOT_SATISFIABLE, 0, true),
            ResumeAction::Fail("HTTP 416 Range Not Satisfiable".to_string())
        );
        // A 416 on a fresh start (no Range sent) is a hard failure.
        assert_eq!(
            classify_resume_response(reqwest::StatusCode::RANGE_NOT_SATISFIABLE, 0, false),
            ResumeAction::Fail("HTTP 416 Range Not Satisfiable".to_string())
        );
    }

    #[test]
    fn classify_error_statuses_fail() {
        assert_eq!(
            classify_resume_response(reqwest::StatusCode::FORBIDDEN, 500, false),
            ResumeAction::Fail("HTTP 403 Forbidden".to_string())
        );
        assert_eq!(
            classify_resume_response(reqwest::StatusCode::NOT_FOUND, 0, false),
            ResumeAction::Fail("HTTP 404 Not Found".to_string())
        );
        assert_eq!(
            classify_resume_response(reqwest::StatusCode::INTERNAL_SERVER_ERROR, 0, true),
            ResumeAction::Fail("HTTP 500 Internal Server Error".to_string())
        );
    }
}
