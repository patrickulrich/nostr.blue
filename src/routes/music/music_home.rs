use dioxus::prelude::*;
use crate::services::wavlake::{WavlakeAPI, WavlakeTrack};
use crate::components::{TrackCard, TrackCardSkeleton};

#[component]
pub fn MusicHome() -> Element {
    let navigator = navigator();
    let mut search_query = use_signal(|| String::new());
    let mut tracks = use_signal(|| Vec::<WavlakeTrack>::new());
    let mut loading = use_signal(|| true);
    let mut selected_genre = use_signal(|| String::from("all"));
    let mut selected_days = use_signal(|| 7u32);

    let genres = vec![
        "all", "Rock", "Pop", "Hip-Hop", "Electronic", "Folk", "Jazz",
        "Classical", "Blues", "Country", "Reggae", "Punk", "Metal"
    ];

    let time_periods = vec![
        (1, "24h"),
        (7, "7d"),
        (30, "30d"),
        (90, "90d"),
    ];

    // Load trending tracks when genre or time period changes
    use_effect(move || {
        let genre = selected_genre.read().clone();
        let days = *selected_days.read();

        loading.set(true);
        spawn(async move {
            log::info!("Loading tracks: genre={}, days={}", genre, days);
            let api = WavlakeAPI::new();
            let genre_filter = if genre == "all" { None } else { Some(genre.as_str()) };

            match api.get_rankings("sats", Some(days), None, None, genre_filter, Some(50)).await {
                Ok(results) => {
                    log::info!("Successfully loaded {} tracks", results.len());
                    tracks.set(results);
                    loading.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load tracks: {}", e);
                    loading.set(false);
                }
            }
        });
    });

    // Search handler - navigates to dedicated search page
    let handle_search = move |_| {
        let query = search_query.read().trim().to_string();
        if !query.is_empty() {
            // URL-encode the query to handle special characters like &, #, ?
            let encoded_query = urlencoding::encode(&query).to_string();
            navigator.push(crate::routes::Route::MusicSearch { q: encoded_query });
        }
    };

    rsx! {
        div {
            class: "max-w-4xl mx-auto p-4 space-y-6",

            // Header
            div {
                class: "flex items-center justify-between",
                h1 {
                    class: "text-3xl font-bold",
                    "🎵 Music"
                }

                div {
                    class: "flex gap-2",
                    Link {
                        to: crate::routes::Route::MusicRadio {},
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-full hover:bg-primary/90 transition",
                        "📻 Radio"
                    }
                    Link {
                        to: crate::routes::Route::MusicLeaderboard {},
                        class: "px-4 py-2 bg-accent text-accent-foreground rounded-full hover:bg-accent/90 transition",
                        "🏆 Leaderboard"
                    }
                }
            }

            // Search Bar
            div {
                class: "relative",
                input {
                    r#type: "text",
                    placeholder: "Search for tracks, artists, or albums...",
                    class: "w-full px-4 py-3 pr-12 border border-border rounded-full focus:outline-none focus:ring-2 focus:ring-primary bg-background",
                    value: "{search_query}",
                    oninput: move |evt| search_query.set(evt.value()),
                    onkeydown: move |evt| {
                        if evt.key() == Key::Enter {
                            handle_search(());
                        }
                    }
                }
                button {
                    class: "absolute right-2 top-1/2 -translate-y-1/2 p-2 hover:bg-muted rounded-full transition",
                    onclick: move |_| handle_search(()),
                    "🔍"
                }
            }

            // Filters
            div {
                class: "space-y-4",

                // Genre filter
                div {
                    div {
                        class: "text-sm font-medium mb-2",
                        "Genre"
                    }
                    div {
                        class: "flex flex-wrap gap-2",
                        for genre in genres.iter() {
                            {
                                let is_selected = *selected_genre.read() == *genre;
                                let genre_val = genre.to_string();
                                rsx! {
                                    button {
                                        key: "{genre}",
                                        class: if is_selected {
                                            "px-3 py-1 rounded-full text-sm transition bg-primary text-primary-foreground"
                                        } else {
                                            "px-3 py-1 rounded-full text-sm transition bg-muted hover:bg-muted/80"
                                        },
                                        onclick: move |_| selected_genre.set(genre_val.clone()),
                                        "{genre}"
                                    }
                                }
                            }
                        }
                    }
                }

                // Time period filter
                div {
                    div {
                        class: "text-sm font-medium mb-2",
                        "Time Period"
                    }
                    div {
                        class: "flex flex-wrap gap-2",
                        for (days, label) in time_periods.iter() {
                            {
                                let is_selected = *selected_days.read() == *days;
                                let days_val = *days;
                                rsx! {
                                    button {
                                        key: "{days}",
                                        class: if is_selected {
                                            "px-3 py-1 rounded-full text-sm transition bg-primary text-primary-foreground"
                                        } else {
                                            "px-3 py-1 rounded-full text-sm transition bg-muted hover:bg-muted/80"
                                        },
                                        onclick: move |_| selected_days.set(days_val),
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Track List
            div {
                class: "space-y-1",

                if *loading.read() {
                    for _ in 0..10 {
                        TrackCardSkeleton {}
                    }
                } else if tracks.read().is_empty() {
                    div {
                        class: "text-center py-12 text-muted-foreground",
                        p { "No tracks found" }
                        p {
                            class: "text-sm mt-2",
                            "Try a different search or filter"
                        }
                    }
                } else {
                    for track in tracks.read().iter() {
                        TrackCard {
                            key: "{track.id}",
                            track: track.clone(),
                            show_album: true
                        }
                    }
                }
            }
        }
    }
}
