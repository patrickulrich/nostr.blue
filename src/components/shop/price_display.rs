//! PriceDisplay component - shows price in sats with optional fiat conversion
use crate::utils::format::format_sats_with_separator;
use dioxus::prelude::*;
#[derive(Props, Clone, PartialEq)]
pub struct PriceDisplayProps {
    pub price_sats: u64,
    /// Optional currency for display (not yet implemented)
    #[props(default)]
    pub currency: Option<String>,
    /// Show larger font
    #[props(default = false)]
    pub large: bool,
}
/// Price display with sats formatting
#[component]
pub fn PriceDisplay(props: PriceDisplayProps) -> Element {
    let formatted = format_sats_with_separator(props.price_sats);
    let size_class = if props.large {
        "text-2xl font-bold"
    } else {
        "text-base font-semibold"
    };
    rsx! {
        div { class: "flex items-center gap-1 {size_class}",
            span { class: "text-amber-500", "⚡" }
            span { "{formatted}" }
            span { class: "text-muted-foreground text-sm font-normal", "sats" }
        }
    }
}
/// Compact price display for tight spaces
#[component]
pub fn PriceDisplayCompact(price_sats: u64) -> Element {
    let formatted = if price_sats >= 1_000_000 {
        if price_sats % 1_000_000 == 0 {
            format!("{}M", price_sats / 1_000_000)
        } else {
            format!("{:.1}M", price_sats as f64 / 1_000_000.0)
        }
    } else if price_sats >= 1_000 {
        if price_sats % 1_000 == 0 {
            format!("{}k", price_sats / 1_000)
        } else {
            format!("{:.1}k", price_sats as f64 / 1_000.0)
        }
    } else {
        price_sats.to_string()
    };
    rsx! {
        span { class: "text-sm font-medium", "⚡{formatted}" }
    }
}
