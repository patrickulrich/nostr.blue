use crate::components::mostro::trade_card_compact::TradeCardCompact;
use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::mostro::trade_store;
use dioxus::prelude::*;

#[component]
pub fn MostroMyTrades() -> Element {
    // Reactive: `all_trades_for_daemon` reads the `TRADES` GlobalSignal, so
    // wrapping it in `use_memo` makes My Trades update live (new trade,
    // status change, recovery) without a manual reload. Previously this was
    // a one-shot synchronous read that went stale until navigation.
    let trades = use_memo(trade_store::all_trades_for_daemon);
    let sorted: Vec<_> = {
        let mut v: Vec<_> = trades.read().iter().cloned().collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        v
    };
    let has_trades = !sorted.is_empty();

    rsx! {
        div { class: "min-h-screen p-4 max-w-3xl mx-auto",
            if !*crate::stores::nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else {
                div { class: "space-y-4",
                    div { class: "flex items-center justify-between",
                        div { class: "flex items-center gap-3",
                            button {
                                class: "p-2 hover:bg-accent rounded-lg",
                                title: "Back to P2P",
                                onclick: move |_| {
                                    let _ = navigator().push(Route::MostroHome {});
                                },
                                crate::components::icons::ArrowLeftIcon { class: "w-5 h-5".to_string() }
                            }
                            h1 { class: "text-xl font-bold", "My Trades" }
                        }
                        if has_trades {
                            button {
                                class: "p-2 hover:bg-accent rounded-lg text-sm text-muted-foreground",
                                title: "Export trades as CSV",
                                onclick: move |_| {
                                    export_trades_csv();
                                },
                                "Export"
                            }
                        }
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
                                    let _ = navigator().push(Route::MostroHome {});
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
                                        let _ = navigator().push(Route::MostroTradeDetail { order_id });
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

fn export_trades_csv() {
    let trades = trade_store::all_trades_for_daemon();
    let mut csv = String::from("Order ID,Kind,Role,Fiat Amount,Fiat Code,Sats,Premium,Status,Payment Methods,Created,Updated\n");
    for t in &trades {
        let methods = t.payment_methods.join("; ");
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{}\n",
            t.order_id,
            t.kind,
            t.role.as_str(),
            t.fiat_amount,
            t.fiat_code,
            t.sats_amount.unwrap_or(0),
            t.premium,
            t.status.label(),
            methods,
            t.created_at,
            t.updated_at,
        ));
    }
    let _ = crate::platform::download::save_file("mostro-trades.csv", &csv, "text/csv");
}
