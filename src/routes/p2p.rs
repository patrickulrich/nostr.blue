//! P2P Orders Home Page
//!
//! NIP-69 P2P Trading - View peer-to-peer Bitcoin orders from the network

use dioxus::prelude::*;
use std::time::Duration;

use crate::components::{
    ClientInitializing, P2PDepthChart, P2PDepthChartSkeleton, P2POrderCard, P2POrderCardSkeleton,
    P2POrderFilters,
};
use crate::services::btc_price;
use crate::stores::{
    nostr_client,
    p2p_store::{self, OrderSortBy, P2PFilterState},
};
use crate::utils::nip69::{OrderType, P2POrder};

/// Order tab selection
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum OrderTab {
    #[default]
    All,
    Buy,
    Sell,
}

#[component]
pub fn P2PHome() -> Element {
    // State
    let mut orders = use_signal(Vec::<P2POrder>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut tab = use_signal(|| OrderTab::All);
    let filters = use_signal(P2PFilterState::default);
    let mut sort_by = use_signal(|| OrderSortBy::Newest);
    let mut show_filters = use_signal(|| false);
    let mut show_depth_chart = use_signal(|| true); // Show depth chart by default

    // Load ALL orders on mount (full order book) - wait for client initialization
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            log::debug!("Waiting for client initialization before loading P2P orders...");
            return;
        }

        spawn(async move {
            loading.set(true);
            error.set(None);

            // Fetch ALL orders for complete market depth
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

    // BTC price refresh - fetch immediately if stale, then every 30 seconds
    use_future(move || async move {
        // Fetch immediately if prices are stale
        if btc_price::prices_are_stale() {
            let _ = btc_price::fetch_btc_prices().await;
        }
        loop {
            gloo_timers::future::sleep(Duration::from_secs(30)).await;
            let _ = btc_price::fetch_btc_prices().await;
        }
    });

    // Filter and sort orders based on current state
    let filtered_orders = use_memo(move || {
        let all_orders = orders.read();
        let current_tab = *tab.read();
        let current_filters = filters.read().clone();
        let current_sort = *sort_by.read();

        // First filter by tab
        let mut result: Vec<P2POrder> = all_orders
            .iter()
            .filter(|o| match current_tab {
                OrderTab::All => true,
                OrderTab::Buy => o.order_type == OrderType::Buy,
                OrderTab::Sell => o.order_type == OrderType::Sell,
            })
            .cloned()
            .collect();

        // Apply additional filters
        if !current_filters.is_empty() {
            result = p2p_store::filter_orders(&result, &current_filters);
        }

        // Filter out expired orders
        result.retain(|o| {
            // Keep if no expiration or still has time remaining
            o.time_remaining().map(|t| t > 0).unwrap_or(true)
        });

        // Sort
        p2p_store::sort_orders(&mut result, current_sort);

        result
    });

    // Calculate effective P2P price (CoinGecko + average premium from orders)
    let effective_price = use_memo(move || {
        // Get base price from CoinGecko
        let base_price = btc_price::get_btc_price("USD")?;

        // Calculate average premium from active orders
        let all_orders = orders.read();
        let premiums: Vec<f64> = all_orders
            .iter()
            .filter(|o| o.is_active())
            .filter_map(|o| o.premium)
            .collect();

        if premiums.is_empty() {
            // No premium data, just show base price
            return Some(base_price);
        }

        let avg_premium = premiums.iter().sum::<f64>() / premiums.len() as f64;

        // Apply average premium to base price
        Some(base_price * (1.0 + avg_premium / 100.0))
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/95 backdrop-blur border-b border-border",
                div {
                    class: "px-4 py-3",
                    div {
                        class: "flex items-center justify-between",
                        h1 {
                            class: "text-xl font-bold",
                            "P2P Trading"
                        }
                    }

                    // Stats and chart toggle
                    div {
                        class: "flex items-center justify-between mt-2",
                        span {
                            class: "text-sm text-muted-foreground",
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
                                path {
                                    d: "M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"
                                }
                            }
                            if *show_depth_chart.read() {
                                "Hide Chart"
                            } else {
                                "Show Chart"
                            }
                        }
                    }
                }

                // Tabs with market price
                div {
                    class: "flex items-center justify-between border-b border-border",

                    // Tab buttons
                    div {
                        class: "flex",
                        TabButton {
                            label: "All",
                            active: *tab.read() == OrderTab::All,
                            onclick: move |_| tab.set(OrderTab::All)
                        }
                        TabButton {
                            label: "Buy",
                            active: *tab.read() == OrderTab::Buy,
                            onclick: move |_| tab.set(OrderTab::Buy)
                        }
                        TabButton {
                            label: "Sell",
                            active: *tab.read() == OrderTab::Sell,
                            onclick: move |_| tab.set(OrderTab::Sell)
                        }
                    }

                    // Effective P2P price (CoinGecko + avg premium)
                    if let Some(price) = *effective_price.read() {
                        div {
                            class: "flex items-center gap-1.5 px-4 py-2",
                            span {
                                class: "text-xs text-muted-foreground",
                                "BTC"
                            }
                            span {
                                class: "text-sm font-medium text-foreground",
                                "~${format_price(price)}"
                            }
                        }
                    }
                }

                // Filter toggle
                div {
                    class: "px-4 py-2 flex items-center justify-between",
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
                            path {
                                d: "M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z"
                            }
                        }
                        "Filters"
                        if !filters.read().is_empty() {
                            span {
                                class: "px-1.5 py-0.5 text-xs bg-primary text-primary-foreground rounded-full",
                                "Active"
                            }
                        }
                    }

                    // Sort dropdown
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

            // Filter panel (collapsible)
            if *show_filters.read() {
                P2POrderFilters {
                    filters: filters,
                    on_apply: move |_| {
                        show_filters.set(false);
                    },
                    on_clear: move |_| {
                        show_filters.set(false);
                    }
                }
            }

            // Depth Chart (collapsible)
            if *show_depth_chart.read() {
                div {
                    class: "p-4 border-b border-border",
                    h3 {
                        class: "text-sm font-medium text-muted-foreground mb-3",
                        "Market Depth"
                    }
                    if *loading.read() {
                        P2PDepthChartSkeleton {}
                    } else {
                        P2PDepthChart {
                            orders: filtered_orders.read().clone()
                        }
                    }
                }
            }

            // Orders list
            div {
                class: "divide-y divide-border",

                // Client initialization check
                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                // Loading state
                } else if *loading.read() {
                    for _ in 0..5 {
                        P2POrderCardSkeleton {}
                    }
                } else if let Some(err) = error.read().as_ref() {
                    // Error state
                    div {
                        class: "p-8 text-center",
                        p {
                            class: "text-red-500 mb-4",
                            "Failed to load orders: {err}"
                        }
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
                    // Empty state
                    div {
                        class: "p-8 text-center",
                        div {
                            class: "text-4xl mb-4",
                            "📊"
                        }
                        h3 {
                            class: "text-lg font-medium mb-2",
                            "No orders found"
                        }
                        p {
                            class: "text-muted-foreground",
                            "No P2P orders match your current filters. Try adjusting the filters or check back later."
                        }
                    }
                } else {
                    // Orders list
                    for order in filtered_orders.read().iter() {
                        P2POrderCard {
                            key: "{order.naddr}",
                            order: order.clone()
                        }
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
        button {
            class: "{class}",
            onclick: move |e| onclick.call(e),
            "{label}"
        }
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
