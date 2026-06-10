use crate::stores::social::mostro::trade_store::TradeStatus;
use dioxus::prelude::*;

#[component]
pub fn TradeStatusBadge(status: TradeStatus) -> Element {
    let (class, label) = match status {
        TradeStatus::Pending => (
            "bg-gray-500/20 text-gray-600 dark:text-gray-400",
            "Pending",
        ),
        TradeStatus::WaitingBuyerInvoice => (
            "bg-yellow-500/20 text-yellow-600 dark:text-yellow-400",
            "Waiting Invoice",
        ),
        TradeStatus::WaitingSellerToPay => (
            "bg-orange-500/20 text-orange-600 dark:text-orange-400",
            "Awaiting Payment",
        ),
        TradeStatus::WaitingBond => (
            "bg-amber-500/20 text-amber-600 dark:text-amber-400",
            "Waiting Bond",
        ),
        TradeStatus::WaitingTakerBond => (
            "bg-amber-500/20 text-amber-600 dark:text-amber-400",
            "Bond Required",
        ),
        TradeStatus::Active => (
            "bg-blue-500/20 text-blue-600 dark:text-blue-400",
            "In Progress",
        ),
        TradeStatus::FiatSent => (
            "bg-cyan-500/20 text-cyan-600 dark:text-cyan-400",
            "Fiat Sent",
        ),
        TradeStatus::Settled => (
            "bg-indigo-500/20 text-indigo-600 dark:text-indigo-400",
            "Settling",
        ),
        TradeStatus::Success => (
            "bg-green-500/20 text-green-600 dark:text-green-400",
            "Completed",
        ),
        TradeStatus::Canceled => (
            "bg-gray-500/20 text-gray-600 dark:text-gray-400",
            "Canceled",
        ),
        TradeStatus::CancelPending => (
            "bg-amber-500/20 text-amber-600 dark:text-amber-400",
            "Cancel Pending",
        ),
        TradeStatus::CooperativelyCanceled => (
            "bg-gray-500/20 text-gray-600 dark:text-gray-400",
            "Mutual Cancel",
        ),
        TradeStatus::CanceledByAdmin => (
            "bg-red-500/20 text-red-600 dark:text-red-400",
            "Admin Canceled",
        ),
        TradeStatus::Expired => (
            "bg-red-500/20 text-red-600 dark:text-red-400",
            "Expired",
        ),
        TradeStatus::Dispute => (
            "bg-red-600/20 text-red-700 dark:text-red-400",
            "Dispute",
        ),
        TradeStatus::PaymentFailed => (
            "bg-purple-500/20 text-purple-600 dark:text-purple-400",
            "Payment Failed",
        ),
    };
    rsx! {
        span { class: "px-2 py-0.5 text-xs font-medium rounded-full {class}", "{label}" }
    }
}
