//! Nutzap Send Modal Component
//!
//! UI for sending nutzaps (NIP-61 P2PK-locked ecash via Nostr events).
use crate::stores::cashu;
use crate::utils::{format::truncate_pubkey, shorten_url};
use dioxus::hooks::use_reactive;
use dioxus::prelude::*;
#[component]
pub fn NutzapSendModal(
    /// Recipient's Nostr pubkey (npub or hex)
    recipient_pubkey: String,
    /// Optional event ID being nutzapped
    target_event_id: Option<String>,
    /// Optional target event kind
    target_kind: Option<u16>,
    /// Close handler
    on_close: EventHandler<()>,
    /// Success handler (receives event_id of published nutzap)
    on_success: Option<EventHandler<String>>,
) -> Element {
    let mut amount = use_signal(String::new);
    let mut comment = use_signal(String::new);
    let mut is_sending = use_signal(|| false);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut success_message = use_signal(|| Option::<String>::None);
    let mut recipient_info = use_signal(|| Option::<cashu::NutzapInfo>::None);
    let mut compatible_mint = use_signal(|| Option::<cashu::NutzapMint>::None);
    let mut is_loading_info = use_signal(|| true);
    let mut load_error = use_signal(|| Option::<String>::None);
    let mut request_version: Signal<u64> = use_signal(|| 0);
    let recipient_pubkey_for_effect = recipient_pubkey.clone();
    let client_initialized = *crate::stores::nostr_client::CLIENT_INITIALIZED.read();
    #[allow(unused_variables)]
    use_effect(use_reactive!(|(
        recipient_pubkey_for_effect,
        client_initialized,
    )| {
        let recipient = recipient_pubkey_for_effect.clone();
        // Use peek() to read without creating dependency, then increment
        let version = *request_version.peek() + 1;
        request_version.set(version);

        // Reset synchronously - UI updates immediately
        is_loading_info.set(true);
        load_error.set(None);
        recipient_info.set(None);
        compatible_mint.set(None);

        spawn(async move {
            match cashu::fetch_nutzap_info(&recipient).await {
                Ok(info) => {
                    // Stale check: use peek() to avoid subscription in async
                    if *request_version.peek() != version {
                        log::debug!("Stale nutzap fetch discarded (v{} != current)", version);
                        return;
                    }
                    match cashu::validate_nutzap_recipient_with_info(&info) {
                        Ok(mint) => compatible_mint.set(Some(mint)),
                        Err(e) => {
                            log::warn!("No compatible mint for nutzap: {}", e);
                        }
                    }
                    recipient_info.set(Some(info));
                }
                Err(e) => {
                    if *request_version.peek() != version {
                        return;
                    }
                    load_error.set(Some(e));
                }
            }

            if *request_version.peek() == version {
                is_loading_info.set(false);
            }
        });
    }));
    let on_send = {
        let recipient_pubkey = recipient_pubkey.clone();
        let target_event_id = target_event_id.clone();
        move |_| {
            if *is_sending.read() {
                return;
            }
            let amount_str = amount.read().clone();
            let comment_str = comment.read().clone();
            let recipient = recipient_pubkey.clone();
            let target_id = target_event_id.clone();
            let amount_sats = match amount_str.parse::<u64>() {
                Ok(a) if a > 0 => a,
                _ => {
                    error_message.set(Some("Please enter a valid amount".to_string()));
                    return;
                }
            };
            if compatible_mint.read().is_none() {
                error_message.set(Some("No compatible mint with recipient".to_string()));
                return;
            }
            is_sending.set(true);
            error_message.set(None);
            success_message.set(None);
            spawn(async move {
                let comment_opt = if comment_str.is_empty() {
                    None
                } else {
                    Some(comment_str.as_str())
                };
                match cashu::send_nutzap(
                    &recipient,
                    amount_sats,
                    target_id.as_deref(),
                    target_kind,
                    comment_opt,
                )
                .await
                {
                    Ok(result) => {
                        if result.is_pending_retry {
                            success_message.set(Some(format!(
                                "Sent {} sats (fee: {} sats) - publishing pending",
                                result.amount, result.fee,
                            )));
                            is_sending.set(false);
                        } else {
                            success_message.set(Some(format!(
                                "Sent {} sats (fee: {} sats)",
                                result.amount, result.fee,
                            )));
                            is_sending.set(false);
                            amount.set(String::new());
                            comment.set(String::new());
                            if let Some(handler) = &on_success {
                                handler.call(result.event_id);
                            }
                        }
                    }
                    Err(e) => {
                        error_message.set(Some(format!("Failed to send: {}", e)));
                        is_sending.set(false);
                    }
                }
            });
        }
    };
    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-card border border-border rounded-lg max-w-md w-full shadow-xl",
                onclick: move |e| e.stop_propagation(),
                div { class: "px-6 py-4 border-b border-border flex items-center justify-between",
                    h3 { class: "text-xl font-bold", "Send Nutzap" }
                    button {
                        class: "text-2xl text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| on_close.call(()),
                        "x"
                    }
                }
                div { class: "p-6 space-y-4",
                    if *is_loading_info.read() {
                        div { class: "flex items-center justify-center py-8",
                            div { class: "animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500" }
                        }
                    }
                    if let Some(error) = load_error.read().as_ref() {
                        div { class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-4",
                            p { class: "text-sm text-red-800 dark:text-red-200",
                                "Could not load nutzap information: {error}"
                            }
                        }
                    }
                    if let Some(info) = recipient_info.read().as_ref() {
                        div { class: "bg-accent/50 rounded-lg p-4",
                            h4 { class: "text-sm font-semibold mb-2", "Recipient" }
                            div { class: "space-y-2 text-sm",
                                div { class: "flex justify-between",
                                    span { class: "text-muted-foreground", "Pubkey:" }
                                    span { class: "font-mono text-xs",
                                        {truncate_pubkey(&recipient_pubkey)}
                                    }
                                }
                                div { class: "flex justify-between",
                                    span { class: "text-muted-foreground", "Accepted mints:" }
                                    span { "{info.mints.len()}" }
                                }
                                if let Some(mint) = compatible_mint.read().as_ref() {
                                    div { class: "flex justify-between items-center",
                                        span { class: "text-muted-foreground", "Using mint:" }
                                        span { class: "font-mono text-xs text-green-600 dark:text-green-400",
                                            {shorten_url(&mint.url, 25)}
                                        }
                                    }
                                } else {
                                    div { class: "flex justify-between items-center",
                                        span { class: "text-muted-foreground", "Compatible mint:" }
                                        span { class: "text-amber-600 dark:text-amber-400",
                                            "None found"
                                        }
                                    }
                                }
                            }
                        }
                        div {
                            label { class: "block text-sm font-semibold mb-2", "Amount (sats)" }
                            input {
                                class: "w-full px-4 py-3 bg-background border border-border rounded-lg text-lg",
                                r#type: "number",
                                placeholder: "0",
                                min: "1",
                                value: "{amount}",
                                oninput: move |evt| amount.set(evt.value()),
                                disabled: compatible_mint.read().is_none(),
                            }
                        }
                        div {
                            label { class: "block text-sm font-semibold mb-2", "Comment (optional)" }
                            input {
                                class: "w-full px-4 py-2 bg-background border border-border rounded-lg text-sm",
                                r#type: "text",
                                placeholder: "Thanks for the great content!",
                                maxlength: "256",
                                value: "{comment}",
                                oninput: move |evt| comment.set(evt.value()),
                                disabled: compatible_mint.read().is_none(),
                            }
                        }
                        if target_event_id.is_some() {
                            div { class: "text-xs text-muted-foreground bg-accent/30 rounded px-3 py-2",
                                "Nutzapping event: {target_event_id.as_ref().map(|id| truncate_pubkey(id)).unwrap_or_default()}"
                                if let Some(kind) = target_kind {
                                    span { class: "ml-2", "(kind: {kind})" }
                                }
                            }
                        }
                    }
                    if let Some(msg) = error_message.read().as_ref() {
                        div { class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-4",
                            p { class: "text-sm text-red-800 dark:text-red-200", "{msg}" }
                        }
                    }
                    if let Some(msg) = success_message.read().as_ref() {
                        div { class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-4",
                            div { class: "flex items-center gap-2",
                                span { class: "text-xl", "Nutzap sent!" }
                                p { class: "text-sm text-green-800 dark:text-green-200",
                                    "{msg}"
                                }
                            }
                        }
                    }
                }
                div { class: "px-6 py-4 border-t border-border flex gap-3",
                    button {
                        class: "flex-1 px-4 py-3 bg-accent hover:bg-accent/80 rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    {
                        let is_disabled = *is_sending.read() || *is_loading_info.read()
                            || load_error.read().is_some() || compatible_mint.read().is_none()
                            || amount.read().parse::<u64>().map_or(true, |a| a == 0);
                        rsx! {
                            button {
                                class: if is_disabled { "flex-1 px-4 py-3 bg-orange-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed" } else { "flex-1 px-4 py-3 bg-orange-500 hover:bg-orange-600 text-white font-semibold rounded-lg transition" },
                                disabled: is_disabled,
                                onclick: on_send,
                                if *is_sending.read() {
                                    "Sending..."
                                } else {
                                    "Send Nutzap"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
