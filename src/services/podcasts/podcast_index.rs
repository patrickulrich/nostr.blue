//! Podcast Index API Service
//!
//! Client for the Podcast Index proxy at podnostrblue.ulrich-patrickr.workers.dev
//! Provides podcast search, trending, categories, and episode discovery.
//! Uses NIP-98 HTTP Authentication for API access.
use nostr_sdk::nips::nip98;
use serde::{Deserialize, Serialize};
use crate::utils::nip98 as nip98_utils;
use crate::utils::validation::parse_http_url;
/// Base URL for the Podcast Index proxy
const API_BASE: &str = "https://podnostrblue.ulrich-patrickr.workers.dev";
fn http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        #[cfg(not(feature = "web"))]
        {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("Failed to create HTTP client")
        }
        #[cfg(feature = "web")]
        {
            reqwest::Client::new()
        }
    })
}
/// Make an authenticated GET request to the Podcast Index proxy
async fn authenticated_get<T: for<'de> Deserialize<'de>>(
    url: &str,
) -> Result<T, String> {
    let auth_result = nip98_utils::create_auth_header(url, nip98::HttpMethod::GET)
        .await?;
    let response = http_client()
        .get(&auth_result.signed_url)
        .header("Authorization", &auth_result.header)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, body));
    }
    response.json().await.map_err(|e| format!("Parse error: {}", e))
}
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
/// Search podcasts by term
pub async fn search_podcasts(
    query: &str,
    max: Option<u32>,
) -> Result<Vec<PodcastFeed>, String> {
    let max = max.unwrap_or(20);
    let url = format!(
        "{}/search/byterm?q={}&max={}",
        API_BASE,
        urlencoding::encode(query),
        max,
    );
    let data: ApiResponse<SearchData> = authenticated_get(&url).await?;
    Ok(data.data.feeds)
}
/// Get trending podcasts
pub async fn get_trending(
    max: Option<u32>,
    category: Option<&str>,
) -> Result<Vec<PodcastFeed>, String> {
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
    let url = format!(
        "{}/podcasts/byfeedurl?url={}",
        API_BASE,
        urlencoding::encode(feed_url),
    );
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
pub async fn get_episode_by_guid(
    guid: &str,
    podcast_guid: Option<&str>,
) -> Result<(Episode, Option<PodcastFeed>), String> {
    let mut url = format!(
        "{}/episodes/byguid?guid={}&fulltext",
        API_BASE,
        urlencoding::encode(guid),
    );
    if let Some(pg) = podcast_guid {
        url.push_str(&format!("&podcastguid={}", urlencoding::encode(pg)));
    }
    log::debug!("[podcast_index] get_episode_by_guid: fetching {}", url);
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
    let podcast = if let Some(feed_id) = data.data.episode.feed_id {
        get_podcast_by_id(feed_id).await.ok()
    } else {
        None
    };
    Ok((data.data.episode, podcast))
}
/// Get episodes by podcast GUID
#[allow(dead_code)]
pub async fn get_episodes_by_podcast_guid(
    podcast_guid: &str,
    max: Option<u32>,
) -> Result<Vec<Episode>, String> {
    let max = max.unwrap_or(20);
    let url = format!(
        "{}/episodes/bypodcastguid?guid={}&max={}&fulltext",
        API_BASE,
        urlencoding::encode(podcast_guid),
        max,
    );
    let data: ApiResponse<EpisodesData> = authenticated_get(&url).await?;
    Ok(data.data.items)
}
/// Get episodes by feed ID
///
/// # Arguments
/// * `feed_id` - The Podcast Index feed ID
/// * `max` - Maximum number of episodes to return (default 20)
/// * `skip` - Number of episodes to skip from the beginning (for pagination)
pub async fn get_episodes_by_feed_id(
    feed_id: u64,
    max: Option<u32>,
    skip: Option<usize>,
) -> Result<Vec<Episode>, String> {
    let skip_count = skip.unwrap_or(0);
    // Request enough episodes to cover skip + max
    let fetch_count = max.unwrap_or(20) as usize + skip_count;
    let url = format!("{}/episodes/byfeedid?id={}&max={}", API_BASE, feed_id, fetch_count);
    let data: ApiResponse<EpisodesData> = authenticated_get(&url).await?;

    // Skip the first N episodes (already loaded) and take the rest
    let items: Vec<Episode> = data.data.items.into_iter().skip(skip_count).collect();
    Ok(items)
}
/// Get a single episode by its numeric ID
pub async fn get_episode_by_id(episode_id: u64) -> Result<Episode, String> {
    let url = format!("{}/episodes/byid?id={}&fulltext", API_BASE, episode_id);
    log::debug!("[podcast_index] get_episode_by_id: fetching {}", url);
    #[derive(Deserialize)]
    struct EpisodeByIdData {
        episode: Episode,
    }
    let data: ApiResponse<EpisodeByIdData> = match authenticated_get(&url).await {
        Ok(d) => d,
        Err(e) => {
            log::error!("[podcast_index] get_episode_by_id failed: {}", e);
            return Err(e);
        }
    };
    Ok(data.data.episode)
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
pub async fn get_podcasts_by_medium(
    medium: &str,
    max: Option<u32>,
) -> Result<Vec<PodcastFeed>, String> {
    let max = max.unwrap_or(20);
    let url = format!(
        "{}/podcasts/bymedium?medium={}&max={}",
        API_BASE,
        urlencoding::encode(medium),
        max,
    );
    let data: ApiResponse<TrendingData> = authenticated_get(&url).await?;
    Ok(data.data.feeds)
}
/// Get music albums from Podcast Index (medium="music")
pub async fn get_music_albums(max: Option<u32>) -> Result<Vec<PodcastFeed>, String> {
    get_podcasts_by_medium("music", max).await
}
/// Search for music feeds specifically using the dedicated /search/music/byterm endpoint
pub async fn search_music(
    query: &str,
    max: Option<u32>,
) -> Result<Vec<PodcastFeed>, String> {
    let max = max.unwrap_or(20);
    let url = format!(
        "{}/search/music/byterm?q={}&max={}",
        API_BASE,
        urlencoding::encode(query),
        max,
    );
    let data: ApiResponse<SearchData> = authenticated_get(&url).await?;
    Ok(data.data.feeds)
}
/// Generic helper to fetch JSON content through the proxy with timeout and proper cancellation.
///
/// Validates the input URL, builds the proxy URL, handles NIP-98 authentication,
/// sets up request cancellation via AbortController, and parses the JSON response.
async fn fetch_via_proxy<T: for<'de> Deserialize<'de>>(
    url: &str,
    resource_type: &str,
) -> Result<T, String> {
    if parse_http_url(url).is_none() {
        return Err(format!("Invalid {} URL - must be http or https", resource_type));
    }
    let proxy_url = format!("{}/proxy/fetch?url={}", API_BASE, urlencoding::encode(url));
    log::debug!("[podcast_index] fetching {} via proxy", resource_type);
    let auth_result = nip98_utils::create_auth_header(&proxy_url, nip98::HttpMethod::GET)
        .await?;
    let response = http_client()
        .get(&auth_result.signed_url)
        .header("Authorization", &auth_result.header)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch {}: {}", resource_type, e))?;
    if !response.status().is_success() {
        let status = response.status();
        return Err(format!("{} fetch failed with status {}", resource_type, status));
    }
    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse {} JSON: {}", resource_type, e))
}
/// Helper to fetch text content through the proxy with timeout and proper cancellation.
async fn fetch_text_via_proxy(url: &str, resource_type: &str) -> Result<String, String> {
    if parse_http_url(url).is_none() {
        return Err(format!("Invalid {} URL - must be http or https", resource_type));
    }
    let proxy_url = format!("{}/proxy/fetch?url={}", API_BASE, urlencoding::encode(url));
    log::debug!("[podcast_index] fetching {} via proxy", resource_type);
    let auth_result = nip98_utils::create_auth_header(&proxy_url, nip98::HttpMethod::GET)
        .await?;
    let response = http_client()
        .get(&auth_result.signed_url)
        .header("Authorization", &auth_result.header)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch {}: {}", resource_type, e))?;
    if !response.status().is_success() {
        let status = response.status();
        return Err(format!("{} fetch failed with status {}", resource_type, status));
    }
    response.text().await.map_err(|e| format!("Failed to read {}: {}", resource_type, e))
}
/// Fetch podcast chapters through the proxy to avoid CORS issues
///
/// This proxies the request through our worker to bypass browser CORS restrictions
/// when fetching chapters from external podcast hosts.
pub async fn fetch_chapters_proxied(
    chapters_url: &str,
) -> Result<crate::utils::podcast::ChaptersFile, String> {
    fetch_via_proxy(chapters_url, "chapters").await
}
/// Fetch podcast transcript through the proxy to avoid CORS issues
///
/// This proxies the request through our worker to bypass browser CORS restrictions
/// when fetching transcripts from external podcast hosts.
pub async fn fetch_transcript_proxied(transcript_url: &str) -> Result<String, String> {
    fetch_text_via_proxy(transcript_url, "transcript").await
}
