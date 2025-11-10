use dioxus::prelude::*;
use crate::stores::cashu_wallet;

#[component]
pub fn CashuSendLightningModal(
    on_close: EventHandler<()>,
) -> Element {
    let mut invoice = use_signal(|| String::new());
    let mints = cashu_wallet::get_mints();
    let mut selected_mint = use_signal(|| mints.first().cloned().unwrap_or_default());
    let mut is_creating_quote = use_signal(|| false);
    let mut is_paying = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut quote_info = use_signal(|| Option::<cashu_wallet::MeltQuoteInfo>::None);
    let mut payment_result = use_signal(|| Option::<(bool, Option<String>, u64)>::None);

    // Keep selected_mint in sync with available mints
    use_effect(move || {
        let current_mints = cashu_wallet::get_mints();
        let current_selection = selected_mint.read().clone();

        if current_selection.is_empty() {
            if let Some(first_mint) = current_mints.first() {
                selected_mint.set(first_mint.clone());
            }
        } else if !current_mints.contains(&current_selection) {
            if let Some(first_mint) = current_mints.first() {
                selected_mint.set(first_mint.clone());
            } else {
                selected_mint.set(String::new());
            }
        }
    });

    let on_create_quote = move |_| {
        let invoice_str = invoice.read().clone().trim().to_string();
        let mint = selected_mint.read().clone();

        if invoice_str.is_empty() {
            error_message.set(Some("Please enter a lightning invoice".to_string()));
            return;
        }

        if !invoice_str.to_lowercase().starts_with("lnbc") && !invoice_str.to_lowercase().starts_with("lntb") {
            error_message.set(Some("Invalid lightning invoice format".to_string()));
            return;
        }

        if mint.is_empty() {
            error_message.set(Some("Please select a mint".to_string()));
            return;
        }

        is_creating_quote.set(true);
        error_message.set(None);
        payment_result.set(None);

        spawn(async move {
            match cashu_wallet::create_melt_quote(mint, invoice_str).await {
                Ok(quote) => {
                    quote_info.set(Some(quote));
                    is_creating_quote.set(false);
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to create quote: {}", e)));
                    is_creating_quote.set(false);
                }
            }
        });
    };

    let on_pay = move |_| {
        if let Some(quote) = quote_info.read().as_ref() {
            let quote_id = quote.quote_id.clone();
            let mint = quote.mint_url.clone();

            is_paying.set(true);
            error_message.set(None);

            spawn(async move {
                match cashu_wallet::melt_tokens(mint, quote_id).await {
                    Ok((paid, preimage, fee)) => {
                        payment_result.set(Some((paid, preimage, fee)));
                        is_paying.set(false);

                        if paid {
                            // Auto-close after 3 seconds
                            spawn(async move {
                                gloo_timers::future::TimeoutFuture::new(3000).await;
                                on_close.call(());
                            });
                        }
                    }
                    Err(e) => {
                        error_message.set(Some(format!("Payment failed: {}", e)));
                        is_paying.set(false);
                    }
                }
            });
        }
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
                        class: "text-xl font-bold flex items-center gap-2",
                        span { "⚡" }
                        "Send Lightning"
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

                    // Payment result
                    if let Some((paid, preimage, fee)) = payment_result.read().as_ref() {
                        if *paid {
                            // Success case: payment was settled
                            div {
                                class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-4 space-y-2",
                                div {
                                    class: "flex items-start gap-3",
                                    div { class: "text-2xl", "✅" }
                                    div {
                                        p {
                                            class: "text-sm font-semibold text-green-800 dark:text-green-200",
                                            "Payment successful!"
                                        }
                                        if let Some(pre) = preimage {
                                            p {
                                                class: "text-xs text-green-700 dark:text-green-300 mt-1 font-mono break-all",
                                                "Preimage: {pre}"
                                            }
                                        }
                                        p {
                                            class: "text-xs text-green-700 dark:text-green-300 mt-1",
                                            "Fee paid: {fee} sats"
                                        }
                                    }
                                }
                            }
                        } else {
                            // Unpaid/pending case: payment not settled
                            div {
                                class: "bg-yellow-50 dark:bg-yellow-950/20 border border-yellow-200 dark:border-yellow-800 rounded-lg p-4 space-y-2",
                                div {
                                    class: "flex items-start gap-3",
                                    div { class: "text-2xl", "⏳" }
                                    div {
                                        p {
                                            class: "text-sm font-semibold text-yellow-800 dark:text-yellow-200",
                                            "Payment pending or unpaid"
                                        }
                                        p {
                                            class: "text-xs text-yellow-700 dark:text-yellow-300 mt-1",
                                            "The payment has not been settled yet. Please check the status or try again."
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
                                div { class: "text-2xl", "⚠️" }
                                div {
                                    p {
                                        class: "text-sm text-red-800 dark:text-red-200",
                                        "{msg}"
                                    }
                                }
                            }
                        }
                    }

                    // Quote display
                    if let Some(quote) = quote_info.read().as_ref() {
                        div {
                            class: "bg-accent/50 rounded-lg p-4 space-y-2",
                            h4 {
                                class: "text-sm font-semibold mb-2",
                                "Payment Details"
                            }
                            div {
                                class: "space-y-2 text-sm",
                                div {
                                    class: "flex justify-between",
                                    span { class: "text-muted-foreground", "Amount:" }
                                    span { class: "font-mono font-semibold", "{quote.amount} sats" }
                                }
                                div {
                                    class: "flex justify-between",
                                    span { class: "text-muted-foreground", "Fee reserve:" }
                                    span { class: "font-mono", "{quote.fee_reserve} sats" }
                                }
                                div {
                                    class: "flex justify-between border-t border-border pt-2",
                                    span { class: "font-semibold", "Total:" }
                                    span { class: "font-mono font-semibold", "{quote.amount + quote.fee_reserve} sats" }
                                }
                            }
                        }
                    }

                    // Invoice input (before quote created)
                    if quote_info.read().is_none() && payment_result.read().is_none() {
                        div {
                            label {
                                class: "block text-sm font-semibold mb-2",
                                "Lightning Invoice"
                            }
                            textarea {
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg font-mono text-sm min-h-[100px]",
                                placeholder: "lnbc...",
                                value: invoice.read().clone(),
                                oninput: move |evt| invoice.set(evt.value())
                            }
                        }

                        // Mint selection
                        if !mints.is_empty() {
                            div {
                                label {
                                    class: "block text-sm font-semibold mb-2",
                                    "Pay from Mint"
                                }
                                select {
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg",
                                    value: selected_mint.read().clone(),
                                    onchange: move |evt| selected_mint.set(evt.value()),
                                    for mint_url in mints.iter() {
                                        option {
                                            value: mint_url.clone(),
                                            "{shorten_url(mint_url)}"
                                        }
                                    }
                                }
                            }
                        }

                        // Wallet balance info
                        div {
                            class: "text-sm text-muted-foreground",
                            "Available balance: ",
                            span { class: "font-mono font-semibold", "{*cashu_wallet::WALLET_BALANCE.read()} sats" }
                        }
                    }
                }

                // Footer
                div {
                    class: "px-6 py-4 border-t border-border flex gap-3",
                    button {
                        class: "flex-1 px-4 py-3 bg-accent hover:bg-accent/80 rounded-lg transition",
                        onclick: move |_| {
                            quote_info.set(None);
                            payment_result.set(None);
                            error_message.set(None);
                            on_close.call(());
                        },
                        if payment_result.read().is_some() { "Close" } else { "Cancel" }
                    }

                    // Show appropriate action button
                    if payment_result.read().is_none() {
                        if quote_info.read().is_some() {
                            // Pay button (when quote is created)
                            button {
                                class: if *is_paying.read() {
                                    "flex-1 px-4 py-3 bg-orange-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                                } else {
                                    "flex-1 px-4 py-3 bg-orange-500 hover:bg-orange-600 text-white font-semibold rounded-lg transition"
                                },
                                disabled: *is_paying.read(),
                                onclick: on_pay,
                                if *is_paying.read() {
                                    "Paying..."
                                } else {
                                    "Pay Invoice"
                                }
                            }
                        } else {
                            // Create quote button (when no quote yet)
                            button {
                                class: if *is_creating_quote.read() || invoice.read().is_empty() {
                                    "flex-1 px-4 py-3 bg-blue-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                                } else {
                                    "flex-1 px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition"
                                },
                                disabled: *is_creating_quote.read() || invoice.read().is_empty(),
                                onclick: on_create_quote,
                                if *is_creating_quote.read() {
                                    "Creating Quote..."
                                } else {
                                    "Continue"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Shorten URL for display
fn shorten_url(url: &str) -> String {
    let url = url.trim_start_matches("https://").trim_start_matches("http://");
    if url.len() > 35 {
        format!("{}...", &url[..32])
    } else {
        url.to_string()
    }
}
