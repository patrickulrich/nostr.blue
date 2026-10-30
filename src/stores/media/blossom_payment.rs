//! BUD-07 payment handling: 402 `Payment Required` flows for Blossom
//! uploads/downloads (X-Lightning / X-Cashu).
//!
//! Architecture: when a preflight or request surfaces a payment challenge,
//! the failing task publishes a [`PaymentPrompt`] on a global signal and
//! *awaits* a matching [`BlossomPaymentProof`] (submitted by the payment
//! modal once the user pays), then transparently retries with the proof.
//! This keeps every existing upload/download call site unchanged.

use std::collections::HashMap;
use std::str::FromStr;

use base64::Engine as _;
use dioxus::prelude::*;
use nostr_blossom::client::{BlossomPaymentProof, BlossomPaymentRequest};

use crate::stores::media::blossom_uri::sanitize_server_text;

/// Hard cap on automatic payments — refused before any wallet interaction
/// (Amethyst's `MAX_PAYMENT_SATS` rail).
pub const MAX_PAYMENT_SATS: u64 = 10_000;

/// How long a pending operation waits for the user to complete payment
/// before giving up (polls the proof map below).
const PAYMENT_WAIT_SECS: u64 = 300;

/// A payment challenge surfaced by a Blossom server (402).
#[derive(Clone, Debug, PartialEq)]
pub struct PaymentPrompt {
    /// Server origin the challenge came from.
    pub server: String,
    /// Hash of the blob the payment unlocks (uploads: blob about to be
    /// sent; downloads: blob being fetched).
    pub sha256: String,
    /// Parsed `X-{method}` payment requests (may be empty on the web when
    /// the server does not expose them cross-origin).
    pub requests: Vec<BlossomPaymentRequest>,
    /// Sanitized `X-Reason`, display only.
    pub reason: Option<String>,
}

impl PaymentPrompt {
    /// Proof-map key for this (server, blob) pair.
    pub fn key(&self) -> String {
        proof_key(&self.server, &self.sha256)
    }

    /// The lightning `X-Lightning` request, if advertised.
    pub fn lightning_request(&self) -> Option<&BlossomPaymentRequest> {
        self.requests
            .iter()
            .find(|r| r.method.eq_ignore_ascii_case("lightning"))
    }

    /// The cashu `X-Cashu` request, if advertised.
    pub fn cashu_request(&self) -> Option<&BlossomPaymentRequest> {
        self.requests
            .iter()
            .find(|r| r.method.eq_ignore_ascii_case("cashu"))
    }
}

fn proof_key(server: &str, sha256: &str) -> String {
    format!("{server}|{sha256}")
}

/// The active payment challenge, rendered by the app-shell modal. The
/// pending upload/download task set this and waits on the proof map.
pub static BLOSSOM_PAYMENT_PROMPT: GlobalSignal<Option<PaymentPrompt>> = Signal::global(|| None);
/// Proofs submitted after successful payment, keyed by (server, sha256).
static BLOSSOM_PAYMENT_PROOFS: GlobalSignal<HashMap<String, BlossomPaymentProof>> =
    Signal::global(HashMap::new);
/// Guards against a malicious server looping pay→402→prompt: at most one
/// prompt per (server, blob) per app session (Amethyst's prompt ledger).
static ANSWERED_PROMPTS: GlobalSignal<std::collections::HashSet<String>> =
    Signal::global(std::collections::HashSet::new);

/// Publish a payment prompt (unless this challenge was already answered and
/// the server is asking again — that is a pay-loop, surface as failure).
pub fn prompt_for_payment(prompt: PaymentPrompt) -> bool {
    let key = prompt.key();
    if ANSWERED_PROMPTS.read().contains(&key) {
        log::warn!("Blossom server {key} re-requesting payment after proof; refusing");
        return false;
    }
    *BLOSSOM_PAYMENT_PROMPT.write() = Some(prompt);
    true
}

/// Called by the payment modal after a successful payment. Wakes the
/// pending task, which retries with the proof attached.
pub fn submit_proof(server: &str, sha256: &str, proof: BlossomPaymentProof) {
    ANSWERED_PROMPTS.write().insert(proof_key(server, sha256));
    BLOSSOM_PAYMENT_PROOFS
        .write()
        .insert(proof_key(server, sha256), proof);
    *BLOSSOM_PAYMENT_PROMPT.write() = None;
}

/// Dismiss the modal without paying.
pub fn dismiss_prompt() {
    *BLOSSOM_PAYMENT_PROMPT.write() = None;
}

/// Wait (poll) for a proof to appear, up to [`PAYMENT_WAIT_SECS`]. Used by
/// upload/download tasks that have surfaced a prompt; returns the proof or
/// None on timeout/dismissal.
pub async fn wait_for_proof(server: &str, sha256: &str) -> Option<BlossomPaymentProof> {
    let key = proof_key(server, sha256);
    let deadline = crate::platform::timestamp::now_secs() + PAYMENT_WAIT_SECS;
    loop {
        if let Some(proof) = BLOSSOM_PAYMENT_PROOFS.write().remove(&key) {
            return Some(proof);
        }
        if crate::platform::timestamp::now_secs() >= deadline
            || BLOSSOM_PAYMENT_PROMPT.read().is_none()
        {
            return None;
        }
        crate::platform::timer::sleep_ms(400).await;
    }
}

/// Derived, verified details of a lightning payment challenge. Amountless
/// invoices are refused outright (they cannot be shown to the user safely).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightningDetails {
    pub invoice: String,
    /// Re-derived from the BOLT11 string itself, never from UI state.
    pub amount_sats: u64,
    pub memo: String,
}

/// Parse an `X-Lightning` BOLT11 request, re-deriving the amount from the
/// invoice (never trust a displayed amount — the invoice is what gets paid).
/// Refuses amountless invoices and amounts above [`MAX_PAYMENT_SATS`].
pub fn parse_lightning_request(request: &str) -> Result<LightningDetails, String> {
    let invoice = request.trim().trim_matches('"');
    let inv = lightning_invoice::Bolt11Invoice::from_str(invoice)
        .map_err(|e| format!("Invalid BOLT11 invoice: {e}"))?;
    if inv.currency() != lightning_invoice::Currency::Bitcoin {
        return Err("Invoice is not a Bitcoin mainnet invoice".to_string());
    }
    let msats = inv
        .amount_milli_satoshis()
        .ok_or_else(|| "Amountless invoices are not supported".to_string())?;
    let amount_sats = msats.div_ceil(1000);
    if amount_sats == 0 {
        return Err("Invoice amount is zero".to_string());
    }
    if amount_sats > MAX_PAYMENT_SATS {
        return Err(format!(
            "Payment of {amount_sats} sats exceeds the {MAX_PAYMENT_SATS} sat safety cap"
        ));
    }
    Ok(LightningDetails {
        invoice: invoice.to_string(),
        amount_sats,
        memo: sanitize_server_text(&inv.description().to_string()),
    })
}

/// Decoded NUT-24 `creqA` cashu payment request.
/// (Outside the `cashu` feature there is no payer, so this is decode-only.)
#[cfg_attr(not(feature = "cashu"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CashuDetails {
    /// Requested amount in the request unit (payments only supported in sat).
    pub amount: u64,
    pub unit: String,
    /// Mints the server accepts tokens from.
    pub mints: Vec<String>,
    pub description: Option<String>,
}

/// Decode a `creqA…` NUT-24 request: `"creq" + "A" + base64(CBOR)`. The CBOR
/// map is `{a, u, m, d, …}` (NUT-18); tolerant of standard vs url-safe
/// base64 alphabets and padding, as the spec text and examples disagree.
#[cfg_attr(not(feature = "cashu"), allow(dead_code))]
pub fn decode_cashu_request(request: &str) -> Result<CashuDetails, String> {
    let payload = request
        .trim()
        .strip_prefix("creqA")
        .or_else(|| request.trim().strip_prefix("creqa"))
        .ok_or_else(|| "Unsupported cashu request (expected creqA)".to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.as_bytes())
        .or_else(|_| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload.as_bytes())
        })
        .map_err(|e| format!("Invalid creq base64: {e}"))?;
    let value: serde_json::Value =
        ciborium::from_reader(std::io::Cursor::new(&bytes)).map_err(|e| format!("CBOR: {e}"))?;
    let amount = value
        .get("a")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "Cashu request has no amount".to_string())?;
    let unit = value
        .get("u")
        .and_then(|v| v.as_str())
        .unwrap_or("sat")
        .to_string();
    let mints: Vec<String> = value
        .get("m")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let description = value
        .get("d")
        .and_then(|v| v.as_str())
        .map(sanitize_server_text);
    if amount == 0 {
        return Err("Cashu request amount must be positive".to_string());
    }
    if unit != "sat" {
        return Err(format!("Cashu payments in '{unit}' are not supported"));
    }
    if mints.is_empty() {
        return Err("Cashu request lists no mints".to_string());
    }
    Ok(CashuDetails {
        amount,
        unit,
        mints,
        description,
    })
}

/// Pay a cashu challenge with the local wallet: picks the first accepted
/// mint the wallet holds tokens for, sends exactly `amount` sats of ecash
/// (swap + serialize handled by the wallet store), and returns the `cashuB`
/// token for the retry header. Requires the `cashu` feature.
#[cfg(feature = "cashu")]
pub async fn pay_with_cashu(details: &CashuDetails) -> Result<BlossomPaymentProof, String> {
    if details.amount > MAX_PAYMENT_SATS {
        return Err(format!(
            "Payment of {} sats exceeds the {MAX_PAYMENT_SATS} sat safety cap",
            details.amount
        ));
    }
    // Per-mint balances from the wallet store; intersect with the server's
    // accepted list (NUT-24: the token MUST be from `m`).
    let mint = details
        .mints
        .iter()
        .find(|m| crate::stores::cashu::get_mint_balance(m) >= details.amount)
        .ok_or_else(|| {
            format!(
                "No accepted mint with sufficient balance. Server accepts: {}",
                details.mints.join(", ")
            )
        })?
        .clone();
    let token = crate::stores::cashu::send::send_tokens(mint.clone(), details.amount).await?;
    if !token.starts_with("cashuB") {
        return Err("Wallet produced an unsupported token version".to_string());
    }
    Ok(BlossomPaymentProof {
        method: "cashu".to_string(),
        proof: token,
    })
}

/// Pay a lightning challenge via WebLN, returning the preimage proof.
/// Web only; other platforms open an external wallet (no preimage, so the
/// retry is not possible automatically — the modal surfaces a manual path).
pub async fn pay_lightning_webln(invoice: &str) -> Result<BlossomPaymentProof, String> {
    let preimage = crate::platform::lightning::pay_webln_preimage(invoice).await?;
    Ok(BlossomPaymentProof {
        method: "lightning".to_string(),
        proof: preimage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_nut_24_creq_example() {
        // NUT-18 §Example encoded request (in-band variant of NUT-24).
        let request = "creqApWF0gaNhdGVub3N0cmFheKlucHJvZmlsZTFxeTI4d3VtbjhnaGo3dW45ZDNzaGp0bnl2OWtoMnVld2Q5aHN6OW1od2RlbjV0ZTB3ZmprY2N0ZTljdXJ4dmVuOWVlaHFjdHJ2NWhzenJ0aHdkZW41dGUwZGVoaHh0bnZkYWtxcWd5ZGFxeTdjdXJrNDM5eWtwdGt5c3Y3dWRoZGh1NjhzdWNtMjk1YWtxZWZkZWhrZjBkNDk1Y3d1bmw1YWeBgmFuYjE3YWloYjdhOTAxNzZhYQphdWNzYXRhbYF4Imh0dHBzOi8vbm9mZWVzLnRlc3RudXQuY2FzaHUuc3BhY2U=";
        let details = decode_cashu_request(request).unwrap();
        assert_eq!(details.amount, 10);
        assert_eq!(details.unit, "sat");
        assert_eq!(details.mints, ["https://nofees.testnut.cashu.space"]);
    }

    #[test]
    fn rejects_non_creq_and_wrong_unit() {
        assert!(decode_cashu_request("cashuAtoken").is_err());
        let wrong_unit: String = {
            let mut data = Vec::new();
            let map = serde_json::json!({"a": 5, "u": "usd", "m": ["https://m.example"]});
            ciborium::into_writer(&map, &mut data).unwrap();
            format!(
                "creqA{}",
                base64::engine::general_purpose::STANDARD.encode(&data)
            )
        };
        assert!(decode_cashu_request(&wrong_unit).is_err());
    }

    #[test]
    fn amountless_lightning_invoice_refused() {
        // A well-formed but amountless invoice must be rejected, not paid.
        // (Generated offline; signature is irrelevant for this check.)
        let amountless = "lnbc1p5nnthqsp5syyhqwdgs9nvzdyy9fmr2l3x34rlyajqz6sewcgmzp09g9dh87ljspp5qqqyqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqgq9q9qyysgqfqqqqqqqqqqqqqqqsqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq";
        assert!(parse_lightning_request(amountless).is_err());
    }
}
