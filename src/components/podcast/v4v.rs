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
    let total_split: u32 = props.recipients.iter().map(|r| r.split).sum();
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
    total_split: u32,
}
#[component]
fn RecipientRow(props: RecipientRowProps) -> Element {
    let recipient = &props.recipient;
    let percentage = if props.total_split > 0 {
        (recipient.split as f64 / props.total_split as f64 * 100.0).round() as u32
    } else {
        0
    };
    let name = recipient
        .name
        .clone()
        .unwrap_or_else(|| {
            let addr = &recipient.address;
            if addr.len() > 20 {
                format!("{}...{}", &addr[..8], &addr[addr.len() - 8..])
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
                                                match send_v4v_payment(&vb, amt).await {
                                                    Ok(_) => {
                                                        if let Some(handler) = on_boost {
                                                            handler.call(amt);
                                                        }
                                                        log::info!("Boost sent: {} sats", amt);
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
                            Ok(_) => {
                                on_send.call(amt);
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
/// Send V4V payment split across recipients
async fn send_v4v_payment(
    value_block: &ValueBlock,
    total_sats: u64,
) -> Result<usize, String> {
    if value_block.recipients.is_empty() {
        return Err("No recipients configured".to_string());
    }
    let total_split: u32 = value_block.recipients.iter().map(|r| r.split).sum();
    if total_split == 0 {
        return Err("Invalid split configuration".to_string());
    }
    let nwc_connected = nwc_store::is_connected();
    if !nwc_connected {
        return Err(
            "No wallet connected. Please connect a wallet in settings.".to_string(),
        );
    }
    let mut success_count = 0;
    for recipient in &value_block.recipients {
        let amount = (total_sats as f64 * recipient.split as f64 / total_split as f64)
            .round() as u64;
        if amount == 0 {
            continue;
        }
        match recipient.recipient_type.as_str() {
            "lnaddress" => {
                match lnurl::get_lnurl_pay_info(Some(&recipient.address), None).await {
                    Ok(info) => {
                        let amount_msats = amount * 1000;
                        if amount_msats < info.min_sendable
                            || amount_msats > info.max_sendable
                        {
                            log::warn!(
                                "Amount {} out of range for {}", amount, recipient.address
                            );
                            continue;
                        }
                        let callback_url = format!(
                            "{}?amount={}",
                            info.callback,
                            amount_msats,
                        );
                        match http_client().get(&callback_url).send().await {
                            Ok(response) => {
                                if let Ok(invoice_response) = response
                                    .json::<serde_json::Value>()
                                    .await
                                {
                                    if let Some(pr) = invoice_response
                                        .get("pr")
                                        .and_then(|v| v.as_str())
                                    {
                                        match nwc_store::pay_invoice(pr.to_string()).await {
                                            Ok(_) => {
                                                log::info!(
                                                    "V4V payment sent: {} sats to {}", amount, recipient.address
                                                );
                                                success_count += 1;
                                            }
                                            Err(e) => {
                                                log::error!(
                                                    "Payment failed for {}: {}", recipient.address, e
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!(
                                    "Failed to get invoice from {}: {}", recipient.address, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to fetch LNURL for {}: {:?}", recipient.address, e
                        );
                    }
                }
            }
            "node" => {
                log::warn!(
                    "Keysend payments not yet supported for node: {}", recipient.address
                );
            }
            _ => {
                log::warn!("Unknown recipient type: {}", recipient.recipient_type);
            }
        }
    }
    if success_count == 0 {
        Err("All payment attempts failed".to_string())
    } else {
        Ok(success_count)
    }
}
