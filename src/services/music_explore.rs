//! Music Explore orchestration.
//!
//! Merges Wavlake + native Nostr + Podcasting 2.0 (V4V) RSS tracks into unified
//! lists for the `/music` Explore tab and its sub-routes.
//!
//! Cross-platform: Wavlake and Nostr fetchers run unauthenticated on both WASM
//! and native. RSS (Podcast Index) calls are NIP-98 signer-gated and are only
//! attempted when a signer is attached; rows degrade gracefully for logged-out
//! users (the non-RSS sources still populate).

use crate::services::podcast_index::{self, PodcastFeed};
use crate::services::wavlake::{WavlakeAPI, WavlakeTrack};
use crate::stores::music_player::MusicTrack;
use crate::stores::nostr_client;
use crate::stores::nostr_music::{self, MusicFeedFilter, NostrPlaylist, NostrTrack};
use nostr_sdk::nips::nip19::Nip19;
use nostr_sdk::{Filter, FromBech32, Kind, Timestamp};
use std::collections::HashMap;
use std::time::Duration;

/// A unified album surfaced in the Explore "Albums" row.
///
/// Wavlake albums are derived from rankings (grouped by `album_id`); RSS albums
/// are Podcast Index feeds with `medium="music"`. Nostr has no album concept.
#[derive(Clone, Debug, PartialEq)]
pub enum ExploreAlbum {
    Wavlake {
        id: String,
        title: String,
        art_url: String,
        artist: String,
    },
    Rss {
        feed_id: u64,
        title: String,
        art_url: Option<String>,
        author: Option<String>,
    },
}

/// A unified artist surfaced in the Explore "Artists" row.
#[derive(Clone, Debug, PartialEq)]
pub enum ExploreArtist {
    Wavlake {
        id: String,
        name: String,
        art_url: String,
    },
    Nostr {
        pubkey: String,
    },
    Rss {
        name: String,
    },
}

/// Aggregated preview data for the Explore tab (one efficient pass per source).
#[derive(Clone, Debug, Default)]
pub struct ExploreOverview {
    pub songs: Vec<MusicTrack>,
    pub albums: Vec<ExploreAlbum>,
    pub playlists: Vec<NostrPlaylist>,
    pub artists: Vec<ExploreArtist>,
    pub listening: Vec<ListeningEntry>,
}

/// A NIP-38 (kind 30315, d "music") "now playing" status entry.
#[derive(Clone, Debug, PartialEq)]
pub struct ListeningEntry {
    pub pubkey: String,
    pub content: String,
    pub created_at: u64,
    /// Optional track coordinate ("36787:pubkey:d") when resolvable from an
    /// `a` tag or a `nostr:naddr` `r` tag — enables a Play button.
    pub coordinate: Option<String>,
}

// ---------------------------------------------------------------------------
// Internal building blocks (each isolates failures so one source down doesn't
// blank the whole row).
// ---------------------------------------------------------------------------

async fn wavlake_ranking_tracks(limit: usize, genre: Option<&str>) -> Vec<WavlakeTrack> {
    let api = WavlakeAPI::new();
    match api
        .get_rankings("release_date", None, None, None, genre, Some(limit as u32))
        .await
    {
        Ok(tracks) => tracks,
        Err(e) => {
            log::error!("Explore: Wavlake rankings fetch failed: {e}");
            Vec::new()
        }
    }
}

async fn nostr_tracks_safe(
    limit: usize,
    genre: Option<&str>,
    until: Option<Timestamp>,
) -> Vec<NostrTrack> {
    if nostr_client::get_client().is_none() {
        return Vec::new();
    }
    match nostr_music::fetch_nostr_tracks(MusicFeedFilter::All, limit, genre, until).await {
        Ok(tracks) => tracks,
        Err(e) => {
            log::error!("Explore: Nostr tracks fetch failed: {e}");
            Vec::new()
        }
    }
}

async fn nostr_playlists_safe(limit: usize, until: Option<Timestamp>) -> Vec<NostrPlaylist> {
    if nostr_client::get_client().is_none() {
        return Vec::new();
    }
    match nostr_music::fetch_playlists(None, limit, until).await {
        Ok(playlists) => playlists,
        Err(e) => {
            log::error!("Explore: Nostr playlists fetch failed: {e}");
            Vec::new()
        }
    }
}

/// Cheap chart fetch (no per-episode hydration). Returns the raw chart items,
/// empty if no signer is attached (the Podcast Index proxy is NIP-98 gated).
async fn chart_items_safe() -> Vec<podcast_index::V4VMusicChartItem> {
    if !nostr_client::has_signer() {
        return Vec::new();
    }
    match podcast_index::get_v4v_music_chart().await {
        Ok(chart) => chart.items,
        Err(e) => {
            log::error!("Explore: V4V chart fetch failed: {e}");
            Vec::new()
        }
    }
}

/// Hydrate chart items into playable `MusicTrack`s (one NIP-98 call per item).
async fn hydrate_chart_tracks(
    items: &[podcast_index::V4VMusicChartItem],
    limit: usize,
) -> Vec<MusicTrack> {
    let futures: Vec<_> = items
        .iter()
        .take(limit)
        .map(|item| {
            let item = item.clone();
            async move {
                match podcast_index::get_episode_by_guid(&item.item_guid, Some(&item.feed_guid))
                    .await
                {
                    Ok((episode, feed)) => {
                        let feed = feed.unwrap_or_else(|| fallback_feed(&item));
                        Some(MusicTrack::from_rss_music_track(
                            &episode,
                            &feed,
                            item.image.as_deref(),
                        ))
                    }
                    Err(e) => {
                        log::warn!(
                            "Explore: failed to hydrate chart episode {}: {e}",
                            item.rank
                        );
                        None
                    }
                }
            }
        })
        .collect();
    futures::future::join_all(futures)
        .await
        .into_iter()
        .flatten()
        .collect()
}

/// Build a minimal `PodcastFeed` from a chart item when the lookup returns none.
fn fallback_feed(item: &podcast_index::V4VMusicChartItem) -> PodcastFeed {
    PodcastFeed {
        id: item.feed_id,
        title: item.author.clone().unwrap_or_default(),
        url: item.feed_url.clone(),
        original_url: None,
        link: None,
        description: None,
        author: item.author.clone(),
        owner_name: None,
        image: item.image.clone(),
        artwork: None,
        language: None,
        itunes_id: None,
        podcast_guid: Some(item.feed_guid.clone()),
        episode_count: None,
        categories: None,
        trending_score: None,
        value: None,
    }
}

/// RSS music "albums" (Podcast Index feeds with medium="music"). Signer-gated.
async fn rss_music_albums_safe(limit: usize) -> Vec<PodcastFeed> {
    if !nostr_client::has_signer() {
        return Vec::new();
    }
    match podcast_index::get_music_albums(Some(limit as u32)).await {
        Ok(feeds) => feeds,
        Err(e) => {
            log::error!("Explore: RSS music albums fetch failed: {e}");
            Vec::new()
        }
    }
}

// ---------------------------------------------------------------------------
// Derivation helpers
// ---------------------------------------------------------------------------

fn derive_wavlake_albums(tracks: &[WavlakeTrack], limit: usize) -> Vec<ExploreAlbum> {
    let mut seen = std::collections::HashSet::new();
    let mut albums = Vec::new();
    for t in tracks {
        if t.album_id.is_empty() || !seen.insert(t.album_id.clone()) {
            continue;
        }
        albums.push(ExploreAlbum::Wavlake {
            id: t.album_id.clone(),
            title: t.album_title.clone(),
            art_url: t.album_art_url.clone(),
            artist: t.artist.clone(),
        });
        if albums.len() >= limit {
            break;
        }
    }
    albums
}

fn derive_wavlake_artists(tracks: &[WavlakeTrack], limit: usize) -> Vec<ExploreArtist> {
    let mut seen = std::collections::HashSet::new();
    let mut artists = Vec::new();
    for t in tracks {
        if t.artist_id.is_empty() || !seen.insert(t.artist_id.clone()) {
            continue;
        }
        artists.push(ExploreArtist::Wavlake {
            id: t.artist_id.clone(),
            name: t.artist.clone(),
            art_url: t.artist_art_url.clone(),
        });
        if artists.len() >= limit {
            break;
        }
    }
    artists
}

fn derive_nostr_artists(tracks: &[NostrTrack], limit: usize) -> Vec<ExploreArtist> {
    let mut seen = std::collections::HashSet::new();
    let mut artists = Vec::new();
    for t in tracks {
        if !seen.insert(t.pubkey.clone()) {
            continue;
        }
        artists.push(ExploreArtist::Nostr {
            pubkey: t.pubkey.clone(),
        });
        if artists.len() >= limit {
            break;
        }
    }
    artists
}

fn derive_rss_artists(
    items: &[podcast_index::V4VMusicChartItem],
    limit: usize,
) -> Vec<ExploreArtist> {
    let mut seen = std::collections::HashSet::new();
    let mut artists = Vec::new();
    for item in items {
        let Some(author) = item.author.as_deref().filter(|a| !a.is_empty()) else {
            continue;
        };
        if !seen.insert(author.to_string()) {
            continue;
        }
        artists.push(ExploreArtist::Rss {
            name: author.to_string(),
        });
        if artists.len() >= limit {
            break;
        }
    }
    artists
}

fn merge_songs(
    wavlake: Vec<WavlakeTrack>,
    nostr: Vec<NostrTrack>,
    rss: Vec<MusicTrack>,
    limit: usize,
) -> Vec<MusicTrack> {
    let mut all: Vec<MusicTrack> = Vec::with_capacity(wavlake.len() + nostr.len() + rss.len());
    all.extend(wavlake.into_iter().map(Into::into));
    all.extend(nostr.into_iter().map(Into::into));
    all.extend(rss);
    // Newest first.
    all.sort_by_key(|b| std::cmp::Reverse(b.created_at.unwrap_or(0)));
    all.truncate(limit);
    all
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetch all four Explore rows in one efficient pass (each source fetched once,
/// then derived into songs/albums/artists/playlists). `limit` caps each row.
pub async fn fetch_explore_overview(limit: usize) -> ExploreOverview {
    let wavlake = wavlake_ranking_tracks(limit, None).await;
    let nostr = nostr_tracks_safe(limit, None, None).await;
    let chart_items = chart_items_safe().await;

    // Hydrate chart tracks for the Songs row (signer-gated, no-op when empty).
    let rss_songs = hydrate_chart_tracks(&chart_items, limit).await;

    // Playlists are Nostr-only and independent.
    let playlists = nostr_playlists_safe(limit, None).await;

    // NIP-38 "now playing" statuses (global, public — no signer needed).
    let listening = fetch_listening_entries().await;

    let songs = merge_songs(wavlake.clone(), nostr.clone(), rss_songs, limit);

    // Albums: Wavlake-derived + RSS feeds.
    let mut albums = derive_wavlake_albums(&wavlake, limit);
    let rss_albums = rss_music_albums_safe(limit).await;
    for feed in rss_albums {
        let art_url = feed.get_image().map(String::from);
        albums.push(ExploreAlbum::Rss {
            feed_id: feed.id,
            title: feed.title,
            art_url,
            author: feed.author,
        });
        if albums.len() >= limit * 2 {
            break;
        }
    }

    // Artists: Wavlake-derived + Nostr pubkeys + RSS chart authors.
    let mut artists = derive_wavlake_artists(&wavlake, limit);
    artists.extend(derive_nostr_artists(&nostr, limit));
    artists.extend(derive_rss_artists(&chart_items, limit));
    artists.truncate(limit * 2);

    ExploreOverview {
        songs,
        albums,
        playlists,
        artists,
        listening,
    }
}

/// Full Songs list page-1 seed (all three sources, newest first) for
/// `/music/tracks`. Wavlake/RSS contribute once; Nostr grows via
/// `fetch_more_nostr_tracks`.
pub async fn fetch_explore_songs(limit: usize, genre: Option<&str>) -> Vec<MusicTrack> {
    let wavlake = wavlake_ranking_tracks(limit, genre).await;
    let nostr = nostr_tracks_safe(limit, genre, None).await;
    let chart_items = chart_items_safe().await;
    let rss_songs = hydrate_chart_tracks(&chart_items, limit).await;
    merge_songs(wavlake, nostr, rss_songs, limit)
}

/// Load more Songs — Nostr-only backward pagination (the only source that can
/// grow). Used by the `/music/tracks` infinite-scroll sentinel.
pub async fn fetch_more_nostr_tracks(
    limit: usize,
    genre: Option<&str>,
    until: Timestamp,
) -> Vec<MusicTrack> {
    nostr_tracks_safe(limit, genre, Some(until))
        .await
        .into_iter()
        .map(MusicTrack::from)
        .collect()
}

/// Full Albums list for `/music/albums`.
pub async fn fetch_explore_albums(limit: usize) -> Vec<ExploreAlbum> {
    let wavlake = wavlake_ranking_tracks(limit, None).await;
    let mut albums = derive_wavlake_albums(&wavlake, limit);
    let rss_albums = rss_music_albums_safe(limit).await;
    for feed in rss_albums {
        let art_url = feed.get_image().map(String::from);
        albums.push(ExploreAlbum::Rss {
            feed_id: feed.id,
            title: feed.title,
            art_url,
            author: feed.author,
        });
        if albums.len() >= limit * 2 {
            break;
        }
    }
    albums
}

/// Full Playlists list for `/music/playlists` (Nostr kind 34139), with a
/// backward-pagination cursor for infinite scroll.
pub async fn fetch_explore_playlists(limit: usize, until: Option<Timestamp>) -> Vec<NostrPlaylist> {
    nostr_playlists_safe(limit, until).await
}

/// Full Artists list for `/music/artists`.
pub async fn fetch_explore_artists(limit: usize) -> Vec<ExploreArtist> {
    let wavlake = wavlake_ranking_tracks(limit, None).await;
    // Fetch a deeper Nostr sample for artist derivation (unique pubkeys), since
    // artists are derived from tracks and music relays now hold real content.
    let nostr_sample = limit.max(150);
    let nostr = nostr_tracks_safe(nostr_sample, None, None).await;
    let chart_items = chart_items_safe().await;
    let mut artists = derive_wavlake_artists(&wavlake, limit);
    artists.extend(derive_nostr_artists(&nostr, nostr_sample));
    artists.extend(derive_rss_artists(&chart_items, limit));
    artists.truncate(nostr_sample * 2);
    artists
}

/// Fetch recent NIP-38 music "now playing" statuses (last hour, global).
/// Public (no signer required); deduped per listener (newest wins) and pruned
/// of expired entries client-side.
pub async fn fetch_listening_entries() -> Vec<ListeningEntry> {
    if nostr_client::get_client().is_none() {
        return Vec::new();
    }
    let since = Timestamp::now().as_secs().saturating_sub(3600);
    let filter = Filter::new()
        .kind(Kind::UserStatus)
        .identifier("music")
        .since(Timestamp::from(since))
        .limit(500);
    let events = fetch_events(filter).await;
    let now = Timestamp::now().as_secs();
    let mut seen: HashMap<String, ListeningEntry> = HashMap::new();
    for event in events {
        // Skip "clear" statuses (empty content per NIP-38).
        if event.content.trim().is_empty() {
            continue;
        }
        // Prune expired entries.
        if let Some(exp) = event.tags.expiration() {
            if exp.as_secs() <= now {
                continue;
            }
        }
        let pubkey = event.pubkey.to_hex();
        let created_at = event.created_at.as_secs();
        let coordinate = coordinate_from_event(&event);
        let entry = seen.entry(pubkey.clone()).or_insert(ListeningEntry {
            pubkey: pubkey.clone(),
            content: event.content.clone(),
            created_at,
            coordinate: coordinate.clone(),
        });
        if created_at >= entry.created_at {
            entry.content = event.content.clone();
            entry.created_at = created_at;
            entry.coordinate = coordinate;
        }
    }
    let mut entries: Vec<_> = seen.into_values().collect();
    entries.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    entries
}

/// Fetch events via the cross-platform aggregated path, degrading to empty.
async fn fetch_events(filter: Filter) -> Vec<nostr_sdk::Event> {
    nostr_client::fetch_events_aggregated(filter, Duration::from_secs(8))
        .await
        .unwrap_or_default()
}

/// Resolve an optional track coordinate from a 30315 event: prefers an `a` tag
/// (nostria-style), falls back to a `nostr:naddr` `r` tag (nostr.blue-style).
fn coordinate_from_event(event: &nostr_sdk::Event) -> Option<String> {
    for tag in event.tags.iter() {
        let tag_vec: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
        if let ["a", coordinate, ..] = tag_vec.as_slice() {
            return Some(coordinate.to_string());
        }
    }
    for tag in event.tags.iter() {
        let tag_vec: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
        if let ["r", reference, ..] = tag_vec.as_slice() {
            if let Some(naddr) = reference.strip_prefix("nostr:") {
                if let Some(coord) = coordinate_from_naddr(naddr) {
                    return Some(coord);
                }
            }
        }
    }
    None
}

/// Decode a bech32 naddr into a "kind:pubkey:identifier" coordinate string.
fn coordinate_from_naddr(naddr: &str) -> Option<String> {
    match Nip19::from_bech32(naddr).ok()? {
        Nip19::Coordinate(c) => Some(format!("{}:{}:{}", c.kind.as_u16(), c.public_key, c.identifier)),
        _ => None,
    }
}
