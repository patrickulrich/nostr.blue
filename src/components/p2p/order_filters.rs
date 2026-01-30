//! P2P Order Filters Component
//!
//! Advanced filtering panel for NIP-69 P2P orders

use crate::stores::p2p_store::P2PFilterState;
use crate::utils::nip69::{Layer, Network, OrderStatus};
use dioxus::prelude::*;

/// Common currencies for quick selection
const COMMON_CURRENCIES: &[&str] = &["USD", "EUR", "GBP", "BRL", "MXN", "ARS", "CAD", "AUD"];

// ============================================================================
// Generic Filter Chip Component
// ============================================================================

/// Generic filter chip that renders a toggle button with active/inactive states.
/// Reduces duplication across status, network, layer, and payment method chips.
#[component]
fn FilterChip(label: String, is_active: bool, on_toggle: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: if is_active {
                "px-3 py-1 text-sm rounded-full bg-primary text-primary-foreground"
            } else {
                "px-3 py-1 text-sm rounded-full bg-muted hover:bg-accent transition"
            },
            onclick: move |_| on_toggle.call(()),
            "{label}"
        }
    }
}

/// Common payment methods
const COMMON_PAYMENT_METHODS: &[&str] = &[
    "Bank Transfer",
    "PayPal",
    "Revolut",
    "Wise",
    "Venmo",
    "Zelle",
    "Cash App",
    "PIX",
    "SEPA",
    "Cash",
];

/// Known P2P platforms
const KNOWN_PLATFORMS: &[&str] = &["robosats", "mostro", "lnp2pbot", "peach"];

#[component]
pub fn P2POrderFilters(
    filters: Signal<P2PFilterState>,
    on_apply: EventHandler<()>,
    on_clear: EventHandler<()>,
) -> Element {
    // Local state for form inputs
    let mut currency_input = use_signal(|| filters.read().currency.clone().unwrap_or_default());
    let mut min_amount = use_signal(|| {
        filters
            .read()
            .min_amount
            .map(|a| a.to_string())
            .unwrap_or_default()
    });
    let mut max_amount = use_signal(|| {
        filters
            .read()
            .max_amount
            .map(|a| a.to_string())
            .unwrap_or_default()
    });

    // Apply filters
    let apply_filters = move |_| {
        let mut f = filters.write();

        // Currency
        let curr = currency_input.read().trim().to_string();
        f.currency = if curr.is_empty() {
            None
        } else {
            Some(curr.to_uppercase())
        };

        // Amount range
        f.min_amount = min_amount.read().parse().ok();
        f.max_amount = max_amount.read().parse().ok();

        drop(f);
        on_apply.call(());
    };

    // Clear all filters
    let clear_filters = move |_| {
        filters.write().clear();
        currency_input.set(String::new());
        min_amount.set(String::new());
        max_amount.set(String::new());
        on_clear.call(());
    };

    // Check if any filters are active
    let has_active_filters = !filters.read().is_empty();

    rsx! {
        div {
            class: "p-4 bg-card border-b border-border space-y-4",

            // Status filters
            div {
                h4 {
                    class: "text-sm font-medium mb-2",
                    "Status"
                }
                div {
                    class: "flex flex-wrap gap-2",
                    StatusFilterChip {
                        status: OrderStatus::Pending,
                        filters: filters,
                        label: "Pending"
                    }
                    StatusFilterChip {
                        status: OrderStatus::InProgress,
                        filters: filters,
                        label: "In Progress"
                    }
                    StatusFilterChip {
                        status: OrderStatus::Success,
                        filters: filters,
                        label: "Completed"
                    }
                }
            }

            // Currency filter
            div {
                h4 {
                    class: "text-sm font-medium mb-2",
                    "Currency"
                }
                div {
                    class: "flex flex-wrap gap-2 mb-2",
                    for curr in COMMON_CURRENCIES.iter() {
                        button {
                            class: if filters.read().currency.as_ref().map(|c| c == *curr).unwrap_or(false) {
                                "px-3 py-1 text-sm rounded-full bg-primary text-primary-foreground"
                            } else {
                                "px-3 py-1 text-sm rounded-full bg-muted hover:bg-accent transition"
                            },
                            onclick: {
                                let curr = curr.to_string();
                                move |_| {
                                    let current = filters.read().currency.clone();
                                    if current.as_ref().map(|c| c == &curr).unwrap_or(false) {
                                        filters.write().currency = None;
                                        currency_input.set(String::new());
                                    } else {
                                        filters.write().currency = Some(curr.clone());
                                        currency_input.set(curr.clone());
                                    }
                                }
                            },
                            "{curr}"
                        }
                    }
                }
                input {
                    r#type: "text",
                    class: "w-full py-2 px-3 bg-muted rounded-lg text-sm",
                    placeholder: "Other currency (e.g., COP, CLP)...",
                    value: "{currency_input}",
                    oninput: move |evt| currency_input.set(evt.value())
                }
            }

            // Amount range
            div {
                h4 {
                    class: "text-sm font-medium mb-2",
                    "Fiat Amount Range"
                }
                div {
                    class: "flex gap-2",
                    input {
                        r#type: "number",
                        class: "flex-1 py-2 px-3 bg-muted rounded-lg text-sm",
                        placeholder: "Min",
                        value: "{min_amount}",
                        oninput: move |evt| min_amount.set(evt.value())
                    }
                    span {
                        class: "self-center text-muted-foreground",
                        "-"
                    }
                    input {
                        r#type: "number",
                        class: "flex-1 py-2 px-3 bg-muted rounded-lg text-sm",
                        placeholder: "Max",
                        value: "{max_amount}",
                        oninput: move |evt| max_amount.set(evt.value())
                    }
                }
            }

            // Network filter
            div {
                h4 {
                    class: "text-sm font-medium mb-2",
                    "Network"
                }
                div {
                    class: "flex flex-wrap gap-2",
                    NetworkFilterChip {
                        network: Network::Mainnet,
                        filters: filters,
                        label: "Mainnet"
                    }
                    NetworkFilterChip {
                        network: Network::Testnet,
                        filters: filters,
                        label: "Testnet"
                    }
                    NetworkFilterChip {
                        network: Network::Signet,
                        filters: filters,
                        label: "Signet"
                    }
                }
            }

            // Layer filter
            div {
                h4 {
                    class: "text-sm font-medium mb-2",
                    "Layer"
                }
                div {
                    class: "flex flex-wrap gap-2",
                    LayerFilterChip {
                        layer: Layer::Lightning,
                        filters: filters,
                        label: "Lightning"
                    }
                    LayerFilterChip {
                        layer: Layer::Onchain,
                        filters: filters,
                        label: "On-chain"
                    }
                    LayerFilterChip {
                        layer: Layer::Liquid,
                        filters: filters,
                        label: "Liquid"
                    }
                }
            }

            // Payment methods
            div {
                h4 {
                    class: "text-sm font-medium mb-2",
                    "Payment Methods"
                }
                div {
                    class: "flex flex-wrap gap-2",
                    for method in COMMON_PAYMENT_METHODS.iter() {
                        PaymentMethodChip {
                            method: method.to_string(),
                            filters: filters
                        }
                    }
                }
            }

            // Platform filter
            div {
                h4 {
                    class: "text-sm font-medium mb-2",
                    "Platform"
                }
                div {
                    class: "flex flex-wrap gap-2",
                    for platform in KNOWN_PLATFORMS.iter() {
                        button {
                            class: if filters.read().platform.as_ref().map(|p| p == *platform).unwrap_or(false) {
                                "px-3 py-1 text-sm rounded-full bg-primary text-primary-foreground"
                            } else {
                                "px-3 py-1 text-sm rounded-full bg-muted hover:bg-accent transition"
                            },
                            onclick: {
                                let platform = platform.to_string();
                                move |_| {
                                    let current = filters.read().platform.clone();
                                    if current.as_ref().map(|p| p == &platform).unwrap_or(false) {
                                        filters.write().platform = None;
                                    } else {
                                        filters.write().platform = Some(platform.clone());
                                    }
                                }
                            },
                            "{platform}"
                        }
                    }
                }
            }

            // Action buttons
            div {
                class: "flex gap-2 pt-2",
                button {
                    class: "flex-1 py-2 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition",
                    onclick: apply_filters,
                    "Apply Filters"
                }
                if has_active_filters {
                    button {
                        class: "px-4 py-2 bg-muted hover:bg-accent rounded-lg transition",
                        onclick: clear_filters,
                        "Clear All"
                    }
                }
            }
        }
    }
}

/// Status filter chip - uses generic FilterChip
#[component]
fn StatusFilterChip(
    status: OrderStatus,
    filters: Signal<P2PFilterState>,
    label: &'static str,
) -> Element {
    let is_active = filters.read().status.map(|s| s == status).unwrap_or(false);

    rsx! {
        FilterChip {
            label: label.to_string(),
            is_active,
            on_toggle: move |_| {
                if filters.read().status.map(|s| s == status).unwrap_or(false) {
                    filters.write().status = None;
                } else {
                    filters.write().status = Some(status);
                }
            }
        }
    }
}

/// Network filter chip - uses generic FilterChip
#[component]
fn NetworkFilterChip(
    network: Network,
    filters: Signal<P2PFilterState>,
    label: &'static str,
) -> Element {
    let is_active = filters
        .read()
        .network
        .map(|n| n == network)
        .unwrap_or(false);

    rsx! {
        FilterChip {
            label: label.to_string(),
            is_active,
            on_toggle: move |_| {
                if filters.read().network.map(|n| n == network).unwrap_or(false) {
                    filters.write().network = None;
                } else {
                    filters.write().network = Some(network);
                }
            }
        }
    }
}

/// Layer filter chip - uses generic FilterChip
#[component]
fn LayerFilterChip(layer: Layer, filters: Signal<P2PFilterState>, label: &'static str) -> Element {
    let is_active = filters.read().layer.map(|l| l == layer).unwrap_or(false);

    rsx! {
        FilterChip {
            label: label.to_string(),
            is_active,
            on_toggle: move |_| {
                if filters.read().layer.map(|l| l == layer).unwrap_or(false) {
                    filters.write().layer = None;
                } else {
                    filters.write().layer = Some(layer);
                }
            }
        }
    }
}

/// Payment method filter chip - uses generic FilterChip
#[component]
fn PaymentMethodChip(method: String, filters: Signal<P2PFilterState>) -> Element {
    let is_active = filters
        .read()
        .payment_method
        .as_ref()
        .map(|m| m == &method)
        .unwrap_or(false);
    let method_for_toggle = method.clone();

    rsx! {
        FilterChip {
            label: method,
            is_active,
            on_toggle: move |_| {
                let method = method_for_toggle.clone();
                if filters.read().payment_method.as_ref().map(|m| m == &method).unwrap_or(false) {
                    filters.write().payment_method = None;
                } else {
                    filters.write().payment_method = Some(method.clone());
                }
            }
        }
    }
}
