//! Podcast Index API Service
//!
//! Client for the Podcast Index proxy at podnostrblue.ulrich-patrickr.workers.dev
//! Provides podcast search, trending, categories, and episode discovery.
//! Uses NIP-98 HTTP Authentication for API access.

use futures::future::{select, Either};
use gloo_net::http::Request;
use nostr_sdk::nips::nip98;
use serde::{Deserialize, Serialize};

use crate::utils::nip98 as nip98_utils;
use crate::utils::validation::parse_http_url;

/// Timeout for proxy fetch requests (30 seconds)
const PROXY_TIMEOUT_MS: u32 = 30_000;

/// Base URL for the Podcast Index proxy
const API_BASE: &str = "https://podnostrblue.ulrich-patrickr.workers.dev";

// ============================================================================
// Authenticated Request Helper
// ============================================================================

/// Make an authenticated GET request to the Podcast Index proxy
async fn authenticated_get<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, String> {
    let auth_result = nip98_utils::create_auth_header(url, nip98::HttpMethod::GET).await?;

    // IMPORTANT: Use the signed_url for the request to ensure URL consistency
    // with the NIP-98 event. URL parsing can normalize URLs slightly differently.
    let response = Request::get(&auth_result.signed_url)
        .header("Authorization", &auth_result.header)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, body));
    }

    response.json().await.map_err(|e| format!("Parse error: {}", e))
}

// ============================================================================
// API Response Types
// ============================================================================

/// Generic API response wrapper
#[derive(Debug, Clone, Deserialize)]
pub struct ApiResponse<T> {
    #[allow(dead_code)]
    status: String,
    #[allow(dead_code)]
    #[serde(default)]
    count: Option<u32>,
    #[serde(flatten)]
    pub data: T,
}

/// Search response data
#[derive(Debug, Clone, Deserialize)]
pub struct SearchData {
    #[serde(default)]
    pub feeds: Vec<PodcastFeed>,
}

/// Trending response data
#[derive(Debug, Clone, Deserialize)]
pub struct TrendingData {
    #[serde(default)]
    pub feeds: Vec<PodcastFeed>,
}

/// Categories response data
#[derive(Debug, Clone, Deserialize)]
pub struct CategoriesData {
    #[serde(default)]
    pub feeds: Vec<Category>,
}

/// Episodes response data
#[derive(Debug, Clone, Deserialize)]
pub struct EpisodesData {
    #[serde(default)]
    pub items: Vec<Episode>,
}

/// Podcast feed from Podcast Index
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodcastFeed {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub original_url: Option<String>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub owner_name: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub artwork: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub itunes_id: Option<u64>,
    #[serde(default)]
    pub podcast_guid: Option<String>,
    #[serde(default)]
    pub episode_count: Option<u32>,
    #[serde(default)]
    pub categories: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub trending_score: Option<i32>,
    #[serde(default)]
    pub value: Option<ValueBlock>,
}

impl PodcastFeed {
    /// Get the best available image URL
    pub fn get_image(&self) -> Option<&str> {
        self.artwork.as_deref().or(self.image.as_deref())
    }

    /// Check if this podcast has V4V support
    #[allow(dead_code)]
    pub fn has_v4v(&self) -> bool {
        self.value.is_some()
    }

    /// Get RSS feed URL
    #[allow(dead_code)]
    pub fn feed_url(&self) -> &str {
        self.original_url.as_deref().unwrap_or(&self.url)
    }
}

/// Value block for V4V payments
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueBlock {
    #[serde(default)]
    pub model: Option<ValueModel>,
    #[serde(default)]
    pub destinations: Vec<ValueDestination>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueModel {
    #[serde(rename = "type")]
    pub model_type: Option<String>,
    pub method: Option<String>,
    pub suggested: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueDestination {
    pub name: Option<String>,
    pub address: Option<String>,
    #[serde(rename = "type")]
    pub dest_type: Option<String>,
    pub split: Option<u32>,
}

/// Category from Podcast Index
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id: u32,
    pub name: String,
}

/// Episode from Podcast Index
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub enclosure_url: Option<String>,
    #[serde(default)]
    pub enclosure_type: Option<String>,
    #[serde(default)]
    pub enclosure_length: Option<u64>,
    #[serde(default)]
    pub duration: Option<u64>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub feed_image: Option<String>,
    #[serde(default)]
    pub feed_id: Option<u64>,
    #[serde(default)]
    pub feed_title: Option<String>,
    #[serde(default)]
    pub feed_url: Option<String>,
    #[serde(default)]
    pub podcast_guid: Option<String>,
    #[serde(default)]
    pub date_published: Option<i64>,
    #[serde(default)]
    pub season: Option<u32>,
    #[serde(default)]
    pub episode: Option<u32>,
    #[serde(default)]
    pub transcripts: Vec<TranscriptInfo>,
    #[serde(default)]
    pub chapters_url: Option<String>,
    #[serde(default)]
    pub soundbites: Vec<SoundbiteInfo>,
    #[serde(default)]
    pub value: Option<ValueBlock>,
}

impl Episode {
    /// Get the best available image URL
    pub fn get_image(&self) -> Option<&str> {
        self.image.as_deref().or(self.feed_image.as_deref())
    }

    /// Check if this episode has V4V support
    #[allow(dead_code)]
    pub fn has_v4v(&self) -> bool {
        self.value.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptInfo {
    pub url: Option<String>,
    #[serde(rename = "type")]
    pub transcript_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundbiteInfo {
    pub title: Option<String>,
    pub start_time: Option<f64>,
    pub duration: Option<f64>,
}

// ============================================================================
// API Functions
// ============================================================================

/// Search podcasts by term
pub async fn search_podcasts(query: &str, max: Option<u32>) -> Result<Vec<PodcastFeed>, String> {
    let max = max.unwrap_or(20);
    let url = format!("{}/search/byterm?q={}&max={}", API_BASE, urlencoding::encode(query), max);

    let data: ApiResponse<SearchData> = authenticated_get(&url).await?;
    Ok(data.data.feeds)
}

/// Get trending podcasts
pub async fn get_trending(max: Option<u32>, category: Option<&str>) -> Result<Vec<PodcastFeed>, String> {
    let max = max.unwrap_or(20);
    let mut url = format!("{}/podcasts/trending?max={}", API_BASE, max);

    if let Some(cat) = category {
        url.push_str(&format!("&cat={}", urlencoding::encode(cat)));
    }

    let data: ApiResponse<TrendingData> = authenticated_get(&url).await?;
    Ok(data.data.feeds)
}

/// Get all categories
pub async fn get_categories() -> Result<Vec<Category>, String> {
    let url = format!("{}/categories/list", API_BASE);

    let data: ApiResponse<CategoriesData> = authenticated_get(&url).await?;
    Ok(data.data.feeds)
}

/// Get podcast by feed URL
pub async fn get_podcast_by_url(feed_url: &str) -> Result<PodcastFeed, String> {
    let url = format!("{}/podcasts/byfeedurl?url={}", API_BASE, urlencoding::encode(feed_url));

    #[derive(Deserialize)]
    struct SingleFeedResponse {
        feed: PodcastFeed,
    }

    let data: ApiResponse<SingleFeedResponse> = authenticated_get(&url).await?;
    Ok(data.data.feed)
}

/// Get podcast by Podcast Index ID
pub async fn get_podcast_by_id(feed_id: u64) -> Result<PodcastFeed, String> {
    let url = format!("{}/podcasts/byfeedid?id={}", API_BASE, feed_id);

    #[derive(Deserialize)]
    struct SingleFeedResponse {
        feed: PodcastFeed,
    }

    let data: ApiResponse<SingleFeedResponse> = authenticated_get(&url).await?;
    Ok(data.data.feed)
}

/// Get podcast by podcast GUID (NIP-73 podcast:guid: format)
pub async fn get_podcast_by_guid(guid: &str) -> Result<PodcastFeed, String> {
    let url = format!("{}/podcasts/byguid?guid={}", API_BASE, urlencoding::encode(guid));

    #[derive(Deserialize)]
    struct SingleFeedResponse {
        feed: PodcastFeed,
    }

    let data: ApiResponse<SingleFeedResponse> = authenticated_get(&url).await?;
    Ok(data.data.feed)
}

/// Get episode by episode GUID (NIP-73 podcast:item:guid: format)
/// Optionally provide the podcast GUID for more reliable lookups
pub async fn get_episode_by_guid(guid: &str, podcast_guid: Option<&str>) -> Result<(Episode, Option<PodcastFeed>), String> {
    let mut url = format!("{}/episodes/byguid?guid={}&fulltext", API_BASE, urlencoding::encode(guid));
    if let Some(pg) = podcast_guid {
        url.push_str(&format!("&podcastguid={}", urlencoding::encode(pg)));
    }
    log::info!("[podcast_index] get_episode_by_guid: fetching {}", url);

    #[derive(Deserialize)]
    struct EpisodeByGuidData {
        episode: Episode,
    }

    let data: ApiResponse<EpisodeByGuidData> = match authenticated_get(&url).await {
        Ok(d) => d,
        Err(e) => {
            log::error!("[podcast_index] get_episode_by_guid failed: {}", e);
            return Err(e);
        }
    };

    // Try to fetch the podcast info for context
    let podcast = if let Some(feed_id) = data.data.episode.feed_id {
        get_podcast_by_id(feed_id).await.ok()
    } else {
        None
    };

    Ok((data.data.episode, podcast))
}

/// Get episodes by podcast GUID
#[allow(dead_code)]
pub async fn get_episodes_by_podcast_guid(podcast_guid: &str, max: Option<u32>) -> Result<Vec<Episode>, String> {
    let max = max.unwrap_or(20);
    let url = format!("{}/episodes/bypodcastguid?guid={}&max={}&fulltext",
        API_BASE, urlencoding::encode(podcast_guid), max);

    let data: ApiResponse<EpisodesData> = authenticated_get(&url).await?;
    Ok(data.data.items)
}

/// Get episodes by feed ID
pub async fn get_episodes_by_feed_id(feed_id: u64, max: Option<u32>) -> Result<Vec<Episode>, String> {
    let max = max.unwrap_or(20);
    let url = format!("{}/episodes/byfeedid?id={}&max={}", API_BASE, feed_id, max);

    let data: ApiResponse<EpisodesData> = authenticated_get(&url).await?;
    Ok(data.data.items)
}

/// Get currently live podcast streams
pub async fn get_live_episodes(max: Option<u32>) -> Result<Vec<Episode>, String> {
    let max = max.unwrap_or(20);
    let url = format!("{}/episodes/live?max={}", API_BASE, max);

    let data: ApiResponse<EpisodesData> = authenticated_get(&url).await?;
    Ok(data.data.items)
}

/// Get podcasts by medium type (podcast, music, video, film, etc.)
#[allow(dead_code)]
pub async fn get_podcasts_by_medium(medium: &str, max: Option<u32>) -> Result<Vec<PodcastFeed>, String> {
    let max = max.unwrap_or(20);
    let url = format!("{}/podcasts/bymedium?medium={}&max={}", API_BASE, urlencoding::encode(medium), max);

    let data: ApiResponse<TrendingData> = authenticated_get(&url).await?;
    Ok(data.data.feeds)
}

/// Get music albums from Podcast Index (medium="music")
pub async fn get_music_albums(max: Option<u32>) -> Result<Vec<PodcastFeed>, String> {
    get_podcasts_by_medium("music", max).await
}

/// Search for music feeds specifically using the dedicated /search/music/byterm endpoint
pub async fn search_music(query: &str, max: Option<u32>) -> Result<Vec<PodcastFeed>, String> {
    let max = max.unwrap_or(20);
    let url = format!("{}/search/music/byterm?q={}&max={}", API_BASE, urlencoding::encode(query), max);

    let data: ApiResponse<SearchData> = authenticated_get(&url).await?;
    Ok(data.data.feeds)
}

// ============================================================================
// Proxy Functions for External Content (CORS-safe)
// ============================================================================

/// Fetch podcast chapters through the proxy to avoid CORS issues
///
/// This proxies the request through our worker to bypass browser CORS restrictions
/// when fetching chapters from external podcast hosts.
pub async fn fetch_chapters_proxied(chapters_url: &str) -> Result<crate::utils::podcast::ChaptersFile, String> {
    // Validate input URL before proxying
    if parse_http_url(chapters_url).is_none() {
        return Err("Invalid chapters URL - must be http or https".to_string());
    }

    let url = format!("{}/proxy/fetch?url={}", API_BASE, urlencoding::encode(chapters_url));
    log::info!("[podcast_index] fetching chapters via proxy");

    let auth_result = nip98_utils::create_auth_header(&url, nip98::HttpMethod::GET).await?;

    let request_future = Request::get(&auth_result.signed_url)
        .header("Authorization", &auth_result.header)
        .send();

    let timeout = gloo_timers::future::TimeoutFuture::new(PROXY_TIMEOUT_MS);

    let response = match select(Box::pin(request_future), Box::pin(timeout)).await {
        Either::Left((result, _)) => result.map_err(|e| format!("Failed to fetch chapters: {}", e))?,
        Either::Right((_, _)) => return Err("Chapters fetch timed out".to_string()),
    };

    if !response.ok() {
        let status = response.status();
        // Don't include raw response body in error - could leak sensitive info
        return Err(format!("Chapters fetch failed with status {}", status));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse chapters JSON: {}", e))
}

/// Fetch podcast transcript through the proxy to avoid CORS issues
///
/// This proxies the request through our worker to bypass browser CORS restrictions
/// when fetching transcripts from external podcast hosts.
pub async fn fetch_transcript_proxied(transcript_url: &str) -> Result<String, String> {
    // Validate input URL before proxying
    if parse_http_url(transcript_url).is_none() {
        return Err("Invalid transcript URL - must be http or https".to_string());
    }

    let url = format!("{}/proxy/fetch?url={}", API_BASE, urlencoding::encode(transcript_url));
    log::info!("[podcast_index] fetching transcript via proxy");

    let auth_result = nip98_utils::create_auth_header(&url, nip98::HttpMethod::GET).await?;

    let request_future = Request::get(&auth_result.signed_url)
        .header("Authorization", &auth_result.header)
        .send();

    let timeout = gloo_timers::future::TimeoutFuture::new(PROXY_TIMEOUT_MS);

    let response = match select(Box::pin(request_future), Box::pin(timeout)).await {
        Either::Left((result, _)) => result.map_err(|e| format!("Failed to fetch transcript: {}", e))?,
        Either::Right((_, _)) => return Err("Transcript fetch timed out".to_string()),
    };

    if !response.ok() {
        let status = response.status();
        // Don't include raw response body in error - could leak sensitive info
        return Err(format!("Transcript fetch failed with status {}", status));
    }

    response
        .text()
        .await
        .map_err(|e| format!("Failed to read transcript: {}", e))
}
