use crate::components::ConfirmModal;
use crate::stores::social::mostro::trade_store::{Trade, TradeStatus};
use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub enum TradeAction {
    AddInvoice(String),
    AddBondInvoice(String),
    FiatSent,
    Release,
    Cancel,
    AcceptCancel,
    Dispute,
    Rate(u8),
}

#[derive(Props, Clone, PartialEq)]
pub struct TradeActionPanelProps {
    pub trade: Trade,
    pub on_action: EventHandler<TradeAction>,
}

#[component]
pub fn TradeActionPanel(props: TradeActionPanelProps) -> Element {
    let t = &props.trade;
    let mut invoice_input = use_signal(String::new);
    let mut rating = use_signal(|| 0u8);

    let show_invoice_input = (matches!(t.status, TradeStatus::WaitingBuyerInvoice)
        || matches!(t.status, TradeStatus::PaymentFailed))
        && t.is_buyer();
    let show_payment_failed_info = matches!(t.status, TradeStatus::PaymentFailed)
        && t.payment_failed_attempts.is_some();
    let show_bond_input = t.needs_bond_invoice
        && matches!(t.status, TradeStatus::Pending | TradeStatus::WaitingBond | TradeStatus::WaitingTakerBond);
    let show_bond_payout = t.needs_bond_invoice
        && !matches!(t.status, TradeStatus::Pending | TradeStatus::WaitingBond | TradeStatus::WaitingTakerBond)
        && t.status.is_terminal();
    let show_fiat_sent = matches!(t.status, TradeStatus::Active)
        && t.is_buyer();
    let show_release = matches!(t.status, TradeStatus::FiatSent)
        && t.is_seller();
    let show_dispute = matches!(
        t.status,
        TradeStatus::Active | TradeStatus::FiatSent | TradeStatus::CancelPending
    );
    let show_cancel = matches!(
        t.status,
        TradeStatus::Pending
            | TradeStatus::WaitingBond
            | TradeStatus::WaitingTakerBond
            | TradeStatus::WaitingBuyerInvoice
            | TradeStatus::WaitingSellerToPay
            | TradeStatus::Active
            | TradeStatus::FiatSent
    );
    let show_accept_cancel = matches!(t.status, TradeStatus::CancelPending);
    let show_rating = matches!(t.status, TradeStatus::Success);
    let mut rating_confirm_open = use_signal(|| false);

    rsx! {
        div { class: "p-4 bg-card border border-border rounded-lg space-y-3",
            h3 { class: "text-sm font-semibold", "Actions" }

            if show_invoice_input {
                div { class: "space-y-2",
                    label { class: "text-xs text-muted-foreground", "Payout Invoice" }
                    input {
                        class: "w-full p-2 border border-border rounded-lg bg-background text-sm font-mono",
                        r#type: "text",
                        placeholder: "lnbc... or you@domain.com",
                        value: "{invoice_input}",
                        oninput: move |e| invoice_input.set(e.value()),
                    }
                    button {
                        class: "w-full px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium disabled:opacity-50",
                        disabled: invoice_input.read().trim().is_empty(),
                        onclick: {
                            let val = invoice_input.read().clone();
                            move |_| {
                                let v = val.trim().to_string();
                                if !v.is_empty() {
                                    (props.on_action)(TradeAction::AddInvoice(v));
                                }
                            }
                        },
                        "Submit Invoice"
                    }
                }
            }

            if show_payment_failed_info {
                {
                    let attempts = t.payment_failed_attempts.unwrap_or(0);
                    let interval = t.payment_failed_retries_interval.unwrap_or(0);
                    rsx! {
                        div { class: "p-3 bg-amber-500/10 border border-amber-500/30 rounded-lg space-y-1",
                            p { class: "text-xs font-medium text-amber-500",
                                "Payment Failed"
                            }
                            p { class: "text-xs text-muted-foreground",
                                "The daemon will retry up to {attempts} time(s), every {interval}s. Submit a new invoice below to retry immediately."
                            }
                        }
                    }
                }
            }

            if show_bond_payout {
                div { class: "p-3 bg-amber-500/10 border border-amber-500/30 rounded-lg space-y-2",
                    p { class: "text-xs font-medium text-amber-500",
                        "Claim Slashed Bond"
                    }
                    p { class: "text-xs text-muted-foreground",
                        "Counterparty's bond was slashed. Submit an invoice to claim your share."
                    }
                    input {
                        class: "w-full p-2 border border-border rounded-lg bg-background text-sm font-mono",
                        r#type: "text",
                        placeholder: "lnbc... or you@domain.com",
                        value: "{invoice_input}",
                        oninput: move |e| invoice_input.set(e.value()),
                    }
                    button {
                        class: "w-full px-4 py-2 bg-amber-600 text-white rounded-lg text-sm font-medium disabled:opacity-50",
                        disabled: invoice_input.read().trim().is_empty(),
                        onclick: {
                            let val = invoice_input.read().clone();
                            move |_| {
                                let v = val.trim().to_string();
                                if !v.is_empty() {
                                    (props.on_action)(TradeAction::AddBondInvoice(v));
                                }
                            }
                        },
                        "Submit Payout Invoice"
                    }
                }
            }

            if show_bond_input {
                div { class: "space-y-2",
                    p { class: "text-xs text-muted-foreground",
                        "The daemon requires a bond. Paste your bond invoice below."
                    }
                    input {
                        class: "w-full p-2 border border-border rounded-lg bg-background text-sm font-mono",
                        r#type: "text",
                        placeholder: "lnbc... or you@domain.com",
                        value: "{invoice_input}",
                        oninput: move |e| invoice_input.set(e.value()),
                    }
                    button {
                        class: "w-full px-4 py-2 bg-amber-600 text-white rounded-lg text-sm font-medium disabled:opacity-50",
                        disabled: invoice_input.read().trim().is_empty(),
                        onclick: {
                            let val = invoice_input.read().clone();
                            move |_| {
                                let v = val.trim().to_string();
                                if !v.is_empty() {
                                    (props.on_action)(TradeAction::AddBondInvoice(v));
                                }
                            }
                        },
                        "Submit Bond Invoice"
                    }
                }
            }

            if show_fiat_sent {
                button {
                    class: "w-full px-4 py-2 bg-blue-600 text-white rounded-lg text-sm font-medium",
                    onclick: move |_| (props.on_action)(TradeAction::FiatSent),
                    "I Sent Fiat"
                }
            }

            if show_release {
                button {
                    class: "w-full px-4 py-2 bg-green-600 text-white rounded-lg text-sm font-medium",
                    onclick: move |_| (props.on_action)(TradeAction::Release),
                    "Release Sats"
                }
            }

            if show_accept_cancel {
                div { class: "space-y-2",
                    if t.fiat_was_sent {
                        div { class: "p-3 bg-red-500/10 border border-red-500/30 rounded-lg",
                            p { class: "text-xs text-red-500",
                                "Warning: Fiat was already sent. Accepting cancel will NOT reverse the fiat transfer."
                            }
                        }
                    }
                    button {
                        class: "w-full px-4 py-2 bg-amber-600 text-white rounded-lg text-sm font-medium",
                        onclick: move |_| (props.on_action)(TradeAction::AcceptCancel),
                        "Accept Cancel"
                    }
                }
            }

            if show_rating {
                div { class: "space-y-2",
                    p { class: "text-xs text-muted-foreground", "Rate your counterparty" }
                    div { class: "flex gap-1",
                        for star in 1..=5u8 {
                            {
                                let filled = *rating.read() >= star;
                                let cls = if filled {
                                    "text-yellow-400 cursor-pointer".to_string()
                                } else {
                                    "text-muted-foreground cursor-pointer hover:text-yellow-300".to_string()
                                };
                                let star_label = format!("★ {star}");
                                rsx! {
                                    button {
                                        key: "{star}",
                                        class: "text-2xl {cls}",
                                        onclick: move |_| {
                                            rating.set(star);
                                        },
                                        "{star_label}"
                                    }
                                }
                            }
                        }
                    }
                    if *rating.read() > 0 {
                        button {
                            class: "w-full px-4 py-2 bg-yellow-600 text-white rounded-lg text-sm font-medium",
                            onclick: move |_| {
                                let r = *rating.read();
                                rating_confirm_open.set(false);
                                (props.on_action)(TradeAction::Rate(r));
                            },
                            "Submit {rating()} Star Rating"
                        }
                    }
                }
            }

            if show_dispute {
                button {
                    class: "w-full px-4 py-2 border border-red-500/50 text-red-500 rounded-lg text-sm font-medium hover:bg-red-500/10",
                    onclick: move |_| rating_confirm_open.set(true),
                    "Open Dispute"
                }
            }

            if show_cancel && !show_accept_cancel {
                button {
                    class: "w-full px-4 py-2 border border-border rounded-lg text-sm text-muted-foreground hover:text-foreground",
                    onclick: move |_| (props.on_action)(TradeAction::Cancel),
                    "Cancel Trade"
                }
            }
        }

        if *rating_confirm_open.read() {
            ConfirmModal {
                title: "Open Dispute".to_string(),
                message: "Are you sure you want to open a dispute? An admin/solver will be assigned to resolve the trade.".to_string(),
                confirm_text: Some("Open Dispute".to_string()),
                cancel_text: Some("Cancel".to_string()),
                on_confirm: move |_| {
                    rating_confirm_open.set(false);
                    (props.on_action)(TradeAction::Dispute);
                },
                on_cancel: move |_| rating_confirm_open.set(false),
            }
        }
    }
}
