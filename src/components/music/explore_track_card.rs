use crate::components::icons;
use super::FALLBACK_ART_URL;
use crate::routes::Route;
use crate::stores::music_library;
use crate::stores::music_player::{self, MusicPlayerStateStoreExt, MusicTrack};
use crate::stores::nostr_client;
use crate::stores::nostr_music::TrackSource;
use crate::stores::profiles;
use dioxus::prelude::*;
use std::sync::Arc;

#[derive(Props, Clone, PartialEq)]
pub struct ExploreTrackCardProps {
    pub track: MusicTrack,
    /// Optional playlist to enable continuous playback.
    #[props(default)]
    pub playlist: Option<Arc<Vec<MusicTrack>>>,
}
/// Vertical 1:1 cover card for the Explore "Songs" row. Mirrors
/// `UnifiedTrackCard`'s multi-source handling (artist resolution, play, zap,
/// vote) but uses a nostria/zaptrax-style vertical layout.
#[component]
pub fn ExploreTrackCard(props: ExploreTrackCardProps) -> Element {
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
    // Music Library save state (+ button).
    let library_key = music_library::track_key(&track);
    let library_key_memo = library_key.clone();
    let is_saved = use_memo(move || music_library::is_saved(&library_key_memo));
    let toggle_library = {
        let track = track.clone();
        let key = library_key.clone();
        move |e: Event<MouseData>| {
            e.stop_propagation();
            if !nostr_client::has_signer() {
                return;
            }
            let track = track.clone();
            let key = key.clone();
            let saved = music_library::is_saved(&key);
            spawn(async move {
                let _ = if saved {
                    music_library::remove_track(&key).await
                } else {
                    music_library::add_track(&track).await
                };
            });
        }
    };
    let source_info = match &track.source {
        TrackSource::Wavlake { .. } => ("W", "Wavlake", "bg-orange-500/20 text-orange-400"),
        TrackSource::Nostr { .. } => ("N", "Nostr", "bg-purple-500/20 text-purple-400"),
        TrackSource::NostrPodcast { .. } => ("P", "Nostr Podcast", "bg-green-500/20 text-green-400"),
        TrackSource::RssPodcast { .. } => ("R", "RSS Podcast", "bg-green-500/20 text-green-400"),
        TrackSource::RssMusic { .. } => ("RSS", "Podcasting 2.0", "bg-orange-500/20 text-orange-400"),
        TrackSource::Radio { .. } => ("LIVE", "Internet Radio", "bg-red-500/20 text-red-400"),
        TrackSource::Bible { .. } => ("B", "Bible", "bg-blue-500/20 text-blue-400"),
        TrackSource::Quran { .. } => ("Q", "Quran", "bg-green-500/20 text-green-400"),
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
    let artist_route = match &track.source {
        TrackSource::Wavlake { artist_id, .. } => Some(Route::MusicArtist {
            artist_id: artist_id.clone(),
        }),
        TrackSource::Nostr { pubkey, .. } => Some(Route::MusicArtist {
            artist_id: pubkey.clone(),
        }),
        TrackSource::NostrPodcast { pubkey, .. } => Some(Route::AddressViewer {
            address: crate::utils::nip19_urls::profile_route_id(pubkey),
        }),
        TrackSource::RssPodcast { podcast_id, .. } => {
            podcast_id.map(|id| Route::PodcastRssFeedDetail { podcast_id: id.to_string() })
        }
        TrackSource::RssMusic { artist, .. } => {
            artist.as_ref().map(|a| Route::MusicRssArtist { artist: a.clone() })
        }
        TrackSource::Radio { pubkey, .. } => Some(Route::AddressViewer {
            address: crate::utils::nip19_urls::profile_route_id(pubkey),
        }),
        TrackSource::Bible { .. } | TrackSource::Quran { .. } => None,
    };
    rsx! {
        div { class: "group cursor-pointer",
            div {
                class: "relative aspect-square rounded-lg overflow-hidden bg-muted",
                onclick: handle_play,
                img {
                    src: "{img_src}",
                    alt: "Album art",
                    class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-300",
                    loading: "lazy",
                    referrerpolicy: "no-referrer",
                    onerror: move |_| img_src.set(FALLBACK_ART_URL.to_string()),
                }
                div {
                    class: "absolute top-2 right-2 px-1.5 py-0.5 rounded-full text-[10px] font-bold {source_info.2}",
                    title: "{source_info.1}",
                    "{source_info.0}"
                }
                div {
                    class: "absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition flex items-center justify-center",
                    div { class: "w-12 h-12 bg-primary rounded-full flex items-center justify-center shadow-lg",
                        dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY },
                    }
                }
                div {
                    class: "absolute bottom-2 left-1/2 -translate-x-1/2 flex items-center gap-1 opacity-0 group-hover:opacity-100 transition",
                    button {
                        class: "p-1.5 bg-black/60 backdrop-blur-sm rounded-full text-white hover:bg-black/80 transition",
                        title: if *is_saved.read() { "Remove from Library" } else { "Save to Library" },
                        onclick: toggle_library,
                        dangerous_inner_html: if *is_saved.read() { icons::CHECK } else { icons::PLUS },
                    }
                    button {
                        class: "p-1.5 bg-black/60 backdrop-blur-sm rounded-full text-white hover:bg-black/80 transition",
                        title: "Zap this track",
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
                        class: "p-1.5 bg-black/60 backdrop-blur-sm rounded-full text-white hover:bg-black/80 transition",
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
                }
            }
            div { class: "mt-2",
                div {
                    class: "font-medium text-sm truncate group-hover:text-primary transition",
                    "{track.title}"
                }
                div { class: "text-xs text-muted-foreground truncate",
                    if let Some(route) = artist_route.clone() {
                        Link {
                            to: route,
                            class: "hover:text-foreground hover:underline",
                            "{artist_name}"
                        }
                    } else {
                        span { "{artist_name}" }
                    }
                }
            }
        }
    }
}
#[component]
pub fn ExploreTrackCardSkeleton() -> Element {
    rsx! {
        div { class: "animate-pulse",
            div { class: "aspect-square rounded-lg bg-muted" }
            div { class: "mt-2 space-y-1",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
            }
        }
    }
}
