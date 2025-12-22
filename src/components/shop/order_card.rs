//! OrderCard component - displays an order in the orders list

use dioxus::prelude::*;
use crate::utils::nip99::ShopOrder;
use crate::utils::format::format_sats_with_separator;
use super::OrderStatusBadge;

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

    // Truncate order ID for display
    let order_id_short = if order.order_id.len() > 8 {
        order.order_id[..8].to_string()
    } else {
        order.order_id.clone()
    };

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

            // Counterparty (truncated pubkey)
            {
                let counterparty = if props.is_buyer {
                    format!("Seller: {}...", &order.merchant_pubkey[..8.min(order.merchant_pubkey.len())])
                } else {
                    format!("Buyer: {}...", &order.buyer_pubkey[..8.min(order.buyer_pubkey.len())])
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
