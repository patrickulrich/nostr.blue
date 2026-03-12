use crate::platform::storage;
use crate::services::trending::{get_trending_notes, TrendingNote};
use crate::stores::{nostr_client, profiles, relay};
use dioxus::prelude::ReadableExt;
use nostr_sdk::{Event, Filter, Kind, PublicKey, SingleLetterTag, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

pub const DITTO_RELAY_URL: &str = "wss://relay.ditto.pub/";
const DITTO_STATS_PUBKEY: &str = "5f68e85ee174102ca8978eef302129f081f03456c884185d5ec1c1224ab633ea";
pub const HOT_POST_SOURCE_KEY: &str = "right_sidebar.hot_posts_source";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotPostSource {
    NostrWine,
    Ditto,
}

impl HotPostSource {
    pub fn from_query(source: &str) -> Option<Self> {
        match source {
            "nostr_wine" => Some(Self::NostrWine),
            "ditto" => Some(Self::Ditto),
            _ => None,
        }
    }

    pub fn query_value(self) -> &'static str {
        match self {
            Self::NostrWine => "nostr_wine",
            Self::Ditto => "ditto",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NostrWine => "nostr.wine",
            Self::Ditto => "Ditto",
        }
    }

    pub fn cycle(self, forward: bool) -> Self {
        match (self, forward) {
            (Self::NostrWine, true) | (Self::Ditto, false) => Self::Ditto,
            (Self::Ditto, true) | (Self::NostrWine, false) => Self::NostrWine,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum HotPostItem {
    NostrWine(TrendingNote),
    Ditto(Event),
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
    storage::get(HOT_POST_SOURCE_KEY).unwrap_or(HotPostSource::NostrWine)
}

pub fn save_hot_post_source(source: HotPostSource) -> Result<(), String> {
    storage::set(HOT_POST_SOURCE_KEY, &source)
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
    }
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
    events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
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
}
