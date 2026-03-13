//! Wallet Health Indicator Component
//!
//! Displays a badge when there are stuck/pending proofs and opens a health modal.
use crate::stores::cashu::proof_recovery;
use dioxus::prelude::*;
/// Inline wallet health indicator shown in the wallet card.
///
/// Shows pending balance and stuck proofs badge (if any).
/// Clicking the stuck badge opens the health modal.
#[component]
pub fn WalletHealthIndicator(on_open_modal: EventHandler<()>) -> Element {
    let stats = use_memo(move || {
        let _ = crate::stores::cashu_cdk_bridge::WALLET_BALANCES.read();
        let _ = crate::stores::cashu::WALLET_TOKENS.read();
        proof_recovery::get_wallet_health_stats()
    });
    let health = stats.read().clone();
    if health.stuck_count == 0 && health.pending_count == 0 {
        return rsx! {};
    }
    rsx! {
        div { class: "flex items-center gap-2 text-sm mt-2",
            if health.pending_count > 0 {
                div { class: "flex items-center gap-1 text-yellow-300",
                    span { class: "animate-pulse", aria_hidden: "true", "..." }
                    span { "{health.pending_balance} sats pending" }
                    span { class: "sr-only", "pending balance: {health.pending_balance} sats" }
                }
            }
            if health.stuck_count > 0 {
                button {
                    class: "flex items-center gap-1 px-2 py-1 bg-red-500/20 text-red-200 rounded-md hover:bg-red-500/30 transition",
                    aria_label: "Open stuck proofs modal",
                    onclick: move |_| on_open_modal.call(()),
                    span { "!" }
                    span { "{health.stuck_count} stuck" }
                }
            }
        }
    }
}
