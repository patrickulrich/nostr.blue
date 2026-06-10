use crate::components::p2p::trade_card_compact::TradeCardCompact;
use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::social::mostro::trade_store::TRADES;
use dioxus::prelude::*;

#[component]
pub fn P2PMyTrades() -> Element {
    let trades = TRADES.read();
    let sorted: Vec<_> = {
        let mut v: Vec<_> = trades.iter().collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        v
    };

    rsx! {
        div { class: "min-h-screen p-4 max-w-3xl mx-auto",
            if !*crate::stores::nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else {
                div { class: "space-y-4",
                    div { class: "flex items-center gap-3",
                        button {
                            class: "p-2 hover:bg-accent rounded-lg",
                            title: "Back to P2P",
                            onclick: move |_| {
                                let _ = navigator().push(Route::P2PHome {});
                            },
                            crate::components::icons::ArrowLeftIcon { class: "w-5 h-5".to_string() }
                        }
                        h1 { class: "text-xl font-bold", "My Trades" }
                    }

                    if sorted.is_empty() {
                        div { class: "p-8 text-center",
                            div { class: "text-4xl mb-4", "📋" }
                            h3 { class: "text-lg font-medium mb-2", "No trades yet" }
                            p { class: "text-muted-foreground mb-4",
                                "Take an order to start trading."
                            }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                                onclick: move |_| {
                                    let _ = navigator().push(Route::P2PHome {});
                                },
                                "Browse Orders"
                            }
                        }
                    } else {
                        div { class: "space-y-2",
                            for trade in sorted {
                                TradeCardCompact {
                                    key: "{trade.order_id}",
                                    trade: trade.clone(),
                                    on_click: move |order_id: String| {
                                        let _ = navigator().push(Route::P2PTradeDetail { order_id });
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
