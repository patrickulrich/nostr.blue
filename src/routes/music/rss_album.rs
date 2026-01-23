use dioxus::prelude::*;
use std::sync::Arc;
use crate::routes::Route;
use crate::components::icons::*;
use crate::services::podcast_index::{self, PodcastFeed, Episode};
use crate::stores::music_player::{self, MusicTrack};
use crate::stores::nostr_client;
use crate::components::{UnifiedTrackCard, UnifiedTrackCardSkeleton, ContentShareModal, ContentType};

#[component]
pub fn MusicRssAlbum(feed_id: u64) -> Element {
    let mut feed_state = use_signal(|| None::<PodcastFeed>);
    let mut episodes_state = use_signal(Vec::<Episode>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    let mut show_share_modal = use_signal(|| false);

    // Fetch album (feed) and tracks (episodes)
    // Wait for client initialization and signer before making NIP-98 authenticated requests
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = nostr_client::has_signer();

        // Only fetch if client is initialized and has signer (NIP-98 auth requires signer)
        if !client_initialized || !has_signer {
            return;
        }

        // Capture feed_id for the async block
        let feed_id = feed_id;

        loading.set(true);
        error_msg.set(None);

        spawn(async move {
            // Fetch feed metadata
            match podcast_index::get_podcast_by_id(feed_id).await {
                Ok(feed) => {
                    feed_state.set(Some(feed));
                }
                Err(e) => {
                    error_msg.set(Some(format!("Failed to load album: {}", e)));
                    loading.set(false);
                    return;
                }
            }

            // Fetch episodes (tracks)
            match podcast_index::get_episodes_by_feed_id(feed_id, Some(100)).await {
                Ok(episodes) => {
                    episodes_state.set(episodes);
                }
                Err(e) => {
                    log::warn!("Failed to load tracks: {}", e);
                }
            }

            loading.set(false);
        });
    });

    // Convert episodes to MusicTrack for player, wrapped in Arc for efficient sharing
    let tracks_arc = use_memo(move || {
        let tracks = if let Some(ref feed) = *feed_state.read() {
            episodes_state.read()
                .iter()
                .map(|ep| MusicTrack::from_rss_music_track(ep, feed))
                .collect::<Vec<MusicTrack>>()
        } else {
            Vec::new()
        };
        Arc::new(tracks)
    });

    let play_album = move |tracks: Arc<Vec<MusicTrack>>| {
        if let Some(first_track) = tracks.first().cloned() {
            // Convert Arc to Vec for player (player owns the playlist)
            music_player::play_track(first_track, Some((*tracks).clone()), Some(0));
        }
    };

    rsx! {
        div { class: "container mx-auto px-4 py-8 max-w-5xl",
            // Back button
            Link {
                to: Route::MusicHome {},
                class: "inline-flex items-center gap-2 text-muted-foreground hover:text-foreground mb-6 transition-colors",
                ArrowLeftIcon { class: "w-4 h-4".to_string() }
                "Back to Music"
            }

            // Loading state
            if *loading.read() {
                div { class: "bg-muted/30 rounded-lg border border-border p-6",
                    div { class: "flex items-center gap-6",
                        div { class: "w-48 h-48 bg-muted rounded-lg animate-pulse" }
                        div { class: "flex-1 space-y-4",
                            div { class: "h-8 bg-muted rounded w-64 animate-pulse" }
                            div { class: "h-4 bg-muted rounded w-48 animate-pulse" }
                            div { class: "h-4 bg-muted rounded w-32 animate-pulse" }
                        }
                    }
                }
                div { class: "mt-6 space-y-1",
                    for i in 0..8 {
                        UnifiedTrackCardSkeleton { key: "{i}" }
                    }
                }
            }

            // Error state
            if let Some(err) = error_msg.read().clone() {
                div { class: "bg-muted/30 rounded-lg border border-border p-8 text-center",
                    MusicIcon { class: "w-12 h-12 text-muted-foreground mx-auto mb-4" }
                    h2 { class: "text-2xl font-bold mb-2", "Album Not Found" }
                    p { class: "text-muted-foreground", "{err}" }
                }
            }

            // Album content
            if !*loading.read() && error_msg.read().is_none() {
                if let Some(feed) = feed_state.read().clone() {
                    {
                        let tracks_arc = tracks_arc();
                        let album_art = feed.get_image().map(String::from);
                        let artist = feed.author.clone().unwrap_or_else(|| feed.title.clone());

                        rsx! {
                            div { class: "space-y-6",
                                // Album Header
                                div { class: "bg-muted/30 rounded-lg border border-border p-6",
                                    div { class: "flex flex-col sm:flex-row items-start gap-6",
                                        // Album art
                                        div { class: "w-48 h-48 bg-muted rounded-lg flex items-center justify-center overflow-hidden shrink-0",
                                            if let Some(ref art_url) = album_art {
                                                img {
                                                    src: "{art_url}",
                                                    alt: "{feed.title}",
                                                    class: "w-full h-full object-cover"
                                                }
                                            } else {
                                                MusicIcon { class: "w-24 h-24 text-muted-foreground" }
                                            }
                                        }

                                        div { class: "flex-1 space-y-4",
                                            div {
                                                div { class: "flex items-center gap-2 mb-2",
                                                    span { class: "inline-block px-2 py-1 rounded text-xs font-medium bg-orange-500/20 text-orange-400",
                                                        "RSS Music"
                                                    }
                                                    if feed.value.is_some() {
                                                        span { class: "inline-block px-2 py-1 rounded text-xs font-medium bg-green-500/20 text-green-400",
                                                            "V4V"
                                                        }
                                                    }
                                                }
                                                h1 { class: "text-3xl font-bold", "{feed.title}" }
                                                p { class: "text-xl text-muted-foreground mt-2",
                                                    "by {artist}"
                                                }
                                            }

                                            div { class: "flex items-center gap-4 text-sm text-muted-foreground",
                                                span { class: "flex items-center gap-1",
                                                    MusicIcon { class: "w-3 h-3" }
                                                    "{tracks_arc.len()} "
                                                    if tracks_arc.len() == 1 { "track" } else { "tracks" }
                                                }
                                                if let Some(lang) = &feed.language {
                                                    span { class: "flex items-center gap-1",
                                                        "{lang}"
                                                    }
                                                }
                                            }

                                            if let Some(ref desc) = feed.description {
                                                p { class: "text-sm text-muted-foreground line-clamp-3",
                                                    "{desc}"
                                                }
                                            }

                                            div { class: "flex items-center gap-4 pt-2",
                                                button {
                                                    class: "px-4 py-2 bg-primary hover:bg-primary/90 rounded text-primary-foreground transition-colors inline-flex items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed",
                                                    disabled: tracks_arc.is_empty(),
                                                    onclick: move |_| {
                                                        play_album(tracks_arc.clone());
                                                    },
                                                    PlayIcon { class: "w-4 h-4" }
                                                    "Play Album"
                                                }

                                                if let Some(first_track) = tracks_arc.first() {
                                                    {
                                                        let zap_track = first_track.clone();
                                                        rsx! {
                                                            button {
                                                                class: "px-4 py-2 rounded border border-border hover:border-primary text-sm transition-colors inline-flex items-center gap-1",
                                                                onclick: move |_| music_player::show_zap_dialog_for_track(Some(zap_track.clone())),
                                                                ZapIcon { class: "w-3 h-3" }
                                                                "Zap Artist"
                                                            }
                                                        }
                                                    }
                                                }

                                                // Share button
                                                button {
                                                    class: "px-4 py-2 rounded border border-border hover:border-primary text-sm transition-colors inline-flex items-center gap-1",
                                                    onclick: move |_| show_share_modal.set(true),
                                                    ShareIcon { class: "w-3 h-3" }
                                                    "Share"
                                                }
                                            }
                                        }
                                    }
                                }

                                // Share modal
                                if *show_share_modal.read() {
                                    ContentShareModal {
                                        title: feed.title.clone(),
                                        url: format!("https://nostr.blue/music/rss/album/{}", feed_id),
                                        content_type: ContentType::MusicAlbum,
                                        image_url: feed.get_image().map(String::from),
                                        on_close: move |_| show_share_modal.set(false)
                                    }
                                }

                                // Track List
                                div { class: "space-y-1",
                                    div { class: "flex items-center gap-2 mb-4 px-3",
                                        MusicIcon { class: "w-5 h-5 text-primary" }
                                        h2 { class: "text-xl font-bold", "Tracks" }
                                    }

                                    if tracks_arc.is_empty() {
                                        div { class: "text-center py-8",
                                            MusicIcon { class: "w-12 h-12 text-muted-foreground mx-auto mb-4" }
                                            p { class: "text-muted-foreground", "No tracks found in this album." }
                                        }
                                    } else {
                                        div { class: "divide-y divide-border/50",
                                            for track in tracks_arc.iter() {
                                                UnifiedTrackCard {
                                                    key: "{track.id}",
                                                    track: track.clone(),
                                                    show_album: false,
                                                    show_sats: true,
                                                    playlist: Some(tracks_arc.clone())
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
    }
}
