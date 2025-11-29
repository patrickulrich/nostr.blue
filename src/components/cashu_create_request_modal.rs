use dioxus::prelude::*;
use crate::stores::cashu_wallet::{self, PaymentRequestProgress};

#[component]
pub fn CashuCreateRequestModal(
    on_close: EventHandler<()>,
) -> Element {
    let mut amount_input = use_signal(|| String::new());
    let mut description_input = use_signal(|| String::new());
    let mut use_nostr_transport = use_signal(|| true);
    let mut request_string = use_signal(|| Option::<String>::None);
    let mut current_request_id = use_signal(|| Option::<String>::None);
    let mut is_creating = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut copied = use_signal(|| false);
    let mut copy_error = use_signal(|| Option::<String>::None);

    let progress = cashu_wallet::PAYMENT_REQUEST_PROGRESS.read();
    let balance = *cashu_wallet::WALLET_BALANCE.read();

    // Handle creating a new request
    let handle_create = move |_| {
        let amount_str = amount_input.read().clone();
        let description = description_input.read().clone();
        let use_nostr = *use_nostr_transport.read();

        // Parse amount (optional)
        let amount: Option<u64> = if amount_str.is_empty() {
            None
        } else {
            match amount_str.parse::<u64>() {
                Ok(a) if a > 0 => Some(a),
                Ok(_) => {
                    error_message.set(Some("Amount must be greater than 0".to_string()));
                    return;
                }
                Err(_) => {
                    error_message.set(Some("Invalid amount".to_string()));
                    return;
                }
            }
        };

        let desc = if description.is_empty() { None } else { Some(description) };

        error_message.set(None);
        is_creating.set(true);

        spawn(async move {
            match cashu_wallet::create_payment_request(amount, desc, use_nostr).await {
                Ok((request, nostr_info)) => {
                    request_string.set(Some(request.clone()));

                    // If using Nostr transport, start waiting for payment
                    if let Some(info) = nostr_info {
                        current_request_id.set(Some(info.request_id.clone()));

                        // Start waiting in background
                        let request_id = info.request_id.clone();
                        spawn(async move {
                            match cashu_wallet::wait_for_nostr_payment(request_id.clone(), 300).await {
                                Ok(amount) => {
                                    log::info!("Received payment of {} sats", amount);
                                }
                                Err(e) => {
                                    // Don't log error for intentional cancellations (user closed modal)
                                    if e != "Payment request cancelled" {
                                        log::error!("Payment wait error: {}", e);
                                    }
                                }
                            }
                        });
                    }

                    is_creating.set(false);
                }
                Err(e) => {
                    error_message.set(Some(e));
                    is_creating.set(false);
                }
            }
        });
    };

    // Handle copying to clipboard
    let handle_copy = move |_| {
        if let Some(req) = request_string.read().as_ref() {
            let req_clone = req.clone();
            // Clear any previous error when attempting copy
            copy_error.set(None);
            spawn(async move {
                if let Some(window) = web_sys::window() {
                    let clipboard = window.navigator().clipboard();
                    match wasm_bindgen_futures::JsFuture::from(
                        clipboard.write_text(&req_clone)
                    ).await {
                        Ok(_) => {
                            copied.set(true);
                            // Reset copied state after 2 seconds
                            gloo_timers::future::TimeoutFuture::new(2000).await;
                            copied.set(false);
                        }
                        Err(e) => {
                            let err_msg = format!("{:?}", e);
                            log::error!("Failed to copy to clipboard: {}", err_msg);
                            copy_error.set(Some("Copy failed".to_string()));
                            // Clear error after 3 seconds
                            gloo_timers::future::TimeoutFuture::new(3000).await;
                            copy_error.set(None);
                        }
                    }
                } else {
                    log::error!("Failed to copy: window not available");
                    copy_error.set(Some("Copy failed".to_string()));
                    // Clear error after 3 seconds
                    gloo_timers::future::TimeoutFuture::new(3000).await;
                    copy_error.set(None);
                }
            });
        }
    };

    // Handle cancel/close
    let handle_close = move |_| {
        // Cancel any pending request
        if let Some(req_id) = current_request_id.read().as_ref() {
            cashu_wallet::cancel_payment_request(req_id);
        }
        on_close.call(());
    };

    // Check if payment was received
    let payment_received = matches!(&*progress, Some(PaymentRequestProgress::Received { .. }));
    let received_amount = if let Some(PaymentRequestProgress::Received { amount }) = &*progress {
        Some(*amount)
    } else {
        None
    };

    rsx! {
        // Modal backdrop
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |e| {
                e.stop_propagation();
                handle_close(());
            },

            // Modal content
            div {
                class: "bg-card border border-border rounded-xl p-6 w-full max-w-md mx-4 shadow-xl",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between mb-6",
                    h2 {
                        class: "text-xl font-bold",
                        "Create Payment Request"
                    }
                    button {
                        class: "text-muted-foreground hover:text-foreground transition p-1",
                        onclick: move |_| handle_close(()),
                        "X"
                    }
                }

                // Success state
                if payment_received {
                    div {
                        class: "text-center py-8",
                        div {
                            class: "text-6xl mb-4",
                            "🎉"
                        }
                        h3 {
                            class: "text-xl font-bold text-green-500 mb-2",
                            "Payment Received!"
                        }
                        if let Some(amount) = received_amount {
                            p {
                                class: "text-lg",
                                "{amount} sats received"
                            }
                        }
                        button {
                            class: "mt-6 w-full py-3 bg-green-500 hover:bg-green-600 text-white rounded-lg font-semibold transition",
                            onclick: move |_| handle_close(()),
                            "Done"
                        }
                    }
                } else if request_string.read().is_some() {
                    // Request created - show QR and string
                    div {
                        // Request string display
                        div {
                            class: "mb-4",
                            label {
                                class: "block text-sm font-medium mb-2",
                                "Payment Request"
                            }
                            div {
                                class: "bg-background border border-border rounded-lg p-3 break-all text-xs font-mono",
                                {request_string.read().as_ref().unwrap_or(&String::new()).clone()}
                            }
                        }

                        // Copy button
                        button {
                            class: if copy_error.read().is_some() {
                                "w-full py-3 bg-red-500/20 border border-red-500/50 text-red-500 rounded-lg font-semibold transition flex items-center justify-center gap-2 mb-4"
                            } else {
                                "w-full py-3 bg-accent hover:bg-accent/80 rounded-lg font-semibold transition flex items-center justify-center gap-2 mb-4"
                            },
                            onclick: handle_copy,
                            if let Some(err) = copy_error.read().as_ref() {
                                span { "{err}" }
                            } else if *copied.read() {
                                span { "Copied!" }
                            } else {
                                span { "Copy to Clipboard" }
                            }
                        }

                        // Status indicator
                        if *use_nostr_transport.read() {
                            div {
                                class: "text-center text-sm text-muted-foreground",
                                match &*progress {
                                    Some(PaymentRequestProgress::WaitingForPayment) => rsx! {
                                        div {
                                            class: "flex items-center justify-center gap-2",
                                            div {
                                                class: "w-2 h-2 bg-yellow-500 rounded-full animate-pulse"
                                            }
                                            span { "Waiting for payment via Nostr..." }
                                        }
                                    },
                                    Some(PaymentRequestProgress::Error { message }) => rsx! {
                                        div {
                                            class: "text-red-500",
                                            "Error: {message}"
                                        }
                                    },
                                    _ => rsx! {
                                        span { "Share this request with the payer" }
                                    }
                                }
                            }
                        }

                        // Create another button
                        button {
                            class: "w-full py-2 mt-4 text-sm text-muted-foreground hover:text-foreground transition",
                            onclick: move |_| {
                                // Cancel current request
                                if let Some(req_id) = current_request_id.read().as_ref() {
                                    cashu_wallet::cancel_payment_request(req_id);
                                }
                                request_string.set(None);
                                current_request_id.set(None);
                                amount_input.set(String::new());
                                description_input.set(String::new());
                            },
                            "Create Another Request"
                        }
                    }
                } else {
                    // Creation form
                    div {
                        // Amount input (optional)
                        div {
                            class: "mb-4",
                            label {
                                class: "block text-sm font-medium mb-2",
                                "Amount (optional)"
                            }
                            div {
                                class: "relative",
                                input {
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                                    r#type: "number",
                                    placeholder: "Any amount",
                                    value: "{amount_input}",
                                    oninput: move |e| amount_input.set(e.value()),
                                }
                                span {
                                    class: "absolute right-4 top-1/2 -translate-y-1/2 text-muted-foreground",
                                    "sats"
                                }
                            }
                            p {
                                class: "text-xs text-muted-foreground mt-1",
                                "Leave empty to accept any amount"
                            }
                        }

                        // Description input
                        div {
                            class: "mb-4",
                            label {
                                class: "block text-sm font-medium mb-2",
                                "Description (optional)"
                            }
                            input {
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                                r#type: "text",
                                placeholder: "What's this payment for?",
                                value: "{description_input}",
                                oninput: move |e| description_input.set(e.value()),
                            }
                        }

                        // Transport selection
                        div {
                            class: "mb-6",
                            label {
                                class: "block text-sm font-medium mb-2",
                                "Receive via"
                            }
                            div {
                                class: "flex gap-2",
                                button {
                                    class: if *use_nostr_transport.read() {
                                        "flex-1 py-2 px-4 rounded-lg font-medium transition bg-purple-500 text-white"
                                    } else {
                                        "flex-1 py-2 px-4 rounded-lg font-medium transition bg-background border border-border hover:bg-accent"
                                    },
                                    onclick: move |_| use_nostr_transport.set(true),
                                    "Nostr (auto-receive)"
                                }
                                button {
                                    class: if !*use_nostr_transport.read() {
                                        "flex-1 py-2 px-4 rounded-lg font-medium transition bg-purple-500 text-white"
                                    } else {
                                        "flex-1 py-2 px-4 rounded-lg font-medium transition bg-background border border-border hover:bg-accent"
                                    },
                                    onclick: move |_| use_nostr_transport.set(false),
                                    "Manual"
                                }
                            }
                            p {
                                class: "text-xs text-muted-foreground mt-2",
                                if *use_nostr_transport.read() {
                                    "Payment will be automatically received via encrypted Nostr message"
                                } else {
                                    "Payer must send you the ecash token separately"
                                }
                            }
                        }

                        // Error message
                        if let Some(err) = error_message.read().as_ref() {
                            div {
                                class: "mb-4 p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-500 text-sm",
                                "{err}"
                            }
                        }

                        // Create button
                        button {
                            class: "w-full py-3 bg-blue-500 hover:bg-blue-600 disabled:bg-muted disabled:cursor-not-allowed text-white rounded-lg font-semibold transition",
                            disabled: *is_creating.read(),
                            onclick: handle_create,
                            if *is_creating.read() {
                                "Creating..."
                            } else {
                                "Create Request"
                            }
                        }

                        // Balance info
                        p {
                            class: "text-xs text-muted-foreground mt-3 text-center",
                            "Current balance: {balance} sats"
                        }
                    }
                }
            }
        }
    }
}
