//! Nutzap Inbox Component
//!
//! Displays pending and received nutzaps with redemption controls.
use crate::stores::cashu;
use crate::utils::{format::truncate_pubkey, shorten_url};
use dioxus::prelude::*;
#[component]
pub fn NutzapInbox(on_close: EventHandler<()>) -> Element {
    let nutzap_enabled = *cashu::NUTZAP_ENABLED.read();
    let mut redeeming_ids = use_signal(std::collections::HashSet::<String>::new);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut success_message = use_signal(|| Option::<String>::None);
    let mut is_redeeming_all = use_signal(|| false);
    let mut is_refreshing = use_signal(|| false);
    let on_refresh = move |_| {
        if *is_refreshing.read() {
            return;
        }
        is_refreshing.set(true);
        error_message.set(None);
        success_message.set(None);
        spawn(async move {
            match cashu::fetch_pending_nutzaps().await {
                Ok(count) => {
                    if count > 0 {
                        success_message
                            .set(Some(format!("Found {} new nutzaps", count)));
                    } else {
                        success_message.set(Some("No new nutzaps found".to_string()));
                    }
                }
                Err(e) => {
                    error_message.set(Some(format!("Refresh failed: {}", e)));
                }
            }
            is_refreshing.set(false);
        });
    };
    let mut on_redeem = move |event_id: String| {
        let event_id_clone = event_id.clone();
        redeeming_ids.write().insert(event_id.clone());
        error_message.set(None);
        success_message.set(None);
        spawn(async move {
            match cashu::redeem_nutzap(&event_id_clone).await {
                Ok(result) => {
                    success_message
                        .set(Some(format!("Redeemed {} sats!", result.amount)));
                }
                Err(e) => {
                    error_message.set(Some(format!("Redemption failed: {}", e)));
                }
            }
            redeeming_ids.write().remove(&event_id_clone);
        });
    };
    let on_redeem_all = move |_| {
        if *is_redeeming_all.peek() {
            return;
        }
        let pending: Vec<String> = cashu::PENDING_NUTZAPS
            .read()
            .iter()
            .filter(|n| matches!(n.status, cashu::NutzapStatus::Pending))
            .map(|n| n.event_id.clone())
            .collect();
        if pending.is_empty() {
            return;
        }
        error_message.set(None);
        success_message.set(None);
        is_redeeming_all.set(true);
        for event_id in &pending {
            redeeming_ids.write().insert(event_id.clone());
        }
        spawn(async move {
            let mut success_count = 0u32;
            let mut error_count = 0u32;
            let mut total_redeemed = 0u64;
            for event_id in pending {
                match cashu::redeem_nutzap(&event_id).await {
                    Ok(result) => {
                        success_count += 1;
                        total_redeemed += result.amount;
                    }
                    Err(e) => {
                        log::error!("Failed to redeem {}: {}", event_id, e);
                        error_count += 1;
                    }
                }
                redeeming_ids.write().remove(&event_id);
                crate::platform::timer::sleep_ms(100).await;
            }
            if error_count == 0 {
                success_message
                    .set(
                        Some(
                            format!(
                                "Redeemed {} nutzaps ({} sats total)",
                                success_count,
                                total_redeemed,
                            ),
                        ),
                    );
            } else {
                // Use error_message for partial failures
                error_message
                    .set(
                        Some(
                            format!(
                                "Redeemed {}/{} nutzaps ({} sats). {} failed.",
                                success_count,
                                success_count + error_count,
                                total_redeemed,
                                error_count,
                            ),
                        ),
                    );
            }
            is_redeeming_all.set(false);
        });
    };
    let pending_nutzaps = cashu::PENDING_NUTZAPS.read();
    let total_pending: u64 = pending_nutzaps
        .iter()
        .filter(|n| matches!(n.status, cashu::NutzapStatus::Pending))
        .map(|n| n.amount)
        .sum();
    let pending_count = pending_nutzaps
        .iter()
        .filter(|n| matches!(n.status, cashu::NutzapStatus::Pending))
        .count();
    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 backdrop-blur-sm z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-card border border-border rounded-lg max-w-lg w-full shadow-xl max-h-[80vh] overflow-hidden flex flex-col",
                onclick: move |e| e.stop_propagation(),
                div { class: "px-6 py-4 border-b border-border flex items-center justify-between",
                    div {
                        h3 { class: "text-xl font-bold", "Nutzap Inbox" }
                        if pending_count > 0 {
                            p { class: "text-sm text-muted-foreground",
                                "{pending_count} pending ({total_pending} sats)"
                            }
                        }
                    }
                    div { class: "flex items-center gap-2",
                        button {
                            class: "p-2 hover:bg-accent rounded-lg transition",
                            disabled: *is_refreshing.read(),
                            onclick: on_refresh,
                            title: "Refresh",
                            if *is_refreshing.read() {
                                span { class: "animate-spin inline-block", "..." }
                            } else {
                                "Refresh"
                            }
                        }
                        button {
                            class: "text-2xl text-muted-foreground hover:text-foreground transition",
                            onclick: move |_| on_close.call(()),
                            "x"
                        }
                    }
                }
                div { class: "flex-1 overflow-y-auto p-6 space-y-4",
                    if !nutzap_enabled {
                        div { class: "bg-amber-50 dark:bg-amber-950/20 border border-amber-200 dark:border-amber-800 rounded-lg p-4 text-center",
                            p { class: "text-sm text-amber-800 dark:text-amber-200 mb-2",
                                "Nutzap receiving is not enabled"
                            }
                            p { class: "text-xs text-muted-foreground",
                                "Enable nutzaps in settings to receive tips"
                            }
                        }
                    }
                    if let Some(msg) = error_message.read().as_ref() {
                        div { class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-3",
                            p { class: "text-sm text-red-800 dark:text-red-200", "{msg}" }
                        }
                    }
                    if let Some(msg) = success_message.read().as_ref() {
                        div { class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-3",
                            p { class: "text-sm text-green-800 dark:text-green-200",
                                "{msg}"
                            }
                        }
                    }
                    if pending_nutzaps.is_empty() {
                        div { class: "text-center py-12 text-muted-foreground",
                            p { class: "text-lg mb-2", "No nutzaps yet" }
                            p { class: "text-sm",
                                "When someone sends you a nutzap, it will appear here"
                            }
                        }
                    }
                    for nutzap in pending_nutzaps.iter() {
                        {
                            let is_redeeming = redeeming_ids.read().contains(&nutzap.event_id);
                            let event_id = nutzap.event_id.clone();
                            let is_pending = matches!(nutzap.status, cashu::NutzapStatus::Pending);
                            rsx! {
                                div { key: "{nutzap.event_id}", class: "bg-accent/50 rounded-lg p-4 space-y-3",
                                    div { class: "flex items-center justify-between",
                                        div { class: "flex items-center gap-2",
                                            span { class: "text-2xl font-bold text-orange-500", "{nutzap.amount}" }
                                            span { class: "text-sm text-muted-foreground", "sats" }
                                        }
                                        match &nutzap.status {
                                            cashu::NutzapStatus::Pending => rsx! {
                                                span { class: "px-2 py-1 bg-amber-100 dark:bg-amber-900/30 text-amber-700 dark:text-amber-300 rounded-full text-xs font-medium",
                                                    "Pending"
                                                }
                                            },
                                            cashu::NutzapStatus::Redeeming => rsx! {
                                                span { class: "px-2 py-1 bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 rounded-full text-xs font-medium animate-pulse",
                                                    "Redeeming..."
                                                }
                                            },
                                            cashu::NutzapStatus::Redeemed => rsx! {
                                                span { class: "px-2 py-1 bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300 rounded-full text-xs font-medium",
                                                    "Redeemed"
                                                }
                                            },
                                            cashu::NutzapStatus::Failed(err) => rsx! {
                                                span {
                                                    class: "px-2 py-1 bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300 rounded-full text-xs font-medium",
                                                    title: "{err}",
                                                    "Failed"
                                                }
                                            },
                                        }
                                    }
                                    div { class: "text-sm space-y-1",
                                        div { class: "flex justify-between",
                                            span { class: "text-muted-foreground", "From:" }
                                            span { class: "font-mono text-xs", {truncate_pubkey(&nutzap.sender_pubkey)} }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-muted-foreground", "Mint:" }
                                            span { class: "font-mono text-xs", {shorten_url(&nutzap.mint_url, 25)} }
                                        }
                                        if let Some(comment) = &nutzap.comment {
                                            div { class: "pt-2 border-t border-border/50",
                                                p { class: "text-sm italic text-muted-foreground", "\"{comment}\"" }
                                            }
                                        }
                                        if let Some(target_id) = &nutzap.target_event_id {
                                            div { class: "text-xs text-muted-foreground", "Nutzap for: {truncate_pubkey(target_id)}" }
                                        }
                                    }
                                    if is_pending {
                                        button {
                                            class: if is_redeeming { "w-full px-4 py-2 bg-orange-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed" } else { "w-full px-4 py-2 bg-orange-500 hover:bg-orange-600 text-white font-semibold rounded-lg transition" },
                                            disabled: is_redeeming,
                                            onclick: move |_| on_redeem(event_id.clone()),
                                            if is_redeeming {
                                                "Redeeming..."
                                            } else {
                                                "Redeem"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "px-6 py-4 border-t border-border flex gap-3",
                    button {
                        class: "flex-1 px-4 py-3 bg-accent hover:bg-accent/80 rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                    if pending_count > 1 {
                        button {
                            class: if *is_redeeming_all.read() { "flex-1 px-4 py-3 bg-orange-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed" } else { "flex-1 px-4 py-3 bg-orange-500 hover:bg-orange-600 text-white font-semibold rounded-lg transition" },
                            disabled: *is_redeeming_all.read(),
                            onclick: on_redeem_all,
                            if *is_redeeming_all.read() {
                                "Redeeming All..."
                            } else {
                                "Redeem All ({pending_count})"
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Small badge component for showing nutzap count in wallet UI
///
/// Accepts optional pre-fetched count/value to avoid redundant signal reads.
#[component]
pub fn NutzapBadge(
    /// Pre-fetched nutzap count (if None, fetches from signal)
    count: Option<usize>,
    /// Pre-fetched nutzap value (if None, fetches from signal)
    value: Option<u64>,
) -> Element {
    let count = count.unwrap_or_else(cashu::pending_nutzap_count);
    let value = value.unwrap_or_else(cashu::pending_nutzap_value);
    if count == 0 {
        return rsx! {};
    }
    rsx! {
        div { class: "inline-flex items-center gap-1 px-2 py-0.5 bg-orange-100 dark:bg-orange-900/30 text-orange-700 dark:text-orange-300 rounded-full text-xs font-medium",
            span {
                if count != 1 {
                    "{count} nutzaps"
                } else {
                    "{count} nutzap"
                }
            }
            span { class: "text-orange-500", "({value} sats)" }
        }
    }
}
