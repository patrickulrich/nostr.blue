use dioxus::prelude::*;
use crate::stores::cashu_wallet;

#[component]
pub fn WalletBalanceCard(
    on_send: EventHandler<()>,
    on_receive: EventHandler<()>,
    on_lightning_deposit: EventHandler<()>,
    on_lightning_withdraw: EventHandler<()>,
) -> Element {
    let balance = cashu_wallet::WALLET_BALANCE.read();

    // Format balance with thousands separator
    let formatted_balance = format_sats(*balance);

    rsx! {
        div {
            class: "bg-gradient-to-br from-blue-500 to-purple-600 rounded-xl p-6 text-white shadow-lg",

            // Balance section
            div {
                class: "mb-6",
                div {
                    class: "text-sm opacity-90 mb-2",
                    "Total Balance"
                }
                div {
                    class: "text-5xl font-bold mb-1",
                    "{formatted_balance}"
                }
                div {
                    class: "text-sm opacity-75",
                    "sats"
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
        }
    }
}

/// Format satoshi amount with thousands separator
fn format_sats(sats: u64) -> String {
    let s = sats.to_string();
    let mut result = String::new();
    let mut count = 0;

    for c in s.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
        count += 1;
    }

    result.chars().rev().collect()
}
