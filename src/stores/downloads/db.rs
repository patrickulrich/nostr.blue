//! SQLite persistence for the downloads layer (native only).
//!
//! Follows the bible/quran offline pattern: no long-lived connection; each
//! operation reopens the DB inside `spawn_blocking` with WAL mode. The DB
//! lives at `data_dir()/nostr-blue/downloads.db`.

use super::model::{DownloadItem, DownloadStatus, MediaKind, PlaybackPosition, ShowCacheEntry};
use crate::stores::audio::music_player::MusicTrack;
use std::path::PathBuf;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS download_items (
    id TEXT PRIMARY KEY,
    media_kind TEXT NOT NULL,
    status TEXT NOT NULL,
    remote_url TEXT NOT NULL,
    file_path TEXT,
    bytes_downloaded INTEGER NOT NULL DEFAULT 0,
    bytes_total INTEGER,
    error TEXT,
    auto INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    completed_at INTEGER,
    track_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_download_items_status ON download_items(status);
CREATE INDEX IF NOT EXISTS idx_download_items_kind ON download_items(media_kind);

CREATE TABLE IF NOT EXISTS show_cache (
    show_key TEXT PRIMARY KEY,
    title TEXT,
    image TEXT,
    feed_url TEXT,
    auto_download INTEGER NOT NULL DEFAULT 0,
    last_synced_at INTEGER,
    last_episode_date INTEGER,
    sub_json TEXT
);

CREATE TABLE IF NOT EXISTS episode_cache (
    episode_key TEXT PRIMARY KEY,
    show_key TEXT NOT NULL,
    track_json TEXT NOT NULL,
    published_at INTEGER,
    downloaded INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_episode_cache_show ON episode_cache(show_key);

CREATE TABLE IF NOT EXISTS playback_positions (
    track_id TEXT PRIMARY KEY,
    position_secs REAL NOT NULL,
    duration_secs REAL,
    updated_at INTEGER NOT NULL
);
";

pub fn db_path() -> PathBuf {
    crate::platform::storage::data_dir()
        .join("nostr-blue")
        .join("downloads.db")
}

/// Open the database and ensure the schema exists. Called once at boot.
pub fn init_db() -> Result<(), String> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| e.to_string())?;
    conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
    Ok(())
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<DownloadItem> {
    let track_json: String = row.get(11)?;
    let track = serde_json::from_str::<MusicTrack>(&track_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let auto: i64 = row.get(7)?;
    // Column order (ITEM_COLUMNS):
    //   0 id, 1 media_kind, 2 status, 3 remote_url, 4 file_path,
    //   5 bytes_downloaded, 6 bytes_total, 7 auto, 8 created_at,
    //   9 completed_at, 10 error, 11 track_json
    Ok(DownloadItem {
        id: row.get(0)?,
        kind: MediaKind::from_str_value(&row.get::<_, String>(1)?),
        status: DownloadStatus::from_str_value(&row.get::<_, String>(2)?),
        remote_url: row.get(3)?,
        file_path: row.get(4)?,
        bytes_downloaded: row.get::<_, i64>(5)?.max(0) as u64,
        bytes_total: row.get::<_, Option<i64>>(6)?.map(|v| v.max(0) as u64),
        auto: auto != 0,
        created_at: row.get::<_, i64>(8)?.max(0) as u64,
        completed_at: row.get::<_, Option<i64>>(9)?.map(|v| v.max(0) as u64),
        error: row.get(10)?,
        track,
    })
}

const ITEM_COLUMNS: &str = "id, media_kind, status, remote_url, file_path, bytes_downloaded, bytes_total, auto, created_at, completed_at, error, track_json";

pub async fn upsert_item(item: &DownloadItem) -> Result<(), String> {
    let item = item.clone();
    let path = db_path();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO download_items \
             (id, media_kind, status, remote_url, file_path, bytes_downloaded, bytes_total, auto, created_at, completed_at, error, track_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                item.id,
                item.kind.as_str(),
                item.status.as_str(),
                item.remote_url,
                item.file_path,
                item.bytes_downloaded as i64,
                item.bytes_total.map(|v| v as i64),
                item.auto as i64,
                item.created_at as i64,
                item.completed_at.map(|v| v as i64),
                item.error,
                serde_json::to_string(&item.track).map_err(|e| e.to_string())?,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn get_items() -> Result<Vec<DownloadItem>, String> {
    let path = db_path();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(&format!("SELECT {ITEM_COLUMNS} FROM download_items"))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], row_to_item)
            .map_err(|e| e.to_string())?;
        // Parse failures must be visible: silently dropping rows here once
        // produced "storage used but empty list" bugs.
        let mut items = Vec::new();
        for row in rows {
            match row {
                Ok(item) => items.push(item),
                Err(e) => log::error!("downloads.db: skipping malformed row: {}", e),
            }
        }
        Ok(items)
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn delete_item(id: &str) -> Result<(), String> {
    let id = id.to_string();
    let path = db_path();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM download_items WHERE id = ?1", rusqlite::params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn upsert_show(entry: &ShowCacheEntry, sub_json: Option<&str>) -> Result<(), String> {
    let entry = entry.clone();
    let sub_json = sub_json.map(|s| s.to_string());
    let path = db_path();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO show_cache \
             (show_key, title, image, feed_url, auto_download, last_synced_at, last_episode_date, sub_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                entry.show_key,
                entry.title,
                entry.image,
                entry.feed_url,
                entry.auto_download as i64,
                entry.last_synced_at.map(|v| v as i64),
                entry.last_episode_date.map(|v| v as i64),
                sub_json,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn get_shows() -> Result<Vec<(ShowCacheEntry, Option<String>)>, String> {
    let path = db_path();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT show_key, title, image, feed_url, auto_download, last_synced_at, last_episode_date, sub_json FROM show_cache")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    ShowCacheEntry {
                        show_key: row.get(0)?,
                        title: row.get(1)?,
                        image: row.get(2)?,
                        feed_url: row.get(3)?,
                        auto_download: row.get::<_, i64>(4)? != 0,
                        last_synced_at: row.get::<_, Option<i64>>(5)?.map(|v| v.max(0) as u64),
                        last_episode_date: row.get::<_, Option<i64>>(6)?.map(|v| v.max(0) as u64),
                    },
                    row.get::<_, Option<String>>(7)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn upsert_episode(
    show_key: &str,
    track: &MusicTrack,
    published_at: Option<u64>,
) -> Result<(), String> {
    let show_key = show_key.to_string();
    let track_json = serde_json::to_string(track).map_err(|e| e.to_string())?;
    let episode_key = track.id.clone();
    let path = db_path();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO episode_cache (episode_key, show_key, track_json, published_at, downloaded) \
             VALUES (?1, ?2, ?3, ?4, \
                COALESCE((SELECT downloaded FROM episode_cache WHERE episode_key = ?1), 0))",
            rusqlite::params![episode_key, show_key, track_json, published_at.map(|v| v as i64)],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn mark_episode_downloaded(episode_key: &str, downloaded: bool) -> Result<(), String> {
    let episode_key = episode_key.to_string();
    let path = db_path();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE episode_cache SET downloaded = ?2 WHERE episode_key = ?1",
            rusqlite::params![episode_key, downloaded as i64],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Episodes cached for a show (offline browse API).
#[allow(dead_code)]
pub async fn get_episodes(show_key: &str) -> Result<Vec<MusicTrack>, String> {
    let show_key = show_key.to_string();
    let path = db_path();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT track_json FROM episode_cache WHERE show_key = ?1 \
                 ORDER BY COALESCE(published_at, 0) DESC LIMIT 200",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![show_key], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| e.to_string())?;
        Ok(rows
            .filter_map(|r| r.ok())
            .filter_map(|json| serde_json::from_str(&json).ok())
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn save_position(
    track_id: &str,
    position_secs: f64,
    duration_secs: Option<f64>,
) -> Result<(), String> {
    let track_id = track_id.to_string();
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let path = db_path();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO playback_positions (track_id, position_secs, duration_secs, updated_at) \
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![track_id, position_secs, duration_secs, now as i64],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn get_position(track_id: &str) -> Result<Option<PlaybackPosition>, String> {
    let track_id = track_id.to_string();
    let path = db_path();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT position_secs, duration_secs, updated_at FROM playback_positions WHERE track_id = ?1",
            rusqlite::params![track_id],
            |row| {
                Ok(PlaybackPosition {
                    position_secs: row.get(0)?,
                    duration_secs: row.get(1)?,
                    updated_at: row.get::<_, i64>(2)?.max(0) as u64,
                })
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.to_string()),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Most recently updated positions, newest first. Reserved for
/// continue-listening surfaces beyond the current resume-on-play behavior.
#[allow(dead_code)]
pub async fn recent_positions(limit: u32) -> Result<Vec<(String, PlaybackPosition)>, String> {
    let path = db_path();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT track_id, position_secs, duration_secs, updated_at FROM playback_positions \
                 ORDER BY updated_at DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    PlaybackPosition {
                        position_secs: row.get(1)?,
                        duration_secs: row.get(2)?,
                        updated_at: row.get::<_, i64>(3)?.max(0) as u64,
                    },
                ))
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn delete_position(track_id: &str) -> Result<(), String> {
    let track_id = track_id.to_string();
    let path = db_path();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM playback_positions WHERE track_id = ?1",
            rusqlite::params![track_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
