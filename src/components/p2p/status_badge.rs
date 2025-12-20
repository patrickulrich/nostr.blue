//! P2P Order Status Badge Component
//!
//! Color-coded status indicator for NIP-69 P2P orders

use dioxus::prelude::*;
use crate::utils::nip69::OrderStatus;

/// Status badge with color-coded styling
#[component]
pub fn P2PStatusBadge(status: OrderStatus) -> Element {
    let (class, label) = match status {
        OrderStatus::Pending => (
            "bg-yellow-500/20 text-yellow-600 dark:text-yellow-400",
            "Pending"
        ),
        OrderStatus::InProgress => (
            "bg-blue-500/20 text-blue-600 dark:text-blue-400",
            "In Progress"
        ),
        OrderStatus::Success => (
            "bg-green-500/20 text-green-600 dark:text-green-400",
            "Completed"
        ),
        OrderStatus::Canceled => (
            "bg-gray-500/20 text-gray-600 dark:text-gray-400",
            "Canceled"
        ),
        OrderStatus::Expired => (
            "bg-red-500/20 text-red-600 dark:text-red-400",
            "Expired"
        ),
    };

    rsx! {
        span {
            class: "px-2 py-0.5 text-xs font-medium rounded-full {class}",
            "{label}"
        }
    }
}

/// Order type badge (Buy/Sell)
#[component]
pub fn P2PTypeBadge(order_type: crate::utils::nip69::OrderType) -> Element {
    let (class, label) = match order_type {
        crate::utils::nip69::OrderType::Buy => (
            "bg-green-500/20 text-green-600 dark:text-green-400",
            "BUY"
        ),
        crate::utils::nip69::OrderType::Sell => (
            "bg-red-500/20 text-red-600 dark:text-red-400",
            "SELL"
        ),
    };

    rsx! {
        span {
            class: "px-2 py-1 text-xs font-bold rounded {class}",
            "{label}"
        }
    }
}

/// Layer badge (Lightning/Onchain/Liquid)
#[component]
pub fn P2PLayerBadge(layer: crate::utils::nip69::Layer) -> Element {
    let (icon, label) = match layer {
        crate::utils::nip69::Layer::Lightning => ("⚡", "Lightning"),
        crate::utils::nip69::Layer::Onchain => ("🔗", "On-chain"),
        crate::utils::nip69::Layer::Liquid => ("💧", "Liquid"),
    };

    rsx! {
        span {
            class: "inline-flex items-center gap-1 px-2 py-0.5 text-xs bg-muted rounded",
            span { "{icon}" }
            span { "{label}" }
        }
    }
}

/// Network badge (Mainnet/Testnet/Signet)
#[component]
pub fn P2PNetworkBadge(network: crate::utils::nip69::Network) -> Element {
    let (class, label) = match network {
        crate::utils::nip69::Network::Mainnet => (
            "bg-orange-500/20 text-orange-600 dark:text-orange-400",
            "Mainnet"
        ),
        crate::utils::nip69::Network::Testnet => (
            "bg-purple-500/20 text-purple-600 dark:text-purple-400",
            "Testnet"
        ),
        crate::utils::nip69::Network::Signet => (
            "bg-cyan-500/20 text-cyan-600 dark:text-cyan-400",
            "Signet"
        ),
    };

    rsx! {
        span {
            class: "px-2 py-0.5 text-xs font-medium rounded {class}",
            "{label}"
        }
    }
}
