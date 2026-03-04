//! Zap Distribution Component
//!
//! Modal for splitting zap payments across repository contributors
//! based on configured zap_splits weights.
use crate::components::code::NostrUserPicker;
use crate::services::payments::lnurl;
use crate::stores::nwc_store;
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::truncate_pubkey;
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use futures::future::{select, Either};

/// A single recipient in the zap distribution
#[derive(Clone, Debug)]
struct ZapRecipient {
    pubkey: String,
    weight: u64,
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
    Timeout(String),
}

/// Zap distribution modal for splitting payments across contributors
#[component]
pub fn ZapDistribution(
    /// Repository zap_splits: (pubkey_hex, relay, weight)
    zap_splits: Vec<(String, String, u32)>,
    /// Repository event ID for NIP-57 zap receipt tags (TODO: wire into zap request)
    repo_event_id: String,
    /// Close callback
    on_close: EventHandler<()>,
) -> Element {
    // TODO: wire repo_event_id into NIP-57 zap request for proper zap receipts
    let _repo_event_id = repo_event_id;
    let toast = consume_toast();
    let mut total_amount = use_signal(|| 1000u64);
    let mut custom_amount = use_signal(String::new);
    let mut is_sending = use_signal(|| false);
    let mut recipients = use_signal(Vec::<ZapRecipient>::new);
    let mut send_progress = use_signal(|| 0usize);
    let mut send_total = use_signal(|| 0usize);

    // Deduplicate zap_splits by pubkey, summing weights for duplicates
    let deduped_splits: Vec<(String, u64)> = {
        let mut deduped: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for (pk, _, w) in &zap_splits {
            let entry = deduped.entry(pk.clone()).or_default();
            *entry = entry.saturating_add(*w as u64);
        }
        deduped.into_iter().collect()
    };

    // Manage selected pubkeys for the user picker
    let selected_pubkeys = use_signal({
        let deduped_splits = deduped_splits.clone();
        move || deduped_splits.iter().map(|(pk, _)| pk.clone()).collect::<Vec<String>>()
    });

    // Pure allocation: largest-remainder method to avoid rounding loss
    let compute_allocations = {
        let weight_map: std::collections::HashMap<String, u64> = deduped_splits.iter().cloned().collect();
        move |pubkeys: &[String], amount: u64| -> Vec<(String, u64, u64)> {
            let weights: Vec<u64> = pubkeys
                .iter()
                .map(|pk| weight_map.get(pk).copied().unwrap_or(1))
                .collect();
            let total_weight: u64 = weights.iter().sum();
            if total_weight == 0 || pubkeys.is_empty() {
                return pubkeys.iter().map(|pk| (pk.clone(), 0, 0)).collect();
            }
            // Compute exact shares using integer arithmetic to avoid f64 precision loss
            let mut floors: Vec<u64> = weights
                .iter()
                .map(|w| ((amount as u128) * (*w as u128) / (total_weight as u128)) as u64)
                .collect();
            let allocated: u64 = floors.iter().sum();
            let remainder = amount.saturating_sub(allocated) as usize;
            // Distribute remainder to entries with largest fractional parts (by remainder of integer division)
            let mut remainders: Vec<(usize, u128)> = weights
                .iter()
                .enumerate()
                .map(|(i, w)| (i, (amount as u128) * (*w as u128) % (total_weight as u128)))
                .collect();
            remainders.sort_by(|a, b| b.1.cmp(&a.1));
            for &(i, _) in remainders.iter().take(remainder) {
                floors[i] += 1;
            }
            pubkeys
                .iter()
                .enumerate()
                .map(|(i, pk)| (pk.clone(), weights[i], floors[i]))
                .collect()
        }
    };

    // Recalculate when amount or selection changes
    use_effect(move || {
        if *is_sending.read() {
            return;
        }
        let amount = *total_amount.read();
        let pubkeys = selected_pubkeys.read().clone();
        let allocations = compute_allocations(&pubkeys, amount);
        let current = recipients.peek().clone();
        recipients.set(
            allocations
                .into_iter()
                .map(|(pk, weight, amt)| {
                    let status = current
                        .iter()
                        .find(|r| r.pubkey == pk)
                        .filter(|r| r.status != PaymentStatus::Pending && r.amount == amt)
                        .map(|r| r.status.clone())
                        .unwrap_or(PaymentStatus::Pending);
                    ZapRecipient {
                        pubkey: pk,
                        weight,
                        amount: amt,
                        status,
                    }
                })
                .collect(),
        );
    });

    let preset_amounts = [100u64, 500, 1000, 5000, 10000, 50000];

    let handle_send = move |_| {
        if *is_sending.peek() { return; }
        // Resolve lud16 from profile cache at send time
        let recips = recipients.read().clone();
        let sendable: Vec<(ZapRecipient, String)> = recips
            .into_iter()
            .filter(|r| r.amount > 0 && r.status != PaymentStatus::Success && !matches!(r.status, PaymentStatus::Timeout(_)))
            .filter_map(|r| {
                let lud16 = PROFILE_CACHE.read().peek(&r.pubkey).and_then(|p| p.lud16.clone())?;
                Some((r, lud16))
            })
            .collect();
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
            for (i, (recip, lud16)) in sendable.iter().enumerate() {
                send_progress.set(i + 1);
                // Update status to sending
                {
                    let mut recips = recipients.write();
                    if let Some(r) = recips.iter_mut().find(|r| r.pubkey == recip.pubkey) {
                        r.status = PaymentStatus::Sending;
                    }
                }
                // Race invoice fetch against a 30s timeout
                let invoice_result = match select(
                    Box::pin(lnurl::get_invoice_from_lud16(lud16, recip.amount, None)),
                    Box::pin(crate::platform::timer::sleep_ms(30_000)),
                ).await {
                    Either::Left((Ok(inv), _)) => Ok(inv),
                    Either::Left((Err(e), _)) => Err(PaymentStatus::Failed(format!("{}", e))),
                    Either::Right(_) => Err(PaymentStatus::Failed("Invoice request timed out after 30s".to_string())),
                };
                match invoice_result {
                    Ok(invoice) => {
                        // Race payment against a 30s timeout
                        match select(
                            Box::pin(nwc_store::pay_invoice(invoice)),
                            Box::pin(crate::platform::timer::sleep_ms(30_000)),
                        ).await {
                            Either::Left((Ok(_), _)) => {
                                success_count += 1;
                                let mut recips = recipients.write();
                                if let Some(r) = recips.iter_mut().find(|r| r.pubkey == recip.pubkey) {
                                    r.status = PaymentStatus::Success;
                                }
                            }
                            Either::Left((Err(e), _)) => {
                                fail_count += 1;
                                let mut recips = recipients.write();
                                if let Some(r) = recips.iter_mut().find(|r| r.pubkey == recip.pubkey) {
                                    r.status = PaymentStatus::Failed(e);
                                }
                            }
                            Either::Right(_) => {
                                fail_count += 1;
                                let mut recips = recipients.write();
                                if let Some(r) = recips.iter_mut().find(|r| r.pubkey == recip.pubkey) {
                                    r.status = PaymentStatus::Timeout("Payment timed out after 30s".to_string());
                                }
                            }
                        }
                    }
                    Err(status) => {
                        fail_count += 1;
                        let mut recips = recipients.write();
                        if let Some(r) = recips.iter_mut().find(|r| r.pubkey == recip.pubkey) {
                            r.status = status;
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
            class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm",
            onclick: move |_| { if !*is_sending.peek() { on_close.call(()); } },
        }
        div {
            class: "fixed inset-x-4 top-[10%] z-50 max-w-lg mx-auto bg-background border border-border rounded-xl shadow-xl max-h-[80vh] overflow-y-auto",
            role: "dialog",
            aria_modal: "true",
            aria_labelledby: "distribute-zaps-title",
            onclick: move |evt| evt.stop_propagation(),

            // Header
            div { class: "p-4 border-b border-border flex items-center justify-between",
                h3 { id: "distribute-zaps-title", class: "text-lg font-semibold", "Distribute Zaps" }
                button {
                    class: if *is_sending.read() { "p-1 rounded-lg text-muted-foreground opacity-50" } else { "p-1 hover:bg-accent rounded-lg transition text-muted-foreground" },
                    aria_label: "Close",
                    r#type: "button",
                    disabled: *is_sending.read(),
                    onclick: move |_| { if !*is_sending.peek() { on_close.call(()); } },
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
                                        r#type: "button",
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
                                    PaymentStatus::Timeout(_) => "",
                                };
                                // Resolve profile in RSX path (appropriate place for PROFILE_CACHE.read())
                                let profile = PROFILE_CACHE.read().peek(&recip.pubkey).cloned();
                                let display_name = profile
                                    .as_ref()
                                    .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
                                    .unwrap_or_else(|| truncate_pubkey(&recip.pubkey));
                                let has_lud16 = profile.as_ref().and_then(|p| p.lud16.clone()).is_some();
                                let pic = profile.as_ref().and_then(|p| p.picture.clone());
                                rsx! {
                                    div {
                                        key: "{recip.pubkey}",
                                        class: "flex items-center gap-3 p-2 bg-muted rounded-lg {status_class}",
                                        // Avatar
                                        if let Some(ref pic_url) = pic.as_ref().filter(|u| is_valid_http_url(u)) {
                                            img {
                                                src: "{pic_url}",
                                                class: "w-8 h-8 rounded-full shrink-0",
                                                alt: "{display_name}",
                                                loading: "lazy",
                                            }
                                        } else {
                                            div { class: "w-8 h-8 rounded-full bg-accent flex items-center justify-center text-xs font-bold shrink-0",
                                                {truncate_pubkey(&recip.pubkey)}
                                            }
                                        }
                                        // Name + weight
                                        div { class: "flex-1 min-w-0",
                                            div { class: "text-sm font-medium truncate", "{display_name}" }
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
                                            PaymentStatus::Timeout(msg) => rsx! {
                                                span { class: "text-xs text-orange-500", title: "{msg}", "Timed out" }
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
                        // NostrUserPicker mutates `selected_pubkeys` signal directly;
                        // the use_effect above reacts to changes. on_change is a no-op placeholder.
                        on_change: move |_new: Vec<String>| {},
                    }
                }

                // Send button
                div { class: "pt-2",
                    button {
                        class: "w-full py-2.5 text-sm font-medium bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50",
                        r#type: "button",
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
