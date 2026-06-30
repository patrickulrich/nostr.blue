//! Radio Station Event Parsing Utilities (Kind 31237)
//!
//! Handles parsing and creation of internet radio station events:
//! - Kind 31237: Radio Station Definition (addressable)
//!
//! Radio stations contain stream URLs, metadata, and genre tags
//! for live internet radio playback.
use crate::stores::nostr_client::fetch_radio_events;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;
/// Radio station definition event kind (addressable)
pub const KIND_RADIO_STATION: u16 = 31237;
/// Stream format/codec for radio streams
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum StreamFormat {
    /// HTTP Live Streaming (.m3u8)
    Hls,
    /// MP3 audio stream
    Mp3,
    /// AAC/AAC+ audio stream
    Aac,
    /// Ogg Vorbis stream
    Ogg,
    /// Unknown or undetected format
    #[default]
    Unknown,
}
impl StreamFormat {
    /// Parse format from MIME type string
    pub fn from_mime(mime: &str) -> Self {
        match mime.to_lowercase().as_str() {
            "application/vnd.apple.mpegurl" | "application/x-mpegurl" => Self::Hls,
            "audio/mpeg" | "audio/mp3" => Self::Mp3,
            "audio/aac" | "audio/aacp" | "audio/x-aac" => Self::Aac,
            "audio/ogg" | "application/ogg" | "audio/vorbis" => Self::Ogg,
            _ => Self::Unknown,
        }
    }
    /// Detect format from URL extension
    pub fn from_url(url: &str) -> Self {
        let url_lower = url.to_lowercase();
        if url_lower.contains(".m3u8") {
            Self::Hls
        } else if url_lower.contains(".mp3") {
            Self::Mp3
        } else if url_lower.contains(".aac") {
            Self::Aac
        } else if url_lower.contains(".ogg") || url_lower.contains(".oga") {
            Self::Ogg
        } else {
            Self::Unknown
        }
    }
    /// Check if this is an HLS stream (requires special handling)
    pub fn is_hls(&self) -> bool {
        matches!(self, Self::Hls)
    }
    /// Get display name for this format
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Hls => "HLS",
            Self::Mp3 => "MP3",
            Self::Aac => "AAC",
            Self::Ogg => "OGG",
            Self::Unknown => "Audio",
        }
    }
}
/// Stream quality metadata (wavefunc-compatible JSON format)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct StreamQuality {
    /// Bitrate in kbps
    pub bitrate: u32,
    /// Codec name (e.g., "mp3", "aac", "opus")
    pub codec: String,
    /// Sample rate in Hz (e.g., 44100, 48000)
    #[serde(rename = "sampleRate")]
    pub sample_rate: u32,
}
/// Stream definition in event content JSON (wavefunc format)
#[derive(Clone, Debug, Deserialize)]
struct ContentStream {
    url: String,
    format: String,
    quality: StreamQuality,
    #[serde(default)]
    primary: bool,
}
/// Event content JSON format (wavefunc format)
/// Used as fallback when stream tags are not present
#[derive(Clone, Debug, Deserialize)]
struct StationContent {
    #[serde(default)]
    #[allow(dead_code)]
    description: String,
    #[serde(default)]
    streams: Vec<ContentStream>,
}
/// Individual stream URL with format and quality info
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RadioStream {
    /// Stream URL
    pub url: String,
    /// Detected or specified format
    pub format: StreamFormat,
    /// Quality metadata (wavefunc-compatible)
    pub quality: StreamQuality,
    /// Whether this is the primary/preferred stream
    pub is_primary: bool,
}
impl RadioStream {
    /// Normalize a URL by trimming whitespace and removing backticks
    fn normalize_url(url: &str) -> String {
        url.trim().trim_matches('`').to_string()
    }
    /// Create a new stream from URL, format, and quality
    pub fn new(
        url: String,
        format_str: Option<&str>,
        quality: StreamQuality,
        is_primary: bool,
    ) -> Self {
        let normalized_url = Self::normalize_url(&url);
        let format = format_str
            .map(StreamFormat::from_mime)
            .unwrap_or_else(|| StreamFormat::from_url(&normalized_url));
        Self {
            url: normalized_url,
            format,
            quality,
            is_primary,
        }
    }
    /// Get bitrate (convenience accessor)
    pub fn bitrate(&self) -> Option<u32> {
        if self.quality.bitrate > 0 {
            Some(self.quality.bitrate)
        } else {
            None
        }
    }
    /// Create a stream with a specific bitrate (for testing)
    #[cfg(test)]
    pub fn with_bitrate(
        url: String,
        mime: Option<&str>,
        bitrate: Option<u32>,
        is_primary: bool,
    ) -> Self {
        let quality = StreamQuality {
            bitrate: bitrate.unwrap_or(0),
            codec: mime
                .map(|m| m.split('/').last().unwrap_or("unknown").to_string())
                .unwrap_or_default(),
            sample_rate: 0,
        };
        Self::new(url, mime, quality, is_primary)
    }
}
/// "Now Playing" metadata from stream
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct NowPlaying {
    /// Current song/track title
    pub title: Option<String>,
    /// Current artist name
    pub artist: Option<String>,
    /// Album artwork URL
    pub album_art: Option<String>,
}
impl NowPlaying {
    /// Check if any metadata is available
    #[allow(dead_code)]
    pub fn has_data(&self) -> bool {
        self.title.is_some() || self.artist.is_some()
    }
    /// Get display string (Artist - Title or just Title)
    pub fn display_string(&self) -> Option<String> {
        match (&self.artist, &self.title) {
            (Some(artist), Some(title)) => Some(format!("{} - {}", artist, title)),
            (None, Some(title)) => Some(title.clone()),
            (Some(artist), None) => Some(artist.clone()),
            (None, None) => None,
        }
    }
}
/// Radio station parsed from a Kind 31237 event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RadioStation {
    /// Event ID (hex)
    pub event_id: String,
    /// Station owner pubkey (hex)
    pub pubkey: String,
    /// Unique identifier (d-tag)
    pub d_tag: String,
    /// Event coordinate: "31237:pubkey:d-tag"
    pub coordinate: String,
    /// NIP-19 naddr for this station
    pub naddr: Option<String>,
    /// Station name
    pub name: String,
    /// Station description/bio
    pub description: Option<String>,
    /// Station logo/thumbnail URL
    pub thumbnail: Option<String>,
    /// Station website URL
    pub website: Option<String>,
    /// Station location (city, country)
    pub location: Option<String>,
    /// ISO country code (e.g., "US", "DE")
    pub country_code: Option<String>,
    /// Genre tags
    pub genres: Vec<String>,
    /// Language codes (e.g., "en", "de")
    pub languages: Vec<String>,
    /// Available streams
    pub streams: Vec<RadioStream>,
    /// Created timestamp
    pub created_at: u64,
}
impl RadioStation {
    /// Parse a radio station from a Kind 31237 event
    /// Uses wavefunc-compatible tag format
    pub fn from_event(event: &Event) -> Result<Self, String> {
        if event.kind.as_u16() != KIND_RADIO_STATION {
            return Err(format!(
                "Expected kind {}, got {}",
                KIND_RADIO_STATION,
                event.kind.as_u16(),
            ));
        }
        let pubkey = event.pubkey.to_hex();
        let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag (station ID)")?;
        let name =
            get_tag_value(event, "name").ok_or("Missing required 'name' tag (station name)")?;
        let coordinate = format!("{}:{}:{}", KIND_RADIO_STATION, pubkey, d_tag);
        let naddr = build_station_naddr(&pubkey, &d_tag);
        let mut streams: Vec<RadioStream> = event
            .tags
            .iter()
            .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("stream"))
            .filter_map(|t| {
                let slice = t.as_slice();
                let url = slice.get(1)?.to_string();
                let format_str = slice.get(2).map(|s| s.as_str());
                let is_primary = slice.iter().any(|s| s == "primary");
                let quality = slice
                    .get(3)
                    .and_then(|quality_str| serde_json::from_str::<StreamQuality>(quality_str).ok())
                    .unwrap_or_default();
                Some(RadioStream::new(url, format_str, quality, is_primary))
            })
            .collect();
        if streams.is_empty() {
            if let Ok(content) = serde_json::from_str::<StationContent>(&event.content) {
                streams = content
                    .streams
                    .into_iter()
                    .map(|s| RadioStream::new(s.url, Some(s.format.as_str()), s.quality, s.primary))
                    .collect();
                log::debug!(
                    "Parsed {} streams from event content for station {}",
                    streams.len(),
                    name
                );
            }
        }
        if streams.is_empty() {
            log::warn!("Radio station '{}' has no streams in tags or content", name);
        }
        let genres: Vec<String> = event
            .tags
            .iter()
            .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("t"))
            .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
            .collect();
        let languages: Vec<String> = event
            .tags
            .iter()
            .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("l"))
            .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
            .collect();
        Ok(RadioStation {
            event_id: event.id.to_hex(),
            pubkey,
            d_tag,
            coordinate,
            naddr,
            name,
            description: get_tag_value(event, "description"),
            thumbnail: get_tag_value(event, "thumbnail"),
            website: get_tag_value(event, "website"),
            location: get_tag_value(event, "location"),
            country_code: get_tag_value(event, "country"),
            genres,
            languages,
            streams,
            created_at: event.created_at.as_secs(),
        })
    }
    /// Get display location (location or country code)
    pub fn display_location(&self) -> Option<&str> {
        self.location.as_deref().or(self.country_code.as_deref())
    }
}
/// Fetch radio stations with optional genre filter
///
/// Pass `until = Some(ts)` to page backwards in time (events older than `ts`).
pub async fn fetch_radio_stations(
    genre: Option<&str>,
    limit: usize,
    until: Option<Timestamp>,
) -> Result<Vec<RadioStation>, String> {
    let mut filter = Filter::new()
        .kind(Kind::Custom(KIND_RADIO_STATION))
        .limit(limit);
    if let Some(g) = genre {
        filter = filter.hashtag(g.to_lowercase());
    }
    if let Some(ts) = until {
        filter = filter.until(ts);
    }
    let events = fetch_radio_events(filter, Duration::from_secs(10)).await?;
    let stations: Vec<RadioStation> = events
        .iter()
        .filter_map(|e| {
            match RadioStation::from_event(e) {
                Ok(station) => Some(station),
                Err(err) => {
                    log::warn!("Failed to parse radio event {}: {}", e.id.to_hex(), err);
                    None
                }
            }
        })
        .collect();
    Ok(stations)
}
/// Fetch a single station by naddr
pub async fn fetch_station_by_naddr(naddr: &str) -> Result<RadioStation, String> {
    let (pubkey, d_tag) = parse_station_naddr(naddr)?;
    let pk = PublicKey::from_hex(&pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_RADIO_STATION))
        .author(pk)
        .identifier(&d_tag)
        .limit(1);

    // Phase 1: Try radio relay + connected relays (up to 3 attempts)
    let mut events = Vec::new();
    for attempt in 0..3 {
        events = fetch_radio_events(filter.clone(), Duration::from_secs(10)).await?;
        if !events.is_empty() {
            break;
        }
        if attempt < 2 {
            log::info!(
                "Station fetch returned 0 events (attempt {}), retrying in 4s...",
                attempt + 1
            );
            crate::platform::timer::sleep_ms(4000).await;
        }
    }

    // Phase 2: Full relay discovery (DB, connected relays, author NIP-65, gossip)
    if events.is_empty() {
        log::info!("Station not on radio relay, trying full relay discovery...");
        if let Some(event) =
            crate::stores::nostr_client::fetch_event_by_coordinate_with_relays(
                KIND_RADIO_STATION,
                pubkey.clone(),
                d_tag.clone(),
                Vec::new(),
            )
            .await
            .map_err(|e| format!("Relay discovery failed: {}", e))?
        {
            events = vec![event];
        }
    }

    events
        .into_iter()
        .filter_map(|e| {
            match RadioStation::from_event(&e) {
                Ok(station) => Some(station),
                Err(err) => {
                    log::warn!("Failed to parse radio event {}: {}", e.id.to_hex(), err);
                    None
                }
            }
        })
        .max_by_key(|s| s.created_at)
        .ok_or_else(|| "Station not found".to_string())
}
/// Search radio stations using NIP-50 full-text search
///
/// Searches station names, descriptions, and tags across relays that support NIP-50.
/// Falls back to client-side filtering if needed.
pub async fn search_radio_stations(
    query: &str,
    limit: usize,
    until: Option<Timestamp>,
) -> Result<Vec<RadioStation>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    log::debug!("Searching radio stations for: {}", query);
    let mut filter = Filter::new()
        .kind(Kind::Custom(KIND_RADIO_STATION))
        .search(query)
        .limit(limit);
    if let Some(ts) = until {
        filter = filter.until(ts);
    }
    match fetch_radio_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            log::debug!("NIP-50 search found {} station events", events.len());
            let mut stations: Vec<RadioStation> = events
                .iter()
                .filter_map(|e| {
                    match RadioStation::from_event(e) {
                        Ok(station) => Some(station),
                        Err(err) => {
                            log::warn!(
                                "Failed to parse radio event {}: {}",
                                e.id.to_hex(),
                                err
                            );
                            None
                        }
                    }
                })
                .collect();
            let query_lower = query.to_lowercase();
            stations.sort_by(|a, b| {
                let a_name_match = a.name.to_lowercase().contains(&query_lower);
                let b_name_match = b.name.to_lowercase().contains(&query_lower);
                match (a_name_match, b_name_match) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => b.created_at.cmp(&a.created_at),
                }
            });
            log::debug!(
                "Search for '{}' returned {} stations",
                query,
                stations.len()
            );
            Ok(stations)
        }
        Err(e) => {
            log::warn!("NIP-50 search failed: {}, trying fallback", e);
            let mut filter = Filter::new()
                .kind(Kind::Custom(KIND_RADIO_STATION))
                .limit(200);
            if let Some(ts) = until {
                filter = filter.until(ts);
            }
            let events =
                fetch_radio_events(filter, Duration::from_secs(10)).await?;
            let query_lower = query.to_lowercase();
            let mut stations: Vec<RadioStation> = events
                .iter()
                .filter_map(|e| {
                    match RadioStation::from_event(e) {
                        Ok(station) => Some(station),
                        Err(err) => {
                            log::warn!(
                                "Failed to parse radio event {}: {}",
                                e.id.to_hex(),
                                err
                            );
                            None
                        }
                    }
                })
                .filter(|s| {
                    s.name.to_lowercase().contains(&query_lower)
                        || s.description
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&query_lower))
                            .unwrap_or(false)
                        || s.genres
                            .iter()
                            .any(|g| g.to_lowercase().contains(&query_lower))
                        || s.location
                            .as_ref()
                            .map(|l| l.to_lowercase().contains(&query_lower))
                            .unwrap_or(false)
                })
                .collect();
            stations.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            stations.truncate(limit);
            Ok(stations)
        }
    }
}
/// Get first value of a tag by name
fn get_tag_value(event: &Event, tag_name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}
/// Build a NIP-19 naddr for a radio station
pub fn build_station_naddr(pubkey: &str, d_tag: &str) -> Option<String> {
    let pk = PublicKey::from_hex(pubkey).ok()?;
    let coordinate = Coordinate::new(Kind::Custom(KIND_RADIO_STATION), pk).identifier(d_tag);
    let nip19 = Nip19Coordinate::new(coordinate, vec![]);
    nip19.to_bech32().ok()
}
/// Parse a station naddr back to (pubkey, d_tag)
pub fn parse_station_naddr(naddr: &str) -> Result<(String, String), String> {
    let nip19 = Nip19Coordinate::from_bech32(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    if nip19.coordinate.kind.as_u16() != KIND_RADIO_STATION {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_RADIO_STATION,
            nip19.coordinate.kind.as_u16(),
        ));
    }
    let pubkey = nip19.coordinate.public_key.to_hex();
    let d_tag = nip19.coordinate.identifier.clone();
    Ok((pubkey, d_tag))
}
/// Select the best stream from a list
/// Preference: primary stream > highest bitrate non-HLS > first stream
pub fn select_best_stream(streams: &[RadioStream]) -> Option<&RadioStream> {
    if let Some(primary) = streams.iter().find(|s| s.is_primary) {
        return Some(primary);
    }
    let non_hls: Vec<_> = streams.iter().filter(|s| !s.format.is_hls()).collect();
    if !non_hls.is_empty() {
        return non_hls.iter().max_by_key(|s| s.quality.bitrate).copied();
    }
    streams.first()
}
const SCORE_PRIMARY: u32 = 1000;
const SCORE_HLS: u32 = 100;
const SCORE_AAC: u32 = 90;
const SCORE_MP3: u32 = 80;
const SCORE_OGG: u32 = 70;
const SCORE_UNKNOWN: u32 = 50;
/// Calculate stream priority score for ranking
pub fn stream_score(stream: &RadioStream) -> u32 {
    let mut score = 0;
    if stream.is_primary {
        score += SCORE_PRIMARY;
    }
    score += match stream.format {
        StreamFormat::Hls => SCORE_HLS,
        StreamFormat::Aac => SCORE_AAC,
        StreamFormat::Mp3 => SCORE_MP3,
        StreamFormat::Ogg => SCORE_OGG,
        StreamFormat::Unknown => SCORE_UNKNOWN,
    };
    score += stream.quality.bitrate.min(320) / 6;
    score
}
/// Rank streams by priority score (returns indices in descending score order)
pub fn rank_streams(streams: &[RadioStream]) -> Vec<usize> {
    let mut indexed: Vec<(usize, u32)> = streams
        .iter()
        .enumerate()
        .map(|(i, s)| (i, stream_score(s)))
        .collect();
    indexed.sort_by_key(|b| std::cmp::Reverse(b.1));
    indexed.into_iter().map(|(i, _)| i).collect()
}
/// Get all stream URLs in ranked order for fallback logic
pub fn get_ranked_stream_urls(streams: &[RadioStream]) -> Vec<String> {
    rank_streams(streams)
        .iter()
        .filter_map(|&i| streams.get(i))
        .map(|s| s.url.clone())
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_stream_format_from_mime() {
        assert_eq!(
            StreamFormat::from_mime("application/vnd.apple.mpegurl"),
            StreamFormat::Hls,
        );
        assert_eq!(StreamFormat::from_mime("audio/mpeg"), StreamFormat::Mp3);
        assert_eq!(StreamFormat::from_mime("audio/aac"), StreamFormat::Aac);
        assert_eq!(StreamFormat::from_mime("audio/ogg"), StreamFormat::Ogg);
        assert_eq!(
            StreamFormat::from_mime("unknown/type"),
            StreamFormat::Unknown
        );
    }
    #[test]
    fn test_stream_format_from_url() {
        assert_eq!(
            StreamFormat::from_url("https://example.com/stream.m3u8"),
            StreamFormat::Hls,
        );
        assert_eq!(
            StreamFormat::from_url("https://example.com/stream.mp3"),
            StreamFormat::Mp3,
        );
        assert_eq!(
            StreamFormat::from_url("https://example.com/stream.aac"),
            StreamFormat::Aac,
        );
        assert_eq!(
            StreamFormat::from_url("https://example.com/stream.ogg"),
            StreamFormat::Ogg,
        );
    }
    #[test]
    fn test_select_best_stream() {
        let streams = vec![
            RadioStream::with_bitrate(
                "https://example.com/low.mp3".to_string(),
                Some("audio/mpeg"),
                Some(64),
                false,
            ),
            RadioStream::with_bitrate(
                "https://example.com/high.mp3".to_string(),
                Some("audio/mpeg"),
                Some(320),
                false,
            ),
        ];
        let best = select_best_stream(&streams);
        assert!(best.is_some());
        assert_eq!(best.unwrap().quality.bitrate, 320);
    }
    #[test]
    fn test_select_best_stream_prefers_primary() {
        let streams = vec![
            RadioStream::with_bitrate(
                "https://example.com/high.mp3".to_string(),
                Some("audio/mpeg"),
                Some(320),
                false,
            ),
            RadioStream::with_bitrate(
                "https://example.com/primary.mp3".to_string(),
                Some("audio/mpeg"),
                Some(128),
                true,
            ),
        ];
        let best = select_best_stream(&streams);
        assert!(best.is_some());
        assert!(best.unwrap().is_primary);
    }
    #[test]
    fn test_stream_quality_json_parsing() {
        let json = r#"{"bitrate":320,"codec":"mp3","sampleRate":44100}"#;
        let quality: StreamQuality = serde_json::from_str(json).unwrap();
        assert_eq!(quality.bitrate, 320);
        assert_eq!(quality.codec, "mp3");
        assert_eq!(quality.sample_rate, 44100);
    }
    #[test]
    fn test_stream_quality_json_serialization() {
        let quality = StreamQuality {
            bitrate: 256,
            codec: "aac".to_string(),
            sample_rate: 48000,
        };
        let json = serde_json::to_string(&quality).unwrap();
        assert!(json.contains("256"));
        assert!(json.contains("aac"));
        assert!(json.contains("48000"));
    }
    #[test]
    fn test_now_playing_display() {
        let np = NowPlaying {
            artist: Some("Artist".to_string()),
            title: Some("Song".to_string()),
            album_art: None,
        };
        assert_eq!(np.display_string(), Some("Artist - Song".to_string()));
        let np_title_only = NowPlaying {
            artist: None,
            title: Some("Song".to_string()),
            album_art: None,
        };
        assert_eq!(np_title_only.display_string(), Some("Song".to_string()));
    }
    #[test]
    fn test_build_station_naddr() {
        let pubkey = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let d_tag = "my-radio-station";
        let naddr = build_station_naddr(pubkey, d_tag);
        assert!(naddr.is_some());
        assert!(naddr.unwrap().starts_with("naddr1"));
    }
    #[test]
    fn test_parse_station_naddr() {
        let pubkey = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let d_tag = "my-radio-station";
        let naddr = build_station_naddr(pubkey, d_tag).unwrap();
        let (parsed_pubkey, parsed_d_tag) = parse_station_naddr(&naddr).unwrap();
        assert_eq!(parsed_pubkey, pubkey);
        assert_eq!(parsed_d_tag, d_tag);
    }
}

/// Delete a radio station by publishing a NIP-09 deletion request.
/// Uses coordinate-based deletion (a-tag) so relays remove all versions.
pub async fn delete_radio_station(coordinate: &str) -> Result<String, String> {
    let _client =
        crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    use nostr::nips::nip01::Coordinate;
    use nostr::nips::nip09::EventDeletionRequest;
    let coord = Coordinate::parse(coordinate)
        .map_err(|e| format!("Invalid coordinate '{}': {}", coordinate, e))?;
    let deletion_request = EventDeletionRequest::new().coordinate(coord);
    let builder = EventBuilder::delete(deletion_request);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign deletion: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("radio".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(event_id)
}
