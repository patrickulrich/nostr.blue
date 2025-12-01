use dioxus::prelude::*;
use cdk::nuts::{PaymentRequest, TransportType};
use crate::stores::cashu::{self, WALLET_BALANCE};

#[derive(Clone)]
enum PayState {
    Input,
    Parsed { request: PaymentRequest },
    Paying,
    Success { amount: u64 },
    Error { message: String },
}

impl PartialEq for PayState {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (PayState::Input, PayState::Input) => true,
            (PayState::Parsed { request: a }, PayState::Parsed { request: b }) => {
                // Compare by string representation for simplicity
                a.to_string() == b.to_string()
            }
            (PayState::Paying, PayState::Paying) => true,
            (PayState::Success { amount: a }, PayState::Success { amount: b }) => a == b,
            (PayState::Error { message: a }, PayState::Error { message: b }) => a == b,
            _ => false,
        }
    }
}

#[component]
pub fn CashuPayRequestModal(
    on_close: EventHandler<()>,
) -> Element {
    let mut request_input = use_signal(|| String::new());
    let mut custom_amount = use_signal(|| String::new());
    let mut pay_state = use_signal(|| PayState::Input);

    // Use memo for reactive balance updates while modal is open
    let balance = use_memo(move || *WALLET_BALANCE.read());

    // Handle paste from clipboard
    let handle_paste = move |_| {
        spawn(async move {
            if let Some(window) = web_sys::window() {
                let clipboard = window.navigator().clipboard();
                let promise = clipboard.read_text();
                if let Ok(text) = wasm_bindgen_futures::JsFuture::from(promise).await {
                    if let Some(s) = text.as_string() {
                        request_input.set(s);
                    }
                }
            }
        });
    };

    // Handle parsing the request
    let handle_parse = move |_| {
        let input = request_input.read().trim().to_string();

        if input.is_empty() {
            pay_state.set(PayState::Error { message: "Please enter a payment request".to_string() });
            return;
        }

        match cashu::parse_payment_request(&input) {
            Ok(request) => {
                pay_state.set(PayState::Parsed { request });
            }
            Err(e) => {
                pay_state.set(PayState::Error { message: e });
            }
        }
    };

    // Handle paying the request
    let handle_pay = move |_| {
        // Early guard: prevent duplicate submissions if already paying
        if matches!(*pay_state.read(), PayState::Paying) {
            return;
        }

        let request_str = request_input.read().trim().to_string();
        let custom_amt_str = custom_amount.read().clone();

        let custom_amt: Option<u64> = if custom_amt_str.is_empty() {
            None
        } else {
            match custom_amt_str.parse::<u64>() {
                Ok(a) if a > 0 => Some(a),
                _ => {
                    pay_state.set(PayState::Error { message: "Invalid custom amount".to_string() });
                    return;
                }
            }
        };

        pay_state.set(PayState::Paying);

        spawn(async move {
            match cashu::pay_payment_request(request_str, custom_amt).await {
                Ok(amount) => {
                    pay_state.set(PayState::Success { amount });
                }
                Err(e) => {
                    pay_state.set(PayState::Error { message: e });
                }
            }
        });
    };

    // Handle going back to input
    let handle_back = move |_| {
        pay_state.set(PayState::Input);
        custom_amount.set(String::new());
    };

    rsx! {
        // Modal backdrop
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |e| {
                e.stop_propagation();
                // Don't close while payment is in progress
                if !matches!(*pay_state.read(), PayState::Paying) {
                    on_close.call(());
                }
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
                        "Pay Request"
                    }
                    if !matches!(*pay_state.read(), PayState::Paying) {
                        button {
                            class: "text-muted-foreground hover:text-foreground transition p-1",
                            onclick: move |_| on_close.call(()),
                            "X"
                        }
                    }
                }

                match &*pay_state.read() {
                    PayState::Input => rsx! {
                        // Input form
                        div {
                            // Request input
                            div {
                                class: "mb-4",
                                label {
                                    class: "block text-sm font-medium mb-2",
                                    "Payment Request"
                                }
                                textarea {
                                    class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none font-mono text-sm",
                                    rows: 4,
                                    placeholder: "Paste creqA... payment request",
                                    value: "{request_input}",
                                    oninput: move |e| request_input.set(e.value()),
                                }
                            }

                            // Paste button
                            button {
                                class: "w-full py-2 mb-4 bg-accent hover:bg-accent/80 rounded-lg transition flex items-center justify-center gap-2",
                                onclick: handle_paste,
                                span { "Paste from Clipboard" }
                            }

                            // Parse button
                            button {
                                class: "w-full py-3 bg-blue-500 hover:bg-blue-600 disabled:bg-muted disabled:cursor-not-allowed text-white rounded-lg font-semibold transition",
                                disabled: request_input.read().is_empty(),
                                onclick: handle_parse,
                                "Continue"
                            }

                            // Balance info
                            p {
                                class: "text-xs text-muted-foreground mt-3 text-center",
                                "Your balance: {balance()} sats"
                            }
                        }
                    },

                    PayState::Parsed { request } => {
                        let amount_specified = request.amount.is_some();
                        let request_amount = request.amount.map(u64::from).unwrap_or(0);
                        let has_description = request.description.is_some();
                        let description = request.description.clone().unwrap_or_default();
                        let mint_count = request.mints.as_ref().map(|m| m.len()).unwrap_or(0);
                        let request_mints: Vec<String> = request.mints.clone()
                            .unwrap_or_default()
                            .iter()
                            .map(|m| m.to_string())
                            .collect();
                        let has_nostr = request.transports.iter().any(|t| t._type == TransportType::Nostr);

                        rsx! {
                            div {
                                // Request details
                                div {
                                    class: "bg-background border border-border rounded-lg p-4 mb-4",

                                    // Amount
                                    div {
                                        class: "mb-3",
                                        div {
                                            class: "text-sm text-muted-foreground",
                                            "Amount"
                                        }
                                        div {
                                            class: "text-2xl font-bold",
                                            if amount_specified {
                                                "{request_amount} sats"
                                            } else {
                                                "Any amount"
                                            }
                                        }
                                    }

                                    // Description
                                    if has_description {
                                        div {
                                            class: "mb-3",
                                            div {
                                                class: "text-sm text-muted-foreground",
                                                "Description"
                                            }
                                            div {
                                                class: "text-foreground",
                                                "{description}"
                                            }
                                        }
                                    }

                                    // Mints
                                    if mint_count > 0 {
                                        div {
                                            class: "mb-3",
                                            div {
                                                class: "text-sm text-muted-foreground",
                                                "Accepted Mints"
                                            }
                                            div {
                                                class: "text-sm",
                                                for mint in request_mints.iter() {
                                                    div {
                                                        key: "{mint}",
                                                        class: "truncate text-muted-foreground",
                                                        "{mint}"
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Transport info
                                    div {
                                        div {
                                            class: "text-sm text-muted-foreground",
                                            "Delivery"
                                        }
                                        div {
                                            class: "text-sm flex items-center gap-2",
                                            if has_nostr {
                                                span {
                                                    class: "text-purple-500",
                                                    "Via Nostr (encrypted)"
                                                }
                                            } else {
                                                span {
                                                    class: "text-muted-foreground",
                                                    "Manual / HTTP"
                                                }
                                            }
                                        }
                                    }
                                }

                                // Custom amount input (if not specified)
                                if !amount_specified {
                                    div {
                                        class: "mb-4",
                                        label {
                                            class: "block text-sm font-medium mb-2",
                                            "Amount to Send"
                                        }
                                        div {
                                            class: "relative",
                                            input {
                                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                                                r#type: "number",
                                                placeholder: "Enter amount",
                                                value: "{custom_amount}",
                                                oninput: move |e| custom_amount.set(e.value()),
                                            }
                                            span {
                                                class: "absolute right-4 top-1/2 -translate-y-1/2 text-muted-foreground",
                                                "sats"
                                            }
                                        }
                                    }
                                }

                                // Calculate pay amount once for warning and button state
                                {
                                    let pay_amount = if amount_specified {
                                        request_amount
                                    } else {
                                        custom_amount.read().parse::<u64>().unwrap_or(0)
                                    };
                                    let current_balance = balance();
                                    let insufficient_balance = pay_amount > current_balance;
                                    let pay_disabled = pay_amount == 0 || insufficient_balance;

                                    rsx! {
                                        // Insufficient balance warning
                                        if insufficient_balance {
                                            div {
                                                class: "mb-4 p-3 bg-yellow-500/10 border border-yellow-500/30 rounded-lg text-yellow-600 dark:text-yellow-400 text-sm",
                                                "Insufficient balance. You need {pay_amount} sats but only have {current_balance} sats."
                                            }
                                        }

                                        // Action buttons
                                        div {
                                            class: "flex gap-3",
                                            button {
                                                class: "flex-1 py-3 bg-accent hover:bg-accent/80 rounded-lg font-semibold transition",
                                                onclick: handle_back,
                                                "Back"
                                            }
                                            button {
                                                class: "flex-1 py-3 bg-green-500 hover:bg-green-600 disabled:bg-muted disabled:cursor-not-allowed text-white rounded-lg font-semibold transition",
                                                disabled: pay_disabled,
                                                onclick: handle_pay,
                                                "Pay"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },

                    PayState::Paying => rsx! {
                        div {
                            class: "text-center py-8",
                            p {
                                class: "text-muted-foreground",
                                "Sending payment..."
                            }
                        }
                    },

                    PayState::Success { amount } => rsx! {
                        div {
                            class: "text-center py-8",
                            div {
                                class: "text-6xl mb-4",
                                "✅"
                            }
                            h3 {
                                class: "text-xl font-bold text-green-500 mb-2",
                                "Payment Sent!"
                            }
                            p {
                                class: "text-lg",
                                "{amount} sats"
                            }
                            button {
                                class: "mt-6 w-full py-3 bg-green-500 hover:bg-green-600 text-white rounded-lg font-semibold transition",
                                onclick: move |_| on_close.call(()),
                                "Done"
                            }
                        }
                    },

                    PayState::Error { message } => rsx! {
                        div {
                            class: "text-center py-8",
                            div {
                                class: "text-6xl mb-4",
                                "❌"
                            }
                            h3 {
                                class: "text-xl font-bold text-red-500 mb-2",
                                "Error"
                            }
                            p {
                                class: "text-muted-foreground mb-6",
                                "{message}"
                            }
                            button {
                                class: "w-full py-3 bg-accent hover:bg-accent/80 rounded-lg font-semibold transition",
                                onclick: handle_back,
                                "Try Again"
                            }
                        }
                    },
                }
            }
        }
    }
}
