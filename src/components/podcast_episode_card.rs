//! Podcast Episode Card Component
//!
//! Displays a single podcast episode with play controls,
//! supporting both Nostr and RSS podcast episodes.

use dioxus::prelude::*;
use crate::stores::music_player::{self, MusicTrack};
use crate::stores::nostr_music::TrackSource;
use crate::utils::podcast::{PodcastEpisode, ValueBlock, TranscriptRef, Soundbite, Person};
use crate::services::podcast_rss::{RssEpisode, RssPodcast, format_duration};
use crate::components::icons;
use crate::routes::Route;

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
}

impl DisplayEpisode {
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
                coordinate: episode.coordinate.clone(),
                pubkey: episode.pubkey.clone(),
                d_tag: episode.d_tag.clone(),
                podcast_title: podcast_title.to_string(),
            },
            value: episode.value.clone(),
            transcripts: episode.transcripts.clone(),
            soundbites: episode.soundbites.clone(),
            persons: Vec::new(), // Nostr podcasts don't have persons at episode level yet
        }
    }

    /// Create from RSS episode
    pub fn from_rss_episode(episode: &RssEpisode, podcast: &RssPodcast) -> Self {
        // Parse RFC 2822 date to timestamp for sorting
        let created_at = episode.pub_date.as_ref()
            .and_then(|d| parse_rfc2822_to_timestamp(d))
            .unwrap_or(0);

        Self {
            id: episode.guid.clone(),
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
                episode_guid: episode.guid.clone(),
                podcast_title: podcast.title.clone(),
            },
            // Episode-level V4V overrides podcast-level
            value: episode.value.clone().or(podcast.value.clone()),
            transcripts: episode.transcripts.clone(),
            soundbites: episode.soundbites.clone(),
            persons: episode.persons.clone(),
        }
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
            duration: self.duration.map(|d| d as u32),
            media_url: self.audio_url.clone(),
            album_art_url: self.image.clone().or(self.podcast_image.clone()),
            msat_total: None,
            created_at: None,
            source: self.source.clone(),
            is_podcast: true,
            value_block: self.value.clone(),
            chapters_url: self.chapters_url.clone(),
            transcripts: Vec::new(), // Would need to pass these
        }
    }

    /// Get route to the podcast show page
    pub fn get_show_route(&self) -> Option<Route> {
        match &self.source {
            TrackSource::NostrPodcast { pubkey, .. } => {
                // For Nostr podcasts, we need to construct an naddr for the podcast metadata
                // The podcast metadata uses d="podcast-metadata" by convention
                use nostr::prelude::*;
                if let Ok(pk) = PublicKey::from_hex(pubkey) {
                    let coord = Coordinate::new(Kind::from(30078), pk)
                        .identifier("podcast-metadata");
                    let nip19_coord = Nip19Coordinate::new(coord, vec![]);
                    if let Ok(naddr) = nip19_coord.to_bech32() {
                        return Some(Route::PodcastNostrDetail { naddr });
                    }
                }
                None
            }
            TrackSource::RssPodcast { feed_url, .. } => {
                Some(Route::PodcastRssFeedDetail {
                    feed_url: urlencoding::encode(feed_url).to_string()
                })
            }
            _ => None,
        }
    }

    /// Get route to the episode detail page
    pub fn get_episode_route(&self) -> Option<Route> {
        match &self.source {
            TrackSource::NostrPodcast { pubkey, d_tag, .. } => {
                // Construct naddr for the episode (Kind 30054)
                use nostr::prelude::*;
                if let Ok(pk) = PublicKey::from_hex(pubkey) {
                    let coord = Coordinate::new(Kind::from(30054), pk)
                        .identifier(d_tag);
                    let nip19_coord = Nip19Coordinate::new(coord, vec![]);
                    if let Ok(naddr) = nip19_coord.to_bech32() {
                        return Some(Route::PodcastNostrEpisodeDetail { naddr });
                    }
                }
                None
            }
            TrackSource::RssPodcast { feed_url, episode_guid, .. } => {
                Some(Route::PodcastRssEpisodeDetail {
                    podcast_id: urlencoding::encode(feed_url).to_string(),
                    episode_id: urlencoding::encode(episode_guid).to_string(),
                })
            }
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
    let episode_id = episode.id.clone();
    let episode_id_for_memo = episode_id.clone();

    // Check if this episode is currently playing
    let is_playing = use_memo(move || {
        let player_state = music_player::MUSIC_PLAYER.read();
        if let Some(ref current) = player_state.current_track {
            current.id == episode_id_for_memo && player_state.is_playing
        } else {
            false
        }
    });

    // Handle play button click
    // Clone the Rc (cheap pointer copy, not Vec clone)
    let playlist = props.playlist.clone();
    let handle_play = {
        let episode = episode.clone();
        let episode_id = episode.id.clone();
        let playlist = playlist.clone();
        move |_| {
            let player_state = music_player::MUSIC_PLAYER.read();

            // If this episode is playing, toggle pause
            if let Some(ref current) = player_state.current_track {
                if current.id == episode_id && player_state.is_playing {
                    drop(player_state);
                    music_player::toggle_play();
                    return;
                }
            }
            drop(player_state);

            // Play this episode
            // Dereference Rc to Vec only when user actually clicks play (single clone)
            let track = episode.to_music_track();
            // (**rc) dereferences &Rc -> Rc (auto-deref) -> &Vec (Deref trait), then clone the Vec
            let playlist_vec = playlist.as_ref().map(|rc| (**rc).clone());
            music_player::play_track(track, playlist_vec, None);
        }
    };

    // Format duration
    let duration_str = episode.duration
        .map(|d| format_duration(d))
        .unwrap_or_else(|| "--:--".to_string());

    // Image URL with fallback
    let image_url = episode.image.clone()
        .or(episode.podcast_image.clone())
        .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/shapes/svg?seed={}", episode.id));

    // Episode number display
    let episode_label = match (episode.season, episode.episode_number) {
        (Some(s), Some(e)) => Some(format!("S{}E{}", s, e)),
        (None, Some(e)) => Some(format!("Ep. {}", e)),
        _ => None,
    };

    // Source badge
    let source_badge = match &episode.source {
        TrackSource::NostrPodcast { .. } => ("N", "bg-purple-500/20 text-purple-400"),
        TrackSource::RssPodcast { .. } => ("R", "bg-green-500/20 text-green-400"),
        _ => ("?", "bg-muted text-muted-foreground"),
    };

    // Has V4V support
    let has_v4v = episode.value.is_some();

    rsx! {
        div {
            class: "flex items-start gap-3 p-3 hover:bg-muted/50 rounded-lg transition group",

            // Episode image with play button
            div {
                class: "relative flex-shrink-0",
                img {
                    src: "{image_url}",
                    alt: "{episode.title}",
                    class: "w-16 h-16 rounded-lg object-cover",
                    loading: "lazy"
                }

                // Source badge
                div {
                    class: "absolute -top-1 -right-1 w-4 h-4 rounded-full flex items-center justify-center text-[9px] font-bold {source_badge.1}",
                    "{source_badge.0}"
                }

                // Play button overlay
                button {
                    class: "absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 group-hover:opacity-100 transition rounded-lg",
                    onclick: handle_play,
                    dangerous_inner_html: if *is_playing.read() {
                        icons::PAUSE
                    } else {
                        icons::PLAY
                    }
                }
            }

            // Episode info
            div {
                class: "flex-1 min-w-0",

                // Title row
                div {
                    class: "flex items-center gap-2",
                    if let Some(ref label) = episode_label {
                        span {
                            class: "text-xs text-muted-foreground font-medium",
                            "{label}"
                        }
                    }
                    if let Some(episode_route) = episode.get_episode_route() {
                        Link {
                            to: episode_route,
                            class: if *is_playing.read() { "font-medium text-sm truncate text-primary hover:underline" } else { "font-medium text-sm truncate hover:text-primary hover:underline" },
                            onclick: move |e: MouseEvent| e.stop_propagation(),
                            "{episode.title}"
                        }
                    } else {
                        span {
                            class: if *is_playing.read() { "font-medium text-sm truncate text-primary" } else { "font-medium text-sm truncate" },
                            "{episode.title}"
                        }
                    }
                }

                // Podcast title (if showing) - clickable link to show page
                if props.show_podcast_title {
                    if let Some(show_route) = episode.get_show_route() {
                        Link {
                            to: show_route,
                            class: "text-xs text-muted-foreground truncate hover:text-primary hover:underline transition block",
                            onclick: move |e: MouseEvent| e.stop_propagation(),
                            "{episode.podcast_title}"
                        }
                    } else {
                        div {
                            class: "text-xs text-muted-foreground truncate",
                            "{episode.podcast_title}"
                        }
                    }
                }

                // Description preview (if showing)
                if props.show_description {
                    if let Some(ref desc) = episode.description {
                        div {
                            class: "text-xs text-muted-foreground line-clamp-2 mt-1",
                            // Strip HTML tags for preview
                            {strip_html(desc)}
                        }
                    }
                }

                // Metadata row
                div {
                    class: "flex items-center gap-3 mt-1",

                    // Duration
                    span {
                        class: "text-xs text-muted-foreground",
                        "{duration_str}"
                    }

                    // Publication date
                    if let Some(ref date) = episode.pub_date {
                        span {
                            class: "text-xs text-muted-foreground",
                            "{format_date(date)}"
                        }
                    }

                    // Feature indicators
                    div {
                        class: "flex items-center gap-1",

                        // Chapters indicator
                        if episode.chapters_url.is_some() {
                            span {
                                class: "text-[10px] px-1.5 py-0.5 bg-muted rounded text-muted-foreground",
                                title: "Has chapters",
                                "Ch"
                            }
                        }

                        // Transcript indicator
                        if episode.has_transcript {
                            span {
                                class: "text-[10px] px-1.5 py-0.5 bg-muted rounded text-muted-foreground",
                                title: "Has transcript",
                                "Tx"
                            }
                        }

                        // V4V indicator
                        if has_v4v {
                            span {
                                class: "text-amber-500",
                                title: "Supports Value4Value",
                                dangerous_inner_html: icons::ZAP
                            }
                        }
                    }
                }
            }

            // Action buttons (visible on hover)
            div {
                class: "flex items-center gap-1 flex-shrink-0 opacity-0 group-hover:opacity-100 transition",

                // Play button (larger, always visible target)
                button {
                    class: "p-2 hover:bg-muted rounded-full transition",
                    title: if *is_playing.read() { "Pause" } else { "Play" },
                    onclick: {
                        let episode = episode.clone();
                        let playlist = playlist.clone();
                        move |_| {
                            let track = episode.to_music_track();
                            // Dereference Rc to Vec only when user clicks (single clone)
                            let playlist_vec = playlist.as_ref().map(|rc| (**rc).clone());
                            music_player::play_track(track, playlist_vec, None);
                        }
                    },
                    dangerous_inner_html: if *is_playing.read() {
                        icons::PAUSE
                    } else {
                        icons::PLAY
                    }
                }

                // Zap button (if V4V supported)
                if has_v4v {
                    button {
                        class: "p-2 hover:bg-muted rounded-full transition",
                        title: "Send a boost",
                        onclick: {
                            let episode = episode.clone();
                            move |e: Event<MouseData>| {
                                e.stop_propagation();
                                let track = episode.to_music_track();
                                music_player::show_zap_dialog_for_track(Some(track));
                            }
                        },
                        dangerous_inner_html: icons::ZAP
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
        div {
            class: "flex items-start gap-3 p-3 rounded-lg animate-pulse",

            // Image skeleton
            div {
                class: "w-16 h-16 bg-muted rounded-lg flex-shrink-0"
            }

            // Info skeleton
            div {
                class: "flex-1 min-w-0 space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
                div { class: "h-3 bg-muted rounded w-full" }
                div {
                    class: "flex gap-2",
                    div { class: "h-3 bg-muted rounded w-12" }
                    div { class: "h-3 bg-muted rounded w-16" }
                }
            }
        }
    }
}

/// Strip HTML tags from text (simple version)
fn strip_html(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    // Decode common HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

/// Format date string for display
fn format_date(date_str: &str) -> String {
    // Try RFC 2822 parsing first
    if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(date_str) {
        return dt.format("%b %d, %Y").to_string();
    }

    // Try RFC 3339 / ISO 8601
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
        return dt.format("%b %d, %Y").to_string();
    }

    // Return as-is if parsing fails
    date_str.to_string()
}

/// Parse date string to Unix timestamp for sorting
fn parse_rfc2822_to_timestamp(date_str: &str) -> Option<u64> {
    // Try RFC 2822 parsing first (e.g., "Tue, 05 Dec 2023 14:30:00 +0000")
    if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(date_str) {
        return Some(dt.timestamp() as u64);
    }

    // Try RFC 3339 / ISO 8601 (e.g., "2023-12-05T14:30:00Z")
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
        return Some(dt.timestamp() as u64);
    }

    None
}
