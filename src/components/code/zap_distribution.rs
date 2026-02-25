//! Zap Distribution Component
//!
//! Modal for splitting zap payments across repository contributors
//! based on configured zap_splits weights.
use crate::components::code::NostrUserPicker;
use crate::services::payments::lnurl;
use crate::stores::nwc_store;
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};

/// A single recipient in the zap distribution
#[derive(Clone, Debug)]
struct ZapRecipient {
    pubkey: String,
    display_name: String,
    lud16: Option<String>,
    weight: u32,
    /// Calculated amount in sats
    amount: u64,
    /// Payment status
    status: PaymentStatus,
}

#[derive(Clone, Debug, PartialEq)]
enum PaymentStatus {
    Pending,
    Sending,
    Success,
    Failed(String),
}

/// Zap distribution modal for splitting payments across contributors
#[component]
pub fn ZapDistribution(
    /// Repository zap_splits: (pubkey_hex, relay, weight)
    zap_splits: Vec<(String, String, u32)>,
    /// Repository event ID for zap tags
    repo_event_id: String,
    /// Close callback
    on_close: EventHandler<()>,
) -> Element {
    let toast = consume_toast();
    let mut total_amount = use_signal(|| 1000u64);
    let mut custom_amount = use_signal(String::new);
    let mut is_sending = use_signal(|| false);
    let mut recipients = use_signal(Vec::<ZapRecipient>::new);
    let mut send_progress = use_signal(|| 0usize);
    let mut send_total = use_signal(|| 0usize);

    // Manage selected pubkeys for the user picker
    let selected_pubkeys = use_signal(|| {
        zap_splits.iter().map(|(pk, _, _)| pk.clone()).collect::<Vec<String>>()
    });

    // Build recipients from zap_splits + any additional picks
    let build_recipients = move |amount: u64| {
        let pubkeys = selected_pubkeys.read().clone();
        // Find weights for known splits, default weight 1 for newly added
        let total_weight: u32 = pubkeys
            .iter()
            .map(|pk| {
                zap_splits
                    .iter()
                    .find(|(p, _, _)| p == pk)
                    .map(|(_, _, w)| *w)
                    .unwrap_or(1)
            })
            .sum();

        let recips: Vec<ZapRecipient> = pubkeys
            .iter()
            .map(|pk| {
                let weight = zap_splits
                    .iter()
                    .find(|(p, _, _)| p == pk)
                    .map(|(_, _, w)| *w)
                    .unwrap_or(1);
                let profile = PROFILE_CACHE.read().peek(pk).cloned();
                let display_name = profile
                    .as_ref()
                    .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
                    .unwrap_or_else(|| truncate_pubkey(pk));
                let lud16 = profile.as_ref().and_then(|p| p.lud16.clone());
                let per_amount = if total_weight > 0 {
                    (amount as f64 * weight as f64 / total_weight as f64).round() as u64
                } else {
                    0
                };
                ZapRecipient {
                    pubkey: pk.clone(),
                    display_name,
                    lud16,
                    weight,
                    amount: per_amount,
                    status: PaymentStatus::Pending,
                }
            })
            .collect();
        recips
    };

    // Recalculate when amount or selection changes
    use_effect(move || {
        let amount = *total_amount.read();
        let _sel = selected_pubkeys.read();
        recipients.set(build_recipients(amount));
    });

    let preset_amounts = [100u64, 500, 1000, 5000, 10000, 50000];

    let handle_send = move |_| {
        let recips = recipients.read().clone();
        let sendable: Vec<ZapRecipient> = recips.into_iter().filter(|r| r.lud16.is_some() && r.amount > 0).collect();
        if sendable.is_empty() {
            toast.warning("No recipients with Lightning addresses found".to_string(), ToastOptions::new());
            return;
        }
        is_sending.set(true);
        send_progress.set(0);
        send_total.set(sendable.len());
        spawn(async move {
            let mut success_count = 0usize;
            let mut fail_count = 0usize;
            for (i, recip) in sendable.iter().enumerate() {
                send_progress.set(i + 1);
                // Update status to sending
                {
                    let mut recips = recipients.write();
                    if let Some(r) = recips.iter_mut().find(|r| r.pubkey == recip.pubkey) {
                        r.status = PaymentStatus::Sending;
                    }
                }
                let lud16 = recip.lud16.as_deref().unwrap_or("");
                match lnurl::get_invoice_from_lud16(lud16, recip.amount, None).await {
                    Ok(invoice) => {
                        match nwc_store::pay_invoice(invoice).await {
                            Ok(_) => {
                                success_count += 1;
                                let mut recips = recipients.write();
                                if let Some(r) = recips.iter_mut().find(|r| r.pubkey == recip.pubkey) {
                                    r.status = PaymentStatus::Success;
                                }
                            }
                            Err(e) => {
                                fail_count += 1;
                                let mut recips = recipients.write();
                                if let Some(r) = recips.iter_mut().find(|r| r.pubkey == recip.pubkey) {
                                    r.status = PaymentStatus::Failed(e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        fail_count += 1;
                        let mut recips = recipients.write();
                        if let Some(r) = recips.iter_mut().find(|r| r.pubkey == recip.pubkey) {
                            r.status = PaymentStatus::Failed(format!("{}", e));
                        }
                    }
                }
            }
            is_sending.set(false);
            if fail_count == 0 {
                toast.success(format!("Zapped {} recipients!", success_count), ToastOptions::new());
            } else {
                toast.warning(format!("{} sent, {} failed", success_count, fail_count), ToastOptions::new());
            }
        });
    };

    let nwc_connected = nwc_store::NWC_CLIENT.read().is_some();

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 z-40 bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),
        }
        div {
            class: "fixed inset-x-4 top-[10%] z-50 max-w-lg mx-auto bg-background border border-border rounded-xl shadow-xl max-h-[80vh] overflow-y-auto",
            onclick: move |evt| evt.stop_propagation(),

            // Header
            div { class: "p-4 border-b border-border flex items-center justify-between",
                h3 { class: "text-lg font-semibold", "Distribute Zaps" }
                button {
                    class: "p-1 hover:bg-accent rounded-lg transition text-muted-foreground",
                    onclick: move |_| on_close.call(()),
                    svg {
                        class: "w-5 h-5",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        line { x1: "18", y1: "6", x2: "6", y2: "18" }
                        line { x1: "6", y1: "6", x2: "18", y2: "18" }
                    }
                }
            }

            div { class: "p-4 space-y-4",
                // NWC connection warning
                if !nwc_connected {
                    div { class: "p-3 rounded-lg bg-orange-500/10 border border-orange-500/20 text-sm text-orange-500",
                        "Connect a Nostr Wallet (NWC) in Settings to send zaps."
                    }
                }

                // Amount selection
                div { class: "space-y-2",
                    label { class: "text-sm font-medium text-foreground", "Total Amount (sats)" }
                    div { class: "flex flex-wrap gap-2",
                        for amt in preset_amounts.iter() {
                            {
                                let amt_val = *amt;
                                let is_selected = *total_amount.read() == amt_val;
                                rsx! {
                                    button {
                                        key: "{amt_val}",
                                        class: if is_selected {
                                            "px-3 py-1.5 text-sm rounded-lg bg-primary text-primary-foreground font-medium"
                                        } else {
                                            "px-3 py-1.5 text-sm rounded-lg bg-muted hover:bg-accent transition"
                                        },
                                        onclick: move |_| {
                                            total_amount.set(amt_val);
                                            custom_amount.set(String::new());
                                        },
                                        "{amt_val}"
                                    }
                                }
                            }
                        }
                    }
                    input {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "number",
                        placeholder: "Custom amount...",
                        value: "{custom_amount}",
                        oninput: move |e| {
                            let val = e.value();
                            custom_amount.set(val.clone());
                            if let Ok(n) = val.parse::<u64>() {
                                if n > 0 {
                                    total_amount.set(n);
                                }
                            }
                        },
                    }
                }

                // Recipients
                div { class: "space-y-2",
                    label { class: "text-sm font-medium text-foreground", "Recipients" }
                    div { class: "space-y-2",
                        for recip in recipients.read().iter() {
                            {
                                let status_class = match &recip.status {
                                    PaymentStatus::Pending => "",
                                    PaymentStatus::Sending => "opacity-70",
                                    PaymentStatus::Success => "",
                                    PaymentStatus::Failed(_) => "",
                                };
                                let has_lud16 = recip.lud16.is_some();
                                rsx! {
                                    div {
                                        key: "{recip.pubkey}",
                                        class: "flex items-center gap-3 p-2 bg-muted rounded-lg {status_class}",
                                        // Avatar
                                        {
                                            let profile = PROFILE_CACHE.read().peek(&recip.pubkey).cloned();
                                            let pic = profile.as_ref().and_then(|p| p.picture.clone());
                                            rsx! {
                                                if let Some(ref pic_url) = pic {
                                                    img {
                                                        src: "{pic_url}",
                                                        class: "w-8 h-8 rounded-full shrink-0",
                                                        alt: "{recip.display_name}",
                                                        loading: "lazy",
                                                    }
                                                } else {
                                                    div { class: "w-8 h-8 rounded-full bg-accent flex items-center justify-center text-xs font-bold shrink-0",
                                                        {recip.display_name.chars().next().unwrap_or('?').to_string()}
                                                    }
                                                }
                                            }
                                        }
                                        // Name + weight
                                        div { class: "flex-1 min-w-0",
                                            div { class: "text-sm font-medium truncate", "{recip.display_name}" }
                                            div { class: "text-xs text-muted-foreground",
                                                if has_lud16 {
                                                    "Weight: {recip.weight} · {recip.amount} sats"
                                                } else {
                                                    "No Lightning address"
                                                }
                                            }
                                        }
                                        // Status indicator
                                        match &recip.status {
                                            PaymentStatus::Pending => rsx! {
                                                span { class: "text-xs text-muted-foreground", "{recip.amount} sats" }
                                            },
                                            PaymentStatus::Sending => rsx! {
                                                span { class: "text-xs text-yellow-500 animate-pulse", "Sending..." }
                                            },
                                            PaymentStatus::Success => rsx! {
                                                svg {
                                                    class: "w-5 h-5 text-green-500",
                                                    xmlns: "http://www.w3.org/2000/svg",
                                                    width: "24",
                                                    height: "24",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    polyline { points: "20 6 9 17 4 12" }
                                                }
                                            },
                                            PaymentStatus::Failed(msg) => rsx! {
                                                span { class: "text-xs text-destructive", title: "{msg}", "Failed" }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Add recipient
                div { class: "space-y-1",
                    label { class: "text-xs text-muted-foreground", "Add recipients" }
                    NostrUserPicker {
                        selected: selected_pubkeys,
                        placeholder: "Search or paste npub...".to_string(),
                        max_selections: 0,
                        on_change: move |_new: Vec<String>| {},
                    }
                }

                // Send button
                div { class: "pt-2",
                    button {
                        class: "w-full py-2.5 text-sm font-medium bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50",
                        disabled: !nwc_connected || *is_sending.read() || recipients.read().is_empty(),
                        onclick: handle_send,
                        if *is_sending.read() {
                            "Sending {send_progress}/{send_total}..."
                        } else {
                            "Zap {total_amount} sats"
                        }
                    }
                }
            }
        }
    }
}
