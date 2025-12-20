//! P2P Order Detail Page
//!
//! Full details view for a NIP-69 P2P order

use dioxus::prelude::*;
use nostr::Timestamp;
use crate::routes::Route;
use crate::stores::{nostr_client, p2p_store};
use crate::utils::nip69::{P2POrder, FiatAmount};
use crate::components::{ClientInitializing, P2PStatusBadge, P2PTypeBadge, P2PLayerBadge, P2PNetworkBadge};
use crate::utils::time::format_relative_time;
use crate::utils::format::format_sats_with_separator;

#[component]
pub fn P2POrderDetail(naddr: String) -> Element {
    let navigator = navigator();

    // State
    let mut order = use_signal(|| None::<P2POrder>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Load order - wait for client initialization
    use_effect(move || {
        let naddr = naddr.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            log::debug!("Waiting for client initialization before loading P2P order...");
            return;
        }

        spawn(async move {
            loading.set(true);
            error.set(None);

            // Check cache first
            if let Some(cached) = p2p_store::get_cached_order(&naddr) {
                order.set(Some(cached));
                loading.set(false);
                return;
            }

            // Fetch from relays
            match p2p_store::fetch_order_by_naddr(&naddr).await {
                Ok(Some(fetched)) => {
                    order.set(Some(fetched));
                }
                Ok(None) => {
                    error.set(Some("Order not found".to_string()));
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }

            loading.set(false);
        });
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header with back button
            div {
                class: "sticky top-0 z-20 bg-background/95 backdrop-blur border-b border-border p-4",
                div {
                    class: "flex items-center gap-4",
                    button {
                        class: "p-2 hover:bg-accent rounded-lg transition",
                        onclick: move |_| {
                            navigator.go_back();
                        },
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { d: "M19 12H5M12 19l-7-7 7-7" }
                        }
                    }
                    h1 {
                        class: "text-xl font-bold",
                        "Order Details"
                    }
                }
            }

            // Content
            if !*nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else if *loading.read() {
                // Loading skeleton
                div {
                    class: "p-4 space-y-4 animate-pulse",
                    div { class: "h-8 w-32 bg-muted rounded" }
                    div { class: "h-12 w-48 bg-muted rounded" }
                    div { class: "h-6 w-64 bg-muted rounded" }
                    div { class: "h-20 w-full bg-muted rounded" }
                }
            } else if let Some(err) = error.read().as_ref() {
                // Error state
                div {
                    class: "p-8 text-center",
                    p {
                        class: "text-red-500 mb-4",
                        "{err}"
                    }
                    Link {
                        to: Route::P2PHome {},
                        class: "text-primary hover:underline",
                        "Back to P2P Trading"
                    }
                }
            } else if let Some(ord) = order.read().as_ref() {
                OrderDetailContent { order: ord.clone() }
            }
        }
    }
}

#[component]
fn OrderDetailContent(order: P2POrder) -> Element {
    // Format fiat amount
    let amount_display = match &order.fiat_amount {
        FiatAmount::Fixed(amt) => format!("{:.2} {}", amt, order.currency),
        FiatAmount::Range { min, max } => format!("{:.2} - {:.2} {}", min, max, order.currency),
    };

    // Format premium
    let premium_display = order.premium.map(|p| {
        if p >= 0.0 {
            format!("+{:.2}%", p)
        } else {
            format!("{:.2}%", p)
        }
    });

    // Sats display
    let sats_display = if order.amount_sats > 0 {
        format!("{} sats", format_sats_with_separator(order.amount_sats))
    } else {
        "Market rate".to_string()
    };

    rsx! {
        div {
            class: "p-4 space-y-6",

            // Order type and status
            div {
                class: "flex items-center justify-between",
                P2PTypeBadge { order_type: order.order_type }
                P2PStatusBadge { status: order.status }
            }

            // Amount section
            div {
                class: "bg-card border border-border rounded-lg p-4",
                h3 {
                    class: "text-sm font-medium text-muted-foreground mb-2",
                    "Amount"
                }
                div {
                    class: "text-3xl font-bold mb-2",
                    "{amount_display}"
                }
                div {
                    class: "flex items-center gap-4 text-sm text-muted-foreground",
                    span {
                        class: "flex items-center gap-1",
                        "⚡ {sats_display}"
                    }
                    if let Some(premium) = &premium_display {
                        span {
                            class: if order.premium.unwrap_or(0.0) >= 0.0 { "text-green-500" } else { "text-red-500" },
                            "{premium} premium"
                        }
                    }
                }
            }

            // Payment methods
            div {
                class: "bg-card border border-border rounded-lg p-4",
                h3 {
                    class: "text-sm font-medium text-muted-foreground mb-3",
                    "Payment Methods"
                }
                div {
                    class: "flex flex-wrap gap-2",
                    for method in order.payment_methods.iter() {
                        span {
                            class: "px-3 py-1.5 bg-muted rounded-lg text-sm",
                            "{method}"
                        }
                    }
                }
            }

            // Network details
            div {
                class: "bg-card border border-border rounded-lg p-4",
                h3 {
                    class: "text-sm font-medium text-muted-foreground mb-3",
                    "Network"
                }
                div {
                    class: "flex items-center gap-3",
                    P2PLayerBadge { layer: order.layer }
                    P2PNetworkBadge { network: order.network }
                }
            }

            // Bond info (if available)
            if let Some(bond) = order.bond {
                div {
                    class: "bg-card border border-border rounded-lg p-4",
                    h3 {
                        class: "text-sm font-medium text-muted-foreground mb-2",
                        "Bond Required"
                    }
                    p {
                        class: "text-lg font-medium",
                        "{bond}%"
                    }
                }
            }

            // Maker info
            div {
                class: "bg-card border border-border rounded-lg p-4",
                h3 {
                    class: "text-sm font-medium text-muted-foreground mb-3",
                    "Maker"
                }
                if let Some(name) = &order.maker_name {
                    p {
                        class: "font-medium mb-2",
                        "{name}"
                    }
                }

                // Rating
                if let Some(rating) = &order.rating {
                    div {
                        class: "flex items-center gap-2 text-sm",
                        span { "⭐ {rating.average():.1}" }
                        span {
                            class: "text-muted-foreground",
                            "({rating.total_reviews} reviews)"
                        }
                    }
                }

                // Profile link
                Link {
                    to: Route::Profile { pubkey: order.pubkey.clone() },
                    class: "text-sm text-primary hover:underline mt-2 inline-block",
                    "View Profile"
                }
            }

            // Platform and source
            div {
                class: "bg-card border border-border rounded-lg p-4",
                h3 {
                    class: "text-sm font-medium text-muted-foreground mb-3",
                    "Platform"
                }
                if let Some(platform) = &order.platform {
                    p {
                        class: "font-medium mb-2",
                        "{platform}"
                    }
                }
                if let Some(source) = &order.source {
                    a {
                        href: "{source}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        class: "text-sm text-primary hover:underline flex items-center gap-1",
                        "View on platform"
                        svg {
                            class: "w-4 h-4",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { d: "M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" }
                        }
                    }
                }
            }

            // Timestamps
            div {
                class: "text-sm text-muted-foreground space-y-1",
                p { "Created: {format_relative_time(Timestamp::from(order.created_at))}" }
                if let Some(_expires_at) = order.expires_at {
                    if let Some(remaining) = order.time_remaining() {
                        p {
                            class: if remaining < 3600 { "text-red-500" } else { "" },
                            "Expires in: {format_duration(remaining)}"
                        }
                    } else {
                        p { class: "text-red-500", "Expired" }
                    }
                }
            }

        }
    }
}

/// Format duration in seconds
fn format_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{} seconds", seconds)
    } else if seconds < 3600 {
        format!("{} minutes", seconds / 60)
    } else if seconds < 86400 {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
        if mins > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{} hours", hours)
        }
    } else {
        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        if hours > 0 {
            format!("{}d {}h", days, hours)
        } else {
            format!("{} days", days)
        }
    }
}
