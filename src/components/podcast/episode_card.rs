//! Podcast Episode Card Component
//!
//! Displays a single podcast episode with play controls,
//! supporting both Nostr and RSS podcast episodes.
use dioxus::prelude::*;
#[cfg(feature = "web")]
use dioxus::web::WebEventExt;
/// Default episode artwork fallback URL (local asset)
const DEFAULT_EPISODE_ARTWORK: &str = "/icons/icon-512.svg";

use crate::components::icons;
use crate::routes::Route;
use crate::services::podcast_index::{Episode as PodcastIndexEpisode, PodcastFeed};
use crate::services::podcast_rss::{format_duration, RssEpisode, RssPodcast};
use crate::stores::music_player::{self, MusicPlayerStateStoreExt, MusicTrack};
use crate::stores::nostr_music::{PodcastAddr, TrackSource};
use crate::utils::podcast::{Person, PodcastEpisode, Soundbite, TranscriptRef, ValueBlock};
/// Unified podcast episode for display
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayEpisode {
    /// Unique identifier
    pub id: String,
    /// Episode title
    pub title: String,
    /// Episode description/show notes
    pub description: Option<String>,
    /// Episode image URL
    pub image: Option<String>,
    /// Audio URL
    pub audio_url: String,
    /// Duration in seconds
    pub duration: Option<u64>,
    /// Publication date string
    pub pub_date: Option<String>,
    /// Created/published timestamp (seconds since epoch) for sorting
    pub created_at: u64,
    /// Season number
    pub season: Option<u32>,
    /// Episode number
    pub episode_number: Option<u32>,
    /// Chapters URL (JSON format)
    pub chapters_url: Option<String>,
    /// Has transcript available
    pub has_transcript: bool,
    /// Podcast title (for display)
    pub podcast_title: String,
    /// Podcast image (fallback)
    pub podcast_image: Option<String>,
    /// Track source for player
    pub source: TrackSource,
    /// V4V configuration
    pub value: Option<ValueBlock>,
    /// Transcripts for the episode
    pub transcripts: Vec<TranscriptRef>,
    /// Soundbites for previews
    pub soundbites: Vec<Soundbite>,
    /// Episode-level persons/guests
    pub persons: Vec<Person>,
    /// Whether this is a live streaming episode
    pub is_live: bool,
}
impl DisplayEpisode {
    fn rss_episode_track_id(feed_key: &str, episode_guid: &str) -> String {
        format!("rss-podcast:{feed_key}:{episode_guid}")
    }

    /// Create from Nostr podcast episode
    pub fn from_nostr_episode(
        episode: &PodcastEpisode,
        podcast_title: &str,
        podcast_image: Option<&str>,
    ) -> Self {
        Self {
            id: episode.event_id.clone(),
            title: episode.title.clone(),
            description: episode.description.clone(),
            image: episode.image.clone(),
            audio_url: episode.audio_url.clone(),
            duration: episode.duration,
            pub_date: episode.pubdate.clone(),
            created_at: episode.created_at,
            season: episode.season,
            episode_number: episode.episode_number,
            chapters_url: episode.chapters_url.clone(),
            has_transcript: !episode.transcripts.is_empty(),
            podcast_title: podcast_title.to_string(),
            podcast_image: podcast_image.map(|s| s.to_string()),
            source: TrackSource::NostrPodcast {
                pubkey: episode.pubkey.clone(),
                podcast_title: podcast_title.to_string(),
                addr: if episode.source_kind == crate::utils::podcast::KIND_F4_EPISODE {
                    PodcastAddr::F4 {
                        event_id: episode.event_id.clone(),
                    }
                } else {
                    PodcastAddr::Legacy {
                        coordinate: episode.coordinate.clone(),
                        d_tag: episode.d_tag.clone(),
                    }
                },
            },
            value: episode.value.clone(),
            transcripts: episode.transcripts.clone(),
            soundbites: episode.soundbites.clone(),
            persons: Vec::new(),
            is_live: false,
        }
    }
    /// Create from RSS episode
    pub fn from_rss_episode(episode: &RssEpisode, podcast: &RssPodcast) -> Self {
        let created_at = episode
            .pub_date
            .as_ref()
            .and_then(|d| parse_rfc2822_to_timestamp(d))
            .unwrap_or(0);
        Self {
            id: Self::rss_episode_track_id(&podcast.feed_url, &episode.guid),
            title: episode.title.clone(),
            description: episode.description.clone(),
            image: episode.image.clone(),
            audio_url: episode.enclosure_url.clone(),
            duration: episode.duration,
            pub_date: episode.pub_date.clone(),
            created_at,
            season: episode.season,
            episode_number: episode.episode_number,
            chapters_url: episode.chapters_url.clone(),
            has_transcript: !episode.transcripts.is_empty(),
            podcast_title: podcast.title.clone(),
            podcast_image: podcast.image.clone(),
            source: TrackSource::RssPodcast {
                feed_url: podcast.feed_url.clone(),
                podcast_id: None,
                episode_guid: episode.guid.clone(),
                podcast_title: podcast.title.clone(),
            },
            value: episode.value.clone().or(podcast.value.clone()),
            transcripts: episode.transcripts.clone(),
            soundbites: episode.soundbites.clone(),
            persons: episode.persons.clone(),
            is_live: false,
        }
    }
    /// Create from Podcast Index API episode
    pub fn from_podcast_index_episode(episode: &PodcastIndexEpisode, feed: &PodcastFeed) -> Self {
        let created_at = episode
            .date_published
            .and_then(|ts| u64::try_from(ts).ok())
            .unwrap_or(0);
        let pub_date = episode.date_published.map(|ts| {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.format("%b %d, %Y").to_string())
                .unwrap_or_else(|| "Unknown date".to_string())
        });
        let transcripts: Vec<TranscriptRef> = episode
            .transcripts
            .iter()
            .filter_map(|t| {
                let url = t.url.clone()?;
                Some(TranscriptRef {
                    url,
                    transcript_type: t
                        .transcript_type
                        .clone()
                        .unwrap_or_else(|| "text/plain".to_string()),
                    language: None,
                    rel: None,
                })
            })
            .collect();
        let soundbites: Vec<Soundbite> = episode
            .soundbites
            .iter()
            .filter_map(|s| {
                Some(Soundbite {
                    start_time: s.start_time?,
                    duration: s.duration?,
                    title: s.title.clone(),
                })
            })
            .collect();
        Self {
            id: Self::rss_episode_track_id(&feed.id.to_string(), &episode.id.to_string()),
            title: episode.title.clone(),
            description: episode.description.clone(),
            image: episode.get_image().map(|s| s.to_string()),
            audio_url: episode.enclosure_url.clone().unwrap_or_default(),
            duration: episode.duration,
            pub_date,
            created_at,
            season: episode.season,
            episode_number: episode.episode,
            chapters_url: episode.chapters_url.clone(),
            has_transcript: !episode.transcripts.is_empty(),
            podcast_title: feed.title.clone(),
            podcast_image: feed.get_image().map(|s| s.to_string()),
            source: TrackSource::RssPodcast {
                feed_url: feed.url.clone(),
                podcast_id: Some(feed.id),
                episode_guid: episode.id.to_string(),
                podcast_title: feed.title.clone(),
            },
            value: episode.value.as_ref().or(feed.value.as_ref()).map(|v| {
                let model = v.model.as_ref();
                crate::utils::podcast::ValueBlock {
                    value_type: model
                        .and_then(|m| m.model_type.clone())
                        .unwrap_or_else(|| "lightning".to_string()),
                    method: model
                        .and_then(|m| m.method.clone())
                        .unwrap_or_else(|| "keysend".to_string()),
                    suggested: model
                        .and_then(|m| m.suggested.as_ref())
                        .and_then(|s| s.parse().ok()),
                    recipients: v
                        .destinations
                        .iter()
                        .map(|d| crate::utils::podcast::ValueRecipient {
                            name: d.name.clone(),
                            recipient_type: d
                                .dest_type
                                .clone()
                                .unwrap_or_else(|| "node".to_string()),
                            address: d.address.clone().unwrap_or_default(),
                            custom_key: None,
                            custom_value: None,
                            split: d.split.unwrap_or(100),
                            fee: None,
                        })
                        .collect(),
                }
            }),
            transcripts,
            soundbites,
            persons: Vec::new(),
            is_live: false,
        }
    }
    /// Create from Podcast Index API live episode
    pub fn from_podcast_index_live_episode(
        episode: &PodcastIndexEpisode,
        feed: &PodcastFeed,
    ) -> Self {
        let mut ep = Self::from_podcast_index_episode(episode, feed);
        ep.is_live = true;
        ep.created_at = crate::platform::timestamp::now_secs();
        ep
    }
    /// Convert to MusicTrack for player
    pub fn to_music_track(&self) -> MusicTrack {
        MusicTrack {
            id: self.id.clone(),
            title: self.title.clone(),
            artist: self.podcast_title.clone(),
            artist_npub: None,
            artist_id: None,
            artist_art_url: None,
            album: None,
            album_id: None,
            duration: self.duration.map(|d| d.min(u32::MAX as u64) as u32),
            media_url: self.audio_url.clone(),
            album_art_url: self.image.clone().or(self.podcast_image.clone()),
            msat_total: None,
            created_at: None,
            source: self.source.clone(),
            is_podcast: true,
            is_live_stream: self.is_live,
            value_block: self.value.clone(),
            chapters_url: self.chapters_url.clone(),
            transcripts: Vec::new(),
        }
    }
    /// Get route to the podcast show page
    pub fn get_show_route(&self) -> Option<Route> {
        match &self.source {
            TrackSource::NostrPodcast { pubkey, addr, .. } => {
                if let Some(naddr) = crate::stores::nostr_music::show_share_bech32(pubkey, addr) {
                    return Some(Route::PodcastNostrDetail { naddr });
                }
                None
            }
            TrackSource::RssPodcast { podcast_id, .. } => {
                podcast_id.map(|id| Route::PodcastRssFeedDetail {
                    podcast_id: id.to_string(),
                })
            }
            _ => None,
        }
    }
    /// Get route to the episode detail page
    pub fn get_episode_route(&self) -> Option<Route> {
        match &self.source {
            TrackSource::NostrPodcast { pubkey, addr, .. } => {
                if let Some(naddr) = crate::stores::nostr_music::episode_share_bech32(pubkey, addr) {
                    return Some(Route::PodcastNostrEpisodeDetail { naddr });
                }
                None
            }
            TrackSource::RssPodcast {
                podcast_id,
                episode_guid,
                ..
            } => podcast_id.map(|id| Route::PodcastRssEpisodeDetail {
                podcast_id: id.to_string(),
                episode_id: urlencoding::encode(episode_guid).to_string(),
            }),
            _ => None,
        }
    }
}
#[derive(Props, Clone, PartialEq)]
pub struct PodcastEpisodeCardProps {
    /// The episode to display
    pub episode: DisplayEpisode,
    /// Show podcast title
    #[props(default = true)]
    pub show_podcast_title: bool,
    /// Show description preview
    #[props(default = true)]
    pub show_description: bool,
    /// Optional playlist for continuous playback
    /// Uses Rc to avoid O(n²) cloning when used in episode lists
    #[props(default)]
    pub playlist: Option<std::rc::Rc<Vec<MusicTrack>>>,
}
/// Podcast episode card component
#[component]
pub fn PodcastEpisodeCard(props: PodcastEpisodeCardProps) -> Element {
    let episode = &props.episode;
    let episode_id_for_memo = episode.id.clone();
    let is_playing = use_memo(move || {
        let store = music_player::MUSIC_PLAYER.resolve();
        let current = store.current_track().cloned();
        if let Some(ref cur) = current {
            cur.id == episode_id_for_memo && store.is_playing().cloned()
        } else {
            false
        }
    });
    let playlist = props.playlist.clone();
    let handle_play = {
        let episode = episode.clone();
        let playlist = playlist.clone();
        move |_e: Event<MouseData>| {
            // Guard against clicks on interactive elements inside the card
            #[cfg(feature = "web")]
            {
                use wasm_bindgen::JsCast;
                use web_sys::Element;
                // Use the event in web builds to check target
                let e = _e;
                if let Some(target) = e
                    .data()
                    .try_as_web_event()
                    .and_then(|evt: web_sys::MouseEvent| evt.target())
                    .and_then(|t| t.dyn_into::<Element>().ok())
                {
                    if let Some(_closest) = target.closest(
                        "a,button,input,textarea,select,summary,[role='button'],[role='link'],[contenteditable='true']"
                    ).ok().flatten() {
                        // Click was on an interactive element - don't trigger play
                        return;
                    }
                }
            }
            let track = episode.to_music_track();
            let playlist_vec = playlist.as_ref().map(|rc| (**rc).clone());
            music_player::play_or_toggle_track(track, playlist_vec, None);
        }
    };
    let duration_str = episode
        .duration
        .map(format_duration)
        .unwrap_or_else(|| "--:--".to_string());
    let image_url = episode
        .image
        .clone()
        .or(episode.podcast_image.clone())
        .unwrap_or_else(|| DEFAULT_EPISODE_ARTWORK.to_string());
    let episode_label = match (episode.season, episode.episode_number) {
        (Some(s), Some(e)) => Some(format!("S{}E{}", s, e)),
        (None, Some(e)) => Some(format!("Ep. {}", e)),
        _ => None,
    };
    let source_badge = match &episode.source {
        TrackSource::NostrPodcast { .. } => ("N", "bg-purple-500/20 text-purple-400"),
        TrackSource::RssPodcast { .. } => ("R", "bg-green-500/20 text-green-400"),
        _ => ("?", "bg-muted text-muted-foreground"),
    };
    let has_v4v = episode.value.is_some();
    rsx! {
        div {
            class: "flex items-start gap-3 p-3 hover:bg-muted/50 rounded-lg transition group cursor-pointer",
            onclick: handle_play,
            div { class: "relative shrink-0",
                img {
                    src: "{image_url}",
                    alt: "{episode.title}",
                    class: "w-16 h-16 rounded-lg object-cover",
                    loading: "lazy",
                    referrerpolicy: "no-referrer",
                }
                div { class: "absolute -top-1 -right-1 w-4 h-4 rounded-full flex items-center justify-center text-[9px] font-bold {source_badge.1}",
                    "{source_badge.0}"
                }
                if episode.is_live {
                    div { class: "absolute -top-1 -left-1 px-1.5 py-0.5 rounded flex items-center gap-1 bg-red-600 text-white text-[9px] font-bold uppercase",
                        span { class: "w-1.5 h-1.5 rounded-full bg-white animate-pulse" }
                        "Live"
                    }
                }
                button {
                    class: "absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 group-hover:opacity-100 transition rounded-lg",
                    onclick: {
                        let episode = episode.clone();
                        let playlist = playlist.clone();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            let track = episode.to_music_track();
                            let playlist_vec = playlist.as_ref().map(|rc| (**rc).clone());
                            music_player::play_or_toggle_track(track, playlist_vec, None);
                        }
                    },
                    dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY },
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center gap-2",
                    if let Some(ref label) = episode_label {
                        span { class: "text-xs text-muted-foreground font-medium", "{label}" }
                    }
                    if let Some(episode_route) = episode.get_episode_route() {
                        Link {
                            to: episode_route,
                            class: if *is_playing.read() { "font-medium text-sm truncate text-primary hover:underline" } else { "font-medium text-sm truncate hover:text-primary hover:underline" },
                            onclick: move |e: MouseEvent| e.stop_propagation(),
                            "{episode.title}"
                        }
                    } else {
                        span { class: if *is_playing.read() { "font-medium text-sm truncate text-primary" } else { "font-medium text-sm truncate" },
                            "{episode.title}"
                        }
                    }
                }
                if props.show_podcast_title {
                    if let Some(show_route) = episode.get_show_route() {
                        Link {
                            to: show_route,
                            class: "text-xs text-muted-foreground truncate hover:text-primary hover:underline transition block",
                            onclick: move |e: MouseEvent| e.stop_propagation(),
                            "{episode.podcast_title}"
                        }
                    } else {
                        div { class: "text-xs text-muted-foreground truncate",
                            "{episode.podcast_title}"
                        }
                    }
                }
                if props.show_description {
                    if let Some(ref desc) = episode.description {
                        div { class: "text-xs text-muted-foreground line-clamp-2 mt-1",
                            {strip_html(desc)}
                        }
                    }
                }
                div { class: "flex items-center gap-3 mt-1",
                    span { class: "text-xs text-muted-foreground", "{duration_str}" }
                    if let Some(ref date) = episode.pub_date {
                        span { class: "text-xs text-muted-foreground", "{format_date(date)}" }
                    }
                    div { class: "flex items-center gap-1",
                        if episode.chapters_url.is_some() {
                            span {
                                class: "text-[10px] px-1.5 py-0.5 bg-muted rounded text-muted-foreground",
                                title: "Has chapters",
                                "Ch"
                            }
                        }
                        if episode.has_transcript {
                            span {
                                class: "text-[10px] px-1.5 py-0.5 bg-muted rounded text-muted-foreground",
                                title: "Has transcript",
                                "Tx"
                            }
                        }
                        if has_v4v {
                            span {
                                class: "text-amber-500",
                                title: "Supports Value4Value",
                                dangerous_inner_html: icons::ZAP,
                            }
                        }
                    }
                }
            }
            div { class: "flex items-center gap-1 shrink-0 opacity-0 group-hover:opacity-100 transition",
                button {
                    class: "p-2 hover:bg-muted rounded-full transition",
                    title: if *is_playing.read() { "Pause" } else { "Play" },
                    onclick: {
                        let episode = episode.clone();
                        let playlist = playlist.clone();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            let track = episode.to_music_track();
                            let playlist_vec = playlist.as_ref().map(|rc| (**rc).clone());
                            music_player::play_or_toggle_track(track, playlist_vec, None);
                        }
                    },
                    dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY },
                }
                if has_v4v {
                    button {
                        class: "p-2 hover:bg-muted rounded-full transition",
                        title: "Send a boost",
                        onclick: {
                            let episode = episode.clone();
        #[cfg_attr(not(feature = "web"), allow(unused_variables))]
        move |e: Event<MouseData>| {
                                e.stop_propagation();
                                let track = episode.to_music_track();
                                music_player::show_zap_dialog_for_track(Some(track));
                            }
                        },
                        dangerous_inner_html: icons::ZAP,
                    }
                }
            }
        }
    }
}
/// Skeleton loader for podcast episode card
#[component]
pub fn PodcastEpisodeCardSkeleton() -> Element {
    rsx! {
        div { class: "flex items-start gap-3 p-3 rounded-lg animate-pulse",
            div { class: "w-16 h-16 bg-muted rounded-lg shrink-0" }
            div { class: "flex-1 min-w-0 space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
                div { class: "h-3 bg-muted rounded w-full" }
                div { class: "flex gap-2",
                    div { class: "h-3 bg-muted rounded w-12" }
                    div { class: "h-3 bg-muted rounded w-16" }
                }
            }
        }
    }
}
/// Strip HTML tags from text using ammonia for secure sanitization
fn strip_html(html: &str) -> String {
    use ammonia::Builder;
    use std::collections::HashSet;
    Builder::new()
        .tags(HashSet::new())
        .clean(html)
        .to_string()
        .trim()
        .to_string()
}
/// Format date string for display
fn format_date(date_str: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(date_str) {
        return dt.format("%b %d, %Y").to_string();
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
        return dt.format("%b %d, %Y").to_string();
    }
    date_str.to_string()
}
/// Parse date string to Unix timestamp for sorting
fn parse_rfc2822_to_timestamp(date_str: &str) -> Option<u64> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(date_str) {
        let ts = dt.timestamp();
        if ts >= 0 {
            return Some(ts as u64);
        }
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
        let ts = dt.timestamp();
        return if ts >= 0 { Some(ts as u64) } else { None };
    }
    None
}
