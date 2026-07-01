//! P2P Orders Home Page
//!
//! NIP-69 P2P Trading - View peer-to-peer Bitcoin orders from the network.
//!
//! Gates the Mostro trading flow on the user accepting the Mostro terms
//! of service (NIP-78). Without acceptance, only the public order book
//! (browsing) is available. With acceptance, the user can also take and
//! create Mostro orders.
use crate::components::{
    ClientInitializing, MostroTermsModal, P2PDepthChart, P2PDepthChartSkeleton, P2POrderCard,
    P2POrderCardSkeleton, P2POrderFilters,
};
use crate::routes::Route;
use crate::services::{btc_price, payments::yadio};
use crate::stores::{
    auth_store,
    nostr_client,
    p2p_store::{self, OrderSortBy, P2PFilterState},
};
use crate::stores::mostro::nip78 as mostro_terms;
use crate::stores::mostro::{
    MOSTRO_NODE_CONFIG,
    ensure_node_relays_connected,
    try_get as try_get_mostro_keys, try_get_node_config,
};
use crate::utils::nip69::{OrderType, P2POrder};
use dioxus::prelude::*;
use nostr::prelude::*;
use std::time::Duration;
/// Order tab selection
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum OrderTab {
    #[default]
    All,
    Buy,
    Sell,
}
#[component]
pub fn MostroHome() -> Element {
    let mut orders = use_signal(Vec::<P2POrder>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut tab = use_signal(|| OrderTab::All);
    let filters = use_signal(P2PFilterState::default);
    let mut sort_by = use_signal(|| OrderSortBy::Newest);
    let mut show_filters = use_signal(|| false);
    let mut show_depth_chart = use_signal(|| true);

    let daemon_pk_hex: Option<PublicKey> = MOSTRO_NODE_CONFIG.read().as_ref().and_then(|n| {
        PublicKey::from_hex(&n.pubkey)
            .or_else(|_| PublicKey::from_bech32(&n.pubkey))
            .ok()
    });
    let daemon_pk_hex_str = daemon_pk_hex.as_ref().map(|pk| pk.to_hex());
    let has_daemon_config = daemon_pk_hex.is_some();
    let mut mostro_only = use_signal(|| has_daemon_config);

    // Lazy Mostro terms check: if the user deep-linked to /p2p before
    // main.rs's first-load NIP-78 batch completed, this re-checks.
    // Reading the signals inside the effect makes it re-run when they change.
    use_effect(move || {
        if !auth_store::is_authenticated() {
            return;
        }
        if !*nostr_client::CLIENT_INITIALIZED.read() {
            return;
        }
        // If we already have a definitive answer (Some(true)/Some(false)), skip.
        if mostro_terms::P2P_TERMS_ACCEPTED.read().is_some() {
            return;
        }
        spawn(async move {
            let _ = mostro_terms::check_p2p_terms_accepted().await;
        });
    });

    // Bug #8 fix: clear TRADE_UNREAD when the user navigates to the Mostro
    // home page. The badge is incremented by the background trade monitor
    // (`start_background_trade_monitor` in client.rs) on every incoming
    // event; clearing it here marks all current events as "seen".
    use_effect(move || {
        *crate::stores::mostro::TRADE_UNREAD.write() = 0;
    });

    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            log::debug!("Waiting for client initialization before loading P2P orders...");
            return;
        }
        spawn(async move {
            loading.set(true);
            error.set(None);
            ensure_node_relays_connected().await;
            if let Err(e) = crate::stores::mostro::sync_relays_from_nip65().await {
                log::warn!("NIP-65 relay sync failed: {e}");
            }
            match p2p_store::fetch_all_orders().await {
                Ok(fetched_orders) => {
                    orders.set(fetched_orders);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });
    use_future(move || async move {
        if btc_price::prices_are_stale() {
            let _ = btc_price::fetch_btc_prices().await;
        }
        if yadio::rates_are_stale() {
            let _ = yadio::fetch_yadio_rates().await;
        }
        loop {
            crate::platform::timer::sleep(Duration::from_secs(30)).await;
            let _ = btc_price::fetch_btc_prices().await;
            let _ = yadio::fetch_yadio_rates().await;
        }
    });
    // Live order book subscription: merge new kind 38383 events into cache
    {
        let live_f = Filter::new()
            .kind(Kind::PeerToPeerOrder)
            .custom_tag(SingleLetterTag::lowercase(Alphabet::Z), "order")
            .limit(0);
        let live_filter = Some(live_f);
        crate::hooks::use_relay_subscription(
            live_filter,
            move |event: &nostr_sdk::Event| {
                p2p_store::upsert_order_from_event(event);
                // Reconcile local trade status from the public order
                // event. This is the safety net for missed GiftWraps:
                // when the daemon updates the kind 38383 status before
                // (or instead of) the GiftWrap arriving, this brings the
                // local trade's status in line with the public board.
                crate::stores::mostro::reconciliation::reconcile_order_event(event);
            },
        );
    }

    // Daemon info subscription: capture kind 38385 events to sync PoW, fees, limits
    {
        let daemon_pk_for_info = try_get_node_config().and_then(|n| {
            PublicKey::from_hex(&n.pubkey)
                .or_else(|_| PublicKey::from_bech32(&n.pubkey))
                .ok()
        });
        let info_filter = daemon_pk_for_info.map(|pk| {
            Filter::new()
                .kind(Kind::Custom(38385))
                .author(pk)
                .limit(0)
        });
        crate::hooks::use_relay_subscription(
            info_filter,
            move |event: &nostr_sdk::Event| {
                crate::stores::mostro::update_pow_from_event(event);
            },
        );
    }

    // NIP-65 relay list subscription: auto-sync when daemon changes relays.
    // The daemon publishes kind 10002 every ~60s. When the list changes we
    // update the cached config and reconnect P2P specialty relays.
    {
        let daemon_pk_for_nip65 = try_get_node_config().and_then(|n| {
            PublicKey::from_hex(&n.pubkey)
                .or_else(|_| PublicKey::from_bech32(&n.pubkey))
                .ok()
        });
        let nip65_filter = daemon_pk_for_nip65.map(|pk| {
            Filter::new()
                .kind(Kind::Custom(10002))
                .author(pk)
                .limit(0)
        });
        crate::hooks::use_relay_subscription(
            nip65_filter,
            move |event: &nostr_sdk::Event| {
                let event = event.clone();
                spawn(async move {
                    if let Err(e) = crate::stores::mostro::node_config::update_relays_from_nip65_event(&event) {
                        log::warn!("NIP-65 relay update failed: {e}");
                        return;
                    }
                    if let Some(client) = crate::stores::nostr_client::get_client() {
                        crate::stores::relay::specialty::ensure_p2p_relays_connected(&client).await;
                    }
                });
            },
        );
    }

    // C1: Mostro exchange-rates subscription (kind 30078, d = "mostro-rates").
    // The daemon publishes Yadio-format BTC/fiat rates every ~5 min. We
    // ingest them as the PREFERRED price source (per Mobile's cascade),
    // falling back to CoinGecko/Yadio when stale. See
    // `services/payments/btc_price.rs::ingest_mostro_rates`.
    {
        let daemon_pk_for_rates = try_get_node_config().and_then(|n| {
            PublicKey::from_hex(&n.pubkey)
                .or_else(|_| PublicKey::from_bech32(&n.pubkey))
                .ok()
        });
        let rates_filter = daemon_pk_for_rates.map(|pk| {
            Filter::new()
                .kind(Kind::Custom(30078))
                .author(pk)
                .identifier("mostro-rates")
                .limit(1)
        });
        crate::hooks::use_relay_subscription(
            rates_filter,
            move |event: &nostr_sdk::Event| {
                if let Err(e) = btc_price::ingest_mostro_rates(&event.content) {
                    log::debug!("Mostro rates ingest failed: {e}");
                }
            },
        );
    }

    // The session-driven GiftWrap subscription (active trade pubkeys +
    // identity key, daemon replies for restore/Orders/per-trade actions)
    // now lives in the always-mounted `use_mostro_session` hook in the
    // root Layout (`routes/mod.rs`), so daemon replies are caught on every
    // route. This page keeps only the order-board subscription above.

    // Trigger session restore once per session when keys and node are ready
    {
        use_future(move || async move {
            if !*nostr_client::CLIENT_INITIALIZED.read() {
                return;
            }
            if try_get_mostro_keys().is_none() {
                return;
            }
            if try_get_node_config().is_none() {
                return;
            }
            if crate::stores::mostro::restore::RESTORE_STATE.read().stage
                != crate::stores::mostro::restore::RestoreStage::Idle
            {
                return;
            }
            if let Err(e) =
                crate::stores::mostro::restore::request_restore().await
            {
                log::warn!("Mostro restore request failed: {e}");
            }
        });
    }

    let filtered_orders = use_memo(move || {
        let initial_orders = orders.read();
        let cached = p2p_store::get_all_cached_orders();
        let mut dedup = std::collections::HashMap::<String, P2POrder>::new();
        for o in cached.iter().chain(initial_orders.iter()) {
            dedup
                .entry(o.naddr.clone())
                .and_modify(|existing| {
                    if o.created_at > existing.created_at {
                        *existing = o.clone();
                    }
                })
                .or_insert_with(|| o.clone());
        }
        let all_orders: Vec<P2POrder> = dedup.into_values().collect();
        let current_tab = *tab.read();
        let current_filters = filters.read().clone();
        let current_sort = *sort_by.read();
        let only_mostro = *mostro_only.read();
        let mut result: Vec<P2POrder> = all_orders
            .iter()
            .filter(|o| match current_tab {
                OrderTab::All => true,
                OrderTab::Buy => o.order_type == OrderType::Buy,
                OrderTab::Sell => o.order_type == OrderType::Sell,
            })
            .cloned()
            .collect();
        if only_mostro {
            if let Some(ref pk_hex) = daemon_pk_hex_str {
                result.retain(|o| o.pubkey == *pk_hex);
            } else {
                result.retain(|o| o.platform.as_deref() == Some("mostro"));
            }
        }
        if !current_filters.is_empty() {
            result = p2p_store::filter_orders(&result, &current_filters);
        }
        result.retain(|o| o.time_remaining().map(|t| t > 0).unwrap_or(true));
        p2p_store::sort_orders(&mut result, current_sort);
        result
    });
    let effective_price = use_memo(move || {
        let base_price = btc_price::get_btc_price("USD")?;
        let all_orders = orders.read();
        let premiums: Vec<f64> = all_orders
            .iter()
            .filter(|o| o.is_active())
            .filter_map(|o| o.premium)
            .collect();
        if premiums.is_empty() {
            return Some(base_price);
        }
        let avg_premium = premiums.iter().sum::<f64>() / premiums.len() as f64;
        Some(base_price * (1.0 + avg_premium / 100.0))
    });
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/95 backdrop-blur border-b border-border",
                div { class: "px-4 py-3",
                    div { class: "flex items-center justify-between",
                        h1 { class: "text-xl font-bold", "P2P Trading" }
                        div { class: "flex items-center gap-2",
                            button {
                                class: "px-3 py-1.5 text-sm border border-border rounded-lg hover:bg-accent transition",
                                title: "P2P settings",
                                onclick: move |_| {
                                    let _ = navigator().push(Route::SettingsMostro {});
                                },
                                svg {
                                    class: "w-4 h-4",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path { d: "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" }
                                    path { d: "M15 12a3 3 0 11-6 0 3 3 0 016 0z" }
                                }
                            }
                            button {
                                class: "px-3 py-1.5 text-sm border border-border rounded-lg hover:bg-accent transition",
                                title: "View your trades",
                                onclick: move |_| {
                                    let _ = navigator().push(Route::MostroMyTrades {});
                                },
                                "My Trades"
                            }
                            button {
                                class: "px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition flex items-center gap-1",
                                title: "Create a new Mostro order (requires accepted terms)",
                                onclick: move |_| {
                                    let _ = navigator().push(Route::MostroCreateOrder {});
                                },
                                svg {
                                    class: "w-4 h-4",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path { d: "M12 4v16m8-8H4" }
                                }
                                "Create"
                            }
                        }
                    }
                    div { class: "flex items-center justify-between mt-2",
                        span { class: "text-sm text-muted-foreground",
                            "{filtered_orders.read().len()} orders"
                        }
                        button {
                            class: "flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition",
                            onclick: move |_| show_depth_chart.toggle(),
                            svg {
                                class: "w-4 h-4",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path { d: "M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" }
                            }
                            if *show_depth_chart.read() {
                                "Hide Chart"
                            } else {
                                "Show Chart"
                            }
                        }
                    }
                }
                div { class: "flex items-center justify-between border-b border-border",
                    div { class: "flex",
                        TabButton {
                            label: "All",
                            active: *tab.read() == OrderTab::All,
                            onclick: move |_| tab.set(OrderTab::All),
                        }
                        TabButton {
                            label: "Buy",
                            active: *tab.read() == OrderTab::Buy,
                            onclick: move |_| tab.set(OrderTab::Buy),
                        }
                        TabButton {
                            label: "Sell",
                            active: *tab.read() == OrderTab::Sell,
                            onclick: move |_| tab.set(OrderTab::Sell),
                        }
                    }
                    if let Some(price) = *effective_price.read() {
                        div { class: "flex items-center gap-1.5 px-4 py-2",
                            span { class: "text-xs text-muted-foreground", "BTC" }
                            span { class: "text-sm font-medium text-foreground",
                                "~${format_price(price)}"
                            }
                        }
                    }
                }
                div { class: "px-4 py-2 flex items-center justify-between",
                    div { class: "flex items-center gap-2",
                        button {
                            class: "flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition",
                            onclick: move |_| show_filters.toggle(),
                            svg {
                                class: "w-4 h-4",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path { d: "M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z" }
                            }
                            "Filters"
                            if !filters.read().is_empty() {
                                span { class: "px-1.5 py-0.5 text-xs bg-primary text-primary-foreground rounded-full",
                                    "Active"
                                }
                            }
                        }
                        button {
                            class: if *mostro_only.read() {
                                "flex items-center gap-1.5 px-2 py-1 text-xs rounded-full bg-primary text-primary-foreground transition"
                            } else {
                                "flex items-center gap-1.5 px-2 py-1 text-xs rounded-full border border-border text-muted-foreground hover:text-foreground transition"
                            },
                            title: if has_daemon_config {
                                "Show only orders from your configured daemon"
                            } else {
                                "Show only Mostro-sourced orders (excludes RoboSats, Peach, etc.)"
                            },
                            onclick: move |_| mostro_only.toggle(),
                            if *mostro_only.read() {
                                if has_daemon_config {
                                    "✓ Daemon only"
                                } else {
                                    "✓ Mostro only"
                                }
                            } else {
                                if has_daemon_config {
                                    "Daemon only"
                                } else {
                                    "Mostro only"
                                }
                            }
                        }
                    }
                    select {
                        class: "text-sm bg-transparent border border-border rounded px-2 py-1",
                        value: "{sort_by.read():?}",
                        onchange: move |evt| {
                            let value = evt.value();
                            let new_sort = match value.as_str() {
                                "Newest" => OrderSortBy::Newest,
                                "Oldest" => OrderSortBy::Oldest,
                                "PremiumLow" => OrderSortBy::PremiumLow,
                                "PremiumHigh" => OrderSortBy::PremiumHigh,
                                "AmountLow" => OrderSortBy::AmountLow,
                                "AmountHigh" => OrderSortBy::AmountHigh,
                                "RatingHigh" => OrderSortBy::RatingHigh,
                                "RatingLow" => OrderSortBy::RatingLow,
                                _ => OrderSortBy::Newest,
                            };
                            sort_by.set(new_sort);
                        },
                        option { value: "Newest", "Newest First" }
                        option { value: "Oldest", "Oldest First" }
                        option { value: "PremiumLow", "Premium: Low to High" }
                        option { value: "PremiumHigh", "Premium: High to Low" }
                        option { value: "AmountLow", "Amount: Low to High" }
                        option { value: "AmountHigh", "Amount: High to Low" }
                    }
                }
            }
            if *show_filters.read() {
                P2POrderFilters {
                    filters,
                    on_apply: move |_| {
                        show_filters.set(false);
                    },
                    on_clear: move |_| {
                        show_filters.set(false);
                    },
                }
            }
            if *show_depth_chart.read() {
                div { class: "p-4 border-b border-border",
                    h3 { class: "text-sm font-medium text-muted-foreground mb-3", "Market Depth" }
                    if *loading.read() {
                        P2PDepthChartSkeleton {}
                    } else {
                        P2PDepthChart { orders: filtered_orders.read().clone() }
                    }
                }
            }
            div { class: "divide-y divide-border",
                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else if auth_store::is_authenticated()
                    && *mostro_terms::P2P_TERMS_ACCEPTED.read() == Some(false)
                {
                    MostroTermsModal { on_accept: move |_| {} }
                } else if *loading.read() {
                    for _ in 0..5 {
                        P2POrderCardSkeleton {}
                    }
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "p-8 text-center",
                        p { class: "text-red-500 mb-4", "Failed to load orders: {err}" }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                            onclick: move |_| {
                                spawn(async move {
                                    loading.set(true);
                                    error.set(None);
                                    if let Ok(fetched) = p2p_store::fetch_all_orders().await {
                                        orders.set(fetched);
                                    }
                                    loading.set(false);
                                });
                            },
                            "Retry"
                        }
                    }
                } else if filtered_orders.read().is_empty() {
                    div { class: "p-8 text-center",
                        div { class: "text-4xl mb-4", "📊" }
                        h3 { class: "text-lg font-medium mb-2", "No orders found" }
                        p { class: "text-muted-foreground",
                            "No P2P orders match your current filters. Try adjusting the filters or check back later."
                        }
                    }
                } else {
                    for order in filtered_orders.read().iter() {
                        P2POrderCard { key: "{order.naddr}", order: order.clone() }
                    }
                }
            }
        }
    }
}
/// Tab button component
#[component]
fn TabButton(label: &'static str, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let class = if active {
        "px-4 py-3 text-sm font-medium border-b-2 border-primary text-primary"
    } else {
        "px-4 py-3 text-sm font-medium border-b-2 border-transparent text-muted-foreground hover:text-foreground hover:border-border transition"
    };
    rsx! {
        button { class: "{class}", onclick: move |e| onclick.call(e), "{label}" }
    }
}
/// Format price with thousands separators
fn format_price(price: f64) -> String {
    let rounded = price.round() as u64;
    let s = rounded.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
