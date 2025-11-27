use dioxus::prelude::*;
use crate::services::wavlake::{WavlakeAPI, WavlakeSearchResult, WavlakeTrack, WavlakePlaylist};
use crate::components::{TrackCard, TrackCardSkeleton, ArtistCard, ArtistCardSkeleton, AlbumCard, AlbumCardSkeleton};
use crate::components::icons::ArrowLeftIcon;
use crate::stores::music_player::{self, MusicTrack};

#[derive(Clone, Copy, PartialEq, Debug)]
enum MusicSearchTab {
    Tracks,
    Artists,
    Albums,
    Playlists,
}

impl MusicSearchTab {
    fn label(&self) -> &'static str {
        match self {
            MusicSearchTab::Tracks => "Tracks",
            MusicSearchTab::Artists => "Artists",
            MusicSearchTab::Albums => "Albums",
            MusicSearchTab::Playlists => "Playlists",
        }
    }
}

#[component]
pub fn MusicSearch(q: String) -> Element {
    let navigator = navigator();

    // State
    let mut active_tab = use_signal(|| MusicSearchTab::Tracks);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Search results categorized
    let mut track_results = use_signal(|| Vec::<WavlakeTrack>::new());
    let mut artist_results = use_signal(|| Vec::<WavlakeSearchResult>::new());
    let mut album_results = use_signal(|| Vec::<WavlakeSearchResult>::new());

    // Playlist state
    let mut playlist_id_input = use_signal(|| String::new());
    let mut playlist = use_signal(|| None::<WavlakePlaylist>);
    let mut playlist_loading = use_signal(|| false);
    let mut playlist_error = use_signal(|| None::<String>);

    // Search effect - runs when prop changes using use_reactive!
    use_effect(use_reactive!(|q| {
        let search_query = q.clone();

        if search_query.is_empty() {
            track_results.set(Vec::new());
            artist_results.set(Vec::new());
            album_results.set(Vec::new());
            loading.set(false);
            return;
        }

        loading.set(true);
        error.set(None);

        spawn(async move {
            log::info!("Music search for: {}", search_query);
            let api = WavlakeAPI::new();

            match api.search_content(&search_query).await {
                Ok(results) => {
                    // Categorize results by type
                    let mut tracks = Vec::new();
                    let mut artists = Vec::new();
                    let mut albums = Vec::new();

                    for result in results {
                        match result.result_type.as_str() {
                            "track" => tracks.push(result),
                            "artist" => artists.push(result.clone()),
                            "album" => albums.push(result.clone()),
                            _ => {}
                        }
                    }

                    log::info!("Found {} tracks, {} artists, {} albums", tracks.len(), artists.len(), albums.len());

                    // Fetch full track details for playability (in parallel)
                    let track_futures: Vec<_> = tracks.into_iter().map(|track_result| {
                        async move {
                            let api = WavlakeAPI::new();
                            match api.get_track(&track_result.id).await {
                                Ok(track) => Some(track),
                                Err(e) => {
                                    log::warn!("Failed to fetch track {}: {}", track_result.id, e);
                                    None
                                }
                            }
                        }
                    }).collect();

                    let full_tracks: Vec<_> = futures::future::join_all(track_futures)
                        .await
                        .into_iter()
                        .flatten()
                        .collect();

                    track_results.set(full_tracks);
                    artist_results.set(artists);
                    album_results.set(albums);
                    loading.set(false);
                }
                Err(e) => {
                    log::error!("Search failed: {}", e);
                    error.set(Some(format!("Search failed: {}", e)));
                    loading.set(false);
                }
            }
        });
    }));

    let tabs = [
        MusicSearchTab::Tracks,
        MusicSearchTab::Artists,
        MusicSearchTab::Albums,
        MusicSearchTab::Playlists,
    ];

    // Count badges for tabs
    let track_count = track_results.read().len();
    let artist_count = artist_results.read().len();
    let album_count = album_results.read().len();

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3",

                    // Back button and title
                    div {
                        class: "flex items-center gap-3 mb-2",
                        button {
                            class: "p-2 hover:bg-muted rounded-full transition",
                            onclick: move |_| { navigator.push(crate::routes::Route::MusicHome {}); },
                            ArrowLeftIcon { class: "w-5 h-5".to_string() }
                        }
                        h2 {
                            class: "text-xl font-bold",
                            "Music Search"
                        }
                    }

                    p {
                        class: "text-sm text-muted-foreground ml-11",
                        "Results for: \"{q}\""
                    }
                }

                // Tabs
                div {
                    class: "flex border-b border-border overflow-x-auto scrollbar-hide",
                    for tab in tabs.iter() {
                        {
                            let tab_value = *tab;
                            let is_active = *active_tab.read() == tab_value;
                            let count = match tab_value {
                                MusicSearchTab::Tracks => Some(track_count),
                                MusicSearchTab::Artists => Some(artist_count),
                                MusicSearchTab::Albums => Some(album_count),
                                MusicSearchTab::Playlists => None,
                            };

                            rsx! {
                                button {
                                    key: "{tab.label()}",
                                    class: if is_active {
                                        "px-6 py-3 text-sm font-medium border-b-2 border-primary text-primary transition flex items-center gap-2"
                                    } else {
                                        "px-6 py-3 text-sm font-medium border-b-2 border-transparent text-muted-foreground hover:text-foreground hover:border-border transition flex items-center gap-2"
                                    },
                                    onclick: move |_| active_tab.set(tab_value),
                                    "{tab.label()}"
                                    if let Some(c) = count {
                                        if c > 0 {
                                            span {
                                                class: "text-xs bg-muted px-2 py-0.5 rounded-full",
                                                "{c}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Error state
            if let Some(ref err) = *error.read() {
                div {
                    class: "p-4",
                    div {
                        class: "p-4 bg-destructive/10 text-destructive rounded-lg",
                        "{err}"
                    }
                }
            }

            // Tab content
            div {
                class: "p-4",
                match *active_tab.read() {
                    MusicSearchTab::Tracks => rsx! {
                        div {
                            class: "space-y-1",
                            if *loading.read() {
                                for _ in 0..8 {
                                    TrackCardSkeleton {}
                                }
                            } else if track_results.read().is_empty() {
                                div {
                                    class: "text-center py-12 text-muted-foreground",
                                    p { "No tracks found" }
                                    p {
                                        class: "text-sm mt-2",
                                        "Try a different search term"
                                    }
                                }
                            } else {
                                for track in track_results.read().iter() {
                                    TrackCard {
                                        key: "{track.id}",
                                        track: track.clone(),
                                        show_album: true
                                    }
                                }
                            }
                        }
                    },
                    MusicSearchTab::Artists => rsx! {
                        div {
                            class: "space-y-1",
                            if *loading.read() {
                                for _ in 0..8 {
                                    ArtistCardSkeleton {}
                                }
                            } else if artist_results.read().is_empty() {
                                div {
                                    class: "text-center py-12 text-muted-foreground",
                                    p { "No artists found" }
                                    p {
                                        class: "text-sm mt-2",
                                        "Try a different search term"
                                    }
                                }
                            } else {
                                for artist in artist_results.read().iter() {
                                    ArtistCard {
                                        key: "{artist.id}",
                                        result: artist.clone()
                                    }
                                }
                            }
                        }
                    },
                    MusicSearchTab::Albums => rsx! {
                        div {
                            class: "space-y-1",
                            if *loading.read() {
                                for _ in 0..8 {
                                    AlbumCardSkeleton {}
                                }
                            } else if album_results.read().is_empty() {
                                div {
                                    class: "text-center py-12 text-muted-foreground",
                                    p { "No albums found" }
                                    p {
                                        class: "text-sm mt-2",
                                        "Try a different search term"
                                    }
                                }
                            } else {
                                for album in album_results.read().iter() {
                                    AlbumCard {
                                        key: "{album.id}",
                                        result: album.clone()
                                    }
                                }
                            }
                        }
                    },
                    MusicSearchTab::Playlists => rsx! {
                        div {
                            class: "space-y-4",

                            // Info banner
                            div {
                                class: "p-4 bg-muted/50 rounded-lg border border-border",
                                p {
                                    class: "text-sm text-muted-foreground mb-3",
                                    "Playlist search is not available via the API. Enter a Wavlake playlist ID directly to load it:"
                                }

                                // Input row
                                {
                                    // Closure to load playlist by ID
                                    let mut load_playlist = move || {
                                        let id = playlist_id_input.read().clone();
                                        if !id.is_empty() && !*playlist_loading.read() {
                                            playlist_loading.set(true);
                                            playlist_error.set(None);
                                            spawn(async move {
                                                let api = WavlakeAPI::new();
                                                match api.get_playlist(&id).await {
                                                    Ok(p) => {
                                                        playlist.set(Some(p));
                                                        playlist_loading.set(false);
                                                    }
                                                    Err(e) => {
                                                        playlist_error.set(Some(format!("Failed to load playlist: {}", e)));
                                                        playlist_loading.set(false);
                                                    }
                                                }
                                            });
                                        }
                                    };

                                    rsx! {
                                        div {
                                            class: "flex gap-2",
                                            input {
                                                r#type: "text",
                                                placeholder: "Enter playlist ID...",
                                                class: "flex-1 px-4 py-2 border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary bg-background",
                                                value: "{playlist_id_input}",
                                                oninput: move |evt| playlist_id_input.set(evt.value()),
                                                onkeydown: move |evt| {
                                                    if evt.key() == Key::Enter {
                                                        load_playlist();
                                                    }
                                                }
                                            }
                                            button {
                                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50",
                                                disabled: playlist_id_input.read().is_empty() || *playlist_loading.read(),
                                                onclick: move |_| load_playlist(),
                                                if *playlist_loading.read() { "Loading..." } else { "Load Playlist" }
                                            }
                                        }
                                    }
                                }
                            }

                            // Playlist error
                            if let Some(ref err) = *playlist_error.read() {
                                div {
                                    class: "p-4 bg-destructive/10 text-destructive rounded-lg",
                                    "{err}"
                                }
                            }

                            // Playlist results
                            if let Some(ref pl) = *playlist.read() {
                                div {
                                    class: "space-y-4",

                                    // Playlist header
                                    div {
                                        class: "flex items-center gap-3 p-4 bg-muted/30 rounded-lg",
                                        div {
                                            class: "flex-1",
                                            h3 {
                                                class: "font-semibold text-lg",
                                                "{pl.title}"
                                            }
                                            p {
                                                class: "text-sm text-muted-foreground",
                                                "{pl.tracks.len()} tracks"
                                            }
                                        }
                                        button {
                                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                            onclick: {
                                                let playlist_tracks = pl.tracks.clone();
                                                move |_| {
                                                    if let Some(first_track) = playlist_tracks.first() {
                                                        let music_track: MusicTrack = first_track.clone().into();
                                                        let all_tracks: Vec<MusicTrack> = playlist_tracks.iter().map(|t| t.clone().into()).collect();
                                                        music_player::play_track(music_track, Some(all_tracks), Some(0));
                                                    }
                                                }
                                            },
                                            "Play All"
                                        }
                                    }

                                    // Track list
                                    div {
                                        class: "space-y-1",
                                        for track in pl.tracks.iter() {
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
                    },
                }
            }
        }
    }
}
