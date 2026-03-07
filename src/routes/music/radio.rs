use crate::routes::Route;
use crate::services::wavlake::WavlakeAPI;
use crate::stores::music_player::{self, MusicTrack};
use dioxus::prelude::*;

#[cfg(feature = "web")]
use js_sys;
#[cfg(not(feature = "web"))]
use rand::{thread_rng, Rng};

#[component]
pub fn MusicRadio() -> Element {
    let mut selected_genre = use_signal(|| String::from("all"));
    let mut selected_days = use_signal(|| 7u32);
    let mut loading = use_signal(|| false);
    let mut radio_started = use_signal(|| false);
    let genres = [
        "all",
        "Rock",
        "Pop",
        "Hip-Hop",
        "Electronic",
        "Folk",
        "Jazz",
        "Classical",
        "Blues",
        "Country",
        "Reggae",
        "Punk",
        "Metal",
        "R&B",
        "Alternative",
        "Indie",
        "Ambient",
    ];
    let time_periods = [
        (1, "24 hours"),
        (7, "7 days"),
        (30, "30 days"),
        (90, "90 days"),
    ];
    let start_radio = move |_| {
        let genre = selected_genre.read().clone();
        let days = *selected_days.read();
        loading.set(true);
        spawn(async move {
            log::info!("Starting radio: genre={}, days={}", genre, days);
            let api = WavlakeAPI::new();
            let genre_filter = if genre == "all" { None } else { Some(genre.as_str()) };
            match api
                .get_rankings("sats", Some(days), None, None, genre_filter, Some(100))
                .await
            {
                Ok(tracks) => {
                    if !tracks.is_empty() {
                        log::info!("Loaded {} tracks for radio", tracks.len());
                        let mut music_tracks: Vec<MusicTrack> = tracks
                            .into_iter()
                            .map(|t| t.into())
                            .collect();
                        #[cfg(feature = "web")]
                        let seed = (js_sys::Date::now() as u64) as usize;
                        #[cfg(not(feature = "web"))]
                        let seed = match std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                        {
                            Ok(d) => d.as_millis() as usize,
                            Err(e) => {
                                log::warn!(
                                    "Failed to get system time for shuffle seed: {}, using random fallback",
                                    e
                                );
                                thread_rng().gen::<usize>()
                            }
                        };
                        for i in (1..music_tracks.len()).rev() {
                            let j = seed.wrapping_add(i) % (i + 1);
                            music_tracks.swap(i, j);
                        }
                        if let Some(first_track) = music_tracks.first().cloned() {
                            music_player::play_track(
                                first_track,
                                Some(music_tracks),
                                Some(0),
                            );
                            radio_started.set(true);
                            loading.set(false);
                        }
                    } else {
                        log::error!("No tracks found for radio");
                        loading.set(false);
                    }
                }
                Err(e) => {
                    log::error!("Failed to load radio tracks: {}", e);
                    loading.set(false);
                }
            }
        });
    };
    rsx! {
        div { class: "max-w-2xl mx-auto p-4 space-y-6",
            div { class: "flex items-center justify-between",
                h1 { class: "text-3xl font-bold", "Wavlake Radio" }
                Link {
                    to: Route::MusicHome {},
                    class: "px-3 py-2 bg-muted text-muted-foreground rounded-lg hover:bg-muted/80 transition text-sm font-medium",
                    "Back to Music"
                }
            }
            p { class: "text-muted-foreground", "Create your custom Bitcoin music station" }
            if !*radio_started.read() {
                div { class: "space-y-6",
                    div {
                        div { class: "text-xs font-medium text-muted-foreground mb-2 uppercase tracking-wide",
                            "Genre"
                        }
                        div { class: "flex flex-wrap gap-1.5",
                            for genre in genres.iter() {
                                {
                                    let is_selected = *selected_genre.read() == *genre;
                                    let genre_val = genre.to_string();
                                    rsx! {
                                        button {
                                            key: "{genre}",
                                            class: if is_selected { "px-3 py-1.5 rounded-full text-xs font-medium transition bg-primary text-primary-foreground" } else { "px-3 py-1.5 rounded-full text-xs font-medium transition bg-muted/50 hover:bg-muted text-muted-foreground" },
                                            onclick: move |_| selected_genre.set(genre_val.clone()),
                                            "{genre}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        div { class: "text-xs font-medium text-muted-foreground mb-2 uppercase tracking-wide",
                            "Time Period"
                        }
                        div { class: "flex flex-wrap gap-1.5",
                            for (days , label) in time_periods.iter() {
                                {
                                    let is_selected = *selected_days.read() == *days;
                                    let days_val = *days;
                                    rsx! {
                                        button {
                                            key: "{days}",
                                            class: if is_selected { "px-3 py-1.5 rounded-full text-xs font-medium transition bg-primary text-primary-foreground" } else { "px-3 py-1.5 rounded-full text-xs font-medium transition bg-muted/50 hover:bg-muted text-muted-foreground" },
                                            onclick: move |_| selected_days.set(days_val),
                                            "{label}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "pt-4",
                        button {
                            class: "w-full py-3 bg-primary text-primary-foreground font-medium rounded-lg hover:bg-primary/90 transition disabled:opacity-50 disabled:cursor-not-allowed",
                            disabled: *loading.read(),
                            onclick: start_radio,
                            if *loading.read() {
                                "Loading tracks..."
                            } else {
                                "Start Radio"
                            }
                        }
                    }
                }
            } else {
                div { class: "bg-muted/50 rounded-lg p-8 text-center space-y-4",
                    h3 { class: "text-2xl font-bold", "Radio is Playing!" }
                    p { class: "text-muted-foreground",
                        "Check the player at the bottom of the screen"
                    }
                    p { class: "text-sm text-muted-foreground/70",
                        "Tracks will auto-advance when finished"
                    }
                    button {
                        class: "mt-4 px-4 py-2 bg-muted text-muted-foreground rounded-lg hover:bg-muted/80 transition text-sm font-medium",
                        onclick: move |_| {
                            radio_started.set(false);
                            music_player::close_player();
                        },
                        "Restart Radio"
                    }
                }
            }
        }
    }
}
