use crate::stores::bible_store::{
    download_translation, is_offline_available, remove_offline_translation, DOWNLOAD_IN_PROGRESS,
};
use dioxus::prelude::*;

#[component]
pub fn DownloadProgressButton(translation_id: String) -> Element {
    let downloading = DOWNLOAD_IN_PROGRESS.read().clone();
    let is_downloading = downloading.as_deref() == Some(&translation_id);
    let is_downloaded = is_offline_available(&translation_id);
    let mut confirm_remove = use_signal(|| false);

    if is_downloading {
        rsx! {
            div { class: "flex items-center gap-1.5 px-2 py-1 text-xs text-muted-foreground",
                div { class: "w-3 h-3 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                span { "Downloading..." }
            }
        }
    } else if is_downloaded {
        if *confirm_remove.read() {
            rsx! {
                div { class: "flex items-center gap-1",
                    button {
                        class: "px-2 py-1 text-xs text-red-500 hover:bg-red-500/10 rounded transition",
                        onclick: {
                            let tid = translation_id.clone();
                            move |_| {
                                let tid = tid.clone();
                                confirm_remove.set(false);
                                spawn(async move {
                                    let _ = remove_offline_translation(&tid).await;
                                });
                            }
                        },
                        "Remove"
                    }
                    button {
                        class: "px-2 py-1 text-xs text-muted-foreground hover:bg-muted rounded transition",
                        onclick: move |_| confirm_remove.set(false),
                        "Cancel"
                    }
                }
            }
        } else {
            rsx! {
                button {
                    class: "p-1.5 hover:bg-muted rounded-lg transition text-green-500",
                    title: "Available offline — click to remove",
                    onclick: move |_| confirm_remove.set(true),
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        class: "w-4 h-4",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path { stroke_linecap: "round", stroke_linejoin: "round", d: "M5 13l4 4L19 7" }
                    }
                }
            }
        }
    } else {
        rsx! {
            button {
                class: "p-1.5 hover:bg-muted rounded-lg transition text-muted-foreground",
                title: "Download for offline use",
                onclick: {
                    let tid = translation_id.clone();
                    move |_| {
                        let tid = tid.clone();
                        spawn(async move {
                            if let Err(e) = download_translation(&tid).await {
                                log::error!("Failed to download translation: {}", e);
                            }
                        });
                    }
                },
                svg {
                    xmlns: "http://www.w3.org/2000/svg",
                    class: "w-4 h-4",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path { stroke_linecap: "round", stroke_linejoin: "round", d: "M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" }
                }
            }
        }
    }
}

#[component]
pub fn OfflineBadge() -> Element {
    rsx! {
        span {
            class: "inline-flex items-center gap-1 px-1.5 py-0.5 text-[10px] bg-green-500/10 text-green-600 dark:text-green-400 rounded font-medium",
            title: "Reading from offline storage",
            svg {
                xmlns: "http://www.w3.org/2000/svg",
                class: "w-3 h-3",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                path { stroke_linecap: "round", stroke_linejoin: "round", d: "M5 13l4 4L19 7" }
            }
            "Offline"
        }
    }
}
