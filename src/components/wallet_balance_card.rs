use dioxus::prelude::*;
use crate::stores::cashu_wallet;
use crate::stores::cashu_cdk_bridge::WALLET_BALANCES;
use crate::utils::format_sats_with_separator;

#[component]
pub fn WalletBalanceCard(
    on_send: EventHandler<()>,
    on_receive: EventHandler<()>,
    on_lightning_deposit: EventHandler<()>,
    on_lightning_withdraw: EventHandler<()>,
    on_optimize: EventHandler<()>,
    on_transfer: EventHandler<()>,
    on_create_request: EventHandler<()>,
    on_pay_request: EventHandler<()>,
) -> Element {
    let balance = cashu_wallet::WALLET_BALANCE.read();
    let balances = WALLET_BALANCES.read();
    let proof_count = cashu_wallet::get_total_proof_count();
    let mint_count = cashu_wallet::get_mints().len();

    // Format balance with thousands separator
    let formatted_balance = format_sats_with_separator(*balance);

    // Check if there are pending funds
    let has_pending = balances.pending > 0;
    let formatted_pending = format_sats_with_separator(balances.pending);

    rsx! {
        div {
            class: "bg-gradient-to-br from-blue-500 to-purple-600 rounded-xl p-6 text-white shadow-lg",

            // Balance section
            div {
                class: "mb-6",
                div {
                    class: "text-sm opacity-90 mb-2",
                    if has_pending { "Available Balance" } else { "Total Balance" }
                }
                div {
                    class: "text-5xl font-bold mb-1",
                    "{formatted_balance}"
                }
                div {
                    class: "text-sm opacity-75",
                    "sats"
                }

                // Show pending balance if any
                if has_pending {
                    div {
                        class: "mt-2 text-sm opacity-75 flex items-center gap-2",
                        span {
                            class: "inline-block w-2 h-2 rounded-full bg-yellow-400 animate-pulse"
                        }
                        span { "Pending: {formatted_pending} sats" }
                    }
                }
            }

            // Action buttons row 1: Lightning
            div {
                class: "mb-3",
                div {
                    class: "text-xs opacity-75 mb-2",
                    "Lightning"
                }
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 bg-white/20 hover:bg-white/30 backdrop-blur-sm py-3 px-4 rounded-lg font-semibold transition flex items-center justify-center gap-2",
                        onclick: move |_| on_lightning_deposit.call(()),
                        span { "⚡" }
                        span { "Deposit" }
                    }
                    button {
                        class: "flex-1 bg-white/20 hover:bg-white/30 backdrop-blur-sm py-3 px-4 rounded-lg font-semibold transition flex items-center justify-center gap-2",
                        onclick: move |_| on_lightning_withdraw.call(()),
                        span { "⚡" }
                        span { "Withdraw" }
                    }
                }
            }

            // Action buttons row 2: Ecash
            div {
                div {
                    class: "text-xs opacity-75 mb-2",
                    "Ecash"
                }
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 bg-white/20 hover:bg-white/30 backdrop-blur-sm py-3 px-4 rounded-lg font-semibold transition flex items-center justify-center gap-2",
                        onclick: move |_| on_receive.call(()),
                        span { "⬇️" }
                        span { "Receive" }
                    }
                    button {
                        class: "flex-1 bg-white/20 hover:bg-white/30 backdrop-blur-sm py-3 px-4 rounded-lg font-semibold transition flex items-center justify-center gap-2",
                        onclick: move |_| on_send.call(()),
                        span { "⬆️" }
                        span { "Send" }
                    }
                }
            }

            // Action buttons row 3: Payment Requests (NUT-18)
            div {
                class: "mt-3",
                div {
                    class: "text-xs opacity-75 mb-2",
                    "Payment Requests"
                }
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 bg-white/20 hover:bg-white/30 backdrop-blur-sm py-3 px-4 rounded-lg font-semibold transition flex items-center justify-center gap-2",
                        onclick: move |_| on_create_request.call(()),
                        span { "📝" }
                        span { "Request" }
                    }
                    button {
                        class: "flex-1 bg-white/20 hover:bg-white/30 backdrop-blur-sm py-3 px-4 rounded-lg font-semibold transition flex items-center justify-center gap-2",
                        onclick: move |_| on_pay_request.call(()),
                        span { "💸" }
                        span { "Pay Request" }
                    }
                }
            }

            // Advanced actions row (Transfer between mints)
            if mint_count >= 2 {
                div {
                    class: "mt-3",
                    div {
                        class: "text-xs opacity-75 mb-2",
                        "Advanced"
                    }
                    div {
                        class: "flex gap-3",
                        button {
                            class: "flex-1 bg-white/20 hover:bg-white/30 backdrop-blur-sm py-3 px-4 rounded-lg font-semibold transition flex items-center justify-center gap-2",
                            onclick: move |_| on_transfer.call(()),
                            span { "↔️" }
                            span { "Transfer" }
                        }
                    }
                }
            }

            // Optimize wallet button (only show if there are proofs)
            if proof_count > 8 {
                div {
                    class: "mt-3 pt-3 border-t border-white/20",
                    button {
                        class: "w-full bg-white/10 hover:bg-white/20 backdrop-blur-sm py-2 px-4 rounded-lg text-sm transition flex items-center justify-center gap-2",
                        onclick: move |_| on_optimize.call(()),
                        span { "✨" }
                        span { "Optimize Wallet ({proof_count} proofs)" }
                    }
                }
            }
        }
    }
}
