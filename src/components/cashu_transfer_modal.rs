use dioxus::prelude::*;
use crate::stores::cashu_wallet::{self, TransferProgress, TRANSFER_PROGRESS};
use crate::utils::shorten_url;

#[component]
pub fn CashuTransferModal(
    on_close: EventHandler<()>,
) -> Element {
    let mut amount = use_signal(|| String::new());
    let mints = cashu_wallet::get_mints();
    let mut source_mint = use_signal(|| mints.first().cloned().unwrap_or_default());
    let mut target_mint = use_signal(|| mints.get(1).cloned().unwrap_or_else(|| mints.first().cloned().unwrap_or_default()));
    let mut is_transferring = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut fee_estimate = use_signal(|| Option::<u64>::None);
    let mut transfer_result = use_signal(|| Option::<cashu_wallet::TransferResult>::None);
    let mut is_estimating = use_signal(|| false);

    // Read transfer progress for UI updates
    let progress = TRANSFER_PROGRESS.read().clone();

    // Keep mints in sync
    use_effect(move || {
        let current_mints = cashu_wallet::get_mints();
        let source = source_mint.read().clone();
        let target = target_mint.read().clone();

        if source.is_empty() {
            if let Some(first) = current_mints.first() {
                source_mint.set(first.clone());
            }
        } else if !current_mints.contains(&source) {
            if let Some(first) = current_mints.first() {
                source_mint.set(first.clone());
            }
        }

        if target.is_empty() || target == source {
            if let Some(second) = current_mints.get(1) {
                target_mint.set(second.clone());
            } else if let Some(first) = current_mints.first() {
                if first != &source {
                    target_mint.set(first.clone());
                }
            }
        } else if !current_mints.contains(&target) {
            for mint in &current_mints {
                if mint != &source {
                    target_mint.set(mint.clone());
                    break;
                }
            }
        }
    });

    // Get source balance for display
    let source_balance = cashu_wallet::get_mint_balance(&source_mint.read());

    // Estimate fees when amount or mints change
    let on_estimate_fees = move |_| {
        if *is_estimating.read() || *is_transferring.read() {
            return;
        }

        let amount_str = amount.read().clone();
        let source = source_mint.read().clone();
        let target = target_mint.read().clone();

        let amount_sats = match amount_str.parse::<u64>() {
            Ok(a) if a > 0 => a,
            _ => {
                fee_estimate.set(None);
                return;
            }
        };

        if source.is_empty() || target.is_empty() || source == target {
            fee_estimate.set(None);
            return;
        }

        is_estimating.set(true);
        error_message.set(None);

        spawn(async move {
            match cashu_wallet::estimate_transfer_fees(source, target, amount_sats).await {
                Ok((fee, _)) => {
                    fee_estimate.set(Some(fee));
                    is_estimating.set(false);
                }
                Err(e) => {
                    error_message.set(Some(format!("Fee estimation failed: {}", e)));
                    fee_estimate.set(None);
                    is_estimating.set(false);
                }
            }
        });
    };

    let on_transfer = move |_| {
        if *is_transferring.read() {
            return;
        }

        let amount_str = amount.read().clone();
        let source = source_mint.read().clone();
        let target = target_mint.read().clone();

        let amount_sats = match amount_str.parse::<u64>() {
            Ok(a) if a > 0 => a,
            _ => {
                error_message.set(Some("Please enter a valid amount".to_string()));
                return;
            }
        };

        if source.is_empty() {
            error_message.set(Some("Please select a source mint".to_string()));
            return;
        }

        if target.is_empty() {
            error_message.set(Some("Please select a target mint".to_string()));
            return;
        }

        if source == target {
            error_message.set(Some("Source and target mints must be different".to_string()));
            return;
        }

        // Check balance (including estimated fee)
        let balance = cashu_wallet::get_mint_balance(&source);
        let fee = fee_estimate.read().unwrap_or(0);
        let required = amount_sats.saturating_add(fee);
        if balance < required {
            error_message.set(Some(format!(
                "Insufficient balance. Have: {} sats, need: {} sats (incl. ~{} fee)",
                balance, required, fee
            )));
            return;
        }

        is_transferring.set(true);
        error_message.set(None);
        transfer_result.set(None);

        spawn(async move {
            match cashu_wallet::transfer_between_mints(source, target, amount_sats).await {
                Ok(result) => {
                    transfer_result.set(Some(result));
                    is_transferring.set(false);
                    amount.set(String::new());
                    fee_estimate.set(None);
                }
                Err(e) => {
                    error_message.set(Some(e));
                    is_transferring.set(false);
                }
            }
        });
    };

    // Swap source and target mints
    let on_swap_mints = move |_| {
        let source = source_mint.read().clone();
        let target = target_mint.read().clone();
        source_mint.set(target);
        target_mint.set(source);
        fee_estimate.set(None);
    };

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-card border border-border rounded-lg max-w-md w-full shadow-xl",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "px-6 py-4 border-b border-border flex items-center justify-between",
                    h3 {
                        class: "text-xl font-bold",
                        "Transfer Between Mints"
                    }
                    button {
                        class: "text-2xl text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }

                // Body
                div {
                    class: "p-6 space-y-4",

                    // Source mint selection
                    if mints.len() >= 2 {
                        div {
                            label {
                                class: "block text-sm font-semibold mb-2",
                                "From Mint"
                            }
                            select {
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg",
                                value: source_mint.read().clone(),
                                onchange: move |evt| {
                                    source_mint.set(evt.value());
                                    fee_estimate.set(None);
                                },
                                for mint_url in mints.iter() {
                                    option {
                                        value: mint_url.clone(),
                                        "{shorten_url(mint_url, 30)} ({cashu_wallet::get_mint_balance(mint_url)} sats)"
                                    }
                                }
                            }
                            p {
                                class: "text-xs text-muted-foreground mt-1",
                                "Balance: {source_balance} sats"
                            }
                        }

                        // Swap button
                        div {
                            class: "flex justify-center",
                            button {
                                class: "p-2 text-muted-foreground hover:text-foreground transition rounded-full hover:bg-muted",
                                onclick: on_swap_mints,
                                title: "Swap source and target",
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "w-6 h-6",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4"
                                    }
                                }
                            }
                        }

                        // Target mint selection
                        div {
                            label {
                                class: "block text-sm font-semibold mb-2",
                                "To Mint"
                            }
                            select {
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg",
                                value: target_mint.read().clone(),
                                onchange: move |evt| {
                                    target_mint.set(evt.value());
                                    fee_estimate.set(None);
                                },
                                for mint_url in mints.iter() {
                                    if mint_url != &*source_mint.read() {
                                        option {
                                            value: mint_url.clone(),
                                            "{shorten_url(mint_url, 30)} ({cashu_wallet::get_mint_balance(mint_url)} sats)"
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // Need at least 2 mints
                        div {
                            class: "bg-yellow-50 dark:bg-yellow-950/20 border border-yellow-200 dark:border-yellow-800 rounded-lg p-4",
                            p {
                                class: "text-sm text-yellow-800 dark:text-yellow-200",
                                "You need at least 2 mints to transfer between them. Add another mint first."
                            }
                        }
                    }

                    // Amount input
                    if mints.len() >= 2 {
                        div {
                            label {
                                class: "block text-sm font-semibold mb-2",
                                "Amount (sats)"
                            }
                            div {
                                class: "flex gap-2",
                                input {
                                    class: "flex-1 px-4 py-3 bg-background border border-border rounded-lg text-lg",
                                    r#type: "number",
                                    placeholder: "0",
                                    min: "1",
                                    value: amount.read().clone(),
                                    oninput: move |evt| {
                                        amount.set(evt.value());
                                        fee_estimate.set(None);
                                    }
                                }
                                button {
                                    class: "px-4 py-3 bg-muted hover:bg-muted/80 rounded-lg text-sm font-medium transition",
                                    disabled: *is_estimating.read() || *is_transferring.read(),
                                    onclick: on_estimate_fees,
                                    if *is_estimating.read() {
                                        "..."
                                    } else {
                                        "Est. Fee"
                                    }
                                }
                            }
                        }

                        // Fee estimate display
                        if let Some(fee) = *fee_estimate.read() {
                            div {
                                class: "bg-blue-50 dark:bg-blue-950/20 border border-blue-200 dark:border-blue-800 rounded-lg p-3",
                                p {
                                    class: "text-sm text-blue-800 dark:text-blue-200",
                                    "Estimated fee: {fee} sats"
                                }
                                if let Ok(amt) = amount.read().parse::<u64>() {
                                    p {
                                        class: "text-xs text-blue-600 dark:text-blue-400 mt-1",
                                        "Total cost: {amt + fee} sats"
                                    }
                                }
                            }
                        }
                    }

                    // Transfer progress
                    if *is_transferring.read() {
                        div {
                            class: "bg-muted rounded-lg p-4",
                            div {
                                class: "flex items-center gap-3",
                                div {
                                    class: "animate-spin text-2xl",
                                    "⟳"
                                }
                                div {
                                    class: "flex-1",
                                    p {
                                        class: "text-sm font-medium",
                                        match &progress {
                                            Some(TransferProgress::CreatingMintQuote) => "Creating invoice at target mint...",
                                            Some(TransferProgress::CreatingMeltQuote) => "Creating payment quote at source...",
                                            Some(TransferProgress::QuotesReady { .. }) => "Quotes ready, preparing transfer...",
                                            Some(TransferProgress::Melting) => "Paying Lightning invoice...",
                                            Some(TransferProgress::WaitingForPayment) => "Waiting for payment confirmation...",
                                            Some(TransferProgress::Minting) => "Minting tokens at target...",
                                            Some(TransferProgress::Completed { .. }) => "Transfer complete!",
                                            Some(TransferProgress::Failed { .. }) => "Transfer failed",
                                            None => "Processing...",
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Error message
                    if let Some(msg) = error_message.read().as_ref() {
                        div {
                            class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-4",
                            div {
                                class: "flex items-start gap-3",
                                div {
                                    class: "text-2xl",
                                    "⚠️"
                                }
                                div {
                                    p {
                                        class: "text-sm text-red-800 dark:text-red-200",
                                        "{msg}"
                                    }
                                }
                            }
                        }
                    }

                    // Success result
                    if let Some(result) = transfer_result.read().as_ref() {
                        div {
                            class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-4",
                            div {
                                class: "flex items-start gap-3",
                                div {
                                    class: "text-2xl",
                                    "✅"
                                }
                                div {
                                    class: "flex-1",
                                    p {
                                        class: "text-sm font-semibold text-green-800 dark:text-green-200",
                                        "Transfer complete!"
                                    }
                                    div {
                                        class: "text-xs text-green-700 dark:text-green-300 mt-2 space-y-1",
                                        p { "Sent: {result.amount_sent} sats" }
                                        p { "Received: {result.amount_received} sats" }
                                        p { "Fees: {result.fees_paid} sats" }
                                    }
                                }
                            }
                        }
                    }
                }

                // Footer
                if mints.len() >= 2 {
                    div {
                        class: "px-6 py-4 border-t border-border flex gap-3",
                        button {
                            class: "flex-1 px-4 py-3 bg-muted hover:bg-muted/80 rounded-lg font-medium transition",
                            onclick: move |_| on_close.call(()),
                            "Cancel"
                        }
                        button {
                            class: "flex-1 px-4 py-3 bg-primary text-primary-foreground rounded-lg font-medium transition hover:opacity-90 disabled:opacity-50",
                            disabled: *is_transferring.read() || amount.read().is_empty(),
                            onclick: on_transfer,
                            if *is_transferring.read() {
                                "Transferring..."
                            } else {
                                "Transfer"
                            }
                        }
                    }
                } else {
                    div {
                        class: "px-6 py-4 border-t border-border",
                        button {
                            class: "w-full px-4 py-3 bg-muted hover:bg-muted/80 rounded-lg font-medium transition",
                            onclick: move |_| on_close.call(()),
                            "Close"
                        }
                    }
                }
            }
        }
    }
}
