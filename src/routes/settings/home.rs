use crate::components::{NwcSetupModal, ReactionDefaultsModal};
use crate::platform::storage;
use crate::routes::Route;
use crate::stores::blossom_store::BlossomServersStoreStoreExt;
use crate::stores::{
    auth_store, blossom_store, nostr_client, nwc_store, reactions_store, relay, settings_store,
    sync_store, theme_store,
};
use crate::utils::{format_relative_time_or, time::format_relative_time_ex};
use dioxus::prelude::*;
use nostr_sdk::{Timestamp, ToBech32};
#[component]
pub fn Settings() -> Element {
    let theme = theme_store::THEME.read();
    let blossom_servers = blossom_store::BLOSSOM_SERVERS.read();
    let mut new_server_input = use_signal(String::new);
    let mut server_error = use_signal(|| None::<String>);
    let mut show_nwc_modal = use_signal(|| false);
    let nwc_status = nwc_store::NWC_STATUS.read().clone();
    let nwc_balance = *nwc_store::NWC_BALANCE.read();
    let mut show_reactions_modal = use_signal(|| false);
    use_effect(move || {
        let is_authenticated = auth_store::AUTH_STATE.peek().is_authenticated;
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.peek();
        if is_authenticated && client_initialized {
            spawn(async move {
                log::info!("Loading settings from Nostr (NIP-78)...");
                if let Err(e) = settings_store::load_settings().await {
                    log::error!("Failed to load settings: {}", e);
                }
            });
        }
    });
    let auth = auth_store::AUTH_STATE.read();
    let mut blossom_save_status = use_signal(|| None::<String>);
    let add_blossom_server = move |_| {
        let server_url = new_server_input.read().clone();
        if server_url.is_empty() {
            server_error.set(Some("Please enter a server URL".to_string()));
            return;
        }
        if !server_url.starts_with("https://") && !server_url.starts_with("http://") {
            server_error.set(Some(
                "Server URL must start with http:// or https://".to_string(),
            ));
            return;
        }
        blossom_store::add_server(server_url);
        new_server_input.set(String::new());
        server_error.set(None);
    };
    let remove_blossom_server = move |url: String| {
        blossom_store::remove_server(&url);
    };
    let publish_blossom_servers = move |_| {
        spawn(async move {
            blossom_save_status.set(Some("Publishing...".to_string()));
            match blossom_store::publish_user_servers().await {
                Ok(_) => {
                    blossom_save_status.set(Some("✅ Blossom servers published!".to_string()));
                    crate::platform::timer::sleep_ms(3000).await;
                    blossom_save_status.set(None);
                }
                Err(e) => {
                    blossom_save_status.set(Some(format!("❌ Failed: {}", e)));
                    crate::platform::timer::sleep_ms(7000).await;
                    blossom_save_status.set(None);
                }
            }
        });
    };
    rsx! {
        div { class: "space-y-6",
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                h2 { class: "text-2xl font-semibold text-gray-900 dark:text-white flex items-center gap-2",
                    crate::components::icons::SettingsIcon { class: "w-7 h-7" }
                    "Settings"
                }
                p { class: "text-gray-600 dark:text-gray-400 mt-2",
                    "Manage your account, relays, and preferences"
                }
            }
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                h3 { class: "text-xl font-semibold mb-4 text-gray-900 dark:text-white flex items-center gap-2",
                    crate::components::icons::UserIcon { class: "w-6 h-6" }
                    "Account"
                }
                if auth.is_authenticated {
                    render_account_info {}
                } else {
                    div { class: "text-center p-6 text-gray-500 dark:text-gray-400",
                        p { "Not logged in" }
                        p { class: "mt-2 text-sm", "Go to the home page to log in" }
                    }
                }
            }
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "🎨 Theme"
                    }
                    if auth.is_authenticated {
                        div { class: "flex items-center gap-2 text-sm",
                            if *settings_store::SETTINGS_LOADING.read() {
                                span { class: "text-gray-500 dark:text-gray-400", "⏳ Syncing..." }
                            } else if let Some(err) = settings_store::SETTINGS_ERROR.read().as_ref() {
                                span { class: "text-red-500", title: "{err}", "⚠️ Sync failed" }
                            } else {
                                span { class: "text-green-500", "✓ Synced via NIP-78" }
                            }
                        }
                    }
                }
                p { class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    if auth.is_authenticated {
                        "Your theme preference is synced across devices using NIP-78"
                    } else {
                        "Login to sync your theme preference across devices"
                    }
                }
                div { class: "flex gap-3",
                    button {
                        class: if matches!(*theme, theme_store::Theme::Light) { "flex-1 px-4 py-3 bg-blue-600 text-white rounded-lg font-medium" } else { "flex-1 px-4 py-3 bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg hover:bg-gray-300 dark:hover:bg-gray-600 transition" },
                        onclick: move |_| theme_store::set_theme(theme_store::Theme::Light),
                        "☀️ Light"
                    }
                    button {
                        class: if matches!(*theme, theme_store::Theme::Dark) { "flex-1 px-4 py-3 bg-blue-600 text-white rounded-lg font-medium" } else { "flex-1 px-4 py-3 bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg hover:bg-gray-300 dark:hover:bg-gray-600 transition" },
                        onclick: move |_| theme_store::set_theme(theme_store::Theme::Dark),
                        "🌙 Dark"
                    }
                    button {
                        class: if matches!(*theme, theme_store::Theme::System) { "flex-1 px-4 py-3 bg-blue-600 text-white rounded-lg font-medium" } else { "flex-1 px-4 py-3 bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg hover:bg-gray-300 dark:hover:bg-gray-600 transition" },
                        onclick: move |_| theme_store::set_theme(theme_store::Theme::System),
                        "💻 System"
                    }
                }
            }
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "✨ AI"
                    }
                    span { class: "text-xs text-gray-500 dark:text-gray-400", "Local" }
                }
                p { class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    "Manage AI providers, local model preferences, and chat persistence settings for this device."
                }
                Link {
                    to: Route::SettingsAi {},
                    class: "flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-700 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-600 transition",
                    div { class: "flex items-center gap-3",
                        span { class: "text-2xl", "🤖" }
                        div {
                            span { class: "block font-medium text-gray-900 dark:text-white",
                                "Manage AI Settings"
                            }
                            span { class: "block text-xs text-gray-500 dark:text-gray-400",
                                "Providers, API keys, and model defaults"
                            }
                        }
                    }
                    span { class: "text-gray-400", "→" }
                }
            }
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "😊 Default Reactions"
                    }
                    span { class: "text-xs text-gray-500 dark:text-gray-400", "NIP-78" }
                }
                p { class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    if auth.is_authenticated {
                        "Customize your preferred reaction emojis. The first emoji is used when you click the heart button."
                    } else {
                        "Login to customize and sync your reaction preferences across devices."
                    }
                }
                div { class: "flex flex-wrap gap-2 p-3 bg-muted rounded-lg mb-4",
                    for reaction in reactions_store::PREFERRED_REACTIONS.read().iter().take(10) {
                        match reaction {
                            reactions_store::PreferredReaction::Standard { emoji } => rsx! {
                                span { class: "text-2xl", "{emoji}" }
                            },
                            reactions_store::PreferredReaction::Custom { shortcode, url } => rsx! {
                                if url.is_empty() {
                                    span { class: "text-sm text-gray-500", ":{shortcode}:" }
                                } else {
                                    img {
                                        class: "w-7 h-7 object-contain",
                                        src: "{url}",
                                        alt: ":{shortcode}:",
                                        loading: "lazy",
                                    }
                                }
                            },
                        }
                    }
                }
                button {
                    class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                    disabled: !auth.is_authenticated,
                    onclick: move |_| show_reactions_modal.set(true),
                    "✏️ Edit Defaults"
                }
                if auth.is_authenticated {
                    div { class: "mt-3 text-xs text-gray-500 dark:text-gray-400",
                        if reactions_store::REACTIONS_STATE.read().is_loading() {
                            "⏳ Loading..."
                        } else if reactions_store::REACTIONS_STATE.read().is_ready() {
                            "✓ Synced via NIP-78"
                        } else {
                            ""
                        }
                    }
                }
            }
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "🔔 Notification Sync"
                    }
                }
                p { class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    if auth.is_authenticated {
                        "Sync notification read status across devices using NIP-78. "
                        span { class: "text-gray-500 dark:text-gray-500 italic",
                            "Note: Sync data is public on Nostr relays."
                        }
                    } else {
                        "Login to sync notification read status across devices"
                    }
                }
                div { class: "flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        label { class: "relative inline-flex items-center cursor-pointer",
                            input {
                                r#type: "checkbox",
                                class: "sr-only peer",
                                checked: settings_store::SETTINGS.read().sync_notifications,
                                disabled: !auth.is_authenticated,
                                onchange: move |evt| {
                                    let enabled = evt.checked();
                                    spawn(async move {
                                        settings_store::update_notification_sync(enabled).await;
                                    });
                                },
                            }
                            div { class: "w-11 h-6 bg-gray-300 dark:bg-gray-700 peer-focus:outline-hidden peer-focus:ring-4 peer-focus:ring-blue-300 dark:peer-focus:ring-blue-800 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-blue-600" }
                        }
                        span { class: "text-sm font-medium text-gray-900 dark:text-white",
                            {
                                let is_enabled = settings_store::SETTINGS.read().sync_notifications;
                                if is_enabled { "Enabled" } else { "Disabled" }
                            }
                        }
                    }
                    {
                        let sync_enabled = settings_store::SETTINGS.read().sync_notifications;
                        if auth.is_authenticated && sync_enabled {
                            rsx! {
                                span { class: "text-xs text-green-500", "✓ Syncing" }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                }
            }
            NegentropySyncSection {}
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "🏷️ Client Tag"
                    }
                    span { class: "text-xs text-gray-500 dark:text-gray-400", "NIP-89" }
                }
                p { class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    if auth.is_authenticated {
                        "Add a standardized client tag to events published by nostr.blue for interoperability with other clients."
                    } else {
                        "Login to control whether nostr.blue adds a client tag to events you publish."
                    }
                }
                div { class: "flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        label { class: "relative inline-flex items-center cursor-pointer",
                            input {
                                r#type: "checkbox",
                                class: "sr-only peer",
                                aria_label: "Publish client tag",
                                checked: settings_store::SETTINGS.read().publish_client_tag,
                                disabled: !auth.is_authenticated || *settings_store::SETTINGS_LOADING.read(),
                                onchange: move |evt| {
                                    if *settings_store::SETTINGS_LOADING.read() {
                                        return;
                                    }
                                    let enabled = evt.checked();
                                    spawn(async move {
                                        settings_store::update_publish_client_tag(enabled).await;
                                    });
                                },
                            }
                            div { class: "w-11 h-6 bg-gray-300 dark:bg-gray-700 peer-focus:outline-hidden peer-focus:ring-4 peer-focus:ring-blue-300 dark:peer-focus:ring-blue-800 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-blue-600" }
                        }
                        span { class: "text-sm font-medium text-gray-900 dark:text-white",
                            "Publish client tag"
                        }
                        span { class: "text-sm text-gray-500 dark:text-gray-400",
                            if settings_store::SETTINGS.read().publish_client_tag {
                                "Enabled"
                            } else {
                                "Disabled"
                            }
                        }
                    }
                }
            }
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "⚡ Nostr Wallet Connect"
                    }
                    span { class: "text-xs text-gray-500 dark:text-gray-400", "NIP-47" }
                }
                p { class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    "Connect your lightning wallet to enable instant zaps and payments."
                }
                match &nwc_status {
                    nwc_store::ConnectionStatus::Connected => {
                        rsx! {
                            div { class: "space-y-4",
                                div { class: "p-4 bg-green-50 dark:bg-green-900/20 border border-green-200
                                                                                                                                                                                                                                                                                                                                                                                                                                                    dark:border-green-800 rounded-lg",
                                    div { class: "flex items-center gap-2 mb-2",
                                        span { class: "text-sm font-medium text-green-800 dark:text-green-200",
                                            "✓ Wallet Connected"
                                        }
                                    }
                                    if let Some(balance_msats) = nwc_balance {
                                        div { class: "flex items-center justify-between",
                                            span { class: "text-xs text-gray-600 dark:text-gray-400", "Balance:" }
                                            span { class: "text-sm font-mono text-gray-900 dark:text-white",
                                                {format!("{} sats", balance_msats / 1000)}
                                            }
                                        }
                                    }
                                }
                                div { class: "flex gap-3",
                                    button {
                                        class: "px-4 py-2 text-sm bg-muted
                                                                                                                                                                                                                                                                                                                                                                                                                                                        text-foreground rounded-lg
                                                                                                                                                                                                                                                                                                                                                                                                                                                        hover:bg-accent transition-colors",
                                        onclick: move |_| {
                                            spawn(async move {
                                                let _ = nwc_store::refresh_balance().await;
                                            });
                                        },
                                        "Refresh Balance"
                                    }
                                    button {
                                        class: "px-4 py-2 text-sm bg-red-100 dark:bg-red-900/30
                                                                                                                                                                                                                                                                                                                                                                                                                                                        text-red-700 dark:text-red-300 rounded-lg
                                                                                                                                                                                                                                                                                                                                                                                                                                                        hover:bg-red-200 dark:hover:bg-red-900/50 transition-colors",
                                        onclick: move |_| {
                                            nwc_store::disconnect_nwc(false);
                                        },
                                        "Disconnect"
                                    }
                                }
                            }
                        }
                    }
                    nwc_store::ConnectionStatus::Connecting => {
                        rsx! {
                            div { class: "p-4 bg-blue-50 dark:bg-blue-900/20 border border-blue-200
                                                                                                                                                                                                                                                                                                                                                                                                                                                dark:border-blue-800 rounded-lg",
                                p { class: "text-sm text-blue-800 dark:text-blue-200", "Connecting to wallet..." }
                            }
                        }
                    }
                    nwc_store::ConnectionStatus::Error(error) => {
                        rsx! {
                            div { class: "space-y-4",
                                div { class: "p-4 bg-red-50 dark:bg-red-900/20 border border-red-200
                                                                                                                                                                                                                                                                                                                                                                                                                                                    dark:border-red-800 rounded-lg",
                                    p { class: "text-sm text-red-800 dark:text-red-200", "Connection error: {error}" }
                                }
                                button {
                                    class: "px-4 py-2 text-sm bg-purple-600 text-white rounded-lg
                                                                                                                                                                                                                                                                                                                                                                                                                                                    hover:bg-purple-700 transition-colors",
                                    onclick: move |_| show_nwc_modal.set(true),
                                    "Connect Wallet"
                                }
                            }
                        }
                    }
                    nwc_store::ConnectionStatus::Disconnected => {
                        rsx! {
                            button {
                                class: "px-4 py-2 text-sm bg-purple-600 text-white rounded-lg
                                                                                                                                                                                                                                                                                                                                                                                                                                                hover:bg-purple-700 transition-colors",
                                onclick: move |_| show_nwc_modal.set(true),
                                "Connect Wallet"
                            }
                        }
                    }
                }
                div { class: "mt-6 pt-6 border-t border-gray-200 dark:border-gray-700",
                    div { class: "flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-700/50 rounded-lg",
                        div {
                            div { class: "text-sm font-medium text-gray-900 dark:text-white",
                                "Auto-load Cashu Wallet"
                            }
                            p { class: "text-xs text-gray-600 dark:text-gray-400 mt-1",
                                "Initialize Cashu wallet automatically on app startup for ecash payments"
                            }
                        }
                        label { class: "relative inline-flex items-center cursor-pointer",
                            input {
                                r#type: "checkbox",
                                class: "sr-only peer",
                                checked: settings_store::SETTINGS.read().cashu_wallet_auto_load,
                                onchange: move |evt| {
                                    let enabled = evt.checked();
                                    spawn(async move {
                                        settings_store::update_cashu_wallet_auto_load(enabled).await;
                                        if !enabled {
                                            let current_pref = settings_store::SETTINGS
                                                .read()
                                                .payment_method_preference
                                                .clone();
                                            if current_pref == "cashu_first" {
                                                let new_pref = if nwc_store::is_connected() {
                                                    "nwc_first"
                                                } else {
                                                    "always_ask"
                                                };
                                                settings_store::update_payment_method_preference(
                                                        new_pref.to_string(),
                                                    )
                                                    .await;
                                                log::info!(
                                                    "Reset payment preference from cashu_first to {}", new_pref
                                                );
                                            }
                                        }
                                    });
                                },
                            }
                            div { class: "w-11 h-6 bg-gray-300 dark:bg-gray-600 peer-focus:outline-hidden peer-focus:ring-4 peer-focus:ring-blue-300 dark:peer-focus:ring-blue-800 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-blue-600" }
                        }
                    }
                }
                if matches!(nwc_status, nwc_store::ConnectionStatus::Connected)
                    || settings_store::SETTINGS.read().cashu_wallet_auto_load
                {
                    div { class: "mt-6 pt-6 border-t border-gray-200 dark:border-gray-700",
                        h4 { class: "text-sm font-medium text-gray-900 dark:text-white mb-3",
                            "Payment Method Preference"
                        }
                        p { class: "text-xs text-gray-600 dark:text-gray-400 mb-3",
                            "Choose how you want to pay when zapping content"
                        }
                        div { class: "space-y-2",
                            if settings_store::SETTINGS.read().cashu_wallet_auto_load {
                                label { class: "flex items-start gap-3 p-3 bg-gray-50 dark:bg-gray-700/50 rounded-lg cursor-pointer
                                            hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors",
                                    input {
                                        r#type: "radio",
                                        name: "payment_method",
                                        value: "cashu_first",
                                        checked: settings_store::SETTINGS.read().payment_method_preference == "cashu_first",
                                        onchange: move |_| {
                                            spawn(async move {
                                                settings_store::update_payment_method_preference("cashu_first".to_string())
                                                    .await;
                                            });
                                        },
                                    }
                                    div {
                                        div { class: "text-sm font-medium text-gray-900 dark:text-white",
                                            "Cashu First (Nutzaps)"
                                        }
                                        p { class: "text-xs text-gray-600 dark:text-gray-400 mt-1",
                                            "Send ecash via Nostr if recipient supports, fallback to Lightning"
                                        }
                                    }
                                }
                            }
                            if matches!(nwc_status, nwc_store::ConnectionStatus::Connected) {
                                label { class: "flex items-start gap-3 p-3 bg-gray-50 dark:bg-gray-700/50 rounded-lg cursor-pointer
                                            hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors",
                                    input {
                                        r#type: "radio",
                                        name: "payment_method",
                                        value: "nwc_first",
                                        checked: settings_store::SETTINGS.read().payment_method_preference == "nwc_first",
                                        onchange: move |_| {
                                            spawn(async move {
                                                settings_store::update_payment_method_preference("nwc_first".to_string())
                                                    .await;
                                            });
                                        },
                                    }
                                    div {
                                        div { class: "text-sm font-medium text-gray-900 dark:text-white",
                                            "NWC First (Recommended)"
                                        }
                                        p { class: "text-xs text-gray-600 dark:text-gray-400 mt-1",
                                            "Try NWC, fallback to WebLN, then show invoice"
                                        }
                                    }
                                }
                            }
                            label { class: "flex items-start gap-3 p-3 bg-gray-50 dark:bg-gray-700/50 rounded-lg cursor-pointer
                                        hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors",
                                input {
                                    r#type: "radio",
                                    name: "payment_method",
                                    value: "webln_first",
                                    checked: settings_store::SETTINGS.read().payment_method_preference == "webln_first",
                                    onchange: move |_| {
                                        spawn(async move {
                                            settings_store::update_payment_method_preference("webln_first".to_string())
                                                .await;
                                        });
                                    },
                                }
                                div {
                                    div { class: "text-sm font-medium text-gray-900 dark:text-white",
                                        "WebLN First"
                                    }
                                    p { class: "text-xs text-gray-600 dark:text-gray-400 mt-1",
                                        "Try WebLN extension, fallback to NWC, then show invoice"
                                    }
                                }
                            }
                            label { class: "flex items-start gap-3 p-3 bg-gray-50 dark:bg-gray-700/50 rounded-lg cursor-pointer
                                        hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors",
                                input {
                                    r#type: "radio",
                                    name: "payment_method",
                                    value: "always_ask",
                                    checked: settings_store::SETTINGS.read().payment_method_preference == "always_ask",
                                    onchange: move |_| {
                                        spawn(async move {
                                            settings_store::update_payment_method_preference("always_ask".to_string())
                                                .await;
                                        });
                                    },
                                }
                                div {
                                    div { class: "text-sm font-medium text-gray-900 dark:text-white",
                                        "Always Ask"
                                    }
                                    p { class: "text-xs text-gray-600 dark:text-gray-400 mt-1",
                                        "Show payment method selector each time"
                                    }
                                }
                            }
                            label { class: "flex items-start gap-3 p-3 bg-gray-50 dark:bg-gray-700/50 rounded-lg cursor-pointer
                                        hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors",
                                input {
                                    r#type: "radio",
                                    name: "payment_method",
                                    value: "manual_only",
                                    checked: settings_store::SETTINGS.read().payment_method_preference == "manual_only",
                                    onchange: move |_| {
                                        spawn(async move {
                                            settings_store::update_payment_method_preference("manual_only".to_string())
                                                .await;
                                        });
                                    },
                                }
                                div {
                                    div { class: "text-sm font-medium text-gray-900 dark:text-white",
                                        "Manual Only"
                                    }
                                    p { class: "text-xs text-gray-600 dark:text-gray-400 mt-1",
                                        "Always show QR code and invoice (no auto-payment)"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div { class: "flex items-center justify-between mb-4",
                        h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                            "🛡️ Content Moderation"
                        }
                        span { class: "text-xs text-gray-500 dark:text-gray-400", "NIP-51 & NIP-56" }
                    }
                    p { class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                        "Manage blocked users and muted posts"
                    }
                    div { class: "space-y-2",
                        Link {
                            to: Route::SettingsBlocklist {},
                            class: "flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-700 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-600 transition",
                            div { class: "flex items-center gap-3",
                                span { class: "text-lg", "🚫" }
                                div {
                                    span { class: "block font-medium text-gray-900 dark:text-white",
                                        "Blocked Users"
                                    }
                                    span { class: "block text-xs text-gray-500 dark:text-gray-400",
                                        "Manage users you've blocked"
                                    }
                                }
                            }
                            span { class: "text-gray-400", "→" }
                        }
                        Link {
                            to: Route::SettingsMuted {},
                            class: "flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-700 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-600 transition",
                            div { class: "flex items-center gap-3",
                                span { class: "text-lg", "🔇" }
                                div {
                                    span { class: "block font-medium text-gray-900 dark:text-white",
                                        "Muted Posts"
                                    }
                                    span { class: "block text-xs text-gray-500 dark:text-gray-400",
                                        "Manage posts you've muted or reported"
                                    }
                                }
                            }
                            span { class: "text-gray-400", "→" }
                        }
                    }
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div { class: "flex items-center justify-between mb-4",
                        h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                            "Relay Management"
                        }
                        span { class: "text-xs text-gray-500 dark:text-gray-400", "NIP-65 & NIP-17" }
                    }
                    p { class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                        "Nostr relays are servers that store and distribute your posts. Configure which relays to use for reading content and publishing your notes."
                    }
                    Link {
                        to: Route::SettingsRelays {},
                        class: "flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-700 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-600 transition",
                        div { class: "flex items-center gap-3",
                            span { class: "text-2xl", "📡" }
                            div {
                                span { class: "block font-medium text-gray-900 dark:text-white",
                                    "Manage Relays"
                                }
                                span { class: "block text-xs text-gray-500 dark:text-gray-400",
                                    {
                                        let general_count = relay::USER_RELAY_METADATA
                                            .read()
                                            .as_ref()
                                            .map(|m| m.relays.len())
                                            .unwrap_or(0);
                                        let dm_count = relay::USER_RELAY_METADATA
                                            .read()
                                            .as_ref()
                                            .map(|m| m.dm_relays.len())
                                            .unwrap_or(0);
                                        format!("{} general, {} DM relays configured", general_count, dm_count)
                                    }
                                }
                            }
                        }
                        span { class: "text-gray-400 text-xl", "→" }
                    }
                }
            }
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "🌸 Blossom Servers"
                    }
                    span { class: "text-xs text-gray-500 dark:text-gray-400", "NIP-B7 (kind 10063)" }
                }
                p { class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    "Configure servers for image and media uploads. The first server in the list is used for uploads. "
                    if auth.is_authenticated {
                        "Your server list is synced across devices via Nostr."
                    } else {
                        "Login to sync your server list across devices."
                    }
                }
                div { class: "space-y-2 mb-4",
                    for (index , server) in blossom_servers.data().read().iter().enumerate() {
                        div {
                            key: "{server}",
                            class: "flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                            div { class: "flex items-center gap-2 flex-wrap",
                                if server == blossom_store::DEFAULT_SERVER {
                                    span { class: "px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200 text-xs font-medium rounded",
                                        "Default"
                                    }
                                }
                                if index == 0 {
                                    span { class: "px-2 py-1 bg-purple-100 dark:bg-purple-900 text-purple-800 dark:text-purple-200 text-xs font-medium rounded",
                                        "⭐ Preferred"
                                    }
                                }
                                span { class: "text-gray-900 dark:text-white font-mono text-sm",
                                    "{server}"
                                }
                            }
                            div { class: "flex items-center gap-2",
                                if index != 0 {
                                    button {
                                        class: "px-3 py-1 bg-gray-100 hover:bg-purple-100 dark:bg-gray-600 dark:hover:bg-purple-800 text-gray-700 dark:text-gray-200 rounded-lg text-sm transition",
                                        onclick: {
                                            let server = server.clone();
                                            move |_| blossom_store::set_as_preferred(&server)
                                        },
                                        "Set as Preferred"
                                    }
                                }
                                if blossom_servers.data().read().len() > 1 {
                                    button {
                                        class: "px-3 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded-lg text-sm transition",
                                        onclick: {
                                            let server = server.clone();
                                            move |_| remove_blossom_server(server.clone())
                                        },
                                        "Remove"
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "space-y-2",
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "url",
                            placeholder: "https://your-blossom-server.com",
                            value: "{new_server_input}",
                            oninput: move |evt| new_server_input.set(evt.value()),
                        }
                        button {
                            class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition",
                            onclick: add_blossom_server,
                            "Add Server"
                        }
                    }
                    if let Some(err) = server_error.read().as_ref() {
                        div { class: "p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                            "❌ {err}"
                        }
                    }
                }
                if auth.is_authenticated {
                    div { class: "pt-4 border-t border-gray-200 dark:border-gray-700 mt-4",
                        button {
                            class: "w-full px-6 py-3 bg-green-600 hover:bg-green-700 text-white rounded-lg font-medium transition",
                            onclick: publish_blossom_servers,
                            "📤 Publish Server List to Nostr"
                        }
                        if let Some(status) = blossom_save_status.read().as_ref() {
                            div { class: "mt-3 p-3 bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200 rounded text-sm text-center",
                                "{status}"
                            }
                        }
                    }
                }
            }
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "₿ Bitcoin Settings"
                    }
                    span { class: "text-xs text-gray-500 dark:text-gray-400", "NIP-73 Content" }
                }
                p { class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    "Configure how Bitcoin transactions and addresses are displayed. "
                    "Uses mempool.space API for transaction data. You can use your own self-hosted instance for privacy."
                }
                BitcoinSettingsSection {}
            }
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                h3 { class: "text-xl font-semibold mb-4 text-gray-900 dark:text-white",
                    "ℹ️ About"
                }
                div { class: "space-y-2 text-sm text-gray-600 dark:text-gray-400",
                    p { "nostr.blue (Rust Edition) with NIP-65 Outbox Model" }
                    p { "Built with ❤️ using Rust, Dioxus, and rust-nostr" }
                    p { class: "pt-2",
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
        if *show_nwc_modal.read() {
            NwcSetupModal { on_close: move |_| show_nwc_modal.set(false) }
        }
        if *show_reactions_modal.read() {
            ReactionDefaultsModal { on_close: move |_| show_reactions_modal.set(false) }
        }
    }
}
#[component]
fn render_account_info() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut show_nsec = use_signal(|| false);
    let _show_npub_export = use_signal(|| false);
    #[cfg_attr(not(feature = "web"), allow(unused_mut))]
    let mut copy_status = use_signal(|| None::<String>);
    let mut logout_error = use_signal(|| None::<String>);
    let copy_to_clipboard = move |_text: String, _label: &str| {
        #[cfg(feature = "web")]
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
                            crate::platform::timer::sleep_ms(2000).await;
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
        div { class: "space-y-4",
            div { class: "p-4 bg-gray-50 dark:bg-gray-700 rounded-lg",
                div { class: "flex items-center justify-between mb-2",
                    p { class: "text-sm font-medium text-gray-600 dark:text-gray-400",
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
                    p { class: "font-mono text-xs text-gray-900 dark:text-white break-all",
                        "{pubkey}"
                    }
                }
            }
            if matches!(auth.login_method, Some(auth_store::LoginMethod::PrivateKey)) {
                div { class: "p-4 bg-yellow-50 dark:bg-yellow-900/20 border-2 border-yellow-300 dark:border-yellow-700 rounded-lg",
                    div { class: "flex items-center justify-between mb-2",
                        p { class: "text-sm font-medium text-yellow-800 dark:text-yellow-300",
                            "⚠️ Private Key (nsec)"
                        }
                        div { class: "flex gap-2",
                            button {
                                class: "px-3 py-1 text-xs bg-yellow-600 hover:bg-yellow-700 text-white rounded transition",
                                onclick: move |_| {
                                    let current = *show_nsec.read();
                                    show_nsec.set(!current);
                                },
                                if *show_nsec.read() {
                                    "👁️ Hide"
                                } else {
                                    "👁️ Show"
                                }
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
                            p { class: "font-mono text-xs text-gray-900 dark:text-white break-all",
                                "{nsec}"
                            }
                        }
                    } else {
                        p { class: "text-xs text-yellow-700 dark:text-yellow-400",
                            "Click 'Show' to reveal your private key. Keep it safe!"
                        }
                    }
                    p { class: "text-xs text-yellow-700 dark:text-yellow-400 mt-2",
                        "⚠️ Never share your private key with anyone!"
                    }
                }
            }
            if matches!(auth.login_method, Some(auth_store::LoginMethod::RemoteSigner)) {
                div { class: "p-4 bg-blue-50 dark:bg-blue-900/20 border-2 border-blue-300 dark:border-blue-700 rounded-lg space-y-3",
                    div {
                        div { class: "flex items-center justify-between mb-2",
                            p { class: "text-sm font-medium text-blue-800 dark:text-blue-300",
                                "🔐 Bunker URI"
                            }
                            button {
                                class: "px-3 py-1 text-xs bg-blue-600 hover:bg-blue-700 text-white rounded transition",
                                onclick: move |_| {
                                    if let Ok(uri) = storage::get::<String>("nostr_bunker_uri") {
                                        copy_to_clipboard(uri, "Bunker URI");
                                    }
                                },
                                "📋 Copy"
                            }
                        }
                        if let Ok(uri) = storage::get::<String>("nostr_bunker_uri") {
                            p { class: "font-mono text-xs text-gray-900 dark:text-white break-all",
                                {
                                    if uri.len() > 60 {
                                        format!("{}...{}", &uri[..30], &uri[uri.len() - 25..])
                                    } else {
                                        uri
                                    }
                                }
                            }
                        }
                    }
                    div {
                        div { class: "flex items-center justify-between mb-2",
                            p { class: "text-sm font-medium text-blue-800 dark:text-blue-300",
                                "🔑 App Public Key"
                            }
                            button {
                                class: "px-3 py-1 text-xs bg-blue-600 hover:bg-blue-700 text-white rounded transition",
                                onclick: move |_| {
                                    if let Ok(app_keys_str) = storage::get::<
                                        String,
                                    >("nostr_app_keys") {
                                        if let Ok(keys) = nostr::Keys::parse(&app_keys_str) {
                                            let npub = keys.public_key().to_bech32().unwrap();
                                            copy_to_clipboard(npub, "App public key");
                                        }
                                    }
                                },
                                "📋 Copy"
                            }
                        }
                        if let Ok(app_keys_str) = storage::get::<String>("nostr_app_keys") {
                            if let Ok(keys) = nostr::Keys::parse(&app_keys_str) {
                                p { class: "font-mono text-xs text-gray-900 dark:text-white break-all",
                                    "{keys.public_key().to_bech32().unwrap()}"
                                }
                            }
                        }
                    }
                    p { class: "text-xs text-blue-700 dark:text-blue-400 mt-2",
                        "ℹ️ Your keys are stored on your remote signing device. The app public key is used to authenticate this app to your signer."
                    }
                }
            }
            div { class: "p-4 bg-gray-50 dark:bg-gray-700 rounded-lg",
                p { class: "text-sm font-medium text-gray-600 dark:text-gray-400 mb-2",
                    "Login Method"
                }
                p { class: "text-gray-900 dark:text-white flex items-center gap-2",
                    match auth_store::get_login_method() {
                        Some(auth_store::LoginMethod::PrivateKey) => "🔑 Private Key (nsec)",
                        Some(auth_store::LoginMethod::ReadOnly) => "👁️ Read-Only (npub)",
                        Some(auth_store::LoginMethod::BrowserExtension) => {
                            "🔌 Browser Extension (NIP-07)"
                        }
                        Some(auth_store::LoginMethod::RemoteSigner) => "🔐 Remote Signer (NIP-46)",
                        #[cfg(feature = "mobile")]
                        Some(auth_store::LoginMethod::AndroidSigner) => "📱 Android Signer (NIP-55)",
                        None => "Unknown",
                    }
                }
            }
            if let Some(status) = copy_status.read().as_ref() {
                div { class: "p-3 bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200 rounded-lg text-sm text-center",
                    "✅ {status}"
                }
            }
            button {
                class: "w-full px-4 py-3 bg-red-600 hover:bg-red-700 text-white rounded-lg font-medium transition",
                onclick: move |_| {
                    let nav = navigator();
                    spawn(async move {
                        match auth_store::logout().await {
                            Ok(()) => {
                                logout_error.set(None);
                                nav.push(Route::Home { list: String::new() });
                            }
                            Err(e) => {
                                log::error!("{}", e);
                                logout_error.set(Some(e));
                            }
                        }
                    });
                },
                "🚪 Logout"
            }
            if let Some(error) = logout_error.read().as_ref() {
                div { class: "rounded-lg bg-red-100 p-3 text-sm text-red-700 dark:bg-red-900/30 dark:text-red-300",
                    "{error}"
                }
            }
        }
    }
}

#[component]
fn NegentropySyncSection() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let settings = settings_store::SETTINGS.read().clone();
    let sync_state = sync_store::SYNC_SERVICE_STATE.read().clone();
    let (status_text, status_class) = match sync_state.phase {
        sync_store::SyncPhase::Idle => ("Idle", "text-gray-500 dark:text-gray-400"),
        sync_store::SyncPhase::Waiting => ("Waiting", "text-amber-600 dark:text-amber-400"),
        sync_store::SyncPhase::Running => ("Running", "text-blue-600 dark:text-blue-400"),
        sync_store::SyncPhase::Succeeded => ("Healthy", "text-green-600 dark:text-green-400"),
        sync_store::SyncPhase::Failed => ("Failed", "text-red-600 dark:text-red-400"),
    };
    let next_run = sync_state
        .next_scheduled_at
        .map(|ts| format_relative_time_ex(Timestamp::from(ts), true, true))
        .unwrap_or_else(|| "Not scheduled".to_string());
    let last_success = sync_state
        .last_success_at
        .map(|ts| format_relative_time_or(ts, "just now"))
        .unwrap_or_else(|| "Never".to_string());
    let last_started = sync_state
        .last_started_at
        .map(|ts| format_relative_time_or(ts, "just now"))
        .unwrap_or_else(|| "Never".to_string());

    rsx! {
        div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
            div { class: "flex items-center justify-between mb-4",
                h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                    "🔄 Negentropy Sync"
                }
                span { class: "text-xs text-gray-500 dark:text-gray-400", "NIP-77" }
            }
            p { class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                if auth.is_authenticated {
                    "Keep your following feed and relay list reconciled in the background on startup, reconnect, and a repeating interval."
                } else {
                    "Login to enable background following-feed and relay-list sync."
                }
            }
            div { class: "flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between",
                div { class: "flex items-center gap-3",
                    label { class: "relative inline-flex items-center cursor-pointer",
                        input {
                            r#type: "checkbox",
                            class: "sr-only peer",
                            checked: settings.enable_negentropy_sync,
                            disabled: !auth.is_authenticated,
                            onchange: move |evt| {
                                let enabled = evt.checked();
                                spawn(async move {
                                    settings_store::update_negentropy_sync_enabled(enabled).await;
                                });
                            },
                        }
                        div { class: "w-11 h-6 bg-gray-300 dark:bg-gray-700 peer-focus:outline-hidden peer-focus:ring-4 peer-focus:ring-blue-300 dark:peer-focus:ring-blue-800 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-blue-600" }
                    }
                    span { class: "text-sm font-medium text-gray-900 dark:text-white",
                        if settings.enable_negentropy_sync { "Enabled" } else { "Disabled" }
                    }
                }
                div { class: "flex items-center gap-3",
                    label { class: "text-sm font-medium text-gray-900 dark:text-white",
                        "Interval"
                    }
                    input {
                        class: "w-24 rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 dark:border-gray-600 dark:bg-gray-700 dark:text-white",
                        r#type: "number",
                        min: "1",
                        max: "1440",
                        value: "{settings.negentropy_sync_interval_minutes}",
                        disabled: !auth.is_authenticated,
                        onchange: move |evt| {
                            if let Ok(interval) = evt.value().parse::<u32>() {
                                spawn(async move {
                                    settings_store::update_negentropy_sync_interval_minutes(interval).await;
                                });
                            }
                        },
                    }
                    span { class: "text-sm text-gray-500 dark:text-gray-400", "minutes" }
                }
            }
            div { class: "mt-4 grid gap-3 md:grid-cols-2",
                div { class: "rounded-lg bg-gray-50 dark:bg-gray-700/50 p-4",
                    div { class: "flex items-center justify-between",
                        span { class: "text-sm font-medium text-gray-900 dark:text-white", "Status" }
                        span { class: format!("text-sm font-medium {}", status_class), "{status_text}" }
                    }
                    p { class: "mt-2 text-xs text-gray-600 dark:text-gray-400",
                        {sync_state.waiting_reason.clone().unwrap_or_else(|| "No sync has run yet".to_string())}
                    }
                    if let Some(target) = sync_state.active_target {
                        p { class: "mt-2 text-xs text-gray-500 dark:text-gray-400",
                            "Active target: "
                            {
                                match target {
                                    sync_store::SyncTarget::FollowingFeed => "Following feed",
                                    sync_store::SyncTarget::RelayList => "Relay list",
                                }
                            }
                        }
                    }
                    if let Some(progress) = sync_state.progress {
                        p { class: "mt-2 text-xs text-gray-500 dark:text-gray-400",
                            "Progress: {progress.current}/{progress.total}"
                        }
                    }
                }
                div { class: "rounded-lg bg-gray-50 dark:bg-gray-700/50 p-4 space-y-2",
                    div { class: "flex items-center justify-between text-sm",
                        span { class: "text-gray-600 dark:text-gray-400", "Last success" }
                        span { class: "text-gray-900 dark:text-white", "{last_success}" }
                    }
                    div { class: "flex items-center justify-between text-sm",
                        span { class: "text-gray-600 dark:text-gray-400", "Last start" }
                        span { class: "text-gray-900 dark:text-white", "{last_started}" }
                    }
                    div { class: "flex items-center justify-between text-sm",
                        span { class: "text-gray-600 dark:text-gray-400", "Next run" }
                        span { class: "text-gray-900 dark:text-white", "{next_run}" }
                    }
                }
            }
            if let Some(error) = sync_state.last_error.as_ref() {
                div { class: "mt-4 rounded-lg bg-red-100 p-3 text-sm text-red-700 dark:bg-red-900/30 dark:text-red-300",
                    "Last error: {error}"
                }
            }
            div { class: "mt-4 grid gap-3 xl:grid-cols-2",
                div { class: "rounded-lg border border-gray-200 dark:border-gray-700 p-4",
                    div { class: "flex items-center justify-between mb-2",
                        h4 { class: "text-sm font-semibold text-gray-900 dark:text-white",
                            "Following Feed"
                        }
                        span { class: "text-xs text-gray-500 dark:text-gray-400",
                            {
                                sync_state.following_feed.last_success_at
                                    .map(|ts| format_relative_time_or(ts, "just now"))
                                    .unwrap_or_else(|| "Never".to_string())
                            }
                        }
                    }
                    p { class: "text-xs text-gray-600 dark:text-gray-400",
                        "Received {sync_state.following_feed.received_count} · Local {sync_state.following_feed.local_count} · Remote {sync_state.following_feed.remote_count} · Sent {sync_state.following_feed.sent_count} · Failures {sync_state.following_feed.send_failure_count}"
                    }
                }
                div { class: "rounded-lg border border-gray-200 dark:border-gray-700 p-4",
                    div { class: "flex items-center justify-between mb-2",
                        h4 { class: "text-sm font-semibold text-gray-900 dark:text-white",
                            "Relay List"
                        }
                        span { class: "text-xs text-gray-500 dark:text-gray-400",
                            {
                                sync_state.relay_list.last_success_at
                                    .map(|ts| format_relative_time_or(ts, "just now"))
                                    .unwrap_or_else(|| "Never".to_string())
                            }
                        }
                    }
                    p { class: "text-xs text-gray-600 dark:text-gray-400",
                        "Received {sync_state.relay_list.received_count} · Local {sync_state.relay_list.local_count} · Remote {sync_state.relay_list.remote_count} · Sent {sync_state.relay_list.sent_count} · Failures {sync_state.relay_list.send_failure_count}"
                    }
                }
            }
        }
    }
}
/// Bitcoin settings component for configuring mempool.space endpoint
#[component]
fn BitcoinSettingsSection() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let current_endpoint = settings_store::get_mempool_endpoint();
    let mut endpoint_input = use_signal(|| current_endpoint.clone());
    let mut save_status = use_signal(|| None::<String>);
    let mut is_saving = use_signal(|| false);
    let is_modified = endpoint_input.read().as_str() != current_endpoint.as_str();
    let is_default = current_endpoint == crate::services::mempool::DEFAULT_ENDPOINT;
    let save_endpoint = move |_| {
        let endpoint = endpoint_input.read().clone();
        is_saving.set(true);
        save_status.set(None);
        spawn(async move {
            settings_store::update_mempool_endpoint(endpoint).await;
            is_saving.set(false);
            save_status.set(Some("Mempool endpoint saved".to_string()));
            crate::platform::timer::sleep_ms(3000).await;
            save_status.set(None);
        });
    };
    let reset_to_default = move |_| {
        endpoint_input.set(crate::services::mempool::DEFAULT_ENDPOINT.to_string());
        is_saving.set(true);
        save_status.set(None);
        spawn(async move {
            settings_store::reset_mempool_endpoint().await;
            is_saving.set(false);
            save_status.set(Some("Reset to default".to_string()));
            crate::platform::timer::sleep_ms(3000).await;
            save_status.set(None);
        });
    };
    rsx! {
        div { class: "space-y-4",
            div { class: "p-4 bg-gray-50 dark:bg-gray-700 rounded-lg",
                div { class: "flex items-center justify-between mb-2",
                    p { class: "text-sm font-medium text-gray-600 dark:text-gray-400",
                        "Mempool API Endpoint"
                    }
                    if is_default {
                        span { class: "px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200 text-xs font-medium rounded",
                            "Default"
                        }
                    } else {
                        span { class: "px-2 py-1 bg-purple-100 dark:bg-purple-900 text-purple-800 dark:text-purple-200 text-xs font-medium rounded",
                            "Custom"
                        }
                    }
                }
                div { class: "flex gap-2",
                    input {
                        class: "flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white font-mono text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                        r#type: "url",
                        placeholder: "https://mempool.space/api",
                        value: "{endpoint_input}",
                        oninput: move |evt| endpoint_input.set(evt.value()),
                    }
                }
                p { class: "mt-2 text-xs text-gray-500 dark:text-gray-400",
                    "Enter the base URL for your mempool.space instance (e.g., https://mempool.space/api or https://your-server.com/api)"
                }
            }
            div { class: "flex gap-2",
                if is_modified {
                    button {
                        class: "flex-1 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition disabled:opacity-50",
                        disabled: *is_saving.read(),
                        onclick: save_endpoint,
                        if *is_saving.read() {
                            "Saving..."
                        } else {
                            "Save Endpoint"
                        }
                    }
                }
                if !is_default {
                    button {
                        class: "px-4 py-2 bg-gray-200 hover:bg-gray-300 dark:bg-gray-600 dark:hover:bg-gray-500 text-gray-800 dark:text-white rounded-lg font-medium transition disabled:opacity-50",
                        disabled: *is_saving.read(),
                        onclick: reset_to_default,
                        "Reset to Default"
                    }
                }
            }
            if let Some(status) = save_status.read().as_ref() {
                div { class: "p-3 bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200 rounded-lg text-sm text-center",
                    "✅ {status}"
                }
            }
            if auth.is_authenticated {
                p { class: "text-xs text-gray-500 dark:text-gray-400",
                    "Your mempool endpoint setting is synced across devices via Nostr (NIP-78)."
                }
            }
        }
    }
}
