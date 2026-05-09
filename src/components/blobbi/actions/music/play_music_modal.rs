use dioxus::prelude::*;

use crate::components::blobbi::actions::music::audio_playback::{use_audio_player, AudioPlayer};
use crate::components::blobbi::actions::music::track_catalog;

#[component]
pub fn PlayMusicModal(on_select: EventHandler<String>, on_close: EventHandler<()>) -> Element {
    let mut player = use_audio_player();
    #[allow(unused_mut)]
    let mut previewing_id: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| {
                player.stop();
                on_close.call(());
            },

            div {
                class: "bg-card border border-border rounded-2xl w-full max-w-md mx-4 shadow-2xl max-h-[85vh] flex flex-col",
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                div { class: "flex items-center justify-between p-4 border-b border-border",
                    div { class: "flex items-center gap-2",
                        span { class: "text-xl", "\u{1F3B5}" }
                        h3 { class: "text-lg font-bold", "BlobbiFM" }
                    }
                    button {
                        class: "p-1.5 hover:bg-accent rounded-lg transition",
                        onclick: move |_| {
                            player.stop();
                            on_close.call(());
                        },
                        "\u{2715}"
                    }
                }

                div { class: "flex-1 overflow-y-auto p-4 space-y-2",
                    for track in track_catalog::TRACKS {
                        {rsx! {
                            TrackRow {
                                key: "{track.id}",
                                track_id: track.id,
                                track_title: track.title,
                                track_artist: track.artist,
                                track_url: track.url,
                                track_duration: track.duration_secs,
                                player: player,
                                previewing_id: previewing_id,
                                on_select: on_select,
                                on_close: on_close,
                            }
                        }}
                    }
                }

                div { class: "p-4 border-t border-border text-center",
                    span { class: "text-xs text-muted-foreground",
                        "Pick a track for your Blobbi"
                    }
                }
            }
        }

        super::audio_playback::AudioElement { player: player }
    }
}

#[component]
fn TrackRow(
    track_id: &'static str,
    track_title: &'static str,
    track_artist: &'static str,
    track_url: &'static str,
    track_duration: u32,
    mut player: AudioPlayer,
    previewing_id: Signal<Option<String>>,
    on_select: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    let is_current = *previewing_id.read() == Some(track_id.to_string());
    let is_playing = is_current && player.is_playing();
    let icon = if is_playing { "\u{23F8}" } else { "\u{25B6}" };

    rsx! {
        div { class: "w-full flex items-center gap-3 p-3 rounded-xl hover:bg-accent transition",
            button {
                class: "text-lg w-8 text-center shrink-0",
                onclick: move |_| {
                    if is_playing {
                        player.stop();
                        previewing_id.set(None);
                    } else {
                        player.play(track_url, track_id);
                        previewing_id.set(Some(track_id.to_string()));
                    }
                },
                "{icon}"
            }

            div { class: "flex-1 min-w-0",
                p { class: "font-medium text-sm truncate", "{track_title}" }
                p { class: "text-xs text-muted-foreground", "{track_artist}" }
            }

            span { class: "text-xs text-muted-foreground shrink-0",
                {track_catalog::format_duration(track_duration)}
            }

            button {
                class: "ml-2 px-3 py-1 text-xs rounded-lg bg-blue-500/10 text-blue-500 hover:bg-blue-500/20 transition shrink-0",
                onclick: move |e: Event<MouseData>| {
                    e.stop_propagation();
                    player.stop();
                    on_select.call(track_id.to_string());
                    on_close.call(());
                },
                "Play"
            }
        }
    }
}
