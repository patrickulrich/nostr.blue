use dioxus::prelude::*;
use dioxus_primitives::tabs::{Tabs, TabList, TabTrigger, TabContent};
use crate::stores::{auth_store, theme_store, nostr_client, settings_store, blossom_store, relay_metadata, nwc_store, reactions_store};
use crate::stores::blossom_store::BlossomServersStoreStoreExt;
use crate::components::{NwcSetupModal, ReactionDefaultsModal};
use crate::routes::Route;
use nostr_sdk::ToBech32;
use gloo_storage::Storage;

#[component]
pub fn Settings() -> Element {
    let theme = theme_store::THEME.read();
    let blossom_servers = blossom_store::BLOSSOM_SERVERS.read();

    // Relay management state - initialize from USER_RELAY_METADATA using peek()
    let mut general_relays = use_signal(|| {
        // Use peek() to avoid reactive tracking during initialization
        relay_metadata::USER_RELAY_METADATA.peek().as_ref()
            .map(|m| m.relays.clone())
            .unwrap_or_else(relay_metadata::default_relays)
    });

    let mut dm_relays = use_signal(|| {
        // Use peek() to avoid reactive tracking during initialization
        relay_metadata::USER_RELAY_METADATA.peek().as_ref()
            .map(|m| m.dm_relays.clone())
            .unwrap_or_else(relay_metadata::default_dm_relays)
    });

    // Watch for changes to USER_RELAY_METADATA and update signals reactively
    use_effect(move || {
        // Use peek() to avoid holding borrows during signal updates
        if let Some(metadata) = relay_metadata::USER_RELAY_METADATA.peek().as_ref() {
            log::info!("Updating with {} general relays and {} DM relays from metadata",
                metadata.relays.len(), metadata.dm_relays.len());
            general_relays.set(metadata.relays.clone());
            dm_relays.set(metadata.dm_relays.clone());
        }
    });

    let mut new_relay_url = use_signal(String::new);
    let mut new_dm_relay_url = use_signal(String::new);
    let mut relay_error = use_signal(|| None::<String>);
    let mut dm_relay_error = use_signal(|| None::<String>);
    let mut save_status = use_signal(|| None::<String>);

    // Relay tab selection state (as ReadSignal for Tabs component)
    let relay_tab: ReadSignal<Option<String>> = use_signal(|| None::<String>).into();

    // Live relay connection info for Connections tab
    let connection_info = use_resource(move || async move {
        nostr_client::get_relay_display_info().await
    });

    let mut new_server_input = use_signal(String::new);
    let mut server_error = use_signal(|| None::<String>);

    // NWC state
    let mut show_nwc_modal = use_signal(|| false);
    let nwc_status = nwc_store::NWC_STATUS.read().clone();
    let nwc_balance = *nwc_store::NWC_BALANCE.read();

    // Reactions modal state
    let mut show_reactions_modal = use_signal(|| false);

    // Load settings from Nostr on mount
    use_effect(move || {
        // Use peek() to avoid holding borrows during async operations
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

    // Normalize relay URL (add wss:// if needed)
    let normalize_relay_url = |input: &str| -> Result<String, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("URL cannot be empty".to_string());
        }

        // Try parsing as-is
        if let Ok(url) = nostr::Url::parse(trimmed) {
            return Ok(url.to_string());
        }

        // Try adding wss:// prefix
        if let Ok(url) = nostr::Url::parse(&format!("wss://{}", trimmed)) {
            return Ok(url.to_string());
        }

        Err("Invalid relay URL".to_string())
    };

    // Strip protocol for display
    let display_relay_url = |url: &str| -> String {
        if let Ok(parsed) = nostr::Url::parse(url) {
            if parsed.scheme() == "wss" && parsed.path() == "/" {
                parsed.host_str().unwrap_or(url).to_string()
            } else {
                format!("{}{}", parsed.host_str().unwrap_or(""), parsed.path())
            }
        } else {
            url.to_string()
        }
    };

    // Format bytes for display (KB, MB, GB)
    let format_bytes = |bytes: usize| -> String {
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    };

    // Add general relay
    let add_general_relay = move |_| {
        let url = new_relay_url.read().clone();
        match normalize_relay_url(&url) {
            Ok(normalized) => {
                // Check for duplicates
                if general_relays.read().iter().any(|r| r.url == normalized) {
                    relay_error.set(Some("Relay already exists".to_string()));
                    return;
                }

                general_relays.write().push(relay_metadata::RelayConfig {
                    url: normalized,
                    read: true,
                    write: true,
                });
                new_relay_url.set(String::new());
                relay_error.set(None);
            }
            Err(e) => {
                relay_error.set(Some(e));
            }
        }
    };

    // Remove general relay
    let mut remove_general_relay = move |index: usize| {
        let mut relays = general_relays.write();
        if index < relays.len() {
            relays.remove(index);
        }
    };

    // Toggle relay read/write
    let mut toggle_relay_read = move |index: usize| {
        let mut relays = general_relays.write();
        if let Some(relay) = relays.get_mut(index) {
            relay.read = !relay.read;
        }
    };

    let mut toggle_relay_write = move |index: usize| {
        let mut relays = general_relays.write();
        if let Some(relay) = relays.get_mut(index) {
            relay.write = !relay.write;
        }
    };

    // Add DM relay
    let add_dm_relay = move |_| {
        let url = new_dm_relay_url.read().clone();
        match normalize_relay_url(&url) {
            Ok(normalized) => {
                // Check for duplicates
                if dm_relays.read().contains(&normalized) {
                    dm_relay_error.set(Some("Relay already exists".to_string()));
                    return;
                }

                dm_relays.write().push(normalized);
                new_dm_relay_url.set(String::new());
                dm_relay_error.set(None);
            }
            Err(e) => {
                dm_relay_error.set(Some(e));
            }
        }
    };

    // Remove DM relay
    let mut remove_dm_relay = move |index: usize| {
        let mut relays = dm_relays.write();
        if index < relays.len() {
            relays.remove(index);
        }
    };

    // Save relay lists to Nostr
    let save_relay_lists = move |_| {
        let general = general_relays.read().clone();
        let dm = dm_relays.read().clone();

        spawn(async move {
            save_status.set(Some("Saving...".to_string()));

            // Get client
            let client = match nostr_client::get_client() {
                Some(c) => c,
                None => {
                    save_status.set(Some("❌ Client not initialized".to_string()));
                    return;
                }
            };

            // Publish kind 10002 (general relays)
            match relay_metadata::publish_relay_list(general.clone(), client.clone()).await {
                Ok(_) => {
                    log::info!("General relay list published");
                }
                Err(e) => {
                    save_status.set(Some(format!("❌ Failed to publish relay list: {}", e)));
                    return;
                }
            }

            // Publish kind 10050 (DM relays)
            match relay_metadata::publish_dm_relay_list(dm.clone(), client.clone()).await {
                Ok(_) => {
                    log::info!("DM relay list published");
                }
                Err(e) => {
                    save_status.set(Some(format!("❌ Failed to publish DM list: {}", e)));
                    return;
                }
            }

            // Update local state
            let mut metadata = relay_metadata::USER_RELAY_METADATA.write();

            // Use JS timestamp for WASM compatibility
            #[cfg(target_arch = "wasm32")]
            let now_secs = (js_sys::Date::now() / 1000.0) as u64;
            #[cfg(not(target_arch = "wasm32"))]
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            *metadata = Some(relay_metadata::RelayListMetadata {
                relays: general,
                dm_relays: dm,
                updated_at: now_secs,
            });

            save_status.set(Some("✅ Relay lists published successfully!".to_string()));

            // Clear message after 3 seconds
            gloo_timers::future::TimeoutFuture::new(3000).await;
            save_status.set(None);
        });
    };

    // Blossom server handlers
    let mut blossom_save_status = use_signal(|| None::<String>);

    let add_blossom_server = move |_| {
        let server_url = new_server_input.read().clone();
        if server_url.is_empty() {
            server_error.set(Some("Please enter a server URL".to_string()));
            return;
        }

        if !server_url.starts_with("https://") && !server_url.starts_with("http://") {
            server_error.set(Some("Server URL must start with http:// or https://".to_string()));
            return;
        }

        blossom_store::add_server(server_url);
        new_server_input.set(String::new());
        server_error.set(None);
    };

    let remove_blossom_server = move |url: String| {
        blossom_store::remove_server(&url);
    };

    // Publish Blossom servers to kind 10063 (NIP-B7)
    let publish_blossom_servers = move |_| {
        spawn(async move {
            blossom_save_status.set(Some("Publishing...".to_string()));

            match blossom_store::publish_user_servers().await {
                Ok(_) => {
                    blossom_save_status.set(Some("✅ Blossom servers published!".to_string()));
                    // Clear after 3 seconds
                    gloo_timers::future::TimeoutFuture::new(3000).await;
                    blossom_save_status.set(None);
                }
                Err(e) => {
                    blossom_save_status.set(Some(format!("❌ Failed: {}", e)));
                    // Auto-clear error after 7 seconds
                    gloo_timers::future::TimeoutFuture::new(7000).await;
                    blossom_save_status.set(None);
                }
            }
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
                    render_account_info {}
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

            // Default Reactions section
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div {
                    class: "flex items-center justify-between mb-4",
                    h3 {
                        class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "😊 Default Reactions"
                    }
                    span {
                        class: "text-xs text-gray-500 dark:text-gray-400",
                        "NIP-78"
                    }
                }
                p {
                    class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    if auth.is_authenticated {
                        "Customize your preferred reaction emojis. The first emoji is used when you click the heart button."
                    } else {
                        "Login to customize and sync your reaction preferences across devices."
                    }
                }

                // Show current reactions preview
                div {
                    class: "flex flex-wrap gap-2 p-3 bg-gray-100 dark:bg-gray-700 rounded-lg mb-4",
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
                                        loading: "lazy"
                                    }
                                }
                            }
                        }
                    }
                }

                button {
                    class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                    disabled: !auth.is_authenticated,
                    onclick: move |_| show_reactions_modal.set(true),
                    "✏️ Edit Defaults"
                }

                // Sync status
                if auth.is_authenticated {
                    div {
                        class: "mt-3 text-xs text-gray-500 dark:text-gray-400",
                        if *reactions_store::REACTIONS_LOADING.read() {
                            "⏳ Loading..."
                        } else if *reactions_store::REACTIONS_LOADED.read() {
                            "✓ Synced via NIP-78"
                        } else {
                            ""
                        }
                    }
                }
            }

            // Notification Sync section
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div {
                    class: "flex items-center justify-between mb-4",
                    h3 {
                        class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "🔔 Notification Sync"
                    }
                }
                p {
                    class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    if auth.is_authenticated {
                        "Sync notification read status across devices using NIP-78. "
                        span {
                            class: "text-gray-500 dark:text-gray-500 italic",
                            "Note: Sync data is public on Nostr relays."
                        }
                    } else {
                        "Login to sync notification read status across devices"
                    }
                }
                div {
                    class: "flex items-center justify-between",
                    div {
                        class: "flex items-center gap-3",
                        label {
                            class: "relative inline-flex items-center cursor-pointer",
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
                                }
                            }
                            div {
                                class: "w-11 h-6 bg-gray-300 dark:bg-gray-700 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-blue-300 dark:peer-focus:ring-blue-800 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-blue-600"
                            }
                        }
                        span {
                            class: "text-sm font-medium text-gray-900 dark:text-white",
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
                                span {
                                    class: "text-xs text-green-500",
                                    "✓ Syncing"
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                }
            }

            // NWC Section
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div {
                    class: "flex items-center justify-between mb-4",
                    h3 {
                        class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "⚡ Nostr Wallet Connect"
                    }
                    span {
                        class: "text-xs text-gray-500 dark:text-gray-400",
                        "NIP-47"
                    }
                }

                p {
                    class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    "Connect your lightning wallet to enable instant zaps and payments."
                }

                // Connection status
                match &nwc_status {
                    nwc_store::ConnectionStatus::Connected => {
                        rsx! {
                            div {
                                class: "space-y-4",

                                // Wallet info
                                div {
                                    class: "p-4 bg-green-50 dark:bg-green-900/20 border border-green-200
                                            dark:border-green-800 rounded-lg",
                                    div {
                                        class: "flex items-center gap-2 mb-2",
                                        span {
                                            class: "text-sm font-medium text-green-800 dark:text-green-200",
                                            "✓ Wallet Connected"
                                        }
                                    }

                                    // Balance display
                                    if let Some(balance_msats) = nwc_balance {
                                        div {
                                            class: "flex items-center justify-between",
                                            span {
                                                class: "text-xs text-gray-600 dark:text-gray-400",
                                                "Balance:"
                                            }
                                            span {
                                                class: "text-sm font-mono text-gray-900 dark:text-white",
                                                {format!("{} sats", balance_msats / 1000)}
                                            }
                                        }
                                    }
                                }

                                // Action buttons
                                div {
                                    class: "flex gap-3",
                                    button {
                                        class: "px-4 py-2 text-sm bg-gray-100 dark:bg-gray-700
                                                text-gray-700 dark:text-gray-300 rounded-lg
                                                hover:bg-gray-200 dark:hover:bg-gray-600 transition-colors",
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
                                            nwc_store::disconnect_nwc();
                                        },
                                        "Disconnect"
                                    }
                                }
                            }
                        }
                    },
                    nwc_store::ConnectionStatus::Connecting => {
                        rsx! {
                            div {
                                class: "p-4 bg-blue-50 dark:bg-blue-900/20 border border-blue-200
                                        dark:border-blue-800 rounded-lg",
                                p {
                                    class: "text-sm text-blue-800 dark:text-blue-200",
                                    "Connecting to wallet..."
                                }
                            }
                        }
                    },
                    nwc_store::ConnectionStatus::Error(error) => {
                        rsx! {
                            div {
                                class: "space-y-4",
                                div {
                                    class: "p-4 bg-red-50 dark:bg-red-900/20 border border-red-200
                                            dark:border-red-800 rounded-lg",
                                    p {
                                        class: "text-sm text-red-800 dark:text-red-200",
                                        "Connection error: {error}"
                                    }
                                }
                                button {
                                    class: "px-4 py-2 text-sm bg-purple-600 text-white rounded-lg
                                            hover:bg-purple-700 transition-colors",
                                    onclick: move |_| show_nwc_modal.set(true),
                                    "Connect Wallet"
                                }
                            }
                        }
                    },
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

                // Payment Method Preference (shown when NWC is connected)
                if matches!(nwc_status, nwc_store::ConnectionStatus::Connected) {
                    div {
                        class: "mt-6 pt-6 border-t border-gray-200 dark:border-gray-700",
                        h4 {
                            class: "text-sm font-medium text-gray-900 dark:text-white mb-3",
                            "Payment Method Preference"
                        }
                        p {
                            class: "text-xs text-gray-600 dark:text-gray-400 mb-3",
                            "Choose how you want to pay when zapping content"
                        }
                        div {
                            class: "space-y-2",

                            // NWC First
                            label {
                                class: "flex items-start gap-3 p-3 bg-gray-50 dark:bg-gray-700/50 rounded-lg cursor-pointer
                                        hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors",
                                input {
                                    r#type: "radio",
                                    name: "payment_method",
                                    value: "nwc_first",
                                    checked: settings_store::SETTINGS.read().payment_method_preference == "nwc_first",
                                    onchange: move |_| {
                                        spawn(async move {
                                            settings_store::update_payment_method_preference("nwc_first".to_string()).await;
                                        });
                                    }
                                }
                                div {
                                    div {
                                        class: "text-sm font-medium text-gray-900 dark:text-white",
                                        "NWC First (Recommended)"
                                    }
                                    p {
                                        class: "text-xs text-gray-600 dark:text-gray-400 mt-1",
                                        "Try NWC, fallback to WebLN, then show invoice"
                                    }
                                }
                            }

                            // WebLN First
                            label {
                                class: "flex items-start gap-3 p-3 bg-gray-50 dark:bg-gray-700/50 rounded-lg cursor-pointer
                                        hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors",
                                input {
                                    r#type: "radio",
                                    name: "payment_method",
                                    value: "webln_first",
                                    checked: settings_store::SETTINGS.read().payment_method_preference == "webln_first",
                                    onchange: move |_| {
                                        spawn(async move {
                                            settings_store::update_payment_method_preference("webln_first".to_string()).await;
                                        });
                                    }
                                }
                                div {
                                    div {
                                        class: "text-sm font-medium text-gray-900 dark:text-white",
                                        "WebLN First"
                                    }
                                    p {
                                        class: "text-xs text-gray-600 dark:text-gray-400 mt-1",
                                        "Try WebLN extension, fallback to NWC, then show invoice"
                                    }
                                }
                            }

                            // Always Ask
                            label {
                                class: "flex items-start gap-3 p-3 bg-gray-50 dark:bg-gray-700/50 rounded-lg cursor-pointer
                                        hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors",
                                input {
                                    r#type: "radio",
                                    name: "payment_method",
                                    value: "always_ask",
                                    checked: settings_store::SETTINGS.read().payment_method_preference == "always_ask",
                                    onchange: move |_| {
                                        spawn(async move {
                                            settings_store::update_payment_method_preference("always_ask".to_string()).await;
                                        });
                                    }
                                }
                                div {
                                    div {
                                        class: "text-sm font-medium text-gray-900 dark:text-white",
                                        "Always Ask"
                                    }
                                    p {
                                        class: "text-xs text-gray-600 dark:text-gray-400 mt-1",
                                        "Show payment method selector each time"
                                    }
                                }
                            }

                            // Manual Only
                            label {
                                class: "flex items-start gap-3 p-3 bg-gray-50 dark:bg-gray-700/50 rounded-lg cursor-pointer
                                        hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors",
                                input {
                                    r#type: "radio",
                                    name: "payment_method",
                                    value: "manual_only",
                                    checked: settings_store::SETTINGS.read().payment_method_preference == "manual_only",
                                    onchange: move |_| {
                                        spawn(async move {
                                            settings_store::update_payment_method_preference("manual_only".to_string()).await;
                                        });
                                    }
                                }
                                div {
                                    div {
                                        class: "text-sm font-medium text-gray-900 dark:text-white",
                                        "Manual Only"
                                    }
                                    p {
                                        class: "text-xs text-gray-600 dark:text-gray-400 mt-1",
                                        "Always show QR code and invoice (no auto-payment)"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Content Moderation section
            if auth.is_authenticated {
                div {
                    class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div {
                        class: "flex items-center justify-between mb-4",
                        h3 {
                            class: "text-xl font-semibold text-gray-900 dark:text-white",
                            "🛡️ Content Moderation"
                        }
                        span {
                            class: "text-xs text-gray-500 dark:text-gray-400",
                            "NIP-51 & NIP-56"
                        }
                    }
                    p {
                        class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                        "Manage blocked users and muted posts"
                    }

                    // Links to sub-pages
                    div {
                        class: "space-y-2",

                        Link {
                            to: Route::SettingsBlocklist {},
                            class: "flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-700 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-600 transition",
                            div {
                                class: "flex items-center gap-3",
                                span {
                                    class: "text-lg",
                                    "🚫"
                                }
                                div {
                                    span {
                                        class: "block font-medium text-gray-900 dark:text-white",
                                        "Blocked Users"
                                    }
                                    span {
                                        class: "block text-xs text-gray-500 dark:text-gray-400",
                                        "Manage users you've blocked"
                                    }
                                }
                            }
                            span {
                                class: "text-gray-400",
                                "→"
                            }
                        }

                        Link {
                            to: Route::SettingsMuted {},
                            class: "flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-700 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-600 transition",
                            div {
                                class: "flex items-center gap-3",
                                span {
                                    class: "text-lg",
                                    "🔇"
                                }
                                div {
                                    span {
                                        class: "block font-medium text-gray-900 dark:text-white",
                                        "Muted Posts"
                                    }
                                    span {
                                        class: "block text-xs text-gray-500 dark:text-gray-400",
                                        "Manage posts you've muted or reported"
                                    }
                                }
                            }
                            span {
                                class: "text-gray-400",
                                "→"
                            }
                        }
                    }
                }
            }

            // Relay Management (NIP-65/NIP-17) with Tabs
            if auth.is_authenticated {
                div {
                    class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div {
                        class: "flex items-center justify-between mb-4",
                        h3 {
                            class: "text-xl font-semibold text-gray-900 dark:text-white",
                            "📡 Relay Management"
                        }
                    }

                    Tabs {
                        value: relay_tab,
                        default_value: "general".to_string(),
                        horizontal: true,

                        TabList {
                            class: "flex border-b border-gray-200 dark:border-gray-700 mb-4",
                            TabTrigger {
                                value: "general".to_string(),
                                index: 0usize,
                                class: "px-4 py-2 text-sm font-medium text-gray-600 dark:text-gray-400 border-b-2 border-transparent data-[state=active]:text-blue-600 data-[state=active]:border-blue-600 dark:data-[state=active]:text-blue-400 dark:data-[state=active]:border-blue-400 transition-colors",
                                "General Relays"
                            }
                            TabTrigger {
                                value: "dm".to_string(),
                                index: 1usize,
                                class: "px-4 py-2 text-sm font-medium text-gray-600 dark:text-gray-400 border-b-2 border-transparent data-[state=active]:text-blue-600 data-[state=active]:border-blue-600 dark:data-[state=active]:text-blue-400 dark:data-[state=active]:border-blue-400 transition-colors",
                                "DM Inbox"
                            }
                            TabTrigger {
                                value: "connections".to_string(),
                                index: 2usize,
                                class: "px-4 py-2 text-sm font-medium text-gray-600 dark:text-gray-400 border-b-2 border-transparent data-[state=active]:text-blue-600 data-[state=active]:border-blue-600 dark:data-[state=active]:text-blue-400 dark:data-[state=active]:border-blue-400 transition-colors",
                                "Connections"
                            }
                        }

                        // General Relays Tab (NIP-65)
                        TabContent {
                            value: "general".to_string(),
                            index: 0usize,
                            id: None::<String>,

                            div {
                                class: "space-y-4",
                                div {
                                    class: "flex items-center justify-between",
                                    p {
                                        class: "text-xs text-gray-500 dark:text-gray-400",
                                        "NIP-65 • Read: fetch content • Write: publish content"
                                    }
                                    button {
                                        class: "text-xs text-blue-600 hover:underline dark:text-blue-400",
                                        onclick: move |_| {
                                            relay_metadata::reset_general_relays_to_default();
                                            general_relays.set(relay_metadata::default_relays());
                                        },
                                        "Reset to Defaults"
                                    }
                                }

                                // Relay list
                                div {
                                    class: "space-y-2",
                                    for (index, relay) in general_relays.read().iter().enumerate() {
                                        div {
                                            key: "{relay.url}",
                                            class: "flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                            div {
                                                class: "flex items-center gap-3 flex-1",
                                                span {
                                                    class: "text-gray-900 dark:text-white font-mono text-sm",
                                                    {display_relay_url(&relay.url)}
                                                }
                                            }
                                            div {
                                                class: "flex items-center gap-2",
                                                button {
                                                    class: if relay.read {
                                                        "px-3 py-1 bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200 rounded text-xs font-medium"
                                                    } else {
                                                        "px-3 py-1 bg-gray-200 text-gray-600 dark:bg-gray-600 dark:text-gray-400 rounded text-xs font-medium"
                                                    },
                                                    onclick: move |_| toggle_relay_read(index),
                                                    if relay.read { "📖 Read" } else { "Read" }
                                                }
                                                button {
                                                    class: if relay.write {
                                                        "px-3 py-1 bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200 rounded text-xs font-medium"
                                                    } else {
                                                        "px-3 py-1 bg-gray-200 text-gray-600 dark:bg-gray-600 dark:text-gray-400 rounded text-xs font-medium"
                                                    },
                                                    onclick: move |_| toggle_relay_write(index),
                                                    if relay.write { "✏️ Write" } else { "Write" }
                                                }
                                                button {
                                                    class: "px-3 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded text-xs transition",
                                                    onclick: move |_| remove_general_relay(index),
                                                    "✕"
                                                }
                                            }
                                        }
                                    }
                                }

                                // Add relay form
                                div {
                                    class: "flex gap-2",
                                    input {
                                        class: "flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                                        r#type: "text",
                                        placeholder: "relay.example.com or wss://relay.example.com",
                                        value: "{new_relay_url}",
                                        oninput: move |evt| new_relay_url.set(evt.value())
                                    }
                                    button {
                                        class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition",
                                        onclick: add_general_relay,
                                        "+ Add"
                                    }
                                }
                                if let Some(err) = relay_error.read().as_ref() {
                                    div {
                                        class: "p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                                        "{err}"
                                    }
                                }
                            }
                        }

                        // DM Inbox Tab (NIP-17)
                        TabContent {
                            value: "dm".to_string(),
                            index: 1usize,
                            id: None::<String>,

                            div {
                                class: "space-y-4",
                                div {
                                    class: "flex items-center justify-between",
                                    p {
                                        class: "text-xs text-gray-500 dark:text-gray-400",
                                        "NIP-17 • Where others send you direct messages"
                                    }
                                    button {
                                        class: "text-xs text-blue-600 hover:underline dark:text-blue-400",
                                        onclick: move |_| {
                                            relay_metadata::reset_dm_relays_to_default();
                                            dm_relays.set(relay_metadata::default_dm_relays());
                                        },
                                        "Reset to Defaults"
                                    }
                                }

                                // DM relay list
                                div {
                                    class: "space-y-2",
                                    for (index, url) in dm_relays.read().iter().enumerate() {
                                        div {
                                            key: "{url}",
                                            class: "flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                            div {
                                                class: "flex items-center gap-3 flex-1",
                                                span {
                                                    class: "text-gray-900 dark:text-white font-mono text-sm",
                                                    "📨 {display_relay_url(url)}"
                                                }
                                            }
                                            button {
                                                class: "px-3 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded text-xs transition",
                                                onclick: move |_| remove_dm_relay(index),
                                                "✕"
                                            }
                                        }
                                    }
                                }

                                // Add DM relay form
                                div {
                                    class: "flex gap-2",
                                    input {
                                        class: "flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                                        r#type: "text",
                                        placeholder: "relay.example.com or wss://relay.example.com",
                                        value: "{new_dm_relay_url}",
                                        oninput: move |evt| new_dm_relay_url.set(evt.value())
                                    }
                                    button {
                                        class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition",
                                        onclick: add_dm_relay,
                                        "+ Add"
                                    }
                                }
                                if let Some(err) = dm_relay_error.read().as_ref() {
                                    div {
                                        class: "p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                                        "{err}"
                                    }
                                }
                            }
                        }

                        // Connections Tab (Live Stats)
                        TabContent {
                            value: "connections".to_string(),
                            index: 2usize,
                            id: None::<String>,

                            div {
                                class: "space-y-4",
                                p {
                                    class: "text-xs text-gray-500 dark:text-gray-400",
                                    "Live connection status and data transfer stats"
                                }

                                match &*connection_info.read() {
                                    Some(relays) if !relays.is_empty() => rsx! {
                                        div {
                                            class: "space-y-2",
                                            for relay in relays.iter() {
                                                div {
                                                    key: "{relay.url}",
                                                    class: "flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                                    div {
                                                        class: "flex items-center gap-3 flex-1",
                                                        // Status indicator
                                                        span {
                                                            class: match relay.status.as_str() {
                                                                "Connected" => "w-3 h-3 rounded-full bg-green-500",
                                                                "Connecting" | "Pending" => "w-3 h-3 rounded-full bg-yellow-500 animate-pulse",
                                                                _ => "w-3 h-3 rounded-full bg-gray-400",
                                                            }
                                                        }
                                                        div {
                                                            class: "flex-1 min-w-0",
                                                            p {
                                                                class: "font-mono text-sm text-gray-900 dark:text-white truncate",
                                                                {display_relay_url(&relay.url)}
                                                            }
                                                            div {
                                                                class: "flex items-center gap-3 text-xs text-gray-500 dark:text-gray-400",
                                                                span { "{relay.status}" }
                                                                if relay.has_read {
                                                                    span { class: "text-green-600 dark:text-green-400", "R" }
                                                                }
                                                                if relay.has_write {
                                                                    span { class: "text-blue-600 dark:text-blue-400", "W" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    // Data stats
                                                    div {
                                                        class: "flex items-center gap-3 text-xs text-gray-500 dark:text-gray-400",
                                                        span { "↓ {format_bytes(relay.bytes_received)}" }
                                                        span { "↑ {format_bytes(relay.bytes_sent)}" }
                                                    }
                                                }
                                            }
                                        }
                                    },
                                    Some(_) => rsx! {
                                        div {
                                            class: "text-center p-8 text-gray-500 dark:text-gray-400",
                                            "No relays connected"
                                        }
                                    },
                                    None => rsx! {
                                        div {
                                            class: "text-center p-8 text-gray-500 dark:text-gray-400",
                                            "Loading..."
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Save button (outside tabs)
                    div {
                        class: "pt-4 border-t border-gray-200 dark:border-gray-700 mt-4",
                        button {
                            class: "w-full px-6 py-3 bg-green-600 hover:bg-green-700 text-white rounded-lg font-medium transition text-lg",
                            onclick: save_relay_lists,
                            "📤 Publish Relay Lists to Nostr"
                        }
                        if let Some(status) = save_status.read().as_ref() {
                            div {
                                class: "mt-3 p-3 bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200 rounded text-sm text-center",
                                "{status}"
                            }
                        }
                        p {
                            class: "text-xs text-gray-500 dark:text-gray-400 mt-3 text-center",
                            "Last synced: "
                            {
                                if let Some(metadata) = relay_metadata::USER_RELAY_METADATA.read().as_ref() {
                                    #[cfg(target_arch = "wasm32")]
                                    let now_secs = (js_sys::Date::now() / 1000.0) as u64;
                                    #[cfg(not(target_arch = "wasm32"))]
                                    let now_secs = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs();

                                    let age = now_secs - metadata.updated_at;
                                    if age < 60 {
                                        "Just now".to_string()
                                    } else if age < 3600 {
                                        format!("{} minutes ago", age / 60)
                                    } else {
                                        format!("{} hours ago", age / 3600)
                                    }
                                } else {
                                    "Never".to_string()
                                }
                            }
                        }
                    }
                }
            }

            // Blossom Servers section
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div {
                    class: "flex items-center justify-between mb-4",
                    h3 {
                        class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "🌸 Blossom Servers"
                    }
                    span {
                        class: "text-xs text-gray-500 dark:text-gray-400",
                        "NIP-B7 (kind 10063)"
                    }
                }

                p {
                    class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    "Configure servers for image and media uploads. The first server in the list is used for uploads. "
                    if auth.is_authenticated {
                        "Your server list is synced across devices via Nostr."
                    } else {
                        "Login to sync your server list across devices."
                    }
                }

                // Server list
                div {
                    class: "space-y-2 mb-4",
                    for (index, server) in blossom_servers.data().read().iter().enumerate() {
                        div {
                            key: "{server}",
                            class: "flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                            div {
                                class: "flex items-center gap-2 flex-wrap",
                                if server == blossom_store::DEFAULT_SERVER {
                                    span {
                                        class: "px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200 text-xs font-medium rounded",
                                        "Default"
                                    }
                                }
                                if index == 0 {
                                    span {
                                        class: "px-2 py-1 bg-purple-100 dark:bg-purple-900 text-purple-800 dark:text-purple-200 text-xs font-medium rounded",
                                        "⭐ Preferred"
                                    }
                                }
                                span {
                                    class: "text-gray-900 dark:text-white font-mono text-sm",
                                    "{server}"
                                }
                            }
                            div {
                                class: "flex items-center gap-2",
                                // Set as Preferred button for non-first servers
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
                                // Remove button (only if more than one server)
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

                // Add new server
                div {
                    class: "space-y-2",
                    div {
                        class: "flex gap-2",
                        input {
                            class: "flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "url",
                            placeholder: "https://your-blossom-server.com",
                            value: "{new_server_input}",
                            oninput: move |evt| new_server_input.set(evt.value())
                        }
                        button {
                            class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition",
                            onclick: add_blossom_server,
                            "Add Server"
                        }
                    }
                    if let Some(err) = server_error.read().as_ref() {
                        div {
                            class: "p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                            "❌ {err}"
                        }
                    }
                }

                // Publish button (only when authenticated)
                if auth.is_authenticated {
                    div {
                        class: "pt-4 border-t border-gray-200 dark:border-gray-700 mt-4",
                        button {
                            class: "w-full px-6 py-3 bg-green-600 hover:bg-green-700 text-white rounded-lg font-medium transition",
                            onclick: publish_blossom_servers,
                            "📤 Publish Server List to Nostr"
                        }
                        if let Some(status) = blossom_save_status.read().as_ref() {
                            div {
                                class: "mt-3 p-3 bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200 rounded text-sm text-center",
                                "{status}"
                            }
                        }
                    }
                }
            }

            // Bitcoin Settings section
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div {
                    class: "flex items-center justify-between mb-4",
                    h3 {
                        class: "text-xl font-semibold text-gray-900 dark:text-white",
                        "₿ Bitcoin Settings"
                    }
                    span {
                        class: "text-xs text-gray-500 dark:text-gray-400",
                        "NIP-73 Content"
                    }
                }

                p {
                    class: "text-sm text-gray-600 dark:text-gray-400 mb-4",
                    "Configure how Bitcoin transactions and addresses are displayed. "
                    "Uses mempool.space API for transaction data. You can use your own self-hosted instance for privacy."
                }

                // Mempool endpoint setting
                BitcoinSettingsSection {}
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
                        "nostr.blue (Rust Edition) with NIP-65 Outbox Model"
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

        // NWC Setup Modal
        if *show_nwc_modal.read() {
            NwcSetupModal {
                on_close: move |_| show_nwc_modal.set(false)
            }
        }

        // Reactions Defaults Modal
        if *show_reactions_modal.read() {
            ReactionDefaultsModal {
                on_close: move |_| show_reactions_modal.set(false)
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

            // Remote Signer Info (only shown for RemoteSigner login method)
            if matches!(auth.login_method, Some(auth_store::LoginMethod::RemoteSigner)) {
                div {
                    class: "p-4 bg-blue-50 dark:bg-blue-900/20 border-2 border-blue-300 dark:border-blue-700 rounded-lg space-y-3",

                    // Bunker URI
                    div {
                        div {
                            class: "flex items-center justify-between mb-2",
                            p {
                                class: "text-sm font-medium text-blue-800 dark:text-blue-300",
                                "🔐 Bunker URI"
                            }
                            button {
                                class: "px-3 py-1 text-xs bg-blue-600 hover:bg-blue-700 text-white rounded transition",
                                onclick: move |_| {
                                    if let Ok(uri) = gloo_storage::LocalStorage::get::<String>("nostr_bunker_uri") {
                                        copy_to_clipboard(uri, "Bunker URI");
                                    }
                                },
                                "📋 Copy"
                            }
                        }
                        if let Ok(uri) = gloo_storage::LocalStorage::get::<String>("nostr_bunker_uri") {
                            p {
                                class: "font-mono text-xs text-gray-900 dark:text-white break-all",
                                {
                                    if uri.len() > 60 {
                                        format!("{}...{}", &uri[..30], &uri[uri.len()-25..])
                                    } else {
                                        uri
                                    }
                                }
                            }
                        }
                    }

                    // App Public Key
                    div {
                        div {
                            class: "flex items-center justify-between mb-2",
                            p {
                                class: "text-sm font-medium text-blue-800 dark:text-blue-300",
                                "🔑 App Public Key"
                            }
                            button {
                                class: "px-3 py-1 text-xs bg-blue-600 hover:bg-blue-700 text-white rounded transition",
                                onclick: move |_| {
                                    if let Ok(app_keys_str) = gloo_storage::LocalStorage::get::<String>("nostr_app_keys") {
                                        if let Ok(keys) = nostr::Keys::parse(&app_keys_str) {
                                            let npub = keys.public_key().to_bech32().unwrap();
                                            copy_to_clipboard(npub, "App public key");
                                        }
                                    }
                                },
                                "📋 Copy"
                            }
                        }
                        if let Ok(app_keys_str) = gloo_storage::LocalStorage::get::<String>("nostr_app_keys") {
                            if let Ok(keys) = nostr::Keys::parse(&app_keys_str) {
                                p {
                                    class: "font-mono text-xs text-gray-900 dark:text-white break-all",
                                    "{keys.public_key().to_bech32().unwrap()}"
                                }
                            }
                        }
                    }

                    p {
                        class: "text-xs text-blue-700 dark:text-blue-400 mt-2",
                        "ℹ️ Your keys are stored on your remote signing device. The app public key is used to authenticate this app to your signer."
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
                        Some(auth_store::LoginMethod::RemoteSigner) => "🔐 Remote Signer (NIP-46)",
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
                onclick: move |_| {
                    let nav = navigator();
                    spawn(async move {
                        auth_store::logout().await;
                        nav.push(Route::Home { list: String::new() });
                    });
                },
                "🚪 Logout"
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

    // Check if endpoint has been modified
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

            // Clear status after 3 seconds
            gloo_timers::future::TimeoutFuture::new(3000).await;
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

            // Clear status after 3 seconds
            gloo_timers::future::TimeoutFuture::new(3000).await;
            save_status.set(None);
        });
    };

    rsx! {
        div {
            class: "space-y-4",

            // Current endpoint display
            div {
                class: "p-4 bg-gray-50 dark:bg-gray-700 rounded-lg",
                div {
                    class: "flex items-center justify-between mb-2",
                    p {
                        class: "text-sm font-medium text-gray-600 dark:text-gray-400",
                        "Mempool API Endpoint"
                    }
                    if is_default {
                        span {
                            class: "px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200 text-xs font-medium rounded",
                            "Default"
                        }
                    } else {
                        span {
                            class: "px-2 py-1 bg-purple-100 dark:bg-purple-900 text-purple-800 dark:text-purple-200 text-xs font-medium rounded",
                            "Custom"
                        }
                    }
                }

                // Endpoint input
                div {
                    class: "flex gap-2",
                    input {
                        class: "flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white font-mono text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                        r#type: "url",
                        placeholder: "https://mempool.space/api",
                        value: "{endpoint_input}",
                        oninput: move |evt| endpoint_input.set(evt.value())
                    }
                }

                p {
                    class: "mt-2 text-xs text-gray-500 dark:text-gray-400",
                    "Enter the base URL for your mempool.space instance (e.g., https://mempool.space/api or https://your-server.com/api)"
                }
            }

            // Action buttons
            div {
                class: "flex gap-2",
                if is_modified {
                    button {
                        class: "flex-1 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition disabled:opacity-50",
                        disabled: *is_saving.read(),
                        onclick: save_endpoint,
                        if *is_saving.read() { "Saving..." } else { "Save Endpoint" }
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

            // Save status
            if let Some(status) = save_status.read().as_ref() {
                div {
                    class: "p-3 bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200 rounded-lg text-sm text-center",
                    "✅ {status}"
                }
            }

            // Note about privacy
            if auth.is_authenticated {
                p {
                    class: "text-xs text-gray-500 dark:text-gray-400",
                    "Your mempool endpoint setting is synced across devices via Nostr (NIP-78)."
                }
            }
        }
    }
}
