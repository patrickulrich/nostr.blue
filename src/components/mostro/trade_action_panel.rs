use crate::components::ConfirmModal;
use crate::stores::mostro::i18n;
use crate::stores::mostro::trade_store::{Trade, TradeStatus};
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
    Discard,
}

#[derive(Props, Clone, PartialEq)]
pub struct TradeActionPanelProps {
    pub trade: Trade,
    pub on_action: EventHandler<TradeAction>,
    #[props(default)]
    pub countdown_tick: u64,
    #[props(default)]
    pub busy: bool,
}

#[component]
pub fn TradeActionPanel(props: TradeActionPanelProps) -> Element {
    let t = &props.trade;
    let mut invoice_input = use_signal(String::new);
    let mut rating = use_signal(|| 0u8);

    let show_invoice_input = (matches!(t.status, TradeStatus::WaitingBuyerInvoice)
        || matches!(t.status, TradeStatus::PaymentFailed)
        || matches!(t.status, TradeStatus::Settled))
        && t.is_buyer();
    let show_payment_failed_info = matches!(t.status, TradeStatus::PaymentFailed)
        && t.payment_failed_attempts.is_some();
    // Phase 2.4e (U11): include `WaitingMakerBond` so a maker can pay their
    // anti-abuse bond when the daemon requests it (was previously excluded,
    // leaving the maker stuck if the daemon's `apply_to` policy includes "make").
    let show_bond_input = t.needs_bond_invoice && !t.needs_bond_payout
        && matches!(t.status, TradeStatus::Pending | TradeStatus::WaitingBond | TradeStatus::WaitingTakerBond | TradeStatus::WaitingMakerBond);
    let show_bond_payout = t.needs_bond_payout;
    let show_fiat_sent = matches!(t.status, TradeStatus::Active)
        && t.is_buyer();
    let show_release = (matches!(t.status, TradeStatus::FiatSent)
        || (matches!(t.status, TradeStatus::CancelPending) && t.fiat_was_sent)
        || matches!(t.status, TradeStatus::Dispute))
        && t.is_seller();
    // Phase 2.4c (U4): include `CooperativelyCanceled` so a user who
    // initiated a cooperative cancel but hasn't yet had it accepted can
    // still dispute (matches the reference FSM). Gated on `fiat_was_sent`
    // — disputing a pre-fiat cancel is just a regular cancel.
    let show_dispute = matches!(
        t.status,
        TradeStatus::Active | TradeStatus::FiatSent | TradeStatus::CancelPending | TradeStatus::Dispute
    ) || (matches!(t.status, TradeStatus::CooperativelyCanceled) && t.fiat_was_sent);
    // Phase 2.4a (U1, U2): cancel must also be available from `Dispute`
    // (both roles can cancel mid-dispute per the reference FSM) and from
    // `Settled` (settled-hold-invoice state allows cancel as an escape
    // hatch while the buyer's payout is in flight).
    let show_cancel = matches!(
        t.status,
        TradeStatus::Pending
            | TradeStatus::WaitingBond
            | TradeStatus::WaitingTakerBond
            | TradeStatus::WaitingMakerBond
            | TradeStatus::WaitingBuyerInvoice
            | TradeStatus::WaitingSellerToPay
            | TradeStatus::Active
            | TradeStatus::FiatSent
            | TradeStatus::Dispute
            | TradeStatus::Settled
    );
    let show_accept_cancel = matches!(t.status, TradeStatus::CancelPending);
    let show_fiat_sent_during_cancel = matches!(t.status, TradeStatus::CancelPending)
        && !t.fiat_was_sent
        && t.is_buyer();
    let show_rating = matches!(t.status, TradeStatus::Success);
    let mut rating_confirm_open = use_signal(|| false);
    let mut cancel_confirm_open = use_signal(|| false);
    // Phase 2.4b (U3): Release is irreversible (escrow settles and the
    // daemon sends the buyer's payout). Match the Cancel/Dispute flow with
    // a confirmation modal.
    let mut release_confirm_open = use_signal(|| false);
    let mut nwc_generating = use_signal(|| false);

    rsx! {
        div { class: "p-4 bg-card border border-border rounded-lg space-y-3",
            h3 { class: "text-sm font-semibold", {i18n::tr("mostro.actions")} }

            if show_invoice_input {
                div { class: "space-y-2",
                    label { class: "text-xs text-muted-foreground", {i18n::tr("mostro.payout_invoice")} }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 p-2 border border-border rounded-lg bg-background text-sm font-mono",
                            r#type: "text",
                            placeholder: "lnbc... or you@domain.com",
                            value: "{invoice_input}",
                            oninput: move |e| invoice_input.set(e.value()),
                        }
                        // Phase 4.3 (F5): QR scanner for invoice input.
                        crate::components::qr_scanner::QrScanner {
                            label: "Scan".to_string(),
                            on_scan: move |decoded| {
                                invoice_input.set(decoded);
                            },
                        }
                    }
                    div { class: "flex gap-2",
                        {
                            let saved_addr = crate::stores::ui::p2p_settings::default_ln_address();
                            let input_empty = invoice_input.read().trim().is_empty();
                            if let Some(addr) = saved_addr {
                                if input_empty {
                                    rsx! {
                                        button {
                                            class: "px-3 py-2 border border-border rounded-lg text-xs text-muted-foreground hover:text-foreground transition",
                                            onclick: move |_| invoice_input.set(addr.clone()),
                                            "Use saved: {addr}"
                                        }
                                    }
                                } else {
                                    rsx! {}
                                }
                            } else {
                                rsx! {}
                            }
                        }
                        button {
                            class: "flex-1 px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium disabled:opacity-50",
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
                            {i18n::tr("mostro.submit_invoice")}
                        }
                        if crate::stores::nwc_store::is_connected() {
                            {
                                let sats = t.sats_amount.unwrap_or(0);
                                rsx! {
                                    button {
                                        class: "px-3 py-2 bg-green-600 text-white rounded-lg text-sm font-medium disabled:opacity-50",
                                        disabled: *nwc_generating.read() || sats == 0,
                                        onclick: move |_| {
                                            nwc_generating.set(true);
                                            let sats_amount = sats;
                                            let mut invoice_input_c = invoice_input;
                                            spawn(async move {
                                                match crate::stores::nwc_store::make_invoice(
                                                    sats_amount as u64 * 1000,
                                                    Some("Mostro payout".to_string()),
                                                    Some(3600),
                                                ).await {
                                                    Ok(resp) => {
                                                        invoice_input_c.set(resp.invoice);
                                                    }
                                                    Err(e) => {
                                                        log::warn!("NWC make_invoice failed: {e}");
                                                    }
                                                }
                                                nwc_generating.set(false);
                                            });
                                        },
                                        if *nwc_generating.read() { "Generating..." } else { "Generate" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if show_payment_failed_info {
                {
                    let _ = props.countdown_tick;
                    let attempts = t.payment_failed_attempts.unwrap_or(0);
                    let interval = t.payment_failed_retries_interval.unwrap_or(0);
                    let now = crate::platform::timestamp::now_secs() as i64;
                    let next_retry = if interval > 0 {
                        let elapsed = (now - t.updated_at).max(0);
                        let remaining = (interval as i64 - elapsed % interval as i64).max(0) as u32;
                        if elapsed < interval as i64 { remaining } else { 0 }
                    } else {
                        0
                    };
                    rsx! {
                        div { class: "p-3 bg-amber-500/10 border border-amber-500/30 rounded-lg space-y-1",
                            p { class: "text-xs font-medium text-amber-500",
                                "Payment Failed"
                            }
                            p { class: "text-xs text-muted-foreground",
                                "The daemon will retry up to {attempts} time(s), every {interval}s. Submit a new invoice below to retry immediately."
                            }
                            if interval > 0 {
                                p { class: "text-xs text-amber-500",
                                    if next_retry > 0 {
                                        "Next retry in {next_retry}s"
                                    } else {
                                        "Retrying now..."
                                    }
                                }
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
                    if let Some(deadline) = t.bond_payout_deadline {
                        {
                            let _ = props.countdown_tick;
                            let now = crate::platform::timestamp::now_secs() as i64;
                            let remaining = (deadline - now).max(0);
                            let rsx_elem = if remaining > 0 {
                                rsx! {
                                    p { class: "text-xs text-amber-500",
                                        "Deadline: {format_deadline_countdown(remaining)}"
                                    }
                                }
                            } else {
                                rsx! {
                                    p { class: "text-xs text-red-500 font-medium",
                                        "Claim window has expired"
                                    }
                                }
                            };
                            rsx_elem
                        }
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
                        "The daemon requires an anti-abuse bond. This collateral is held during the trade and released when complete, or slashed if you lose a dispute."
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

            if show_fiat_sent || show_fiat_sent_during_cancel {
                if show_fiat_sent_during_cancel {
                    div { class: "p-3 bg-amber-500/10 border border-amber-500/30 rounded-lg",
                        p { class: "text-xs text-amber-500",
                            "Counterparty wants to cancel, but fiat hasn't been sent yet. You can still complete this trade."
                        }
                    }
                }
                button {
                    class: "w-full px-4 py-2 bg-blue-600 text-white rounded-lg text-sm font-medium disabled:opacity-50",
                    disabled: props.busy,
                    onclick: move |_| (props.on_action)(TradeAction::FiatSent),
                    "I Sent Fiat"
                }
            }

            if show_release {
                button {
                    class: "w-full px-4 py-2 bg-green-600 text-white rounded-lg text-sm font-medium disabled:opacity-50",
                    disabled: props.busy,
                    onclick: move |_| release_confirm_open.set(true),
                    {i18n::tr("mostro.release_sats")}
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
                    } else {
                        div { class: "p-3 bg-amber-500/10 border border-amber-500/30 rounded-lg",
                            p { class: "text-xs text-amber-500",
                                "Counterparty wants to cancel the trade."
                            }
                        }
                    }
                    button {
                        class: "w-full px-4 py-2 bg-amber-600 text-white rounded-lg text-sm font-medium disabled:opacity-50",
                        disabled: props.busy,
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
                            class: "w-full px-4 py-2 bg-yellow-600 text-white rounded-lg text-sm font-medium disabled:opacity-50",
                            disabled: props.busy,
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
                    class: "w-full px-4 py-2 border border-red-500/50 text-red-500 rounded-lg text-sm font-medium hover:bg-red-500/10 disabled:opacity-50",
                    disabled: props.busy,
                    onclick: move |_| rating_confirm_open.set(true),
                    "Open Dispute"
                }
            }

            if show_cancel && !show_accept_cancel {
                button {
                    class: "w-full px-4 py-2 border border-border rounded-lg text-sm text-muted-foreground hover:text-foreground disabled:opacity-50",
                    disabled: props.busy,
                    onclick: move |_| cancel_confirm_open.set(true),
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

        if *cancel_confirm_open.read() {
            ConfirmModal {
                title: i18n::tr("mostro.confirm_cancel_title"),
                message: "Are you sure you want to cancel this trade? The counterparty will be notified.".to_string(),
                confirm_text: Some("Yes, Cancel".to_string()),
                cancel_text: Some("Keep Trade".to_string()),
                on_confirm: move |_| {
                    cancel_confirm_open.set(false);
                    (props.on_action)(TradeAction::Cancel);
                },
                on_cancel: move |_| cancel_confirm_open.set(false),
            }
        }

        // Phase 2.4b (U3): Release confirmation. Releasing sats settles the
        // escrow hold invoice and triggers the buyer's payout — the action
        // is irreversible once the daemon processes it. Cancel and Dispute
        // already had confirmation modals; Release was firing directly on
        // click, which made accidental releases far too easy.
        if *release_confirm_open.read() {
            ConfirmModal {
                title: i18n::tr("mostro.confirm_release_title"),
                message: "Releasing settles the Lightning escrow and pays out the buyer. \
                    This action is irreversible — only release after you have confirmed \
                    receipt of the fiat payment."
                        .to_string(),
                confirm_text: Some("Release".to_string()),
                cancel_text: Some("Don't Release".to_string()),
                on_confirm: move |_| {
                    release_confirm_open.set(false);
                    (props.on_action)(TradeAction::Release);
                },
                on_cancel: move |_| release_confirm_open.set(false),
            }
        }
    }
}

fn format_deadline_countdown(secs: i64) -> String {
    if secs <= 0 {
        return "expired".to_string();
    }
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    if days > 0 {
        format!("{days}d {hours}h")
    } else {
        let minutes = (secs % 3600) / 60;
        format!("{hours}h {minutes}m")
    }
}
