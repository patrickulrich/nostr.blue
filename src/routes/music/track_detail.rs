//! Music Track Detail Route
//!
//! Displays a single music track with:
//! - Track artwork, title, artist name
//! - Full player controls (uses unified audio player)
//! - Album/artist links
//! - Zap/share buttons
//!
//! Supports multiple track sources:
//! - Wavlake tracks (UUID format)
//! - Nostr tracks (naddr1... format, kind 36787)
//! - RSS Music tracks (rss:{feed_id}:{episode_id} format)

use crate::components::{icons, ContentShareModal, ContentType};
use crate::routes::podcast::podcast_shared_states::PodcastApiAuthRequiredState;
use crate::routes::Route;
use crate::services::{podcast_index, wavlake::WavlakeAPI};
use crate::stores::{music_player, music_player::MusicPlayerStateStoreExt, nostr_client, nostr_music, profiles};
use dioxus::prelude::*;

const AUTH_REQUIRED_SENTINEL: &str = "__auth_required__";

enum TrackDetailState {
    Loading,
    Loaded(Box<music_player::MusicTrack>),
    AuthRequired,
    Error(String),
}

/// Detail page for a music track (supports Wavlake, Nostr, and RSS sources)
#[component]
pub fn MusicTrackDetail(track_id: ReadSignal<String>) -> Element {
    let track_data = use_resource(move || {
        let id = track_id();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = nostr_client::has_signer();
        async move {
            if !client_initialized && id.starts_with("naddr") {
                return Err("Waiting for client initialization...".to_string());
            }
            if id.starts_with("rss:") && !has_signer {
                return Err(AUTH_REQUIRED_SENTINEL.to_string());
            }
            fetch_track(&id).await
        }
    });

    let state = match &*track_data.read() {
        None => TrackDetailState::Loading,
        Some(Ok(track)) => TrackDetailState::Loaded(Box::new(track.clone())),
        Some(Err(e)) if e == AUTH_REQUIRED_SENTINEL => TrackDetailState::AuthRequired,
        Some(Err(e)) => TrackDetailState::Error(e.clone()),
    };

    rsx! {
        div { class: "min-h-screen",
            TrackDetailHeader {}
            match state {
                TrackDetailState::Loaded(track) => rsx! {
                    TrackDetailContent { track: *track }
                },
                TrackDetailState::AuthRequired => rsx! {
                    PodcastApiAuthRequiredState { item_label: "track" }
                },
                TrackDetailState::Error(e) => rsx! {
                    div { class: "p-4 text-center",
                        div { class: "text-destructive mb-2", "Failed to load track" }
                        div { class: "text-sm text-muted-foreground", "{e}" }
                    }
                },
                TrackDetailState::Loading => rsx! {
                    TrackDetailSkeleton {}
                },
            }
        }
    }
}

/// Track detail header with back button
#[component]
fn TrackDetailHeader() -> Element {
    rsx! {
        div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
            div { class: "p-4 flex items-center gap-4",
                Link {
                    to: Route::MusicHome {},
                    class: "p-2 hover:bg-muted rounded-full transition",
                    dangerous_inner_html: icons::ARROW_LEFT,
                }
                h1 { class: "text-xl font-bold", "Track" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TrackDetailContentProps {
    track: music_player::MusicTrack,
}

/// Main track detail content
#[component]
fn TrackDetailContent(props: TrackDetailContentProps) -> Element {
    let track = props.track.clone();
    let store = music_player::MUSIC_PLAYER.resolve();
    let mut show_share_modal = use_signal(|| false);

    let current_track = store.current_track().cloned();
    let is_current_track = current_track
        .as_ref()
        .map(|t| t.id == track.id)
        .unwrap_or(false);
    let is_playing = is_current_track && store.is_playing().cloned();

    let image_url = track
        .album_art_url
        .clone()
        .or(track.artist_art_url.clone())
        .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/shapes/svg?seed={}", track.id));

    let duration_str = track
        .duration
        .map(format_duration)
        .unwrap_or_else(|| "--:--".to_string());

    // Determine routes for artist and album links
    let artist_route = get_artist_route(&track);
    let album_route = get_album_route(&track);
    let share_url = track.share_url();

    rsx! {
        div {
            // Hero section with gradient background
            div { class: "relative",
                div { class: "absolute inset-0 h-64 bg-gradient-to-b from-primary/20 to-background" }
                div { class: "relative p-6",
                    div { class: "flex flex-col md:flex-row gap-6 items-start",
                        // Album artwork
                        div { class: "w-full md:w-64 shrink-0",
                            img {
                                src: "{image_url}",
                                alt: "{track.title}",
                                class: "w-full aspect-square rounded-lg object-cover shadow-lg",
                            }
                        }

                        // Track info
                        div { class: "flex-1 min-w-0",
                            // Album link (if available)
                            if let Some(ref album) = track.album {
                                if let Some(ref route) = album_route {
                                    Link {
                                        to: route.clone(),
                                        class: "text-sm text-muted-foreground mb-1 hover:text-primary transition block",
                                        "{album}"
                                    }
                                } else {
                                    div { class: "text-sm text-muted-foreground mb-1", "{album}" }
                                }
                            }

                            // Track title
                            h1 { class: "text-2xl md:text-3xl font-bold mb-2", "{track.title}" }

                            // Artist link
                            div { class: "text-lg text-muted-foreground mb-4",
                                "by "
                                if let Some(ref route) = artist_route {
                                    Link {
                                        to: route.clone(),
                                        class: "hover:text-primary transition underline",
                                        "{track.artist}"
                                    }
                                } else {
                                    span { "{track.artist}" }
                                }
                            }

                            // Track metadata
                            div { class: "flex items-center gap-4 text-sm text-muted-foreground mb-4 flex-wrap",
                                span { "{duration_str}" }
                                // Source indicator
                                match &track.source {
                                    nostr_music::TrackSource::Wavlake { .. } => rsx! {
                                        span { class: "px-2 py-1 text-xs bg-purple-500/20 text-purple-400 rounded-full font-medium",
                                            "Wavlake"
                                        }
                                    },
                                    nostr_music::TrackSource::Nostr { .. } => rsx! {
                                        span { class: "px-2 py-1 text-xs bg-blue-500/20 text-blue-400 rounded-full font-medium",
                                            "Nostr"
                                        }
                                    },
                                    nostr_music::TrackSource::RssMusic { .. } => rsx! {
                                        span { class: "px-2 py-1 text-xs bg-orange-500/20 text-orange-400 rounded-full font-medium",
                                            "RSS Music"
                                        }
                                    },
                                    _ => rsx! {}
                                }
                            }

                            // Action buttons
                            div { class: "flex items-center gap-3 flex-wrap",
                                // Play/Pause button
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

                                // Zap button (if artist has lightning)
                                {
                                    let zap_track = track.clone();
                                    rsx! {
                                        button {
                                            class: "flex items-center gap-2 px-4 py-3 rounded-full border border-border hover:bg-muted transition",
                                            onclick: move |_| music_player::show_zap_dialog_for_track(Some(zap_track.clone())),
                                            span {
                                                class: "w-5 h-5",
                                                dangerous_inner_html: icons::ZAP,
                                            }
                                            "Zap"
                                        }
                                    }
                                }

                                // Share button
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

            // Share modal
            if *show_share_modal.read() {
                ContentShareModal {
                    title: format!("{} - {}", track.title, track.artist),
                    url: share_url.clone(),
                    content_type: ContentType::MusicTrack,
                    image_url: track.album_art_url.clone(),
                    on_close: move |_| show_share_modal.set(false),
                }
            }

            // More from album section (for Wavlake tracks)
            if let nostr_music::TrackSource::Wavlake { album_id, .. } = &track.source {
                MoreFromAlbum { album_id: album_id.clone(), current_track_id: track.id.clone() }
            }

            // Spacer for bottom player
            div { class: "h-24" }
        }
    }
}

/// More tracks from the same album (Wavlake only)
#[component]
fn MoreFromAlbum(album_id: String, current_track_id: String) -> Element {
    let album_data = use_resource(move || {
        let id = album_id.clone();
        async move { WavlakeAPI::new().get_album(&id).await }
    });

    // Extract data from resource before building RSX
    let album_info = {
        let read = album_data.read();
        match &*read {
            Some(Ok(album)) => {
                let other_tracks: Vec<_> = album
                    .tracks
                    .iter()
                    .filter(|t| t.id != current_track_id)
                    .cloned()
                    .collect();

                if other_tracks.is_empty() {
                    None
                } else {
                    Some((
                        album.id.clone(),
                        album.title.clone(),
                        other_tracks,
                        album.tracks.clone(),
                    ))
                }
            }
            _ => None,
        }
    };

    match album_info {
        Some((album_id, album_title, other_tracks, all_tracks)) => {
            rsx! {
                div { class: "border-t border-border",
                    div { class: "px-6 py-4",
                        h2 { class: "font-semibold text-lg mb-4",
                            "More from "
                            Link {
                                to: Route::MusicAlbum { album_id: album_id.clone() },
                                class: "hover:underline",
                                "{album_title}"
                            }
                        }
                        div { class: "space-y-2",
                            for track in other_tracks.iter() {
                                {
                                    let music_track: music_player::MusicTrack = track.clone().into();
                                    let album_tracks: Vec<music_player::MusicTrack> = all_tracks
                                        .iter()
                                        .map(|t| t.clone().into())
                                        .collect();
                                    let index = all_tracks.iter().position(|t| t.id == track.id).unwrap_or(0);
                                    rsx! {
                                        div {
                                            key: "{track.id}",
                                            class: "flex items-center gap-3 p-2 rounded-lg hover:bg-muted transition cursor-pointer group",
                                            onclick: {
                                                let music_track = music_track.clone();
                                                let album_tracks = album_tracks.clone();
                                                move |_| {
                                                    music_player::play_track(music_track.clone(), Some(album_tracks.clone()), Some(index));
                                                }
                                            },
                                            div { class: "w-12 h-12 rounded overflow-hidden shrink-0",
                                                img {
                                                    src: "{track.album_art_url}",
                                                    alt: "{track.title}",
                                                    class: "w-full h-full object-cover",
                                                }
                                            }
                                            div { class: "flex-1 min-w-0",
                                                div { class: "font-medium truncate", "{track.title}" }
                                                div { class: "text-sm text-muted-foreground truncate", "{track.artist}" }
                                            }
                                            div { class: "text-sm text-muted-foreground",
                                                {format_duration(track.duration)}
                                            }
                                            div { class: "opacity-0 group-hover:opacity-100 transition",
                                                span {
                                                    class: "w-5 h-5 text-primary",
                                                    dangerous_inner_html: icons::PLAY,
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None => rsx! {},
    }
}

/// Skeleton loader for track detail
#[component]
fn TrackDetailSkeleton() -> Element {
    rsx! {
        div { class: "animate-pulse",
            div { class: "p-6",
                div { class: "flex flex-col md:flex-row gap-6",
                    div { class: "w-full md:w-64 aspect-square bg-muted rounded-lg" }
                    div { class: "flex-1 space-y-3",
                        div { class: "h-4 bg-muted rounded w-32" }
                        div { class: "h-8 bg-muted rounded w-3/4" }
                        div { class: "h-5 bg-muted rounded w-48" }
                        div { class: "flex gap-2",
                            div { class: "h-4 bg-muted rounded w-16" }
                            div { class: "h-6 bg-muted rounded-full w-20" }
                        }
                        div { class: "flex gap-3 mt-4",
                            div { class: "h-12 bg-muted rounded-full w-32" }
                            div { class: "h-12 bg-muted rounded-full w-24" }
                            div { class: "h-12 bg-muted rounded-full w-12" }
                        }
                    }
                }
            }
        }
    }
}

/// Fetch track by ID (handles Wavlake, Nostr, and RSS sources)
pub(crate) async fn fetch_track(id: &str) -> Result<music_player::MusicTrack, String> {
    if id.starts_with("naddr1") {
        // Nostr track (kind 36787)
        let parsed = crate::utils::nip19::parse_naddr(id)?;
        let nostr_track = nostr_music::fetch_nostr_track_by_coordinate(&parsed.pubkey, &parsed.identifier, parsed.relay_hints)
            .await?
            .ok_or("Track not found")?;

        // Fetch artist profile for name
        let mut track: music_player::MusicTrack = nostr_track.into();
        if let Ok(profile) =
            profiles::fetch_profile(track.artist_npub.clone().unwrap_or_default()).await
        {
            track.artist = profile
                .display_name
                .or(profile.name)
                .unwrap_or_else(|| "Unknown Artist".to_string());
        }
        Ok(track)
    } else if let Some(rest) = id.strip_prefix("rss:") {
        // RSS Music track (format: rss:{feed_id}:{episode_id})
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        if parts.len() < 2 {
            return Err("Invalid RSS track ID format".to_string());
        }
        let feed_id: u64 = parts[0].parse().map_err(|_| "Invalid feed ID")?;
        let episode_id: u64 = parts[1].parse().map_err(|_| "Invalid episode ID")?;

        let feed = podcast_index::get_podcast_by_id(feed_id).await?;
        let episodes = podcast_index::get_episodes_by_feed_id(feed_id, Some(100), None).await?;
        let episode = episodes
            .iter()
            .find(|e| e.id == episode_id)
            .ok_or("Track not found")?;

        Ok(music_player::MusicTrack::from_rss_music_track(
            episode, &feed,
        ))
    } else {
        // Wavlake track (UUID)
        let api = WavlakeAPI::new();
        let track = api.get_track(id).await?;
        Ok(track.into())
    }
}

/// Get route to artist page based on track source
fn get_artist_route(track: &music_player::MusicTrack) -> Option<Route> {
    match &track.source {
        nostr_music::TrackSource::Wavlake { artist_id, .. } => Some(Route::MusicArtist {
            artist_id: artist_id.clone(),
        }),
        nostr_music::TrackSource::Nostr { pubkey, .. } => Some(Route::Profile {
            pubkey: pubkey.clone(),
        }),
        nostr_music::TrackSource::RssMusic { .. } => None,
        _ => None,
    }
}

/// Get route to album page based on track source
fn get_album_route(track: &music_player::MusicTrack) -> Option<Route> {
    match &track.source {
        nostr_music::TrackSource::Wavlake { album_id, .. } => Some(Route::MusicAlbum {
            album_id: album_id.clone(),
        }),
        nostr_music::TrackSource::RssMusic { feed_id, .. } => {
            Some(Route::MusicRssAlbum { feed_id: *feed_id })
        }
        _ => None,
    }
}

/// Format duration in seconds to MM:SS
fn format_duration(seconds: u32) -> String {
    let mins = seconds / 60;
    let secs = seconds % 60;
    format!("{}:{:02}", mins, secs)
}
