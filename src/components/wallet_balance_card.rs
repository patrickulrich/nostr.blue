use dioxus::prelude::*;
use crate::stores::cashu_wallet;

#[component]
pub fn WalletBalanceCard(
    on_send: EventHandler<()>,
    on_receive: EventHandler<()>,
) -> Element {
    let balance = cashu_wallet::WALLET_BALANCE.read();
    let mint_count = cashu_wallet::get_mint_count();
    let mints = cashu_wallet::get_mints();

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

            // Mints info
            if mint_count > 0 {
                div {
                    class: "mb-6 opacity-90",
                    div {
                        class: "text-sm mb-2",
                        "Active Mints: {mint_count}"
                    }
                    div {
                        class: "flex flex-col gap-1",
                        for mint_url in mints.iter().take(3) {
                            div {
                                class: "text-xs opacity-75 truncate",
                                title: "{mint_url}",
                                "{shorten_url(mint_url)}"
                            }
                        }
                        if mint_count > 3 {
                            div {
                                class: "text-xs opacity-75",
                                "+{mint_count - 3} more"
                            }
                        }
                    }
                }
            }

            // Action buttons
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

            // Phase 2 notice
            div {
                class: "mt-4 text-xs opacity-75 text-center",
                "Phase 1: Display only • Mint operations coming in Phase 2"
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

/// Shorten URL for display
fn shorten_url(url: &str) -> String {
    let url = url.trim_start_matches("https://").trim_start_matches("http://");
    if url.len() > 30 {
        format!("{}...", &url[..27])
    } else {
        url.to_string()
    }
}
