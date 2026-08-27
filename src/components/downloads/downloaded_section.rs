//! Offline library lists: downloaded podcast episodes grouped by show, and
//! downloaded music tracks (native only; components render nothing on web).

#[cfg(feature = "native")]
use crate::components::icons;
#[cfg(feature = "native")]
use crate::stores::audio::music_player;
use dioxus::prelude::*;

/// Chip toggle for the "Downloaded" library filter. Renders nothing on web.
#[component]
pub fn DownloadedFilterChip(
    active: bool,
    ontoggle: EventHandler<()>,
    #[props(default)] label: String,
) -> Element {
    #[cfg(not(feature = "native"))]
    {
        let _ = (active, ontoggle, label);
        return rsx! {};
    }
    #[cfg(feature = "native")]
    {
        let class = if active {
            "px-3 py-1.5 rounded-full text-xs font-medium bg-primary text-primary-foreground transition"
        } else {
            "px-3 py-1.5 rounded-full text-xs font-medium bg-muted text-muted-foreground hover:bg-accent transition"
        };
        rsx! {
            button {
                class: "{class}",
                onclick: move |_| ontoggle.call(()),
                span { class: "inline-flex items-center gap-1",
                    span { class: "inline-flex", dangerous_inner_html: icons::DOWNLOAD }
                    if label.is_empty() { "Downloaded" } else { "{label}" }
                }
            }
        }
    }
}

/// Downloaded podcast episodes grouped by show, with expandable track lists.
#[component]
pub fn DownloadedShowsList() -> Element {
    #[cfg(not(feature = "native"))]
    {
        return rsx! {};
    }
    #[cfg(feature = "native")]
    {
        let shows = use_resource(move || async move {
            crate::stores::downloads::sync::downloaded_episodes_by_show().await
        });
        let mut expanded = use_signal(String::new);
        let shows_state = shows.read().clone();
        rsx! {
            match shows_state {
                None => rsx! {
                    div { class: "p-8 text-center",
                        div { class: "animate-spin rounded-full h-8 w-8 border-b-2 border-primary mx-auto" }
                    }
                },
                Some(ref shows) if shows.is_empty() => rsx! {
                    div { class: "text-center py-12 space-y-3",
                        div { class: "text-4xl", "📥" }
                        h3 { class: "text-lg font-semibold", "No downloaded episodes" }
                        p { class: "text-muted-foreground text-sm max-w-sm mx-auto",
                            "Tap the download icon on any episode to save it for offline listening."
                        }
                    }
                },
                Some(ref shows) => rsx! {
                    div { class: "space-y-2",
                        for show in shows {
                            div {
                                key: "{show.show_key}",
                                class: "bg-card border border-border rounded-lg overflow-hidden",
                                button {
                                    class: "w-full flex items-center gap-3 p-3 hover:bg-accent/50 transition text-left",
                                    onclick: {
                                        let key = show.show_key.clone();
                                        move |_| {
                                            let current = expanded.read().clone();
                                            expanded.set(if current == key {
                                                String::new()
                                            } else {
                                                key.clone()
                                            });
                                        }
                                    },
                                    if let Some(ref image) = show.image {
                                        img {
                                            src: "{image}",
                                            alt: "",
                                            class: "w-10 h-10 rounded object-cover shrink-0",
                                            loading: "lazy",
                                            referrerpolicy: "no-referrer",
                                        }
                                    }
                                    div { class: "flex-1 min-w-0",
                                        div { class: "font-medium text-sm truncate", "{show.title}" }
                                        div { class: "text-xs text-muted-foreground",
                                            "{show.downloaded_count} downloaded · {format_bytes(show.bytes)}"
                                        }
                                    }
                                    span {
                                        class: "text-muted-foreground text-xs shrink-0 transition",
                                        if *expanded.read() == show.show_key { "▲" } else { "▼" }
                                    }
                                }
                                if *expanded.read() == show.show_key {
                                    div { class: "divide-y divide-border border-t border-border",
                                        for track in &show.tracks {
                                            {
                                                let track = track.clone();
                                                let playlist = show.tracks.clone();
                                                rsx! {
                                                    div {
                                                        key: "{track.id}",
                                                        class: "flex items-center gap-2 p-2.5 hover:bg-accent/50 transition",
                                                        button {
                                                            class: "flex-1 min-w-0 text-left",
                                                            onclick: {
                                                                let track = track.clone();
                                                                let playlist = playlist.clone();
                                                                move |_| {
                                                                    music_player::play_or_toggle_track(
                                                                        track.clone(),
                                                                        Some(playlist.clone()),
                                                                        None,
                                                                    );
                                                                }
                                                            },
                                                            span { class: "text-sm truncate block", "{track.title}" }
                                                            span { class: "text-xs text-muted-foreground block",
                                                                "{episode_duration_label(&track)}"
                                                            }
                                                        }
                                                        crate::components::downloads::DownloadButton {
                                                            track: track.clone(),
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
                },
            }
        }
    }
}

/// Downloaded music tracks list.
#[component]
pub fn DownloadedMusicList() -> Element {
    #[cfg(not(feature = "native"))]
    {
        return rsx! {};
    }
    #[cfg(feature = "native")]
    {
        let tracks = use_resource(move || async move {
            crate::stores::downloads::sync::downloaded_music().await
        });
        let tracks_state = tracks.read().clone();
        rsx! {
            match tracks_state {
                None => rsx! {
                    div { class: "p-8 text-center",
                        div { class: "animate-spin rounded-full h-8 w-8 border-b-2 border-primary mx-auto" }
                    }
                },
                Some(ref tracks) if tracks.is_empty() => rsx! {
                    div { class: "text-center py-12 space-y-3",
                        div { class: "text-4xl", "🎵" }
                        h3 { class: "text-lg font-semibold", "No downloaded tracks" }
                        p { class: "text-muted-foreground text-sm max-w-sm mx-auto",
                            "Tap the download icon on any track to save it for offline listening."
                        }
                    }
                },
                Some(ref tracks) => rsx! {
                    div { class: "space-y-2",
                        for track in tracks {
                            {
                                let track = track.clone();
                                let playlist = tracks.clone();
                                rsx! {
                                    div {
                                        key: "{track.id}",
                                        class: "flex items-center gap-3 p-2.5 bg-card border border-border rounded-lg hover:bg-accent/50 transition",
                                        button {
                                            class: "flex-1 min-w-0 text-left flex items-center gap-3",
                                            onclick: {
                                                let track = track.clone();
                                                let playlist = playlist.clone();
                                                move |_| {
                                                    music_player::play_or_toggle_track(
                                                        track.clone(),
                                                        Some(playlist.clone()),
                                                        None,
                                                    );
                                                }
                                            },
                                            if let Some(ref art) = track.album_art_url {
                                                img {
                                                    src: "{art}",
                                                    alt: "",
                                                    class: "w-10 h-10 rounded object-cover shrink-0",
                                                    loading: "lazy",
                                                    referrerpolicy: "no-referrer",
                                                }
                                            }
                                            div { class: "min-w-0",
                                                span { class: "text-sm font-medium truncate block", "{track.title}" }
                                                span { class: "text-xs text-muted-foreground truncate block", "{track.artist}" }
                                            }
                                        }
                                        crate::components::downloads::DownloadButton {
                                            track: track.clone(),
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

#[cfg(feature = "native")]
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    }
}

#[cfg(feature = "native")]
fn episode_duration_label(track: &crate::stores::audio::music_player::MusicTrack) -> String {
    track
        .duration
        .map(|d| format!("{} min", d / 60))
        .unwrap_or_else(|| "Episode".to_string())
}
