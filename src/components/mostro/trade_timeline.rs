use crate::stores::mostro::trade_store::{Trade, TradeStatus};
use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StepState {
    Done,
    Active,
    Pending,
}

struct Step {
    label: String,
    state: StepState,
}

fn buyer_steps(status: TradeStatus) -> Vec<Step> {
    let all: &[(&str, TradeStatus)] = &[
        ("Pending", TradeStatus::Pending),
        ("Submit Invoice", TradeStatus::WaitingBuyerInvoice),
        ("Seller Paying", TradeStatus::WaitingSellerToPay),
        ("In Progress", TradeStatus::Active),
        ("Fiat Sent", TradeStatus::FiatSent),
        ("Settling", TradeStatus::Settled),
        ("Completed", TradeStatus::Success),
    ];
    build_steps(all, status)
}

fn seller_steps(status: TradeStatus) -> Vec<Step> {
    let all: &[(&str, TradeStatus)] = &[
        ("Pending", TradeStatus::Pending),
        ("Pay Invoice", TradeStatus::WaitingSellerToPay),
        ("In Progress", TradeStatus::Active),
        ("Fiat Received", TradeStatus::FiatSent),
        ("Release Sats", TradeStatus::Settled),
        ("Completed", TradeStatus::Success),
    ];
    build_steps(all, status)
}

fn build_steps(all: &[(&str, TradeStatus)], current: TradeStatus) -> Vec<Step> {
    let order: &[TradeStatus] = &[
        TradeStatus::Pending,
        TradeStatus::WaitingBuyerInvoice,
        TradeStatus::WaitingSellerToPay,
        TradeStatus::Active,
        TradeStatus::FiatSent,
        TradeStatus::Settled,
        TradeStatus::Success,
    ];
    let current_idx = order.iter().position(|&s| s == current).unwrap_or(0);

    all.iter()
        .map(|(label, step_status)| {
            let step_idx = order.iter().position(|&s| s == *step_status).unwrap_or(0);
            let state = if current_idx > step_idx {
                StepState::Done
            } else if current_idx == step_idx {
                StepState::Active
            } else {
                StepState::Pending
            };
            Step { label: label.to_string(), state }
        })
        .collect()
}

/// Phase 2.4e (U12): distinguish "off-track" (paused / interrupted) from
/// truly terminal statuses. The previous local `is_terminal` painted every
/// off-track state with a red dot, which visually contradicted the action
/// panel still offering cancel/release/dispute buttons for `Dispute` and
/// `CancelPending`. Now we paint amber for off-track (paused) and red for
/// truly terminal (canceled/expired/admin-canceled).
fn is_off_track(status: TradeStatus) -> bool {
    matches!(
        status,
        TradeStatus::Canceled
            | TradeStatus::CooperativelyCanceled
            | TradeStatus::CanceledByAdmin
            | TradeStatus::Expired
            | TradeStatus::CancelPending
            | TradeStatus::Dispute
    )
}

/// Phase 2.4e (U12): truly terminal statuses — no further user action is
/// possible. Painted with a red dot. Matches `TradeStatus::is_terminal()`
/// plus `CooperativelyCanceled` (which `TradeStatus::is_terminal()` also
/// includes for cleanup/lifecycle purposes).
fn is_truly_terminal(status: TradeStatus) -> bool {
    matches!(
        status,
        TradeStatus::Canceled
            | TradeStatus::CooperativelyCanceled
            | TradeStatus::CanceledByAdmin
            | TradeStatus::Expired
    )
}

#[component]
pub fn TradeTimeline(trade: Trade) -> Element {
    let steps = if trade.is_buyer() {
        buyer_steps(trade.status)
    } else {
        seller_steps(trade.status)
    };

    let off_track = is_off_track(trade.status);

    rsx! {
        div { class: "p-4 bg-card border border-border rounded-lg",
            h3 { class: "text-sm font-semibold mb-3", "Progress" }
            div { class: "space-y-0",
                for (i, step) in steps.iter().enumerate() {
                    {render_step(step, i, steps.len())}
                }
                if off_track {
                    {render_off_track_badge(&trade.status)}
                }
            }
        }
    }
}

fn render_step(step: &Step, index: usize, total: usize) -> Element {
    let is_last = index == total - 1;
    let (dot_class, text_class) = match step.state {
        StepState::Done => (
            "w-3 h-3 rounded-full bg-green-500 shrink-0",
            "text-sm text-foreground",
        ),
        StepState::Active => (
            "w-3 h-3 rounded-full bg-blue-500 ring-2 ring-blue-500/30 shrink-0",
            "text-sm text-foreground font-medium",
        ),
        StepState::Pending => (
            "w-3 h-3 rounded-full bg-muted shrink-0",
            "text-sm text-muted-foreground",
        ),
    };

    rsx! {
        div { class: "flex items-start gap-3",
            div { class: "flex flex-col items-center",
                div { class: "{dot_class}" }
                if !is_last {
                    div { class: "w-px h-6 bg-border" }
                }
            }
            span { class: "{text_class} -mt-0.5", "{step.label}" }
        }
    }
}

/// Phase 2.4e (U12): render an off-track badge for paused/canceled/disputed
/// statuses. Truly terminal statuses (Canceled, Expired, CanceledByAdmin,
/// CooperativelyCanceled) get a RED dot — no further action is possible.
/// Paused statuses (CancelPending, Dispute) get an AMBER dot — the trade
/// is interrupted but the user can still act (accept cancel, dispute,
/// release, etc.). Previously all off-track states used red, which
/// visually contradicted the action panel's still-active buttons.
fn render_off_track_badge(status: &TradeStatus) -> Element {
    let (label, color, dot_class) = if is_truly_terminal(*status) {
        match status {
            TradeStatus::Canceled => (
                "Canceled".to_string(),
                "text-gray-500",
                "w-3 h-3 rounded-full bg-red-500 shrink-0",
            ),
            TradeStatus::CooperativelyCanceled => (
                "Mutual Cancel".to_string(),
                "text-gray-500",
                "w-3 h-3 rounded-full bg-red-500 shrink-0",
            ),
            TradeStatus::CanceledByAdmin => (
                "Admin Canceled".to_string(),
                "text-red-500",
                "w-3 h-3 rounded-full bg-red-500 shrink-0",
            ),
            TradeStatus::Expired => (
                "Expired".to_string(),
                "text-red-500",
                "w-3 h-3 rounded-full bg-red-500 shrink-0",
            ),
            _ => (String::new(), "", ""),
        }
    } else {
        match status {
            TradeStatus::CancelPending => (
                "Cancel Pending".to_string(),
                "text-amber-500",
                // Amber dot signals "paused, action still possible".
                "w-3 h-3 rounded-full bg-amber-500 shrink-0",
            ),
            TradeStatus::Dispute => (
                "In Dispute".to_string(),
                "text-red-600",
                // Amber dot signals "interrupted but not final" —
                // the user can still cancel/release/continue.
                "w-3 h-3 rounded-full bg-amber-500 shrink-0",
            ),
            _ => (String::new(), "", ""),
        }
    };
    if label.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "flex items-start gap-3",
            div { class: "flex flex-col items-center",
                div { class: "{dot_class}" }
            }
            span { class: "text-sm font-medium {color}", "{label}" }
        }
    }
}
