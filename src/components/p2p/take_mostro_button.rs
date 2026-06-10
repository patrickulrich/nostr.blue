use crate::routes::Route;
use crate::stores::social::mostro::{self, TakeRequest};
use crate::utils::nip69::{FiatAmount, P2POrder};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use std::time::Duration;

#[component]
pub fn TakeMostroButton(order: P2POrder) -> Element {
    let taking = use_signal(|| false);
    let mut show_amount_modal = use_signal(|| false);
    let range_amount = use_signal(String::new);
    let is_range = matches!(order.fiat_amount, FiatAmount::Range { .. });

    rsx! {
        button {
            class: if *taking.read() {
                "px-3 py-1.5 text-sm bg-primary/60 text-primary-foreground rounded-lg cursor-wait"
            } else {
                "px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition"
            },
            title: "Take this Mostro order (opens trade screen)",
            disabled: *taking.read(),
            onclick: move |e| {
                e.prevent_default();
                e.stop_propagation();
                if *taking.peek() {
                    return;
                }
                if is_range {
                    show_amount_modal.set(true);
                } else {
                    do_take(&order, None, taking);
                }
            },
            if *taking.read() { "Taking…" } else { "Take with Mostro →" }
        }
        if *show_amount_modal.read() {
            {render_range_modal(&order, show_amount_modal, range_amount, taking)}
        }
    }
}

fn do_take(
    order: &P2POrder,
    fiat_amount_override: Option<f64>,
    mut taking: Signal<bool>,
) {
    taking.set(true);
    let toast = consume_toast();
    let order_clone = order.clone();
    spawn(async move {
        let req = TakeRequest {
            order: order_clone,
            buyer_invoice: None,
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
                let _ = navigator().push(Route::P2PTradeDetail {
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
                                do_take(&order, amt, taking);
                            }
                        },
                        "Take Order"
                    }
                }
            }
        }
    }
}
