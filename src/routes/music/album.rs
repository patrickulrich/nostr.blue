use dioxus::prelude::*;
use crate::components::icons::*;
use crate::services::wavlake::{get_album, WavlakeAlbum};
use crate::stores::music_player::{self, MusicTrack};

#[component]
pub fn MusicAlbum(album_id: String) -> Element {
    let mut album_state = use_signal(|| None::<WavlakeAlbum>);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);

    // Fetch album data
    use_effect(move || {
        let album_id = album_id.clone();
        spawn(async move {
            loading.set(true);
            match get_album(&album_id).await {
                Ok(album_data) => {
                    album_state.set(Some(album_data));
                    loading.set(false);
                }
                Err(e) => {
                    error_msg.set(Some(format!("Failed to load album: {}", e)));
                    loading.set(false);
                }
            }
        });
    });

    // Convert tracks to MusicTrack format
    let music_tracks = album_state().map(|album| {
        album.tracks.iter().map(|track| MusicTrack {
            id: track.id.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: Some(track.album_title.clone()),
            media_url: track.media_url.clone(),
            album_art_url: Some(track.album_art_url.clone()),
            artist_art_url: Some(track.artist_art_url.clone()),
            duration: Some(track.duration),
            artist_id: Some(track.artist_id.clone()),
            album_id: Some(track.album_id.clone()),
            artist_npub: track.artist_npub.clone(),
        }).collect::<Vec<_>>()
    });

    let play_track = move |track: MusicTrack, playlist: Vec<MusicTrack>, index: usize| {
        music_player::play_track(track, Some(playlist), Some(index));
    };

    let play_album = move |tracks: Vec<MusicTrack>| {
        if let Some(first_track) = tracks.first().cloned() {
            music_player::play_track(first_track, Some(tracks), Some(0));
        }
    };

    let format_duration = |seconds: u32| -> String {
        let mins = seconds / 60;
        let secs = seconds % 60;
        format!("{}:{:02}", mins, secs)
    };

    rsx! {
        div { class: "container mx-auto px-4 py-8",
            // Back button
            a {
                href: "/music",
                class: "inline-flex items-center gap-2 text-gray-400 hover:text-white mb-6 transition-colors",
                ArrowLeftIcon { class: "w-4 h-4" }
                "Back to Music Discovery"
            }

            // Loading state
            if loading() {
                div { class: "bg-gray-800/50 backdrop-blur-sm rounded-lg border border-gray-700 p-6",
                    div { class: "flex items-center gap-6",
                        div { class: "w-48 h-48 bg-gray-700 rounded-lg animate-pulse" }
                        div { class: "flex-1 space-y-4",
                            div { class: "h-8 bg-gray-700 rounded w-64 animate-pulse" }
                            div { class: "h-4 bg-gray-700 rounded w-48 animate-pulse" }
                            div { class: "h-4 bg-gray-700 rounded w-32 animate-pulse" }
                        }
                    }
                }
            }

            // Error state
            if let Some(err) = error_msg() {
                div { class: "bg-gray-800/50 backdrop-blur-sm rounded-lg border border-gray-700 p-8 text-center",
                    MusicIcon { class: "w-12 h-12 text-gray-400 mx-auto mb-4" }
                    h2 { class: "text-2xl font-bold mb-2", "Album Not Found" }
                    p { class: "text-gray-400", "{err}" }
                }
            }

            // Album content
            if let Some(album) = album_state() {
                if let Some(tracks) = music_tracks.clone() {
                    div { class: "space-y-6",
                        // Album Header
                        div { class: "bg-gray-800/50 backdrop-blur-sm rounded-lg border border-gray-700 p-6",
                            div { class: "flex items-start gap-6",
                                // Album art
                                div { class: "w-48 h-48 bg-gray-700 rounded-lg flex items-center justify-center overflow-hidden flex-shrink-0",
                                    if let Some(art_url) = &album.album_art_url {
                                        if !art_url.is_empty() {
                                            img {
                                                src: "{art_url}",
                                                alt: "{album.title}",
                                                class: "w-full h-full object-cover"
                                            }
                                        } else {
                                            MusicIcon { class: "w-24 h-24 text-gray-400" }
                                        }
                                    } else {
                                        MusicIcon { class: "w-24 h-24 text-gray-400" }
                                    }
                                }

                                div { class: "flex-1 space-y-4",
                                    div {
                                        span { class: "inline-block px-2 py-1 rounded border border-gray-600 text-xs text-gray-400 mb-2",
                                            "Album"
                                        }
                                        h1 { class: "text-3xl font-bold text-white", "{album.title}" }
                                        p { class: "text-xl text-gray-400 mt-2",
                                            "by "
                                            a {
                                                href: if let Some(first_track) = album.tracks.first() {
                                                    format!("/music/artist/{}", first_track.artist_id)
                                                } else {
                                                    "#".to_string()
                                                },
                                                class: "hover:text-white transition-colors underline",
                                                "{album.artist}"
                                            }
                                        }
                                    }

                                    div { class: "flex items-center gap-4 text-sm text-gray-400",
                                        span { class: "flex items-center gap-1",
                                            CalendarIcon { class: "w-3 h-3" }
                                            {
                                                album.release_date.split('T').next().unwrap_or("Unknown").split('-').next().unwrap_or("Unknown")
                                            }
                                        }
                                        span { class: "flex items-center gap-1",
                                            MusicIcon { class: "w-3 h-3" }
                                            "{album.tracks.len()} "
                                            if album.tracks.len() == 1 { "track" } else { "tracks" }
                                        }
                                        span { class: "flex items-center gap-1",
                                            ClockIcon { class: "w-3 h-3" }
                                            {
                                                let total_duration: u32 = album.tracks.iter().map(|t| t.duration).sum();
                                                let mins = total_duration / 60;
                                                format!("{} min", mins)
                                            }
                                        }
                                    }

                                    div { class: "flex items-center gap-4",
                                        button {
                                            class: "px-4 py-2 bg-purple-600 hover:bg-purple-500 rounded text-white transition-colors inline-flex items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed",
                                            disabled: album.tracks.is_empty(),
                                            onclick: move |_| {
                                                let tracks_clone = tracks.clone();
                                                play_album(tracks_clone);
                                            },
                                            PlayIcon { class: "w-4 h-4" }
                                            "Play Album"
                                        }

                                        if let Some(first_track) = tracks.first() {
                                            {
                                                let zap_track = first_track.clone();
                                                rsx! {
                                                    button {
                                                        class: "px-4 py-2 rounded border border-gray-600 hover:border-purple-500 text-sm transition-colors inline-flex items-center gap-1",
                                                        onclick: move |_| music_player::show_zap_dialog_for_track(Some(zap_track.clone())),
                                                        ZapIcon { class: "w-3 h-3" }
                                                        "Zap Artist"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Track List
                        div { class: "bg-gray-800/50 backdrop-blur-sm rounded-lg border border-gray-700 p-6",
                            div { class: "flex items-center gap-2 mb-4",
                                MusicIcon { class: "w-5 h-5 text-purple-400" }
                                h2 { class: "text-xl font-bold text-white", "Tracks" }
                            }

                            if album.tracks.is_empty() {
                                div { class: "text-center py-8",
                                    MusicIcon { class: "w-12 h-12 text-gray-400 mx-auto mb-4" }
                                    p { class: "text-gray-400", "No tracks found in this album." }
                                }
                            } else {
                                div { class: "space-y-2",
                                    for (index , track) in tracks.iter().enumerate() {
                                        {
                                            let track_clone = track.clone();
                                            let playlist_clone = tracks.clone();
                                            rsx! {
                                                div {
                                                    key: "{track.id}",
                                                    class: "flex items-center gap-4 p-3 rounded-lg transition-colors cursor-pointer group hover:bg-gray-700/50",
                                                    onclick: move |_| {
                                                        play_track(track_clone.clone(), playlist_clone.clone(), index);
                                                    },

                                                    // Track number with play icon on hover
                                                    div { class: "w-8 h-8 flex items-center justify-center text-sm font-medium text-gray-400",
                                                        span { class: "group-hover:hidden", "{index + 1}" }
                                                        div { class: "hidden group-hover:block",
                                                            PlayIcon { class: "w-4 h-4" }
                                                        }
                                                    }

                                                    // Album art thumbnail
                                                    div { class: "w-12 h-12 bg-gray-700 rounded flex items-center justify-center overflow-hidden flex-shrink-0",
                                                        if let Some(art_url) = &track.album_art_url {
                                                            img {
                                                                src: "{art_url}",
                                                                alt: "{track.title}",
                                                                class: "w-full h-full object-cover"
                                                            }
                                                        } else {
                                                            MusicIcon { class: "w-6 h-6 text-gray-400" }
                                                        }
                                                    }

                                                    // Track info
                                                    div { class: "flex-1 min-w-0",
                                                        div {
                                                            class: "font-medium text-white truncate hover:underline",
                                                            "{track.title}"
                                                        }
                                                        p { class: "text-sm text-gray-400 truncate",
                                                            "{track.artist}"
                                                        }
                                                    }

                                                    // Duration
                                                    div { class: "text-sm text-gray-400",
                                                        {track.duration.map(|d| format_duration(d)).unwrap_or_default()}
                                                    }

                                                    // Zap button
                                                    {
                                                        let zap_track = track.clone();
                                                        rsx! {
                                                            button {
                                                                class: "px-2 py-1 text-gray-400 hover:text-purple-400 transition-colors",
                                                                title: "Zap artist",
                                                                onclick: move |e| {
                                                                    e.stop_propagation();
                                                                    music_player::show_zap_dialog_for_track(Some(zap_track.clone()));
                                                                },
                                                                ZapIcon { class: "w-3 h-3" }
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
        }
    }
}
