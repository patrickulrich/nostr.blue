//! Nutzap Inbox Component
//!
//! Displays pending and received nutzaps with redemption controls.

use dioxus::prelude::*;
use crate::stores::cashu;
use crate::utils::{shorten_url, format::truncate_pubkey};

#[component]
pub fn NutzapInbox(on_close: EventHandler<()>) -> Element {
    let nutzap_enabled = *cashu::NUTZAP_ENABLED.read();

    // Redeeming state
    let mut redeeming_ids = use_signal(std::collections::HashSet::<String>::new);
    let mut error_message = use_signal(|| Option::<String>::None);
    let mut success_message = use_signal(|| Option::<String>::None);

    // Manual refresh
    let mut is_refreshing = use_signal(|| false);

    let on_refresh = move |_| {
        if *is_refreshing.read() {
            return;
        }
        is_refreshing.set(true);
        error_message.set(None);

        spawn(async move {
            match cashu::fetch_pending_nutzaps().await {
                Ok(count) => {
                    if count > 0 {
                        success_message.set(Some(format!("Found {} new nutzaps", count)));
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

    // Redeem a single nutzap
    let mut on_redeem = move |event_id: String| {
        let event_id_clone = event_id.clone();
        redeeming_ids.write().insert(event_id.clone());
        error_message.set(None);
        success_message.set(None);

        spawn(async move {
            match cashu::redeem_nutzap(&event_id_clone).await {
                Ok(result) => {
                    success_message.set(Some(format!("Redeemed {} sats!", result.amount)));
                }
                Err(e) => {
                    error_message.set(Some(format!("Redemption failed: {}", e)));
                }
            }
            redeeming_ids.write().remove(&event_id_clone);
        });
    };

    // Redeem all pending nutzaps
    let on_redeem_all = move |_| {
        // Read from signal to get current state
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

        for event_id in pending {
            let event_id_clone = event_id.clone();
            redeeming_ids.write().insert(event_id.clone());

            spawn(async move {
                if let Err(e) = cashu::redeem_nutzap(&event_id_clone).await {
                    log::error!("Failed to redeem {}: {}", event_id_clone, e);
                }
                redeeming_ids.write().remove(&event_id_clone);
            });
        }
    };

    // Calculate totals - read from signal for reactivity
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
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-card border border-border rounded-lg max-w-lg w-full shadow-xl max-h-[80vh] overflow-hidden flex flex-col",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "px-6 py-4 border-b border-border flex items-center justify-between",
                    div {
                        h3 {
                            class: "text-xl font-bold",
                            "Nutzap Inbox"
                        }
                        if pending_count > 0 {
                            p {
                                class: "text-sm text-muted-foreground",
                                "{pending_count} pending ({total_pending} sats)"
                            }
                        }
                    }
                    div {
                        class: "flex items-center gap-2",
                        button {
                            class: "p-2 hover:bg-accent rounded-lg transition",
                            disabled: *is_refreshing.read(),
                            onclick: on_refresh,
                            title: "Refresh",
                            if *is_refreshing.read() {
                                span {
                                    class: "animate-spin inline-block",
                                    "..."
                                }
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

                // Body
                div {
                    class: "flex-1 overflow-y-auto p-6 space-y-4",

                    // Not enabled warning
                    if !nutzap_enabled {
                        div {
                            class: "bg-amber-50 dark:bg-amber-950/20 border border-amber-200 dark:border-amber-800 rounded-lg p-4 text-center",
                            p {
                                class: "text-sm text-amber-800 dark:text-amber-200 mb-2",
                                "Nutzap receiving is not enabled"
                            }
                            p {
                                class: "text-xs text-muted-foreground",
                                "Enable nutzaps in settings to receive tips"
                            }
                        }
                    }

                    // Messages
                    if let Some(msg) = error_message.read().as_ref() {
                        div {
                            class: "bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-800 rounded-lg p-3",
                            p {
                                class: "text-sm text-red-800 dark:text-red-200",
                                "{msg}"
                            }
                        }
                    }
                    if let Some(msg) = success_message.read().as_ref() {
                        div {
                            class: "bg-green-50 dark:bg-green-950/20 border border-green-200 dark:border-green-800 rounded-lg p-3",
                            p {
                                class: "text-sm text-green-800 dark:text-green-200",
                                "{msg}"
                            }
                        }
                    }

                    // Empty state
                    if pending_nutzaps.is_empty() {
                        div {
                            class: "text-center py-12 text-muted-foreground",
                            p {
                                class: "text-lg mb-2",
                                "No nutzaps yet"
                            }
                            p {
                                class: "text-sm",
                                "When someone sends you a nutzap, it will appear here"
                            }
                        }
                    }

                    // Nutzap list
                    for nutzap in pending_nutzaps.iter() {
                        {
                            let is_redeeming = redeeming_ids.read().contains(&nutzap.event_id);
                            let event_id = nutzap.event_id.clone();
                            let is_pending = matches!(nutzap.status, cashu::NutzapStatus::Pending);

                            rsx! {
                                div {
                                    key: "{nutzap.event_id}",
                                    class: "bg-accent/50 rounded-lg p-4 space-y-3",

                                    // Header row
                                    div {
                                        class: "flex items-center justify-between",
                                        div {
                                            class: "flex items-center gap-2",
                                            span {
                                                class: "text-2xl font-bold text-orange-500",
                                                "{nutzap.amount}"
                                            }
                                            span {
                                                class: "text-sm text-muted-foreground",
                                                "sats"
                                            }
                                        }
                                        // Status badge
                                        match &nutzap.status {
                                            cashu::NutzapStatus::Pending => rsx! {
                                                span {
                                                    class: "px-2 py-1 bg-amber-100 dark:bg-amber-900/30 text-amber-700 dark:text-amber-300 rounded-full text-xs font-medium",
                                                    "Pending"
                                                }
                                            },
                                            cashu::NutzapStatus::Redeeming => rsx! {
                                                span {
                                                    class: "px-2 py-1 bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 rounded-full text-xs font-medium animate-pulse",
                                                    "Redeeming..."
                                                }
                                            },
                                            cashu::NutzapStatus::Redeemed => rsx! {
                                                span {
                                                    class: "px-2 py-1 bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300 rounded-full text-xs font-medium",
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

                                    // Details
                                    div {
                                        class: "text-sm space-y-1",
                                        div {
                                            class: "flex justify-between",
                                            span { class: "text-muted-foreground", "From:" }
                                            span { class: "font-mono text-xs", {truncate_pubkey(&nutzap.sender_pubkey)} }
                                        }
                                        div {
                                            class: "flex justify-between",
                                            span { class: "text-muted-foreground", "Mint:" }
                                            span { class: "font-mono text-xs", {shorten_url(&nutzap.mint_url, 25)} }
                                        }
                                        if let Some(comment) = &nutzap.comment {
                                            div {
                                                class: "pt-2 border-t border-border/50",
                                                p {
                                                    class: "text-sm italic text-muted-foreground",
                                                    "\"{comment}\""
                                                }
                                            }
                                        }
                                        if let Some(target_id) = &nutzap.target_event_id {
                                            div {
                                                class: "text-xs text-muted-foreground",
                                                "Nutzap for: {truncate_pubkey(target_id)}"
                                            }
                                        }
                                    }

                                    // Redeem button (only for pending)
                                    if is_pending {
                                        button {
                                            class: if is_redeeming {
                                                "w-full px-4 py-2 bg-orange-500 text-white font-semibold rounded-lg transition opacity-50 cursor-not-allowed"
                                            } else {
                                                "w-full px-4 py-2 bg-orange-500 hover:bg-orange-600 text-white font-semibold rounded-lg transition"
                                            },
                                            disabled: is_redeeming,
                                            onclick: move |_| on_redeem(event_id.clone()),
                                            if is_redeeming { "Redeeming..." } else { "Redeem" }
                                        }
                                    }
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
                        "Close"
                    }
                    if pending_count > 1 {
                        button {
                            class: "flex-1 px-4 py-3 bg-orange-500 hover:bg-orange-600 text-white font-semibold rounded-lg transition",
                            onclick: on_redeem_all,
                            "Redeem All ({pending_count})"
                        }
                    }
                }
            }
        }
    }
}

/// Small badge component for showing nutzap count in wallet UI
#[component]
pub fn NutzapBadge() -> Element {
    let count = cashu::pending_nutzap_count();
    let value = cashu::pending_nutzap_value();

    if count == 0 {
        return rsx! {};
    }

    rsx! {
        div {
            class: "inline-flex items-center gap-1 px-2 py-0.5 bg-orange-100 dark:bg-orange-900/30 text-orange-700 dark:text-orange-300 rounded-full text-xs font-medium",
            span { if count != 1 { "{count} nutzaps" } else { "{count} nutzap" } }
            span { class: "text-orange-500", "({value} sats)" }
        }
    }
}
