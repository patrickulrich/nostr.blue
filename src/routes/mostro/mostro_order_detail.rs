use crate::components::mostro::TakeMostroButton;
use crate::components::{P2PLayerBadge, P2PStatusBadge, P2PTypeBadge};
use crate::routes::Route;
use crate::services::btc_price;
use crate::stores::social::p2p_store;
use crate::utils::format::format_sats_with_unit;
use crate::utils::nip69::P2POrder;
use crate::utils::time::format_relative_time;
use dioxus::prelude::*;
use nostr::nips::nip01::Coordinate;
use nostr::prelude::*;
use nostr_sdk::prelude::Kind as NostrKind;

#[component]
pub fn MostroOrderDetail(naddr: String) -> Element {
    let nav = navigator();
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut fetched_order = use_signal(|| None::<P2POrder>);

    let order = p2p_store::get_cached_order(&naddr).or_else(|| fetched_order.read().clone());

    if order.is_none() {
        let naddr_clone = naddr.clone();
        spawn(async move {
            if *loading.read() {
                return;
            }
            loading.set(true);
            let coord = match Coordinate::parse(&naddr_clone) {
                Ok(c) => c,
                Err(_) => {
                    error_msg.set(Some("Invalid order address".to_string()));
                    loading.set(false);
                    return;
                }
            };
            let pk = coord.public_key;
            let d = coord.identifier.clone();
            let filter = Filter::new()
                .kind(NostrKind::Custom(coord.kind.as_u16()))
                .author(pk)
                .identifier(&d)
                .limit(1);
            let client = match crate::stores::nostr_client::get_client() {
                Some(c) => c,
                None => {
                    error_msg.set(Some("Client not ready".to_string()));
                    loading.set(false);
                    return;
                }
            };
            match client
                .fetch_events(filter, std::time::Duration::from_secs(10))
                .await
            {
                Ok(events) => {
                    if let Some(event) = events.iter().max_by_key(|e| e.created_at) {
                        match crate::utils::nip69::parse_p2p_order(event) {
                            Ok(parsed) => {
                                fetched_order.set(Some(parsed));
                            }
                            Err(e) => {
                                error_msg.set(Some(format!("Failed to parse order: {e}")));
                            }
                        }
                    } else {
                        error_msg.set(Some("Order not found".to_string()));
                    }
                }
                Err(e) => {
                    error_msg.set(Some(format!("Failed to fetch: {e}")));
                }
            }
            loading.set(false);
        });
    }

    rsx! {
        div { class: "min-h-screen max-w-3xl mx-auto",
            div { class: "flex items-center gap-3 p-4 border-b border-border",
                button {
                    class: "p-2 hover:bg-accent rounded-lg",
                    onclick: move |_| { let _ = nav.push(Route::MostroHome {}); },
                    crate::components::icons::ArrowLeftIcon { class: "w-5 h-5".to_string() }
                }
                h1 { class: "text-xl font-bold", "Order Detail" }
            }

            if *loading.read() {
                div { class: "p-8 text-center text-muted-foreground", "Loading order..." }
            } else if let Some(err) = error_msg.read().as_ref() {
                div { class: "p-8 text-center",
                    p { class: "text-red-500 mb-4", "{err}" }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                        onclick: move |_| { let _ = nav.push(Route::MostroHome {}); },
                        "Back to Orders"
                    }
                }
            } else if let Some(order) = order.as_ref() {
                {render_order_detail(order)}
            }
        }
    }
}

fn render_order_detail(order: &P2POrder) -> Element {
    let amount_display = order.fiat_amount.display(&order.currency);
    let sats_display = if order.amount_sats > 0 {
        format_sats_with_unit(order.amount_sats)
    } else if let Some(btc_price) = btc_price::get_btc_price(&order.currency) {
        let sats = order.calc_sats_at_rate(btc_price);
        format!("~{}", format_sats_with_unit(sats))
    } else {
        "Market rate".to_string()
    };
    let premium_display = order.premium.map(|p| {
        if p >= 0.0 {
            format!("+{:.1}%", p)
        } else {
            format!("{:.1}%", p)
        }
    });
    let is_mostro = order.platform.as_deref() == Some("mostro");
    let is_pending = order.status == crate::utils::nip69::OrderStatus::Pending;

    rsx! {
        div { class: "p-4 space-y-4",
            div { class: "bg-card border border-border rounded-lg p-4 space-y-3",
                div { class: "flex items-center justify-between",
                    div { class: "flex items-center gap-2",
                        P2PTypeBadge { order_type: order.order_type }
                        span { class: "text-2xl font-bold", "{amount_display}" }
                    }
                    P2PStatusBadge { status: order.status }
                }

                div { class: "grid grid-cols-2 gap-3 text-sm",
                    div {
                        p { class: "text-muted-foreground text-xs", "Bitcoin" }
                        p { class: "font-medium", "{sats_display}" }
                    }
                    if let Some(premium) = &premium_display {
                        div {
                            p { class: "text-muted-foreground text-xs", "Premium" }
                            {
                                let cls = if order.premium.unwrap_or(0.0) >= 0.0 {
                                    "font-medium text-green-600 dark:text-green-400"
                                } else {
                                    "font-medium text-red-600 dark:text-red-400"
                                };
                                let premium = premium.clone();
                                rsx! { p { class: "{cls}", "{premium}" } }
                            }
                        }
                    }
                    div {
                        p { class: "text-muted-foreground text-xs", "Layer" }
                        P2PLayerBadge { layer: order.layer }
                    }
                    div {
                        p { class: "text-muted-foreground text-xs", "Payment" }
                        p { class: "font-medium text-xs",
                            {order.payment_methods.join(", ")}
                        }
                    }
                }

                if let Some(rating) = &order.rating {
                    if rating.total_reviews > 0 {
                        div { class: "pt-2 border-t border-border",
                            div { class: "flex items-center gap-2 text-sm",
                                span { class: "text-yellow-500", "★ {rating.average():.1}" }
                                span { class: "text-muted-foreground",
                                    "({rating.total_reviews} reviews)"
                                }
                                if rating.days > 0 {
                                    span { class: "text-muted-foreground", "{rating.days}d active" }
                                }
                            }
                        }
                    }
                }

                if let Some(maker_name) = &order.maker_name {
                    div { class: "pt-2 border-t border-border",
                        p { class: "text-muted-foreground text-xs", "Maker" }
                        p { class: "font-medium text-sm", "{maker_name}" }
                    }
                }

                if let Some(remaining) = order.time_remaining() {
                    div { class: "pt-2 border-t border-border",
                        if remaining < 3600 {
                            p { class: "text-red-500 text-sm",
                                "Expires in {crate::utils::duration::format_duration_compact(remaining)}"
                            }
                        } else {
                            p { class: "text-muted-foreground text-sm",
                                "Expires in {crate::utils::duration::format_duration_compact(remaining)}"
                            }
                        }
                    }
                }

                if let Some(bond) = order.bond {
                    div { class: "pt-2 border-t border-border",
                        p { class: "text-muted-foreground text-xs", "Bond" }
                        p { class: "text-sm", "{bond}% anti-abuse collateral" }
                    }
                }

                if let Some(source) = &order.source {
                    div { class: "pt-2 border-t border-border",
                        p { class: "text-muted-foreground text-xs", "Source" }
                        p { class: "text-xs font-mono text-muted-foreground break-all", "{source}" }
                    }
                }
            }

            if is_mostro && is_pending {
                div { class: "bg-card border border-border rounded-lg p-4",
                    p { class: "text-sm text-muted-foreground mb-3",
                        "Ready to trade? Take this order to start a P2P exchange."
                    }
                    TakeMostroButton { order: order.clone() }
                }
            } else if is_mostro && !is_pending {
                div { class: "bg-muted/50 border border-border rounded-lg p-3",
                    p { class: "text-sm text-muted-foreground",
                        "This order is no longer available."
                    }
                }
            }

            if !is_mostro {
                div { class: "bg-muted/50 border border-border rounded-lg p-3",
                    p { class: "text-sm text-muted-foreground",
                        "This order is from a different platform. Look for Mostro orders to trade directly."
                    }
                }
            }

            p { class: "text-xs text-muted-foreground text-center",
                "Created {format_relative_time(Timestamp::from(order.created_at))}"
            }
        }
    }
}
