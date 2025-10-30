use dioxus::prelude::*;
use crate::components::icons::*;
use crate::services::wavlake::{get_artist, WavlakeArtist};

#[component]
pub fn MusicArtist(artist_id: String) -> Element {
    let artist_id_for_effect = artist_id.clone();
    let mut artist_state = use_signal(|| None::<WavlakeArtist>);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);

    // Fetch artist data
    use_effect(move || {
        let id = artist_id_for_effect.clone();
        spawn(async move {
            loading.set(true);
            match get_artist(&id).await {
                Ok(artist_data) => {
                    artist_state.set(Some(artist_data));
                    loading.set(false);
                }
                Err(e) => {
                    error_msg.set(Some(format!("Failed to load artist: {}", e)));
                    loading.set(false);
                }
            }
        });
    });

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
                        div { class: "w-32 h-32 bg-gray-700 rounded-lg animate-pulse" }
                        div { class: "flex-1 space-y-4",
                            div { class: "h-8 bg-gray-700 rounded w-64 animate-pulse" }
                            div { class: "h-4 bg-gray-700 rounded w-48 animate-pulse" }
                            div { class: "h-16 bg-gray-700 rounded w-full animate-pulse" }
                        }
                    }
                }
            }

            // Error state
            if let Some(err) = error_msg() {
                div { class: "bg-gray-800/50 backdrop-blur-sm rounded-lg border border-gray-700 p-8 text-center",
                    UserIcon { class: "w-12 h-12 text-gray-400 mx-auto mb-4" }
                    h2 { class: "text-2xl font-bold mb-2", "Artist Not Found" }
                    p { class: "text-gray-400", "{err}" }
                }
            }

            // Artist content
            if let Some(artist) = artist_state() {
                div { class: "space-y-6",
                    // Artist Profile
                    div { class: "bg-gray-800/50 backdrop-blur-sm rounded-lg border border-gray-700 p-6",
                        div { class: "flex items-start gap-6",
                            // Artist image
                            div { class: "w-32 h-32 bg-gray-700 rounded-lg flex items-center justify-center overflow-hidden flex-shrink-0",
                                if let Some(art_url) = &artist.artist_art_url {
                                    if !art_url.is_empty() {
                                        img {
                                            src: "{art_url}",
                                            alt: "{artist.name}",
                                            class: "w-full h-full object-cover"
                                        }
                                    } else {
                                        UserIcon { class: "w-16 h-16 text-gray-400" }
                                    }
                                } else {
                                    UserIcon { class: "w-16 h-16 text-gray-400" }
                                }
                            }

                            div { class: "flex-1 space-y-4",
                                // Name and metadata
                                div {
                                    h1 { class: "text-3xl font-bold text-white", "{artist.name}" }
                                }

                                // Stats and actions
                                div { class: "flex items-center gap-4 flex-wrap",
                                    span { class: "inline-flex items-center gap-1 px-3 py-1 rounded-full bg-gray-700 text-sm text-gray-300",
                                        MusicIcon { class: "w-3 h-3" }
                                        "{artist.albums.len()} "
                                        if artist.albums.len() == 1 { "Album" } else { "Albums" }
                                    }

                                    // Zap artist button - disabled until we have a real track
                                    // (Zapping requires a valid track ID, not an album ID)
                                    span {
                                        class: "text-sm text-gray-500 italic",
                                        "Zap feature requires selecting a track"
                                    }
                                }

                                // Bio
                                if let Some(bio) = &artist.bio {
                                    if !bio.is_empty() {
                                        p { class: "text-gray-300 leading-relaxed", "{bio}" }
                                    }
                                }
                            }
                        }
                    }

                    // Albums
                    div { class: "bg-gray-800/50 backdrop-blur-sm rounded-lg border border-gray-700 p-6",
                        div { class: "flex items-center gap-2 mb-4",
                            DiscIcon { class: "w-5 h-5 text-purple-400" }
                            h2 { class: "text-xl font-bold text-white",
                                "Albums ({artist.albums.len()})"
                            }
                        }

                        if artist.albums.is_empty() {
                            div { class: "text-center py-12",
                                DiscIcon { class: "w-16 h-16 text-gray-400 mx-auto mb-4" }
                                h3 { class: "text-lg font-semibold mb-2 text-white", "No Albums Found" }
                                p { class: "text-gray-400",
                                    "This artist hasn't released any albums on Wavlake yet."
                                }
                            }
                        } else {
                            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6",
                                for album in artist.albums.iter() {
                                    div {
                                        key: "{album.id}",
                                        class: "group bg-gray-900/50 rounded-lg border border-gray-700 hover:border-purple-500 transition-all duration-200 overflow-hidden",

                                        // Album art with play overlay
                                        div { class: "aspect-square relative overflow-hidden bg-gradient-to-br from-purple-900/20 to-blue-900/20",
                                            if !album.album_art_url.is_empty() {
                                                img {
                                                    src: "{album.album_art_url}",
                                                    alt: "{album.title}",
                                                    class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-200"
                                                }
                                            } else {
                                                div { class: "w-full h-full flex items-center justify-center",
                                                    DiscIcon { class: "w-16 h-16 text-gray-400" }
                                                }
                                            }

                                            // Play overlay
                                            div { class: "absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center",
                                                a {
                                                    href: "/music/album/{album.id}",
                                                    class: "bg-purple-600 hover:bg-purple-500 text-white rounded-full p-4 shadow-lg transition-colors",
                                                    PlayIcon { class: "w-6 h-6" }
                                                }
                                            }
                                        }

                                        // Album info
                                        div { class: "p-4 space-y-3",
                                            div {
                                                h4 {
                                                    class: "font-semibold text-lg text-white truncate",
                                                    title: "{album.title}",
                                                    "{album.title}"
                                                }
                                                p { class: "text-sm text-gray-400 flex items-center gap-1",
                                                    CalendarIcon { class: "w-3 h-3" }
                                                    {
                                                        // Parse release date and extract year
                                                        album.release_date.split('T').next().unwrap_or("Unknown").split('-').next().unwrap_or("Unknown")
                                                    }
                                                }
                                            }

                                            a {
                                                href: "/music/album/{album.id}",
                                                class: "block w-full",
                                                button {
                                                    class: "w-full px-4 py-2 rounded border border-gray-600 hover:border-purple-500 text-sm text-gray-300 hover:text-white transition-colors inline-flex items-center justify-center gap-2",
                                                    PlayIcon { class: "w-3 h-3" }
                                                    "View Album"
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
