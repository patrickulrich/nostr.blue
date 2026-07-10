use crate::routes::Route;
use crate::stores::mostro::i18n;
use crate::stores::mostro::{self, MostroNodeConfig, TakeRequest};
use crate::utils::nip69::{FiatAmount, OrderType, P2POrder};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use std::time::Duration;

#[component]
pub fn TakeMostroButton(order: P2POrder) -> Element {
    let taking = use_signal(|| false);
    let show_amount_modal = use_signal(|| false);
    let show_sell_invoice_modal = use_signal(|| false);
    let mut pending_daemon_switch: Signal<Option<MostroNodeConfig>> = use_signal(|| None);
    let range_amount = use_signal(String::new);
    let is_range = matches!(order.fiat_amount, FiatAmount::Range { .. });
    let is_sell = order.order_type == OrderType::Sell;
    // Disable taking if the current user already owns this order as maker.
    // `take_order` blocks self-takes server-side too, but disabling here is
    // better UX (no error toast) and avoids corrupting local state.
    // We check both TRADES and the durable creation_ledger so the disable
    // survives a TRADES cache wipe (e.g., orphan cleanup).
    let owns_order = crate::stores::mostro::find_by_order_id(&order.order_id)
        .is_some_and(|t| t.role == crate::stores::mostro::TradeRole::Maker)
        || crate::stores::mostro::creation_ledger::entries_for_order(&order.order_id)
            .iter()
            .any(|e| e.role == crate::stores::mostro::TradeRole::Maker);

    rsx! {
        button {
            class: if *taking.read() {
                "px-3 py-1.5 text-sm bg-primary/60 text-primary-foreground rounded-lg cursor-wait"
            } else {
                "px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition"
            },
            title: if owns_order {
                "You can't take your own order"
            } else if crate::stores::mostro::is_restore_in_progress() {
                "Session restore in progress — please wait"
            } else {
                "Take this Mostro order (opens trade screen)"
            },
            disabled: owns_order
                || *taking.read()
                || crate::stores::mostro::is_restore_in_progress(),
            onclick: move |e| {
                e.prevent_default();
                e.stop_propagation();
                if owns_order || *taking.peek() || crate::stores::mostro::is_restore_in_progress() {
                    return;
                }
                // Phase 1.4 (C8): before taking, check if the order's
                // source tag names a different daemon. If so, show a
                // confirmation prompt — silently switching daemons is a
                // phishing vector (a malicious order with a crafted source
                // tag could redirect trades to an attacker's daemon).
                match mostro::check_source_tag_daemon_switch(&order) {
                    Ok(Some(new_cfg)) => {
                        pending_daemon_switch.set(Some(new_cfg));
                    }
                    Ok(None) => {
                        // Same daemon or no source tag — proceed normally.
                        proceed_to_take(
                            is_range,
                            is_sell,
                            show_amount_modal,
                            show_sell_invoice_modal,
                            &order,
                            taking,
                        );
                    }
                    Err(_e) => {
                        // No daemon configured — let take_order surface
                        // the canonical error.
                        proceed_to_take(
                            is_range,
                            is_sell,
                            show_amount_modal,
                            show_sell_invoice_modal,
                            &order,
                            taking,
                        );
                    }
                }
            },
            { if *taking.read() { i18n::tr("mostro.taking") } else { i18n::tr("mostro.take_with_mostro") } }
        }
        if *show_amount_modal.read() {
            {render_range_modal(&order, show_amount_modal, range_amount, taking)}
        }
        // Phase 4.2 (U10): sell-take invoice prompt for non-range sell
        // orders. The user can optionally provide a payout invoice or
        // Lightning Address so the daemon knows where to send funds. If
        // skipped, the daemon will request it via `Action::AddInvoice`
        // later.
        if *show_sell_invoice_modal.read() {
            {render_sell_invoice_modal(&order, show_sell_invoice_modal, taking)}
        }
        if let Some(new_cfg) = (*pending_daemon_switch.read()).clone() {
            {render_daemon_switch_prompt(
                &order,
                &new_cfg,
                pending_daemon_switch,
                show_amount_modal,
                show_sell_invoice_modal,
                is_range,
                is_sell,
                taking,
            )}
        }
    }
}

/// Decide which prompt (if any) to show based on order type + range mode.
/// Phase 4.2 (U10): non-range sell orders get an invoice prompt; range
/// orders get the amount prompt; buy orders proceed directly.
/// Phase 6.4: surfaces client-side validation warnings as a toast before
/// proceeding.
fn proceed_to_take(
    is_range: bool,
    is_sell: bool,
    mut show_amount_modal: Signal<bool>,
    mut show_sell_invoice_modal: Signal<bool>,
    order: &P2POrder,
    taking: Signal<bool>,
) {
    // Phase 6.4: validate against daemon limits before proceeding.
    let warnings = mostro::validate_against_node_limits(
        &order.currency,
        Some(order.amount_sats as i64),
        None,
        "taker",
    );
    if !warnings.is_empty() {
        let toast = consume_toast();
        for w in warnings {
            toast.warning(
                "Daemon limit".to_string(),
                ToastOptions::new()
                    .description(w)
                    .duration(Duration::from_secs(6)),
            );
        }
    }

    if is_range {
        show_amount_modal.set(true);
    } else if is_sell {
        // Phase 4.2 (U10): sell-take (we're the buyer) — prompt for an
        // optional payout invoice / Lightning Address.
        show_sell_invoice_modal.set(true);
    } else {
        do_take(order, None, None, taking);
    }
}

/// Phase 1.4 (C8): confirmation prompt shown when an order's source tag
/// names a different daemon than the currently-selected one. The user must
/// explicitly accept the switch before the take proceeds — silently
/// switching daemons is a phishing vector.
#[allow(clippy::too_many_arguments)]
fn render_daemon_switch_prompt(
    order: &P2POrder,
    new_cfg: &MostroNodeConfig,
    mut pending_daemon_switch: Signal<Option<MostroNodeConfig>>,
    mut show_amount_modal: Signal<bool>,
    mut show_sell_invoice_modal: Signal<bool>,
    is_range: bool,
    is_sell: bool,
    taking: Signal<bool>,
) -> Element {
    let new_cfg_clone = new_cfg.clone();
    let new_cfg_for_cancel = new_cfg.clone();
    let new_pubkey = new_cfg.pubkey.clone();
    // Show a truncated pubkey for readability.
    let short_pk: String = if new_pubkey.len() > 16 {
        format!("{}…{}", &new_pubkey[..8], &new_pubkey[new_pubkey.len() - 8..])
    } else {
        new_pubkey.clone()
    };
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |e| {
                e.stop_propagation();
                e.prevent_default();
            },
            div { class: "bg-card border border-border rounded-lg p-6 w-96 space-y-4",
                h3 { class: "text-sm font-semibold", "Switch Mostro Daemon?" }
                p { class: "text-xs text-muted-foreground",
                    "This order was created on a different Mostro daemon. \
                     Taking it will switch your active daemon."
                }
                div { class: "p-3 bg-accent/50 rounded text-xs space-y-1",
                    p { "New daemon: " code { "{short_pk}" } }
                    p { "Relays: " {new_cfg.relays.to_vec().join(", ")} }
                }
                p { class: "text-xs text-muted-foreground",
                    "Trade keys are not affected (they are daemon-agnostic), \
                     but your future Mostro requests will go to this daemon."
                }
                div { class: "flex gap-2",
                    button {
                        class: "flex-1 px-3 py-2 border border-border rounded-lg text-sm text-muted-foreground",
                        onclick: move |_| {
                            log::info!(
                                "User dismissed daemon switch to {}",
                                new_cfg_for_cancel.pubkey
                            );
                            pending_daemon_switch.set(None);
                        },
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-3 py-2 bg-primary text-primary-foreground rounded-lg text-sm",
                        onclick: {
                            let order = order.clone();
                            let new_cfg = new_cfg_clone.clone();
                            move |_| {
                                let new_cfg = new_cfg.clone();
                                let order = order.clone();
                                pending_daemon_switch.set(None);
                                spawn(async move {
                                    if let Err(e) = mostro::save_node_config(new_cfg.clone()).await {
                                        let toast = consume_toast();
                                        toast.error(
                                            "Failed to switch daemon".to_string(),
                                            ToastOptions::new()
                                                .description(e)
                                                .duration(Duration::from_secs(5)),
                                        );
                                        return;
                                    }
                                    // Switch saved — proceed with the take flow.
                                    if is_range {
                                        show_amount_modal.set(true);
                                    } else if is_sell {
                                        show_sell_invoice_modal.set(true);
                                    } else {
                                        do_take(&order, None, None, taking);
                                    }
                                });
                            }
                        },
                        "Switch & Take"
                    }
                }
            }
        }
    }
}

fn do_take(
    order: &P2POrder,
    fiat_amount_override: Option<f64>,
    buyer_invoice: Option<String>,
    mut taking: Signal<bool>,
) {
    taking.set(true);
    let toast = consume_toast();
    let order_clone = order.clone();
    spawn(async move {
        let req = TakeRequest {
            order: order_clone,
            buyer_invoice: buyer_invoice.filter(|s| !s.trim().is_empty()),
            fiat_amount_override,
            pow: 0,
        };
        match mostro::take_order(req).await {
            Ok(result) => {
                toast.info(
                    "Trade initiated".to_string(),
                    ToastOptions::new()
                        .description(format!(
                            "Sent {} to Mostro daemon",
                            result.sent_action
                        ))
                        .duration(Duration::from_secs(3)),
                );
                let _ = navigator().push(Route::MostroTradeDetail {
                    order_id: result.order_id,
                });
            }
            Err(err) => {
                toast.error(
                    "Failed to take order".to_string(),
                    ToastOptions::new()
                        .description(err.clone())
                        .duration(Duration::from_secs(6)),
                );
                log::error!("Mostro take_order failed: {err}");
            }
        }
        taking.set(false);
    });
}

/// Phase 4.2 (U10): sell-take invoice prompt for non-range sell orders.
/// The user can optionally provide a payout invoice (bolt11) or Lightning
/// Address (user@domain.com). If skipped, the daemon will request the
/// invoice via `Action::AddInvoice` once the trade progresses.
fn render_sell_invoice_modal(
    order: &P2POrder,
    mut show_modal: Signal<bool>,
    taking: Signal<bool>,
) -> Element {
    // Phase 7.3: pre-fill with the user's default Lightning address if set.
    let mut invoice_input =
        use_signal(|| crate::stores::ui::p2p_settings::default_ln_address().unwrap_or_default());
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |e| {
                e.stop_propagation();
                e.prevent_default();
            },
            div { class: "bg-card border border-border rounded-lg p-6 w-96 space-y-4",
                h3 { class: "text-sm font-semibold", "Payout Invoice (optional)" }
                p { class: "text-xs text-muted-foreground",
                    "You're taking a sell order (buying sats). Provide a \
                     Lightning invoice or address where you'd like to \
                     receive the payout. You can skip this and provide it later."
                }
                input {
                    class: "w-full p-2 border border-border rounded-lg bg-background text-sm font-mono",
                    r#type: "text",
                    placeholder: "lnbc... or you@domain.com",
                    value: "{invoice_input}",
                    oninput: move |e| invoice_input.set(e.value()),
                }
                div { class: "flex gap-2",
                    button {
                        class: "flex-1 px-3 py-2 border border-border rounded-lg text-sm text-muted-foreground",
                        onclick: {
                            let order = order.clone();
                            move |_| {
                                // Skip — no invoice. Daemon will request later.
                                show_modal.set(false);
                                do_take(&order, None, None, taking);
                            }
                        },
                        "Skip"
                    }
                    button {
                        class: "flex-1 px-3 py-2 bg-primary text-primary-foreground rounded-lg text-sm disabled:opacity-50",
                        disabled: invoice_input.read().trim().is_empty(),
                        onclick: {
                            let order = order.clone();
                            move |_| {
                                let inv = invoice_input.read().trim().to_string();
                                show_modal.set(false);
                                do_take(&order, None, Some(inv), taking);
                            }
                        },
                        "Take Order"
                    }
                }
            }
        }
    }
}

fn render_range_modal(
    order: &P2POrder,
    mut show_modal: Signal<bool>,
    mut range_amount: Signal<String>,
    taking: Signal<bool>,
) -> Element {
    let (min, max) = match &order.fiat_amount {
        FiatAmount::Range { min, max } => (*min, *max),
        _ => return rsx! {},
    };
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |e| {
                e.stop_propagation();
                e.prevent_default();
            },
            div { class: "bg-card border border-border rounded-lg p-6 w-80 space-y-4",
                h3 { class: "text-sm font-semibold", "Specify Amount" }
                p { class: "text-xs text-muted-foreground",
                    "This order accepts {min:.0}-{max:.0} {order.currency}. Enter your desired amount."
                }
                input {
                    class: "w-full p-2 border border-border rounded-lg bg-background text-sm",
                    r#type: "number",
                    placeholder: "{min:.0}-{max:.0}",
                    value: "{range_amount}",
                    oninput: move |e| range_amount.set(e.value()),
                }
                div { class: "flex gap-2",
                    button {
                        class: "flex-1 px-3 py-2 border border-border rounded-lg text-sm text-muted-foreground",
                        onclick: move |_| show_modal.set(false),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-3 py-2 bg-primary text-primary-foreground rounded-lg text-sm disabled:opacity-50",
                        disabled: range_amount.read().trim().parse::<f64>().ok().filter(|&v| v >= min && v <= max).is_none(),
                        onclick: {
                            let order = order.clone();
                            move |_| {
                                let amt = range_amount.read().trim().parse::<f64>().ok();
                                show_modal.set(false);
                                do_take(&order, amt, None, taking);
                            }
                        },
                        "Take Order"
                    }
                }
            }
        }
    }
}
