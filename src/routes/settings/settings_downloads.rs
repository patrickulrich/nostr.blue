//! Downloads settings: on-device download management for podcasts + music
//! (Android + Linux desktop; hidden on web builds).

use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn SettingsDownloads() -> Element {
    #[cfg(not(feature = "native"))]
    {
        return rsx! {
            div { class: "max-w-2xl mx-auto px-4 py-6",
                div { class: "mb-6",
                    Link {
                        to: Route::Settings {},
                        class: "text-sm text-primary hover:underline mb-4 inline-block",
                        "\u{2190} Back to Settings"
                    }
                    h1 { class: "text-2xl bold", "Downloads" }
                    p { class: "text-muted-foreground mt-2",
                        "Offline media is available on the Android and Linux desktop apps."
                    }
                }
            }
        };
    }
    #[cfg(feature = "native")]
    {
        use crate::stores::downloads;
        let state = downloads::store::DOWNLOADS.read().clone();
        let podcasts_bytes: u64 = state
            .items
            .iter()
            .filter(|i| i.kind == downloads::model::MediaKind::Podcast)
            .filter(|i| i.status == downloads::model::DownloadStatus::Completed)
            .map(|i| i.bytes_downloaded)
            .sum();
        let music_bytes: u64 = state
            .items
            .iter()
            .filter(|i| i.kind == downloads::model::MediaKind::Music)
            .filter(|i| i.status == downloads::model::DownloadStatus::Completed)
            .map(|i| i.bytes_downloaded)
            .sum();
        rsx! {
            div { class: "max-w-2xl mx-auto px-4 py-6",
                div { class: "mb-6",
                    Link {
                        to: Route::Settings {},
                        class: "text-sm text-primary hover:underline mb-4 inline-block",
                        "\u{2190} Back to Settings"
                    }
                    h1 { class: "text-2xl font-bold", "Downloads" }
                    p { class: "text-muted-foreground mt-2",
                        "Manage offline podcasts and music stored on this device."
                    }
                }

                // Storage usage (informational; space is managed manually or
                // via the Keep-per-show retention setting below).
                div { class: "bg-background border border-border rounded-lg shadow-xs mb-6",
                    div { class: "px-4 py-3 border-b border-border",
                        h2 { class: "text-lg font-semibold", "Storage" }
                    }
                    div { class: "p-4 space-y-3",
                        div { class: "flex justify-between text-sm",
                            span { "Used" }
                            span { class: "font-medium",
                                "{format_gb(state.storage_used_bytes)}"
                            }
                        }
                        div { class: "flex gap-4 text-xs text-muted-foreground",
                            span { "Podcasts: {format_gb(podcasts_bytes)}" }
                            span { "Music: {format_gb(music_bytes)}" }
                        }
                    }
                }

                // Download policy
                div { class: "bg-background border border-border rounded-lg shadow-xs mb-6",
                    div { class: "px-4 py-3 border-b border-border",
                        h2 { class: "text-lg font-semibold", "Policy" }
                    }
                    div { class: "divide-y divide-border",
                        SettingToggle {
                            label: "Enable downloads",
                            description: "When off, no new downloads start; existing files stay available.",
                            value: state.settings.enabled,
                            onchange: move |v| {
                                downloads::store::update_settings(|s| s.enabled = v);
                            },
                        }
                        WifiOnlyToggle {}
                    }
                    div { class: "divide-y divide-border border-t border-border",
                        SettingSlider {
                            label: "Episodes per show",
                            description: "How many of the newest episodes to auto-download for each show with auto-download enabled (toggle it on the show's page).",
                            value: state.settings.episodes_per_show,
                            min: 0,
                            max: 20,
                            onchange: move |v| {
                                downloads::store::update_settings(|s| s.episodes_per_show = v);
                            },
                        }
                        SettingSlider {
                            label: "Keep per show",
                            description: "Retention: at most this many auto-downloaded episodes per show are kept — older ones are removed during sync. Manually downloaded episodes are never removed.",
                            value: state.settings.keep_per_show,
                            min: 1,
                            max: 20,
                            onchange: move |v| {
                                downloads::store::update_settings(|s| s.keep_per_show = v);
                            },
                        }
                    }
                }

                // Library sync
                div { class: "bg-background border border-border rounded-lg shadow-xs mb-6",
                    div { class: "px-4 py-3 border-b border-border flex items-center justify-between",
                        h2 { class: "text-lg font-semibold", "Library Sync" }
                        button {
                            class: "px-3 py-1.5 rounded-lg text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50",
                            disabled: state.sync_running,
                            onclick: move |_| {
                                dioxus::prelude::spawn(async move {
                                    downloads::sync::sync_all_shows().await;
                                });
                            },
                            if state.sync_running { "Syncing…" } else { "Sync now" }
                        }
                    }
                    div { class: "p-4 text-sm text-muted-foreground",
                        if let Some(ts) = state.last_sync_at {
                            "Last synced {crate::utils::format::format_relative_time_or(ts * 1000, \"recently\")}"
                        } else {
                            "Never synced. Subscribed feeds refresh automatically every 15 minutes."
                        }
                    }
                }

                // Downloads list
                div { class: "bg-background border border-border rounded-lg shadow-xs mb-6",
                    div { class: "px-4 py-3 border-b border-border flex items-center justify-between",
                        h2 { class: "text-lg font-semibold", "Downloads" }
                        div { class: "flex gap-2",
                            button {
                                class: "px-3 py-1.5 rounded-lg text-xs font-medium bg-muted text-muted-foreground hover:bg-accent",
                                onclick: move |_| downloads::manager::resume_all(),
                                "Resume all"
                            }
                            button {
                                class: "px-3 py-1.5 rounded-lg text-xs font-medium bg-muted text-muted-foreground hover:bg-accent",
                                onclick: move |_| downloads::manager::pause_all(),
                                "Pause all"
                            }
                        }
                    }
                    if state.items.is_empty() {
                        div { class: "p-8 text-center text-muted-foreground text-sm",
                            "No downloads yet. Tap the download icon on any episode or track."
                        }
                    } else {
                        div { class: "divide-y divide-border",
                            for item in &state.items {
                                div { key: "{item.id}", class: "p-3 flex items-center gap-3",
                                    div { class: "flex-1 min-w-0",
                                        div { class: "text-sm font-medium truncate", "{item.track.title}" }
                                        div { class: "text-xs text-muted-foreground truncate",
                                            "{item.track.artist} · {status_label(item.status)}"
                                            if let Some(ref err) = item.error {
                                                span { class: "text-red-500", " — {err}" }
                                            }
                                        }
                                            if let Some(p) = item.progress() {
                                                div { class: "h-1 bg-muted rounded-full overflow-hidden mt-1",
                                                    div { class: "h-full bg-primary rounded-full", style: format!("width: {}%", (p * 100.0) as u32) }
                                                }
                                            }
                                    }
                                    div { class: "flex gap-1 shrink-0",
                                        match item.status {
                                            downloads::model::DownloadStatus::Downloading => rsx! {
                                                button {
                                                    class: "px-3 py-1.5 rounded-lg text-xs font-medium bg-muted hover:bg-accent",
                                                    onclick: {
                                                        let id = item.id.clone();
                                                        move |_| downloads::manager::pause(&id)
                                                    },
                                                    "Pause"
                                                }
                                            },
                                            downloads::model::DownloadStatus::Paused => rsx! {
                                                button {
                                                    class: "px-3 py-1.5 rounded-lg text-xs font-medium bg-muted hover:bg-accent",
                                                    onclick: {
                                                        let id = item.id.clone();
                                                        move |_| downloads::manager::resume(&id)
                                                    },
                                                    "Resume"
                                                }
                                            },
                                            downloads::model::DownloadStatus::Failed => rsx! {
                                                button {
                                                    class: "px-3 py-1.5 rounded-lg text-xs font-medium bg-muted hover:bg-accent",
                                                    onclick: {
                                                        let id = item.id.clone();
                                                        move |_| downloads::manager::retry(&id)
                                                    },
                                                    "Retry"
                                                }
                                            },
                                            _ => rsx! { span { class: "text-xs text-muted-foreground",
                                                "{format_gb(item.bytes_downloaded)}"
                                            } },
                                        }
                                        button {
                                            class: "px-3 py-1.5 rounded-lg text-xs font-medium bg-muted hover:bg-accent text-red-500",
                                            onclick: {
                                                let id = item.id.clone();
                                                move |_| downloads::manager::delete(&id)
                                            },
                                            "Delete"
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "p-4 border-t border-border",
                            button {
                                class: "w-full px-4 py-2 rounded-lg text-sm font-medium bg-destructive/10 text-destructive hover:bg-destructive/20 transition",
                                onclick: move |_| downloads::manager::delete_all(),
                                "Delete all downloads"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn WifiOnlyToggle() -> Element {
    #[cfg(not(feature = "mobile_platform"))]
    {
        return rsx! {};
    }
    #[cfg(feature = "mobile_platform")]
    {
        let wifi_only = crate::stores::downloads::store::DOWNLOADS.read().settings.wifi_only;
        rsx! {
            SettingToggle {
                label: "Wi-Fi only",
                description: "Pause all downloads while on metered networks.",
                value: wifi_only,
                onchange: move |v| {
                    crate::stores::downloads::store::update_settings(|s| s.wifi_only = v);
                },
            }
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn SettingSlider(
    label: String,
    description: String,
    value: u32,
    min: u32,
    max: u32,
    onchange: EventHandler<u32>,
) -> Element {
    rsx! {
        div { class: "p-4",
            div { class: "flex items-center justify-between",
                div { class: "text-sm font-medium", "{label}" }
                span { class: "text-sm font-semibold tabular-nums text-primary",
                    if value == 0 { "Off" } else { "{value}" }
                }
            }
            div { class: "text-xs text-muted-foreground mt-0.5", "{description}" }
            input {
                r#type: "range",
                class: "w-full mt-3 accent-primary",
                min: "{min}",
                max: "{max}",
                step: "1",
                value: "{value}",
                oninput: move |e: Event<FormData>| {
                    if let Ok(v) = e.value().parse::<u32>() {
                        onchange.call(v.clamp(min, max));
                    }
                },
            }
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn SettingToggle(
    label: String,
    description: String,
    value: bool,
    onchange: EventHandler<bool>,
) -> Element {
    rsx! {
        div { class: "p-4 flex items-center justify-between gap-4",
            div { class: "min-w-0",
                div { class: "text-sm font-medium truncate", "{label}" }
                div { class: "text-xs text-muted-foreground mt-0.5", "{description}" }
            }
            button {
                class: if value {
                    "relative w-11 h-6 rounded-full bg-primary transition shrink-0"
                } else {
                    "relative w-11 h-6 rounded-full bg-muted transition shrink-0"
                },
                role: "switch",
                aria_checked: "{value}",
                onclick: move |_| onchange.call(!value),
                span {
                    class: if value {
                        "absolute top-0.5 left-[22px] w-5 h-5 rounded-full bg-white transition-all"
                    } else {
                        "absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white transition-all"
                    },
                }
            }
        }
    }
}

#[cfg(feature = "native")]
fn status_label(status: crate::stores::downloads::model::DownloadStatus) -> &'static str {
    use crate::stores::downloads::model::DownloadStatus;
    match status {
        DownloadStatus::Queued => "Queued",
        DownloadStatus::Downloading => "Downloading",
        DownloadStatus::Paused => "Paused",
        DownloadStatus::Completed => "Downloaded",
        DownloadStatus::Failed => "Failed",
    }
}

#[cfg(feature = "native")]
fn format_gb(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
