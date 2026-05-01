use crate::components::icons;
use crate::routes::Route;
use crate::stores::music_player::{self, MusicPlayerStateStoreExt, MusicTrack, MUSIC_PLAYER};
use crate::utils::radio::{get_ranked_stream_urls, RadioStation};
use dioxus::prelude::*;
#[derive(Props, Clone, PartialEq)]
pub struct RadioCardProps {
    pub station: RadioStation,
    /// Show compact layout (for grids)
    #[props(default = false)]
    pub compact: bool,
}
/// Radio station card component for displaying an internet radio station
#[component]
pub fn RadioCard(props: RadioCardProps) -> Element {
    let station = props.station.clone();
    let station_id = station.coordinate.clone();
    let station_id_for_memo = station_id.clone();
    let is_playing = use_memo(move || {
        let store = MUSIC_PLAYER.resolve();
        let current = store.current_track().cloned();
        if let Some(ref cur) = current {
            cur.id == station_id_for_memo && store.is_playing().cloned()
        } else {
            false
        }
    });
    let handle_play = {
        let station = station.clone();
        let station_id_clone = station_id.clone();
        move |_| {
            let player_state = MUSIC_PLAYER.read();
            if let Some(ref current) = player_state.current_track {
                if current.id == station_id_clone && player_state.is_playing {
                    drop(player_state);
                    music_player::toggle_play();
                    return;
                }
            }
            drop(player_state);
            let ranked_streams = get_ranked_stream_urls(&station.streams);
            if ranked_streams.is_empty() {
                log::warn!("Station has no available streams: {}", station.name);
                return;
            }
            music_player::set_available_streams(ranked_streams);
            let music_track: MusicTrack = station.clone().into();
            music_player::play_track(music_track, None, None);
        }
    };
    let thumbnail_url = station
        .thumbnail
        .clone()
        .unwrap_or_else(|| "https://api.dicebear.com/7.x/shapes/svg?seed=radio".to_string());
    let primary_genre = station.genres.first().cloned();
    let stream_info = station.streams.first().map(|s| {
        let format = s.format.display_name();
        if let Some(bitrate) = s.bitrate() {
            format!("{} {}kbps", format, bitrate)
        } else {
            format.to_string()
        }
    });
    let naddr = station
        .naddr
        .clone()
        .unwrap_or_else(|| station.coordinate.clone());
    if props.compact {
        rsx! {
            div { class: "group relative bg-card rounded-lg overflow-hidden border border-border hover:border-primary/50 transition",
                div { class: "relative aspect-square",
                    img {
                        src: "{thumbnail_url}",
                        alt: "{station.name}",
                        class: "w-full h-full object-cover",
                        loading: "lazy",
                    }
                    if *is_playing.read() {
                        div { class: "absolute top-2 left-2 inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-bold uppercase bg-red-500/90 text-white",
                            span { class: "w-1.5 h-1.5 rounded-full bg-white animate-pulse" }
                            "LIVE"
                        }
                    }
                    button {
                        class: "absolute inset-0 z-10 flex items-center justify-center bg-black/40 opacity-0 group-hover:opacity-100 transition",
                        onclick: handle_play.clone(),
                        div {
                            class: "w-12 h-12 rounded-full bg-primary flex items-center justify-center text-primary-foreground",
                            dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY },
                        }
                    }
                }
                div { class: "p-3",
                    h3 { class: "font-medium text-sm truncate",
                        if *is_playing.read() {
                            span { class: "text-primary", "{station.name}" }
                        } else {
                            "{station.name}"
                        }
                    }
                    div { class: "flex items-center gap-2 mt-1 flex-wrap",
                        if let Some(location) = station.display_location() {
                            span { class: "inline-flex items-center gap-0.5 text-[10px] text-muted-foreground",
                                span {
                                    class: "w-3 h-3 opacity-60",
                                    dangerous_inner_html: icons::MAP_PIN,
                                }
                                "{location}"
                            }
                        }
                        if let Some(genre) = &primary_genre {
                            span { class: "inline-block px-2 py-0.5 text-[10px] bg-muted rounded-full text-muted-foreground",
                                "{genre}"
                            }
                        }
                    }
                    if let Some(info) = &stream_info {
                        div { class: "text-[10px] text-muted-foreground mt-1", "{info}" }
                    }
                }
                Link {
                    to: Route::RadioStation {
                        naddr: naddr.clone(),
                    },
                    class: "absolute inset-0 z-0",
                }
            }
        }
    } else {
        rsx! {
            div { class: "flex items-center gap-3 p-3 hover:bg-muted/50 rounded-lg transition group",
                div { class: "relative shrink-0",
                    img {
                        src: "{thumbnail_url}",
                        alt: "{station.name}",
                        class: "w-14 h-14 rounded object-cover",
                        loading: "lazy",
                    }
                    if *is_playing.read() {
                        div { class: "absolute -top-1 -right-1 inline-flex items-center gap-0.5 px-1 py-0.5 rounded text-[8px] font-bold uppercase bg-red-500 text-white",
                            span { class: "w-1 h-1 rounded-full bg-white animate-pulse" }
                            "LIVE"
                        }
                    }
                    button {
                        class: "absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 group-hover:opacity-100 transition rounded",
                        onclick: handle_play.clone(),
                        dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY },
                    }
                }
                div { class: "flex-1 min-w-0",
                    Link {
                        to: Route::RadioStation {
                            naddr: naddr.clone(),
                        },
                        class: "block",
                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        div { class: "font-medium text-sm truncate hover:underline",
                            if *is_playing.read() {
                                span { class: "text-primary", "{station.name}" }
                            } else {
                                "{station.name}"
                            }
                        }
                    }
                    div { class: "text-xs text-muted-foreground truncate",
                        if let Some(location) = station.location.as_ref().or(station.country_code.as_ref()) {
                            span { "{location}" }
                            if primary_genre.is_some() {
                                span { " · " }
                            }
                        }
                        if let Some(genre) = &primary_genre {
                            span { "{genre}" }
                        }
                    }
                    if let Some(info) = &stream_info {
                        div { class: "text-xs text-muted-foreground", "{info}" }
                    }
                }
                div { class: "flex items-center gap-1 shrink-0 opacity-0 group-hover:opacity-100 transition",
                    button {
                        class: "p-2 hover:bg-muted rounded-full transition",
                        title: "Zap this station",
                        onclick: {
                            let zap_station = station.clone();
                            move |e: Event<MouseData>| {
                                e.stop_propagation();
                                let music_track: MusicTrack = zap_station.clone().into();
                                music_player::show_zap_dialog_for_track(Some(music_track));
                            }
                        },
                        dangerous_inner_html: icons::ZAP,
                    }
                }
            }
        }
    }
}
/// Skeleton loader for radio card (compact grid layout)
#[component]
pub fn RadioCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card rounded-lg overflow-hidden border border-border animate-pulse",
            div { class: "aspect-square bg-muted" }
            div { class: "p-3 space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
            }
        }
    }
}
/// Skeleton loader for radio card (list layout)
#[component]
pub fn RadioCardListSkeleton() -> Element {
    rsx! {
        div { class: "flex items-center gap-3 p-3 rounded-lg animate-pulse",
            div { class: "w-14 h-14 bg-muted rounded shrink-0" }
            div { class: "flex-1 min-w-0 space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
            }
        }
    }
}
