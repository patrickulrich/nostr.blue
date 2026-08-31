//! Library sync + auto-download (native only).
//!
//! Refreshes show/episode metadata for RSS-subscribed podcasts directly from
//! their feeds (no NIP-98 dependency), caches it for offline browsing,
//! enqueues auto-downloads for new episodes, and mirrors the Android Auto
//! browse tree. Nostr podcasts and music tracks are cached write-through as
//! the user browses/plays them.

use super::model::{DownloadStatus, ShowCacheEntry};
use crate::stores::audio::music_player::MusicTrack;
use crate::stores::audio::podcast_subscription::{PodcastSubscription, PODCAST_SUBS};
use dioxus::prelude::*;

const SYNC_INTERVAL_SECS: u64 = 15 * 60;
const EPISODES_PER_SHOW: usize = 200;

fn now_secs() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

/// Hook installed in the app shell: periodically syncs the podcast library.
pub fn use_downloads_service() {
    use dioxus::prelude::*;
    use_future(move || async move {
        let mut last_sync: u64 = 0;
        loop {
            crate::platform::timer::sleep_ms(60_000).await;
            let subs_loaded = PODCAST_SUBS.read().loaded;
            let state = super::store::DOWNLOADS.read();
            let enabled = state.settings.enabled;
            drop(state);
            if !subs_loaded || !enabled {
                continue;
            }
            if last_sync != 0 && now_secs().saturating_sub(last_sync) < SYNC_INTERVAL_SECS {
                continue;
            }
            sync_all_shows().await;
            last_sync = now_secs();
        }
    });
}

/// Fetch every RSS subscription's feed and refresh caches + auto-downloads.
pub async fn sync_all_shows() {
    if super::store::DOWNLOADS.read().sync_running {
        return;
    }
    super::store::set_sync_running(true);
    let result = sync_all_shows_inner().await;
    super::store::set_sync_running(false);
    super::store::set_last_sync(now_secs());
    if let Err(e) = result {
        log::warn!("Podcast library sync finished with error: {}", e);
    }
}

async fn sync_all_shows_inner() -> Result<(), String> {
    let subscriptions: Vec<PodcastSubscription> = PODCAST_SUBS.read().subscriptions.clone();
    let mut failures = 0usize;
    for sub in &subscriptions {
        let Some(feed_url) = sub.feed_url.as_deref() else {
            continue;
        };
        let feed = match crate::services::podcast_rss::fetch_podcast_feed(feed_url).await {
            Ok(feed) => feed,
            Err(e) => {
                log::warn!("Sync: failed to fetch {}: {}", feed_url, e);
                failures += 1;
                continue;
            }
        };
        if let Err(e) = sync_show(sub, &feed).await {
            log::warn!("Sync: failed to ingest {}: {}", feed_url, e);
            failures += 1;
        }
    }
    mirror_subscriptions_browse_cache().await;
    if failures > 0 {
        Err(format!("{failures} feed(s) failed to sync"))
    } else {
        Ok(())
    }
}

async fn sync_show(
    sub: &PodcastSubscription,
    feed: &crate::services::podcast_rss::RssPodcast,
) -> Result<(), String> {
    let show_key = format!("rss:{}", feed.feed_url);
    let existing = super::db::get_shows()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|(entry, _)| entry.show_key == show_key)
        .map(|(entry, _)| entry);
    // Auto-download is purely a per-show decision (show-page toggle);
    // there is no global default to inherit.
    let auto_download = existing.as_ref().is_some_and(|e| e.auto_download);

    let mut newest_date = existing.as_ref().and_then(|e| e.last_episode_date).unwrap_or(0);
    let mut episodes: Vec<MusicTrack> = Vec::new();
    for episode in feed.episodes.iter().take(EPISODES_PER_SHOW) {
        let display = crate::components::podcast::episode_card::DisplayEpisode::from_rss_episode(
            episode, feed,
        );
        let track = display.to_music_track();
        let published = display.created_at;
        super::db::upsert_episode(&show_key, &track, (published > 0).then_some(published))
            .await?;
        episodes.push(track);
        if published > newest_date {
            newest_date = published;
        }
    }

    let entry = ShowCacheEntry {
        show_key: show_key.clone(),
        title: Some(feed.title.clone()),
        image: feed.image.clone(),
        feed_url: Some(feed.feed_url.clone()),
        auto_download,
        last_synced_at: Some(now_secs()),
        last_episode_date: Some(newest_date),
    };
    let sub_json = serde_json::to_string(sub).map_err(|e| e.to_string())?;
    super::db::upsert_show(&entry, Some(&sub_json)).await?;

    if auto_download {
        enqueue_newest_for_show(&show_key, &episodes).await;
        enforce_show_retention(&show_key).await;
    }

    mirror_show_browse_cache(sub, feed).await;
    Ok(())
}

/// Enqueue up to `settings.episodes_per_show` newest not-yet-known episodes
/// of a show (auto-flagged, so storage-cap LRU eviction may reclaim them).
async fn enqueue_newest_for_show(show_key: &str, episodes: &[MusicTrack]) {
    let settings = super::store::DOWNLOADS.read().settings.clone();
    if !settings.enabled || settings.episodes_per_show == 0 {
        return;
    }
    let mut candidates: Vec<&MusicTrack> = episodes
        .iter()
        .filter(|t| !t.is_live_stream && t.media_url.starts_with("http"))
        .collect();
    // Newest first (fall back to list order when dates are missing).
    candidates.sort_by_key(|t| std::cmp::Reverse(t.created_at.unwrap_or(0)));
    let known: std::collections::HashSet<String> = {
        let state = super::store::DOWNLOADS.read();
        state.items.iter().map(|i| i.id.clone()).collect()
    };
    let mut enqueued = 0usize;
    for track in candidates {
        if enqueued >= settings.episodes_per_show as usize {
            break;
        }
        if known.contains(&track.id) {
            continue;
        }
        if super::manager::enqueue(track, true) {
            enqueued += 1;
        }
    }
    if enqueued > 0 {
        log::info!(
            "Auto-download: enqueued {enqueued} episode(s) for {show_key} (target {})",
            settings.episodes_per_show
        );
    }
}

/// Retention: keep at most `settings.keep_per_show` newest AUTO-downloaded
/// episodes of a show; delete older auto items. Manual downloads are never
/// evicted here.
async fn enforce_show_retention(show_key: &str) {
    let keep = super::store::DOWNLOADS.read().settings.keep_per_show as usize;
    if keep == 0 {
        return;
    }
    let mut auto_items: Vec<super::model::DownloadItem> = {
        let state = super::store::DOWNLOADS.read();
        state
            .items
            .iter()
            .filter(|i| {
                i.auto
                    && super::model::show_key_for_track(&i.track)
                        .is_some_and(|k| k == show_key)
            })
            .cloned()
            .collect()
    };
    if auto_items.len() <= keep {
        return;
    }
    auto_items.sort_by_key(|i| {
        std::cmp::Reverse(i.track.created_at.unwrap_or_else(|| {
            i.completed_at.unwrap_or(i.created_at)
        }))
    });
    let victims: Vec<String> = auto_items
        .iter()
        .skip(keep)
        .map(|i| i.id.clone())
        .collect();
    log::info!(
        "Retention: evicting {} older auto-download(s) from {show_key} (keep {keep})",
        victims.len()
    );
    for id in victims {
        super::manager::delete(&id);
    }
}

/// Toggle auto-download for a show (show-page button) and act immediately:
/// ON  → record the flag, then fetch the feed and enqueue the newest
///       `episodes_per_show` episodes.
/// OFF → just record the flag; existing files stay (retention still applies
///       on the next sync if the flag is re-enabled; disabling stops future
///       enqueues only).
pub async fn toggle_show_auto_download(
    feed_url: &str,
    title: Option<&str>,
    image: Option<&str>,
    enable: bool,
) -> Result<bool, String> {
    let show_key = format!("rss:{feed_url}");
    let (existing_entry, existing_sub) = super::db::get_shows()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|(entry, _)| entry.show_key == show_key)
        .unwrap_or((ShowCacheEntry {
            show_key: show_key.clone(),
            title: title.map(String::from),
            image: image.map(String::from),
            feed_url: Some(feed_url.to_string()),
            auto_download: false,
            last_synced_at: None,
            last_episode_date: None,
        }, None));
    // Preserve an existing subscription blob (sync writes it); otherwise
    // look the show up in the subscription list so mirrors keep working.
    let sub_json = match existing_sub {
        Some(json) => Some(json),
        None => {
            let sub = PODCAST_SUBS
                .read()
                .subscriptions
                .iter()
                .find(|s| s.feed_url.as_deref() == Some(feed_url))
                .cloned();
            sub.and_then(|s| serde_json::to_string(&s).ok())
        }
    };
    let entry = ShowCacheEntry {
        auto_download: enable,
        ..existing_entry
    };
    super::db::upsert_show(&entry, sub_json.as_deref()).await?;

    if enable {
        let feed = crate::services::podcast_rss::fetch_podcast_feed(feed_url).await?;
        let episodes: Vec<MusicTrack> = feed
            .episodes
            .iter()
            .take(EPISODES_PER_SHOW)
            .map(|episode| {
                crate::components::podcast::episode_card::DisplayEpisode::from_rss_episode(
                    episode, &feed,
                )
                .to_music_track()
            })
            .collect();
        for track in &episodes {
            let published = track.created_at;
            let _ = super::db::upsert_episode(&show_key, track, published).await;
        }
        enqueue_newest_for_show(&show_key, &episodes).await;
        enforce_show_retention(&show_key).await;
    }
    Ok(enable)
}

/// Current auto-download flag for a show (show-page button state).
pub async fn show_auto_download_enabled(feed_url: &str) -> bool {
    let show_key = format!("rss:{feed_url}");
    super::db::get_shows()
        .await
        .unwrap_or_default()
        .into_iter()
        .any(|(entry, _)| entry.show_key == show_key && entry.auto_download)
}

/// Write-through cache for browsed/played tracks (nostr podcasts, music).
pub fn cache_track(track: &MusicTrack) {
    if track.is_live_stream || track.media_url.is_empty() {
        return;
    }
    let Some(show_key) = super::model::show_key_for_track(track) else {
        return;
    };
    if !show_key.starts_with("nostr:") {
        // RSS shows are cached by the sync pass; avoid double-writing here.
        return;
    }
    let track = track.clone();
    let published = track.created_at;
    // Signal read happens here (caller is on the Dioxus runtime thread);
    // the spawned closure stays pure-IO for tokio.
    let downloaded = super::store::is_downloaded(&track.id);
    crate::platform::spawn::spawn_detached(async move {
        let _ = super::db::upsert_episode(&show_key, &track, published).await;
        if downloaded {
            let _ = super::db::mark_episode_downloaded(&track.id, true).await;
        }
    });
}

// ---------------------------------------------------------------------------
// Offline library queries (UI)
// ---------------------------------------------------------------------------

/// Downloaded episodes grouped by show, sorted by most recent completion.
pub async fn downloaded_episodes_by_show() -> Vec<DownloadedShow> {
    let items: Vec<super::model::DownloadItem> = {
        let state = super::store::DOWNLOADS.read();
        state
            .items
            .iter()
            .filter(|i| {
                i.kind == super::model::MediaKind::Podcast
                    && i.status == DownloadStatus::Completed
            })
            .cloned()
            .collect()
    };
    group_by_show(items).await
}

/// Downloaded music tracks, newest completion first.
pub async fn downloaded_music() -> Vec<MusicTrack> {
    let mut items: Vec<super::model::DownloadItem> = {
        let state = super::store::DOWNLOADS.read();
        state
            .items
            .iter()
            .filter(|i| {
                i.kind == super::model::MediaKind::Music && i.status == DownloadStatus::Completed
            })
            .cloned()
            .collect()
    };
    items.sort_by_key(|item| std::cmp::Reverse(item.completed_at));
    items.into_iter().map(|i| i.track).collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct DownloadedShow {
    pub show_key: String,
    pub title: String,
    pub image: Option<String>,
    pub auto_download: bool,
    pub episode_count: usize,
    pub downloaded_count: usize,
    pub bytes: u64,
    pub latest_episode_at: Option<u64>,
    pub tracks: Vec<MusicTrack>,
}

async fn group_by_show(items: Vec<super::model::DownloadItem>) -> Vec<DownloadedShow> {
    let mut shows: Vec<DownloadedShow> = Vec::new();
    for item in items {
        let show_title = item.track.artist.clone();
        let image = item.track.album_art_url.clone();
        let latest = item.track.created_at;
        let show_key = super::model::show_key_for_track(&item.track)
            .unwrap_or_else(|| "podcasts".to_string());
        let entry = match shows.iter_mut().find(|s| s.show_key == show_key) {
            Some(entry) => entry,
            None => {
                shows.push(DownloadedShow {
                    show_key: show_key.clone(),
                    title: if show_title.is_empty() {
                        "Unknown Show".into()
                    } else {
                        show_title.clone()
                    },
                    image: image.clone(),
                    auto_download: false,
                    episode_count: 0,
                    downloaded_count: 0,
                    bytes: 0,
                    latest_episode_at: None,
                    tracks: Vec::new(),
                });
                shows.last_mut().expect("just pushed")
            }
        };
        entry.episode_count += 1;
        entry.downloaded_count += 1;
        entry.bytes += item.bytes_downloaded;
        entry.latest_episode_at = entry.latest_episode_at.max(latest);
        entry.tracks.push(item.track.clone());
    }
    shows
}

// ---------------------------------------------------------------------------
// Android Auto browse mirrors
// ---------------------------------------------------------------------------

/// Mirror a synced show's episodes into the Android Auto browse cache using
/// the same key scheme `MediaBrowseTree` subscribes with (`rss:{guid}` or
/// `rss:{numeric id}`).
async fn mirror_show_browse_cache(
    sub: &PodcastSubscription,
    feed: &crate::services::podcast_rss::RssPodcast,
) {
    #[cfg(feature = "mobile_platform")]
    {
        let key = browse_key_for_sub(sub);
        let tracks: Vec<MusicTrack> = feed
            .episodes
            .iter()
            .take(50)
            .map(|episode| {
                crate::components::podcast::episode_card::DisplayEpisode::from_rss_episode(
                    episode, feed,
                )
                .to_music_track()
            })
            .collect();
        if let Ok(json) = serde_json::to_string(&tracks) {
            let _ = crate::platform::android_media::save_browse_cache(&key, &json);
        }
    }
    #[cfg(not(feature = "mobile_platform"))]
    {
        let _ = (sub, feed);
    }
}

/// Mirror the subscription list itself (kept offline-fresh for Android Auto).
async fn mirror_subscriptions_browse_cache() {
    #[cfg(feature = "mobile_platform")]
    {
        let subs = PODCAST_SUBS.read().subscriptions.clone();
        if let Ok(json) = serde_json::to_string(&subs) {
            let _ = crate::platform::android_media::save_browse_cache("subscriptions", &json);
        }
    }
    #[cfg(not(feature = "mobile_platform"))]
    {}
}

#[cfg(feature = "mobile_platform")]
fn browse_key_for_sub(sub: &PodcastSubscription) -> String {
    if let Some(guid) = sub.podcast_guid.as_deref() {
        format!("episodes:rss:{guid}")
    } else if let Some(id) = sub.podcast_id {
        format!("episodes:rss:{id}")
    } else if let Some(url) = sub.feed_url.as_deref() {
        format!("episodes:rss:{url}")
    } else {
        "episodes:unknown".to_string()
    }
}
