use crate::components::mostro::TakeMostroButton;
use crate::components::{P2PLayerBadge, P2PStatusBadge, P2PTypeBadge};
use crate::routes::Route;
use crate::services::btc_price;
use crate::utils::duration::format_duration_compact;
use crate::utils::format::format_sats_with_unit;
use crate::utils::nip69::P2POrder;
use crate::utils::time::format_relative_time;
use dioxus::prelude::*;
use nostr::Timestamp;
/// P2P Order Card for list display
#[component]
pub fn P2POrderCard(order: P2POrder) -> Element {
    let amount_display = order.fiat_amount.display(&order.currency);
    let (premium_display, premium_class) = order
        .premium
        .map(|p| {
            if p >= 0.0 {
                (format!("+{:.1}%", p), "text-green-600 dark:text-green-400")
            } else {
                (format!("{:.1}%", p), "text-red-600 dark:text-red-400")
            }
        })
        .map(|(display, class)| (Some(display), class))
        .unwrap_or((None, ""));
    let sats_display = if order.amount_sats > 0 {
        format_sats_with_unit(order.amount_sats)
    } else if let Some(btc_price) = btc_price::get_btc_price(&order.currency) {
        let sats = order.calc_sats_at_rate(btc_price);
        format!("~{}", format_sats_with_unit(sats))
    } else {
        "Market rate".to_string()
    };
    let payment_methods_display: Vec<&str> = order
        .payment_methods
        .iter()
        .take(3)
        .map(|s| s.as_str())
        .collect();
    let extra_methods = order.payment_methods.len().saturating_sub(3);
    rsx! {
        Link {
            to: Route::MostroOrderDetail {
                naddr: order.naddr.clone(),
            },
            class: "block p-4 hover:bg-accent/50 transition",
            div { class: "flex items-center justify-between mb-2",
                P2PTypeBadge { order_type: order.order_type }
                span { class: "text-lg font-bold", "{amount_display}" }
            }
            div { class: "flex items-center gap-4 text-sm text-muted-foreground mb-2",
                span { class: "flex items-center gap-1",
                    svg {
                        class: "w-4 h-4 text-orange-500",
                        xmlns: "http://www.w3.org/2000/svg",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        view_box: "0 0 24 24",
                        path { d: "M11.767 19.089c4.924.868 6.14-6.025 1.216-6.894m-1.216 6.894L5.86 18.047m5.908 1.042-.347 1.97m1.563-8.864c4.924.869 6.14-6.025 1.215-6.893m-1.215 6.893-3.94-.694m5.155-6.2L8.29 4.26m5.908 1.042.348-1.97M7.48 20.364l3.126-17.727" }
                    }
                    "{sats_display}"
                }
                if let Some(premium) = premium_display {
                    span { class: "{premium_class} font-medium", "{premium}" }
                }
                if let Some(rating) = &order.rating {
                    if rating.total_reviews > 0 {
                        span { class: "text-xs text-yellow-500 flex items-center gap-0.5",
                            "★"
                            span { class: "text-yellow-500", "{rating.average():.1}" }
                            span { class: "text-muted-foreground", "({rating.total_reviews})" }
                        }
                        if rating.days > 0 {
                            span { class: "text-xs text-muted-foreground",
                                "{rating.days}d"
                            }
                        }
                    }
                }
            }
            div { class: "flex flex-wrap gap-1 mb-2",
                for method in payment_methods_display {
                    span { class: "px-2 py-0.5 text-xs bg-muted rounded", "{method}" }
                }
                if extra_methods > 0 {
                    span { class: "px-2 py-0.5 text-xs text-muted-foreground", "+{extra_methods} more" }
                }
            }
            div { class: "flex items-center justify-between text-xs text-muted-foreground",
                div { class: "flex items-center gap-2",
                    P2PStatusBadge { status: order.status }
                    P2PLayerBadge { layer: order.layer }
                    if let Some(platform) = &order.platform {
                        span { class: "px-1.5 py-0.5 bg-accent rounded", "{platform}" }
                    }
                    if !crate::stores::mostro::creation_ledger::entries_for_order(&order.order_id)
                        .is_empty()
                    {
                        span { class: "px-1.5 py-0.5 bg-primary/15 text-primary rounded",
                            "✓ Yours"
                        }
                    }
                }
                span { "{format_relative_time(Timestamp::from(order.created_at))}" }
            }
            if let Some(remaining) = order.time_remaining() {
                if remaining < 3600 {
                    div { class: "mt-2 text-xs text-red-500 flex items-center gap-1",
                        svg {
                            class: "w-3 h-3",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { d: "M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" }
                        }
                        "Expires in {format_duration_compact(remaining)}"
                    }
                }
            }
            // Mostro-specific: a Take button for orders that the user can
            // execute on a Mostro node. Stops propagation so the wrapping
            // <Link> doesn't also navigate to the detail view.
            if order.platform.as_deref() == Some("mostro") {
                div { class: "mt-3 pt-3 border-t border-border flex justify-end",
                    TakeMostroButton { order: order.clone() }
                }
            }
        }
    }
}


/// Skeleton loader for order cards
#[component]
pub fn P2POrderCardSkeleton() -> Element {
    rsx! {
        div { class: "p-4 animate-pulse",
            div { class: "flex items-center justify-between mb-2",
                div { class: "h-6 w-12 bg-muted rounded" }
                div { class: "h-6 w-24 bg-muted rounded" }
            }
            div { class: "flex items-center gap-4 mb-2",
                div { class: "h-4 w-20 bg-muted rounded" }
                div { class: "h-4 w-12 bg-muted rounded" }
            }
            div { class: "flex gap-1 mb-2",
                div { class: "h-5 w-16 bg-muted rounded" }
                div { class: "h-5 w-20 bg-muted rounded" }
                div { class: "h-5 w-14 bg-muted rounded" }
            }
            div { class: "flex items-center justify-between",
                div { class: "flex items-center gap-2",
                    div { class: "h-5 w-16 bg-muted rounded-full" }
                    div { class: "h-5 w-20 bg-muted rounded" }
                }
                div { class: "h-4 w-16 bg-muted rounded" }
            }
        }
    }
}
