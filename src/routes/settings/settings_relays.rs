//! Relay Settings Page
//!
//! Dedicated relay management page with 7 sections:
//! 1. General Relays (NIP-65 kind 10002)
//! 2. DM Inbox Relays (NIP-17 kind 10050)
//! 3. Search Relays (NIP-51 kind 10007)
//! 4. Blocked Relays (NIP-51 kind 10006)
//! 5. Local Relays (web: browser storage; native: config directory)
//! 6. Broadcast Relays (web: browser storage; native: config directory)
//! 7. Connected Relays (read-only live stats)
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client, relay};
use dioxus::prelude::*;
use std::collections::HashMap;
use url::Url;
#[component]
pub fn SettingsRelays() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut general_relays = use_signal(|| {
        relay::USER_RELAY_METADATA
            .peek()
            .as_ref()
            .map(|m| m.relays.clone())
            .unwrap_or_else(relay::default_relays)
    });
    let mut dm_relays = use_signal(|| {
        relay::USER_RELAY_METADATA
            .peek()
            .as_ref()
            .map(|m| m.dm_relays.clone())
            .unwrap_or_else(relay::default_dm_relays)
    });
    let mut search_relays = use_signal(|| relay::SEARCH_RELAYS.peek().clone());
    let mut blocked_relays = use_signal(|| relay::BLOCKED_RELAYS.peek().clone());
    let mut local_relays = use_signal(|| relay::LOCAL_RELAYS.peek().clone());
    let mut broadcast_relays = use_signal(|| relay::BROADCAST_RELAYS.peek().clone());
    let mut new_general_relay = use_signal(String::new);
    let mut new_dm_relay = use_signal(String::new);
    let mut new_search_relay = use_signal(String::new);
    let mut new_blocked_relay = use_signal(String::new);
    let mut new_local_relay = use_signal(String::new);
    let mut new_broadcast_relay = use_signal(String::new);
    let mut general_error = use_signal(|| None::<String>);
    let mut dm_error = use_signal(|| None::<String>);
    let mut search_error = use_signal(|| None::<String>);
    let mut blocked_error = use_signal(|| None::<String>);
    let mut local_error = use_signal(|| None::<String>);
    let mut broadcast_error = use_signal(|| None::<String>);
    let mut save_status = use_signal(|| None::<String>);
    let mut publishing = use_signal(|| false);
    use_effect(move || {
        if let Some(metadata) = relay::USER_RELAY_METADATA.read().as_ref() {
            general_relays.set(metadata.relays.clone());
            dm_relays.set(metadata.dm_relays.clone());
        }
    });
    use_effect(move || {
        search_relays.set(relay::SEARCH_RELAYS.read().clone());
    });
    use_effect(move || {
        blocked_relays.set(relay::BLOCKED_RELAYS.read().clone());
    });
    use_effect(move || {
        local_relays.set(relay::LOCAL_RELAYS.read().clone());
    });
    use_effect(move || {
        broadcast_relays.set(relay::BROADCAST_RELAYS.read().clone());
    });
    let connection_info = use_resource(move || async move {
        let _initialized = *nostr_client::CLIENT_INITIALIZED.read();
        nostr_client::get_relay_display_info().await
    });
    let stats_map = use_memo(move || {
        connection_info
            .read()
            .as_ref()
            .map(|infos| {
                infos
                    .iter()
                    .map(|info| (info.url.clone(), info.clone()))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default()
    });
    let normalize_relay_url = |input: &str| -> Result<String, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("URL cannot be empty".to_string());
        }
        if let Ok(url) = nostr::Url::parse(trimmed) {
            let scheme = url.scheme();
            if scheme == "ws" || scheme == "wss" {
                return Ok(url.to_string());
            }
            return Err("Unsupported URL scheme (use ws:// or wss://)".to_string());
        }
        if let Ok(url) = nostr::Url::parse(&format!("wss://{}", trimmed)) {
            return Ok(url.to_string());
        }
        Err("Invalid relay URL".to_string())
    };
    let normalize_local_relay_url = |input: &str| -> Result<String, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("URL cannot be empty".to_string());
        }
        let lower = trimmed.to_lowercase();
        if lower.starts_with("ws://") || lower.starts_with("wss://") {
            return Ok(trimmed.to_string());
        }
        if lower.starts_with("http://") || lower.starts_with("https://") || lower.contains("://") {
            return Err("Unsupported URL scheme (use ws:// or wss://)".to_string());
        }
        fn is_private_172(host: &str) -> bool {
            if let Some(rest) = host.strip_prefix("172.") {
                if let Some(second_octet) = rest.split('.').next() {
                    if let Ok(n) = second_octet.parse::<u8>() {
                        return (16..=31).contains(&n);
                    }
                }
            }
            false
        }
        let is_local = lower.contains("127.0.0.1")
            || lower.contains("localhost")
            || lower.contains("192.168.")
            || lower.starts_with("10.")
            || is_private_172(&lower)
            || lower.contains("[::1]")
            || lower.contains("::1:")
            || lower.ends_with(".local")
            || lower.contains(".local:")
            || lower.contains(".local/")
            || lower.contains("umbrel:");
        let scheme = if is_local { "ws://" } else { "wss://" };
        Ok(format!("{}{}", scheme, trimmed))
    };
    let display_relay_url = |url: &str| -> String {
        if let Ok(parsed) = nostr::Url::parse(url) {
            let host = parsed.host_str().unwrap_or(url);
            let host_with_port = match parsed.port() {
                Some(port) => format!("{}:{}", host, port),
                None => host.to_string(),
            };
            if (parsed.scheme() == "wss" || parsed.scheme() == "ws") && parsed.path() == "/" {
                host_with_port
            } else {
                format!("{}{}", host_with_port, parsed.path())
            }
        } else {
            url.to_string()
        }
    };
    let relay_detail_route = |url: &str| Route::RelayDetail {
        relay_id: crate::utils::relay::encode_relay_route_id(url),
    };
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
    let add_general_relay = move |_| {
        let url = new_general_relay.read().clone();
        match normalize_relay_url(&url) {
            Ok(normalized) => {
                if general_relays.read().iter().any(|r| r.url == normalized) {
                    general_error.set(Some("Relay already exists".to_string()));
                    return;
                }
                general_relays.write().push(relay::RelayConfig {
                    url: normalized,
                    read: true,
                    write: true,
                });
                new_general_relay.set(String::new());
                general_error.set(None);
            }
            Err(e) => general_error.set(Some(e)),
        }
    };
    let mut remove_general_relay = move |index: usize| {
        let mut relays = general_relays.write();
        if index < relays.len() {
            relays.remove(index);
        }
    };
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
    let add_dm_relay = move |_| {
        let url = new_dm_relay.read().clone();
        match normalize_relay_url(&url) {
            Ok(normalized) => {
                if dm_relays.read().contains(&normalized) {
                    dm_error.set(Some("Relay already exists".to_string()));
                    return;
                }
                dm_relays.write().push(normalized);
                new_dm_relay.set(String::new());
                dm_error.set(None);
            }
            Err(e) => dm_error.set(Some(e)),
        }
    };
    let mut remove_dm_relay = move |index: usize| {
        let mut relays = dm_relays.write();
        if index < relays.len() {
            relays.remove(index);
        }
    };
    let add_search_relay = move |_| {
        let url = new_search_relay.read().clone();
        match normalize_relay_url(&url) {
            Ok(normalized) => {
                if search_relays.read().contains(&normalized) {
                    search_error.set(Some("Relay already exists".to_string()));
                    return;
                }
                search_relays.write().push(normalized);
                new_search_relay.set(String::new());
                search_error.set(None);
            }
            Err(e) => search_error.set(Some(e)),
        }
    };
    let mut remove_search_relay = move |index: usize| {
        let mut relays = search_relays.write();
        if index < relays.len() {
            relays.remove(index);
        }
    };
    let add_blocked_relay = move |_| {
        let url = new_blocked_relay.read().clone();
        match normalize_relay_url(&url) {
            Ok(normalized) => {
                if blocked_relays.read().contains(&normalized) {
                    blocked_error.set(Some("Relay already exists".to_string()));
                    return;
                }
                blocked_relays.write().push(normalized);
                new_blocked_relay.set(String::new());
                blocked_error.set(None);
            }
            Err(e) => blocked_error.set(Some(e)),
        }
    };
    let mut remove_blocked_relay = move |index: usize| {
        let mut relays = blocked_relays.write();
        if index < relays.len() {
            relays.remove(index);
        }
    };
    let add_local_relay = move |_| {
        let url = new_local_relay.read().clone();
        match normalize_local_relay_url(&url) {
            Ok(normalized) => {
                match Url::parse(&normalized) {
                    Ok(parsed)
                        if matches!(parsed.scheme(), "ws" | "wss")
                            && parsed.host_str().is_some() => {}
                    _ => {
                        local_error.set(Some("Invalid relay URL".to_string()));
                        return;
                    }
                }
                if local_relays.read().contains(&normalized) {
                    local_error.set(Some("Relay already exists".to_string()));
                    return;
                }
                let mut relays = local_relays.read().clone();
                relays.push(normalized);
                relay::save_local_relays(&relays);
                local_relays.set(relays.clone());
                *relay::LOCAL_RELAYS.write() = relays;
                new_local_relay.set(String::new());
                local_error.set(None);
            }
            Err(e) => local_error.set(Some(e)),
        }
    };
    let mut remove_local_relay = move |index: usize| {
        let mut relays = local_relays.write();
        if index < relays.len() {
            relays.remove(index);
            relay::save_local_relays(&relays);
            *relay::LOCAL_RELAYS.write() = relays.clone();
        }
    };
    let add_broadcast_relay = move |_| {
        let url = new_broadcast_relay.read().trim().to_string();
        match normalize_local_relay_url(&url) {
            Ok(normalized) => {
                match Url::parse(&normalized) {
                    Ok(parsed)
                        if matches!(parsed.scheme(), "ws" | "wss")
                            && parsed.host_str().is_some() => {}
                    _ => {
                        broadcast_error.set(Some("Invalid relay URL".to_string()));
                        return;
                    }
                }
                if broadcast_relays.read().contains(&normalized) {
                    broadcast_error.set(Some("Relay already exists".to_string()));
                    return;
                }
                let mut relays = broadcast_relays.read().clone();
                relays.push(normalized);
                match relay::save_broadcast_relays(&relays) {
                    Ok(()) => {
                        broadcast_relays.set(relays.clone());
                        *relay::BROADCAST_RELAYS.write() = relays;
                        new_broadcast_relay.set(String::new());
                        broadcast_error.set(None);
                    }
                    Err(e) => broadcast_error.set(Some(e)),
                }
            }
            Err(e) => broadcast_error.set(Some(e)),
        }
    };
    let mut remove_broadcast_relay = move |index: usize| {
        let mut relays = broadcast_relays.read().clone();
        if index < relays.len() {
            relays.remove(index);
            match relay::save_broadcast_relays(&relays) {
                Ok(()) => {
                    broadcast_relays.set(relays.clone());
                    *relay::BROADCAST_RELAYS.write() = relays;
                    broadcast_error.set(None);
                }
                Err(e) => broadcast_error.set(Some(e)),
            }
        }
    };
    let publish_relay_lists = move |_| {
        if *publishing.read() {
            return;
        }
        publishing.set(true);
        let general = general_relays.read().clone();
        let dm = dm_relays.read().clone();
        let search = search_relays.read().clone();
        let blocked = blocked_relays.read().clone();
        spawn(async move {
            save_status.set(Some("Publishing...".to_string()));
            let client = match nostr_client::get_client() {
                Some(c) => c,
                None => {
                    save_status.set(Some("Client not initialized".to_string()));
                    publishing.set(false);
                    return;
                }
            };
            if let Err(e) = relay::publish_relay_list(general.clone(), client.clone()).await {
                save_status.set(Some(format!("Failed to publish general relays: {}", e)));
                publishing.set(false);
                return;
            }
            if let Err(e) = relay::publish_dm_relay_list(dm.clone(), client.clone()).await {
                save_status.set(Some(format!("Failed to publish DM relays: {}", e)));
                publishing.set(false);
                return;
            }
            if let Err(e) = relay::publish_search_relays(search.clone(), client.clone()).await {
                save_status.set(Some(format!("Failed to publish search relays: {}", e)));
                publishing.set(false);
                return;
            }
            if let Err(e) = relay::publish_blocked_relays(blocked.clone(), client.clone()).await {
                save_status.set(Some(format!("Failed to publish blocked relays: {}", e)));
                publishing.set(false);
                return;
            }
            let mut metadata = relay::USER_RELAY_METADATA.write();
            let now_secs = crate::platform::timestamp::now_secs();
            *metadata = Some(relay::RelayListMetadata {
                relays: general,
                dm_relays: dm,
                updated_at: now_secs,
            });
            *relay::SEARCH_RELAYS.write() = search;
            *relay::BLOCKED_RELAYS.write() = blocked;
            crate::services::search_relays::invalidate_search_relay_cache().await;
            save_status.set(Some("Relay lists published successfully!".to_string()));
            crate::platform::timer::sleep_ms(3000).await;
            save_status.set(None);
            publishing.set(false);
        });
    };
    rsx! {
        div { class: "max-w-2xl mx-auto px-4 py-6 space-y-6",
            div { class: "mb-6",
                Link {
                    to: Route::Settings {},
                    class: "text-blue-600 dark:text-blue-400 hover:underline flex items-center gap-2 mb-4",
                    span { "← Back to Settings" }
                }
                h1 { class: "text-2xl font-bold text-gray-900 dark:text-white", "Relay Management" }
                p { class: "text-sm text-gray-600 dark:text-gray-400 mt-2",
                    "Configure which relays to use for different purposes. Changes are published to Nostr when you click the publish button."
                }
            }
            if !auth.is_authenticated {
                div { class: "bg-yellow-100 dark:bg-yellow-900 border border-yellow-300 dark:border-yellow-700 rounded-lg p-4 text-center",
                    p { class: "text-yellow-800 dark:text-yellow-200",
                        "Please log in to manage your relay settings."
                    }
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div { class: "flex items-center justify-between mb-4",
                        div {
                            h3 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                "General Relays"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                "NIP-65 • Read: fetch content • Write: publish content"
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "px-2 py-1 bg-muted text-muted-foreground rounded text-xs",
                                "kind 10002"
                            }
                            button {
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400",
                                onclick: move |_| {
                                    relay::reset_general_relays_to_default();
                                    general_relays.set(relay::default_relays());
                                },
                                "Reset"
                            }
                        }
                    }
                    div { class: "space-y-2 mb-4",
                        for (index , relay_config) in general_relays.read().iter().enumerate() {
                            {
                                let url = relay_config.url.clone();
                                let stats = stats_map.read().get(&url).cloned();
                                rsx! {
                                    div { key: "{url}", class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                        div { class: "flex items-center justify-between",
                                            Link {
                                                to: relay_detail_route(&url),
                                                class: "font-mono text-sm text-gray-900 dark:text-white hover:underline break-all",
                                                {display_relay_url(&url)}
                                            }
                                            div { class: "flex items-center gap-2",
                                                button {
                                                    class: if relay_config.read { "px-2 py-1 bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200 rounded text-xs font-medium" } else { "px-2 py-1 bg-gray-200 text-gray-600 dark:bg-gray-600 dark:text-gray-400 rounded text-xs font-medium" },
                                                    onclick: move |_| toggle_relay_read(index),
                                                    if relay_config.read {
                                                        "R"
                                                    } else {
                                                        "R"
                                                    }
                                                }
                                                button {
                                                    class: if relay_config.write { "px-2 py-1 bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200 rounded text-xs font-medium" } else { "px-2 py-1 bg-gray-200 text-gray-600 dark:bg-gray-600 dark:text-gray-400 rounded text-xs font-medium" },
                                                    onclick: move |_| toggle_relay_write(index),
                                                    if relay_config.write {
                                                        "W"
                                                    } else {
                                                        "W"
                                                    }
                                                }
                                                button {
                                                    class: "px-2 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded text-xs transition",
                                                    onclick: move |_| remove_general_relay(index),
                                                    "✕"
                                                }
                                            }
                                        }
                                        if let Some(info) = stats {
                                            div { class: "flex items-center gap-4 mt-2 text-xs text-gray-500 dark:text-gray-400",
                                                span {
                                                    class: match info.status_str() {
                                                        "Connected" => "text-green-600 dark:text-green-400",
                                                        "Connecting" | "Pending" => "text-yellow-600 dark:text-yellow-400",
                                                        _ => "text-gray-500 dark:text-gray-400",
                                                    },
                                                    "● {info.status_str()}"
                                                }
                                                span { "↓ {format_bytes(info.bytes_received)}" }
                                                span { "↑ {format_bytes(info.bytes_sent)}" }
                                                if info.connection_attempts > 0 {
                                                    span { class: if info.success_rate > 80.0 { "text-green-600 dark:text-green-400" } else { "text-yellow-600 dark:text-yellow-400" },
                                                        "{info.success_rate as u8}%"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "wss://relay.example.com",
                            value: "{new_general_relay}",
                            oninput: move |evt| new_general_relay.set(evt.value()),
                        }
                        button {
                            class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium text-sm transition",
                            onclick: add_general_relay,
                            "+ Add"
                        }
                    }
                    if let Some(err) = general_error.read().as_ref() {
                        div { class: "mt-2 p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                            "{err}"
                        }
                    }
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div { class: "flex items-center justify-between mb-4",
                        div {
                            h3 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                "DM Inbox Relays"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                "NIP-17 • Where others send you direct messages"
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "px-2 py-1 bg-muted text-muted-foreground rounded text-xs",
                                "kind 10050"
                            }
                            button {
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400",
                                onclick: move |_| {
                                    relay::reset_dm_relays_to_default();
                                    dm_relays.set(relay::default_dm_relays());
                                },
                                "Reset"
                            }
                        }
                    }
                    div { class: "space-y-2 mb-4",
                        for (index , url) in dm_relays.read().iter().enumerate() {
                            {
                                let url_clone = url.clone();
                                let stats = stats_map.read().get(&url_clone).cloned();
                                rsx! {
                                    div { key: "{url_clone}", class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                        div { class: "flex items-center justify-between",
                                            div { class: "flex items-center gap-1 min-w-0",
                                                span { "📨" }
                                                Link {
                                                    to: relay_detail_route(&url_clone),
                                                    class: "font-mono text-sm text-gray-900 dark:text-white hover:underline break-all",
                                                    {display_relay_url(&url_clone)}
                                                }
                                            }
                                            button {
                                                class: "px-2 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded text-xs transition",
                                                onclick: move |_| remove_dm_relay(index),
                                                "✕"
                                            }
                                        }
                                        if let Some(info) = stats {
                                            div { class: "flex items-center gap-4 mt-2 text-xs text-gray-500 dark:text-gray-400",
                                                span {
                                                    class: match info.status_str() {
                                                        "Connected" => "text-green-600 dark:text-green-400",
                                                        "Connecting" | "Pending" => "text-yellow-600 dark:text-yellow-400",
                                                        _ => "text-gray-500 dark:text-gray-400",
                                                    },
                                                    "● {info.status_str()}"
                                                }
                                                span { "↓ {format_bytes(info.bytes_received)}" }
                                                span { "↑ {format_bytes(info.bytes_sent)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "wss://relay.example.com",
                            value: "{new_dm_relay}",
                            oninput: move |evt| new_dm_relay.set(evt.value()),
                        }
                        button {
                            class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium text-sm transition",
                            onclick: add_dm_relay,
                            "+ Add"
                        }
                    }
                    if let Some(err) = dm_error.read().as_ref() {
                        div { class: "mt-2 p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                            "{err}"
                        }
                    }
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div { class: "flex items-center justify-between mb-4",
                        div {
                            h3 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                "Search Relays"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                "NIP-50 • Relays that support full-text search"
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "px-2 py-1 bg-muted text-muted-foreground rounded text-xs",
                                "kind 10007"
                            }
                            button {
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400",
                                onclick: move |_| {
                                    search_relays.set(relay::default_search_relays());
                                },
                                "Reset"
                            }
                        }
                    }
                    div { class: "space-y-2 mb-4",
                        for (index , url) in search_relays.read().iter().enumerate() {
                            {
                                let url_clone = url.clone();
                                let stats = stats_map.read().get(&url_clone).cloned();
                                rsx! {
                                    div { key: "{url_clone}", class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                        div { class: "flex items-center justify-between",
                                            div { class: "flex items-center gap-1 min-w-0",
                                                span { "🔍" }
                                                Link {
                                                    to: relay_detail_route(&url_clone),
                                                    class: "font-mono text-sm text-gray-900 dark:text-white hover:underline break-all",
                                                    {display_relay_url(&url_clone)}
                                                }
                                            }
                                            button {
                                                class: "px-2 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded text-xs transition",
                                                onclick: move |_| remove_search_relay(index),
                                                "✕"
                                            }
                                        }
                                        if let Some(info) = stats {
                                            div { class: "flex items-center gap-4 mt-2 text-xs text-gray-500 dark:text-gray-400",
                                                span {
                                                    class: match info.status_str() {
                                                        "Connected" => "text-green-600 dark:text-green-400",
                                                        "Connecting" | "Pending" => "text-yellow-600 dark:text-yellow-400",
                                                        _ => "text-gray-500 dark:text-gray-400",
                                                    },
                                                    "● {info.status_str()}"
                                                }
                                                span { "↓ {format_bytes(info.bytes_received)}" }
                                                span { "↑ {format_bytes(info.bytes_sent)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "wss://relay.nostr.band",
                            value: "{new_search_relay}",
                            oninput: move |evt| new_search_relay.set(evt.value()),
                        }
                        button {
                            class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium text-sm transition",
                            onclick: add_search_relay,
                            "+ Add"
                        }
                    }
                    if let Some(err) = search_error.read().as_ref() {
                        div { class: "mt-2 p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                            "{err}"
                        }
                    }
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div { class: "flex items-center justify-between mb-4",
                        div {
                            h3 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                "Blocked Relays"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                "NIP-51 • Relays to never connect to"
                            }
                        }
                        span { class: "px-2 py-1 bg-muted text-muted-foreground rounded text-xs",
                            "kind 10006"
                        }
                    }
                    div { class: "space-y-2 mb-4",
                        if blocked_relays.read().is_empty() {
                            div { class: "text-center py-4 text-gray-500 dark:text-gray-400 text-sm",
                                "No blocked relays"
                            }
                        }
                        for (index , url) in blocked_relays.read().iter().enumerate() {
                            {
                                let url_clone = url.clone();
                                rsx! {
                                    div { key: "{url_clone}", class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                        div { class: "flex items-center justify-between",
                                            div { class: "flex items-center gap-1 min-w-0",
                                                span { "🚫" }
                                                Link {
                                                    to: relay_detail_route(&url_clone),
                                                    class: "font-mono text-sm text-gray-900 dark:text-white hover:underline break-all",
                                                    {display_relay_url(&url_clone)}
                                                }
                                            }
                                            button {
                                                class: "px-2 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded text-xs transition",
                                                onclick: move |_| remove_blocked_relay(index),
                                                "✕"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "wss://spam-relay.example.com",
                            value: "{new_blocked_relay}",
                            oninput: move |evt| new_blocked_relay.set(evt.value()),
                        }
                        button {
                            class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium text-sm transition",
                            onclick: add_blocked_relay,
                            "+ Add"
                        }
                    }
                    if let Some(err) = blocked_error.read().as_ref() {
                        div { class: "mt-2 p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                            "{err}"
                        }
                    }
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div { class: "flex items-center justify-between mb-4",
                        div {
                            h3 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                "Local Relays"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                "Localhost/LAN relays (stored locally, not published to Nostr)"
                            }
                        }
                        span { class: "px-2 py-1 bg-purple-100 dark:bg-purple-900 text-purple-600 dark:text-purple-300 rounded text-xs",
                            "local only"
                        }
                    }
                    div { class: "space-y-2 mb-4",
                        if local_relays.read().is_empty() {
                            div { class: "text-center py-4 text-gray-500 dark:text-gray-400 text-sm",
                                "No local relays configured"
                            }
                        }
                        for (index , url) in local_relays.read().iter().enumerate() {
                            {
                                let url_clone = url.clone();
                                let stats = stats_map.read().get(&url_clone).cloned();
                                rsx! {
                                    div { key: "{url_clone}", class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                        div { class: "flex items-center justify-between",
                                            div { class: "flex items-center gap-1 min-w-0",
                                                span { "🏠" }
                                                Link {
                                                    to: relay_detail_route(&url_clone),
                                                    class: "font-mono text-sm text-gray-900 dark:text-white hover:underline break-all",
                                                    {display_relay_url(&url_clone)}
                                                }
                                            }
                                            button {
                                                class: "px-2 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded text-xs transition",
                                                onclick: move |_| remove_local_relay(index),
                                                "✕"
                                            }
                                        }
                                        if let Some(info) = stats {
                                            div { class: "flex items-center gap-4 mt-2 text-xs text-gray-500 dark:text-gray-400",
                                                span {
                                                    class: match info.status_str() {
                                                        "Connected" => "text-green-600 dark:text-green-400",
                                                        "Connecting" | "Pending" => "text-yellow-600 dark:text-yellow-400",
                                                        _ => "text-gray-500 dark:text-gray-400",
                                                    },
                                                    "● {info.status_str()}"
                                                }
                                                span { "↓ {format_bytes(info.bytes_received)}" }
                                                span { "↑ {format_bytes(info.bytes_sent)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "ws://localhost:7777 or ws://192.168.1.100:4869",
                            value: "{new_local_relay}",
                            oninput: move |evt| new_local_relay.set(evt.value()),
                        }
                        button {
                            class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium text-sm transition",
                            onclick: add_local_relay,
                            "+ Add"
                        }
                    }
                    if let Some(err) = local_error.read().as_ref() {
                        div { class: "mt-2 p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                            "{err}"
                        }
                    }
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div { class: "flex items-center justify-between mb-4",
                        div {
                            h3 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                "Broadcast Relays"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                "Extra write targets for the post menu Broadcast action (stored locally)"
                            }
                        }
                        span { class: "px-2 py-1 bg-purple-100 dark:bg-purple-900 text-purple-600 dark:text-purple-300 rounded text-xs",
                            "local only"
                        }
                    }
                    div { class: "space-y-2 mb-4",
                        if broadcast_relays.read().is_empty() {
                            div { class: "text-center py-4 text-gray-500 dark:text-gray-400 text-sm",
                                "No broadcast relays configured"
                            }
                        }
                        for (index , url) in broadcast_relays.read().iter().enumerate() {
                            {
                                let url_clone = url.clone();
                                let stats = stats_map.read().get(&url_clone).cloned();
                                rsx! {
                                    div { key: "{url_clone}", class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                        div { class: "flex items-center justify-between",
                                            div { class: "flex items-center gap-1 min-w-0",
                                                span { "📡" }
                                                Link {
                                                    to: relay_detail_route(&url_clone),
                                                    class: "font-mono text-sm text-gray-900 dark:text-white hover:underline break-all",
                                                    {display_relay_url(&url_clone)}
                                                }
                                            }
                                            button {
                                                class: "px-2 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded text-xs transition",
                                                onclick: move |_| remove_broadcast_relay(index),
                                                "✕"
                                            }
                                        }
                                        if let Some(info) = stats {
                                            div { class: "flex items-center gap-4 mt-2 text-xs text-gray-500 dark:text-gray-400",
                                                span {
                                                    class: match info.status_str() {
                                                        "Connected" => "text-green-600 dark:text-green-400",
                                                        "Connecting" | "Pending" => "text-yellow-600 dark:text-yellow-400",
                                                        _ => "text-gray-500 dark:text-gray-400",
                                                    },
                                                    "● {info.status_str()}"
                                                }
                                                span { "↓ {format_bytes(info.bytes_received)}" }
                                                span { "↑ {format_bytes(info.bytes_sent)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                            r#type: "text",
                            placeholder: "wss://relay.example.com",
                            value: "{new_broadcast_relay}",
                            oninput: move |evt| new_broadcast_relay.set(evt.value()),
                        }
                        button {
                            class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium text-sm transition",
                            onclick: add_broadcast_relay,
                            "+ Add"
                        }
                    }
                    if let Some(err) = broadcast_error.read().as_ref() {
                        div { class: "mt-2 p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                            "{err}"
                        }
                    }
                }
            }
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    div {
                        h3 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                            "Connected Relays"
                        }
                        p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                            "Currently active connections with live statistics"
                        }
                    }
                    span { class: "px-2 py-1 bg-muted text-muted-foreground rounded text-xs",
                        "read-only"
                    }
                }
                match &*connection_info.read() {
                    Some(relays) if !relays.is_empty() => rsx! {
                        div { class: "space-y-2",
                            for relay_info in relays.iter() {
                                div {
                                    key: "{relay_info.url}",
                                    class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                    div { class: "flex items-center justify-between",
                                        div { class: "flex items-center gap-3",
                                            span {
                                                class: match relay_info.status_str() {
                                                    "Connected" => "w-3 h-3 rounded-full bg-green-500",
                                                    "Connecting" | "Pending" => "w-3 h-3 rounded-full bg-yellow-500 animate-pulse",
                                                    _ => "w-3 h-3 rounded-full bg-gray-400",
                                                },
                                            }
                                            Link {
                                                to: relay_detail_route(&relay_info.url),
                                                class: "font-mono text-sm text-gray-900 dark:text-white hover:underline break-all",
                                                {display_relay_url(&relay_info.url)}
                                            }
                                        }
                                        div { class: "flex items-center gap-2 text-xs",
                                            if relay_info.has_read {
                                                span { class: "text-green-600 dark:text-green-400", "R" }
                                            }
                                            if relay_info.has_write {
                                                span { class: "text-blue-600 dark:text-blue-400", "W" }
                                            }
                                            if relay_info.is_gossip {
                                                span { class: "text-purple-600 dark:text-purple-400", "G" }
                                            }
                                        }
                                    }
                                    div { class: "flex items-center gap-4 mt-2 text-xs text-gray-500 dark:text-gray-400",
                                        span { "{relay_info.status_str()}" }
                                        span { "↓ {format_bytes(relay_info.bytes_received)}" }
                                        span { "↑ {format_bytes(relay_info.bytes_sent)}" }
                                        if relay_info.connection_attempts > 0 {
                                            span { class: if relay_info.success_rate > 80.0 { "text-green-600 dark:text-green-400" } else if relay_info.success_rate > 50.0 { "text-yellow-600 dark:text-yellow-400" } else { "text-red-600 dark:text-red-400" },
                                                "{relay_info.success_rate as u8}%"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Some(_) => rsx! {
                        div { class: "text-center py-8 text-gray-500 dark:text-gray-400", "No relays connected" }
                    },
                    None => rsx! {
                        div { class: "text-center py-8 text-gray-500 dark:text-gray-400", "Loading..." }
                    },
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    button {
                        class: "w-full px-6 py-3 bg-green-600 hover:bg-green-700 text-white rounded-lg font-medium transition text-lg",
                        onclick: publish_relay_lists,
                        "📤 Publish Relay Lists to Nostr"
                    }
                    if let Some(status) = save_status.read().as_ref() {
                        div { class: "mt-3 p-3 bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200 rounded text-sm text-center",
                            "{status}"
                        }
                    }
                    p { class: "text-xs text-gray-500 dark:text-gray-400 mt-3 text-center",
                        "This publishes your General, DM, Search, and Blocked relay lists to Nostr. Local and Broadcast relays are stored locally on this device."
                    }
                }
            }
        }
    }
}
