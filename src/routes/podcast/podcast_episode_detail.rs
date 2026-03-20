//! Podcast Episode Detail Route
//!
//! Displays a single podcast episode with:
//! - Episode artwork, title, show name
//! - Full player controls (uses unified audio player)
//! - Episode description/notes
//! - Chapters list with images and links
//! - Zap/reaction buttons (Nostr episodes)
//! - Share functionality
use crate::components::{
    icons, ContentShareModal, ContentType, DisplayEpisode, FeaturedSoundbite, InlineCredits,
    PodcastChapters, PodcastPersons, PodcastSoundbites, PodcastTranscript, V4VBoostButton, V4VInfo,
};
use crate::routes::Route;
use crate::routes::podcast::podcast_shared_states::{
    PodcastApiAuthRequiredState, PodcastApiInitializingState,
};
use crate::services::podcast_rss::{self, format_duration};
use crate::stores::{music_player, nostr_client, nostr_music};
use crate::utils::podcast::{self, PodcastMetadata};
use dioxus::prelude::*;
use nostr_sdk::prelude::{Filter, Kind, PublicKey, SingleLetterTag};
use std::time::Duration;

enum RssEpisodeDetailState {
    Initializing,
    Loaded(Box<DisplayEpisode>),
    AuthRequired,
    Error(String),
}

#[derive(Props, Clone, PartialEq)]
pub struct PodcastNostrEpisodeDetailProps {
    /// Episode naddr or coordinate
    pub naddr: String,
}
/// Detail page for a Nostr podcast episode
#[component]
pub fn PodcastNostrEpisodeDetail(props: PodcastNostrEpisodeDetailProps) -> Element {
    let naddr = props.naddr.clone();
    let mut episode_data = use_signal(|| None::<Result<(DisplayEpisode, PodcastMetadata), String>>);
    let mut loading = use_signal(|| true);
    let mut load_generation = use_signal(|| 0u32);
    use_effect(move || {
        let naddr = naddr.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let generation = load_generation.with_mut(|current| {
            *current = current.wrapping_add(1);
            *current
        });
        loading.set(true);
        spawn(async move {
            let result = fetch_nostr_episode(&naddr).await;
            if *load_generation.read() != generation {
                return;
            }
            episode_data.set(Some(result));
            loading.set(false);
        });
    });
    rsx! {
        div { class: "min-h-screen",
            EpisodeDetailHeader {}
            if !*nostr_client::CLIENT_INITIALIZED.read() || *loading.read() {
                EpisodeDetailSkeleton {}
            } else {
                match episode_data.read().as_ref() {
                    Some(Ok((episode, metadata))) => rsx! {
                        EpisodeDetailContent { episode: episode.clone(), podcast_metadata: Some(metadata.clone()) }
                    },
                    Some(Err(e)) => rsx! {
                        div { class: "p-4 text-center",
                            div { class: "text-destructive mb-2", "Failed to load episode" }
                            div { class: "text-sm text-muted-foreground", "{e}" }
                        }
                    },
                    None => rsx! {
                        EpisodeDetailSkeleton {}
                    },
                }
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
pub struct PodcastRssEpisodeDetailProps {
    /// Podcast GUID or feed URL
    pub podcast_id: String,
    /// Episode GUID
    pub episode_id: String,
}
/// Detail page for an RSS podcast episode
#[component]
pub fn PodcastRssEpisodeDetail(props: PodcastRssEpisodeDetailProps) -> Element {
    let podcast_id = props.podcast_id.clone();
    let episode_id = props.episode_id.clone();
    let episode_data = use_resource(move || {
        let podcast_id = podcast_id.clone();
        let episode_id = episode_id.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = nostr_client::has_signer();
        async move {
            if !client_initialized {
                return RssEpisodeDetailState::Initializing;
            }
            if podcast_id.parse::<u64>().is_ok() && !has_signer {
                return RssEpisodeDetailState::AuthRequired;
            }
            match fetch_rss_episode(&podcast_id, &episode_id).await {
                Ok(episode) => RssEpisodeDetailState::Loaded(Box::new(episode)),
                Err(error) => RssEpisodeDetailState::Error(error),
            }
        }
    });
    rsx! {
        div { class: "min-h-screen",
            EpisodeDetailHeader {}
            match &*episode_data.read() {
                Some(RssEpisodeDetailState::Initializing) => rsx! {
                    PodcastApiInitializingState {
                        item_label: "episode",
                    }
                },
                Some(RssEpisodeDetailState::Loaded(episode)) => rsx! {
                    EpisodeDetailContent { episode: (**episode).clone(), podcast_metadata: None }
                },
                Some(RssEpisodeDetailState::AuthRequired) => rsx! {
                    PodcastApiAuthRequiredState {
                        item_label: "episode",
                    }
                },
                Some(RssEpisodeDetailState::Error(e)) => rsx! {
                    div { class: "p-4 text-center",
                        div { class: "text-destructive mb-2", "Failed to load episode" }
                        div { class: "text-sm text-muted-foreground", "{e}" }
                    }
                },
                None => rsx! {
                    EpisodeDetailSkeleton {}
                },
            }
        }
    }
}
/// Episode detail header with back button
#[component]
fn EpisodeDetailHeader() -> Element {
    rsx! {
        div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
            div { class: "p-4 flex items-center gap-4",
                Link {
                    to: Route::PodcastHome {},
                    class: "p-2 hover:bg-muted rounded-full transition",
                    dangerous_inner_html: icons::ARROW_LEFT,
                }
                h1 { class: "text-xl font-bold", "Episode" }
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
struct EpisodeDetailContentProps {
    episode: DisplayEpisode,
    #[props(default)]
    podcast_metadata: Option<PodcastMetadata>,
}
/// Main episode detail content
#[component]
fn EpisodeDetailContent(props: EpisodeDetailContentProps) -> Element {
    let episode = props.episode.clone();
    let player_state = music_player::MUSIC_PLAYER.read();
    let mut show_chapters = use_signal(|| true);
    let mut show_share_modal = use_signal(|| false);
    let is_current_track = player_state
        .current_track
        .as_ref()
        .map(|t| t.id == episode.id)
        .unwrap_or(false);
    let is_playing = is_current_track && player_state.is_playing;
    let current_time = if is_current_track {
        player_state.current_time
    } else {
        0.0
    };
    let image_url = episode
        .image
        .clone()
        .or(episode.podcast_image.clone())
        .unwrap_or_else(|| {
            format!(
                "https://api.dicebear.com/7.x/shapes/svg?seed={}",
                episode.id
            )
        });
    let duration_str = episode
        .duration
        .map(format_duration)
        .unwrap_or_else(|| "--:--".to_string());
    let track = episode.to_music_track();
    rsx! {
        div {
            div { class: "relative",
                div { class: "absolute inset-0 h-64 bg-gradient-to-b from-primary/20 to-background" }
                div { class: "relative p-6",
                    div { class: "flex flex-col md:flex-row gap-6 items-start",
                        div { class: "w-full md:w-64 shrink-0",
                            img {
                                src: "{image_url}",
                                alt: "{episode.title}",
                                class: "w-full aspect-square rounded-lg object-cover shadow-lg",
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            {
                                if let nostr_music::TrackSource::RssPodcast { podcast_id: Some(id), .. } = &episode.source {
                                    rsx! {
                                        Link {
                                            to: Route::PodcastRssFeedDetail { podcast_id: id.to_string() },
                                            class: "text-sm text-muted-foreground mb-1 hover:text-primary transition",
                                            "{episode.podcast_title}"
                                        }
                                    }
                                } else {
                                    rsx! {
                                        div { class: "text-sm text-muted-foreground mb-1", "{episode.podcast_title}" }
                                    }
                                }
                            }
                            h1 { class: "text-2xl md:text-3xl font-bold mb-2", "{episode.title}" }
                            div { class: "flex items-center gap-4 text-sm text-muted-foreground mb-4 flex-wrap",
                                if let Some(ref date) = episode.pub_date {
                                    span { "{date}" }
                                }
                                span { "{duration_str}" }
                                if let Some(season) = episode.season {
                                    span { "Season {season}" }
                                }
                                if let Some(ep_num) = episode.episode_number {
                                    span { "Episode {ep_num}" }
                                }
                            }
                            if !episode.persons.is_empty() {
                                div { class: "mb-3",
                                    InlineCredits { persons: episode.persons.clone() }
                                }
                            }
                            div { class: "flex items-center gap-2 mb-4 flex-wrap",
                                if episode.chapters_url.is_some() {
                                    span { class: "px-2 py-1 text-xs bg-blue-500/20 text-blue-400 rounded-full font-medium flex items-center gap-1",
                                        "Chapters"
                                    }
                                }
                                if episode.has_transcript {
                                    span { class: "px-2 py-1 text-xs bg-green-500/20 text-green-400 rounded-full font-medium",
                                        "Transcript"
                                    }
                                }
                                if !episode.soundbites.is_empty() {
                                    span { class: "px-2 py-1 text-xs bg-pink-500/20 text-pink-400 rounded-full font-medium",
                                        "{episode.soundbites.len()} Clips"
                                    }
                                }
                                if episode.value.is_some() {
                                    span {
                                        class: "px-2 py-1 text-xs bg-amber-500/20 text-amber-400 rounded-full font-medium flex items-center gap-1",
                                        dangerous_inner_html: icons::ZAP,
                                        "V4V"
                                    }
                                }
                            }
                            div { class: "flex items-center gap-3",
                                button {
                                    class: "flex items-center gap-2 px-6 py-3 bg-primary text-primary-foreground rounded-full font-semibold hover:bg-primary/90 transition",
                                    onclick: {
                                        let track = track.clone();
                                        move |_| {
                                            if is_current_track {
                                                music_player::toggle_play();
                                            } else {
                                                music_player::play_track(track.clone(), None, None);
                                            }
                                        }
                                    },
                                    if is_playing {
                                        span {
                                            class: "w-5 h-5",
                                            dangerous_inner_html: icons::PAUSE,
                                        }
                                        "Pause"
                                    } else {
                                        span {
                                            class: "w-5 h-5",
                                            dangerous_inner_html: icons::PLAY,
                                        }
                                        "Play"
                                    }
                                }
                                if let Some(ref value_block) = episode.value {
                                    {
                                        let boost_episode = episode.clone();
                                        rsx! {
                                            V4VBoostButton {
                                                value_block: value_block.clone(),
                                                on_boost: move |amount| {
                                                    log::info!("Boost of {} sats requested", amount);
                                                    let track = boost_episode.to_music_track();
                                                    music_player::show_zap_dialog_for_track(Some(track));
                                                },
                                            }
                                        }
                                    }
                                }
                                button {
                                    class: "p-3 hover:bg-muted rounded-full transition",
                                    title: "Share",
                                    onclick: move |_| show_share_modal.set(true),
                                    dangerous_inner_html: icons::SHARE,
                                }
                            }
                        }
                    }
                }
            }
            {
                let share_url = match &episode.source {
                    nostr_music::TrackSource::NostrPodcast { coordinate, .. } => {
                        format!("https://nostr.blue/podcast/episode/{}", coordinate)
                    }
                    nostr_music::TrackSource::RssPodcast {
                        feed_url,
                        podcast_id,
                        episode_guid,
                        ..
                    } => {
                        if let Some(id) = podcast_id {
                            format!(
                                "https://nostr.blue/podcast/rss/episode/{}/{}",
                                urlencoding::encode(&id.to_string()),
                                urlencoding::encode(episode_guid),
                            )
                        } else {
                            format!(
                                "https://nostr.blue/podcast/rss/episode/{}/{}",
                                urlencoding::encode(feed_url),
                                urlencoding::encode(episode_guid),
                            )
                        }
                    }
                    _ => episode.audio_url.clone(),
                };
                rsx! {
                    if *show_share_modal.read() {
                        ContentShareModal {
                            title: format!("{} - {}", episode.title, episode.podcast_title),
                            url: share_url,
                            content_type: ContentType::PodcastEpisode,
                            image_url: episode.image.clone().or(episode.podcast_image.clone()),
                            on_close: move |_| show_share_modal.set(false),
                        }
                    }
                }
            }
            if is_current_track && is_playing {
                div { class: "border-b border-border bg-primary/5",
                    div { class: "px-6 py-3",
                        if episode.chapters_url.is_some() {
                            div { class: "flex items-center gap-2 text-xs text-muted-foreground mb-2",
                                span {
                                    class: "text-primary",
                                    dangerous_inner_html: icons::MUSIC_NOTE,
                                }
                                span { "See chapters below" }
                            }
                        }
                        if !episode.soundbites.is_empty() {
                            if let Some(first_soundbite) = episode.soundbites.first() {
                                div { class: "mt-2",
                                    FeaturedSoundbite {
                                        soundbite: first_soundbite.clone(),
                                        episode_title: episode.title.clone(),
                                        podcast_title: episode.podcast_title.clone(),
                                        image_url: episode.image.clone(),
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(ref chapters_url) = episode.chapters_url {
                div { class: "border-b border-border",
                    div {
                        class: "px-6 py-4 flex items-center justify-between cursor-pointer hover:bg-muted/30 transition",
                        onclick: move |_| {
                            let current = *show_chapters.read();
                            show_chapters.set(!current);
                        },
                        h2 { class: "font-semibold text-lg", "Chapters" }
                        span {
                            class: "text-muted-foreground transition-transform",
                            style: if *show_chapters.read() { "" } else { "transform: rotate(180deg)" },
                            dangerous_inner_html: icons::CHEVRON_UP,
                        }
                    }
                    if *show_chapters.read() {
                        div { class: "px-6 pb-4",
                            PodcastChapters {
                                chapters_url: chapters_url.clone(),
                                current_time,
                            }
                        }
                    }
                }
            }
            if !episode.transcripts.is_empty() {
                div { class: "border-b border-border",
                    div { class: "px-6 py-4",
                        h2 { class: "font-semibold text-lg", "Transcript" }
                    }
                    div { class: "px-6 pb-4",
                        PodcastTranscript {
                            transcripts: episode.transcripts.clone(),
                            current_time,
                        }
                    }
                }
            }
            if !episode.soundbites.is_empty() {
                div { class: "border-b border-border",
                    div { class: "px-6 py-4",
                        PodcastSoundbites {
                            soundbites: episode.soundbites.clone(),
                            episode_title: Some(episode.title.clone()),
                        }
                    }
                }
            }
            if !episode.persons.is_empty() {
                div { class: "border-b border-border",
                    div { class: "px-6 py-4",
                        h2 { class: "font-semibold text-lg mb-3", "Guests" }
                        PodcastPersons { persons: episode.persons.clone() }
                    }
                }
            }
            if let Some(ref value_block) = episode.value {
                div { class: "border-b border-border",
                    div { class: "px-6 py-4",
                        h2 { class: "font-semibold text-lg mb-3", "Support" }
                        V4VInfo {
                            value_block: value_block.clone(),
                            show_details: true,
                        }
                    }
                }
            }
            if let Some(ref description) = episode.description {
                div { class: "px-6 py-6",
                    h2 { class: "font-semibold text-lg mb-4", "Show Notes" }
                    div { class: "prose prose-sm dark:prose-invert max-w-none text-muted-foreground",
                        p { "{description}" }
                    }
                }
            }
            div { class: "h-24" }
        }
    }
}
/// Skeleton loader for episode detail
#[component]
fn EpisodeDetailSkeleton() -> Element {
    rsx! {
        div { class: "animate-pulse",
            div { class: "p-6",
                div { class: "flex flex-col md:flex-row gap-6",
                    div { class: "w-full md:w-64 aspect-square bg-muted rounded-lg" }
                    div { class: "flex-1 space-y-3",
                        div { class: "h-4 bg-muted rounded w-32" }
                        div { class: "h-8 bg-muted rounded w-3/4" }
                        div { class: "flex gap-2",
                            div { class: "h-4 bg-muted rounded w-24" }
                            div { class: "h-4 bg-muted rounded w-16" }
                        }
                        div { class: "flex gap-2",
                            div { class: "h-6 bg-muted rounded-full w-20" }
                            div { class: "h-6 bg-muted rounded-full w-16" }
                        }
                        div { class: "flex gap-3 mt-4",
                            div { class: "h-12 bg-muted rounded-full w-32" }
                            div { class: "h-12 bg-muted rounded-full w-12" }
                        }
                    }
                }
            }
            div { class: "px-6 py-4 border-t border-border",
                div { class: "h-6 bg-muted rounded w-24 mb-4" }
                div { class: "space-y-2",
                    for i in 0..5 {
                        div { key: "{i}", class: "flex gap-3 p-2",
                            div { class: "w-12 h-12 bg-muted rounded" }
                            div { class: "flex-1 space-y-1",
                                div { class: "h-4 bg-muted rounded w-3/4" }
                                div { class: "h-3 bg-muted rounded w-1/2" }
                            }
                            div { class: "w-12 h-4 bg-muted rounded" }
                        }
                    }
                }
            }
            div { class: "px-6 py-6 border-t border-border space-y-2",
                div { class: "h-6 bg-muted rounded w-32 mb-4" }
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-5/6" }
                div { class: "h-4 bg-muted rounded w-4/6" }
            }
        }
    }
}
/// Fetch Nostr podcast episode by naddr/coordinate
async fn fetch_nostr_episode(naddr: &str) -> Result<(DisplayEpisode, PodcastMetadata), String> {
    let (pubkey, d_tag) = parse_coordinate(naddr)?;
    let episode_filter = Filter::new()
        .kind(Kind::from(podcast::KIND_PODCAST_EPISODE))
        .author(PublicKey::from_hex(&pubkey).map_err(|e| e.to_string())?)
        .custom_tag(SingleLetterTag::from_char('d').unwrap(), d_tag.clone())
        .limit(1);
    let episode_events =
        nostr_client::fetch_events_aggregated(episode_filter, Duration::from_secs(10)).await?;
    let episode_event = episode_events
        .first()
        .ok_or_else(|| "Episode not found".to_string())?;
    let episode = podcast::parse_podcast_episode(episode_event)?;
    let metadata_filter = Filter::new()
        .kind(Kind::from(podcast::KIND_APP_DATA))
        .author(PublicKey::from_hex(&pubkey).map_err(|e| e.to_string())?)
        .limit(1);
    let metadata_events =
        nostr_client::fetch_events_aggregated(metadata_filter, Duration::from_secs(5)).await?;
    let metadata = if let Some(meta_event) = metadata_events.first() {
        podcast::parse_podcast_metadata(meta_event).ok()
    } else {
        None
    };
    let metadata = metadata.unwrap_or_else(|| PodcastMetadata {
        title: "Unknown Podcast".to_string(),
        description: None,
        author: None,
        image: None,
        language: None,
        categories: Vec::new(),
        explicit: false,
        website: None,
        funding: Vec::new(),
        value: None,
        pubkey: pubkey.clone(),
        d_tag: "".to_string(),
        created_at: 0,
    });
    let display_episode =
        DisplayEpisode::from_nostr_episode(&episode, &metadata.title, metadata.image.as_deref());
    Ok((display_episode, metadata))
}
/// Fetch RSS podcast episode by podcast ID (numeric Podcast Index ID) or feed URL and episode GUID
async fn fetch_rss_episode(podcast_id: &str, episode_id: &str) -> Result<DisplayEpisode, String> {
    use crate::services::podcast_index;
    let decoded_episode_id = urlencoding::decode(episode_id)
        .map_err(|e| format!("Invalid episode ID encoding: {}", e))?
        .into_owned();
    if let Ok(feed_id) = podcast_id.parse::<u64>() {
        log::info!(
            "Fetching episode via Podcast Index API: feed_id={}, episode_id={}",
            feed_id,
            decoded_episode_id
        );
        let feed = podcast_index::get_podcast_by_id(feed_id).await?;
        let episodes = podcast_index::get_episodes_by_feed_id(feed_id, Some(100), None).await?;
        let episode = episodes
            .iter()
            .find(|ep| ep.id.to_string() == decoded_episode_id)
            .ok_or_else(|| format!("Episode not found: {}", decoded_episode_id))?;
        return Ok(DisplayEpisode::from_podcast_index_episode(episode, &feed));
    }
    let feed_url = urlencoding::decode(podcast_id)
        .map_err(|e| format!("Invalid podcast URL encoding: {}", e))?
        .into_owned();
    log::info!(
        "Fetching episode via RSS feed: url={}, episode_guid={}",
        feed_url,
        decoded_episode_id
    );
    let podcast = podcast_rss::fetch_podcast_feed(&feed_url).await?;
    let episode = podcast
        .episodes
        .iter()
        .find(|ep| ep.guid == decoded_episode_id)
        .ok_or_else(|| "Episode not found".to_string())?;
    Ok(DisplayEpisode::from_rss_episode(episode, &podcast))
}
/// Parse coordinate string into pubkey and d-tag
fn parse_coordinate(coord: &str) -> Result<(String, String), String> {
    use nostr::prelude::*;
    if coord.starts_with("naddr") {
        let nip19 = Nip19::from_bech32(coord).map_err(|e| format!("Invalid naddr: {}", e))?;
        match nip19 {
            Nip19::Coordinate(nip19_coord) => {
                let pubkey = nip19_coord.coordinate.public_key.to_hex();
                let d_tag = nip19_coord.coordinate.identifier;
                Ok((pubkey, d_tag))
            }
            _ => Err("Expected naddr coordinate".to_string()),
        }
    } else {
        let parts: Vec<&str> = coord.splitn(3, ':').collect();
        if parts.len() >= 3 {
            return Ok((parts[1].to_string(), parts[2].to_string()));
        }
        if parts.len() == 2 {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
        Err(format!("Invalid coordinate format: {}", coord))
    }
}
