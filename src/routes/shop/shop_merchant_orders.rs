//! Shop Merchant Orders - Seller's incoming orders
use crate::components::shop::OrderStatusBadge;
use crate::routes::Route;
use crate::stores::shop_store::{
    fetch_seller_orders, listen_for_order_updates, update_order_status,
};
use crate::utils::format::truncate_id;
use crate::utils::nip99::{
    extract_product_name_from_coordinate, OrderStatus, ShippingStatus, ShopOrder,
};
use dioxus::prelude::*;
/// Merchant orders page
#[component]
pub fn ShopMerchantOrders() -> Element {
    let mut orders = use_signal(Vec::<ShopOrder>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut filter = use_signal(|| OrderFilter::All);
    let mut selected_order = use_signal(|| None::<ShopOrder>);
    let mut updating = use_signal(|| false);
    let mut tracking_number = use_signal(String::new);
    let mut carrier = use_signal(String::new);
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            error.set(None);
            if let Err(e) = listen_for_order_updates().await {
                log::warn!("Failed to fetch order updates: {}", e);
            }
            match fetch_seller_orders().await {
                Ok(o) => orders.set(o),
                Err(e) => {
                    log::error!("Failed to fetch seller orders: {}", e);
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });
    let filtered_orders = {
        let all = orders.read();
        let f = *filter.read();
        all.iter()
            .filter(|o| match f {
                OrderFilter::All => true,
                OrderFilter::Pending => o.status == OrderStatus::Pending,
                OrderFilter::Confirmed => o.status == OrderStatus::Confirmed,
                OrderFilter::Processing => o.status == OrderStatus::Processing,
                OrderFilter::Completed => o.status == OrderStatus::Completed,
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    let pending_count = orders
        .read()
        .iter()
        .filter(|o| o.status == OrderStatus::Pending)
        .count();
    rsx! {
        div { class: "min-h-screen",
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
                    h1 { class: "text-xl font-bold flex-1", "My Shop" }
                    button {
                        class: "p-2 hover:bg-accent rounded-full transition",
                        disabled: *loading.read(),
                        onclick: move |_| {
                            spawn(async move {
                                loading.set(true);
                                let _ = listen_for_order_updates().await;
                                if let Ok(o) = fetch_seller_orders().await {
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
                }
                div { class: "flex border-b border-border",
                    Link {
                        to: Route::ShopMerchant {},
                        class: "flex-1 py-3 text-center text-muted-foreground hover:text-foreground transition",
                        "Products"
                    }
                    Link {
                        to: Route::ShopMerchantOrders {},
                        class: "flex-1 py-3 text-center font-medium border-b-2 border-blue-500 text-blue-500 relative",
                        "Orders"
                        if pending_count > 0 {
                            span { class: "absolute top-2 ml-1 bg-red-500 text-white text-xs rounded-full px-1.5",
                                "{pending_count}"
                            }
                        }
                    }
                }
                div { class: "flex overflow-x-auto gap-2 px-4 py-3",
                    for f in [
                        OrderFilter::All,
                        OrderFilter::Pending,
                        OrderFilter::Confirmed,
                        OrderFilter::Processing,
                        OrderFilter::Completed,
                    ]
                    {
                        button {
                            key: "{f.label()}",
                            class: if *filter.read() == f { "px-3 py-1 text-sm bg-blue-500 text-white rounded-full whitespace-nowrap" } else { "px-3 py-1 text-sm bg-muted hover:bg-accent rounded-full whitespace-nowrap transition" },
                            onclick: move |_| filter.set(f),
                            "{f.label()}"
                        }
                    }
                }
            }
            div { class: "p-4",
                if *loading.read() {
                    div { class: "space-y-4",
                        for i in 0..4 {
                            div {
                                key: "{i}",
                                class: "h-32 bg-muted rounded-lg animate-pulse",
                            }
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
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
                                    match fetch_seller_orders().await {
                                        Ok(o) => orders.set(o),
                                        Err(e) => error.set(Some(e)),
                                    }
                                    loading.set(false);
                                });
                            },
                            "Try Again"
                        }
                    }
                } else if filtered_orders.is_empty() {
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "📦" }
                        h2 { class: "text-xl font-semibold mb-2",
                            if *filter.read() == OrderFilter::All {
                                "No Incoming Orders"
                            } else {
                                "No Orders"
                            }
                        }
                        p { class: "text-muted-foreground",
                            if *filter.read() == OrderFilter::All {
                                "Orders from buyers will appear here"
                            } else {
                                "No orders match this filter"
                            }
                        }
                    }
                } else {
                    div { class: "space-y-4",
                        for order in filtered_orders.iter() {
                            div {
                                key: "{order.order_id}",
                                class: "bg-card border border-border rounded-lg p-4 cursor-pointer hover:border-ring transition",
                                onclick: {
                                    let order = order.clone();
                                    move |_| {
                                        tracking_number.set(order.tracking_number.clone().unwrap_or_default());
                                        carrier.set(order.carrier.clone().unwrap_or_default());
                                        selected_order.set(Some(order.clone()));
                                    }
                                },
                                div { class: "flex items-start justify-between mb-3",
                                    {
                                        let order_id_short = if order.order_id.len() > 8 {
                                            format!("{}...", truncate_id(&order.order_id, 8))
                                        } else {
                                            order.order_id.clone()
                                        };
                                        let created_date = chrono::DateTime::from_timestamp(order.created_at as i64, 0)
                                            .map(|d| d.format("%b %d, %H:%M").to_string())
                                            .unwrap_or_else(|| "Unknown".to_string());
                                        rsx! {
                                            div {
                                                p { class: "font-medium text-sm font-mono", "#{order_id_short}" }
                                                p { class: "text-xs text-muted-foreground", "{created_date}" }
                                            }
                                        }
                                    }
                                    OrderStatusBadge { status: order.status }
                                }
                                div { class: "text-sm text-muted-foreground mb-2",
                                    "{order.items.len()} item(s)"
                                }
                                div { class: "flex items-center justify-between",
                                    span { class: "text-amber-500 font-medium",
                                        "⚡{order.amount_sats} sats"
                                    }
                                    {
                                        let buyer_short = if order.buyer_pubkey.len() > 8 {
                                            format!("{}...", truncate_id(&order.buyer_pubkey, 8))
                                        } else {
                                            order.buyer_pubkey.clone()
                                        };
                                        rsx! {
                                            span { class: "text-xs text-muted-foreground", "from {buyer_short}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(order) = selected_order.read().as_ref() {
                div {
                    class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
                    onclick: move |_| selected_order.set(None),
                    div {
                        class: "bg-background rounded-lg max-w-lg w-full max-h-[90vh] overflow-y-auto",
                        onclick: move |e| e.stop_propagation(),
                        div { class: "sticky top-0 bg-background border-b border-border p-4 flex items-center justify-between",
                            h2 { class: "text-lg font-semibold", "Order Details" }
                            button {
                                class: "p-2 hover:bg-accent rounded-full transition",
                                onclick: move |_| selected_order.set(None),
                                crate::components::icons::XIcon { class: "w-5 h-5" }
                            }
                        }
                        div { class: "p-4 space-y-4",
                            div { class: "space-y-2",
                                div { class: "flex items-center justify-between",
                                    span { class: "text-sm text-muted-foreground", "Order ID" }
                                    span { class: "font-mono text-sm", "{order.order_id}" }
                                }
                                div { class: "flex items-center justify-between",
                                    span { class: "text-sm text-muted-foreground", "Status" }
                                    OrderStatusBadge { status: order.status }
                                }
                                div { class: "flex items-center justify-between",
                                    span { class: "text-sm text-muted-foreground", "Total" }
                                    span { class: "text-amber-500 font-medium",
                                        "⚡{order.amount_sats} sats"
                                    }
                                }
                            }
                            div { class: "border-t border-border pt-4",
                                h3 { class: "font-medium mb-2", "Items" }
                                div { class: "space-y-2",
                                    for item in order.items.iter() {
                                        {
                                            let product_name = extract_product_name_from_coordinate(
                                                &item.product_coordinate,
                                            );
                                            rsx! {
                                                div { class: "flex items-center justify-between text-sm",
                                                    span { "{product_name} x{item.quantity}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "border-t border-border pt-4",
                                h3 { class: "font-medium mb-2", "Buyer" }
                                p { class: "text-sm font-mono text-muted-foreground",
                                    "{order.buyer_pubkey}"
                                }
                            }
                            if let Some(addr) = &order.shipping_address {
                                div { class: "border-t border-border pt-4",
                                    h3 { class: "font-medium mb-2", "Shipping Address" }
                                    p { class: "text-sm whitespace-pre-wrap", "{addr}" }
                                }
                            }
                            div { class: "border-t border-border pt-4 space-y-4",
                                h3 { class: "font-medium", "Update Status" }
                                div { class: "flex flex-wrap gap-2",
                                    if order.status == OrderStatus::Pending {
                                        button {
                                            class: "px-4 py-2 bg-green-500 hover:bg-green-600 text-white rounded-lg text-sm font-medium transition disabled:opacity-50",
                                            disabled: *updating.read(),
                                            onclick: {
                                                let order_id = order.order_id.clone();
                                                move |_| {
                                                    updating.set(true);
                                                    let oid = order_id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = update_order_status(
                                                                &oid,
                                                                OrderStatus::Confirmed,
                                                                None,
                                                                None,
                                                                None,
                                                            )
                                                            .await
                                                        {
                                                            log::error!("Failed to update order: {}", e);
                                                        }
                                                        if let Ok(o) = fetch_seller_orders().await {
                                                            orders.set(o);
                                                        }
                                                        updating.set(false);
                                                        selected_order.set(None);
                                                    });
                                                }
                                            },
                                            "Confirm Order"
                                        }
                                    }
                                    if order.status == OrderStatus::Confirmed {
                                        button {
                                            class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg text-sm font-medium transition disabled:opacity-50",
                                            disabled: *updating.read(),
                                            onclick: {
                                                let order_id = order.order_id.clone();
                                                move |_| {
                                                    updating.set(true);
                                                    let oid = order_id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = update_order_status(
                                                                &oid,
                                                                OrderStatus::Processing,
                                                                None,
                                                                None,
                                                                None,
                                                            )
                                                            .await
                                                        {
                                                            log::error!("Failed to update order: {}", e);
                                                        }
                                                        if let Ok(o) = fetch_seller_orders().await {
                                                            orders.set(o);
                                                        }
                                                        updating.set(false);
                                                        selected_order.set(None);
                                                    });
                                                }
                                            },
                                            "Start Processing"
                                        }
                                    }
                                    if order.status == OrderStatus::Processing {
                                        div { class: "w-full space-y-3",
                                            div { class: "grid grid-cols-2 gap-2",
                                                input {
                                                    r#type: "text",
                                                    class: "px-3 py-2 bg-muted rounded-lg text-sm",
                                                    placeholder: "Tracking number",
                                                    value: "{tracking_number}",
                                                    oninput: move |e| tracking_number.set(e.value()),
                                                }
                                                input {
                                                    r#type: "text",
                                                    class: "px-3 py-2 bg-muted rounded-lg text-sm",
                                                    placeholder: "Carrier (USPS, FedEx...)",
                                                    value: "{carrier}",
                                                    oninput: move |e| carrier.set(e.value()),
                                                }
                                            }
                                            button {
                                                class: "w-full py-2 bg-purple-500 hover:bg-purple-600 text-white rounded-lg text-sm font-medium transition disabled:opacity-50",
                                                disabled: *updating.read(),
                                                onclick: {
                                                    let order_id = order.order_id.clone();
                                                    move |_| {
                                                        updating.set(true);
                                                        let oid = order_id.clone();
                                                        let track = if tracking_number.read().is_empty() {
                                                            None
                                                        } else {
                                                            Some(tracking_number.read().clone())
                                                        };
                                                        let carr = if carrier.read().is_empty() {
                                                            None
                                                        } else {
                                                            Some(carrier.read().clone())
                                                        };
                                                        spawn(async move {
                                                            if let Err(e) = update_order_status(
                                                                    &oid,
                                                                    OrderStatus::Processing,
                                                                    Some(ShippingStatus::Shipped),
                                                                    track,
                                                                    carr,
                                                                )
                                                                .await
                                                            {
                                                                log::error!("Failed to update order: {}", e);
                                                            }
                                                            if let Ok(o) = fetch_seller_orders().await {
                                                                orders.set(o);
                                                            }
                                                            updating.set(false);
                                                            selected_order.set(None);
                                                        });
                                                    }
                                                },
                                                "Mark as Shipped"
                                            }
                                        }
                                    }
                                    {
                                        let has_shipping = order.shipping_address.is_some();
                                        let is_shipped = order.shipping_status == Some(ShippingStatus::Shipped);
                                        let can_complete = order.status == OrderStatus::Processing
                                            && (!has_shipping || is_shipped);
                                        rsx! {
                                            if can_complete {
                                                button {
                                                    class: "px-4 py-2 bg-green-500 hover:bg-green-600 text-white rounded-lg text-sm font-medium transition disabled:opacity-50",
                                                    disabled: *updating.read(),
                                                    onclick: {
                                                        let order_id = order.order_id.clone();
                                                        move |_| {
                                                            updating.set(true);
                                                            let oid = order_id.clone();
                                                            spawn(async move {
                                                                if let Err(e) = update_order_status(
                                                                        &oid,
                                                                        OrderStatus::Completed,
                                                                        Some(ShippingStatus::Delivered),
                                                                        None,
                                                                        None,
                                                                    )
                                                                    .await
                                                                {
                                                                    log::error!("Failed to update order: {}", e);
                                                                }
                                                                if let Ok(o) = fetch_seller_orders().await {
                                                                    orders.set(o);
                                                                }
                                                                updating.set(false);
                                                                selected_order.set(None);
                                                            });
                                                        }
                                                    },
                                                    if has_shipping {
                                                        "Mark Delivered"
                                                    } else {
                                                        "Complete Order"
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
            }
        }
    }
}
/// Order filter options
#[derive(Clone, Copy, PartialEq)]
enum OrderFilter {
    All,
    Pending,
    Confirmed,
    Processing,
    Completed,
}
impl OrderFilter {
    fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Pending => "Pending",
            Self::Confirmed => "Confirmed",
            Self::Processing => "Processing",
            Self::Completed => "Completed",
        }
    }
}
