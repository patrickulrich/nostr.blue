use crate::stores::social::mostro::trade_store::{Trade, TradeStatus};
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

fn is_terminal(status: TradeStatus) -> bool {
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

#[component]
pub fn TradeTimeline(trade: Trade) -> Element {
    let steps = if trade.is_buyer() {
        buyer_steps(trade.status)
    } else {
        seller_steps(trade.status)
    };

    let terminal = is_terminal(trade.status);

    rsx! {
        div { class: "p-4 bg-card border border-border rounded-lg",
            h3 { class: "text-sm font-semibold mb-3", "Progress" }
            div { class: "space-y-0",
                for (i, step) in steps.iter().enumerate() {
                    {render_step(step, i, steps.len())}
                }
                if terminal {
                    {render_terminal(&trade.status)}
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

fn render_terminal(status: &TradeStatus) -> Element {
    let (label, color) = match status {
        TradeStatus::Canceled => ("Canceled".to_string(), "text-gray-500"),
        TradeStatus::CooperativelyCanceled => {
            ("Mutual Cancel".to_string(), "text-gray-500")
        }
        TradeStatus::CanceledByAdmin => ("Admin Canceled".to_string(), "text-red-500"),
        TradeStatus::CancelPending => ("Cancel Pending".to_string(), "text-amber-500"),
        TradeStatus::Expired => ("Expired".to_string(), "text-red-500"),
        TradeStatus::Dispute => ("In Dispute".to_string(), "text-red-600"),
        _ => (String::new(), ""),
    };
    if label.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "flex items-start gap-3",
            div { class: "flex flex-col items-center",
                div { class: "w-3 h-3 rounded-full bg-red-500 shrink-0" }
            }
            span { class: "text-sm font-medium {color}", "{label}" }
        }
    }
}
