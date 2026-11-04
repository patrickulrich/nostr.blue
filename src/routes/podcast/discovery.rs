//! Discovery rails for the /podcasts Discover tab.
//!
//! A stack of independent horizontal rails, each with its own loading,
//! empty and (where needed) sign-in states — one rail failing never breaks
//! the others:
//!
//! 1. Trending This Week / This Month — Podcast Index trending, time-windowed
//! 2. Most Boosted Shows / Top Boosted Episodes This Week — Nostr boost-note
//!    rollups enriched from Podcast Index (see `services/podcasts/boost_scan`)
//! 3. Top Boosters This Week — boost rollups over nostr profiles
//! 4. From Your Follows — boosts from the signed-in user's kind-3 contacts;
//!    signed-out visitors see the locked sign-in card
//! 5. New and Noteworthy / New V4V Feeds — recently added Podcast Index feeds
//! 6. Fresh on Nostr — native nostr podcast shows
//! 7. Top Podcasts by Language — trending per language derived from the
//!    month chart's language mix
use crate::components::podcast::{
    DiscoveryRail, LockedRailCard, RailAuthRequired, RailBadge, RailFooter, RailTileData,
};
use crate::components::PodcastShow;
use crate::routes::podcast::home::{fetch_nostr_podcast_shows, get_show_route};
use crate::routes::Route;
use crate::services::podcast_index;
use crate::services::podcasts::boost_scan;
use crate::stores::{auth_store, nostr_client, profiles};
use crate::utils::format::format_sats_compact;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr::prelude::Timestamp;

/// Tiles per rail track.
const RAIL_SIZE: usize = 12;
/// Seconds per day, for window math.
const DAY: u64 = 24 * 60 * 60;
/// The boost rails rank over the trailing week.
const BOOST_WINDOW_DAYS: u64 = 7;

fn now_secs() -> u64 {
    Timestamp::now().as_secs()
}

fn plural_s(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Map feeds → ranked tiles, clamped to the rail size.
fn trending_tiles(feeds: &[podcast_index::PodcastFeed]) -> Vec<RailTileData> {
    feeds
        .iter()
        .take(RAIL_SIZE)
        .enumerate()
        .map(|(i, feed)| RailTileData {
            rank: Some(i + 1),
            ..RailTileData::rss_show(feed)
        })
        .collect()
}

/// Show-rollup row → tile, with Podcast Index enrichment (artwork, link,
/// badges). Unresolvable GUIDs are skipped: a tile with no page to open
/// would be a dead end.
async fn boosted_show_tiles(only_boosters: Option<&[String]>) -> Result<Vec<RailTileData>, String> {
    let records = boost_scan::cached_boosts().await?;
    let filtered: Vec<boost_scan::BoostRecord> = match only_boosters {
        Some(set) => records
            .iter()
            .filter(|r| set.contains(&r.booster_pubkey))
            .cloned()
            .collect(),
        None => records,
    };
    let since = now_secs().saturating_sub(BOOST_WINDOW_DAYS * DAY);
    let rollups = boost_scan::rollup_shows(&filtered, since);
    let top: Vec<boost_scan::ShowRollup> = rollups.into_iter().take(RAIL_SIZE).collect();
    let feeds = futures::future::join_all(
        top.iter()
            .map(|r| podcast_index::get_podcast_by_guid(&r.podcast_guid)),
    )
    .await;
    let tiles = top
        .iter()
        .enumerate()
        .zip(feeds)
        .filter_map(|((i, rollup), feed)| {
            let feed = feed.ok()?;
            Some(RailTileData {
                key: format!("boost-show-{}", rollup.podcast_guid),
                title: feed.title.clone(),
                subtitle: Some(format!(
                    "{} sats · {} booster{}",
                    format_sats_compact(rollup.sats),
                    rollup.boosters,
                    plural_s(rollup.boosters),
                )),
                subtitle_title: Some(format!(
                    "{} boost{} sent {} this week",
                    rollup.boosts,
                    plural_s(rollup.boosts),
                    format_sats_compact(rollup.sats),
                )),
                image: feed.get_image().map(String::from),
                rank: Some(i + 1),
                badge: feed.has_v4v().then_some(RailBadge::V4v),
                person: false,
                route: Some(Route::PodcastRssFeedDetail {
                    podcast_id: feed.id.to_string(),
                }),
                glyph: "⚡",
            })
        })
        .collect();
    Ok(tiles)
}

#[derive(Props, Clone, PartialEq)]
pub struct TrendingRailProps {
    /// Window length in days.
    pub days: u64,
}

/// Podcast Index trending chart over a trailing window.
#[component]
pub fn TrendingRail(props: TrendingRailProps) -> Element {
    let has_signer = *nostr_client::HAS_SIGNER.read();
    let feeds = use_resource(move || {
        let days = props.days;
        let ready =
            *nostr_client::CLIENT_INITIALIZED.read() && *nostr_client::HAS_SIGNER.read();
        async move {
            if !ready {
                return Ok(Vec::new());
            }
            let since = now_secs().saturating_sub(days * DAY);
            podcast_index::get_trending_window(Some(30), Some(since), None, None).await
        }
    });
    let title = if props.days <= BOOST_WINDOW_DAYS {
        "Trending This Week"
    } else {
        "Trending This Month"
    };
    if !has_signer {
        return rsx! { RailAuthRequired { item_label: "trending charts" } };
    }
    let state = feeds.read();
    let tiles = match state.as_ref() {
        Some(Ok(feeds)) => Some(trending_tiles(feeds)),
        Some(Err(_)) => Some(Vec::new()),
        None => None,
    };
    rsx! {
        DiscoveryRail {
            title,
            footer: RailFooter::Link {
                to: Route::PodcastTrending {},
                label: "See All →".to_string(),
            },
            tiles,
            empty_message: "No trending podcasts in this window.".to_string(),
        }
    }
}

/// Shows ranked by the sats Nostr users sent them this week.
#[component]
pub fn BoostedShowsRail() -> Element {
    let has_signer = *nostr_client::HAS_SIGNER.read();
    let tiles = use_resource(move || {
        let ready =
            *nostr_client::CLIENT_INITIALIZED.read() && *nostr_client::HAS_SIGNER.read();
        async move {
            if !ready {
                return Ok::<_, String>(Vec::<RailTileData>::new());
            }
            boosted_show_tiles(None).await
        }
    });
    if !has_signer {
        return rsx! { RailAuthRequired { item_label: "boost charts" } };
    }
    let state = tiles.read();
    let tiles = match state.as_ref() {
        Some(Ok(t)) => Some(t.clone()),
        Some(Err(_)) => Some(Vec::new()),
        None => None,
    };
    rsx! {
        DiscoveryRail {
            title: "Most Boosted Shows This Week".to_string(),
            footer: RailFooter::Note("Ranked by sats sent in boost notes on Nostr".to_string()),
            tiles,
            empty_message: "No podcast boosts on Nostr in the last week.".to_string(),
        }
    }
}

/// Episodes ranked by the sats Nostr users sent them this week.
#[component]
pub fn BoostedEpisodesRail() -> Element {
    let has_signer = *nostr_client::HAS_SIGNER.read();
    let tiles = use_resource(move || {
        let ready =
            *nostr_client::CLIENT_INITIALIZED.read() && *nostr_client::HAS_SIGNER.read();
        async move {
            if !ready {
                return Ok::<_, String>(Vec::<RailTileData>::new());
            }
            let records = boost_scan::cached_boosts().await?;
            let since = now_secs().saturating_sub(BOOST_WINDOW_DAYS * DAY);
            let rollups = boost_scan::rollup_episodes(&records, since);
            let top: Vec<boost_scan::EpisodeRollup> =
                rollups.into_iter().take(RAIL_SIZE).collect();
            let lookups = futures::future::join_all(top.iter().map(|r| {
                podcast_index::get_episode_by_guid(&r.item_guid, r.podcast_guid.as_deref())
            }))
            .await;
            let tiles = top
                .iter()
                .enumerate()
                .zip(lookups)
                .filter_map(|((i, rollup), lookup)| {
                    let (episode, feed) = lookup.ok()?;
                    let podcast_id = feed
                        .as_ref()
                        .map(|f| f.id.to_string())
                        .or_else(|| episode.feed_id.map(|id| id.to_string()))?;
                    Some(RailTileData {
                        key: format!("boost-ep-{}", rollup.item_guid),
                        title: episode.title.clone(),
                        subtitle: Some(format!(
                            "{} sats · {} booster{}",
                            format_sats_compact(rollup.sats),
                            rollup.boosters,
                            plural_s(rollup.boosters),
                        )),
                        subtitle_title: episode.feed_title.clone(),
                        image: episode
                            .image
                            .clone()
                            .or_else(|| episode.feed_image.clone())
                            .or_else(|| {
                                feed.as_ref().and_then(|f| f.get_image().map(String::from))
                            }),
                        rank: Some(i + 1),
                        badge: feed
                            .as_ref()
                            .map(|f| f.has_v4v())
                            .unwrap_or(false)
                            .then_some(RailBadge::V4v),
                        person: false,
                        route: Some(Route::PodcastRssEpisodeDetail {
                            podcast_id,
                            episode_id: episode.id.to_string(),
                        }),
                        glyph: "⚡",
                    })
                })
                .collect();
            Ok(tiles)
        }
    });
    if !has_signer {
        return rsx! { RailAuthRequired { item_label: "boost charts" } };
    }
    let state = tiles.read();
    let tiles = match state.as_ref() {
        Some(Ok(t)) => Some(t.clone()),
        Some(Err(_)) => Some(Vec::new()),
        None => None,
    };
    rsx! {
        DiscoveryRail {
            title: "Top Boosted Episodes This Week".to_string(),
            footer: RailFooter::Note("Ranked by sats sent in boost notes on Nostr".to_string()),
            tiles,
            empty_message: "No episode boosts on Nostr in the last week.".to_string(),
        }
    }
}

/// People ranked by the sats they sent in boost notes this week. Works
/// signed-out: the corpus is Nostr and the faces come from the profile cache.
#[component]
pub fn TopBoostersRail() -> Element {
    let tiles = use_resource(move || {
        let ready = *nostr_client::CLIENT_INITIALIZED.read();
        async move {
            if !ready {
                return Ok::<_, String>(Vec::<RailTileData>::new());
            }
            let records = boost_scan::cached_boosts().await?;
            let since = now_secs().saturating_sub(BOOST_WINDOW_DAYS * DAY);
            let rollups = boost_scan::rollup_boosters(&records, since);
            let tiles = rollups
                .into_iter()
                .take(RAIL_SIZE)
                .enumerate()
                .map(|(i, rollup)| {
                    let profile = profiles::get_profile(&rollup.pubkey);
                    if profile.is_none() {
                        profiles::queue_profile_request(rollup.pubkey.clone());
                    }
                    let name = profile
                        .as_ref()
                        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
                        .unwrap_or_else(|| truncate_pubkey(&rollup.pubkey));
                    RailTileData {
                        key: format!("booster-{}", rollup.pubkey),
                        title: name,
                        subtitle: Some(format!(
                            "{} sats boosted",
                            format_sats_compact(rollup.sats),
                        )),
                        subtitle_title: Some(format!(
                            "{} boost{} sent this week",
                            rollup.boosts,
                            plural_s(rollup.boosts),
                        )),
                        image: profile.as_ref().and_then(|p| p.picture.clone()),
                        rank: Some(i + 1),
                        badge: None,
                        person: true,
                        route: Some(Route::Profile {
                            pubkey: rollup.pubkey,
                        }),
                        glyph: "👤",
                    }
                })
                .collect();
            Ok(tiles)
        }
    });
    let state = tiles.read();
    let tiles = match state.as_ref() {
        Some(Ok(t)) => Some(t.clone()),
        Some(Err(_)) => Some(Vec::new()),
        None => None,
    };
    rsx! {
        DiscoveryRail {
            title: "Top Boosters This Week".to_string(),
            footer: RailFooter::Note("People sending sats to podcasts over Nostr".to_string()),
            tiles,
            empty_message: "No podcast boosts on Nostr in the last week.".to_string(),
        }
    }
}

/// The personal rail: shows the signed-in user's follows boosted this week.
/// Signed-out visitors see the locked sign-in card. The resource is created
/// unconditionally so the hook order is stable across auth changes; logged
/// out it resolves to an empty list harmlessly.
#[component]
pub fn FollowsBoostsRail() -> Element {
    let is_authenticated = auth_store::is_authenticated();
    let tiles = use_resource(move || {
        let ready =
            *nostr_client::CLIENT_INITIALIZED.read() && *nostr_client::HAS_SIGNER.read();
        async move {
            if !ready {
                return Ok::<_, String>(Vec::<RailTileData>::new());
            }
            let Some(pubkey) = auth_store::get_pubkey() else {
                return Ok(Vec::new());
            };
            let contacts = nostr_client::fetch_contacts(pubkey).await?;
            // Unranked: this is the user's own crowd, not a public chart.
            let ranked = boosted_show_tiles(Some(&contacts)).await?;
            Ok(ranked
                .into_iter()
                .map(|mut tile| {
                    tile.rank = None;
                    tile
                })
                .collect())
        }
    });
    if !is_authenticated {
        return rsx! {
            LockedRailCard {
                title: "See what your follows boost",
                description: "The shows the accounts you follow have sent sats to this week.",
            }
        };
    }
    let state = tiles.read();
    let tiles = match state.as_ref() {
        Some(Ok(t)) => Some(t.clone()),
        Some(Err(_)) => Some(Vec::new()),
        None => None,
    };
    rsx! {
        DiscoveryRail {
            title: "From Your Follows".to_string(),
            footer: RailFooter::Note("Shows your follows boosted this week".to_string()),
            tiles,
            empty_message: "No boosts from your follows this week.".to_string(),
        }
    }
}

/// Recently added Podcast Index feeds.
#[component]
pub fn NewFeedsRail() -> Element {
    let has_signer = *nostr_client::HAS_SIGNER.read();
    let feeds = use_resource(move || {
        let ready =
            *nostr_client::CLIENT_INITIALIZED.read() && *nostr_client::HAS_SIGNER.read();
        async move {
            if !ready {
                return Ok(Vec::new());
            }
            let since = now_secs().saturating_sub(90 * DAY);
            podcast_index::get_new_feeds(Some(30), Some(since)).await
        }
    });
    if !has_signer {
        return rsx! { RailAuthRequired { item_label: "new podcast feeds" } };
    }
    let state = feeds.read();
    let tiles = match state.as_ref() {
        Some(Ok(feeds)) => Some(
            feeds
                .iter()
                .take(RAIL_SIZE)
                .map(RailTileData::rss_show)
                .collect::<Vec<_>>(),
        ),
        Some(Err(_)) => Some(Vec::new()),
        None => None,
    };
    rsx! {
        DiscoveryRail {
            title: "New and Noteworthy".to_string(),
            footer: RailFooter::Note("Feeds picked up by Podcast Index in the last 90 days".to_string()),
            tiles,
            empty_message: "No new feeds in the last 90 days.".to_string(),
        }
    }
}

/// Feeds that recently gained a value block (V4V-ready).
#[component]
pub fn NewValueFeedsRail() -> Element {
    let has_signer = *nostr_client::HAS_SIGNER.read();
    let feeds = use_resource(move || {
        let ready =
            *nostr_client::CLIENT_INITIALIZED.read() && *nostr_client::HAS_SIGNER.read();
        async move {
            if !ready {
                return Ok(Vec::new());
            }
            let since = now_secs().saturating_sub(30 * DAY);
            podcast_index::get_new_value_feeds(Some(30), Some(since)).await
        }
    });
    if !has_signer {
        return rsx! { RailAuthRequired { item_label: "new V4V feeds" } };
    }
    let state = feeds.read();
    let tiles = match state.as_ref() {
        Some(Ok(feeds)) => Some(
            feeds
                .iter()
                .take(RAIL_SIZE)
                .map(|feed| RailTileData {
                    badge: Some(RailBadge::V4v),
                    ..RailTileData::rss_show(feed)
                })
                .collect::<Vec<_>>(),
        ),
        Some(Err(_)) => Some(Vec::new()),
        None => None,
    };
    rsx! {
        DiscoveryRail {
            title: "New V4V Feeds".to_string(),
            footer: RailFooter::Note("Feeds that started accepting sats in the last 30 days".to_string()),
            tiles,
            empty_message: "No feeds gained a value block recently.".to_string(),
        }
    }
}

/// Native nostr podcast shows. Works signed-out.
#[component]
pub fn NostrShowsRail() -> Element {
    let shows = use_resource(move || {
        let ready = *nostr_client::CLIENT_INITIALIZED.read();
        async move {
            if !ready {
                return Ok(Vec::<PodcastShow>::new());
            }
            fetch_nostr_podcast_shows().await
        }
    });
    let state = shows.read();
    let tiles = match state.as_ref() {
        Some(Ok(shows)) => Some(
            shows
                .iter()
                .take(RAIL_SIZE)
                .map(|show| RailTileData {
                    key: format!("nostr-{}", show.id),
                    title: show.title.clone(),
                    subtitle: show.author.clone(),
                    subtitle_title: None,
                    image: show.image.clone(),
                    rank: None,
                    badge: Some(RailBadge::Nostr),
                    person: false,
                    route: Some(get_show_route(show)),
                    glyph: "🎙",
                })
                .collect::<Vec<_>>(),
        ),
        Some(Err(_)) => Some(Vec::new()),
        None => None,
    };
    rsx! {
        DiscoveryRail {
            title: "Fresh on Nostr".to_string(),
            footer: RailFooter::Note("Podcasts published natively on Nostr".to_string()),
            tiles,
            empty_message: "No native nostr podcasts found.".to_string(),
        }
    }
}

/// Display names for common language codes; the code itself otherwise.
fn language_name(code: &str) -> String {
    match code.to_lowercase().as_str() {
        "de" => "German".to_string(),
        "es" => "Spanish".to_string(),
        "fr" => "French".to_string(),
        "it" => "Italian".to_string(),
        "pt" => "Portuguese".to_string(),
        "nl" => "Dutch".to_string(),
        "sv" => "Swedish".to_string(),
        "pl" => "Polish".to_string(),
        "ru" => "Russian".to_string(),
        "ja" => "Japanese".to_string(),
        "ko" => "Korean".to_string(),
        "zh" => "Chinese".to_string(),
        "pt-br" => "Brazilian Portuguese".to_string(),
        other => other.to_uppercase(),
    }
}

/// Derives the two largest non-English languages on the month trending chart
/// and renders one per-language trending rail each. Renders nothing while
/// deriving or when no language qualifies.
#[component]
pub fn LanguageRails() -> Element {
    let has_signer = *nostr_client::HAS_SIGNER.read();
    let langs = use_resource(move || {
        let ready =
            *nostr_client::CLIENT_INITIALIZED.read() && *nostr_client::HAS_SIGNER.read();
        async move {
            if !ready {
                return Ok::<_, String>(Vec::<String>::new());
            }
            let since = now_secs().saturating_sub(30 * DAY);
            let feeds = podcast_index::get_trending_window(Some(50), Some(since), None, None)
                .await
                .unwrap_or_default();
            // Count feeds per primary language, skipping English and unset.
            let mut counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for feed in &feeds {
                if let Some(lang) = feed.language.as_deref() {
                    let primary = lang
                        .split(',')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_lowercase();
                    if !primary.is_empty() && primary != "en" {
                        *counts.entry(primary).or_insert(0) += 1;
                    }
                }
            }
            let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            Ok(ranked
                .into_iter()
                .take(2)
                .filter(|(_, n)| *n >= 3)
                .map(|(lang, _)| lang)
                .collect::<Vec<_>>())
        }
    });
    if !has_signer {
        return rsx! {};
    }
    let state = langs.read();
    match state.as_ref() {
        Some(Ok(list)) if !list.is_empty() => rsx! {
            for lang in list.clone() {
                LanguageRail { key: "{lang}", lang }
            }
        },
        _ => rsx! {},
    }
}

#[derive(Props, Clone, PartialEq)]
struct LanguageRailProps {
    lang: String,
}

/// One per-language trending rail.
#[component]
fn LanguageRail(props: LanguageRailProps) -> Element {
    let lang = props.lang.clone();
    let name = language_name(&lang);
    let feeds = use_resource(move || {
        let lang = lang.clone();
        let ready =
            *nostr_client::CLIENT_INITIALIZED.read() && *nostr_client::HAS_SIGNER.read();
        async move {
            if !ready {
                return Ok(Vec::new());
            }
            let since = now_secs().saturating_sub(30 * DAY);
            podcast_index::get_trending_window(Some(30), Some(since), Some(&lang), None).await
        }
    });
    let state = feeds.read();
    let tiles = match state.as_ref() {
        Some(Ok(feeds)) => Some(trending_tiles(feeds)),
        Some(Err(_)) => Some(Vec::new()),
        None => None,
    };
    rsx! {
        DiscoveryRail {
            title: format!("Top {} Podcasts", name),
            footer: RailFooter::Note("Trending this month".to_string()),
            tiles,
            empty_message: format!("No {} podcasts trending this month.", name),
        }
    }
}
