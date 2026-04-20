use crate::platform::storage;
use crate::services::trending::{get_trending_notes, TrendingNote};
use crate::stores::{nostr_client, profiles, relay};
use dioxus::prelude::ReadableExt;
use nostr_sdk::{Event, Filter, Kind, PublicKey, SingleLetterTag, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[allow(dead_code)]
const NOSTR_ARCHIVES_API: &str = "https://api.nostrarchives.com";
pub const DITTO_RELAY_URL: &str = "wss://relay.ditto.pub/";
const DITTO_STATS_PUBKEY: &str = "5f68e85ee174102ca8978eef302129f081f03456c884185d5ec1c1224ab633ea";
pub const HOT_POST_SOURCE_KEY: &str = "right_sidebar.hot_posts_source";
pub const TREND_SOURCE_KEY: &str = "right_sidebar.trend_source";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotPostSource {
    NostrWine,
    Ditto,
    Nostrarchives,
}

impl HotPostSource {
    pub fn from_query(source: &str) -> Option<Self> {
        match source {
            "nostr_wine" => Some(Self::NostrWine),
            "ditto" => Some(Self::Ditto),
            "nostrarchives" => Some(Self::Nostrarchives),
            _ => None,
        }
    }

    pub fn query_value(self) -> &'static str {
        match self {
            Self::NostrWine => "nostr_wine",
            Self::Ditto => "ditto",
            Self::Nostrarchives => "nostrarchives",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NostrWine => "nostr.wine",
            Self::Ditto => "Ditto",
            Self::Nostrarchives => "Nostrarchives",
        }
    }

    pub fn cycle(self, forward: bool) -> Self {
        match (self, forward) {
            (Self::NostrWine, true) => Self::Ditto,
            (Self::Ditto, true) => Self::Nostrarchives,
            (Self::Nostrarchives, true) => Self::NostrWine,
            (Self::Nostrarchives, false) => Self::Ditto,
            (Self::Ditto, false) => Self::NostrWine,
            (Self::NostrWine, false) => Self::Nostrarchives,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrendSource {
    Ditto,
    Nostrarchives,
}

impl TrendSource {
    #[allow(dead_code)]
    pub fn from_query(source: &str) -> Option<Self> {
        match source {
            "ditto" => Some(Self::Ditto),
            "nostrarchives" => Some(Self::Nostrarchives),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn query_value(self) -> &'static str {
        match self {
            Self::Ditto => "ditto",
            Self::Nostrarchives => "nostrarchives",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ditto => "Ditto",
            Self::Nostrarchives => "Nostrarchives",
        }
    }

    pub fn cycle(self, forward: bool) -> Self {
        match (self, forward) {
            (Self::Ditto, true) => Self::Nostrarchives,
            (Self::Nostrarchives, true) => Self::Ditto,
            (Self::Nostrarchives, false) => Self::Ditto,
            (Self::Ditto, false) => Self::Nostrarchives,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum HotPostItem {
    NostrWine(TrendingNote),
    Ditto(Event),
    Nostrarchives(NostrarchivesNote),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NostrarchivesNote {
    pub id: String,
    pub pubkey: String,
    pub created_at: i64,
    pub kind: i32,
    pub content: String,
    pub sig: String,
    pub tags: Vec<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TrendItem {
    Ditto(TrendingTagData),
    Nostrarchives(NostrarchivesTrendingTag),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NostrarchivesTrendingTag {
    pub hashtag: String,
    pub count: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrendingTagData {
    pub tag: String,
    pub accounts: u32,
    pub uses: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrendingTagsResult {
    pub tags: Vec<TrendingTagData>,
    pub label_created_at: u64,
}

pub fn load_hot_post_source() -> HotPostSource {
    storage::get(HOT_POST_SOURCE_KEY).unwrap_or(HotPostSource::Nostrarchives)
}

pub fn save_hot_post_source(source: HotPostSource) -> Result<(), String> {
    storage::set(HOT_POST_SOURCE_KEY, &source)
}

pub fn load_trend_source() -> TrendSource {
    storage::get(TREND_SOURCE_KEY).unwrap_or(TrendSource::Nostrarchives)
}

pub fn save_trend_source(source: TrendSource) -> Result<(), String> {
    storage::set(TREND_SOURCE_KEY, &source)
}

pub async fn get_hot_posts(
    source: HotPostSource,
    limit: usize,
) -> Result<Vec<HotPostItem>, String> {
    match source {
        HotPostSource::NostrWine => Ok(get_trending_notes(Some(limit))
            .await?
            .into_iter()
            .map(HotPostItem::NostrWine)
            .collect()),
        HotPostSource::Ditto => {
            let events = fetch_ditto_events(
                Filter::new()
                    .kind(Kind::TextNote)
                    .search("sort:hot protocol:nostr")
                    .limit(limit),
                Duration::from_secs(8),
            )
            .await?;
            Ok(events.into_iter().map(HotPostItem::Ditto).collect())
        }
        HotPostSource::Nostrarchives => {
            let notes = get_nostrarchives_trending_notes(limit).await?;
            Ok(notes
                .into_iter()
                .map(HotPostItem::Nostrarchives)
                .collect())
        }
    }
}

#[cfg(feature = "web")]
pub async fn get_nostrarchives_trending_notes(
    limit: usize,
) -> Result<Vec<NostrarchivesNote>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let url = format!("{}/v1/notes/trending?limit={}", NOSTR_ARCHIVES_API, limit);
    log::info!("Fetching trending from Nostrarchives: {}", url);

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);
    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("Failed to set header: {:?}", e))?;

    let window = web_sys::window().ok_or("No window object")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;
    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| "Failed to cast to Response")?;
    if !resp.ok() {
        return Err(format!("Nostrarchives API returned status: {}", resp.status()));
    }
    let json = JsFuture::from(
        resp.json()
            .map_err(|e| format!("Failed to get JSON: {:?}", e))?,
    )
    .await
    .map_err(|e| format!("Failed to parse JSON: {:?}", e))?;

    let response: NostrarchivesNotesResponse = serde_wasm_bindgen::from_value(json)
        .map_err(|e| format!("Failed to deserialize nostrarchives response: {:?}", e))?;

    log::info!(
        "Got {} trending notes from Nostrarchives",
        response.notes.len()
    );

    Ok(response
        .notes
        .into_iter()
        .map(|n| NostrarchivesNote {
            id: n.event.id,
            pubkey: n.event.pubkey,
            created_at: n.event.created_at,
            kind: n.event.kind,
            content: n.event.content,
            sig: n.event.sig,
            tags: n.event.tags,
        })
        .collect())
}

#[cfg(not(feature = "web"))]
pub async fn get_nostrarchives_trending_notes(
    _limit: usize,
) -> Result<Vec<NostrarchivesNote>, String> {
    Err("Nostrarchives trending not yet supported on native".to_string())
}

#[cfg(feature = "web")]
pub async fn get_nostrarchives_trending_tags(
    limit: usize,
) -> Result<Vec<NostrarchivesTrendingTag>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let url = format!(
        "{}/v1/hashtags/trending?limit={}",
        NOSTR_ARCHIVES_API, limit
    );
    log::info!("Fetching trending hashtags from Nostrarchives: {}", url);

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);
    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("Failed to set header: {:?}", e))?;

    let window = web_sys::window().ok_or("No window object")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;
    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| "Failed to cast to Response")?;
    if !resp.ok() {
        return Err(format!("Nostrarchives API returned status: {}", resp.status()));
    }
    let json = JsFuture::from(
        resp.json()
            .map_err(|e| format!("Failed to get JSON: {:?}", e))?,
    )
    .await
    .map_err(|e| format!("Failed to parse JSON: {:?}", e))?;

    let response: NostrarchivesHashtagsResponse = serde_wasm_bindgen::from_value(json)
        .map_err(|e| format!("Failed to deserialize nostrarchives hashtags: {:?}", e))?;

    log::info!(
        "Got {} trending hashtags from Nostrarchives",
        response.hashtags.len()
    );

    Ok(response
        .hashtags
        .into_iter()
        .map(|h| NostrarchivesTrendingTag {
            hashtag: h.hashtag,
            count: h.count as u64,
        })
        .collect())
}

#[cfg(not(feature = "web"))]
pub async fn get_nostrarchives_trending_tags(
    _limit: usize,
) -> Result<Vec<NostrarchivesTrendingTag>, String> {
    Err("Nostrarchives trending hashtags not yet supported on native".to_string())
}

pub async fn get_ditto_trending_tags(limit: usize) -> Result<TrendingTagsResult, String> {
    let stats_pubkey = PublicKey::from_hex(DITTO_STATS_PUBKEY)
        .map_err(|e| format!("Invalid Ditto stats pubkey: {e}"))?;
    let label_filter = Filter::new()
        .kind(Kind::Custom(1985))
        .author(stats_pubkey)
        .custom_tag(
            SingleLetterTag::uppercase(nostr_sdk::Alphabet::L),
            "pub.ditto.trends",
        )
        .custom_tag(SingleLetterTag::lowercase(nostr_sdk::Alphabet::L), "#t")
        .limit(1);

    let mut events = fetch_ditto_events(label_filter, Duration::from_secs(8)).await?;
    events.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    let Some(event) = events.into_iter().next() else {
        return Ok(TrendingTagsResult {
            tags: Vec::new(),
            label_created_at: 0,
        });
    };

    let mut tags: Vec<TrendingTagData> = event.tags.iter().filter_map(parse_trending_tag).collect();
    tags.sort_by(|a, b| b.uses.cmp(&a.uses).then_with(|| a.tag.cmp(&b.tag)));
    tags.truncate(limit);

    Ok(TrendingTagsResult {
        tags,
        label_created_at: event.created_at.as_secs(),
    })
}

pub async fn get_ditto_tag_sparklines(
    tags: &[String],
    label_created_at: u64,
) -> Result<HashMap<String, Vec<u32>>, String> {
    if tags.is_empty() || label_created_at == 0 {
        return Ok(HashMap::new());
    }

    let stats_pubkey = PublicKey::from_hex(DITTO_STATS_PUBKEY)
        .map_err(|e| format!("Invalid Ditto stats pubkey: {e}"))?;
    let mut series: HashMap<String, Vec<u32>> = tags
        .iter()
        .map(|tag| (tag.to_lowercase(), vec![0; 7]))
        .collect();
    let day_ranges = generate_sparkline_days(label_created_at);

    for (tag, slot_index, since, until) in tags.iter().flat_map(|tag| {
        day_ranges
            .iter()
            .enumerate()
            .map(move |(idx, (since, until))| (tag.to_lowercase(), idx, *since, *until))
    }) {
        let filter = Filter::new()
            .kind(Kind::Custom(1985))
            .author(stats_pubkey)
            .custom_tag(
                SingleLetterTag::uppercase(nostr_sdk::Alphabet::L),
                "pub.ditto.trends",
            )
            .custom_tag(SingleLetterTag::lowercase(nostr_sdk::Alphabet::L), "#t")
            .hashtag(tag.clone())
            .since(since)
            .until(until)
            .limit(1);
        let events = fetch_ditto_events(filter, Duration::from_secs(5)).await?;
        let uses = events
            .iter()
            .flat_map(|event| event.tags.iter())
            .filter_map(parse_trending_tag)
            .find(|entry| entry.tag == tag)
            .map(|entry| entry.uses)
            .unwrap_or(0);
        if let Some(bucket) = series.get_mut(&tag) {
            bucket[slot_index] = uses;
        }
    }

    Ok(series)
}

pub async fn prefetch_author_profiles(items: &[HotPostItem]) {
    let pubkeys: Vec<PublicKey> = items
        .iter()
        .filter_map(|item| match item {
            HotPostItem::NostrWine(note) => PublicKey::from_hex(&note.event.pubkey).ok(),
            HotPostItem::Ditto(event) => Some(event.pubkey),
            HotPostItem::Nostrarchives(note) => PublicKey::from_hex(&note.pubkey).ok(),
        })
        .collect();
    if !pubkeys.is_empty() {
        crate::utils::profile_prefetch::prefetch_pubkeys(pubkeys).await;
    }
}

async fn fetch_ditto_events(filter: Filter, timeout: Duration) -> Result<Vec<Event>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !relay::ensure_connected(&client, DITTO_RELAY_URL).await {
        return Err("Failed to connect to relay.ditto.pub".to_string());
    }

    relay::fetch_events_from_relays(&client, filter, vec![DITTO_RELAY_URL.to_string()], timeout)
        .await
}

fn parse_trending_tag(tag: &nostr_sdk::Tag) -> Option<TrendingTagData> {
    let values = tag.as_slice();
    if values.first().map(|item| item.as_ref()) != Some("t") {
        return None;
    }
    let tag_name = values.get(1)?.to_string().to_lowercase();
    let accounts = values.get(3)?.parse::<u32>().ok()?;
    let uses = values.get(4)?.parse::<u32>().ok()?;
    Some(TrendingTagData {
        tag: tag_name,
        accounts,
        uses,
    })
}

fn generate_sparkline_days(label_created_at: u64) -> Vec<(Timestamp, Timestamp)> {
    let dt = chrono::DateTime::from_timestamp(label_created_at as i64, 0)
        .unwrap_or_else(chrono::Utc::now);
    let midnight = dt.date_naive().and_hms_opt(0, 0, 0).unwrap();
    let mut days = Vec::with_capacity(7);
    for i in (0..7).rev() {
        let since = midnight - chrono::TimeDelta::days(i as i64);
        let until = since + chrono::TimeDelta::days(1);
        days.push((
            Timestamp::from_secs(since.and_utc().timestamp() as u64),
            Timestamp::from_secs(until.and_utc().timestamp() as u64),
        ));
    }
    days
}

pub fn profile_display_name(pubkey: &str) -> String {
    profiles::PROFILE_CACHE
        .peek()
        .peek(pubkey)
        .map(|profile| profile.get_display_name())
        .unwrap_or_else(|| crate::utils::truncate_pubkey(pubkey))
}

pub fn profile_avatar(pubkey: &str) -> String {
    profiles::PROFILE_CACHE
        .peek()
        .peek(pubkey)
        .map(|profile| profile.get_avatar_url())
        .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/identicon/svg?seed={pubkey}"))
}

#[cfg(feature = "web")]
mod nostrarchives_api_types {
    use serde::Deserialize;

    #[derive(Debug, Clone, Deserialize)]
    pub(super) struct NostrarchivesNotesResponse {
        pub notes: Vec<NostrarchivesNoteResponse>,
    }

    #[derive(Debug, Clone, Deserialize)]
    #[allow(dead_code)]
    pub struct NostrarchivesNoteResponse {
        pub event: NostrarchivesEventResponse,
        pub score: i64,
        pub zap_sats: i64,
        pub reposts: i64,
        pub replies: i64,
        pub reactions: i64,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct NostrarchivesEventResponse {
        pub id: String,
        pub pubkey: String,
        pub created_at: i64,
        pub kind: i32,
        pub content: String,
        pub sig: String,
        pub tags: Vec<Vec<String>>,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct NostrarchivesHashtagsResponse {
        pub hashtags: Vec<NostrarchivesHashtagResponse>,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct NostrarchivesHashtagResponse {
        pub hashtag: String,
        pub count: i64,
    }
}

#[cfg(feature = "web")]
use nostrarchives_api_types::{NostrarchivesHashtagsResponse, NostrarchivesNotesResponse};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ditto_trending_tag() {
        let tag = nostr_sdk::Tag::parse(["t", "nostr", "", "12", "34"]).unwrap();
        let parsed = parse_trending_tag(&tag).unwrap();
        assert_eq!(parsed.tag, "nostr");
        assert_eq!(parsed.accounts, 12);
        assert_eq!(parsed.uses, 34);
    }

    #[test]
    fn sparkline_days_cover_seven_buckets() {
        let days = generate_sparkline_days(1_700_000_000);
        assert_eq!(days.len(), 7);
        assert!(days.windows(2).all(|window| window[0].1 == window[1].0));
    }

    #[test]
    fn hot_post_source_cycle_forward() {
        assert_eq!(
            HotPostSource::NostrWine.cycle(true),
            HotPostSource::Ditto
        );
        assert_eq!(
            HotPostSource::Ditto.cycle(true),
            HotPostSource::Nostrarchives
        );
        assert_eq!(
            HotPostSource::Nostrarchives.cycle(true),
            HotPostSource::NostrWine
        );
    }

    #[test]
    fn hot_post_source_cycle_backward() {
        assert_eq!(
            HotPostSource::NostrWine.cycle(false),
            HotPostSource::Nostrarchives
        );
        assert_eq!(
            HotPostSource::Nostrarchives.cycle(false),
            HotPostSource::Ditto
        );
        assert_eq!(
            HotPostSource::Ditto.cycle(false),
            HotPostSource::NostrWine
        );
    }

    #[test]
    fn trend_source_cycle() {
        assert_eq!(TrendSource::Ditto.cycle(true), TrendSource::Nostrarchives);
        assert_eq!(TrendSource::Nostrarchives.cycle(true), TrendSource::Ditto);
        assert_eq!(TrendSource::Nostrarchives.cycle(false), TrendSource::Ditto);
        assert_eq!(TrendSource::Ditto.cycle(false), TrendSource::Nostrarchives);
    }

    #[test]
    fn hot_post_source_from_query() {
        assert_eq!(
            HotPostSource::from_query("nostrarchives"),
            Some(HotPostSource::Nostrarchives)
        );
        assert_eq!(HotPostSource::from_query("unknown"), None);
    }

    #[test]
    fn trend_source_from_query() {
        assert_eq!(
            TrendSource::from_query("nostrarchives"),
            Some(TrendSource::Nostrarchives)
        );
        assert_eq!(TrendSource::from_query("unknown"), None);
    }
}
