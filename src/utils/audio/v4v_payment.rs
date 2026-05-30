use crate::platform::http::http_client;
use crate::services::lnurl;
use crate::stores::nwc_store;
use crate::utils::audio::blip10::{build_blip10_json, build_custom_records, BoostMetadata};
use crate::utils::podcast::{ValueBlock, ValueRecipient};
use nwc::prelude::KeysendTLVRecord;
use url::Url;

#[derive(Clone, Debug, PartialEq)]
pub struct RecipientSplit {
    pub recipient: ValueRecipient,
    pub amount_sats: u64,
    pub percentage: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RecipientPaymentStatus {
    Pending,
    Paying,
    Success,
    Failed(String),
    Skipped(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaymentOutcome {
    FullSuccess,
    PartialSuccess {
        success_count: usize,
        attempted_count: usize,
        failed_recipients: Vec<String>,
    },
    NoAttempts,
}

pub struct BoostContext {
    pub podcast_name: Option<String>,
    pub episode_name: Option<String>,
    pub feed_guid: Option<String>,
    pub episode_guid: Option<String>,
    pub feed_url: Option<String>,
    pub sender_name: Option<String>,
    pub message: Option<String>,
}

pub fn calculate_splits(value_block: &ValueBlock, total_sats: u64) -> Vec<RecipientSplit> {
    if value_block.recipients.is_empty() || total_sats == 0 {
        return Vec::new();
    }
    let total_split: u64 = value_block
        .recipients
        .iter()
        .try_fold(0u64, |acc, r| acc.checked_add(r.split as u64))
        .unwrap_or(0);
    if total_split == 0 {
        return Vec::new();
    }
    let mut splits = Vec::with_capacity(value_block.recipients.len());
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
        let percentage = if total_split > 0 {
            (recipient.split as f64 / total_split as f64 * 100.0).round() as u32
        } else {
            0
        };
        splits.push(RecipientSplit {
            recipient: recipient.clone(),
            amount_sats: amount,
            percentage,
        });
    }
    splits
}

fn build_boost_metadata(
    split: &RecipientSplit,
    total_sats: u64,
    ctx: &BoostContext,
) -> BoostMetadata {
    BoostMetadata {
        action: "boost",
        value_msat_total: total_sats * 1000,
        value_msat: split.amount_sats * 1000,
        app_name: Some("nostr.blue".to_string()),
        app_version: None,
        sender_name: ctx.sender_name.clone(),
        message: ctx.message.clone(),
        podcast: ctx.podcast_name.clone(),
        episode: ctx.episode_name.clone(),
        guid: ctx.feed_guid.clone(),
        episode_guid: ctx.episode_guid.clone(),
        url: ctx.feed_url.clone(),
        feed_id: None,
        ts: None,
        time: None,
        recipient_name: split.recipient.name.clone(),
    }
}

async fn resolve_lnurl_invoice(address: &str, amount_sats: u64) -> Result<String, String> {
    let info = lnurl::get_lnurl_pay_info(Some(address), None)
        .await
        .map_err(|e| format!("LNURL resolution failed for {}: {:?}", address, e))?;
    let amount_msats = amount_sats * 1000;
    if amount_msats < info.min_sendable || amount_msats > info.max_sendable {
        return Err(format!(
            "Amount {} out of range for {} (min {} max {})",
            amount_sats, address, info.min_sendable / 1000, info.max_sendable / 1000
        ));
    }
    let callback_url = Url::parse(&info.callback)
        .map_err(|e| format!("Invalid callback URL for {}: {}", address, e))?;
    let mut callback_url = callback_url;
    callback_url
        .query_pairs_mut()
        .append_pair("amount", &amount_msats.to_string());
    let client = http_client()
        .map_err(|e| format!("HTTP client init failed for {}: {}", address, e))?;
    let response = client
        .get(callback_url.as_str())
        .send()
        .await
        .map_err(|e| format!("Invoice request failed for {}: {}", address, e))?;
    let raw_response = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response for {}: {}", address, e))?;
    let invoice_response: serde_json::Value = serde_json::from_str(&raw_response)
        .map_err(|e| format!("Failed to parse invoice response for {}: {}", address, e))?;
    if let Some(error) = invoice_response.get("status").and_then(|v| v.as_str()) {
        if error.to_uppercase() == "ERROR" {
            let reason = invoice_response
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(format!("Invoice error for {}: {}", address, reason));
        }
    }
    let pr = invoice_response
        .get("pr")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("No invoice in response for {}", address))?;
    let expected_msats = amount_sats * 1000;
    match crate::utils::bolt11::parse_bolt11_amount(pr) {
        Some(parsed) if parsed == expected_msats => Ok(pr.to_string()),
        Some(parsed) => Err(format!(
            "Bolt11 amount mismatch for {}: expected {} msats, got {} msats",
            address, expected_msats, parsed
        )),
        None => Err(format!(
            "Could not parse bolt11 amount for {}",
            address
        )),
    }
}

pub async fn pay_recipient_via_nwc(
    split: &RecipientSplit,
    total_sats: u64,
    ctx: &BoostContext,
) -> Result<(), String> {
    let meta = build_boost_metadata(split, total_sats, ctx);
    match split.recipient.recipient_type.as_str() {
        "lnaddress" => {
            let invoice =
                resolve_lnurl_invoice(&split.recipient.address, split.amount_sats).await?;
            nwc_store::pay_invoice(invoice).await?;
            log::info!(
                "V4V payment sent: {} sats to {} (lnaddress)",
                split.amount_sats,
                split.recipient.address
            );
            Ok(())
        }
        "node" => {
            let blip_json = build_blip10_json(&meta);
            let custom_records = build_custom_records(
                &blip_json,
                split.recipient.custom_key.as_deref(),
                split.recipient.custom_value.as_deref(),
            );
            let tlv_records: Vec<KeysendTLVRecord> = custom_records
                .into_iter()
                .map(|r| KeysendTLVRecord {
                    tlv_type: r.tlv_type,
                    value: r.value,
                })
                .collect();
            let amount_msats = split.amount_sats * 1000;
            nwc_store::pay_keysend(
                split.recipient.address.clone(),
                amount_msats,
                tlv_records,
            )
            .await?;
            log::info!(
                "V4V keysend sent: {} sats to {} (node)",
                split.amount_sats,
                split.recipient.address
            );
            Ok(())
        }
        _ => Err(format!(
            "Unknown recipient type: {}",
            split.recipient.recipient_type
        )),
    }
}

#[allow(dead_code)]
pub async fn generate_recipient_invoice(split: &RecipientSplit) -> Result<String, String> {
    match split.recipient.recipient_type.as_str() {
        "lnaddress" => resolve_lnurl_invoice(&split.recipient.address, split.amount_sats).await,
        "node" => Err(format!(
            "Keysend to node {} requires a wallet connection (NWC)",
            split.recipient.address
        )),
        _ => Err(format!(
            "Unknown recipient type: {}",
            split.recipient.recipient_type
        )),
    }
}

pub async fn execute_v4v_boost(
    value_block: &ValueBlock,
    total_sats: u64,
    ctx: &BoostContext,
) -> Result<PaymentOutcome, String> {
    if value_block.recipients.is_empty() {
        return Err("No recipients configured".to_string());
    }
    let splits = calculate_splits(value_block, total_sats);
    if splits.is_empty() {
        return Err("No valid splits calculated".to_string());
    }
    let mut success_count = 0;
    let mut attempted_count = 0;
    let mut failed_recipients = Vec::new();
    for split in &splits {
        if split.amount_sats == 0 {
            continue;
        }
        attempted_count += 1;
        match pay_recipient_via_nwc(split, total_sats, ctx).await {
            Ok(()) => success_count += 1,
            Err(e) => {
                log::error!(
                    "V4V payment failed for {} ({}): {}",
                    split.recipient.address,
                    split.recipient.recipient_type,
                    e
                );
                failed_recipients.push(split.recipient.address.clone());
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
