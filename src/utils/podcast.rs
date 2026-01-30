//! Podcast Event Parsing Utilities
//!
//! Handles parsing of native Nostr podcast events:
//! - Kind 30054: Podcast Episode (addressable)
//! - Kind 30055: Podcast Trailer (addressable)
//! - Kind 30078: Application-specific metadata (d="podcast-metadata")
//!
//! Also includes Podcasting 2.0 data structures for V4V payments, chapters, etc.
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
/// Podcast episode event kind (addressable)
pub const KIND_PODCAST_EPISODE: u16 = 30054;
/// Application-specific data kind (for podcast metadata)
pub const KIND_APP_DATA: u16 = 30078;
/// Podcast-level metadata parsed from Kind 30078 event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodcastMetadata {
    /// Podcast title
    pub title: String,
    /// Podcast description
    pub description: Option<String>,
    /// Author/owner name
    pub author: Option<String>,
    /// Cover image URL
    pub image: Option<String>,
    /// Language code (e.g., "en")
    pub language: Option<String>,
    /// Category tags
    pub categories: Vec<String>,
    /// Explicit content flag
    pub explicit: bool,
    /// Website URL
    pub website: Option<String>,
    /// Funding links
    pub funding: Vec<FundingLink>,
    /// V4V payment configuration
    pub value: Option<ValueBlock>,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// d-tag identifier
    pub d_tag: String,
    /// Created timestamp
    pub created_at: u64,
}
/// Parsed podcast episode from Kind 30054 event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodcastEpisode {
    /// Event ID
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// d-tag identifier
    pub d_tag: String,
    /// Coordinate string: "30054:pubkey:d-tag"
    pub coordinate: String,
    /// Episode title
    pub title: String,
    /// Audio file URL
    pub audio_url: String,
    /// Audio MIME type
    pub audio_type: Option<String>,
    /// Publication date (RFC2822 or ISO8601)
    pub pubdate: Option<String>,
    /// Episode description/show notes
    pub description: Option<String>,
    /// Episode image URL
    pub image: Option<String>,
    /// Duration in seconds
    pub duration: Option<u64>,
    /// Topic tags
    pub topics: Vec<String>,
    /// Season number
    pub season: Option<u32>,
    /// Episode number
    pub episode_number: Option<u32>,
    /// Chapters URL (JSON format)
    pub chapters_url: Option<String>,
    /// Transcript references
    pub transcripts: Vec<TranscriptRef>,
    /// Soundbites for previews
    pub soundbites: Vec<Soundbite>,
    /// Episode-level V4V configuration (overrides show-level)
    pub value: Option<ValueBlock>,
    /// Created timestamp
    pub created_at: u64,
}
/// Value for Value (V4V) Lightning payment configuration
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValueBlock {
    /// Value type (always "lightning" for now)
    pub value_type: String,
    /// Payment method ("keysend" or "amp")
    pub method: String,
    /// Suggested sats per minute
    pub suggested: Option<f64>,
    /// Payment recipients with splits
    pub recipients: Vec<ValueRecipient>,
}
impl Default for ValueBlock {
    fn default() -> Self {
        Self {
            value_type: "lightning".to_string(),
            method: "keysend".to_string(),
            suggested: None,
            recipients: Vec::new(),
        }
    }
}
/// V4V payment recipient
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValueRecipient {
    /// Recipient name (for display)
    pub name: Option<String>,
    /// Recipient type: "node" (keysend) or "lnaddress" (Lightning Address)
    pub recipient_type: String,
    /// Lightning node pubkey or Lightning Address
    pub address: String,
    /// Share of payment (splits sum to 100%)
    pub split: u32,
    /// Custom TLV key (for keysend)
    pub custom_key: Option<String>,
    /// Custom TLV value (for keysend)
    pub custom_value: Option<String>,
    /// True if this is a processing fee
    pub fee: Option<bool>,
}
/// JSON Chapters format (Podcasting 2.0)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChaptersFile {
    /// Chapters format version
    pub version: String,
    /// Chapter list
    pub chapters: Vec<Chapter>,
    /// Chapter file author
    pub author: Option<String>,
    /// Episode title
    pub title: Option<String>,
    /// Podcast name
    #[serde(rename = "podcastName")]
    pub podcast_name: Option<String>,
}
/// Single chapter entry
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    /// Start time in seconds
    #[serde(rename = "startTime")]
    pub start_time: f64,
    /// Chapter title
    pub title: Option<String>,
    /// Chapter image URL
    pub img: Option<String>,
    /// Link associated with chapter
    pub url: Option<String>,
    /// Show in table of contents (default true)
    pub toc: Option<bool>,
    /// Optional explicit end time
    #[serde(rename = "endTime")]
    pub end_time: Option<f64>,
    /// OpenStreetMap query string
    pub location: Option<String>,
}
/// Transcript reference
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TranscriptRef {
    /// Transcript file URL
    pub url: String,
    /// MIME type: "text/vtt", "application/json", "text/plain", "application/x-subrip"
    #[serde(rename = "type")]
    pub transcript_type: String,
    /// Language code
    pub language: Option<String>,
    /// Relation: "captions" for closed captions
    pub rel: Option<String>,
}
/// Soundbite for episode previews
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Soundbite {
    /// Start time in seconds
    #[serde(rename = "startTime")]
    pub start_time: f64,
    /// Duration in seconds (recommended 15-120)
    pub duration: f64,
    /// Optional title (max 128 chars)
    pub title: Option<String>,
}
/// Person/contributor credit
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Person {
    /// Person's name
    pub name: String,
    /// Role: "host", "guest", "editor", etc.
    pub role: Option<String>,
    /// Group: "cast", "crew"
    pub group: Option<String>,
    /// Avatar image URL
    pub img: Option<String>,
    /// Bio/social link URL
    pub href: Option<String>,
}
/// Funding link
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FundingLink {
    /// Funding page URL
    pub url: String,
    /// Display name
    pub name: Option<String>,
}
/// Source of podcast content for routing and display
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PodcastSource {
    /// Native Nostr podcast (kind 30054/30055/30078)
    Nostr { pubkey: String, d_tag: String, coordinate: String },
    /// Traditional RSS feed
    Rss {
        feed_url: String,
        guid: String,
        /// Podcast Index feed ID (for routing)
        podcast_id: Option<u64>,
    },
}
/// Parse a Kind 30054 event into a PodcastEpisode
pub fn parse_podcast_episode(event: &Event) -> Result<PodcastEpisode, String> {
    if event.kind.as_u16() != KIND_PODCAST_EPISODE {
        return Err(
            format!(
                "Expected kind {}, got {}",
                KIND_PODCAST_EPISODE,
                event.kind.as_u16(),
            ),
        );
    }
    let pubkey = event.pubkey.to_hex();
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;
    let title = get_tag_value(event, "title").ok_or("Missing required 'title' tag")?;
    let audio_url = get_tag_value(event, "audio")
        .or_else(|| get_tag_value(event, "url"))
        .or_else(|| get_tag_value(event, "enclosure"))
        .or_else(|| {
            if event.content.starts_with("http") {
                Some(event.content.split_whitespace().next()?.to_string())
            } else {
                None
            }
        })
        .ok_or("Missing audio URL (audio, url, or enclosure tag)")?;
    let audio_type = get_tag_second_value(event, "audio")
        .or_else(|| get_tag_value(event, "type"));
    let coordinate = format!("{}:{}:{}", KIND_PODCAST_EPISODE, pubkey, d_tag);
    let pubdate = get_tag_value(event, "pubdate")
        .or_else(|| get_tag_value(event, "published_at"));
    let description = get_tag_value(event, "description")
        .or_else(|| get_tag_value(event, "summary"))
        .or_else(|| {
            if !event.content.is_empty() && !event.content.starts_with("http") {
                Some(event.content.clone())
            } else {
                None
            }
        });
    let image = get_tag_value(event, "image").or_else(|| get_tag_value(event, "thumb"));
    let duration = get_tag_value(event, "duration").and_then(|d| d.parse::<u64>().ok());
    let season = get_tag_value(event, "season").and_then(|s| s.parse::<u32>().ok());
    let episode_number = get_tag_value(event, "episode")
        .and_then(|e| e.parse::<u32>().ok());
    let chapters_url = get_tag_value(event, "chapters");
    let topics: Vec<String> = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("t"))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
        .collect();
    let transcripts = parse_transcripts(event);
    let soundbites = parse_soundbites(event);
    let value = parse_value_block(event);
    Ok(PodcastEpisode {
        event_id: event.id.to_hex(),
        pubkey,
        d_tag,
        coordinate,
        title,
        audio_url,
        audio_type,
        pubdate,
        description,
        image,
        duration,
        topics,
        season,
        episode_number,
        chapters_url,
        transcripts,
        soundbites,
        value,
        created_at: event.created_at.as_secs(),
    })
}
/// Parse podcast metadata from Kind 30078 event (d="podcast-metadata" or similar)
/// Per NIP spec, metadata is stored as JSON in the content field
pub fn parse_podcast_metadata(event: &Event) -> Result<PodcastMetadata, String> {
    if event.kind.as_u16() != KIND_APP_DATA {
        return Err(
            format!("Expected kind {}, got {}", KIND_APP_DATA, event.kind.as_u16()),
        );
    }
    let pubkey = event.pubkey.to_hex();
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;
    let content_json: Option<serde_json::Value> = if !event.content.is_empty() {
        serde_json::from_str(&event.content).ok()
    } else {
        None
    };
    let title = content_json
        .as_ref()
        .and_then(|j| { j.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()) })
        .or_else(|| get_tag_value(event, "title"))
        .ok_or("Missing required 'title' in content or tags")?;
    let description = content_json
        .as_ref()
        .and_then(|j| {
            j.get("description").and_then(|v| v.as_str()).map(|s| s.to_string())
        })
        .or_else(|| get_tag_value(event, "description"));
    let author = content_json
        .as_ref()
        .and_then(|j| {
            j.get("author").and_then(|v| v.as_str()).map(|s| s.to_string())
        })
        .or_else(|| get_tag_value(event, "author"));
    let image = content_json
        .as_ref()
        .and_then(|j| { j.get("image").and_then(|v| v.as_str()).map(|s| s.to_string()) })
        .or_else(|| get_tag_value(event, "image"));
    let language = content_json
        .as_ref()
        .and_then(|j| {
            j.get("language").and_then(|v| v.as_str()).map(|s| s.to_string())
        })
        .or_else(|| get_tag_value(event, "language"));
    let website = content_json
        .as_ref()
        .and_then(|j| {
            j.get("website").and_then(|v| v.as_str()).map(|s| s.to_string())
        })
        .or_else(|| get_tag_value(event, "website"))
        .or_else(|| get_tag_value(event, "link"));
    let explicit = content_json
        .as_ref()
        .and_then(|j| j.get("explicit").and_then(|v| v.as_bool()))
        .or_else(|| get_tag_value(event, "explicit").map(|v| v == "true" || v == "yes"))
        .unwrap_or(false);
    let categories: Vec<String> = content_json
        .as_ref()
        .and_then(|j| j.get("categories").and_then(|v| v.as_array()))
        .map(|arr| {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
        })
        .unwrap_or_else(|| {
            event
                .tags
                .iter()
                .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("t"))
                .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
                .collect()
        });
    let funding = content_json
        .as_ref()
        .and_then(|j| j.get("funding").and_then(|v| v.as_array()))
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    let url = v.as_str().map(|s| s.to_string())?;
                    Some(FundingLink { url, name: None })
                })
                .collect()
        })
        .unwrap_or_else(|| parse_funding_links(event));
    let value = content_json
        .as_ref()
        .and_then(|j| j.get("value"))
        .and_then(parse_value_block_from_json)
        .or_else(|| parse_value_block(event));
    Ok(PodcastMetadata {
        title,
        description,
        author,
        image,
        language,
        categories,
        explicit,
        website,
        funding,
        value,
        pubkey,
        d_tag,
        created_at: event.created_at.as_secs(),
    })
}
/// Get a tag value by name (first parameter after tag name)
fn get_tag_value(event: &Event, tag_name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}
/// Get the second value of a tag (e.g., media type from ["audio", "url", "audio/mpeg"])
fn get_tag_second_value(event: &Event, tag_name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .and_then(|t| t.as_slice().get(2).map(|s| s.to_string()))
}
/// Parse transcript references from event tags
fn parse_transcripts(event: &Event) -> Vec<TranscriptRef> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("transcript"))
        .filter_map(|t| {
            let slice = t.as_slice();
            let url = slice.get(1)?.to_string();
            let transcript_type = slice
                .get(2)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "text/plain".to_string());
            let language = slice.get(3).map(|s| s.to_string());
            let rel = slice.get(4).map(|s| s.to_string());
            Some(TranscriptRef {
                url,
                transcript_type,
                language,
                rel,
            })
        })
        .collect()
}
/// Parse soundbites from event tags
fn parse_soundbites(event: &Event) -> Vec<Soundbite> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("soundbite"))
        .filter_map(|t| {
            let slice = t.as_slice();
            let start_time: f64 = slice.get(1)?.parse().ok()?;
            let duration: f64 = slice.get(2)?.parse().ok()?;
            let title = slice.get(3).map(|s| s.to_string());
            Some(Soundbite {
                start_time,
                duration,
                title,
            })
        })
        .collect()
}
/// Parse funding links from event tags
fn parse_funding_links(event: &Event) -> Vec<FundingLink> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("funding"))
        .filter_map(|t| {
            let slice = t.as_slice();
            let url = slice.get(1)?.to_string();
            let name = slice.get(2).map(|s| s.to_string());
            Some(FundingLink { url, name })
        })
        .collect()
}
/// Parse V4V value block from JSON content
fn parse_value_block_from_json(json: &serde_json::Value) -> Option<ValueBlock> {
    let obj = json.as_object()?;
    let value_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("lightning")
        .to_string();
    let method = obj
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("keysend")
        .to_string();
    let suggested = obj.get("suggested").and_then(|v| v.as_f64());
    let recipients: Vec<ValueRecipient> = obj
        .get("recipients")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| {
                    let r_obj = r.as_object()?;
                    let name = r_obj
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let recipient_type = r_obj
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("node")
                        .to_string();
                    let address = r_obj
                        .get("address")
                        .and_then(|v| v.as_str())?
                        .to_string();
                    let split = r_obj
                        .get("split")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(100) as u32;
                    let custom_key = r_obj
                        .get("customKey")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let custom_value = r_obj
                        .get("customValue")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let fee = r_obj.get("fee").and_then(|v| v.as_bool());
                    Some(ValueRecipient {
                        name,
                        recipient_type,
                        address,
                        split,
                        custom_key,
                        custom_value,
                        fee,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(ValueBlock {
        value_type,
        method,
        suggested,
        recipients,
    })
}
/// Parse V4V value block from event tags
fn parse_value_block(event: &Event) -> Option<ValueBlock> {
    let value_tag = event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("value"))?;
    let slice = value_tag.as_slice();
    let value_type = slice.get(1)?.to_string();
    let method = slice
        .get(2)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "keysend".to_string());
    let suggested: Option<f64> = slice.get(3).and_then(|s| s.parse().ok());
    let recipients: Vec<ValueRecipient> = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("valueRecipient"))
        .filter_map(|t| {
            let slice = t.as_slice();
            let name = slice
                .get(1)
                .and_then(|s| { if s.is_empty() { None } else { Some(s.to_string()) } });
            let recipient_type = slice.get(2)?.to_string();
            let address = slice.get(3)?.to_string();
            let split: u32 = slice.get(4)?.parse().ok()?;
            let custom_key = slice.get(5).map(|s| s.to_string());
            let custom_value = slice.get(6).map(|s| s.to_string());
            let fee = slice.get(7).map(|s| s == "true");
            Some(ValueRecipient {
                name,
                recipient_type,
                address,
                split,
                custom_key,
                custom_value,
                fee,
            })
        })
        .collect();
    Some(ValueBlock {
        value_type,
        method,
        suggested,
        recipients,
    })
}
/// Check if an event is podcast metadata (Kind 30078)
/// Per NIP spec, the d-tag should be the podcast's GUID (not necessarily containing "podcast")
/// We detect podcast metadata by:
/// 1. Kind 30078
/// 2. Has a d-tag containing "podcast" OR has podcast-like content/tags
pub fn is_podcast_metadata(event: &Event) -> bool {
    if event.kind.as_u16() != KIND_APP_DATA {
        return false;
    }
    let d_tag = match get_tag_value(event, "d") {
        Some(d) => d,
        None => return false,
    };
    if d_tag.to_lowercase().contains("podcast") {
        return true;
    }
    if !event.content.is_empty() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&event.content) {
            if json.get("title").is_some() {
                return true;
            }
        }
    }
    get_tag_value(event, "title").is_some()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_value_block_default() {
        let block = ValueBlock::default();
        assert_eq!(block.value_type, "lightning");
        assert_eq!(block.method, "keysend");
        assert!(block.recipients.is_empty());
    }
    #[test]
    fn test_chapter_serialization() {
        let chapter = Chapter {
            start_time: 0.0,
            title: Some("Intro".to_string()),
            img: None,
            url: None,
            toc: Some(true),
            end_time: Some(120.0),
            location: None,
        };
        let json = serde_json::to_string(&chapter).unwrap();
        assert!(json.contains("startTime"));
        assert!(json.contains("Intro"));
    }
}
