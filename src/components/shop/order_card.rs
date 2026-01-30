//! OrderCard component - displays an order in the orders list

use super::OrderStatusBadge;
use crate::utils::format::{format_sats_with_separator, truncate_id};
use crate::utils::nip99::ShopOrder;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct OrderCardProps {
    pub order: ShopOrder,
    /// Whether this is shown from buyer or seller perspective
    #[props(default = true)]
    pub is_buyer: bool,
}

/// Order card for order lists
#[component]
pub fn OrderCard(props: OrderCardProps) -> Element {
    let order = &props.order;
    let total_formatted = format_sats_with_separator(order.amount_sats);
    let item_count = order.items.len();

    // Truncate order ID for display (safe UTF-8 handling)
    let order_id_short = truncate_id(&order.order_id, 8);

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 hover:border-ring transition",
            // Header
            div { class: "flex items-start justify-between mb-3",
                div {
                    div { class: "flex items-center gap-2",
                        span { class: "font-mono text-sm text-muted-foreground",
                            "Order #{order_id_short}"
                        }
                        OrderStatusBadge { status: order.status }
                    }
                }
                div { class: "text-right",
                    div { class: "font-semibold text-amber-500",
                        "⚡{total_formatted} sats"
                    }
                    p { class: "text-xs text-muted-foreground",
                        {
                            let suffix = if item_count != 1 { "s" } else { "" };
                            format!("{} item{}", item_count, suffix)
                        }
                    }
                }
            }

            // Items preview placeholder (we don't have product details in OrderItem)
            div { class: "flex gap-2 mb-3",
                for (i, _item) in order.items.iter().take(3).enumerate() {
                    div {
                        key: "{i}",
                        class: "w-12 h-12 bg-muted rounded flex items-center justify-center text-lg",
                        "📦"
                    }
                }
                if item_count > 3 {
                    div { class: "w-12 h-12 bg-muted rounded flex items-center justify-center text-sm text-muted-foreground",
                        "+{item_count - 3}"
                    }
                }
            }

            // Counterparty (truncated pubkey - safe UTF-8 handling)
            {
                let counterparty = if props.is_buyer {
                    format!("Seller: {}...", truncate_id(&order.merchant_pubkey, 8))
                } else {
                    format!("Buyer: {}...", truncate_id(&order.buyer_pubkey, 8))
                };
                rsx! {
                    p { class: "text-sm text-muted-foreground",
                        "{counterparty}"
                    }
                }
            }

            // Shipping status if available
            if let Some(shipping) = &order.shipping_status {
                div { class: "mt-2 text-xs text-muted-foreground",
                    "Shipping: {shipping}"
                }
            }
        }
    }
}
