//! Podcast Value 4 Value (V4V) Component
//!
//! Handles V4V Lightning payments for podcasts with:
//! - Boost payments with custom amounts
//! - Split payments to multiple recipients
//! - Support for both node pubkeys and Lightning Addresses
use crate::components::icons;
use crate::platform::http::http_client;
use crate::services::lnurl;
use crate::stores::nwc_store;
use crate::utils::podcast::{ValueBlock, ValueRecipient};
use dioxus::prelude::*;
use url::Url;
#[derive(Props, Clone, PartialEq)]
pub struct V4VInfoProps {
    /// The V4V value block
    pub value_block: ValueBlock,
    /// Show detailed recipient breakdown
    #[props(default = false)]
    pub show_details: bool,
}
/// Display V4V payment info for a podcast
#[component]
pub fn V4VInfo(props: V4VInfoProps) -> Element {
    let value_block = &props.value_block;
    if value_block.recipients.is_empty() {
        return rsx! {};
    }
    let suggested = value_block.suggested.unwrap_or(0.0);
    let recipient_count = value_block.recipients.len();
    rsx! {
        div { class: "space-y-3",
            div { class: "flex items-center gap-2",
                div { class: "flex items-center gap-1.5 px-2 py-1 rounded-full bg-amber-500/10 text-amber-600 text-xs font-medium",
                    span {
                        class: "w-3.5 h-3.5",
                        dangerous_inner_html: icons::ZAP,
                    }
                    "Value 4 Value"
                }
                if suggested > 0.0 {
                    span { class: "text-xs text-muted-foreground",
                        "Suggested: {suggested as u64} sats/min"
                    }
                }
            }
            if props.show_details {
                V4VRecipientList { recipients: value_block.recipients.clone() }
            } else {
                {
                    let suffix = if recipient_count != 1 { "s" } else { "" };
                    rsx! {
                        div { class: "text-sm text-muted-foreground",
                            "Support {recipient_count} creator{suffix} directly with Lightning"
                        }
                    }
                }
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
pub struct V4VRecipientListProps {
    /// Recipients to display
    pub recipients: Vec<ValueRecipient>,
}
/// Display V4V payment recipients with their splits
#[component]
pub fn V4VRecipientList(props: V4VRecipientListProps) -> Element {
    if props.recipients.is_empty() {
        return rsx! {};
    }
    let total_split: u64 = props
        .recipients
        .iter()
        .try_fold(0u64, |acc, r| acc.checked_add(r.split as u64))
        .unwrap_or(0);
    rsx! {
        div { class: "space-y-2",
            div { class: "text-xs font-medium text-muted-foreground uppercase tracking-wide",
                "Payment Recipients"
            }
            div { class: "space-y-1",
                for (idx , recipient) in props.recipients.iter().enumerate() {
                    RecipientRow {
                        key: "{idx}",
                        recipient: recipient.clone(),
                        total_split,
                    }
                }
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
struct RecipientRowProps {
    recipient: ValueRecipient,
    total_split: u64,
}
#[component]
fn RecipientRow(props: RecipientRowProps) -> Element {
    let recipient = &props.recipient;
    let percentage = if props.total_split > 0 {
        (recipient.split as f64 / props.total_split as f64 * 100.0).round() as u32
    } else {
        0
    };
    let name = recipient.name.clone().unwrap_or_else(|| {
        let addr = &recipient.address;
        let char_count = addr.chars().count();
        if char_count > 20 {
            let first_8: String = addr.chars().take(8).collect();
            let last_8: String = addr
                .chars()
                .rev()
                .take(8)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            format!("{}...{}", first_8, last_8)
        } else {
            addr.clone()
        }
    });
    let type_icon = if recipient.recipient_type == "lnaddress" {
        icons::AT_SIGN
    } else {
        icons::ZAP
    };
    let is_fee = recipient.fee.unwrap_or(false);
    rsx! {
        div { class: "flex items-center gap-2 p-2 rounded-lg bg-muted/50",
            span {
                class: if is_fee { "w-4 h-4 text-muted-foreground" } else { "w-4 h-4 text-amber-500" },
                dangerous_inner_html: type_icon,
            }
            div { class: "flex-1 min-w-0",
                span { class: if is_fee { "text-sm text-muted-foreground truncate block" } else { "text-sm font-medium truncate block" },
                    "{name}"
                }
                if is_fee {
                    span { class: "text-xs text-muted-foreground", "(Processing fee)" }
                }
            }
            div { class: "flex items-center gap-1",
                div { class: "w-16 h-1.5 rounded-full bg-muted overflow-hidden",
                    div {
                        class: "h-full bg-amber-500 rounded-full",
                        style: "width: {percentage}%",
                    }
                }
                span { class: "text-xs text-muted-foreground font-mono w-8 text-right",
                    "{percentage}%"
                }
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
pub struct V4VBoostButtonProps {
    /// The V4V value block
    pub value_block: ValueBlock,
    /// Preset boost amounts
    #[props(default = vec![100, 500, 1000, 5000])]
    pub presets: Vec<u64>,
    /// Callback after successful boost
    #[props(default)]
    pub on_boost: Option<EventHandler<u64>>,
}
/// Boost button with quick amounts for V4V payments
#[component]
pub fn V4VBoostButton(props: V4VBoostButtonProps) -> Element {
    let mut show_menu = use_signal(|| false);
    let mut is_sending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    if props.value_block.recipients.is_empty() {
        return rsx! {};
    }
    let value_block = props.value_block.clone();
    rsx! {
        div { class: "relative",
            button {
                class: "flex items-center gap-2 px-4 py-2 rounded-full bg-amber-500 hover:bg-amber-600 text-white font-medium transition disabled:opacity-50",
                disabled: *is_sending.read(),
                onclick: move |_| {
                    let current = *show_menu.read();
                    show_menu.set(!current);
                },
                if *is_sending.read() {
                    span {
                        class: "w-4 h-4 animate-spin",
                        dangerous_inner_html: icons::LOADER,
                    }
                } else {
                    span { class: "w-4 h-4", dangerous_inner_html: icons::ZAP }
                }
                "Boost"
            }
            if *show_menu.read() {
                div {
                    class: "absolute bottom-full left-0 mb-2 p-2 rounded-lg bg-popover border border-border shadow-lg min-w-[200px] z-50",
                    onclick: move |e| e.stop_propagation(),
                    div { class: "grid grid-cols-2 gap-2 mb-2",
                        for amount in props.presets.iter() {
                            {
                                let amt = *amount;
                                let vb = value_block.clone();
                                let on_boost = props.on_boost;
                                rsx! {
                                    button {
                                        key: "{amt}",
                                        class: "px-3 py-2 rounded-lg bg-muted hover:bg-muted/80 text-sm font-medium transition",
                                        onclick: move |_| {
                                            let vb = vb.clone();
                                            let on_boost = on_boost;
                                            show_menu.set(false);
                                            is_sending.set(true);
                                            spawn(async move {
                                                error.set(None);
                                                match send_v4v_payment(&vb, amt).await {
                                                    Ok(PaymentOutcome::FullSuccess) => {
                                                        if let Some(handler) = on_boost {
                                                            handler.call(amt);
                                                        }
                                                        log::info!("Boost sent: {} sats", amt);
                                                    }
                                                    Ok(PaymentOutcome::NoAttempts) => {
                                                        error.set(Some("No payment attempts: all computed amounts were zero".to_string()));
                                                    }
                                                    Ok(PaymentOutcome::PartialSuccess {
                                                        success_count,
                                                        attempted_count,
                                                        failed_recipients,
                                                    }) => {
                                                        error.set(Some(format!(
                                                            "Sent {}/{} payments. Failed: {}",
                                                            success_count,
                                                            attempted_count,
                                                            failed_recipients.join(", ")
                                                        )));
                                                    }
                                                    Err(e) => {
                                                        error.set(Some(e));
                                                    }
                                                }
                                                is_sending.set(false);
                                            });
                                        },
                                        "{amt} sats"
                                    }
                                }
                            }
                        }
                    }
                    div { class: "pt-2 border-t border-border",
                        CustomBoostInput {
                            value_block: value_block.clone(),
                            on_send: move |amt| {
                                show_menu.set(false);
                                if let Some(handler) = &props.on_boost {
                                    handler.call(amt);
                                }
                            },
                        }
                    }
                }
            }
            if let Some(ref err) = *error.read() {
                div { class: "absolute top-full left-0 mt-2 p-2 rounded-lg bg-destructive/10 text-destructive text-xs max-w-[200px]",
                    "{err}"
                }
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
struct CustomBoostInputProps {
    value_block: ValueBlock,
    on_send: EventHandler<u64>,
}
#[component]
fn CustomBoostInput(props: CustomBoostInputProps) -> Element {
    let mut amount = use_signal(String::new);
    let mut is_sending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let handle_send = {
        let vb = props.value_block.clone();
        let on_send = props.on_send;
        move |_| {
            if let Ok(amt) = amount.read().parse::<u64>() {
                if amt > 0 {
                    let vb = vb.clone();
                    let on_send = on_send;
                    error.set(None);
                    is_sending.set(true);
                    spawn(async move {
                        match send_v4v_payment(&vb, amt).await {
                            Ok(PaymentOutcome::FullSuccess) => {
                                on_send.call(amt);
                            }
                            Ok(PaymentOutcome::NoAttempts) => {
                                error.set(Some(
                                    "No payment attempts: all computed amounts were zero"
                                        .to_string(),
                                ));
                            }
                            Ok(PaymentOutcome::PartialSuccess {
                                success_count,
                                attempted_count,
                                failed_recipients,
                            }) => {
                                error.set(Some(format!(
                                    "Sent {}/{} payments. Failed: {}",
                                    success_count,
                                    attempted_count,
                                    failed_recipients.join(", ")
                                )));
                            }
                            Err(e) => {
                                error.set(Some(e));
                            }
                        }
                        is_sending.set(false);
                    });
                }
            }
        }
    };
    rsx! {
        div { class: "flex gap-2 relative",
            input {
                r#type: "number",
                placeholder: "Custom sats",
                class: "flex-1 px-3 py-2 rounded-lg bg-muted text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                value: "{amount}",
                oninput: move |e| amount.set(e.value()),
            }
            button {
                class: "px-3 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium disabled:opacity-50",
                disabled: *is_sending.read() || amount.read().parse::<u64>().unwrap_or(0) == 0,
                onclick: handle_send,
                if *is_sending.read() {
                    span {
                        class: "w-4 h-4 animate-spin",
                        dangerous_inner_html: icons::LOADER,
                    }
                } else {
                    "Send"
                }
            }
            if let Some(ref err) = *error.read() {
                div { class: "absolute top-full left-0 mt-2 p-2 rounded-lg bg-destructive/10 text-destructive text-xs max-w-[200px]",
                    "{err}"
                }
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
pub struct V4VBadgeProps {
    /// Has V4V configuration
    #[props(default = true)]
    pub enabled: bool,
}
/// Small V4V indicator badge
#[component]
pub fn V4VBadge(props: V4VBadgeProps) -> Element {
    if !props.enabled {
        return rsx! {};
    }
    rsx! {
        div {
            class: "inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-xs bg-amber-500/10 text-amber-600",
            title: "Supports Value 4 Value payments",
            span { class: "w-3 h-3", dangerous_inner_html: icons::ZAP }
            "V4V"
        }
    }
}

enum PaymentOutcome {
    FullSuccess,
    PartialSuccess {
        success_count: usize,
        attempted_count: usize,
        failed_recipients: Vec<String>,
    },
    NoAttempts,
}
/// Send V4V payment split across recipients
async fn send_v4v_payment(
    value_block: &ValueBlock,
    total_sats: u64,
) -> Result<PaymentOutcome, String> {
    if value_block.recipients.is_empty() {
        return Err("No recipients configured".to_string());
    }
    let total_split: u64 = value_block
        .recipients
        .iter()
        .try_fold(0u64, |acc, r| acc.checked_add(r.split as u64))
        .ok_or_else(|| "Overflow in split total".to_string())?;
    if total_split == 0 {
        return Err("Zero total_split: invalid V4V configuration".to_string());
    }
    let mut success_count = 0;
    let mut attempted_count = 0;
    let mut failed_recipients = Vec::new();
    let mut remaining_sats = total_sats;
    let mut remaining_split = total_split;
    let recipients_len = value_block.recipients.len();
    for (idx, recipient) in value_block.recipients.iter().enumerate() {
        let amount = if idx == recipients_len - 1 {
            remaining_sats
        } else {
            let rem_sats = remaining_sats as f64;
            let rem_split = remaining_split as f64;
            let recipient_split = recipient.split as f64;
            (rem_sats * recipient_split / rem_split).round() as u64
        };
        remaining_sats = remaining_sats.saturating_sub(amount);
        remaining_split = remaining_split.saturating_sub(recipient.split as u64);
        if amount == 0 {
            continue;
        }
        attempted_count += 1;
        match recipient.recipient_type.as_str() {
            "lnaddress" => match lnurl::get_lnurl_pay_info(Some(&recipient.address), None).await {
                Ok(info) => {
                    let amount_msats = amount * 1000;
                    if amount_msats < info.min_sendable || amount_msats > info.max_sendable {
                        log::warn!("Amount {} out of range for {}", amount, recipient.address);
                        failed_recipients.push(recipient.address.clone());
                        continue;
                    }
                    let callback_url = match Url::parse(&info.callback) {
                        Ok(mut url) => {
                            url.query_pairs_mut()
                                .append_pair("amount", &amount_msats.to_string());
                            url.to_string()
                        }
                        Err(e) => {
                            log::error!(
                                "Failed to parse callback URL for {}: {}",
                                recipient.address,
                                e
                            );
                            failed_recipients.push(recipient.address.clone());
                            continue;
                        }
                    };
                    let client = match http_client() {
                        Ok(client) => client,
                        Err(e) => {
                            log::error!(
                                "Failed to initialize HTTP client for {}: {}",
                                recipient.address,
                                e
                            );
                            failed_recipients.push(recipient.address.clone());
                            continue;
                        }
                    };
                    match client.get(&callback_url).send().await {
                        Ok(response) => {
                            let raw_response = match response.text().await {
                                Ok(body) => body,
                                Err(e) => {
                                    log::error!(
                                        "Failed to read invoice response body for {}: {}",
                                        recipient.address,
                                        e
                                    );
                                    failed_recipients.push(recipient.address.clone());
                                    continue;
                                }
                            };
                            let parse_result =
                                serde_json::from_str::<serde_json::Value>(&raw_response);
                            match parse_result {
                                Ok(invoice_response) => {
                                    if let Some(pr) =
                                        invoice_response.get("pr").and_then(|v| v.as_str())
                                    {
                                        let expected_amount_msats = amount * 1000;
                                        match crate::utils::bolt11::parse_bolt11_amount(pr) {
                                            Some(parsed_amount) => {
                                                if parsed_amount != expected_amount_msats {
                                                    log::error!(
                                                        "Bolt11 amount mismatch for {}: expected {} msats, got {} msats",
                                                        recipient.address,
                                                        expected_amount_msats,
                                                        parsed_amount
                                                    );
                                                    failed_recipients
                                                        .push(recipient.address.clone());
                                                    continue;
                                                }
                                            }
                                            None => {
                                                log::error!(
                                                    "Could not parse bolt11 amount for {}, rejecting payment",
                                                    recipient.address
                                                );
                                                failed_recipients.push(recipient.address.clone());
                                                continue;
                                            }
                                        }
                                        match nwc_store::pay_invoice(pr.to_string()).await {
                                            Ok(_) => {
                                                log::info!(
                                                    "V4V payment sent: {} sats to {}",
                                                    amount,
                                                    recipient.address
                                                );
                                                success_count += 1;
                                            }
                                            Err(e) => {
                                                log::error!(
                                                    "Payment failed for {}: {}",
                                                    recipient.address,
                                                    e
                                                );
                                                failed_recipients.push(recipient.address.clone());
                                            }
                                        }
                                    } else {
                                        log::error!(
                                            "Invoice response missing pr for {}: {}",
                                            recipient.address,
                                            raw_response
                                        );
                                        failed_recipients.push(recipient.address.clone());
                                    }
                                }
                                Err(parse_error) => {
                                    log::error!(
                                        "Failed to parse invoice response for {}: {}. Raw response: {}",
                                        recipient.address,
                                        parse_error,
                                        raw_response
                                    );
                                    failed_recipients.push(recipient.address.clone());
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to get invoice from {}: {}", recipient.address, e);
                            failed_recipients.push(recipient.address.clone());
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch LNURL for {}: {:?}", recipient.address, e);
                    failed_recipients.push(recipient.address.clone());
                }
            },
            "node" => {
                log::warn!(
                    "Keysend payments not yet supported for node: {}",
                    recipient.address
                );
                failed_recipients.push(recipient.address.clone());
            }
            _ => {
                log::warn!("Unknown recipient type: {}", recipient.recipient_type);
                failed_recipients.push(recipient.address.clone());
            }
        }
    }
    if attempted_count == 0 {
        Ok(PaymentOutcome::NoAttempts)
    } else if success_count == 0 {
        Err("All payment attempts failed".to_string())
    } else if success_count < attempted_count {
        Ok(PaymentOutcome::PartialSuccess {
            success_count,
            attempted_count,
            failed_recipients,
        })
    } else {
        Ok(PaymentOutcome::FullSuccess)
    }
}
