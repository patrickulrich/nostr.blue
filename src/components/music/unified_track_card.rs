use crate::components::icons;
use crate::components::{ContentShareModal, ContentType};
use crate::routes::Route;
use crate::stores::music_player::{self, MusicPlayerStateStoreExt, MusicTrack};
use crate::stores::nostr_music::TrackSource;
use crate::stores::profiles;
use super::FALLBACK_ART_URL;
use dioxus::prelude::*;
use std::sync::Arc;

#[derive(Props, Clone, PartialEq)]
pub struct UnifiedTrackCardProps {
    pub track: MusicTrack,
    #[props(default = false)]
    pub show_album: bool,
    #[props(default = true)]
    pub show_source_badge: bool,
    #[props(default = true)]
    pub show_sats: bool,
    /// Optional playlist to enable continuous playback (uses Arc for efficient sharing)
    #[props(default)]
    pub playlist: Option<Arc<Vec<MusicTrack>>>,
}
/// Unified track card that handles both Wavlake and Nostr tracks
#[component]
pub fn UnifiedTrackCard(props: UnifiedTrackCardProps) -> Element {
    let track = props.track.clone();
    let track_id = track.id.clone();
    let track_id_for_memo = track_id.clone();
    let is_playing = use_memo(move || {
        let store = music_player::MUSIC_PLAYER.resolve();
        let current = store.current_track().cloned();
        if let Some(ref cur) = current {
            cur.id == track_id_for_memo && store.is_playing().cloned()
        } else {
            false
        }
    });
    let mut show_share_modal = use_signal(|| false);
    let artist_pubkey = track.artist_npub.clone();
    let artist_is_empty = track.artist.is_empty();
    let mut artist_name = use_signal(|| track.artist.clone());
    let mut artist_lookup_gen = use_signal(|| 0u32);
    use_effect(use_reactive(
        (&track.id, &track.artist, &artist_pubkey, &artist_is_empty),
        move |(track_id, track_artist, pubkey_opt, is_empty)| {
            let _ = track_id;
            artist_name.set(track_artist.clone());
            let gen = artist_lookup_gen.with_mut(|g| {
                *g = g.wrapping_add(1);
                *g
            });
            if let Some(pubkey) = pubkey_opt.clone() {
                if is_empty {
                    spawn(async move {
                        if let Ok(profile) = profiles::fetch_profile(pubkey).await {
                            if *artist_lookup_gen.peek() == gen {
                                artist_name.set(profile.get_display_name());
                            }
                        }
                    });
                }
            }
        },
    ));
    let playlist = props.playlist.clone();
    let handle_play = {
        let track = track.clone();
        let playlist = playlist.clone();
        move |_| {
            let playlist_vec = playlist.as_ref().map(|arc| (**arc).clone());
            music_player::play_or_toggle_track(track.clone(), playlist_vec, None);
        }
    };
    let duration_str = track
        .duration
        .map(|d| {
            let mins = d / 60;
            let secs = d % 60;
            format!("{:02}:{:02}", mins, secs)
        })
        .unwrap_or_else(|| "--:--".to_string());
    let sats_display = track.msat_total.map(|msats| {
        let sats = msats / 1000;
        if sats >= 1_000_000 {
            format!("{}M sats", sats / 1_000_000)
        } else if sats >= 1_000 {
            format!("{}K sats", sats / 1_000)
        } else {
            format!("{} sats", sats)
        }
    });
    let source_info = match &track.source {
        TrackSource::Wavlake { .. } => ("W", "Wavlake", "bg-orange-500/20 text-orange-400"),
        TrackSource::Nostr { .. } => ("N", "Nostr", "bg-purple-500/20 text-purple-400"),
        TrackSource::NostrPodcast { .. } => {
            ("P", "Nostr Podcast", "bg-green-500/20 text-green-400")
        }
        TrackSource::RssPodcast { .. } => ("R", "RSS Podcast", "bg-green-500/20 text-green-400"),
        TrackSource::RssMusic { .. } => (
            "RSS",
            "Podcasting 2.0 Music",
            "bg-orange-500/20 text-orange-400",
        ),
        TrackSource::Radio { .. } => ("LIVE", "Internet Radio", "bg-red-500/20 text-red-400"),
        TrackSource::Bible { .. } => ("B", "Bible", "bg-blue-500/20 text-blue-400"),
    };
    let art_url = track
        .album_art_url
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| FALLBACK_ART_URL.to_string());
    let mut img_src = use_signal(|| art_url.clone());
    {
        let url_for_sync = art_url.clone();
        use_effect(use_reactive((&url_for_sync,), move |(url,)| {
            img_src.set(url);
        }));
    }
    let share_url = track.share_url();
    let share_content_type = match &track.source {
        TrackSource::NostrPodcast { .. } | TrackSource::RssPodcast { .. } => ContentType::PodcastEpisode,
        TrackSource::Radio { .. } => ContentType::RadioStation,
        TrackSource::Bible { .. } => ContentType::BibleVerse,
        _ => ContentType::MusicTrack,
    };
    let artist_route = match &track.source {
        TrackSource::Wavlake { artist_id, .. } => Some(Route::MusicArtist {
            artist_id: artist_id.clone(),
        }),
        TrackSource::Nostr { pubkey, .. } => Some(Route::MusicArtist {
            artist_id: pubkey.clone(),
        }),
        TrackSource::NostrPodcast { pubkey, .. } => Some(Route::Profile {
            pubkey: pubkey.clone(),
        }),
        TrackSource::RssPodcast { podcast_id, .. } => {
            podcast_id.map(|id| Route::PodcastRssFeedDetail {
                podcast_id: id.to_string(),
            })
        }
        TrackSource::RssMusic { feed_id, .. } => Some(Route::MusicRssAlbum { feed_id: *feed_id }),
        TrackSource::Radio { pubkey, .. } => Some(Route::Profile {
            pubkey: pubkey.clone(),
        }),
        TrackSource::Bible { translation, book, chapter, .. } => Some(Route::BibleChapter {
            translation: translation.clone(),
            book: book.clone(),
            chapter: *chapter,
        }),
    };
    rsx! {
        div {
            class: "flex items-center gap-3 p-3 hover:bg-muted/50 rounded-lg transition group cursor-pointer",
            onclick: handle_play,
            div { class: "relative shrink-0",
                img {
                    src: "{img_src}",
                    alt: "Album art",
                    class: "w-14 h-14 rounded object-cover",
                    loading: "lazy",
                    referrerpolicy: "no-referrer",
                    onerror: move |_| img_src.set(FALLBACK_ART_URL.to_string()),
                }
                if props.show_source_badge {
                    div {
                        class: "absolute -top-1 -right-1 w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold {source_info.2}",
                        title: "{source_info.1}",
                        "{source_info.0}"
                    }
                }
                button {
                    class: "absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 group-hover:opacity-100 transition rounded",
                    dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY },
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "font-medium text-sm truncate",
                    {
                        let title_class = if *is_playing.read() { "text-primary" } else { "" };
                        if let Some(route) = track.get_track_route() {
                            rsx! {
                                Link {
                                    to: route,
                                    class: "hover:underline {title_class}",
                                    onclick: move |e: Event<MouseData>| e.stop_propagation(),
                                    "{track.title}"
                                }
                            }
                        } else if let Some(episode_route) = track.get_episode_route() {
                            rsx! {
                                Link {
                                    to: episode_route,
                                    class: "hover:underline {title_class}",
                                    onclick: move |e: Event<MouseData>| e.stop_propagation(),
                                    "{track.title}"
                                }
                            }
                        } else {
                            rsx! {
                                span { class: "{title_class}", "{track.title}" }
                            }
                        }
                    }
                }
                div { class: "text-xs text-muted-foreground truncate",
                    if let Some(route) = artist_route.clone() {
                        Link {
                            to: route,
                            class: "hover:text-foreground hover:underline",
                            onclick: move |e: Event<MouseData>| e.stop_propagation(),
                            "{artist_name}"
                        }
                    } else {
                        span { "{artist_name}" }
                    }
                }
                if props.show_album {
                    if let Some(ref album) = track.album {
                        div { class: "text-xs text-muted-foreground truncate",
                            match &track.source {
                                TrackSource::Wavlake { album_id, .. } => rsx! {
                                    Link {
                                        to: Route::MusicAlbum {
                                            album_id: album_id.clone(),
                                        },
                                        class: "hover:text-foreground hover:underline",
                                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                                        "{album}"
                                    }
                                },
                                TrackSource::RssMusic { feed_id, .. } => rsx! {
                                    Link {
                                        to: Route::MusicRssAlbum {
                                            feed_id: *feed_id,
                                        },
                                        class: "hover:text-foreground hover:underline",
                                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                                        "{album}"
                                    }
                                },
                                TrackSource::Nostr { .. }
                                | TrackSource::NostrPodcast { .. }
                                | TrackSource::RssPodcast { .. }
                                | TrackSource::Radio { .. }
                                | TrackSource::Bible { .. } => rsx! {
                                    span { "{album}" }
                                },
                            }
                        }
                    }
                }
            }
            if props.show_sats {
                if let Some(sats) = &sats_display {
                    div {
                        class: "flex items-center gap-1 text-xs font-medium text-amber-500 shrink-0",
                        dangerous_inner_html: icons::ZAP,
                        span { "{sats}" }
                    }
                }
            }
            div { class: "text-xs text-muted-foreground shrink-0", "{duration_str}" }
            div { class: "flex items-center gap-1 shrink-0 opacity-0 group-hover:opacity-100 transition",
                button {
                    class: "p-2 hover:bg-muted rounded-full transition",
                    title: "Vote for this track",
                    onclick: {
                        let vote_track = track.clone();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            let t = vote_track.clone();
                            spawn(async move {
                                if let Err(e) = music_player::vote_for_music(&t).await {
                                    log::error!("Vote failed: {}", e);
                                }
                            });
                        }
                    },
                    dangerous_inner_html: icons::HEART,
                }
                button {
                    class: "p-2 hover:bg-muted rounded-full transition",
                    title: "Zap this artist",
                    onclick: {
                        let zap_track = track.clone();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            music_player::show_zap_dialog_for_track(Some(zap_track.clone()));
                        }
                    },
                    dangerous_inner_html: icons::ZAP,
                }
                button {
                    class: "p-2 hover:bg-muted rounded-full transition",
                    title: "Share this track",
                    onclick: move |e: Event<MouseData>| {
                        e.stop_propagation();
                        show_share_modal.set(true);
                    },
                    dangerous_inner_html: icons::SHARE,
                }
            }
            if *show_share_modal.read() {
                ContentShareModal {
                    title: format!("{} - {}", track.title, track.artist),
                    url: share_url.clone(),
                    content_type: share_content_type,
                    image_url: track.album_art_url.clone(),
                    on_close: move |_| show_share_modal.set(false),
                }
            }
        }
    }
}
/// Skeleton loader for unified track card
#[component]
pub fn UnifiedTrackCardSkeleton() -> Element {
    rsx! {
        div { class: "flex items-center gap-3 p-3 rounded-lg animate-pulse",
            div { class: "w-14 h-14 bg-muted rounded shrink-0" }
            div { class: "flex-1 min-w-0 space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
            }
            div { class: "w-16 h-4 bg-muted rounded shrink-0" }
            div { class: "w-12 h-3 bg-muted rounded shrink-0" }
        }
    }
}
