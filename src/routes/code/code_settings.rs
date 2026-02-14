//! Code Settings Page
//!
//! User-specific settings for the code section: git identity,
//! default branch preference, editor preferences, and notification toggles.
use crate::routes::Route;
use crate::stores::auth_store;
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

const CODE_SETTINGS_KEY: &str = "nostrblue_code_settings";

/// Persisted code-section settings (local storage only).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CodeSettingsData {
    /// Display name used for git commit attribution.
    #[serde(default)]
    pub git_display_name: String,
    /// Default branch name preference (e.g. "main", "master").
    #[serde(default = "default_branch")]
    pub default_branch: String,
    /// Tab size in spaces for the code viewer.
    #[serde(default = "default_tab_size")]
    pub tab_size: u8,
    /// Whether to wrap long lines in the code viewer.
    #[serde(default)]
    pub line_wrap: bool,
    /// Receive notifications for issues you participate in.
    #[serde(default = "default_true")]
    pub notify_issues: bool,
    /// Receive notifications for pull requests you participate in.
    #[serde(default = "default_true")]
    pub notify_pull_requests: bool,
    /// Receive notifications when you are requested as a reviewer.
    #[serde(default = "default_true")]
    pub notify_review_requests: bool,
}

fn default_branch() -> String {
    "main".to_string()
}
fn default_tab_size() -> u8 {
    4
}
fn default_true() -> bool {
    true
}

impl Default for CodeSettingsData {
    fn default() -> Self {
        Self {
            git_display_name: String::new(),
            default_branch: default_branch(),
            tab_size: default_tab_size(),
            line_wrap: false,
            notify_issues: true,
            notify_pull_requests: true,
            notify_review_requests: true,
        }
    }
}

fn load_code_settings() -> CodeSettingsData {
    LocalStorage::get::<CodeSettingsData>(CODE_SETTINGS_KEY).unwrap_or_default()
}

fn save_code_settings(settings: &CodeSettingsData) {
    let _ = LocalStorage::set(CODE_SETTINGS_KEY, settings);
}

/// Code Settings page component.
#[component]
pub fn CodeSettings() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    if !auth.is_authenticated {
        return rsx! {
            div { class: "p-8 text-center text-muted-foreground",
                "Sign in to manage code settings"
            }
        };
    }

    let mut settings = use_signal(load_code_settings);
    let mut save_success = use_signal(|| false);

    let handle_save = move |_| {
        let data = settings.read().clone();
        save_code_settings(&data);
        save_success.set(true);
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(2500).await;
            save_success.set(false);
        });
    };

    rsx! {
        div { class: "min-h-screen",
            // Sticky header
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        Link {
                            to: Route::CodeHome {},
                            class: "p-2 hover:bg-accent rounded-lg transition text-muted-foreground hover:text-foreground",
                            svg {
                                class: "w-5 h-5",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                line { x1: "19", y1: "12", x2: "5", y2: "12" }
                                polyline { points: "12 19 5 12 12 5" }
                            }
                        }
                        h1 { class: "text-xl font-bold", "Code Settings" }
                    }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition text-sm font-medium",
                        onclick: handle_save,
                        "Save Changes"
                    }
                }
            }

            div { class: "p-4 max-w-2xl mx-auto space-y-6",
                // Success banner
                if *save_success.read() {
                    div { class: "p-4 bg-green-500/10 border border-green-500/20 rounded-lg text-green-600 dark:text-green-400 text-sm flex items-center gap-2",
                        svg {
                            class: "w-4 h-4",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M22 11.08V12a10 10 0 1 1-5.93-9.14" }
                            polyline { points: "22 4 12 14.01 9 11.01" }
                        }
                        "Settings saved successfully!"
                    }
                }

                // ── Git Identity ──────────────────────────────
                div { class: "bg-card border border-border rounded-lg p-4 space-y-4",
                    h2 { class: "text-lg font-semibold text-foreground", "Git Identity" }
                    p { class: "text-sm text-muted-foreground",
                        "Display name used for git commit attribution. Leave blank to use your Nostr profile name."
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Display Name" }
                        input {
                            class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "e.g. Ada Lovelace",
                            value: "{settings.read().git_display_name}",
                            oninput: move |e| {
                                settings.write().git_display_name = e.value();
                            },
                        }
                    }
                }

                // ── Default Branch ────────────────────────────
                div { class: "bg-card border border-border rounded-lg p-4 space-y-4",
                    h2 { class: "text-lg font-semibold text-foreground", "Default Branch" }
                    p { class: "text-sm text-muted-foreground",
                        "Preferred default branch name when creating new repositories."
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Branch Name" }
                        input {
                            class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "main",
                            value: "{settings.read().default_branch}",
                            oninput: move |e| {
                                settings.write().default_branch = e.value();
                            },
                        }
                    }
                }

                // ── Code Editor Preferences ───────────────────
                div { class: "bg-card border border-border rounded-lg p-4 space-y-4",
                    h2 { class: "text-lg font-semibold text-foreground", "Code Editor" }
                    p { class: "text-sm text-muted-foreground",
                        "Preferences for viewing code in the repository browser."
                    }

                    // Tab size
                    div {
                        label { class: "block text-sm font-medium mb-2", "Tab Size" }
                        div { class: "flex gap-2",
                            for size in [2u8, 4, 8] {
                                {
                                    let is_active = settings.read().tab_size == size;
                                    let cls = if is_active {
                                        "px-4 py-2 rounded-lg text-sm font-medium bg-primary text-primary-foreground"
                                    } else {
                                        "px-4 py-2 rounded-lg text-sm font-medium bg-muted text-muted-foreground hover:bg-accent transition"
                                    };
                                    rsx! {
                                        button {
                                            key: "{size}",
                                            class: cls,
                                            onclick: move |_| {
                                                settings.write().tab_size = size;
                                            },
                                            "{size}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Line wrap toggle
                    div { class: "flex items-center justify-between",
                        div {
                            span { class: "text-sm font-medium", "Line Wrapping" }
                            p { class: "text-xs text-muted-foreground", "Wrap long lines instead of horizontal scrolling" }
                        }
                        ToggleSwitch {
                            checked: settings.read().line_wrap,
                            on_toggle: move |val: bool| {
                                settings.write().line_wrap = val;
                            },
                        }
                    }
                }

                // ── Notification Preferences ──────────────────
                div { class: "bg-card border border-border rounded-lg p-4 space-y-4",
                    h2 { class: "text-lg font-semibold text-foreground", "Notifications" }
                    p { class: "text-sm text-muted-foreground",
                        "Choose which code-related notifications you want to receive."
                    }

                    // Issues
                    div { class: "flex items-center justify-between py-2 border-b border-border",
                        div {
                            span { class: "text-sm font-medium", "Issue Notifications" }
                            p { class: "text-xs text-muted-foreground", "New comments and status changes on issues you participate in" }
                        }
                        ToggleSwitch {
                            checked: settings.read().notify_issues,
                            on_toggle: move |val: bool| {
                                settings.write().notify_issues = val;
                            },
                        }
                    }

                    // Pull Requests
                    div { class: "flex items-center justify-between py-2 border-b border-border",
                        div {
                            span { class: "text-sm font-medium", "Pull Request Notifications" }
                            p { class: "text-xs text-muted-foreground", "Updates on pull requests you created or are mentioned in" }
                        }
                        ToggleSwitch {
                            checked: settings.read().notify_pull_requests,
                            on_toggle: move |val: bool| {
                                settings.write().notify_pull_requests = val;
                            },
                        }
                    }

                    // Review Requests
                    div { class: "flex items-center justify-between py-2",
                        div {
                            span { class: "text-sm font-medium", "Review Requests" }
                            p { class: "text-xs text-muted-foreground", "When someone requests your review on a pull request" }
                        }
                        ToggleSwitch {
                            checked: settings.read().notify_review_requests,
                            on_toggle: move |val: bool| {
                                settings.write().notify_review_requests = val;
                            },
                        }
                    }
                }
            }
        }
    }
}

/// A simple toggle-switch component styled as a sliding pill.
#[component]
fn ToggleSwitch(checked: bool, on_toggle: EventHandler<bool>) -> Element {
    let bg = if checked { "bg-primary" } else { "bg-muted" };
    let dot_translate = if checked {
        "translate-x-5"
    } else {
        "translate-x-0"
    };

    rsx! {
        button {
            class: "relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-hidden focus:ring-2 focus:ring-primary focus:ring-offset-2 {bg}",
            r#type: "button",
            role: "switch",
            aria_checked: "{checked}",
            onclick: move |_| on_toggle.call(!checked),
            span {
                class: "pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out {dot_translate}",
            }
        }
    }
}
