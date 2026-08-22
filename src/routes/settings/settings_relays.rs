//! Relay Settings Page
//!
//! Dedicated relay management page with 12 sections:
//! 1. General Relays (NIP-65 kind 10002)
//! 2. DM Inbox Relays (NIP-17 kind 10050)
//! 3. Search Relays (NIP-51 kind 10007)
//! 4. Blocked Relays (NIP-51 kind 10006)
//! 5. Indexer Relays (kind 10086, gift-wrapped)
//! 6. Private Outbox Relays (kind 10013)
//! 7. Favorite / Feed Relays (kind 10012)
//! 8. Proxy Relays (kind 10087, gift-wrapped)
//! 9. Trusted Relays (kind 10089, gift-wrapped)
//! 10. Local Relays (web: browser storage; native: config directory)
//! 11. Broadcast Relays (web: browser storage; native: config directory)
//! 12. Connected Relays (read-only live stats)
//!
//! Education & enrichment (issue #359): each section header carries a
//! plain-language explainer ([`crate::routes::settings::relay_explainers`]),
//! relay rows are enriched with cached NIP-11 name/icon/paid data
//! (`stores::relay::nip11_info`), and the add-relay inputs offer
//! autocomplete suggestions (`components::relay_url_input`).
use crate::components::{RelayDisplayName, RelayUrlInput};
use crate::routes::settings::relay_explainers::{section_hint, RelaySectionKind, SectionExplainer};
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client, relay};
use crate::utils::format_bytes;
use crate::utils::relay::{build_known_relay_set, normalize_known_relay_url};
use dioxus::prelude::*;
use std::collections::HashMap;

/// Pristine snapshot of the local relay edit signals, captured once at
/// mount. Local ≠ snapshot means the user has unpublished edits, and the
/// background global→local reconciliation effect must not run — it would
/// silently revert them when a NIP-65 re-fetch or a publish from another
/// surface mutates the global signals. The snapshot is refreshed after a
/// successful publish (and after the immediately-persisted local/broadcast
/// removals), at which point reconciliation safely resumes.
#[derive(Default, Clone, PartialEq)]
struct RelayEditSnapshot {
    general: Vec<relay::RelayConfig>,
    dm: Vec<String>,
    search: Vec<String>,
    blocked: Vec<String>,
    local: Vec<String>,
    broadcast: Vec<String>,
    indexer: Vec<String>,
    outbox: Vec<String>,
    favorite: Vec<String>,
    proxy: Vec<String>,
    trusted: Vec<String>,
}

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
    let new_general_relay = use_signal(String::new);
    let new_dm_relay = use_signal(String::new);
    let new_search_relay = use_signal(String::new);
    let new_blocked_relay = use_signal(String::new);
    let new_local_relay = use_signal(String::new);
    let new_broadcast_relay = use_signal(String::new);
    let general_error = use_signal(|| None::<String>);
    let dm_error = use_signal(|| None::<String>);
    let search_error = use_signal(|| None::<String>);
    let blocked_error = use_signal(|| None::<String>);
    let local_error = use_signal(|| None::<String>);
    let mut broadcast_error = use_signal(|| None::<String>);
    let mut indexer_relays = use_signal(|| relay::INDEXER_RELAYS.peek().clone());
    let mut outbox_relays = use_signal(|| relay::OUTBOX_RELAYS.peek().clone());
    let mut favorite_relays = use_signal(|| relay::FAVORITE_RELAYS.peek().clone());
    let mut proxy_relays = use_signal(|| relay::PROXY_RELAYS.peek().clone());
    let mut trusted_relays = use_signal(|| relay::TRUSTED_RELAYS.peek().clone());
    let new_indexer_relay = use_signal(String::new);
    let new_outbox_relay = use_signal(String::new);
    let new_favorite_relay = use_signal(String::new);
    let new_proxy_relay = use_signal(String::new);
    let new_trusted_relay = use_signal(String::new);
    let indexer_error = use_signal(|| None::<String>);
    let outbox_error = use_signal(|| None::<String>);
    let favorite_error = use_signal(|| None::<String>);
    let proxy_error = use_signal(|| None::<String>);
    let trusted_error = use_signal(|| None::<String>);
    let mut save_status = use_signal(|| None::<String>);
    let mut publishing = use_signal(|| false);
    // Captured after the local signals initialize: their mount-time values
    // (seeded from the globals) are the pristine baseline.
    let mut baseline = use_hook(|| {
        Signal::new(RelayEditSnapshot {
            general: general_relays.peek().clone(),
            dm: dm_relays.peek().clone(),
            search: search_relays.peek().clone(),
            blocked: blocked_relays.peek().clone(),
            local: local_relays.peek().clone(),
            broadcast: broadcast_relays.peek().clone(),
            indexer: indexer_relays.peek().clone(),
            outbox: outbox_relays.peek().clone(),
            favorite: favorite_relays.peek().clone(),
            proxy: proxy_relays.peek().clone(),
            trusted: trusted_relays.peek().clone(),
        })
    });
    let has_unpublished_edits = move || {
        // peek: no subscription — this only runs inside the reconciliation
        // effect, which is driven by the global signal reads below.
        let b = baseline.peek();
        *general_relays.peek() != b.general
            || *dm_relays.peek() != b.dm
            || *search_relays.peek() != b.search
            || *blocked_relays.peek() != b.blocked
            || *local_relays.peek() != b.local
            || *broadcast_relays.peek() != b.broadcast
            || *indexer_relays.peek() != b.indexer
            || *outbox_relays.peek() != b.outbox
            || *favorite_relays.peek() != b.favorite
            || *proxy_relays.peek() != b.proxy
            || *trusted_relays.peek() != b.trusted
    };
    // NIP-11 enrichment: fetch documents for every listed relay.
    // Idempotent — cached/negative-cached/in-flight URLs are skipped in
    // the store, so repeated runs are cheap no-ops. Safe to run even while
    // the user has unpublished edits (read-only per-URL docs).
    let refresh_nip11_docs = move || {
        let mut row_urls: Vec<String> = Vec::new();
        row_urls.extend(general_relays.read().iter().map(|r| r.url.clone()));
        row_urls.extend(dm_relays.read().iter().cloned());
        row_urls.extend(search_relays.read().iter().cloned());
        row_urls.extend(blocked_relays.read().iter().cloned());
        row_urls.extend(local_relays.read().iter().cloned());
        row_urls.extend(broadcast_relays.read().iter().cloned());
        row_urls.extend(indexer_relays.read().iter().cloned());
        row_urls.extend(outbox_relays.read().iter().cloned());
        row_urls.extend(favorite_relays.read().iter().cloned());
        row_urls.extend(proxy_relays.read().iter().cloned());
        row_urls.extend(trusted_relays.read().iter().cloned());
        relay::nip11_info::ensure_nip11_for(row_urls);
    };
    use_effect(move || {
        // Never reconcile global→local while the user has unpublished
        // edits — a background global refresh would silently revert them.
        if has_unpublished_edits() {
            refresh_nip11_docs();
            return;
        }
        if let Some(metadata) = relay::USER_RELAY_METADATA.read().as_ref() {
            if *general_relays.peek() != metadata.relays {
                general_relays.set(metadata.relays.clone());
            }
            if *dm_relays.peek() != metadata.dm_relays {
                dm_relays.set(metadata.dm_relays.clone());
            }
        }
        {
            let v = relay::SEARCH_RELAYS.read();
            if *search_relays.peek() != *v {
                search_relays.set(v.clone());
            }
        }
        {
            let v = relay::BLOCKED_RELAYS.read();
            if *blocked_relays.peek() != *v {
                blocked_relays.set(v.clone());
            }
        }
        {
            let v = relay::LOCAL_RELAYS.read();
            if *local_relays.peek() != *v {
                local_relays.set(v.clone());
            }
        }
        {
            let v = relay::BROADCAST_RELAYS.read();
            if *broadcast_relays.peek() != *v {
                broadcast_relays.set(v.clone());
            }
        }
        {
            let v = relay::INDEXER_RELAYS.read();
            if *indexer_relays.peek() != *v {
                indexer_relays.set(v.clone());
            }
        }
        {
            let v = relay::OUTBOX_RELAYS.read();
            if *outbox_relays.peek() != *v {
                outbox_relays.set(v.clone());
            }
        }
        {
            let v = relay::FAVORITE_RELAYS.read();
            if *favorite_relays.peek() != *v {
                favorite_relays.set(v.clone());
            }
        }
        {
            let v = relay::PROXY_RELAYS.read();
            if *proxy_relays.peek() != *v {
                proxy_relays.set(v.clone());
            }
        }
        {
            let v = relay::TRUSTED_RELAYS.read();
            if *trusted_relays.peek() != *v {
                trusted_relays.set(v.clone());
            }
        }
        // The locals now mirror the globals — record them as the pristine
        // baseline so a subsequent user edit (and only an edit) is
        // detectable.
        baseline.set(RelayEditSnapshot {
            general: general_relays.peek().clone(),
            dm: dm_relays.peek().clone(),
            search: search_relays.peek().clone(),
            blocked: blocked_relays.peek().clone(),
            local: local_relays.peek().clone(),
            broadcast: broadcast_relays.peek().clone(),
            indexer: indexer_relays.peek().clone(),
            outbox: outbox_relays.peek().clone(),
            favorite: favorite_relays.peek().clone(),
            proxy: proxy_relays.peek().clone(),
            trusted: trusted_relays.peek().clone(),
        });
        refresh_nip11_docs();
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
    // Autocomplete: seed suggestions from pool stats + coverage + curated
    // defaults once the pool info resolves, then kick off the one-shot
    // NIP-66 background fetch that merges RTT data.
    let mut suggestions_seeded = use_signal(|| false);
    use_effect(move || {
        if *suggestions_seeded.read() {
            return;
        }
        if let Some(infos) = connection_info.read().as_ref() {
            suggestions_seeded.set(true);
            relay::suggestions::seed_base_suggestions(infos);
            relay::suggestions::spawn_nip66_suggestions_fetch();
        }
    });
    let relay_detail_route = |url: &str| Route::RelayDetail {
        relay_id: crate::utils::relay::encode_relay_route_id(url),
    };
    let known_relays = use_memo(move || build_known_relay_set(connection_info.read().as_deref()));
    let can_open_relay_detail = |url: &str| {
        !relay::is_relay_blocked(url)
            && known_relays
                .read()
                .contains(&normalize_known_relay_url(url))
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
    let mut remove_dm_relay = move |index: usize| {
        let mut relays = dm_relays.write();
        if index < relays.len() {
            relays.remove(index);
        }
    };
    let mut remove_search_relay = move |index: usize| {
        let mut relays = search_relays.write();
        if index < relays.len() {
            relays.remove(index);
        }
    };
    let mut remove_blocked_relay = move |index: usize| {
        let mut relays = blocked_relays.write();
        if index < relays.len() {
            relays.remove(index);
        }
    };
    let mut remove_local_relay = move |index: usize| {
        let mut relays = local_relays.write();
        if index < relays.len() {
            relays.remove(index);
            relay::save_local_relays(&relays);
            *relay::LOCAL_RELAYS.write() = relays.clone();
            // Persisted immediately — treat as the new pristine local state.
            baseline.with_mut(|b| b.local = relays.clone());
        }
    };
    let mut remove_broadcast_relay = move |index: usize| {
        let mut relays = broadcast_relays.read().clone();
        if index < relays.len() {
            relays.remove(index);
            match relay::save_broadcast_relays(&relays) {
                Ok(()) => {
                    broadcast_relays.set(relays.clone());
                    *relay::BROADCAST_RELAYS.write() = relays.clone();
                    // Persisted immediately — new pristine broadcast state.
                    baseline.with_mut(|b| b.broadcast = relays.clone());
                    broadcast_error.set(None);
                }
                Err(e) => broadcast_error.set(Some(e)),
            }
        }
    };
    let mut remove_indexer_relay = move |index: usize| {
        let mut relays = indexer_relays.write();
        if index < relays.len() {
            relays.remove(index);
        }
    };
    let mut remove_outbox_relay = move |index: usize| {
        let mut relays = outbox_relays.write();
        if index < relays.len() {
            relays.remove(index);
        }
    };
    let mut remove_favorite_relay = move |index: usize| {
        let mut relays = favorite_relays.write();
        if index < relays.len() {
            relays.remove(index);
        }
    };
    let mut remove_proxy_relay = move |index: usize| {
        let mut relays = proxy_relays.write();
        if index < relays.len() {
            relays.remove(index);
        }
    };
    let mut remove_trusted_relay = move |index: usize| {
        let mut relays = trusted_relays.write();
        if index < relays.len() {
            relays.remove(index);
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
        let indexer = indexer_relays.read().clone();
        let outbox = outbox_relays.read().clone();
        let favorites = favorite_relays.read().clone();
        let proxy = proxy_relays.read().clone();
        let trusted = trusted_relays.read().clone();
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
            if !indexer.is_empty() {
                if let Err(e) = relay::publish_indexer_relays(indexer.clone(), client.clone()).await {
                    log::warn!("Failed to publish indexer relays: {}", e);
                }
            }
            if !outbox.is_empty() {
                if let Err(e) = relay::publish_outbox_relays(outbox.clone(), client.clone()).await {
                    log::warn!("Failed to publish outbox relays: {}", e);
                }
            }
            if let Err(e) = relay::publish_favorite_relays(favorites.clone(), client.clone()).await {
                log::warn!("Failed to publish favorite relays: {}", e);
            }
            if !proxy.is_empty() {
                if let Err(e) = relay::publish_proxy_relays(proxy.clone(), client.clone()).await {
                    log::warn!("Failed to publish proxy relays: {}", e);
                }
            }
            if !trusted.is_empty() {
                if let Err(e) = relay::publish_trusted_relays(trusted.clone(), client.clone()).await {
                    log::warn!("Failed to publish trusted relays: {}", e);
                }
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
            *relay::INDEXER_RELAYS.write() = indexer;
            *relay::OUTBOX_RELAYS.write() = outbox;
            *relay::FAVORITE_RELAYS.write() = favorites;
            *relay::PROXY_RELAYS.write() = proxy;
            *relay::TRUSTED_RELAYS.write() = trusted;
            relay::persistence::persist_public_relay_lists();
            // The published values are the new pristine baseline —
            // reconciliation may resume without reverting the just-published
            // state. (The global writes above consumed the locals' clones, so
            // re-read from the local signals, which still hold them.)
            // local/broadcast are intentionally absent: both persist
            // immediately through their own removal handlers, which refresh
            // their baseline fields inline — they are never "unpublished".
            baseline.with_mut(|b| {
                b.general = general_relays.read().clone();
                b.dm = dm_relays.read().clone();
                b.search = search_relays.read().clone();
                b.blocked = blocked_relays.read().clone();
                b.indexer = indexer_relays.read().clone();
                b.outbox = outbox_relays.read().clone();
                b.favorite = favorite_relays.read().clone();
                b.proxy = proxy_relays.read().clone();
                b.trusted = trusted_relays.read().clone();
            });
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
                Link {
                    to: Route::RelayExplorer {},
                    class: "inline-flex items-center gap-2 mt-3 px-4 py-2 bg-primary text-white rounded-lg hover:opacity-80 transition text-sm",
                    "Explore Relays"
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
                                "{section_hint(RelaySectionKind::General)}"
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
                            Link {
                                to: Route::RelayExplorer {},
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400 whitespace-nowrap",
                                "Find more relays"
                            }
                        }
                    }
                    SectionExplainer { kind: RelaySectionKind::General }
                    div { class: "space-y-2 mb-4",
                        for (index , relay_config) in general_relays.read().iter().enumerate() {
                            {
                                let url = relay_config.url.clone();
                                let stats = stats_map.read().get(&url).cloned();
                                rsx! {
                                    div { key: "{url}", class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                        div { class: "flex items-center justify-between",
                                            div { class: "flex items-center gap-1 min-w-0",
                                                if can_open_relay_detail(&url) {
                                                    Link {
                                                        to: relay_detail_route(&url),
                                                        class: "text-sm text-gray-900 dark:text-white hover:underline break-all min-w-0",
                                                        RelayDisplayName { url: url.clone() }
                                                    }
                                                } else {
                                                    RelayDisplayName { url: url.clone() }
                                                }
                                            }
                                            div { class: "flex items-center gap-2",
                                                button {
                                                    class: if relay_config.read { "px-2 py-1 bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200 rounded text-xs font-medium" } else { "px-2 py-1 bg-gray-200 text-gray-600 dark:bg-gray-600 dark:text-gray-400 rounded text-xs font-medium" },
                                                    onclick: move |_| toggle_relay_read(index),
                                                    "R"
                                                }
                                                button {
                                                    class: if relay_config.write { "px-2 py-1 bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200 rounded text-xs font-medium" } else { "px-2 py-1 bg-gray-200 text-gray-600 dark:bg-gray-600 dark:text-gray-400 rounded text-xs font-medium" },
                                                    onclick: move |_| toggle_relay_write(index),
                                                    "W"
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
                    RelayUrlInput {
                        text: new_general_relay,
                        error: general_error,
                        existing: general_relays.read().iter().map(|r| r.url.clone()).collect::<Vec<_>>(),
                        placeholder: "wss://relay.example.com",
                        on_add: move |url: String| {
                            general_relays.write().push(relay::RelayConfig {
                                url: url.clone(),
                                read: true,
                                write: true,
                            });
                            relay::nip11_info::ensure_nip11_for(vec![url]);
                        },
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
                                "{section_hint(RelaySectionKind::DmInbox)}"
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
                            Link {
                                to: Route::RelayExplorer {},
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400 whitespace-nowrap",
                                "Find more relays"
                            }
                        }
                    }
                    SectionExplainer { kind: RelaySectionKind::DmInbox }
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
                                                if can_open_relay_detail(&url_clone) {
                                                    Link {
                                                        to: relay_detail_route(&url_clone),
                                                        class: "text-sm text-gray-900 dark:text-white hover:underline break-all min-w-0",
                                                        RelayDisplayName { url: url_clone.clone() }
                                                    }
                                                } else {
                                                    RelayDisplayName { url: url_clone.clone() }
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
                    RelayUrlInput {
                        text: new_dm_relay,
                        error: dm_error,
                        existing: dm_relays.read().clone(),
                        placeholder: "wss://relay.example.com",
                        on_add: move |url: String| {
                            dm_relays.write().push(url.clone());
                            relay::nip11_info::ensure_nip11_for(vec![url]);
                        },
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
                                "{section_hint(RelaySectionKind::Search)}"
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
                            Link {
                                to: Route::RelayExplorer {},
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400 whitespace-nowrap",
                                "Find more relays"
                            }
                        }
                    }
                    SectionExplainer { kind: RelaySectionKind::Search }
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
                                                if can_open_relay_detail(&url_clone) {
                                                    Link {
                                                        to: relay_detail_route(&url_clone),
                                                        class: "text-sm text-gray-900 dark:text-white hover:underline break-all min-w-0",
                                                        RelayDisplayName { url: url_clone.clone() }
                                                    }
                                                } else {
                                                    RelayDisplayName { url: url_clone.clone() }
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
                    RelayUrlInput {
                        text: new_search_relay,
                        error: search_error,
                        existing: search_relays.read().clone(),
                        placeholder: "wss://relay.nostr.band",
                        on_add: move |url: String| {
                            search_relays.write().push(url.clone());
                            relay::nip11_info::ensure_nip11_for(vec![url]);
                        },
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
                                "{section_hint(RelaySectionKind::Blocked)}"
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "px-2 py-1 bg-muted text-muted-foreground rounded text-xs",
                                "kind 10006"
                            }
                            button {
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400",
                                onclick: move |_| blocked_relays.write().clear(),
                                "Clear all"
                            }
                            Link {
                                to: Route::RelayExplorer {},
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400 whitespace-nowrap",
                                "Find more relays"
                            }
                        }
                    }
                    SectionExplainer { kind: RelaySectionKind::Blocked }
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
                                                RelayDisplayName { url: url_clone.clone() }
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
                    RelayUrlInput {
                        text: new_blocked_relay,
                        error: blocked_error,
                        existing: blocked_relays.read().clone(),
                        placeholder: "wss://spam-relay.example.com",
                        on_add: move |url: String| {
                            blocked_relays.write().push(url.clone());
                            relay::nip11_info::ensure_nip11_for(vec![url]);
                        },
                    }
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div { class: "flex items-center justify-between mb-4",
                        div {
                            h3 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                "Indexer Relays"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                "{section_hint(RelaySectionKind::Indexer)}"
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "px-2 py-1 bg-muted text-muted-foreground rounded text-xs",
                                "kind 10086"
                            }
                            span { class: "px-2 py-1 bg-amber-100 dark:bg-amber-900 text-amber-700 dark:text-amber-300 rounded text-xs",
                                "private"
                            }
                            button {
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400",
                                onclick: move |_| {
                                    indexer_relays.set(relay::default_indexer_relays());
                                },
                                "Reset"
                            }
                            Link {
                                to: Route::RelayExplorer {},
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400 whitespace-nowrap",
                                "Find more relays"
                            }
                        }
                    }
                    SectionExplainer { kind: RelaySectionKind::Indexer }
                    div { class: "space-y-2 mb-4",
                        for (index , url) in indexer_relays.read().iter().enumerate() {
                            {
                                let url_clone = url.clone();
                                let stats = stats_map.read().get(&url_clone).cloned();
                                rsx! {
                                    div { key: "{url_clone}", class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                        div { class: "flex items-center justify-between",
                                            div { class: "flex items-center gap-1 min-w-0",
                                                span { "📡" }
                                                if can_open_relay_detail(&url_clone) {
                                                    Link {
                                                        to: relay_detail_route(&url_clone),
                                                        class: "text-sm text-gray-900 dark:text-white hover:underline break-all min-w-0",
                                                        RelayDisplayName { url: url_clone.clone() }
                                                    }
                                                } else {
                                                    RelayDisplayName { url: url_clone.clone() }
                                                }
                                            }
                                            button {
                                                class: "px-2 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded text-xs transition",
                                                onclick: move |_| remove_indexer_relay(index),
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
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    RelayUrlInput {
                        text: new_indexer_relay,
                        error: indexer_error,
                        existing: indexer_relays.read().clone(),
                        placeholder: "wss://purplepag.es",
                        on_add: move |url: String| {
                            indexer_relays.write().push(url.clone());
                            relay::nip11_info::ensure_nip11_for(vec![url]);
                        },
                    }
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div { class: "flex items-center justify-between mb-4",
                        div {
                            h3 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                "Private Outbox Relays"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                "{section_hint(RelaySectionKind::PrivateOutbox)}"
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "px-2 py-1 bg-muted text-muted-foreground rounded text-xs",
                                "kind 10013"
                            }
                            button {
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400",
                                onclick: move |_| outbox_relays.write().clear(),
                                "Clear all"
                            }
                            Link {
                                to: Route::RelayExplorer {},
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400 whitespace-nowrap",
                                "Find more relays"
                            }
                        }
                    }
                    SectionExplainer { kind: RelaySectionKind::PrivateOutbox }
                    div { class: "space-y-2 mb-4",
                        if outbox_relays.read().is_empty() {
                            div { class: "text-center py-4 text-gray-500 dark:text-gray-400 text-sm",
                                "No outbox relays configured"
                            }
                        }
                        for (index , url) in outbox_relays.read().iter().enumerate() {
                            {
                                let url_clone = url.clone();
                                rsx! {
                                    div { key: "{url_clone}", class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                        div { class: "flex items-center justify-between",
                                            div { class: "flex items-center gap-1 min-w-0",
                                                span { "📤" }
                                                RelayDisplayName { url: url_clone.clone() }
                                            }
                                            button {
                                                class: "px-2 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded text-xs transition",
                                                onclick: move |_| remove_outbox_relay(index),
                                                "✕"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    RelayUrlInput {
                        text: new_outbox_relay,
                        error: outbox_error,
                        existing: outbox_relays.read().clone(),
                        placeholder: "wss://relay.example.com",
                        on_add: move |url: String| {
                            outbox_relays.write().push(url.clone());
                            relay::nip11_info::ensure_nip11_for(vec![url]);
                        },
                    }
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div { class: "flex items-center justify-between mb-4",
                        div {
                            h3 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                "Favorite / Feed Relays"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                "{section_hint(RelaySectionKind::FavoriteFeed)}"
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "px-2 py-1 bg-muted text-muted-foreground rounded text-xs",
                                "kind 10012"
                            }
                            button {
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400",
                                onclick: move |_| {
                                    favorite_relays.set(relay::default_favorite_relays());
                                },
                                "Reset"
                            }
                            Link {
                                to: Route::RelayExplorer {},
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400 whitespace-nowrap",
                                "Find more relays"
                            }
                        }
                    }
                    SectionExplainer { kind: RelaySectionKind::FavoriteFeed }
                    div { class: "space-y-2 mb-4",
                        for (index , url) in favorite_relays.read().iter().enumerate() {
                            {
                                let url_clone = url.clone();
                                let stats = stats_map.read().get(&url_clone).cloned();
                                rsx! {
                                    div { key: "{url_clone}", class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                        div { class: "flex items-center justify-between",
                                            div { class: "flex items-center gap-1 min-w-0",
                                                span { "⭐" }
                                                if can_open_relay_detail(&url_clone) {
                                                    Link {
                                                        to: relay_detail_route(&url_clone),
                                                        class: "text-sm text-gray-900 dark:text-white hover:underline break-all min-w-0",
                                                        RelayDisplayName { url: url_clone.clone() }
                                                    }
                                                } else {
                                                    RelayDisplayName { url: url_clone.clone() }
                                                }
                                            }
                                            button {
                                                class: "px-2 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded text-xs transition",
                                                onclick: move |_| remove_favorite_relay(index),
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
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    RelayUrlInput {
                        text: new_favorite_relay,
                        error: favorite_error,
                        existing: favorite_relays.read().clone(),
                        placeholder: "wss://nostr.wine",
                        on_add: move |url: String| {
                            favorite_relays.write().push(url.clone());
                            relay::nip11_info::ensure_nip11_for(vec![url]);
                        },
                    }
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div { class: "flex items-center justify-between mb-4",
                        div {
                            h3 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                "Proxy Relays"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                "{section_hint(RelaySectionKind::Proxy)}"
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "px-2 py-1 bg-muted text-muted-foreground rounded text-xs",
                                "kind 10087"
                            }
                            span { class: "px-2 py-1 bg-amber-100 dark:bg-amber-900 text-amber-700 dark:text-amber-300 rounded text-xs",
                                "private"
                            }
                            button {
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400",
                                onclick: move |_| proxy_relays.write().clear(),
                                "Clear all"
                            }
                            Link {
                                to: Route::RelayExplorer {},
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400 whitespace-nowrap",
                                "Find more relays"
                            }
                        }
                    }
                    SectionExplainer { kind: RelaySectionKind::Proxy }
                    div { class: "space-y-2 mb-4",
                        if proxy_relays.read().is_empty() {
                            div { class: "text-center py-4 text-gray-500 dark:text-gray-400 text-sm",
                                "No proxy relays configured"
                            }
                        }
                        for (index , url) in proxy_relays.read().iter().enumerate() {
                            {
                                let url_clone = url.clone();
                                rsx! {
                                    div { key: "{url_clone}", class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                        div { class: "flex items-center justify-between",
                                            div { class: "flex items-center gap-1 min-w-0",
                                                span { "🔄" }
                                                RelayDisplayName { url: url_clone.clone() }
                                            }
                                            button {
                                                class: "px-2 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded text-xs transition",
                                                onclick: move |_| remove_proxy_relay(index),
                                                "✕"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    RelayUrlInput {
                        text: new_proxy_relay,
                        error: proxy_error,
                        existing: proxy_relays.read().clone(),
                        placeholder: "wss://relay.example.com",
                        on_add: move |url: String| {
                            proxy_relays.write().push(url.clone());
                            relay::nip11_info::ensure_nip11_for(vec![url]);
                        },
                    }
                }
            }
            if auth.is_authenticated {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
                    div { class: "flex items-center justify-between mb-4",
                        div {
                            h3 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                "Trusted Relays"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                "{section_hint(RelaySectionKind::Trusted)}"
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "px-2 py-1 bg-muted text-muted-foreground rounded text-xs",
                                "kind 10089"
                            }
                            span { class: "px-2 py-1 bg-amber-100 dark:bg-amber-900 text-amber-700 dark:text-amber-300 rounded text-xs",
                                "private"
                            }
                            button {
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400",
                                onclick: move |_| trusted_relays.write().clear(),
                                "Clear all"
                            }
                            Link {
                                to: Route::RelayExplorer {},
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400 whitespace-nowrap",
                                "Find more relays"
                            }
                        }
                    }
                    SectionExplainer { kind: RelaySectionKind::Trusted }
                    div { class: "space-y-2 mb-4",
                        if trusted_relays.read().is_empty() {
                            div { class: "text-center py-4 text-gray-500 dark:text-gray-400 text-sm",
                                "No trusted relays configured"
                            }
                        }
                        for (index , url) in trusted_relays.read().iter().enumerate() {
                            {
                                let url_clone = url.clone();
                                rsx! {
                                    div { key: "{url_clone}", class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                                        div { class: "flex items-center justify-between",
                                            div { class: "flex items-center gap-1 min-w-0",
                                                span { "🔒" }
                                                RelayDisplayName { url: url_clone.clone() }
                                            }
                                            button {
                                                class: "px-2 py-1 bg-red-100 hover:bg-red-200 dark:bg-red-900 dark:hover:bg-red-800 text-red-800 dark:text-red-200 rounded text-xs transition",
                                                onclick: move |_| remove_trusted_relay(index),
                                                "✕"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    RelayUrlInput {
                        text: new_trusted_relay,
                        error: trusted_error,
                        existing: trusted_relays.read().clone(),
                        placeholder: "wss://relay.example.com",
                        on_add: move |url: String| {
                            trusted_relays.write().push(url.clone());
                            relay::nip11_info::ensure_nip11_for(vec![url]);
                        },
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
                                "{section_hint(RelaySectionKind::Local)}"
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "px-2 py-1 bg-purple-100 dark:bg-purple-900 text-purple-600 dark:text-purple-300 rounded text-xs",
                                "local only"
                            }
                            button {
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400",
                                onclick: move |_| {
                                    relay::save_local_relays(&Vec::new());
                                    local_relays.set(Vec::new());
                                    *relay::LOCAL_RELAYS.write() = Vec::new();
                                },
                                "Clear all"
                            }
                            Link {
                                to: Route::RelayExplorer {},
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400 whitespace-nowrap",
                                "Find more relays"
                            }
                        }
                    }
                    SectionExplainer { kind: RelaySectionKind::Local }
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
                                                if can_open_relay_detail(&url_clone) {
                                                    Link {
                                                        to: relay_detail_route(&url_clone),
                                                        class: "text-sm text-gray-900 dark:text-white hover:underline break-all min-w-0",
                                                        RelayDisplayName { url: url_clone.clone() }
                                                    }
                                                } else {
                                                    RelayDisplayName { url: url_clone.clone() }
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
                    RelayUrlInput {
                        text: new_local_relay,
                        error: local_error,
                        existing: local_relays.read().clone(),
                        placeholder: "ws://localhost:7777 or ws://192.168.1.100:4869",
                        allow_insecure: true,
                        on_add: move |url: String| {
                            let mut relays = local_relays.read().clone();
                            relays.push(url.clone());
                            relay::save_local_relays(&relays);
                            local_relays.set(relays.clone());
                            *relay::LOCAL_RELAYS.write() = relays;
                            relay::nip11_info::ensure_nip11_for(vec![url]);
                        },
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
                                "{section_hint(RelaySectionKind::Broadcast)}"
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "px-2 py-1 bg-purple-100 dark:bg-purple-900 text-purple-600 dark:text-purple-300 rounded text-xs",
                                "local only"
                            }
                            button {
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400",
                                onclick: move |_| {
                                    match relay::save_broadcast_relays(&Vec::new()) {
                                        Ok(()) => {
                                            broadcast_relays.set(Vec::new());
                                            *relay::BROADCAST_RELAYS.write() = Vec::new();
                                            broadcast_error.set(None);
                                        }
                                        Err(e) => broadcast_error.set(Some(e)),
                                    }
                                },
                                "Clear all"
                            }
                            Link {
                                to: Route::RelayExplorer {},
                                class: "text-xs text-blue-600 hover:underline dark:text-blue-400 whitespace-nowrap",
                                "Find more relays"
                            }
                        }
                    }
                    SectionExplainer { kind: RelaySectionKind::Broadcast }
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
                                                if can_open_relay_detail(&url_clone) {
                                                    Link {
                                                        to: relay_detail_route(&url_clone),
                                                        class: "text-sm text-gray-900 dark:text-white hover:underline break-all min-w-0",
                                                        RelayDisplayName { url: url_clone.clone() }
                                                    }
                                                } else {
                                                    RelayDisplayName { url: url_clone.clone() }
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
                    RelayUrlInput {
                        text: new_broadcast_relay,
                        error: broadcast_error,
                        existing: broadcast_relays.read().clone(),
                        placeholder: "wss://relay.example.com",
                        allow_insecure: true,
                        on_add: move |url: String| {
                            let mut relays = broadcast_relays.read().clone();
                            relays.push(url.clone());
                            match relay::save_broadcast_relays(&relays) {
                                Ok(()) => {
                                    broadcast_relays.set(relays.clone());
                                    *relay::BROADCAST_RELAYS.write() = relays;
                                    broadcast_error.set(None);
                                }
                                Err(e) => broadcast_error.set(Some(e)),
                            }
                            relay::nip11_info::ensure_nip11_for(vec![url]);
                        },
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
                            "{section_hint(RelaySectionKind::Connected)}"
                        }
                    }
                    div { class: "flex items-center gap-2",
                        span { class: "px-2 py-1 bg-muted text-muted-foreground rounded text-xs",
                            "read-only"
                        }
                        Link {
                            to: Route::RelayExplorer {},
                            class: "text-xs text-blue-600 hover:underline dark:text-blue-400 whitespace-nowrap",
                            "Find more relays"
                        }
                    }
                }
                SectionExplainer { kind: RelaySectionKind::Connected }
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
                                            if can_open_relay_detail(&relay_info.url) {
                                                Link {
                                                    to: relay_detail_route(&relay_info.url),
                                                    class: "text-sm text-gray-900 dark:text-white hover:underline break-all min-w-0",
                                                    RelayDisplayName { url: relay_info.url.clone() }
                                                }
                                            } else {
                                                RelayDisplayName { url: relay_info.url.clone() }
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
                        "Publishes General, DM, Search, Blocked, Indexer, Outbox, Favorites, Proxy, and Trusted relay lists. Local and Broadcast relays are stored locally on this device."
                    }
                }
            }
        }
    }
}
