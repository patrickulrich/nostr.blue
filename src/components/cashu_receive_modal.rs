use dioxus::prelude::*;
use crate::stores::cashu_wallet::{self, ReceiveTokensOptions};

#[component]
pub fn CashuReceiveModal(
    on_close: EventHandler<()>,
) -> Element {
    let mut token_string = use_signal(|| String::new());
    let mut is_receiving = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut success_message = use_signal(|| Option::<String>::None);
    let mut verify_dleq = use_signal(|| false); // NUT-12 DLEQ verification toggle

    let on_receive = move |_| {
        let token = token_string.read().trim().to_string();
        if token.is_empty() {
            error_message.set(Some("Please paste a token string".to_string()));
            return;
        }

        let should_verify_dleq = *verify_dleq.read();
        is_receiving.set(true);
        error_message.set(None);
        success_message.set(None);

        spawn(async move {
            let options = ReceiveTokensOptions {
                verify_dleq: should_verify_dleq,
            };

            match cashu_wallet::receive_tokens_with_options(token, options).await {
                Ok(amount) => {
                    let msg = if should_verify_dleq {
                        format!("Successfully received {} sats (DLEQ verified)", amount)
                    } else {
                        format!("Successfully received {} sats!", amount)
                    };
                    success_message.set(Some(msg));
                    is_receiving.set(false);
                    // Clear token input
                    token_string.set(String::new());
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to receive: {}", e)));
                    is_receiving.set(false);
                }
            }
        });
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
                        "Receive Tokens"
                    }
                    button {
                        class: "text-2xl text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| on_close.call(()),
                        "x"
                    }
                }

                // Body
                div {
                    class: "p-6 space-y-4",

                    // Token input
                    div {
                        label {
                            class: "block text-sm font-semibold mb-2",
                            "Paste Token String"
                        }
                        textarea {
                            class: "w-full px-4 py-3 bg-background border border-border rounded-lg font-mono text-sm min-h-[120px]",
                            placeholder: "cashuA...",
                            value: token_string.read().clone(),
                            oninput: move |evt| token_string.set(evt.value())
                        }
                        p {
                            class: "text-xs text-muted-foreground mt-2",
                            "Paste a Cashu token string to receive ecash"
                        }
                    }

                    // NUT-12 DLEQ Verification toggle
                    div {
                        class: "flex items-start gap-3 p-3 bg-accent/30 rounded-lg",
                        input {
                            r#type: "checkbox",
                            id: "verify-dleq",
                            class: "mt-1 w-4 h-4 rounded border-border",
                            checked: *verify_dleq.read(),
                            disabled: *is_receiving.read(),
                            onchange: move |evt| verify_dleq.set(evt.checked())
                        }
                        div {
                            class: "flex-1",
                            label {
                                r#for: "verify-dleq",
                                class: "text-sm font-medium cursor-pointer",
                                "Verify signatures (NUT-12)"
                            }
                            p {
                                class: "text-xs text-muted-foreground mt-1",
                                "Cryptographically verify the mint's blind signatures before accepting. Rejects tokens without DLEQ proofs."
                            }
                        }
                    }

                    // Success message
                    if let Some(msg) = success_message.read().as_ref() {
                        div {
                            class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-4",
                            div {
                                class: "flex items-start gap-3",
                                div {
                                    class: "text-2xl",
                                    "+"
                                }
                                div {
                                    p {
                                        class: "text-sm text-green-800 dark:text-green-200",
                                        "{msg}"
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
                                    "!"
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

                    // Info box
                    div {
                        class: "bg-accent/50 rounded-lg p-4",
                        h4 {
                            class: "text-sm font-semibold mb-2",
                            "How it works:"
                        }
                        ul {
                            class: "text-sm text-muted-foreground space-y-1",
                            li { "1. Paste the token string from sender" }
                            li { "2. Token is validated and decoded" }
                            if *verify_dleq.read() {
                                li { "3. DLEQ proofs are verified (NUT-12)" }
                                li { "4. Proofs are redeemed at the mint" }
                                li { "5. New token event is created (kind 7375)" }
                                li { "6. Balance is updated" }
                            } else {
                                li { "3. Proofs are redeemed at the mint" }
                                li { "4. New token event is created (kind 7375)" }
                                li { "5. Balance is updated" }
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "px-6 py-4 border-t border-border flex gap-3",
                    button {
                        class: "flex-1 px-4 py-3 bg-accent hover:bg-accent/80 rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: if *is_receiving.read() || token_string.read().is_empty() {
                            "flex-1 px-4 py-3 bg-blue-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                        } else {
                            "flex-1 px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition"
                        },
                        disabled: *is_receiving.read() || token_string.read().is_empty(),
                        onclick: on_receive,
                        if *is_receiving.read() {
                            if *verify_dleq.read() {
                                "Verifying & Receiving..."
                            } else {
                                "Receiving..."
                            }
                        } else {
                            "Receive Tokens"
                        }
                    }
                }
            }
        }
    }
}
