//! CartSummary component - displays cart totals

use crate::utils::format::format_sats_with_separator;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct CartSummaryProps {
    pub total_sats: u64,
    #[props(default = 0)]
    pub shipping_sats: u64,
    #[props(default = 0)]
    pub item_count: usize,
}

/// Cart summary with totals
#[component]
pub fn CartSummary(props: CartSummaryProps) -> Element {
    let subtotal_formatted = format_sats_with_separator(props.total_sats);
    let shipping_formatted = format_sats_with_separator(props.shipping_sats);
    let grand_total = props.total_sats + props.shipping_sats;
    let grand_total_formatted = format_sats_with_separator(grand_total);

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 space-y-3",
            h3 { class: "font-semibold text-lg mb-3", "Order Summary" }

            // Subtotal
            div { class: "flex justify-between text-sm",
                span { class: "text-muted-foreground",
                    {
                        let suffix = if props.item_count != 1 { "s" } else { "" };
                        format!("Subtotal ({} item{})", props.item_count, suffix)
                    }
                }
                span { "⚡{subtotal_formatted} sats" }
            }

            // Shipping
            if props.shipping_sats > 0 {
                div { class: "flex justify-between text-sm",
                    span { class: "text-muted-foreground", "Shipping" }
                    span { "⚡{shipping_formatted} sats" }
                }
            }

            // Divider
            div { class: "border-t border-border my-2" }

            // Total
            div { class: "flex justify-between font-bold text-lg",
                span { "Total" }
                span { class: "text-amber-500", "⚡{grand_total_formatted} sats" }
            }
        }
    }
}
