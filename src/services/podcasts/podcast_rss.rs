//! Podcast RSS Feed Parser
//!
//! Parses Podcasting 2.0 RSS feeds with full namespace support including:
//! - Standard RSS podcast fields
//! - podcast:value (V4V Lightning payments)
//! - podcast:chapters (JSON chapters)
//! - podcast:transcript
//! - podcast:soundbite
//! - podcast:person
//! - podcast:funding
//!
//! Reference: https://github.com/Podcastindex-org/podcast-namespace
use crate::platform::http::http_client;
use crate::utils::podcast::{
    ChaptersFile, FundingLink, Person, Soundbite, TranscriptRef, ValueBlock, ValueRecipient,
};
use serde::{Deserialize, Serialize};

/// Full Podcasting 2.0 podcast representation
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RssPodcast {
    /// Podcast GUID (from podcast:guid or generated)
    pub guid: String,
    /// Podcast title
    pub title: String,
    /// Podcast description
    pub description: Option<String>,
    /// Author/owner
    pub author: Option<String>,
    /// Cover image URL
    pub image: Option<String>,
    /// Website link
    pub link: Option<String>,
    /// Language code
    pub language: Option<String>,
    /// iTunes categories
    pub categories: Vec<String>,
    /// Episodes
    pub episodes: Vec<RssEpisode>,
    /// RSS feed URL
    pub feed_url: String,
    /// Funding links (podcast:funding)
    pub funding: Vec<FundingLink>,
    /// V4V Lightning payments (podcast:value)
    pub value: Option<ValueBlock>,
    /// Is the feed locked (podcast:locked)
    pub locked: bool,
    /// Medium type: audio, video, music, etc. (podcast:medium)
    pub medium: Option<String>,
    /// Show-level hosts (podcast:person)
    pub persons: Vec<Person>,
    /// Trailer info (podcast:trailer)
    pub trailer: Option<TrailerInfo>,
    /// License (podcast:license)
    pub license: Option<String>,
    /// Recommended shows (podcast:podroll)
    pub podroll: Vec<PodrollItem>,
    /// Explicit content flag
    pub explicit: bool,
}
/// Podcasting 2.0 episode representation
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RssEpisode {
    /// Episode GUID
    pub guid: String,
    /// Episode title
    pub title: String,
    /// Episode description/show notes
    pub description: Option<String>,
    /// Audio file URL
    pub enclosure_url: String,
    /// Audio MIME type
    pub enclosure_type: String,
    /// File length in bytes
    pub enclosure_length: Option<u64>,
    /// Publication date (RFC 2822)
    pub pub_date: Option<String>,
    /// Duration in seconds
    pub duration: Option<u64>,
    /// Episode image URL
    pub image: Option<String>,
    /// Season number
    pub season: Option<u32>,
    /// Episode number
    pub episode_number: Option<u32>,
    /// Episode type: full, trailer, bonus
    pub episode_type: Option<String>,
    /// Chapters URL (podcast:chapters) - JSON format
    pub chapters_url: Option<String>,
    /// Transcripts (podcast:transcript)
    pub transcripts: Vec<TranscriptRef>,
    /// Soundbites (podcast:soundbite)
    pub soundbites: Vec<Soundbite>,
    /// Episode-level guests (podcast:person)
    pub persons: Vec<Person>,
    /// Episode-level V4V override (podcast:value)
    pub value: Option<ValueBlock>,
    /// Location (podcast:location)
    pub location: Option<String>,
    /// Alternate enclosures (podcast:alternateEnclosure)
    pub alternate_enclosures: Vec<AlternateEnclosure>,
}
/// Alternate enclosure for different formats/bitrates
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlternateEnclosure {
    /// Media URL
    pub url: String,
    /// MIME type
    pub enclosure_type: String,
    /// File length in bytes
    pub length: Option<u64>,
    /// Bitrate in kbps
    pub bitrate: Option<u32>,
    /// Is this the default enclosure
    pub default: bool,
    /// Title/label
    pub title: Option<String>,
}
/// Trailer reference
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrailerInfo {
    /// Trailer URL
    pub url: String,
    /// Publication date
    pub pub_date: Option<String>,
    /// File length in bytes
    pub length: Option<u64>,
    /// MIME type
    pub media_type: Option<String>,
    /// Season this trailer is for
    pub season: Option<u32>,
}
/// Podroll recommendation
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodrollItem {
    /// Podcast GUID
    pub guid: String,
    /// RSS feed URL
    pub feed_url: Option<String>,
}
/// Fetch and parse a podcast RSS feed
pub async fn fetch_podcast_feed(url: &str) -> Result<RssPodcast, String> {
    log::info!("Fetching RSS feed from: {}", url);
    if url.is_empty() {
        return Err("RSS feed URL is empty".to_string());
    }
    let fetch_url = if url.starts_with("http://") {
        log::warn!("Converting HTTP to HTTPS for RSS feed: {}", url);
        url.replacen("http://", "https://", 1)
    } else {
        url.to_string()
    };
    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .get(&fetch_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch RSS feed from {}: {}", fetch_url, e))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {} from {}", response.status(), fetch_url,));
    }
    let xml = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response from {}: {}", fetch_url, e))?;
    log::info!("Successfully fetched RSS feed ({} bytes)", xml.len());
    parse_podcast_feed(&xml, url)
}
/// Parse RSS XML into a RssPodcast
pub fn parse_podcast_feed(xml: &str, feed_url: &str) -> Result<RssPodcast, String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    reader.config_mut().expand_empty_elements = true;
    let mut podcast = RssPodcast {
        guid: String::new(),
        title: String::new(),
        description: None,
        author: None,
        image: None,
        link: None,
        language: None,
        categories: Vec::new(),
        episodes: Vec::new(),
        feed_url: feed_url.to_string(),
        funding: Vec::new(),
        value: None,
        locked: false,
        medium: None,
        persons: Vec::new(),
        trailer: None,
        license: None,
        podroll: Vec::new(),
        explicit: false,
    };
    let mut current_element = String::new();
    let mut in_channel = false;
    let mut in_item = false;
    let mut in_image = false;
    let mut in_value = false;
    let mut current_channel_value: Option<ValueBlock> = None;
    let mut current_episode_value: Option<ValueBlock> = None;
    let mut current_episode: Option<RssEpisode> = None;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_element = name.clone();
                match name.as_str() {
                    "channel" => in_channel = true,
                    "image" if !in_item => in_image = true,
                    "item" => {
                        in_item = true;
                        current_episode = Some(RssEpisode {
                            guid: String::new(),
                            title: String::new(),
                            description: None,
                            enclosure_url: String::new(),
                            enclosure_type: "audio/mpeg".to_string(),
                            enclosure_length: None,
                            pub_date: None,
                            duration: None,
                            image: None,
                            season: None,
                            episode_number: None,
                            episode_type: None,
                            chapters_url: None,
                            transcripts: Vec::new(),
                            soundbites: Vec::new(),
                            persons: Vec::new(),
                            value: None,
                            location: None,
                            alternate_enclosures: Vec::new(),
                        });
                    }
                    "enclosure" => {
                        if let Some(ref mut ep) = current_episode {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref());
                                let value = String::from_utf8_lossy(&attr.value);
                                match key.as_ref() {
                                    "url" => ep.enclosure_url = value.to_string(),
                                    "type" => ep.enclosure_type = value.to_string(),
                                    "length" => ep.enclosure_length = value.parse().ok(),
                                    _ => {}
                                }
                            }
                        }
                    }
                    "podcast:value" => {
                        let value_block = parse_value_element(e);
                        in_value = true;
                        if in_item {
                            current_episode_value = Some(value_block);
                        } else if in_channel {
                            current_channel_value = Some(value_block);
                        }
                    }
                    "podcast:valueRecipient" if in_value => {
                        let recipient = parse_value_recipient_element(e);
                        if in_item {
                            if let Some(ref mut vb) = current_episode_value {
                                vb.recipients.push(recipient);
                            }
                        } else if let Some(ref mut vb) = current_channel_value {
                            vb.recipients.push(recipient);
                        }
                    }
                    "podcast:chapters" => {
                        if let Some(ref mut ep) = current_episode {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref());
                                if key == "url" {
                                    ep.chapters_url =
                                        Some(String::from_utf8_lossy(&attr.value).to_string());
                                }
                            }
                        }
                    }
                    "podcast:transcript" => {
                        if let Some(ref mut ep) = current_episode {
                            ep.transcripts.push(parse_transcript_element(e));
                        }
                    }
                    "podcast:soundbite" => {
                        if let Some(ref mut ep) = current_episode {
                            ep.soundbites.push(parse_soundbite_element(e));
                        }
                    }
                    "podcast:person" => {
                        let person = parse_person_element(e);
                        if in_item {
                            if let Some(ref mut ep) = current_episode {
                                ep.persons.push(person);
                            }
                        } else if in_channel {
                            podcast.persons.push(person);
                        }
                    }
                    "podcast:funding" => {
                        podcast.funding.push(parse_funding_element(e));
                    }
                    "podcast:trailer" => {
                        podcast.trailer = Some(parse_trailer_element(e));
                    }
                    "podcast:alternateEnclosure" => {
                        if let Some(ref mut ep) = current_episode {
                            ep.alternate_enclosures
                                .push(parse_alternate_enclosure_element(e));
                        }
                    }
                    "itunes:image" | "podcast:image" => {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref());
                            if key == "href" || key == "url" {
                                let url =
                                    upgrade_to_https(String::from_utf8_lossy(&attr.value).to_string());
                                if in_item {
                                    if let Some(ref mut ep) = current_episode {
                                        ep.image = Some(url);
                                    }
                                } else if in_channel {
                                    podcast.image = Some(url);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "channel" => in_channel = false,
                    "image" => in_image = false,
                    "podcast:value" => {
                        in_value = false;
                        if in_item {
                            if let Some(vb) = current_episode_value.take() {
                                if let Some(ref mut ep) = current_episode {
                                    ep.value = Some(vb);
                                }
                            }
                        } else if let Some(vb) = current_channel_value.take() {
                            podcast.value = Some(vb);
                        }
                    }
                    "item" => {
                        in_item = false;
                        if let Some(ep) = current_episode.take() {
                            if !ep.enclosure_url.is_empty() {
                                podcast.episodes.push(ep);
                            }
                        }
                    }
                    _ => {}
                }
                current_element.clear();
            }
            Ok(Event::Text(ref e)) => {
                let text = e.decode().unwrap_or_default().to_string();
                if text.trim().is_empty() {
                    continue;
                }
                if in_image {
                    if current_element == "url" && podcast.image.is_none() {
                        podcast.image = Some(upgrade_to_https(text));
                    }
                    continue;
                }
                if in_item {
                    if let Some(ref mut ep) = current_episode {
                        match current_element.as_str() {
                            "title" => ep.title = text,
                            "description" | "itunes:summary"
                                if ep.description.is_none() =>
                            {
                                ep.description = Some(text);
                            }
                            "guid" => ep.guid = text,
                            "pubDate" => ep.pub_date = Some(text),
                            "itunes:duration" => ep.duration = parse_duration(&text),
                            "itunes:season" => ep.season = text.parse().ok(),
                            "itunes:episode" => ep.episode_number = text.parse().ok(),
                            "itunes:episodeType" => ep.episode_type = Some(text),
                            "podcast:location" => ep.location = Some(text),
                            _ => {}
                        }
                    }
                } else if in_channel {
                    match current_element.as_str() {
                        "title" => podcast.title = text,
                        "description" | "itunes:summary"
                            if podcast.description.is_none() =>
                        {
                            podcast.description = Some(text);
                        }
                        "itunes:author" | "author" => podcast.author = Some(text),
                        "link" => podcast.link = Some(text),
                        "language" => podcast.language = Some(text),
                        "itunes:category" => podcast.categories.push(text),
                        "podcast:guid" => podcast.guid = text,
                        "podcast:medium" => podcast.medium = Some(text),
                        "podcast:license" => podcast.license = Some(text),
                        "itunes:explicit" => {
                            podcast.explicit = text == "yes" || text == "true";
                        }
                        "podcast:locked" => {
                            podcast.locked = text == "yes" || text == "true";
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::CData(ref e)) => {
                let text = String::from_utf8_lossy(e.as_ref()).to_string();
                if in_item {
                    if let Some(ref mut ep) = current_episode {
                        if current_element == "description" || current_element == "content:encoded"
                        {
                            ep.description = Some(text);
                        }
                    }
                } else if in_channel
                    && (current_element == "description" || current_element == "content:encoded")
                {
                    podcast.description = Some(text);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parsing error: {}", e)),
            _ => {}
        }
        buf.clear();
    }
    if podcast.guid.is_empty() {
        podcast.guid = generate_guid(&podcast.title, feed_url);
    }
    for ep in &mut podcast.episodes {
        if ep.guid.is_empty() {
            ep.guid = generate_guid(&ep.title, &ep.enclosure_url);
        }
    }
    Ok(podcast)
}
/// Fetch and parse JSON chapters file (direct request, may have CORS issues)
/// Note: For browser use, prefer podcast_index::fetch_chapters_proxied
#[allow(dead_code)]
pub async fn fetch_chapters(url: &str) -> Result<ChaptersFile, String> {
    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch chapters: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    response
        .json::<ChaptersFile>()
        .await
        .map_err(|e| format!("Failed to parse chapters: {}", e))
}
/// Fetch transcript content (direct request, may have CORS issues)
/// Note: For browser use, prefer podcast_index::fetch_transcript_proxied
#[allow(dead_code)]
pub async fn fetch_transcript(transcript: &TranscriptRef) -> Result<String, String> {
    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .get(&transcript.url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch transcript: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    response
        .text()
        .await
        .map_err(|e| format!("Failed to read transcript: {}", e))
}
fn upgrade_to_https(url: String) -> String {
    if url.starts_with("http://") {
        url.replacen("http://", "https://", 1)
    } else {
        url
    }
}
/// Parse duration string to seconds
/// Supports formats: "HH:MM:SS", "MM:SS", "SS", or numeric seconds
fn parse_duration(s: &str) -> Option<u64> {
    if let Ok(secs) = s.parse::<u64>() {
        return Some(secs);
    }
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        3 => {
            let hours: u64 = parts[0].parse().ok()?;
            let mins: u64 = parts[1].parse().ok()?;
            let secs: u64 = parts[2].parse().ok()?;
            Some(hours * 3600 + mins * 60 + secs)
        }
        2 => {
            let mins: u64 = parts[0].parse().ok()?;
            let secs: u64 = parts[1].parse().ok()?;
            Some(mins * 60 + secs)
        }
        _ => None,
    }
}
/// Generate a GUID from title and URL
fn generate_guid(title: &str, url: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    title.hash(&mut hasher);
    url.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}
fn parse_value_element(e: &quick_xml::events::BytesStart<'_>) -> ValueBlock {
    let mut block = ValueBlock::default();
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref());
        let value = String::from_utf8_lossy(&attr.value);
        match key.as_ref() {
            "type" => block.value_type = value.to_string(),
            "method" => block.method = value.to_string(),
            "suggested" => block.suggested = value.parse().ok(),
            _ => {}
        }
    }
    block
}
fn parse_value_recipient_element(e: &quick_xml::events::BytesStart<'_>) -> ValueRecipient {
    let mut recipient = ValueRecipient {
        name: None,
        recipient_type: "node".to_string(),
        address: String::new(),
        split: 0,
        custom_key: None,
        custom_value: None,
        fee: None,
    };
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref());
        let value = String::from_utf8_lossy(&attr.value);
        match key.as_ref() {
            "name" => recipient.name = Some(value.to_string()),
            "type" => recipient.recipient_type = value.to_string(),
            "address" => recipient.address = value.to_string(),
            "split" => recipient.split = value.parse().unwrap_or(0),
            "customKey" => recipient.custom_key = Some(value.to_string()),
            "customValue" => recipient.custom_value = Some(value.to_string()),
            "fee" => recipient.fee = Some(value == "true" || value == "1"),
            _ => {}
        }
    }
    recipient
}
fn parse_transcript_element(e: &quick_xml::events::BytesStart<'_>) -> TranscriptRef {
    let mut transcript = TranscriptRef {
        url: String::new(),
        transcript_type: "text/plain".to_string(),
        language: None,
        rel: None,
    };
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref());
        let value = String::from_utf8_lossy(&attr.value);
        match key.as_ref() {
            "url" => transcript.url = value.to_string(),
            "type" => transcript.transcript_type = value.to_string(),
            "language" => transcript.language = Some(value.to_string()),
            "rel" => transcript.rel = Some(value.to_string()),
            _ => {}
        }
    }
    transcript
}
fn parse_soundbite_element(e: &quick_xml::events::BytesStart<'_>) -> Soundbite {
    let mut soundbite = Soundbite {
        start_time: 0.0,
        duration: 30.0,
        title: None,
    };
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref());
        let value = String::from_utf8_lossy(&attr.value);
        match key.as_ref() {
            "startTime" => soundbite.start_time = value.parse().unwrap_or(0.0),
            "duration" => soundbite.duration = value.parse().unwrap_or(30.0),
            "title" => soundbite.title = Some(value.to_string()),
            _ => {}
        }
    }
    soundbite
}
fn parse_person_element(e: &quick_xml::events::BytesStart<'_>) -> Person {
    let mut person = Person {
        name: String::new(),
        role: None,
        group: None,
        img: None,
        href: None,
    };
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref());
        let value = String::from_utf8_lossy(&attr.value);
        match key.as_ref() {
            "role" => person.role = Some(value.to_string()),
            "group" => person.group = Some(value.to_string()),
            "img" => person.img = Some(value.to_string()),
            "href" => person.href = Some(value.to_string()),
            _ => {}
        }
    }
    person
}
fn parse_funding_element(e: &quick_xml::events::BytesStart<'_>) -> FundingLink {
    let mut funding = FundingLink {
        url: String::new(),
        name: None,
    };
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref());
        let value = String::from_utf8_lossy(&attr.value);
        if key == "url" {
            funding.url = value.to_string();
        }
    }
    funding
}
fn parse_trailer_element(e: &quick_xml::events::BytesStart<'_>) -> TrailerInfo {
    let mut trailer = TrailerInfo {
        url: String::new(),
        pub_date: None,
        length: None,
        media_type: None,
        season: None,
    };
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref());
        let value = String::from_utf8_lossy(&attr.value);
        match key.as_ref() {
            "url" => trailer.url = value.to_string(),
            "pubdate" => trailer.pub_date = Some(value.to_string()),
            "length" => trailer.length = value.parse().ok(),
            "type" => trailer.media_type = Some(value.to_string()),
            "season" => trailer.season = value.parse().ok(),
            _ => {}
        }
    }
    trailer
}
fn parse_alternate_enclosure_element(e: &quick_xml::events::BytesStart<'_>) -> AlternateEnclosure {
    let mut enclosure = AlternateEnclosure {
        url: String::new(),
        enclosure_type: "audio/mpeg".to_string(),
        length: None,
        bitrate: None,
        default: false,
        title: None,
    };
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref());
        let value = String::from_utf8_lossy(&attr.value);
        match key.as_ref() {
            "url" => enclosure.url = value.to_string(),
            "type" => enclosure.enclosure_type = value.to_string(),
            "length" => enclosure.length = value.parse().ok(),
            "bitrate" => enclosure.bitrate = value.parse().ok(),
            "default" => enclosure.default = value == "true",
            "title" => enclosure.title = Some(value.to_string()),
            _ => {}
        }
    }
    enclosure
}
pub use crate::utils::duration::format_duration_timecode as format_duration;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("3600"), Some(3600));
        assert_eq!(parse_duration("1:30:00"), Some(5400));
        assert_eq!(parse_duration("30:00"), Some(1800));
        assert_eq!(parse_duration("45"), Some(45));
    }
    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(3600), "1:00:00");
        assert_eq!(format_duration(90), "1:30");
        assert_eq!(format_duration(45), "0:45");
    }
    #[test]
    fn test_generate_guid() {
        let guid1 = generate_guid("Episode 1", "https://example.com/ep1.mp3");
        let guid2 = generate_guid("Episode 2", "https://example.com/ep2.mp3");
        assert_ne!(guid1, guid2);
    }
    #[test]
    fn test_parse_podcast_feed_self_closing_tags() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"
  xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"
  xmlns:podcast="https://podcastindex.org/namespace/1.0">
<channel>
    <title>Test Show</title>
    <image>
        <url>http://example.com/rss-image.jpg</url>
        <title>Test Show</title>
        <link>https://example.com</link>
    </image>
    <itunes:image href="http://example.com/itunes-art.jpg"/>
    <podcast:image href="http://example.com/podcast-art.jpg"/>
    <item>
        <title>Episode 1</title>
        <guid>ep1</guid>
        <enclosure url="https://example.com/ep1.mp3" type="audio/mpeg" length="12345"/>
        <itunes:image href="http://example.com/ep1-art.jpg"/>
        <podcast:chapters url="https://example.com/chapters.json" type="application/json"/>
    </item>
    <item>
        <title>Episode 2</title>
        <guid>ep2</guid>
        <enclosure url="https://example.com/ep2.mp3" type="audio/mpeg"/>
    </item>
</channel>
</rss>"#;
        let result = parse_podcast_feed(xml, "https://example.com/feed.xml").unwrap();
        assert_eq!(result.title, "Test Show");
        assert_eq!(
            result.image,
            Some("https://example.com/podcast-art.jpg".to_string())
        );
        assert_eq!(result.episodes.len(), 2);
        let ep1 = &result.episodes[0];
        assert_eq!(ep1.title, "Episode 1");
        assert_eq!(ep1.enclosure_url, "https://example.com/ep1.mp3");
        assert_eq!(ep1.enclosure_type, "audio/mpeg");
        assert_eq!(
            ep1.image,
            Some("https://example.com/ep1-art.jpg".to_string())
        );
        assert_eq!(
            ep1.chapters_url,
            Some("https://example.com/chapters.json".to_string())
        );
        let ep2 = &result.episodes[1];
        assert_eq!(ep2.title, "Episode 2");
    }
    #[test]
    fn test_parse_podcast_feed_image_fallback() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
<channel>
    <title>Fallback Show</title>
    <image>
        <url>http://example.com/fallback.jpg</url>
        <title>Fallback Show</title>
        <link>https://example.com</link>
    </image>
    <item>
        <title>Episode A</title>
        <guid>epA</guid>
        <enclosure url="https://example.com/epA.mp3" type="audio/mpeg"/>
    </item>
</channel>
</rss>"#;
        let result = parse_podcast_feed(xml, "https://example.com/feed.xml").unwrap();
        assert_eq!(
            result.image,
            Some("https://example.com/fallback.jpg".to_string())
        );
    }
    #[test]
    fn test_upgrade_to_https() {
        assert_eq!(
            upgrade_to_https("http://example.com/img.jpg".to_string()),
            "https://example.com/img.jpg"
        );
        assert_eq!(
            upgrade_to_https("https://example.com/img.jpg".to_string()),
            "https://example.com/img.jpg"
        );
        assert_eq!(
            upgrade_to_https("//example.com/img.jpg".to_string()),
            "//example.com/img.jpg"
        );
    }
}
