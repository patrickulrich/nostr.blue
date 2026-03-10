//! Code Settings Page
//!
//! User-specific settings for the code section: git identity,
//! default branch preference, editor preferences, and notification toggles.
use crate::components::code::SshKeyManager;
use crate::platform::storage;
use crate::routes::Route;
use crate::stores::auth_store;
use crate::stores::nwc_store::{self, ConnectionStatus};
use crate::stores::profiles::PROFILE_CACHE;
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use serde::{Deserialize, Serialize};
use std::time::Duration;

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

fn settings_storage_key() -> String {
    match auth_store::get_pubkey() {
        Some(pk) => format!("{}_{}", CODE_SETTINGS_KEY, pk),
        None => CODE_SETTINGS_KEY.to_string(),
    }
}

fn load_code_settings() -> CodeSettingsData {
    match storage::get::<CodeSettingsData>(&settings_storage_key()) {
        Ok(data) => data,
        Err(e) => {
            if e.contains("not found") {
                CodeSettingsData::default()
            } else {
                log::warn!(
                    "Failed to load code settings from {}: {}",
                    settings_storage_key(),
                    e
                );
                CodeSettingsData::default()
            }
        }
    }
}

fn save_code_settings(settings: &CodeSettingsData) -> Result<(), String> {
    storage::set(&settings_storage_key(), settings)
        .map_err(|e| format!("Failed to save settings: {}", e))
}

/// Code Settings page component.
#[component]
pub fn CodeSettings() -> Element {
    // All hooks must be called before any early returns to maintain stable call-order indices.
    let mut settings = use_signal(load_code_settings);
    let mut save_success = use_signal(|| false);
    let mut save_error = use_signal(|| None::<String>);

    // Reload settings when pubkey changes (e.g., switching accounts)
    use_effect(move || {
        let _pk = auth_store::AUTH_STATE.read().pubkey.clone();
        save_success.set(false);
        save_error.set(None);
        settings.set(load_code_settings());
    });

    let auth = auth_store::AUTH_STATE.read();
    if !auth.is_authenticated {
        return rsx! {
            div { class: "p-8 text-center text-muted-foreground",
                "Sign in to manage code settings"
            }
        };
    }

    let handle_save = move |_| {
        let data = settings.read().clone();
        match save_code_settings(&data) {
            Ok(()) => {
                save_error.set(None);
                save_success.set(true);
                spawn(async move {
                    crate::platform::timer::sleep_ms(2500).await;
                    save_success.set(false);
                });
            }
            Err(e) => {
                save_success.set(false);
                save_error.set(Some(e.clone()));
                log::error!("Settings save failed: {}", e);
                spawn(async move {
                    crate::platform::timer::sleep_ms(2500).await;
                    save_error.set(None);
                });
            }
        }
    };

    rsx! {
        div { class: "min-h-screen",
            // Sticky header
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        Link {
                            to: Route::CodeHome {},
                            class: "text-muted-foreground hover:text-foreground transition",
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
                // Error banner
                if let Some(err) = save_error.read().as_ref() {
                    div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm flex items-center gap-2",
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
                            circle { cx: "12", cy: "12", r: "10" }
                            line { x1: "15", y1: "9", x2: "9", y2: "15" }
                            line { x1: "9", y1: "9", x2: "15", y2: "15" }
                        }
                        "{err}"
                    }
                }
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
                                        "px-4 py-2 rounded-lg text-sm font-medium bg-primary text-primary-foreground transition"
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

                // ── Lightning Wallet (NWC) ──────────────────────
                WalletSection {}

                // ── SSH Keys ────────────────────────────────────
                div { class: "bg-card border border-border rounded-lg p-4 space-y-4",
                    h2 { class: "text-lg font-semibold text-foreground flex items-center gap-2",
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
                            path { d: "M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4" }
                        }
                        "SSH Keys"
                    }
                    p { class: "text-sm text-muted-foreground",
                        "Manage SSH keys for Git push access to repositories."
                    }
                    SshKeyManager {}
                }
            }
        }
    }
}

/// NWC Wallet configuration section.
#[component]
fn WalletSection() -> Element {
    let status = nwc_store::NWC_STATUS.read().clone();
    let balance = *nwc_store::NWC_BALANCE.read();
    let mut nwc_uri = use_signal(String::new);
    let mut connect_error = use_signal(|| None::<String>);
    let mut is_connecting = use_signal(|| false);
    let toast = consume_toast();

    // Look up current user's lightning address
    let lightning_address = {
        let auth = auth_store::AUTH_STATE.read();
        if let Some(ref pk) = auth.pubkey {
            let cache = PROFILE_CACHE.read();
            cache.peek(pk).and_then(|p| p.lud16.clone())
        } else {
            None
        }
    };

    let handle_connect = move |_| {
        let uri = nwc_uri.read().clone();
        if uri.trim().is_empty() {
            connect_error.set(Some("Please enter a NWC connection string".to_string()));
            return;
        }
        connect_error.set(None);
        is_connecting.set(true);
        spawn(async move {
            match nwc_store::connect_nwc(&uri, true).await {
                Ok(()) => {
                    nwc_uri.set(String::new());
                    connect_error.set(None);
                }
                Err(e) => {
                    connect_error.set(Some(e));
                }
            }
            is_connecting.set(false);
        });
    };

    let handle_disconnect = move |_| {
        nwc_store::disconnect_nwc(false);
    };

    let handle_refresh = move |_| {
        let toast = toast;
        spawn(async move {
            if let Err(e) = nwc_store::refresh_balance().await {
                log::error!("Failed to refresh balance: {}", e);
                toast.error(
                    format!("Failed to refresh balance: {}", e),
                    ToastOptions::new()
                        .duration(Duration::from_secs(5))
                        .permanent(false),
                );
            }
        });
    };

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 space-y-4",
            h2 { class: "text-lg font-semibold text-foreground flex items-center gap-2",
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
                    path { d: "M13 2L3 14h9l-1 8 10-12h-9l1-8z" }
                }
                "Lightning Wallet"
            }
            p { class: "text-sm text-muted-foreground",
                "Connect a Nostr Wallet Connect (NWC) compatible wallet to enable bounty payments and zap distribution."
            }

            // Status badge
            div { class: "flex items-center gap-3",
                match &status {
                    ConnectionStatus::Connected => rsx! {
                        span { class: "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-green-500/10 text-green-600 dark:text-green-400",
                            span { class: "w-2 h-2 rounded-full bg-green-500" }
                            "Connected"
                        }
                    },
                    ConnectionStatus::Connecting => rsx! {
                        span { class: "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-yellow-500/10 text-yellow-600 dark:text-yellow-400",
                            span { class: "w-2 h-2 rounded-full bg-yellow-500 animate-pulse" }
                            "Connecting..."
                        }
                    },
                    ConnectionStatus::Error(msg) => rsx! {
                        span { class: "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-red-500/10 text-red-600 dark:text-red-400",
                            span { class: "w-2 h-2 rounded-full bg-red-500" }
                            "Error: {msg}"
                        }
                    },
                    ConnectionStatus::Disconnected => rsx! {
                        span { class: "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-muted text-muted-foreground",
                            span { class: "w-2 h-2 rounded-full bg-muted-foreground" }
                            "Not connected"
                        }
                    },
                }
            }

            // Balance + Lightning address (when connected)
            if matches!(status, ConnectionStatus::Connected) {
                div { class: "p-3 bg-muted rounded-lg space-y-2",
                    div { class: "flex items-center justify-between",
                        span { class: "text-sm text-muted-foreground", "Balance" }
                        div { class: "flex items-center gap-2",
                            span { class: "text-sm font-mono font-medium",
                                match balance {
                                    Some(msats) => format!("{} sats", msats / 1000),
                                    None => "Loading...".to_string(),
                                }
                            }
                            button {
                                class: "p-1 hover:bg-accent rounded transition text-muted-foreground hover:text-foreground",
                                title: "Refresh balance",
                                onclick: handle_refresh,
                                svg {
                                    class: "w-3.5 h-3.5",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "24",
                                    height: "24",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    polyline { points: "23 4 23 10 17 10" }
                                    polyline { points: "1 20 1 14 7 14" }
                                    path { d: "M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" }
                                }
                            }
                        }
                    }
                    if let Some(ref addr) = lightning_address {
                        div { class: "flex items-center justify-between",
                            span { class: "text-sm text-muted-foreground", "Lightning Address" }
                            span { class: "text-sm font-mono", "{addr}" }
                        }
                    }
                }

                button {
                    class: "px-4 py-2 bg-destructive text-destructive-foreground rounded-lg hover:opacity-90 transition text-sm font-medium",
                    onclick: handle_disconnect,
                    "Disconnect Wallet"
                }
            }

            // Connect form (when disconnected)
            if matches!(status, ConnectionStatus::Disconnected | ConnectionStatus::Error(_)) {
                div { class: "space-y-3",
                    div {
                        label { class: "block text-sm font-medium mb-2", "NWC Connection String" }
                        input {
                            class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "password",
                            placeholder: "nostr+walletconnect://...",
                            value: "{nwc_uri.read()}",
                            oninput: move |e| {
                                nwc_uri.set(e.value());
                            },
                        }
                        p { class: "text-xs text-muted-foreground mt-1",
                            "Get this from your NWC-compatible wallet (e.g. Alby, Mutiny, Coinos)."
                        }
                    }
                    if let Some(ref err) = *connect_error.read() {
                        p { class: "text-sm text-destructive", "{err}" }
                    }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition text-sm font-medium disabled:opacity-50",
                        disabled: *is_connecting.read(),
                        onclick: handle_connect,
                        if *is_connecting.read() {
                            "Connecting..."
                        } else {
                            "Connect Wallet"
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
