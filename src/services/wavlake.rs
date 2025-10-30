use serde::{Deserialize, Serialize};
use gloo_net::http::Request;

/// Wavlake API base URL
const WAVLAKE_API_BASE: &str = "https://wavlake.com/api/v1";

/// A track from Wavlake
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WavlakeTrack {
    pub id: String,
    pub title: String,
    #[serde(rename = "albumTitle")]
    pub album_title: String,
    pub artist: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    #[serde(rename = "albumId")]
    pub album_id: String,
    #[serde(rename = "artistArtUrl")]
    pub artist_art_url: String,
    #[serde(rename = "albumArtUrl")]
    pub album_art_url: String,
    #[serde(rename = "mediaUrl")]
    pub media_url: String,
    pub duration: u32,
    #[serde(rename = "releaseDate", default)]
    pub release_date: Option<String>,
    #[serde(rename = "msatTotal")]
    pub msat_total: String,
    #[serde(rename = "artistNpub", default)]
    pub artist_npub: Option<String>,
    pub order: Option<u32>,
    pub url: Option<String>,
}

/// A Wavlake artist with their albums
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WavlakeArtist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(rename = "artistArtUrl", default)]
    pub artist_art_url: Option<String>,
    pub albums: Vec<WavlakeAlbumSummary>,
    #[serde(rename = "artistNpub")]
    pub artist_npub: Option<String>,
}

/// Summary information about an album
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WavlakeAlbumSummary {
    pub id: String,
    pub title: String,
    #[serde(rename = "albumArtUrl")]
    pub album_art_url: String,
    #[serde(rename = "releaseDate")]
    pub release_date: String,
}

/// Full album information with tracks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WavlakeAlbum {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(rename = "artistUrl", default)]
    pub artist_url: Option<String>,
    #[serde(rename = "artistArtUrl", default)]
    pub artist_art_url: Option<String>,
    #[serde(rename = "albumArtUrl", default)]
    pub album_art_url: Option<String>,
    #[serde(rename = "releaseDate")]
    pub release_date: String,
    pub tracks: Vec<WavlakeTrack>,
}

/// Search result from Wavlake
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WavlakeSearchResult {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub result_type: String, // "artist" | "album" | "track"
    #[serde(rename = "albumArtUrl")]
    pub album_art_url: Option<String>,
    #[serde(rename = "artistArtUrl")]
    pub artist_art_url: Option<String>,
    #[serde(rename = "albumId")]
    pub album_id: Option<String>,
    #[serde(rename = "albumTitle")]
    pub album_title: Option<String>,
    #[serde(rename = "artistId")]
    pub artist_id: Option<String>,
    pub artist: Option<String>,
    pub duration: Option<u32>,
}

/// Playlist from Wavlake
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WavlakePlaylist {
    #[serde(rename = "userId")]
    pub user_id: String,
    pub title: String,
    pub tracks: Vec<WavlakeTrack>,
}

/// LNURL response from Wavlake
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WavlakeLnurlResponse {
    pub lnurl: String,
}

/// Error response from Wavlake API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WavlakeErrorResponse {
    pub code: u32,
    pub success: bool,
    pub error: String,
}

/// Wavlake API client
pub struct WavlakeAPI {
    base_url: String,
}

/// Standalone helper function to get artist
pub async fn get_artist(artist_id: &str) -> Result<WavlakeArtist, String> {
    let api = WavlakeAPI::new();
    api.get_artist(artist_id).await
}

/// Standalone helper function to get album
pub async fn get_album(album_id: &str) -> Result<WavlakeAlbum, String> {
    let api = WavlakeAPI::new();
    api.get_album(album_id).await
}

impl WavlakeAPI {
    /// Create a new Wavlake API client
    pub fn new() -> Self {
        Self {
            base_url: WAVLAKE_API_BASE.to_string(),
        }
    }

    /// Search for content on Wavlake
    pub async fn search_content(&self, term: &str) -> Result<Vec<WavlakeSearchResult>, String> {
        let url = format!(
            "{}/content/search?term={}",
            self.base_url,
            urlencoding::encode(term)
        );

        let response = Request::get(&url)
            .send()
            .await
            .map_err(|e| format!("Search request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("Search failed: {}", response.status_text()));
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse search results: {}", e))
    }

    /// Get rankings/trending tracks
    pub async fn get_rankings(
        &self,
        sort: &str,
        days: Option<u32>,
        start_date: Option<&str>,
        end_date: Option<&str>,
        genre: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<WavlakeTrack>, String> {
        let mut params = vec![("sort", sort.to_string())];

        if let Some(d) = days {
            params.push(("days", d.to_string()));
        }
        if let Some(sd) = start_date {
            params.push(("startDate", sd.to_string()));
        }
        if let Some(ed) = end_date {
            params.push(("endDate", ed.to_string()));
        }
        if let Some(g) = genre {
            params.push(("genre", g.to_string()));
        }
        if let Some(l) = limit {
            params.push(("limit", l.to_string()));
        }

        let query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        let url = format!("{}/content/rankings?{}", self.base_url, query_string);

        let response = Request::get(&url)
            .send()
            .await
            .map_err(|e| format!("Rankings request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("Rankings failed: {}", response.status_text()));
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse rankings: {}", e))
    }

    /// Get a specific track
    pub async fn get_track(&self, track_id: &str) -> Result<WavlakeTrack, String> {
        let url = format!("{}/content/track/{}", self.base_url, track_id);

        let response = Request::get(&url)
            .send()
            .await
            .map_err(|e| format!("Track request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("Track fetch failed: {}", response.status_text()));
        }

        // The API returns an array, but we want the first track
        let result: Vec<WavlakeTrack> = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse track: {}", e))?;

        result
            .into_iter()
            .next()
            .ok_or_else(|| "No track found".to_string())
    }

    /// Get an artist's information
    pub async fn get_artist(&self, artist_id: &str) -> Result<WavlakeArtist, String> {
        let url = format!("{}/content/artist/{}", self.base_url, artist_id);

        let response = Request::get(&url)
            .send()
            .await
            .map_err(|e| format!("Artist request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("Artist fetch failed: {}", response.status_text()));
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse artist: {}", e))
    }

    /// Get an album's information
    pub async fn get_album(&self, album_id: &str) -> Result<WavlakeAlbum, String> {
        let url = format!("{}/content/album/{}", self.base_url, album_id);

        let response = Request::get(&url)
            .send()
            .await
            .map_err(|e| format!("Album request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("Album fetch failed: {}", response.status_text()));
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse album: {}", e))
    }

    /// Get a playlist
    pub async fn get_playlist(&self, playlist_id: &str) -> Result<WavlakePlaylist, String> {
        let url = format!("{}/content/playlist/{}", self.base_url, playlist_id);

        let response = Request::get(&url)
            .send()
            .await
            .map_err(|e| format!("Playlist request failed: {}", e))?;

        if !response.ok() {
            return Err(format!("Playlist fetch failed: {}", response.status_text()));
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse playlist: {}", e))
    }

    /// Get LNURL for lightning payments
    pub async fn get_lnurl(
        &self,
        content_id: &str,
        app_id: Option<&str>,
    ) -> Result<WavlakeLnurlResponse, String> {
        let url = if let Some(app) = app_id {
            format!(
                "{}/lnurl?contentId={}&appId={}",
                self.base_url, content_id, app
            )
        } else {
            format!("{}/lnurl?contentId={}", self.base_url, content_id)
        };

        log::info!("Requesting LNURL from: {}", url);

        let response = Request::get(&url)
            .send()
            .await
            .map_err(|e| format!("LNURL request failed: {}", e))?;

        if !response.ok() {
            let status = response.status();
            let status_text = response.status_text();
            let body = response.text().await.unwrap_or_else(|_| "Unable to read body".to_string());
            let error_msg = format!("LNURL fetch failed: {} {}. Body: {}", status, status_text, body);
            log::error!("{}", error_msg);
            return Err(error_msg);
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse LNURL response: {}", e))
    }
}

impl Default for WavlakeAPI {
    fn default() -> Self {
        Self::new()
    }
}
