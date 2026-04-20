use crate::components::icons;
use crate::routes::Route;
use crate::stores::music_player::{self, MusicTrack, MUSIC_PLAYER};
use crate::stores::nostr_client;
use crate::utils::radio::{
    fetch_station_by_naddr, get_ranked_stream_urls, RadioStation as RadioStationData,
};
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
#[component]
pub fn RadioStation(naddr: String) -> Element {
    let mut station = use_signal(|| None::<RadioStationData>);
    let mut is_loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut selected_stream_idx = use_signal(|| 0usize);
    let naddr_clone = naddr.clone();
    use_effect(use_reactive(&naddr_clone, move |naddr_val| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            is_loading.set(true);
            error.set(None);
            match fetch_station_by_naddr(&naddr_val).await {
                Ok(fetched_station) => {
                    station.set(Some(fetched_station));
                }
                Err(e) => {
                    log::error!("Failed to fetch radio station: {}", e);
                    error.set(Some(e));
                }
            }
            is_loading.set(false);
        });
    }));
    let station_id = station
        .read()
        .as_ref()
        .map(|s| s.coordinate.clone())
        .unwrap_or_default();
    let station_id_for_memo = station_id.clone();
    let is_playing = use_memo(move || {
        let player_state = MUSIC_PLAYER.read();
        if let Some(ref current) = player_state.current_track {
            current.id == station_id_for_memo && player_state.is_playing
        } else {
            false
        }
    });
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-30 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-3",
                    Link {
                        to: Route::RadioHome {},
                        class: "p-2 hover:bg-muted rounded-lg transition",
                        dangerous_inner_html: icons::ARROW_LEFT,
                    }
                    h1 { class: "text-lg font-bold", "Radio Station" }
                }
            }
            div { class: "p-4 max-w-2xl mx-auto",
                if *is_loading.read() {
                    div { class: "animate-pulse space-y-6",
                        div { class: "aspect-square max-w-sm mx-auto bg-muted rounded-xl" }
                        div { class: "space-y-3",
                            div { class: "h-8 bg-muted rounded w-3/4 mx-auto" }
                            div { class: "h-4 bg-muted rounded w-1/2 mx-auto" }
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "flex flex-col items-center justify-center py-12 text-center",
                        div { class: "w-16 h-16 rounded-full bg-destructive/10 flex items-center justify-center mb-4",
                            span { class: "text-destructive text-2xl", "!" }
                        }
                        p { class: "text-lg font-medium text-destructive", "Failed to load station" }
                        p { class: "text-sm text-muted-foreground mt-1", "{err}" }
                        Link {
                            to: Route::RadioHome {},
                            class: "mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Back to Stations"
                        }
                    }
                } else if let Some(s) = station.read().as_ref() {
                    div { class: "space-y-6",
                        div { class: "relative aspect-square max-w-sm mx-auto",
                            img {
                                src: s.thumbnail
                                    .clone()
                                    .unwrap_or_else(|| {
                                        "https://api.dicebear.com/7.x/shapes/svg?seed=radio".to_string()
                                    }),
                                alt: "{s.name}",
                                class: "w-full h-full object-cover rounded-xl shadow-lg",
                            }
                            if *is_playing.read() {
                                div { class: "absolute top-4 left-4 inline-flex items-center gap-1.5 px-2 py-1 rounded-full bg-red-500 text-white text-sm font-bold",
                                    span { class: "w-2 h-2 rounded-full bg-white animate-pulse" }
                                    "LIVE"
                                }
                            }
                            button {
                                class: "absolute inset-0 flex items-center justify-center bg-black/30 hover:bg-black/50 transition rounded-xl group",
                                onclick: {
                                    let play_station = s.clone();
                                    let stream_idx = *selected_stream_idx.read();
                                    move |_| {
                                        let player_state = MUSIC_PLAYER.read();
                                        if let Some(ref current) = player_state.current_track {
                                            if current.id == play_station.coordinate && player_state.is_playing {
                                                drop(player_state);
                                                music_player::toggle_play();
                                                return;
                                            }
                                        }
                                        drop(player_state);
                                        let ranked_streams = get_ranked_stream_urls(&play_station.streams);
                                        music_player::set_available_streams(ranked_streams);
                                        let mut music_track: MusicTrack = play_station.clone().into();
                                        if let Some(stream) = play_station.streams.get(stream_idx) {
                                            music_track.media_url = stream.url.clone();
                                        }
                                        music_player::play_track(music_track, None, None);
                                    }
                                },
                                div {
                                    class: "w-20 h-20 rounded-full bg-primary flex items-center justify-center text-primary-foreground shadow-lg group-hover:scale-110 transition",
                                    dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY },
                                }
                            }
                        }
                        div { class: "text-center space-y-2",
                            h1 { class: "text-2xl font-bold", "{s.name}" }
                            if let Some(location) = s.location.as_ref().or(s.country_code.as_ref()) {
                                p { class: "text-muted-foreground", "{location}" }
                            }
                            if !s.genres.is_empty() {
                                div { class: "flex flex-wrap justify-center gap-2 mt-3",
                                    for genre in s.genres.iter() {
                                        span { class: "px-3 py-1 bg-muted rounded-full text-sm",
                                            "{genre}"
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(desc) = s.description.as_ref() {
                            div { class: "bg-muted/50 rounded-lg p-4",
                                p { class: "text-sm text-muted-foreground", "{desc}" }
                            }
                        }
                        if s.streams.len() > 1 {
                            div { class: "space-y-2",
                                h3 { class: "text-sm font-medium text-muted-foreground",
                                    "Stream Quality"
                                }
                                div { class: "grid gap-2",
                                    for (idx , stream) in s.streams.iter().enumerate() {
                                        button {
                                            key: "{idx}",
                                            class: if *selected_stream_idx.read() == idx { "flex items-center justify-between p-3 rounded-lg border-2 border-primary bg-primary/10" } else { "flex items-center justify-between p-3 rounded-lg border border-border hover:border-primary/50 transition" },
                                            onclick: move |_| selected_stream_idx.set(idx),
                                            div { class: "flex items-center gap-3",
                                                span { class: "font-medium",
                                                    "{stream.format.display_name()}"
                                                }
                                                if let Some(bitrate) = stream.bitrate() {
                                                    span { class: "text-sm text-muted-foreground",
                                                        "{bitrate} kbps"
                                                    }
                                                }
                                            }
                                            if stream.is_primary {
                                                span { class: "text-xs px-2 py-0.5 bg-primary/20 text-primary rounded-full",
                                                    "Primary"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(website) = s.website.as_ref().filter(|w| is_valid_http_url(w)) {
                            a {
                                href: "{website}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                class: "flex items-center justify-center gap-2 w-full p-3 bg-muted rounded-lg hover:bg-muted/80 transition",
                                span {
                                    class: "w-4 h-4",
                                    dangerous_inner_html: icons::EXTERNAL_LINK,
                                }
                                "Visit Website"
                            }
                        }
                        div { class: "pt-4",
                            button {
                                class: "flex items-center justify-center gap-2 w-full p-3 bg-amber-500/10 text-amber-500 rounded-lg hover:bg-amber-500/20 transition font-medium",
                                onclick: {
                                    let zap_station = s.clone();
                                    move |_| {
                                        let music_track: MusicTrack = zap_station.clone().into();
                                        music_player::show_zap_dialog_for_track(Some(music_track));
                                    }
                                },
                                span {
                                    class: "w-5 h-5",
                                    dangerous_inner_html: icons::ZAP,
                                }
                                "Zap this Station"
                            }
                        }
                    }
                }
            }
        }
    }
}
