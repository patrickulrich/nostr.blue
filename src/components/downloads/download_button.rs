//! Download button for podcast episodes and music tracks (native only).
//!
//! Renders nothing on web. States: not-downloaded (download arrow),
//! queued (pulsing), downloading (percent), paused (resume), completed
//! (check — click deletes), failed (retry).

#[cfg(feature = "native")]
use crate::components::icons;
#[cfg(feature = "native")]
use crate::stores::downloads::model::DownloadStatus;
use crate::stores::audio::music_player::MusicTrack;
use dioxus::prelude::*;

#[component]
pub fn DownloadButton(track: MusicTrack, #[props(default)] class: String) -> Element {
    #[cfg(not(feature = "native"))]
    {
        let _ = track;
        let _ = class;
        return rsx! {};
    }
    #[cfg(feature = "native")]
    {
        let item = {
            let state = crate::stores::downloads::store::DOWNLOADS.read();
            state
                .items
                .iter()
                .find(|i| i.id == track.id)
                .cloned()
        };
        let btn_class = format!("p-2 hover:bg-muted rounded-full transition {class}");
        match item.as_ref().map(|i| i.status) {
            None => rsx! {
                button {
                    class: "{btn_class}",
                    title: "Download for offline",
                    onclick: {
                        let track = track.clone();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            crate::stores::downloads::manager::enqueue(&track, false);
                        }
                    },
                    dangerous_inner_html: icons::DOWNLOAD,
                }
            },
            Some(DownloadStatus::Queued) => rsx! {
                button {
                    class: "{btn_class}",
                    title: "Queued — click to cancel",
                    onclick: {
                        let id = track.id.clone();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            crate::stores::downloads::manager::delete(&id);
                        }
                    },
                    span { class: "animate-pulse inline-flex",
                        dangerous_inner_html: icons::DOWNLOAD,
                    }
                }
            },
            Some(DownloadStatus::Downloading) => {
                let progress = item
                    .and_then(|i| i.progress())
                    .map(|p| format!("{}%", (p * 100.0) as u32))
                    .unwrap_or_else(|| "...".to_string());
                rsx! {
                    button {
                        class: "{btn_class} text-xs font-medium tabular-nums min-w-10",
                        title: "Downloading — click to pause",
                        onclick: {
                            let id = track.id.clone();
                            move |e: Event<MouseData>| {
                                e.stop_propagation();
                                crate::stores::downloads::manager::pause(&id);
                            }
                        },
                        "{progress}"
                    }
                }
            }
            Some(DownloadStatus::Paused) => rsx! {
                button {
                    class: "{btn_class}",
                    title: "Paused — click to resume",
                    onclick: {
                        let id = track.id.clone();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            crate::stores::downloads::manager::resume(&id);
                        }
                    },
                    dangerous_inner_html: icons::DOWNLOAD,
                }
            },
            Some(DownloadStatus::Completed) => rsx! {
                button {
                    class: "{btn_class} text-primary",
                    title: "Downloaded — click to remove",
                    onclick: {
                        let id = track.id.clone();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            crate::stores::downloads::manager::delete(&id);
                        }
                    },
                    dangerous_inner_html: icons::CHECK_CIRCLE,
                }
            },
            Some(DownloadStatus::Failed) => rsx! {
                button {
                    class: "{btn_class} text-red-500",
                    title: item
                        .and_then(|i| i.error)
                        .map(|e| format!("Failed: {e} — click to retry"))
                        .unwrap_or_else(|| "Failed — click to retry".to_string()),
                    onclick: {
                        let id = track.id.clone();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            crate::stores::downloads::manager::retry(&id);
                        }
                    },
                    dangerous_inner_html: icons::ROTATE_CW,
                }
            },
        }
    }
}
