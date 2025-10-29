use dioxus::prelude::*;
use crate::stores::{auth_store, theme_store, nostr_client, settings_store};

#[component]
pub fn Settings() -> Element {
    let theme = theme_store::THEME.read();
    let relays = nostr_client::RELAY_POOL.read();

    // Load settings from Nostr on mount
    use_effect(move || {
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        if is_authenticated {
            spawn(async move {
                log::info!("Loading settings from Nostr (NIP-78)...");
                if let Err(e) = settings_store::load_settings().await {
                    log::error!("Failed to load settings: {}", e);
                }
            });
        }
    });

    let auth = auth_store::AUTH_STATE.read();

    let mut new_relay_url = use_signal(|| String::new());
    let mut relay_error = use_signal(|| None::<String>);

    let add_relay = move |_| {
        let url = new_relay_url.read().clone();
        if url.is_empty() {
            relay_error.set(Some("Please enter a relay URL".to_string()));
            return;
        }

        if !url.starts_with("wss://") && !url.starts_with("ws://") {
            relay_error.set(Some("Relay URL must start with wss:// or ws://".to_string()));
            return;
        }

        spawn(async move {
            match nostr_client::add_relay(&url).await {
                Ok(_) => {
                    relay_error.set(None);
                    new_relay_url.set(String::new());
                }
                Err(e) => {
                    relay_error.set(Some(e));
                }
            }
        });
    };

    let remove_relay = move |url: String| {
        spawn(async move {
            nostr_client::remove_relay(&url).await.ok();
        });
    };

    rsx! {
        div {
            class: "space-y-6",

            // Page header
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                h2 {
                    class: "text-2xl font-semibold text-gray-900 dark:text-white flex items-center gap-2",
                    crate::components::icons::SettingsIcon { class: "w-7 h-7" }
                    "Settings"
                }
                p {
                    class: "text-gray-600 dark:text-gray-400 mt-2",
                    "Manage your account, relays, and preferences"
                }
            }

            // Account section
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                h3 {
                    class: "text-xl font-semibold mb-4 text-gray-900 dark:text-white flex items-center gap-2",
                    crate::components::icons::UserIcon { class: "w-6 h-6" }
                    "Account"
                }

                if auth.is_authenticated {
                    {render_account_info()}
                } else {
                    div {
                        class: "text-center p-6 text-gray-500 dark:text-gray-400",
                        p { "Not logged in" }
                        p {
                            class: "mt-2 text-sm",
                            "Go to the home page to log in"
                        }
                    }
                }
            }

            // Theme section
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div {
                    class: "flex items-center justify-between mb-4",
                    h3 {
                        class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "🎨 Theme"
                    }
                    // Settings sync status
                    if auth.is_authenticated {
                        div {
                            class: "flex items-center gap-2 text-sm",
                            if *settings_store::SETTINGS_LOADING.read() {
                                span {
                                    class: "text-gray-500 dark:text-gray-400",
                                    "⏳ Syncing..."
                                }
                            } else if let Some(err) = settings_store::SETTINGS_ERROR.read().as_ref() {
                                span {
                                    class: "text-red-500",
                                    title: "{err}",
                                    "⚠️ Sync failed"
                                }
                            } else {
                                span {
                                    class: "text-green-500",
                                    "✓ Synced via NIP-78"
                                }
                            }
                        }
                    }
                }
                p {
                    class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    if auth.is_authenticated {
                        "Your theme preference is synced across devices using NIP-78"
                    } else {
                        "Login to sync your theme preference across devices"
                    }
                }
                div {
                    class: "flex gap-3",
                    button {
                        class: if matches!(*theme, theme_store::Theme::Light) {
                            "flex-1 px-4 py-3 bg-blue-600 text-white rounded-lg font-medium"
                        } else {
                            "flex-1 px-4 py-3 bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg hover:bg-gray-300 dark:hover:bg-gray-600 transition"
                        },
                        onclick: move |_| theme_store::set_theme(theme_store::Theme::Light),
                        "☀️ Light"
                    }
                    button {
                        class: if matches!(*theme, theme_store::Theme::Dark) {
                            "flex-1 px-4 py-3 bg-blue-600 text-white rounded-lg font-medium"
                        } else {
                            "flex-1 px-4 py-3 bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg hover:bg-gray-300 dark:hover:bg-gray-600 transition"
                        },
                        onclick: move |_| theme_store::set_theme(theme_store::Theme::Dark),
                        "🌙 Dark"
                    }
                    button {
                        class: if matches!(*theme, theme_store::Theme::System) {
                            "flex-1 px-4 py-3 bg-blue-600 text-white rounded-lg font-medium"
                        } else {
                            "flex-1 px-4 py-3 bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg hover:bg-gray-300 dark:hover:bg-gray-600 transition"
                        },
                        onclick: move |_| theme_store::set_theme(theme_store::Theme::System),
                        "💻 System"
                    }
                }
            }

            // Relay management
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                h3 {
                    class: "text-xl font-semibold mb-4 text-gray-900 dark:text-white",
                    "📡 Relay Management"
                }
                p {
                    class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    "Connected to {relays.len()} relay(s)"
                }

                // Add relay form
                div {
                    class: "mb-6 space-y-2",
                    div {
                        class: "flex gap-2",
                        input {
                            class: "flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "wss://relay.example.com",
                            value: "{new_relay_url}",
                            oninput: move |evt| new_relay_url.set(evt.value())
                        }
                        button {
                            class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition",
                            onclick: add_relay,
                            "Add Relay"
                        }
                    }
                    if let Some(err) = relay_error.read().as_ref() {
                        div {
                            class: "p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                            "❌ {err}"
                        }
                    }
                }

                // Relay list
                div {
                    class: "space-y-2",
                    if relays.is_empty() {
                        div {
                            class: "text-center p-8 text-gray-500 dark:text-gray-400",
                            "No relays connected"
                        }
                    } else {
                        for relay in relays.iter() {
                            div {
                                key: "{relay.url}",
                                class: "flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                div {
                                    class: "flex items-center gap-3 flex-1",
                                    span {
                                        class: match relay.status {
                                            nostr_client::RelayStatus::Connected => "w-3 h-3 rounded-full bg-green-500",
                                            nostr_client::RelayStatus::Connecting => "w-3 h-3 rounded-full bg-yellow-500 animate-pulse",
                                            nostr_client::RelayStatus::Disconnected => "w-3 h-3 rounded-full bg-gray-400",
                                            nostr_client::RelayStatus::Error(_) => "w-3 h-3 rounded-full bg-red-500",
                                        }
                                    }
                                    div {
                                        class: "flex-1",
                                        p {
                                            class: "font-mono text-sm text-gray-900 dark:text-white",
                                            "{relay.url}"
                                        }
                                        p {
                                            class: "text-xs text-gray-500 dark:text-gray-400",
                                            match &relay.status {
                                                nostr_client::RelayStatus::Connected => "Connected",
                                                nostr_client::RelayStatus::Connecting => "Connecting...",
                                                nostr_client::RelayStatus::Disconnected => "Disconnected",
                                                nostr_client::RelayStatus::Error(e) => e,
                                            }
                                        }
                                    }
                                }
                                div {
                                    class: "flex gap-2",
                                    // Connect/Disconnect button
                                    if matches!(relay.status, nostr_client::RelayStatus::Connected) {
                                        button {
                                            class: "px-3 py-1 bg-yellow-600 hover:bg-yellow-700 text-white text-xs rounded transition",
                                            onclick: move |_| {
                                                spawn(async move {
                                                    nostr_client::disconnect().await;
                                                });
                                            },
                                            "Disconnect All"
                                        }
                                    } else if matches!(relay.status, nostr_client::RelayStatus::Disconnected) {
                                        button {
                                            class: "px-3 py-1 bg-green-600 hover:bg-green-700 text-white text-xs rounded transition",
                                            onclick: move |_| {
                                                spawn(async move {
                                                    nostr_client::reconnect().await;
                                                });
                                            },
                                            "Reconnect All"
                                        }
                                    }
                                    // Remove button
                                    button {
                                        class: "px-3 py-1 bg-red-600 hover:bg-red-700 text-white text-xs rounded transition",
                                        onclick: {
                                            let url = relay.url.clone();
                                            move |_| remove_relay(url.clone())
                                        },
                                        "Remove"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // About section
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                h3 {
                    class: "text-xl font-semibold mb-4 text-gray-900 dark:text-white",
                    "ℹ️ About"
                }
                div {
                    class: "space-y-2 text-sm text-gray-600 dark:text-gray-400",
                    p {
                        "nostr.blue (Rust Edition)"
                    }
                    p {
                        "Built with ❤️ using Rust, Dioxus, and rust-nostr"
                    }
                    p {
                        class: "pt-2",
                        a {
                            href: "https://github.com/rust-nostr/nostr",
                            target: "_blank",
                            class: "text-blue-600 dark:text-blue-400 hover:underline",
                            "rust-nostr on GitHub →"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn render_account_info() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut show_nsec = use_signal(|| false);
    let _show_npub_export = use_signal(|| false);
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
    let mut copy_status = use_signal(|| None::<String>);

    let copy_to_clipboard = move |_text: String, _label: &str| {
        #[cfg(target_arch = "wasm32")]
        {
            use web_sys::window;

            if let Some(window) = window() {
                let clipboard = window.navigator().clipboard();
                let promise = clipboard.write_text(&_text);
                let label_str = _label.to_string();

                wasm_bindgen_futures::spawn_local(async move {
                    match wasm_bindgen_futures::JsFuture::from(promise).await {
                        Ok(_) => {
                            copy_status.set(Some(format!("{} copied!", label_str)));
                            // Clear after 2 seconds
                            gloo_timers::future::TimeoutFuture::new(2000).await;
                            copy_status.set(None);
                        }
                        Err(_) => {
                            copy_status.set(Some("Failed to copy".to_string()));
                        }
                    }
                });
            }
        }
    };

    rsx! {
        div {
            class: "space-y-4",

            // Public Key
            div {
                class: "p-4 bg-gray-50 dark:bg-gray-700 rounded-lg",
                div {
                    class: "flex items-center justify-between mb-2",
                    p {
                        class: "text-sm font-medium text-gray-600 dark:text-gray-400",
                        "Public Key (npub)"
                    }
                    button {
                        class: "px-3 py-1 text-xs bg-blue-600 hover:bg-blue-700 text-white rounded transition",
                        onclick: move |_| {
                            if let Ok(npub) = auth_store::export_npub() {
                                copy_to_clipboard(npub, "Public key");
                            }
                        },
                        "📋 Copy"
                    }
                }
                if let Some(pubkey) = &auth.pubkey {
                    p {
                        class: "font-mono text-xs text-gray-900 dark:text-white break-all",
                        "{pubkey}"
                    }
                }
            }

            // Private Key (only shown for PrivateKey login method)
            if matches!(auth.login_method, Some(auth_store::LoginMethod::PrivateKey)) {
                div {
                    class: "p-4 bg-yellow-50 dark:bg-yellow-900/20 border-2 border-yellow-300 dark:border-yellow-700 rounded-lg",
                    div {
                        class: "flex items-center justify-between mb-2",
                        p {
                            class: "text-sm font-medium text-yellow-800 dark:text-yellow-300",
                            "⚠️ Private Key (nsec)"
                        }
                        div {
                            class: "flex gap-2",
                            button {
                                class: "px-3 py-1 text-xs bg-yellow-600 hover:bg-yellow-700 text-white rounded transition",
                                onclick: move |_| {
                                    let current = *show_nsec.read();
                                    show_nsec.set(!current);
                                },
                                if *show_nsec.read() { "👁️ Hide" } else { "👁️ Show" }
                            }
                            if *show_nsec.read() {
                                button {
                                    class: "px-3 py-1 text-xs bg-blue-600 hover:bg-blue-700 text-white rounded transition",
                                    onclick: move |_| {
                                        if let Ok(nsec) = auth_store::export_nsec() {
                                            copy_to_clipboard(nsec, "Private key");
                                        }
                                    },
                                    "📋 Copy"
                                }
                            }
                        }
                    }
                    if *show_nsec.read() {
                        if let Ok(nsec) = auth_store::export_nsec() {
                            p {
                                class: "font-mono text-xs text-gray-900 dark:text-white break-all",
                                "{nsec}"
                            }
                        }
                    } else {
                        p {
                            class: "text-xs text-yellow-700 dark:text-yellow-400",
                            "Click 'Show' to reveal your private key. Keep it safe!"
                        }
                    }
                    p {
                        class: "text-xs text-yellow-700 dark:text-yellow-400 mt-2",
                        "⚠️ Never share your private key with anyone!"
                    }
                }
            }

            // Login Method
            div {
                class: "p-4 bg-gray-50 dark:bg-gray-700 rounded-lg",
                p {
                    class: "text-sm font-medium text-gray-600 dark:text-gray-400 mb-2",
                    "Login Method"
                }
                p {
                    class: "text-gray-900 dark:text-white flex items-center gap-2",
                    match auth_store::get_login_method() {
                        Some(auth_store::LoginMethod::PrivateKey) => "🔑 Private Key (nsec)",
                        Some(auth_store::LoginMethod::ReadOnly) => "👁️ Read-Only (npub)",
                        Some(auth_store::LoginMethod::BrowserExtension) => "🔌 Browser Extension (NIP-07)",
                        None => "Unknown",
                    }
                }
            }

            // Copy status
            if let Some(status) = copy_status.read().as_ref() {
                div {
                    class: "p-3 bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200 rounded-lg text-sm text-center",
                    "✅ {status}"
                }
            }

            // Logout button
            button {
                class: "w-full px-4 py-3 bg-red-600 hover:bg-red-700 text-white rounded-lg font-medium transition",
                onclick: move |_| auth_store::logout(),
                "🚪 Logout"
            }
        }
    }
}
