//! Podcast Event Parsing Utilities
//!
//! Handles parsing of native Nostr podcast events for both the legacy custom
//! scheme ("podcast-episodes-and-trailers") and the official NIP-F4:
//!
//! Legacy (custom NIP):
//! - Kind 30054: Podcast Episode (addressable)
//! - Kind 30055: Podcast Trailer (addressable)
//! - Kind 30078: Application-specific metadata (d="podcast-metadata")
//!
//! NIP-F4 (official):
//! - Kind 10154: Podcast Metadata (replaceable, tag-based, no d-tag)
//! - Kind 54: Podcast Episode (regular, event-id addressed)
//! - Kind 10164: Authored Podcasts (replaceable, authorship counter-claims)
//! - Kind 10054: Favorite podcasts (NIP-51; subscriptions stay on 30003)
//!
//! Use the `parse_any_*` entry points to handle both schemes transparently.
//!
//! Also includes Podcasting 2.0 data structures for V4V payments, chapters, etc.
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

// ===== Legacy custom-NIP kinds =====
/// Podcast episode event kind (addressable)
pub const KIND_PODCAST_EPISODE: u16 = 30054;
/// Application-specific data kind (for podcast metadata)
pub const KIND_APP_DATA: u16 = 30078;

// ===== NIP-F4 kinds =====
/// NIP-F4 podcast metadata (replaceable, kind 10154)
pub const KIND_F4_PODCAST_META: u16 = 10154;
/// NIP-F4 podcast episode (regular, kind 54)
pub const KIND_F4_EPISODE: u16 = 54;
/// NIP-F4 authored podcasts list (replaceable, kind 10164)
pub const KIND_F4_AUTHORED: u16 = 10164;
/// NIP-F4 favorite podcasts (NIP-51, replaceable, kind 10054)
/// (Subscriptions stay on kind 30003; this const documents the spec kind.)
#[allow(dead_code)]
pub const KIND_F4_FAVORITES: u16 = 10054;
/// A podcast author/contributor (NIP-F4 `p` tag in kind 10154)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodcastAuthor {
    /// Author public key (hex)
    pub pubkey: String,
    /// Role: `"host"`, `"cohost"` or `"editor"` (optional)
    pub role: Option<String>,
}
/// Podcast-level metadata parsed from a podcast metadata event
///
/// Supports both the legacy custom-NIP scheme (Kind 30078, JSON content) and
/// NIP-F4 (Kind 10154, tag-based). `source_kind` records which scheme produced
/// this instance so callers can build the correct address/naddr.
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
    /// d-tag identifier (empty for NIP-F4 kind 10154)
    pub d_tag: String,
    /// Created timestamp
    pub created_at: u64,
    /// NIP-F4 authors/contributors from `p` tags (empty for legacy 30078)
    #[serde(default)]
    pub authors: Vec<PodcastAuthor>,
    /// Source kind: `30078` (legacy) or `10154` (NIP-F4)
    #[serde(default = "default_metadata_kind")]
    pub source_kind: u16,
}

/// Default `source_kind` for [`PodcastMetadata`] (legacy 30078)
fn default_metadata_kind() -> u16 {
    KIND_APP_DATA
}
/// Parsed podcast episode from a podcast episode event
///
/// Supports both the legacy custom-NIP scheme (Kind 30054, addressable) and
/// NIP-F4 (Kind 54, regular). For NIP-F4 episodes `d_tag` and `coordinate`
/// are empty and the canonical identifier is `event_id`. `source_kind`
/// records which scheme produced this instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodcastEpisode {
    /// Event ID
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// d-tag identifier (empty for NIP-F4 kind 54)
    pub d_tag: String,
    /// Coordinate string: "30054:pubkey:d-tag" (empty for NIP-F4 kind 54)
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
    /// Source kind: `30054` (legacy) or `54` (NIP-F4)
    #[serde(default = "default_episode_kind")]
    pub source_kind: u16,
}

/// Default `source_kind` for [`PodcastEpisode`] (legacy 30054)
fn default_episode_kind() -> u16 {
    KIND_PODCAST_EPISODE
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
    Nostr {
        pubkey: String,
        d_tag: String,
        coordinate: String,
    },
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
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_PODCAST_EPISODE,
            event.kind.as_u16(),
        ));
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
    let audio_type = get_tag_second_value(event, "audio").or_else(|| get_tag_value(event, "type"));
    let coordinate = format!("{}:{}:{}", KIND_PODCAST_EPISODE, pubkey, d_tag);
    let pubdate = get_tag_value(event, "pubdate").or_else(|| get_tag_value(event, "published_at"));
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
    let episode_number = get_tag_value(event, "episode").and_then(|e| e.parse::<u32>().ok());
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
        source_kind: KIND_PODCAST_EPISODE,
    })
}
/// Parse podcast metadata from Kind 30078 event (d="podcast-metadata" or similar)
/// Per NIP spec, metadata is stored as JSON in the content field
pub fn parse_podcast_metadata(event: &Event) -> Result<PodcastMetadata, String> {
    if event.kind.as_u16() != KIND_APP_DATA {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_APP_DATA,
            event.kind.as_u16()
        ));
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
        .and_then(|j| {
            j.get("title")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| get_tag_value(event, "title"))
        .ok_or("Missing required 'title' in content or tags")?;
    let description = content_json
        .as_ref()
        .and_then(|j| {
            j.get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| get_tag_value(event, "description"));
    let author = content_json
        .as_ref()
        .and_then(|j| {
            j.get("author")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| get_tag_value(event, "author"));
    let image = content_json
        .as_ref()
        .and_then(|j| {
            j.get("image")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| get_tag_value(event, "image"));
    let language = content_json
        .as_ref()
        .and_then(|j| {
            j.get("language")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| get_tag_value(event, "language"));
    let website = content_json
        .as_ref()
        .and_then(|j| {
            j.get("website")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
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
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
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
        authors: Vec::new(),
        source_kind: KIND_APP_DATA,
    })
}

// ===========================================================================
// NIP-F4 parsers (kind 10154 metadata, kind 54 episodes, kind 10164 authored)
// ===========================================================================

/// Parse NIP-F4 podcast metadata from a Kind 10154 event (tag-based).
///
/// Per NIP-F4, show-level fields live in tags (`title`, `image`, `description`,
/// `website`, `p`+role). Podcasting 2.0 extras (`value`, `funding`, `t`) are
/// also parsed when present. `content` is ignored (spec mandates empty).
pub fn parse_f4_podcast_metadata(event: &Event) -> Result<PodcastMetadata, String> {
    if event.kind.as_u16() != KIND_F4_PODCAST_META {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_F4_PODCAST_META,
            event.kind.as_u16()
        ));
    }
    let pubkey = event.pubkey.to_hex();
    let title =
        get_tag_value(event, "title").ok_or("Missing required 'title' tag for NIP-F4 metadata")?;
    let description = get_tag_value(event, "description");
    let image = get_tag_value(event, "image");
    let website = get_tag_value(event, "website");
    let authors = parse_authors(event);
    let categories: Vec<String> = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("t"))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
        .collect();
    let funding = parse_funding_links(event);
    let value = parse_value_block(event);
    Ok(PodcastMetadata {
        title,
        description,
        author: None,
        image,
        language: None,
        categories,
        explicit: false,
        website,
        funding,
        value,
        pubkey,
        // NIP-F4 metadata is replaceable (kind 10154) with no d-tag.
        d_tag: String::new(),
        created_at: event.created_at.as_secs(),
        authors,
        source_kind: KIND_F4_PODCAST_META,
    })
}

/// Parse NIP-F4 podcast episode from a Kind 54 event (regular, tag-based).
///
/// Requires a non-empty `audio` tag (per NIP-F4) so unrelated kind-54 events on
/// relays are rejected. Podcasting 2.0 extras (chapters, transcripts,
/// soundbites, V4V, season/episode) are parsed from tags when present.
/// `d_tag`/`coordinate` are empty because kind 54 is regular (event-id keyed).
pub fn parse_f4_episode(event: &Event) -> Result<PodcastEpisode, String> {
    if event.kind.as_u16() != KIND_F4_EPISODE {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_F4_EPISODE,
            event.kind.as_u16()
        ));
    }
    let pubkey = event.pubkey.to_hex();
    let title =
        get_tag_value(event, "title").ok_or("Missing required 'title' tag for NIP-F4 episode")?;
    let audio_url = get_tag_value(event, "audio")
        .ok_or("Missing required 'audio' tag for NIP-F4 episode")?;
    let audio_type = get_tag_second_value(event, "audio");
    let pubdate = get_tag_value(event, "pubdate").or_else(|| get_tag_value(event, "published_at"));
    let description = get_tag_value(event, "description").or_else(|| {
        if !event.content.is_empty() {
            Some(event.content.clone())
        } else {
            None
        }
    });
    let image = get_tag_value(event, "image").or_else(|| get_tag_value(event, "thumb"));
    let duration = get_tag_value(event, "duration").and_then(|d| d.parse::<u64>().ok());
    let season = get_tag_value(event, "season").and_then(|s| s.parse::<u32>().ok());
    let episode_number = get_tag_value(event, "episode").and_then(|e| e.parse::<u32>().ok());
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
        // Kind 54 is regular: no d-tag, no coordinate. Identified by event_id.
        d_tag: String::new(),
        coordinate: String::new(),
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
        source_kind: KIND_F4_EPISODE,
    })
}

/// Parse a NIP-F4 authored-podcasts event (kind 10164).
///
/// Returns the podcast pubkeys the author claims to author. Used to
/// counter-verify the `p` tags declared in a podcast's own kind 10154 metadata.
pub fn parse_authored_event(event: &Event) -> Result<Vec<String>, String> {
    if event.kind.as_u16() != KIND_F4_AUTHORED {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_F4_AUTHORED,
            event.kind.as_u16()
        ));
    }
    let pubkeys: Vec<String> = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p"))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
        .collect();
    Ok(pubkeys)
}

// ===========================================================================
// Unified dispatchers (handle both legacy custom-NIP and NIP-F4)
// ===========================================================================

/// Parse podcast metadata from either a legacy Kind 30078 or NIP-F4 Kind 10154.
pub fn parse_any_podcast_metadata(event: &Event) -> Result<PodcastMetadata, String> {
    match event.kind.as_u16() {
        KIND_F4_PODCAST_META => parse_f4_podcast_metadata(event),
        KIND_APP_DATA => parse_podcast_metadata(event),
        k => Err(format!(
            "Expected podcast metadata kind ({} or {}), got {}",
            KIND_F4_PODCAST_META, KIND_APP_DATA, k
        )),
    }
}

/// Parse a podcast episode from either a legacy Kind 30054 or NIP-F4 Kind 54.
pub fn parse_any_episode(event: &Event) -> Result<PodcastEpisode, String> {
    match event.kind.as_u16() {
        KIND_F4_EPISODE => parse_f4_episode(event),
        KIND_PODCAST_EPISODE => parse_podcast_episode(event),
        k => Err(format!(
            "Expected podcast episode kind ({} or {}), got {}",
            KIND_F4_EPISODE, KIND_PODCAST_EPISODE, k
        )),
    }
}

/// Whether an event is podcast metadata (legacy 30078 or NIP-F4 10154).
pub fn is_any_podcast_metadata(event: &Event) -> bool {
    match event.kind.as_u16() {
        KIND_F4_PODCAST_META => get_tag_value(event, "title").is_some(),
        KIND_APP_DATA => is_podcast_metadata(event),
        _ => false,
    }
}

/// Whether an event is a podcast episode (legacy 30054 or NIP-F4 54).
#[allow(dead_code)]
pub fn is_any_episode(event: &Event) -> bool {
    match event.kind.as_u16() {
        KIND_F4_EPISODE => {
            get_tag_value(event, "title").is_some() && get_tag_value(event, "audio").is_some()
        }
        KIND_PODCAST_EPISODE => get_tag_value(event, "title").is_some(),
        _ => false,
    }
}

// ===========================================================================
// NIP-F4 greenfield builders (return EventBuilder; sign + enqueue at call site)
// ===========================================================================

/// Build a NIP-F4 podcast metadata event (kind 10154, tag-based, empty content).
///
/// Further tags (e.g. Podcasting 2.0 `value`/`funding`) may be chained onto the
/// returned builder via `.tags(...)`.
#[allow(dead_code)] // greenfield builder; no publish UI yet
#[allow(clippy::too_many_arguments)]
pub fn build_f4_metadata_event(
    title: String,
    description: Option<String>,
    image: Option<String>,
    websites: Vec<String>,
    authors: Vec<PodcastAuthor>,
) -> EventBuilder {
    let mut tags: Vec<Tag> = vec![Tag::custom(
        TagKind::Custom("title".into()),
        vec![title],
    )];
    if let Some(desc) = description {
        tags.push(Tag::custom(
            TagKind::Custom("description".into()),
            vec![desc],
        ));
    }
    if let Some(img) = image {
        tags.push(Tag::custom(TagKind::Custom("image".into()), vec![img]));
    }
    for site in websites {
        tags.push(Tag::custom(
            TagKind::Custom("website".into()),
            vec![site],
        ));
    }
    for author in authors {
        let mut vals = vec![author.pubkey];
        if let Some(role) = author.role {
            vals.push(role);
        }
        tags.push(Tag::custom(TagKind::p(), vals));
    }
    EventBuilder::new(Kind::from(KIND_F4_PODCAST_META), "").tags(tags)
}

/// Build a NIP-F4 podcast episode event (kind 54, regular).
///
/// `content_markdown` becomes the event `content` (show notes). Podcasting 2.0
/// extras may be chained onto the returned builder via `.tags(...)`.
#[allow(dead_code)] // greenfield builder; no publish UI yet
pub fn build_f4_episode_event(
    title: String,
    description: Option<String>,
    image: Option<String>,
    audio_url: String,
    audio_mime: Option<String>,
    content_markdown: String,
) -> EventBuilder {
    let mut tags: Vec<Tag> = vec![Tag::custom(
        TagKind::Custom("title".into()),
        vec![title],
    )];
    if let Some(desc) = description {
        tags.push(Tag::custom(
            TagKind::Custom("description".into()),
            vec![desc],
        ));
    }
    if let Some(img) = image {
        tags.push(Tag::custom(TagKind::Custom("image".into()), vec![img]));
    }
    let mut audio_vals = vec![audio_url];
    if let Some(mime) = audio_mime {
        audio_vals.push(mime);
    }
    tags.push(Tag::custom(TagKind::Custom("audio".into()), audio_vals));
    EventBuilder::new(Kind::from(KIND_F4_EPISODE), content_markdown).tags(tags)
}

/// Build a NIP-F4 authored-podcasts event (kind 10164) listing podcast pubkeys.
#[allow(dead_code)] // greenfield builder; no publish UI yet
pub fn build_f4_authored_event(podcast_pubkeys: Vec<String>) -> EventBuilder {
    let tags: Vec<Tag> = podcast_pubkeys
        .into_iter()
        .map(|pk| Tag::custom(TagKind::p(), vec![pk]))
        .collect();
    EventBuilder::new(Kind::from(KIND_F4_AUTHORED), "").tags(tags)
}

/// Parse NIP-F4 author `p` tags from a kind 10154 metadata event.
fn parse_authors(event: &Event) -> Vec<PodcastAuthor> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p"))
        .filter_map(|t| {
            let slice = t.as_slice();
            let pubkey = slice.get(1)?.to_string();
            let role = slice.get(2).map(|s| s.to_string());
            Some(PodcastAuthor { pubkey, role })
        })
        .collect()
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
                    let address = r_obj.get("address").and_then(|v| v.as_str())?.to_string();
                    let split = r_obj.get("split").and_then(|v| v.as_u64()).unwrap_or(100) as u32;
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
            let name = slice.get(1).and_then(|s| {
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            });
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
    #[test]
    fn test_f4_metadata_round_trip() {
        let keys = Keys::generate();
        let author_pk = Keys::generate().public_key();
        let builder = build_f4_metadata_event(
            "Test Show".to_string(),
            Some("A description".to_string()),
            Some("https://example.com/cover.png".to_string()),
            vec!["https://example.com".to_string(), "https://x.example".to_string()],
            vec![
                PodcastAuthor {
                    pubkey: author_pk.to_hex(),
                    role: Some("host".to_string()),
                },
                PodcastAuthor {
                    pubkey: Keys::generate().public_key().to_hex(),
                    role: None,
                },
            ],
        );
        let event = builder.sign_with_keys(&keys).unwrap();
        assert_eq!(event.kind.as_u16(), KIND_F4_PODCAST_META);
        let meta = parse_f4_podcast_metadata(&event).unwrap();
        assert_eq!(meta.title, "Test Show");
        assert_eq!(meta.description.as_deref(), Some("A description"));
        assert_eq!(meta.image.as_deref(), Some("https://example.com/cover.png"));
        assert_eq!(meta.website.as_deref(), Some("https://example.com"));
        assert_eq!(meta.pubkey, keys.public_key().to_hex());
        // Replaceable: no d-tag.
        assert!(meta.d_tag.is_empty());
        assert_eq!(meta.source_kind, KIND_F4_PODCAST_META);
        assert_eq!(meta.authors.len(), 2);
        assert_eq!(meta.authors[0].pubkey, author_pk.to_hex());
        assert_eq!(meta.authors[0].role.as_deref(), Some("host"));
        assert!(meta.authors[1].role.is_none());
    }
    #[test]
    fn test_f4_episode_round_trip() {
        let keys = Keys::generate();
        let builder = build_f4_episode_event(
            "Episode 1".to_string(),
            Some("Show notes teaser".to_string()),
            None,
            "https://example.com/ep1.mp3".to_string(),
            Some("audio/mpeg".to_string()),
            "## Full markdown notes".to_string(),
        );
        let event = builder.sign_with_keys(&keys).unwrap();
        assert_eq!(event.kind.as_u16(), KIND_F4_EPISODE);
        let ep = parse_f4_episode(&event).unwrap();
        assert_eq!(ep.title, "Episode 1");
        assert_eq!(ep.audio_url, "https://example.com/ep1.mp3");
        assert_eq!(ep.audio_type.as_deref(), Some("audio/mpeg"));
        assert_eq!(ep.pubkey, keys.public_key().to_hex());
        assert_eq!(ep.source_kind, KIND_F4_EPISODE);
        // Regular event: no d-tag / coordinate.
        assert!(ep.d_tag.is_empty());
        assert!(ep.coordinate.is_empty());
        assert!(!ep.event_id.is_empty());
    }
    #[test]
    fn test_f4_episode_requires_audio_tag() {
        let keys = Keys::generate();
        // A kind-54 event without an audio tag must be rejected.
        let event = EventBuilder::new(Kind::from(KIND_F4_EPISODE), "notes")
            .tag(Tag::custom(
                TagKind::Custom("title".into()),
                vec!["No Audio".to_string()],
            ))
            .sign_with_keys(&keys)
            .unwrap();
        assert!(parse_f4_episode(&event).is_err());
        assert!(!is_any_episode(&event));
    }
    #[test]
    fn test_f4_authored_round_trip() {
        let keys = Keys::generate();
        let pk1 = Keys::generate().public_key().to_hex();
        let pk2 = Keys::generate().public_key().to_hex();
        let event = build_f4_authored_event(vec![pk1.clone(), pk2.clone()])
            .sign_with_keys(&keys)
            .unwrap();
        assert_eq!(event.kind.as_u16(), KIND_F4_AUTHORED);
        let authored = parse_authored_event(&event).unwrap();
        assert_eq!(authored, vec![pk1, pk2]);
    }
    #[test]
    fn test_parse_any_dispatches_both_schemes() {
        let keys = Keys::generate();
        // NIP-F4 episode.
        let f4_event = build_f4_episode_event(
            "F4".to_string(),
            None,
            None,
            "https://example.com/a.mp3".to_string(),
            None,
            String::new(),
        )
        .sign_with_keys(&keys)
        .unwrap();
        let f4_ep = parse_any_episode(&f4_event).unwrap();
        assert_eq!(f4_ep.source_kind, KIND_F4_EPISODE);
        assert!(is_any_episode(&f4_event));

        // Legacy episode (kind 30054).
        let legacy_event = EventBuilder::new(Kind::from(KIND_PODCAST_EPISODE), "")
            .tag(Tag::identifier("ep-1"))
            .tag(Tag::custom(
                TagKind::Custom("title".into()),
                vec!["Legacy".to_string()],
            ))
            .tag(Tag::custom(
                TagKind::Custom("audio".into()),
                vec!["https://example.com/b.mp3".to_string()],
            ))
            .sign_with_keys(&keys)
            .unwrap();
        let legacy_ep = parse_any_episode(&legacy_event).unwrap();
        assert_eq!(legacy_ep.source_kind, KIND_PODCAST_EPISODE);
        assert!(!legacy_ep.d_tag.is_empty());
        assert!(is_any_episode(&legacy_event));

        // Wrong kind is rejected.
        let note = EventBuilder::text_note("hi")
            .sign_with_keys(&keys)
            .unwrap();
        assert!(parse_any_episode(&note).is_err());
        assert!(parse_any_podcast_metadata(&note).is_err());
    }
    #[test]
    fn test_f4_metadata_is_metadata_predicate() {
        let keys = Keys::generate();
        let event = build_f4_metadata_event("T".to_string(), None, None, vec![], vec![])
            .sign_with_keys(&keys)
            .unwrap();
        assert!(is_any_podcast_metadata(&event));
    }
}
