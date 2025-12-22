//! Shop Orders - Buyer's order history

use dioxus::prelude::*;
use crate::routes::Route;
use crate::utils::nip99::{ShopOrder, OrderStatus, ShippingStatus};
use crate::stores::shop_store::{fetch_my_orders, listen_for_order_updates};
use crate::components::shop::{OrderStatusBadge, ReviewForm};

/// Buyer orders page
#[component]
pub fn ShopOrders() -> Element {
    let mut orders = use_signal(Vec::<ShopOrder>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut selected_order = use_signal(|| None::<ShopOrder>);

    // Fetch orders and listen for updates on mount
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            error.set(None);

            // First, listen for any new order updates via NIP-17
            if let Err(e) = listen_for_order_updates().await {
                log::warn!("Failed to fetch order updates: {}", e);
            }

            // Then fetch all orders
            match fetch_my_orders().await {
                Ok(o) => orders.set(o),
                Err(e) => {
                    log::error!("Failed to fetch my orders: {}", e);
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    // Sort orders by date (newest first)
    let sorted_orders = {
        let mut o = orders.read().clone();
        o.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        o
    };

    rsx! {
        div { class: "min-h-screen",
            // Header
            div { class: "sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "flex items-center gap-4 p-4",
                    button {
                        class: "p-2 hover:bg-accent rounded-full transition",
                        onclick: move |_| {
                            let nav = navigator();
                            nav.go_back();
                        },
                        crate::components::icons::ArrowLeftIcon { class: "w-5 h-5" }
                    }
                    h1 { class: "text-xl font-bold flex-1", "My Orders" }

                    // Refresh button
                    button {
                        class: "p-2 hover:bg-accent rounded-full transition",
                        disabled: *loading.read(),
                        onclick: move |_| {
                            spawn(async move {
                                loading.set(true);
                                // Listen for updates first
                                let _ = listen_for_order_updates().await;
                                // Then refresh orders
                                if let Ok(o) = fetch_my_orders().await {
                                    orders.set(o);
                                }
                                loading.set(false);
                            });
                        },
                        if *loading.read() {
                            div { class: "w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                        } else {
                            crate::components::icons::RefreshIcon { class: "w-5 h-5" }
                        }
                    }

                    // Cart link
                    Link {
                        to: Route::ShopCart {},
                        class: "p-2 hover:bg-accent rounded-full transition",
                        crate::components::icons::ShoppingCartIcon { class: "w-5 h-5" }
                    }
                }
            }

            // Orders list
            div { class: "p-4",
                if *loading.read() {
                    // Loading skeleton
                    div { class: "space-y-4",
                        for i in 0..4 {
                            div { key: "{i}", class: "h-32 bg-muted rounded-lg animate-pulse" }
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    // Error state
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "😢" }
                        h2 { class: "text-xl font-semibold mb-2", "Failed to load orders" }
                        p { class: "text-muted-foreground mb-4", "{err}" }
                        button {
                            class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition",
                            onclick: move |_| {
                                spawn(async move {
                                    loading.set(true);
                                    error.set(None);
                                    match fetch_my_orders().await {
                                        Ok(o) => orders.set(o),
                                        Err(e) => error.set(Some(e)),
                                    }
                                    loading.set(false);
                                });
                            },
                            "Try Again"
                        }
                    }
                } else if sorted_orders.is_empty() {
                    // Empty state
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "📋" }
                        h2 { class: "text-xl font-semibold mb-2", "No Orders Yet" }
                        p { class: "text-muted-foreground mb-4",
                            "Your purchase history will appear here"
                        }
                        Link {
                            to: Route::ShopHome {},
                            class: "inline-block px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-full transition font-medium",
                            "Browse Products"
                        }
                    }
                } else {
                    // Orders list
                    div { class: "space-y-4",
                        for order in sorted_orders.iter() {
                            div {
                                key: "{order.order_id}",
                                class: "bg-card border border-border rounded-lg p-4 cursor-pointer hover:border-ring transition",
                                onclick: {
                                    let order = order.clone();
                                    move |_| selected_order.set(Some(order.clone()))
                                },

                                // Order header
                                div { class: "flex items-start justify-between mb-3",
                                    {
                                        let order_id_short = if order.order_id.len() > 8 {
                                            format!("{}...", &order.order_id[..8])
                                        } else {
                                            order.order_id.clone()
                                        };
                                        let created_date = chrono::DateTime::from_timestamp(order.created_at as i64, 0)
                                            .map(|d| d.format("%b %d, %Y at %H:%M").to_string())
                                            .unwrap_or_else(|| "Unknown".to_string());
                                        rsx! {
                                            div {
                                                p { class: "font-medium text-sm font-mono",
                                                    "Order #{order_id_short}"
                                                }
                                                p { class: "text-xs text-muted-foreground",
                                                    "{created_date}"
                                                }
                                            }
                                        }
                                    }
                                    OrderStatusBadge { status: order.status }
                                }

                                // Items preview
                                div { class: "space-y-1 mb-3",
                                    for (i, item) in order.items.iter().take(2).enumerate() {
                                        {
                                            // Parse product coordinate to get d-tag as product name
                                            // Note: This heuristic expects NIP-99 format "30402:pubkey:d-tag"
                                            // Edge cases with malformed coordinates will fallback to full string
                                            let parts: Vec<&str> = item.product_coordinate.split(':').collect();
                                            let product_name = if parts.len() >= 3 {
                                                parts[2].to_string()
                                            } else {
                                                item.product_coordinate.clone()
                                            };
                                            rsx! {
                                                p {
                                                    key: "{i}",
                                                    class: "text-sm text-muted-foreground truncate",
                                                    "{product_name} x{item.quantity}"
                                                }
                                            }
                                        }
                                    }
                                    {
                                        let remaining = order.items.len().saturating_sub(2);
                                        if remaining > 0 {
                                            rsx! {
                                                p { class: "text-sm text-muted-foreground",
                                                    "...and {remaining} more item(s)"
                                                }
                                            }
                                        } else {
                                            rsx! {}
                                        }
                                    }
                                }

                                // Total and shipping status
                                div { class: "flex items-center justify-between",
                                    span { class: "text-amber-500 font-medium",
                                        "⚡{order.amount_sats} sats"
                                    }
                                    if let Some(ref ship_status) = order.shipping_status {
                                        span { class: "text-xs px-2 py-1 bg-muted rounded-full",
                                            match ship_status {
                                                ShippingStatus::Processing => "Preparing",
                                                ShippingStatus::Shipped => "Shipped",
                                                ShippingStatus::Delivered => "Delivered",
                                                ShippingStatus::Exception => "Issue",
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Order detail modal
            if let Some(order) = selected_order.read().as_ref() {
                div { class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
                    onclick: move |_| selected_order.set(None),

                    div {
                        class: "bg-background rounded-lg max-w-lg w-full max-h-[90vh] overflow-y-auto",
                        onclick: move |e| e.stop_propagation(),

                        // Modal header
                        div { class: "sticky top-0 bg-background border-b border-border p-4 flex items-center justify-between",
                            h2 { class: "text-lg font-semibold", "Order Details" }
                            button {
                                class: "p-2 hover:bg-accent rounded-full transition",
                                onclick: move |_| selected_order.set(None),
                                crate::components::icons::XIcon { class: "w-5 h-5" }
                            }
                        }

                        // Modal content
                        div { class: "p-4 space-y-4",
                            // Order info
                            div { class: "space-y-2",
                                div { class: "flex items-center justify-between",
                                    span { class: "text-sm text-muted-foreground", "Order ID" }
                                    span { class: "font-mono text-sm", "{order.order_id}" }
                                }
                                div { class: "flex items-center justify-between",
                                    span { class: "text-sm text-muted-foreground", "Date" }
                                    span { class: "text-sm",
                                        {
                                            let secs = order.created_at as i64;
                                            chrono::DateTime::from_timestamp(secs, 0)
                                                .map(|d| d.format("%b %d, %Y at %H:%M").to_string())
                                                .unwrap_or_else(|| "Unknown".to_string())
                                        }
                                    }
                                }
                                div { class: "flex items-center justify-between",
                                    span { class: "text-sm text-muted-foreground", "Status" }
                                    OrderStatusBadge { status: order.status }
                                }
                                div { class: "flex items-center justify-between",
                                    span { class: "text-sm text-muted-foreground", "Total" }
                                    span { class: "text-amber-500 font-medium", "⚡{order.amount_sats} sats" }
                                }
                            }

                            // Items
                            div { class: "border-t border-border pt-4",
                                h3 { class: "font-medium mb-3", "Items" }
                                div { class: "space-y-3",
                                    for item in order.items.iter() {
                                        {
                                            // Parse product coordinate to get d-tag as product name
                                            let parts: Vec<&str> = item.product_coordinate.split(':').collect();
                                            let product_name = if parts.len() >= 3 {
                                                parts[2].to_string()
                                            } else {
                                                item.product_coordinate.clone()
                                            };
                                            rsx! {
                                                div { class: "flex items-center gap-3",
                                                    // Product image placeholder
                                                    div { class: "w-12 h-12 bg-muted rounded-lg flex items-center justify-center text-lg",
                                                        "📦"
                                                    }
                                                    div { class: "flex-1",
                                                        p { class: "font-medium text-sm", "{product_name}" }
                                                        p { class: "text-xs text-muted-foreground",
                                                            "Qty: {item.quantity}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Shipping info
                            if order.shipping_status.is_some() || order.tracking_number.is_some() {
                                div { class: "border-t border-border pt-4",
                                    h3 { class: "font-medium mb-3", "Shipping" }
                                    div { class: "space-y-2",
                                        if let Some(ref status) = order.shipping_status {
                                            div { class: "flex items-center justify-between",
                                                span { class: "text-sm text-muted-foreground", "Status" }
                                                span { class: "text-sm",
                                                    match status {
                                                        ShippingStatus::Processing => "Processing",
                                                        ShippingStatus::Shipped => "Shipped",
                                                        ShippingStatus::Delivered => "Delivered",
                                                        ShippingStatus::Exception => "Exception",
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(ref tracking) = order.tracking_number {
                                            div { class: "flex items-center justify-between",
                                                span { class: "text-sm text-muted-foreground", "Tracking" }
                                                span { class: "text-sm font-mono", "{tracking}" }
                                            }
                                        }
                                        if let Some(ref carrier) = order.carrier {
                                            div { class: "flex items-center justify-between",
                                                span { class: "text-sm text-muted-foreground", "Carrier" }
                                                span { class: "text-sm", "{carrier}" }
                                            }
                                        }
                                    }
                                }
                            }

                            // Digital delivery info
                            if order.download_url.is_some() || order.license_key.is_some() {
                                div { class: "border-t border-border pt-4",
                                    h3 { class: "font-medium mb-3 flex items-center gap-2",
                                        span { class: "text-lg", "📦" }
                                        "Digital Delivery"
                                    }
                                    div { class: "space-y-3",
                                        // Download URL
                                        if let Some(ref url) = order.download_url {
                                            div { class: "bg-muted/50 rounded-lg p-3",
                                                p { class: "text-xs text-muted-foreground mb-1", "Download Link" }
                                                a {
                                                    href: "{url}",
                                                    target: "_blank",
                                                    rel: "noopener noreferrer",
                                                    class: "text-blue-500 hover:underline text-sm break-all",
                                                    "{url}"
                                                }
                                            }
                                        }

                                        // License key
                                        if let Some(ref key) = order.license_key {
                                            div { class: "bg-muted/50 rounded-lg p-3",
                                                p { class: "text-xs text-muted-foreground mb-1", "License Key" }
                                                div { class: "flex items-center gap-2",
                                                    code { class: "text-sm font-mono bg-background px-2 py-1 rounded border border-border flex-1 break-all",
                                                        "{key}"
                                                    }
                                                    button {
                                                        class: "p-2 hover:bg-accent rounded transition flex-shrink-0",
                                                        title: "Copy license key",
                                                        onclick: {
                                                            let key_clone = key.clone();
                                                            move |_| {
                                                                let key_to_copy = key_clone.clone();
                                                                spawn(async move {
                                                                    let _ = crate::utils::clipboard::copy_to_clipboard(&key_to_copy).await;
                                                                });
                                                            }
                                                        },
                                                        crate::components::icons::CopyIcon { class: "w-4 h-4" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Merchant info
                            div { class: "border-t border-border pt-4",
                                h3 { class: "font-medium mb-2", "Seller" }
                                p { class: "text-sm font-mono text-muted-foreground",
                                    "{order.merchant_pubkey}"
                                }
                            }

                            // Payment info
                            if let Some(paid_at) = order.paid_at {
                                div { class: "border-t border-border pt-4",
                                    h3 { class: "font-medium mb-2", "Payment" }
                                    div { class: "flex items-center gap-2",
                                        span { class: "text-green-500", "✓" }
                                        span { class: "text-sm text-muted-foreground",
                                            "Paid on "
                                            {
                                                chrono::DateTime::from_timestamp(paid_at as i64, 0)
                                                    .map(|d| d.format("%b %d, %Y at %H:%M").to_string())
                                                    .unwrap_or_else(|| "Unknown".to_string())
                                            }
                                        }
                                    }
                                }
                            }

                            // Actions and Reviews for completed orders
                            if order.status == OrderStatus::Completed {
                                // Review section - show form for each item
                                div { class: "border-t border-border pt-4",
                                    h3 { class: "font-medium mb-3 flex items-center gap-2",
                                        span { class: "text-lg", "⭐" }
                                        "Leave a Review"
                                    }
                                    p { class: "text-sm text-muted-foreground mb-4",
                                        "Share your experience with these products"
                                    }
                                    div { class: "space-y-4",
                                        for item in order.items.iter() {
                                            {
                                                let coord = item.product_coordinate.clone();
                                                let parts: Vec<&str> = coord.split(':').collect();
                                                let product_name = if parts.len() >= 3 {
                                                    parts[2].to_string()
                                                } else {
                                                    coord.clone()
                                                };
                                                rsx! {
                                                    div { class: "border border-border rounded-lg p-4",
                                                        p { class: "font-medium text-sm mb-3", "{product_name}" }
                                                        ReviewForm {
                                                            product_coordinate: coord.clone()
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Order again button
                                div { class: "border-t border-border pt-4 mt-4",
                                    Link {
                                        to: Route::ShopHome {},
                                        class: "block w-full text-center py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                                        "Order Again"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
