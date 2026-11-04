//! Podcast boost discovery — aggregates Podcasting 2.0 boost notes from Nostr.
//!
//! A boost note is a kind-1 note that names a podcast feed or episode through
//! NIP-73 external-content tags and carries payment evidence. Two single-letter
//! tags make them relay-queryable:
//!
//! - `k` = `podcast:guid` / `podcast:item:guid` (the marker)
//! - `i` = `podcast:guid:<guid>` / `podcast:item:guid:<guid>` (the identifier)
//!
//! Payment evidence is parsed from local tags and text only (no follow-up
//! fetches): an `amount` tag in millisats, a boost topic tag (`boostagram`,
//! `value4value`, `boost`), or a `"<n> sats"` figure in the message.
//!
//! One cached scan over a trailing window feeds every boost-derived rail on
//! the /podcasts Discover tab (per-show, per-episode, per-booster and
//! follows-scoped rollups are derived client-side from the same records):
//! - the user's relay pool (or the default pool when signed out)
//! - a targeted fetch against boost-dense relays that most podcast apps
//!   publish to but that are not part of a general pool
use crate::stores::nostr_client;
use nostr::event::{Event, EventId};
use nostr::filter::{Filter, SingleLetterTag};
use nostr::prelude::{Kind, Tag, Timestamp};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

/// Single-letter `k` tag values that mark a note as referencing a podcast
/// feed or episode (NIP-73).
const K_TAG_FEED: &str = "podcast:guid";
const K_TAG_ITEM: &str = "podcast:item:guid";
/// `i` tag value prefixes carrying the actual GUIDs.
const I_TAG_FEED_PREFIX: &str = "podcast:guid:";
const I_TAG_ITEM_PREFIX: &str = "podcast:item:guid:";
/// Topic tags that mark a note as a boost when no amount is present.
const BOOST_TOPIC_TAGS: [&str; 3] = ["boostagram", "value4value", "boost"];

/// Relays outside the general pool where boost notes concentrate. Podcast
/// apps predominantly publish their boost notes to one or two dedicated
/// relays; these three cover the bulk of what the pool misses.
const BOOST_DENSE_RELAYS: [&str; 3] = [
    "wss://relay.fountain.fm",
    "wss://podtards.com",
    "wss://relay.wavlake.com",
];

/// Trailing scan window. The rails rank over the last 7 days; one extra day
/// of slack absorbs clock skew between relays and this device.
const SCAN_WINDOW_SECS: u64 = 8 * 24 * 60 * 60;
/// How long a scan stays fresh. Boost notes arrive continuously; five
/// minutes keeps repeat visits cheap without serving stale charts.
const CACHE_TTL: Duration = Duration::from_secs(5 * 60);
/// Per-source event ceiling. A week of network-wide boost notes is low four
/// figures; this only guards a pathological relay response.
const SCAN_LIMIT: usize = 5_000;
/// Deadline per fetch source (pool + targeted each get their own window).
const SCAN_TIMEOUT: Duration = Duration::from_secs(12);

/// One classified boost note.
#[derive(Clone, Debug, PartialEq)]
pub struct BoostRecord {
    pub event_id: String,
    pub booster_pubkey: String,
    pub created_at: u64,
    /// Whole sats parsed from local evidence, when present.
    pub sats: Option<u64>,
    pub podcast_guid: Option<String>,
    pub item_guid: Option<String>,
    pub message: Option<String>,
}

/// Per-show aggregate over a set of records.
#[derive(Clone, Debug, PartialEq)]
pub struct ShowRollup {
    pub podcast_guid: String,
    pub sats: u64,
    pub boosts: usize,
    pub boosters: usize,
}

/// Per-episode aggregate over a set of records.
#[derive(Clone, Debug, PartialEq)]
pub struct EpisodeRollup {
    pub item_guid: String,
    pub podcast_guid: Option<String>,
    pub sats: u64,
    pub boosts: usize,
    pub boosters: usize,
}

/// Per-booster aggregate over a set of records.
#[derive(Clone, Debug, PartialEq)]
pub struct BoosterRollup {
    pub pubkey: String,
    pub sats: u64,
    pub boosts: usize,
}

/// Scan cache shared by every boost rail (see module docs).
static BOOST_CACHE: Mutex<Option<(instant::Instant, Vec<BoostRecord>)>> = Mutex::new(None);

/// Content sats fallbacks: a number next to "boost"/⚡ first, then any
/// "<n> sats" figure. Numbers may carry thousands separators.
static SATS_NEAR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(?:boost\w*|⚡)[^\d]{0,6}([\d][\d,]{0,15})\s*sats").unwrap());
static SATS_ANY: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)([\d][\d,]{0,15})\s*sats\b").unwrap());

fn tag_value(tag: &Tag, index: usize) -> Option<&str> {
    tag.as_slice().get(index).map(|s| s.as_str())
}

/// Best-effort whole sats parsed from a boost note's message text.
fn parse_content_sats(content: &str) -> Option<u64> {
    if content.is_empty() {
        return None;
    }
    for rx in [&*SATS_NEAR, &*SATS_ANY] {
        if let Some(m) = rx.captures(content) {
            if let Some(v) = m
                .get(1)
                .and_then(|d| d.as_str().replace(',', "").parse::<u64>().ok())
            {
                if v > 0 {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Classify one kind-1 note as a boost, or `None` when it lacks a NIP-73
/// podcast identifier or any payment evidence.
pub fn classify_boost(event: &Event) -> Option<BoostRecord> {
    let mut podcast_guid = None;
    let mut item_guid = None;
    let mut topic_boost = false;
    let mut amount_msats: Option<u64> = None;

    for tag in event.tags.iter() {
        match tag_value(tag, 0) {
            Some("i") => {
                if let Some(v) = tag_value(tag, 1) {
                    if let Some(guid) = v.strip_prefix(I_TAG_ITEM_PREFIX) {
                        if !guid.is_empty() {
                            item_guid = Some(guid.to_string());
                        }
                    } else if let Some(guid) = v.strip_prefix(I_TAG_FEED_PREFIX) {
                        if !guid.is_empty() {
                            podcast_guid = Some(guid.to_string());
                        }
                    }
                }
            }
            Some("t") => {
                if let Some(v) = tag_value(tag, 1) {
                    if BOOST_TOPIC_TAGS.contains(&v.to_lowercase().as_str()) {
                        topic_boost = true;
                    }
                }
            }
            Some("amount") => {
                if let Some(v) = tag_value(tag, 1) {
                    // Amount tags carry millisats as a bare integer; tolerate a
                    // trailing unit word by taking the leading digits.
                    let digits: String = v.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if let Ok(msats) = digits.parse::<u64>() {
                        if msats > 0 {
                            amount_msats = Some(msats);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if podcast_guid.is_none() && item_guid.is_none() {
        return None; // no NIP-73 podcast identifier
    }

    let sats = amount_msats.map(|msats| msats / 1000);
    let content_sats = if sats.is_none() {
        parse_content_sats(&event.content)
    } else {
        None
    };
    if sats.is_none() && content_sats.is_none() && !topic_boost {
        return None; // names a podcast but shows no payment evidence
    }

    Some(BoostRecord {
        event_id: event.id.to_hex(),
        booster_pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_secs(),
        sats: sats.or(content_sats),
        podcast_guid,
        item_guid,
        message: if event.content.is_empty() {
            None
        } else {
            Some(event.content.clone())
        },
    })
}

/// The boost-note filter every scan source uses.
fn scan_filter() -> Filter {
    Filter::new()
        .kind(Kind::TextNote)
        .custom_tags(
            SingleLetterTag::from_char('k').expect("valid single-letter tag"),
            [K_TAG_FEED, K_TAG_ITEM],
        )
        .since(Timestamp::now() - Duration::from_secs(SCAN_WINDOW_SECS))
        .limit(SCAN_LIMIT)
}

/// Fetch boost notes from every source, merge, dedupe and classify.
async fn scan() -> Result<Vec<BoostRecord>, String> {
    let filter = scan_filter();
    let mut merged: HashMap<EventId, Event> = HashMap::new();

    // Source 1: the relay pool (user relays when signed in, defaults otherwise).
    match nostr_client::fetch_events_aggregated(filter.clone(), SCAN_TIMEOUT).await {
        Ok(events) => {
            log::info!("[boost_scan] pool fetch returned {} events", events.len());
            for event in events {
                merged.insert(event.id, event);
            }
        }
        Err(e) => log::warn!("[boost_scan] pool fetch failed: {}", e),
    }

    // Source 2: boost-dense relays that most podcast apps publish to.
    if let Some(client) = nostr_client::get_client() {
        let urls: Vec<String> = BOOST_DENSE_RELAYS.iter().map(|s| s.to_string()).collect();
        let ephemeral =
            crate::stores::relay::coverage::connect_ephemeral_relays(&client, &urls).await;
        if !ephemeral.connected.is_empty() {
            match crate::stores::relay::connection::fetch_events_from_relays(
                &client,
                filter,
                ephemeral.connected,
                SCAN_TIMEOUT,
            )
            .await
            {
                Ok(events) => {
                    log::info!(
                        "[boost_scan] boost-dense relays returned {} events",
                        events.len()
                    );
                    for event in events {
                        merged.insert(event.id, event);
                    }
                }
                Err(e) => log::warn!("[boost_scan] boost-dense fetch failed: {}", e),
            }
        }
    }

    let records: Vec<BoostRecord> = merged.values().filter_map(classify_boost).collect();
    log::info!(
        "[boost_scan] {} notes classified as boosts from {} candidates",
        records.len(),
        merged.len()
    );
    Ok(records)
}

/// The cached scan every boost rail reads. Returns the trailing-window
/// records; callers derive rollups and apply per-rail time cuts.
pub async fn cached_boosts() -> Result<Vec<BoostRecord>, String> {
    {
        let guard = BOOST_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, records)) = guard.as_ref() {
            if at.elapsed() < CACHE_TTL {
                return Ok(records.clone());
            }
        }
    }
    let records = scan().await?;
    let mut guard = BOOST_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some((instant::Instant::now(), records.clone()));
    Ok(records)
}

fn in_window(record: &BoostRecord, since: u64) -> bool {
    record.created_at >= since
}

/// Aggregate records into per-show rankings (sats, then boosters, then
/// boosts). Records without a feed guid (episode-only boosts) are skipped —
/// they are covered by [`rollup_episodes`].
pub fn rollup_shows(records: &[BoostRecord], since: u64) -> Vec<ShowRollup> {
    struct Acc {
        sats: u64,
        boosts: usize,
        boosters: HashSet<String>,
    }
    let mut by_show: HashMap<String, Acc> = HashMap::new();
    for record in records.iter().filter(|r| in_window(r, since)) {
        let Some(guid) = record.podcast_guid.as_deref() else {
            continue;
        };
        let acc = by_show
            .entry(guid.to_string())
            .or_insert_with(|| Acc {
                sats: 0,
                boosts: 0,
                boosters: HashSet::new(),
            });
        acc.sats += record.sats.unwrap_or(0);
        acc.boosts += 1;
        acc.boosters.insert(record.booster_pubkey.clone());
    }
    let mut rollups: Vec<ShowRollup> = by_show
        .into_iter()
        .map(|(podcast_guid, acc)| ShowRollup {
            podcast_guid,
            sats: acc.sats,
            boosts: acc.boosts,
            boosters: acc.boosters.len(),
        })
        .collect();
    rollups.sort_by(|a, b| b.sats.cmp(&a.sats).then(b.boosters.cmp(&a.boosters)).then(b.boosts.cmp(&a.boosts)));
    rollups
}

/// Aggregate records into per-episode rankings.
pub fn rollup_episodes(records: &[BoostRecord], since: u64) -> Vec<EpisodeRollup> {
    struct Acc {
        podcast_guid: Option<String>,
        sats: u64,
        boosts: usize,
        boosters: HashSet<String>,
    }
    let mut by_item: HashMap<String, Acc> = HashMap::new();
    for record in records.iter().filter(|r| in_window(r, since)) {
        let Some(guid) = record.item_guid.as_deref() else {
            continue;
        };
        let acc = by_item
            .entry(guid.to_string())
            .or_insert_with(|| Acc {
                podcast_guid: record.podcast_guid.clone(),
                sats: 0,
                boosts: 0,
                boosters: HashSet::new(),
            });
        acc.sats += record.sats.unwrap_or(0);
        acc.boosts += 1;
        acc.boosters.insert(record.booster_pubkey.clone());
    }
    let mut rollups: Vec<EpisodeRollup> = by_item
        .into_iter()
        .map(|(item_guid, acc)| EpisodeRollup {
            item_guid,
            podcast_guid: acc.podcast_guid,
            sats: acc.sats,
            boosts: acc.boosts,
            boosters: acc.boosters.len(),
        })
        .collect();
    rollups.sort_by(|a, b| b.sats.cmp(&a.sats).then(b.boosters.cmp(&a.boosters)).then(b.boosts.cmp(&a.boosts)));
    rollups
}

/// Aggregate records into per-booster rankings.
pub fn rollup_boosters(records: &[BoostRecord], since: u64) -> Vec<BoosterRollup> {
    let mut by_booster: HashMap<String, (u64, usize)> = HashMap::new();
    for record in records.iter().filter(|r| in_window(r, since)) {
        let acc = by_booster
            .entry(record.booster_pubkey.clone())
            .or_insert((0, 0));
        acc.0 += record.sats.unwrap_or(0);
        acc.1 += 1;
    }
    let mut rollups: Vec<BoosterRollup> = by_booster
        .into_iter()
        .map(|(pubkey, (sats, boosts))| BoosterRollup {
            pubkey,
            sats,
            boosts,
        })
        .collect();
    rollups.sort_by(|a, b| b.sats.cmp(&a.sats).then(b.boosts.cmp(&a.boosts)));
    rollups
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::event::FinalizeEvent;
    use nostr::prelude::EventBuilder;

    fn boost_event(tags: Vec<Tag>, content: &str, created_at: u64) -> Event {
        let keys = nostr::key::Keys::generate();
        EventBuilder::new(Kind::TextNote, content)
            .custom_created_at(Timestamp::from(created_at))
            .tags(tags)
            .finalize(&keys)
            .expect("sign")
    }

    fn record(
        booster: &str,
        created_at: u64,
        sats: Option<u64>,
        podcast_guid: Option<&str>,
        item_guid: Option<&str>,
    ) -> BoostRecord {
        BoostRecord {
            event_id: format!("{}-{}", booster, created_at),
            booster_pubkey: booster.to_string(),
            created_at,
            sats,
            podcast_guid: podcast_guid.map(String::from),
            item_guid: item_guid.map(String::from),
            message: None,
        }
    }

    #[test]
    fn classify_parses_amount_tag_and_guids() {
        let event = boost_event(
            vec![
                Tag::custom("i", vec!["podcast:guid:feed-1"]),
                Tag::custom("i", vec!["podcast:item:guid:item-1"]),
                Tag::custom("amount", vec!["21000"]),
                Tag::custom("t", vec!["boostagram"]),
            ],
            "Great episode!",
            1000,
        );
        let boost = classify_boost(&event).expect("is a boost");
        assert_eq!(boost.sats, Some(21));
        assert_eq!(boost.podcast_guid.as_deref(), Some("feed-1"));
        assert_eq!(boost.item_guid.as_deref(), Some("item-1"));
    }

    #[test]
    fn classify_parses_amount_tag_with_unit_suffix() {
        let event = boost_event(
            vec![
                Tag::custom("i", vec!["podcast:guid:feed-1"]),
                Tag::custom("amount", vec!["5000 millisats"]),
            ],
            "",
            1000,
        );
        let boost = classify_boost(&event).expect("is a boost");
        assert_eq!(boost.sats, Some(5));
    }

    #[test]
    fn classify_topic_tag_without_amount_is_a_boost() {
        let event = boost_event(
            vec![
                Tag::custom("i", vec!["podcast:item:guid:item-1"]),
                Tag::custom("t", vec!["value4value"]),
            ],
            "enjoying this",
            1000,
        );
        let boost = classify_boost(&event).expect("is a boost");
        assert_eq!(boost.sats, None);
        assert_eq!(boost.item_guid.as_deref(), Some("item-1"));
    }

    #[test]
    fn classify_falls_back_to_content_sats() {
        let event = boost_event(
            vec![Tag::custom("i", vec!["podcast:guid:feed-1"])],
            "boosted 1,500 sats to the show",
            1000,
        );
        let boost = classify_boost(&event).expect("is a boost");
        assert_eq!(boost.sats, Some(1500));
    }

    #[test]
    fn classify_rejects_note_without_podcast_identifier() {
        let event = boost_event(
            vec![Tag::custom("amount", vec!["21000"]), Tag::custom("t", vec!["boostagram"])],
            "not about a podcast",
            1000,
        );
        assert!(classify_boost(&event).is_none());
    }

    #[test]
    fn classify_rejects_note_without_payment_evidence() {
        let event = boost_event(
            vec![Tag::custom("i", vec!["podcast:guid:feed-1"])],
            "liked this episode",
            1000,
        );
        assert!(classify_boost(&event).is_none());
    }

    #[test]
    fn show_rollup_counts_distinct_boosters_and_orders_by_sats() {
        let now = 10_000u64;
        let records = vec![
            record("a", now, Some(100), Some("feed-a"), None),
            record("a", now - 10, Some(50), Some("feed-a"), None),
            record("b", now - 20, Some(10), Some("feed-a"), None),
            record("c", now - 30, Some(60), Some("feed-b"), None),
            // episode-only boost: excluded from show rollups
            record("d", now, Some(999), None, Some("item-x")),
            // outside the window
            record("e", 0, Some(999), Some("feed-a"), None),
        ];
        let rollups = rollup_shows(&records, 100);
        assert_eq!(rollups.len(), 2);
        assert_eq!(rollups[0].podcast_guid, "feed-a");
        assert_eq!(rollups[0].sats, 160);
        assert_eq!(rollups[0].boosts, 3);
        assert_eq!(rollups[0].boosters, 2);
        assert_eq!(rollups[1].podcast_guid, "feed-b");
    }

    #[test]
    fn episode_rollup_groups_by_item_guid() {
        let records = vec![
            record("a", 100, Some(30), Some("feed-1"), Some("item-1")),
            record("b", 100, Some(20), Some("feed-1"), Some("item-1")),
            record("c", 100, Some(500), None, Some("item-2")),
        ];
        let rollups = rollup_episodes(&records, 0);
        assert_eq!(rollups.len(), 2);
        assert_eq!(rollups[0].item_guid, "item-2"); // highest sats first
        assert_eq!(rollups[0].podcast_guid, None);
        assert_eq!(rollups[1].boosters, 2);
    }

    #[test]
    fn booster_rollup_sums_sats_per_pubkey() {
        let records = vec![
            record("alice", 100, Some(100), Some("f"), None),
            record("alice", 100, Some(40), Some("g"), None),
            record("bob", 100, Some(500), Some("f"), None),
            // no amount parsed: still counted as a boost, contributes 0 sats
            record("carol", 100, None, Some("f"), None),
        ];
        let rollups = rollup_boosters(&records, 0);
        assert_eq!(rollups.len(), 3);
        assert_eq!(rollups[0].pubkey, "bob");
        assert_eq!(rollups[0].sats, 500);
        assert_eq!(rollups[1].pubkey, "alice");
        assert_eq!(rollups[1].sats, 140);
        assert_eq!(rollups[1].boosts, 2);
    }
}
