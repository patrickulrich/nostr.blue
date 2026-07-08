use crate::components::mostro::trade_card_compact::TradeCardCompact;
use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::mostro::trade_store;
use dioxus::prelude::*;

#[component]
pub fn MostroMyTrades() -> Element {
    let heal_done = use_signal(|| false);

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

    // Self-heal: if My Trades is empty and we haven't attempted healing yet,
    // try to recover trades from the NIP-78 blob, the creation ledger, and
    // the daemon's RestoreSession. This catches the case where the local
    // TRADES cache was wiped (orphan cleanup, fresh login, etc.) but the
    // trades still exist on the daemon.
    use_effect(move || {
        if *heal_done.read() {
            return;
        }
        if !*crate::stores::nostr_client::CLIENT_INITIALIZED.read() {
            return;
        }
        if crate::stores::mostro::try_get().is_none() {
            return;
        }
        if crate::stores::mostro::try_get_node_config().is_none() {
            return;
        }
        if has_trades {
            return;
        }
        let mut heal_done = heal_done;
        heal_done.set(true);
        spawn(async move {
            log::info!("My Trades is empty — attempting self-heal");
            // 1. Refresh from NIP-78 trades blob
            let _ = trade_store::refresh_from_relays().await;
            // 2. Recover individual orders from the creation ledger
            let ledger = crate::stores::mostro::creation_ledger::CREATION_LEDGER.read().clone();
            for entry in &ledger {
                if trade_store::find_by_order_id(&entry.order_id).is_some() {
                    continue;
                }
                if let Ok(uuid) = uuid::Uuid::parse_str(&entry.order_id) {
                    log::info!(
                        "My Trades self-heal: recovering order {} from daemon",
                        entry.order_id
                    );
                    match crate::stores::mostro::recover_order_by_id(uuid).await {
                        Ok(1) => {
                            log::info!("My Trades self-heal: recovered {}", entry.order_id);
                        }
                        Ok(_) => {
                            log::debug!(
                                "My Trades self-heal: daemon has no record of {}",
                                entry.order_id
                            );
                        }
                        Err(e) => {
                            log::warn!(
                                "My Trades self-heal: failed to recover {}: {e}",
                                entry.order_id
                            );
                        }
                    }
                }
            }
            // 3. RestoreSession for non-terminal gaps
            if !crate::stores::mostro::is_restore_in_progress() {
                if let Err(e) = crate::stores::mostro::request_restore().await {
                    log::warn!("My Trades self-heal: restore failed: {e}");
                }
            }
        });
    });

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
