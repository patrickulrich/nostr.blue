use dioxus::prelude::*;
use crate::stores::cashu_wallet;

#[component]
pub fn CashuSendModal(
    on_close: EventHandler<()>,
) -> Element {
    let mut amount = use_signal(|| String::new());
    let mints = cashu_wallet::get_mints();
    let mut selected_mint = use_signal(|| mints.first().cloned().unwrap_or_default());
    let mut is_sending = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut token_result = use_signal(|| Option::<String>::None);
    // P2PK (send to npub) support
    let mut p2pk_enabled = use_signal(|| false);
    let mut recipient_pubkey = use_signal(|| String::new());

    // Keep selected_mint in sync with available mints
    use_effect(move || {
        let current_mints = cashu_wallet::get_mints();
        let current_selection = selected_mint.read().clone();

        // If no mint is selected, set it to the first available (only if one exists)
        if current_selection.is_empty() {
            if let Some(first_mint) = current_mints.first() {
                selected_mint.set(first_mint.clone());
            }
        }
        // If the selected mint is no longer in the list, reset to first available (if any)
        else if !current_mints.contains(&current_selection) {
            if let Some(first_mint) = current_mints.first() {
                selected_mint.set(first_mint.clone());
            } else {
                // Clear selection if no mints remain
                selected_mint.set(String::new());
            }
        }
    });

    let on_send = move |_| {
        // Early guard: prevent concurrent send operations
        if *is_sending.read() {
            return;
        }

        let amount_str = amount.read().clone();
        let mint = selected_mint.read().clone();
        let is_p2pk = *p2pk_enabled.read();
        let recipient = recipient_pubkey.read().clone();

        // Validate amount
        let amount_sats = match amount_str.parse::<u64>() {
            Ok(a) if a > 0 => a,
            _ => {
                error_message.set(Some("Please enter a valid amount".to_string()));
                return;
            }
        };

        if mint.is_empty() {
            error_message.set(Some("Please select a mint".to_string()));
            return;
        }

        // Validate recipient for P2PK
        if is_p2pk && recipient.is_empty() {
            error_message.set(Some("Please enter a recipient npub or public key".to_string()));
            return;
        }

        is_sending.set(true);
        error_message.set(None);
        token_result.set(None);

        spawn(async move {
            let result = if is_p2pk {
                // Send with P2PK lock (only recipient can redeem)
                cashu_wallet::send_tokens_p2pk(mint, amount_sats, recipient).await
            } else {
                // Regular send (anyone with token can redeem)
                cashu_wallet::send_tokens(mint, amount_sats).await
            };

            match result {
                Ok(token_string) => {
                    token_result.set(Some(token_string));
                    is_sending.set(false);
                    // Clear inputs
                    amount.set(String::new());
                    recipient_pubkey.set(String::new());
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to send: {}", e)));
                    is_sending.set(false);
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
                        "Send Tokens"
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

                    // Amount input
                    div {
                        label {
                            class: "block text-sm font-semibold mb-2",
                            "Amount (sats)"
                        }
                        input {
                            class: "w-full px-4 py-3 bg-background border border-border rounded-lg text-lg",
                            r#type: "number",
                            placeholder: "0",
                            min: "1",
                            value: amount.read().clone(),
                            oninput: move |evt| amount.set(evt.value())
                        }
                    }

                    // Mint selection
                    if !mints.is_empty() {
                        div {
                            label {
                                class: "block text-sm font-semibold mb-2",
                                "Select Mint"
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

                    // P2PK toggle (send to specific npub)
                    div {
                        class: "flex items-center justify-between py-2",
                        div {
                            label {
                                class: "text-sm font-semibold",
                                "Lock to recipient (P2PK)"
                            }
                            p {
                                class: "text-xs text-muted-foreground",
                                "Only the specified npub can redeem this token"
                            }
                        }
                        button {
                            class: if *p2pk_enabled.read() {
                                "w-12 h-6 rounded-full bg-blue-500 relative transition-colors"
                            } else {
                                "w-12 h-6 rounded-full bg-gray-300 dark:bg-gray-600 relative transition-colors"
                            },
                            onclick: move |_| {
                                let current = *p2pk_enabled.read();
                                p2pk_enabled.set(!current);
                            },
                            div {
                                class: if *p2pk_enabled.read() {
                                    "w-5 h-5 rounded-full bg-white absolute top-0.5 right-0.5 transition-all"
                                } else {
                                    "w-5 h-5 rounded-full bg-white absolute top-0.5 left-0.5 transition-all"
                                }
                            }
                        }
                    }

                    // Recipient input (only shown when P2PK is enabled)
                    if *p2pk_enabled.read() {
                        div {
                            label {
                                class: "block text-sm font-semibold mb-2",
                                "Recipient (npub or hex pubkey)"
                            }
                            input {
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg text-sm font-mono",
                                r#type: "text",
                                placeholder: "npub1... or hex public key",
                                value: recipient_pubkey.read().clone(),
                                oninput: move |evt| recipient_pubkey.set(evt.value())
                            }
                            p {
                                class: "text-xs text-muted-foreground mt-1",
                                "The token can only be redeemed by this user's wallet"
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

                    // Token result
                    if let Some(token) = token_result.read().as_ref() {
                        div {
                            class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-4",
                            div {
                                class: "space-y-2",
                                div {
                                    class: "flex items-start gap-3",
                                    div {
                                        class: "text-2xl",
                                        "✅"
                                    }
                                    div {
                                        p {
                                            class: "text-sm font-semibold text-green-800 dark:text-green-200",
                                            "Token created successfully!"
                                        }
                                    }
                                }
                                div {
                                    label {
                                        class: "block text-xs font-semibold mb-1 text-green-800 dark:text-green-200",
                                        "Share this token:"
                                    }
                                    div {
                                        class: "flex gap-2",
                                        textarea {
                                            id: "send-token",
                                            class: "flex-1 px-3 py-2 bg-white dark:bg-gray-900 border border-green-300 dark:border-green-700 rounded font-mono text-xs min-h-[80px]",
                                            readonly: true,
                                            value: token.clone(),
                                            onclick: move |_| {
                                                // Select all text on click
                                                #[cfg(target_arch = "wasm32")]
                                                {
                                                    use wasm_bindgen::JsCast;
                                                    if let Some(window) = web_sys::window() {
                                                        if let Some(document) = window.document() {
                                                            if let Some(textarea) = document.query_selector("#send-token").ok().flatten() {
                                                                if let Ok(element) = textarea.dyn_into::<web_sys::HtmlTextAreaElement>() {
                                                                    element.select();
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        button {
                                            class: "px-3 py-2 bg-green-600 hover:bg-green-700 text-white text-xs rounded transition",
                                            onclick: move |_| {
                                                #[cfg(target_arch = "wasm32")]
                                                {
                                                    if let Some(token_to_copy) = token_result.read().as_ref() {
                                                        if let Some(window) = web_sys::window() {
                                                            let navigator = window.navigator();
                                                            let clipboard = navigator.clipboard();
                                                            let _ = clipboard.write_text(token_to_copy);
                                                        }
                                                    }
                                                }
                                            },
                                            "Copy"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Preview
                    div {
                        class: "bg-accent/50 rounded-lg p-4",
                        h4 {
                            class: "text-sm font-semibold mb-2",
                            "Preview"
                        }
                        div {
                            class: "space-y-2 text-sm",
                            div {
                                class: "flex justify-between",
                                span { class: "text-muted-foreground", "Amount:" }
                                span { class: "font-mono", "{amount.read()} sats" }
                            }
                            div {
                                class: "flex justify-between",
                                span { class: "text-muted-foreground", "Mint:" }
                                span { class: "font-mono text-xs truncate max-w-[200px]", {selected_mint.read().as_str()} }
                            }
                            div {
                                class: "flex justify-between",
                                span { class: "text-muted-foreground", "Type:" }
                                if *p2pk_enabled.read() {
                                    span { class: "text-blue-500 font-semibold", "P2PK (Locked)" }
                                } else {
                                    span { "Bearer token" }
                                }
                            }
                            if *p2pk_enabled.read() && !recipient_pubkey.read().is_empty() {
                                div {
                                    class: "flex justify-between",
                                    span { class: "text-muted-foreground", "Recipient:" }
                                    span { class: "font-mono text-xs truncate max-w-[180px]", {shorten_pubkey(&recipient_pubkey.read())} }
                                }
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
                    {
                        let is_disabled = *is_sending.read()
                            || amount.read().is_empty()
                            || (*p2pk_enabled.read() && recipient_pubkey.read().is_empty());

                        rsx! {
                            button {
                                class: if is_disabled {
                                    "flex-1 px-4 py-3 bg-blue-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                                } else {
                                    "flex-1 px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition"
                                },
                                disabled: is_disabled,
                                onclick: on_send,
                                if *is_sending.read() {
                                    "Sending..."
                                } else if *p2pk_enabled.read() {
                                    "Send P2PK Token"
                                } else {
                                    "Send Tokens"
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

/// Shorten pubkey for display
fn shorten_pubkey(pubkey: &str) -> String {
    if pubkey.starts_with("npub") && pubkey.len() > 20 {
        format!("{}...{}", &pubkey[..12], &pubkey[pubkey.len()-8..])
    } else if pubkey.len() > 16 {
        format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len()-8..])
    } else {
        pubkey.to_string()
    }
}
