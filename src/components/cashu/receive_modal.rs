use crate::stores::cashu;
use crate::stores::cashu::{ReceiveTokensOptions, TokenPreview};
use crate::stores::ui::online_status::ONLINE_STATUS;
use dioxus::prelude::*;
use dioxus_core::Task;
#[component]
pub fn CashuReceiveModal(on_close: EventHandler<()>) -> Element {
    let mut token_string = use_signal(String::new);
    let mut is_receiving = use_signal(|| false);
    let mut is_previewing = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut success_message = use_signal(|| Option::<String>::None);
    let mut preview = use_signal(|| Option::<TokenPreview>::None);
    let mut preview_task = use_signal(|| None::<Task>);
    let mut show_trust_prompt = use_signal(|| false);
    let on_token_change = move |evt: FormEvent| {
        let value = evt.value();
        token_string.set(value.clone());
        preview.set(None);
        error_message.set(None);
        show_trust_prompt.set(false);
        if let Some(task) = preview_task.read().as_ref() {
            task.cancel();
            is_previewing.set(false);
        }
        preview_task.set(None);
        let trimmed = value.trim().to_string();
        if trimmed.starts_with("cashuA") || trimmed.starts_with("cashuB") {
            is_previewing.set(true);
            let token_snapshot = trimmed.clone();
            let new_task = spawn(async move {
                crate::platform::timer::sleep_ms(300).await;
                match cashu::preview_token(token_snapshot.clone()).await {
                    Ok(p) => {
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
            is_previewing.set(false);
        }
    };
    let mut do_receive = move || {
        let token = token_string.read().trim().to_string();
        if token.is_empty() {
            error_message.set(Some("Please paste a token string".to_string()));
            return;
        }
        is_receiving.set(true);
        error_message.set(None);
        success_message.set(None);
        show_trust_prompt.set(false);
        let is_online = *ONLINE_STATUS.read();
        spawn(async move {
            if is_online {
                let options = ReceiveTokensOptions {
                    preimages: vec![],
                };
                match cashu::receive_tokens_with_options(token, options).await {
                    Ok(amount) => {
                        success_message.set(Some(format!("Successfully received {} sats!", amount)));
                        is_receiving.set(false);
                        token_string.set(String::new());
                        preview.set(None);
                    }
                    Err(e) => {
                        error_message.set(Some(format!("Failed to receive: {}", e)));
                        is_receiving.set(false);
                    }
                }
            } else {
                let mint_url = preview
                    .read()
                    .as_ref()
                    .map(|p| p.mint_url.clone())
                    .unwrap_or_default();
                let value = preview.read().as_ref().map(|p| p.value).unwrap_or(0);
                match cashu::offline_receive::store_offline_token(
                    token, mint_url, value,
                )
                .await
                {
                    Ok(()) => {
                        success_message.set(Some(format!(
                            "Token saved for offline receive. Will be redeemed when back online. ({} sats)",
                            value
                        )));
                        is_receiving.set(false);
                        token_string.set(String::new());
                        preview.set(None);
                    }
                    Err(e) => {
                        error_message.set(Some(format!("Failed to store offline token: {}", e)));
                        is_receiving.set(false);
                    }
                }
            }
        });
    };
    let on_receive = move |_| {
        if let Some(p) = preview.read().as_ref() {
            if p.is_new_mint && !*show_trust_prompt.read() {
                show_trust_prompt.set(true);
                return;
            }
        }
        do_receive();
    };
    let on_trust_and_receive = move |_| {
        show_trust_prompt.set(false);
        do_receive();
    };
    let on_cancel_trust = move |_| {
        show_trust_prompt.set(false);
    };
    let online = ONLINE_STATUS();
    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-card border border-border rounded-lg max-w-md w-full shadow-xl max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),
                div { class: "px-6 py-4 border-b border-border flex items-center justify-between",
                    h3 { class: "text-xl font-bold", "Receive Tokens" }
                    button {
                        class: "text-2xl text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| on_close.call(()),
                        "x"
                    }
                }
                div { class: "p-6 space-y-4",
                    div {
                        label { class: "block text-sm font-semibold mb-2", "Paste Token String" }
                        textarea {
                            class: "w-full px-4 py-3 bg-background border border-border rounded-lg font-mono text-sm min-h-[120px]",
                            placeholder: "cashuA...",
                            value: token_string.read().clone(),
                            oninput: on_token_change,
                            disabled: *is_receiving.read(),
                        }
                        p { class: "text-xs text-muted-foreground mt-2",
                            "Paste a Cashu token string to receive ecash"
                        }
                    }
                    if *is_previewing.read() {
                        div { class: "bg-accent/50 border border-border rounded-lg p-4",
                            div { class: "flex items-center gap-2 text-muted-foreground",
                                div { class: "animate-spin w-4 h-4 border-2 border-current border-t-transparent rounded-full" }
                                span { class: "text-sm", "Analyzing token..." }
                            }
                        }
                    }
                    if let Some(p) = preview.read().as_ref() {
                        div { class: "bg-gradient-to-r from-blue-500/10 to-purple-500/10 border border-blue-500/30 rounded-lg p-4 space-y-3",
                            div { class: "flex items-center justify-between",
                                span { class: "text-sm text-muted-foreground", "Token Value" }
                                span { class: "text-2xl font-bold text-blue-500", "{p.value} {p.unit}" }
                            }
                            div { class: "space-y-2 text-sm",
                                div { class: "flex items-center justify-between",
                                    span { class: "text-muted-foreground", "Mint" }
                                    span {
                                        class: "font-mono text-xs truncate max-w-[200px]",
                                        title: "{p.mint_url}",
                                        "{p.mint_url}"
                                    }
                                }
                                if p.is_new_mint {
                                    div { class: "flex items-center gap-1.5",
                                        span { class: "text-xs px-1.5 py-0.5 rounded bg-amber-500/20 text-amber-600 dark:text-amber-400 font-medium",
                                            "New mint"
                                        }
                                    }
                                }
                                div { class: "flex items-center justify-between",
                                    span { class: "text-muted-foreground", "Proofs" }
                                    span { "{p.proof_count}" }
                                }
                                if let Some(memo) = &p.memo {
                                    {
                                        let sanitized_memo = ammonia::clean_text(memo);
                                        rsx! {
                                            div { class: "flex items-start justify-between",
                                                span { class: "text-muted-foreground", "Memo" }
                                                span { class: "text-right max-w-[200px] italic", "\"{sanitized_memo}\"" }
                                            }
                                        }
                                    }
                                }
                                if let Some(lock_info) = &p.p2pk_locked {
                                    div { class: "flex items-center gap-2 pt-1",
                                        span { class: "text-base", "\u{1f512}" }
                                        if lock_info.is_ours {
                                            span { class: "text-green-600 dark:text-green-400 font-medium text-sm",
                                                if lock_info.is_p2bk { "Locked to you (P2BK)" } else { "Locked to you" }
                                            }
                                        } else {
                                            span { class: "text-amber-600 dark:text-amber-400 font-medium text-sm",
                                                if lock_info.is_p2bk { "P2BK locked" } else { "Locked to another key" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if *show_trust_prompt.read() {
                        div { class: "bg-amber-50 dark:bg-amber-950/20 border border-amber-300 dark:border-amber-700 rounded-lg p-4 space-y-3",
                            h4 { class: "text-sm font-semibold text-amber-800 dark:text-amber-200",
                                "Unknown Mint"
                            }
                            p { class: "text-sm text-muted-foreground",
                                "This token is from a mint not in your wallet:"
                            }
                            if let Some(p) = preview.read().as_ref() {
                                p { class: "text-xs font-mono break-all bg-background rounded p-2",
                                    "{p.mint_url}"
                                }
                            }
                            p { class: "text-xs text-muted-foreground",
                                "Adding unknown mints carries risk. Only proceed if you trust this mint."
                            }
                            div { class: "flex gap-2",
                                button {
                                    class: "flex-1 px-3 py-2 text-sm bg-amber-500 hover:bg-amber-600 text-white rounded-lg transition font-medium",
                                    onclick: on_trust_and_receive,
                                    "Add & Receive"
                                }
                                button {
                                    class: "flex-1 px-3 py-2 text-sm border border-border rounded-lg hover:bg-accent transition",
                                    onclick: on_cancel_trust,
                                    "Cancel"
                                }
                            }
                        }
                    }
                    if let Some(msg) = success_message.read().as_ref() {
                        div { class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-4",
                            div { class: "flex items-start gap-3",
                                div { class: "text-2xl", "+" }
                                div {
                                    p { class: "text-sm text-green-800 dark:text-green-200",
                                        "{msg}"
                                    }
                                }
                            }
                        }
                    }
                    if let Some(msg) = error_message.read().as_ref() {
                        div { class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-4",
                            div { class: "flex items-start gap-3",
                                div { class: "text-2xl", "!" }
                                div {
                                    p { class: "text-sm text-red-800 dark:text-red-200",
                                        "{msg}"
                                    }
                                }
                            }
                        }
                    }
                    div { class: "bg-accent/50 rounded-lg p-4",
                        h4 { class: "text-sm font-semibold mb-2", "How it works:" }
                        ul { class: "text-sm text-muted-foreground space-y-1",
                            li { "1. Paste the token string from sender" }
                            li { "2. Token is validated and decoded" }
                            li { "3. DLEQ signatures are verified (NUT-12)" }
                            li { "4. Proofs are redeemed at the mint" }
                            li { "5. New token event is created (kind 7375)" }
                            li { "6. Balance is updated" }
                        }
                    }
                }
                div { class: "px-6 py-4 border-t border-border flex gap-3",
                    button {
                        class: "flex-1 px-4 py-3 bg-accent hover:bg-accent/80 rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: if *is_receiving.read() || token_string.read().is_empty() || *show_trust_prompt.read() { "flex-1 px-4 py-3 bg-blue-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed" } else { "flex-1 px-4 py-3 bg-blue-500 hover:bg-blue-600 text-white font-semibold rounded-lg transition" },
                        disabled: *is_receiving.read() || token_string.read().is_empty() || *show_trust_prompt.read(),
                        onclick: on_receive,
                        if *is_receiving.read() {
                            "Receiving..."
                        } else if !online {
                            "Receive Offline"
                        } else {
                            "Receive Tokens"
                        }
                    }
                }
            }
        }
    }
}
