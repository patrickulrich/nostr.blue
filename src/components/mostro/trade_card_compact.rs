use crate::components::mostro::trade_status_badge::TradeStatusBadge;
use crate::stores::mostro::trade_store::{Trade, TradeRole};
use crate::utils::time::format_time_ago;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct TradeCardCompactProps {
    pub trade: Trade,
    pub on_click: EventHandler<String>,
}

#[component]
pub fn TradeCardCompact(props: TradeCardCompactProps) -> Element {
    let t = &props.trade;
    let kind_label = match t.kind.as_str() {
        "sell" => ("bg-red-500/20 text-red-600 dark:text-red-400", "SELL"),
        "buy" => ("bg-green-500/20 text-green-600 dark:text-green-400", "BUY"),
        _ => ("bg-muted text-muted-foreground", "OTHER"),
    };
    let role_label = match t.role {
        TradeRole::Taker => "Taker",
        TradeRole::Maker => "Maker",
    };

    rsx! {
        button {
            class: "w-full text-left p-3 bg-card border border-border rounded-lg hover:bg-accent/50 transition",
            onclick: {
                let order_id = t.order_id.clone();
                move |_| (props.on_click)(order_id.clone())
            },
            div { class: "flex items-center justify-between gap-3",
                div { class: "flex items-center gap-2 min-w-0",
                    span { class: "px-1.5 py-0.5 text-[10px] font-bold rounded {kind_label.0}",
                        "{kind_label.1}"
                    }
                    span { class: "text-sm font-medium truncate",
                        "{t.fiat_amount} {t.fiat_code}"
                    }
                }
                div { class: "flex items-center gap-2 shrink-0",
                    span { class: "text-xs text-muted-foreground",
                        "{role_label}"
                    }
                    TradeStatusBadge { status: t.status }
                    span { class: "text-xs text-muted-foreground",
                        {format_time_ago(t.updated_at as u64)}
                    }
                }
            }
            if let Some(sats) = t.sats_amount {
                div { class: "mt-1 text-xs text-muted-foreground",
                    "{sats} sats"
                }
            }
        }
    }
}
