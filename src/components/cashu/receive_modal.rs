use dioxus::prelude::*;
use dioxus_core::Task;
use crate::stores::cashu;
use crate::stores::cashu::{ReceiveTokensOptions, TokenPreview};

#[component]
pub fn CashuReceiveModal(
    on_close: EventHandler<()>,
) -> Element {
    let mut token_string = use_signal(String::new);
    let mut is_receiving = use_signal(|| false);
    let mut is_previewing = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut success_message = use_signal(|| Option::<String>::None);
    let mut preview = use_signal(|| Option::<TokenPreview>::None);
    let mut verify_dleq = use_signal(|| false); // NUT-12 DLEQ verification toggle
    let mut preview_task = use_signal(|| None::<Task>);

    // Auto-preview when token input changes (with debouncing)
    let on_token_change = move |evt: FormEvent| {
        let value = evt.value();
        token_string.set(value.clone());

        // Clear previous preview/errors
        preview.set(None);
        error_message.set(None);

        // Cancel any previous preview task (must be outside prefix check to handle
        // the case where user clears input while a task is in-flight)
        if let Some(task) = preview_task.read().as_ref() {
            task.cancel();
            // Reset spinner immediately when cancelling to prevent stale UI state
            is_previewing.set(false);
        }
        preview_task.set(None);

        // Only preview if it looks like a cashu token
        let trimmed = value.trim().to_string();
        if trimmed.starts_with("cashuA") || trimmed.starts_with("cashuB") {
            is_previewing.set(true);
            let token_snapshot = trimmed.clone();

            let new_task = spawn(async move {
                // Debounce: wait 300ms before making network request
                gloo_timers::future::TimeoutFuture::new(300).await;

                match cashu::preview_token(token_snapshot.clone()).await {
                    Ok(p) => {
                        // Only update if token hasn't changed during async operation
                        if token_string.read().trim() == token_snapshot {
                            preview.set(Some(p));
                            error_message.set(None);
                            is_previewing.set(false);
                        }
                    }
                    Err(e) => {
                        if token_string.read().trim() == token_snapshot {
                            preview.set(None);
                            error_message.set(Some(e));
                            is_previewing.set(false);
                        }
                    }
                }
            });
            preview_task.set(Some(new_task));
        } else {
            // Not a cashu token - ensure spinner is hidden
            is_previewing.set(false);
        }
    };

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
                preimages: vec![],  // No HTLC preimages for normal receive
            };

            match cashu::receive_tokens_with_options(token, options).await {
                Ok(amount) => {
                    let msg = if should_verify_dleq {
                        format!("Successfully received {} sats (DLEQ verified)", amount)
                    } else {
                        format!("Successfully received {} sats!", amount)
                    };
                    success_message.set(Some(msg));
                    is_receiving.set(false);
                    // Clear token input and preview
                    token_string.set(String::new());
                    preview.set(None);
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
                            oninput: on_token_change
                        }
                        p {
                            class: "text-xs text-muted-foreground mt-2",
                            "Paste a Cashu token string to receive ecash"
                        }
                    }

                    // Token preview (shows when valid token is detected)
                    if *is_previewing.read() {
                        div {
                            class: "bg-accent/50 border border-border rounded-lg p-4",
                            div {
                                class: "flex items-center gap-2 text-muted-foreground",
                                div {
                                    class: "animate-spin w-4 h-4 border-2 border-current border-t-transparent rounded-full"
                                }
                                span { class: "text-sm", "Analyzing token..." }
                            }
                        }
                    }

                    if let Some(p) = preview.read().as_ref() {
                        div {
                            class: "bg-gradient-to-r from-blue-500/10 to-purple-500/10 border border-blue-500/30 rounded-lg p-4 space-y-3",

                            // Value header
                            div {
                                class: "flex items-center justify-between",
                                span { class: "text-sm text-muted-foreground", "Token Value" }
                                span {
                                    class: "text-2xl font-bold text-blue-500",
                                    "{p.value} {p.unit}"
                                }
                            }

                            // Details
                            div {
                                class: "space-y-2 text-sm",

                                // Mint
                                div {
                                    class: "flex items-center justify-between",
                                    span { class: "text-muted-foreground", "Mint" }
                                    span {
                                        class: "font-mono text-xs truncate max-w-[200px]",
                                        title: "{p.mint_url}",
                                        "{p.mint_url}"
                                    }
                                }

                                // Proofs
                                div {
                                    class: "flex items-center justify-between",
                                    span { class: "text-muted-foreground", "Proofs" }
                                    span { "{p.proof_count}" }
                                }

                                // Memo (if present) - sanitize for defense-in-depth
                                if let Some(memo) = &p.memo {
                                    {
                                        let sanitized_memo = ammonia::clean_text(memo);
                                        rsx! {
                                            div {
                                                class: "flex items-start justify-between",
                                                span { class: "text-muted-foreground", "Memo" }
                                                span {
                                                    class: "text-right max-w-[200px] italic",
                                                    "\"{sanitized_memo}\""
                                                }
                                            }
                                        }
                                    }
                                }
                            }
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
