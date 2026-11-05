//! NUT-30 on-chain Bitcoin deposit / withdrawal modal.
//!
//! Shown only for mints that advertise an `onchain` entry in their nut04/
//! nut05 method lists (see `capabilities`). Deposit: create a quote, show
//! the address + QR, poll paid/issued, mint the confirmed difference.
//! Withdraw: fetch the mint's fee menu, pick an option, melt through the
//! standard melt flow.

use crate::stores::cashu::{self, onchain};
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
enum OnchainTab {
    Deposit,
    Withdraw,
}

#[component]
pub fn CashuOnchainModal(on_close: EventHandler<()>) -> Element {
    let mints = cashu::get_mints();
    let mut tab = use_signal(|| OnchainTab::Deposit);
    let mut selected_mint = use_signal(|| mints.first().cloned().unwrap_or_default());
    let mut busy = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);

    // Deposit state
    let mut deposit_amount = use_signal(String::new);
    let mut deposit_quote = use_signal(|| Option::<onchain::OnchainDepositQuote>::None);
    let mut deposit_status = use_signal(|| Option::<onchain::OnchainDepositStatus>::None);

    // Withdraw state
    let mut withdraw_address = use_signal(String::new);
    let mut withdraw_amount = use_signal(String::new);
    let mut fee_options = use_signal(Vec::<onchain::OnchainFeeOption>::new);
    let mut selected_fee = use_signal(|| Option::<usize>::None);
    let mut withdraw_result = use_signal(|| Option::<(bool, Option<String>, u64)>::None);

    let deposit_qr_svg = use_memo(move || {
        deposit_quote()
            .map(|q| {
                crate::components::animated_qr::generate_qr_svg(&format!("bitcoin:{}", q.address))
            })
            .unwrap_or_default()
    });

    let create_deposit = move |_| {
        let mint = selected_mint.read().clone();
        if mint.is_empty() {
            error_message.set(Some("Please select a mint".to_string()));
            return;
        }
        let amount = deposit_amount.read().trim().parse::<u64>().ok();
        busy.set(true);
        error_message.set(None);
        deposit_quote.set(None);
        deposit_status.set(None);
        spawn(async move {
            match onchain::create_onchain_deposit_quote(&mint, amount).await {
                Ok(q) => deposit_quote.set(Some(q)),
                Err(e) => error_message.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let refresh_deposit = move |_| {
        let Some(q) = deposit_quote.read().clone() else {
            return;
        };
        busy.set(true);
        spawn(async move {
            match onchain::check_onchain_deposit(&q.mint_url, &q.quote_id).await {
                Ok(s) => deposit_status.set(Some(s)),
                Err(e) => error_message.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let mint_deposit = move |_| {
        let Some(q) = deposit_quote.read().clone() else {
            return;
        };
        busy.set(true);
        error_message.set(None);
        spawn(async move {
            match onchain::mint_onchain_deposit(&q.mint_url, &q.quote_id, None).await {
                Ok(sats) => {
                    deposit_status.set(None);
                    deposit_quote.set(None);
                    deposit_amount.set(String::new());
                    error_message.set(None);
                    log::info!("Minted {sats} sats from on-chain deposit");
                }
                Err(e) => error_message.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let fetch_fees = move |_| {
        let mint = selected_mint.read().clone();
        let address = withdraw_address.read().trim().to_string();
        let amount: u64 = match withdraw_amount.read().trim().parse() {
            Ok(a) if a > 0 => a,
            _ => {
                error_message.set(Some("Enter a valid amount in sats".to_string()));
                return;
            }
        };
        busy.set(true);
        error_message.set(None);
        fee_options.set(Vec::new());
        selected_fee.set(None);
        withdraw_result.set(None);
        spawn(async move {
            match onchain::onchain_withdraw_fee_options(&mint, &address, amount).await {
                Ok(opts) => {
                    if opts.is_empty() {
                        error_message.set(Some("Mint returned no fee options".to_string()));
                    }
                    fee_options.set(opts)
                }
                Err(e) => error_message.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let do_withdraw = move |_| {
        let mint = selected_mint.read().clone();
        let address = withdraw_address.read().trim().to_string();
        let amount: u64 = withdraw_amount.read().trim().parse().unwrap_or(0);
        let idx = selected_fee.read().unwrap_or(usize::MAX);
        let Some(option) = fee_options.read().get(idx).cloned() else {
            error_message.set(Some("Select a fee option".to_string()));
            return;
        };
        busy.set(true);
        error_message.set(None);
        spawn(async move {
            match onchain::withdraw_onchain(&mint, &address, amount, &option).await {
                Ok(result) => {
                    withdraw_result.set(Some(result));
                    fee_options.set(Vec::new());
                    selected_fee.set(None);
                }
                Err(e) => error_message.set(Some(e)),
            }
            busy.set(false);
        });
    };

    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-card border border-border rounded-lg max-w-md w-full shadow-xl max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),
                div { class: "px-6 py-4 border-b border-border flex items-center justify-between",
                    h3 { class: "text-xl font-bold", "On-Chain BTC" }
                    button {
                        class: "text-2xl text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }
                div { class: "p-6 space-y-4",
                    if mints.is_empty() {
                        p { class: "text-sm text-muted-foreground", "Add a mint first." }
                    } else {
                        div {
                            label { class: "block text-sm font-semibold mb-2", "Mint" }
                            select {
                                class: "w-full px-4 py-2 bg-background border border-border rounded-lg text-sm",
                                value: selected_mint.read().clone(),
                                onchange: move |evt| selected_mint.set(evt.value()),
                                for mint in &mints {
                                    option { value: "{mint}", selected: *selected_mint.read() == *mint, "{mint}" }
                                }
                            }
                            p { class: "text-xs text-muted-foreground mt-1",
                                "Only mints advertising an on-chain method (NUT-30) support this"
                            }
                        }
                        div { class: "flex border border-border rounded-lg overflow-hidden",
                            button {
                                class: if *tab.read() == OnchainTab::Deposit { "flex-1 px-4 py-2 text-sm font-semibold bg-accent" } else { "flex-1 px-4 py-2 text-sm hover:bg-accent/50" },
                                onclick: move |_| tab.set(OnchainTab::Deposit),
                                "Deposit"
                            }
                            button {
                                class: if *tab.read() == OnchainTab::Withdraw { "flex-1 px-4 py-2 text-sm font-semibold bg-accent" } else { "flex-1 px-4 py-2 text-sm hover:bg-accent/50" },
                                onclick: move |_| tab.set(OnchainTab::Withdraw),
                                "Withdraw"
                            }
                        }
                        if *tab.read() == OnchainTab::Deposit {
                            div { class: "space-y-3",
                                div {
                                    label { class: "block text-sm font-semibold mb-1", "Amount (sats, optional)" }
                                    input {
                                        class: "w-full px-4 py-2 bg-background border border-border rounded-lg text-sm",
                                        r#type: "number",
                                        min: "0",
                                        placeholder: "variable amount (top-up)",
                                        disabled: deposit_quote.read().is_some(),
                                        value: deposit_amount.read().clone(),
                                        oninput: move |evt| deposit_amount.set(evt.value()),
                                    }
                                }
                                button {
                                    class: "w-full px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition disabled:opacity-50",
                                    disabled: *busy.read(),
                                    onclick: create_deposit,
                                    if *busy.read() { "Creating…" } else { "Get deposit address" }
                                }
                                if let Some(q) = deposit_quote.read().as_ref() {
                                    div { class: "bg-accent/40 border border-border rounded-lg p-4 space-y-3",
                                        p { class: "text-xs font-semibold text-muted-foreground", "Send BTC to:" }
                                        p { class: "font-mono text-sm break-all", "{q.address}" }
                                        div {
                                            class: "bg-white rounded-lg p-3 border border-border flex justify-center",
                                            dangerous_inner_html: "{deposit_qr_svg}",
                                        }
                                        if let Some(s) = deposit_status.read().as_ref() {
                                            div { class: "text-xs text-muted-foreground space-y-1",
                                                p { "state: {s.state}" }
                                                p { "paid: {s.amount_paid} sats · minted: {s.amount_issued} sats · available: {s.mintable_sats} sats" }
                                            }
                                        }
                                        div { class: "flex gap-2",
                                            button {
                                                class: "flex-1 px-3 py-2 border border-border rounded-lg text-sm hover:bg-accent transition",
                                                disabled: *busy.read(),
                                                onclick: refresh_deposit,
                                                "Refresh"
                                            }
                                            button {
                                                class: "flex-1 px-3 py-2 bg-green-600 hover:bg-green-700 text-white rounded-lg text-sm font-semibold transition disabled:opacity-50",
                                                disabled: *busy.read()
                                                    || deposit_status
                                                        .read()
                                                        .as_ref()
                                                        .map(|s| s.mintable_sats == 0)
                                                        .unwrap_or(true),
                                                onclick: mint_deposit,
                                                "Mint confirmed"
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            div { class: "space-y-3",
                                div {
                                    label { class: "block text-sm font-semibold mb-1", "Bitcoin address" }
                                    input {
                                        class: "w-full px-4 py-2 bg-background border border-border rounded-lg text-sm font-mono",
                                        placeholder: "bc1…",
                                        value: withdraw_address.read().clone(),
                                        oninput: move |evt| withdraw_address.set(evt.value()),
                                    }
                                }
                                div {
                                    label { class: "block text-sm font-semibold mb-1", "Amount (sats)" }
                                    input {
                                        class: "w-full px-4 py-2 bg-background border border-border rounded-lg text-sm",
                                        r#type: "number",
                                        min: "1",
                                        placeholder: "1000",
                                        value: withdraw_amount.read().clone(),
                                        oninput: move |evt| withdraw_amount.set(evt.value()),
                                    }
                                }
                                button {
                                    class: "w-full px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition disabled:opacity-50",
                                    disabled: *busy.read(),
                                    onclick: fetch_fees,
                                    if *busy.read() { "Fetching fees…" } else { "Get fee options" }
                                }
                                if !fee_options.read().is_empty() {
                                    div { class: "space-y-2",
                                        p { class: "text-xs font-semibold text-muted-foreground", "Select a fee option:" }
                                        for (i, opt) in fee_options.read().iter().enumerate() {
                                            label {
                                                class: if *selected_fee.read() == Some(i) {
                                                    "flex items-center justify-between gap-2 p-3 border border-blue-500 bg-accent rounded-lg cursor-pointer"
                                                } else {
                                                    "flex items-center justify-between gap-2 p-3 border border-border rounded-lg cursor-pointer hover:bg-accent/50"
                                                },
                                                onclick: move |_| selected_fee.set(Some(i)),
                                                div {
                                                    input {
                                                        r#type: "radio",
                                                        name: "onchain-fee",
                                                        checked: *selected_fee.read() == Some(i),
                                                        onchange: move |_| selected_fee.set(Some(i)),
                                                    }
                                                    span { class: "ml-2 text-sm",
                                                        "{opt.fee_reserve_sats} sats fee"
                                                    }
                                                }
                                                span { class: "text-xs text-muted-foreground",
                                                    "~{opt.estimated_blocks} blocks"
                                                }
                                            }
                                        }
                                        button {
                                            class: "w-full px-4 py-3 bg-green-600 hover:bg-green-700 text-white font-semibold rounded-lg transition disabled:opacity-50",
                                            disabled: *busy.read() || selected_fee.read().is_none(),
                                            onclick: do_withdraw,
                                            if *busy.read() { "Sending…" } else { "Withdraw on-chain" }
                                        }
                                    }
                                }
                                if let Some((paid, proof, amount)) = withdraw_result.read().as_ref() {
                                    div { class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-4 text-sm space-y-1",
                                        if *paid {
                                            p { class: "font-semibold text-green-700 dark:text-green-300",
                                                "Sent {amount} sats on-chain"
                                            }
                                        } else {
                                            p { class: "font-semibold text-amber-600", "Payment pending" }
                                        }
                                        if let Some(outpoint) = proof {
                                            p { class: "text-xs text-muted-foreground break-all", "outpoint: {outpoint}" }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(err) = error_message.read().as_ref() {
                            div { class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-3 text-sm text-red-600 dark:text-red-400",
                                "{err}"
                            }
                        }
                    }
                }
            }
        }
    }
}
