use crate::platform::http::http_client;
use nostr_sdk::nips::nip01::Coordinate;
use nostr_sdk::nips::nip57::ZapRequestData;
use nostr_sdk::{Event, EventBuilder, EventId, JsonUtil, PublicKey, RelayUrl};
use serde::{Deserialize, Serialize};
/// LNURL Pay Response
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LnUrlPayResponse {
    pub callback: String,
    pub min_sendable: u64,
    pub max_sendable: u64,
    pub metadata: String,
    #[serde(default)]
    pub allows_nostr: bool,
    #[serde(rename = "nostrPubkey")]
    pub nostr_pubkey: Option<String>,
    pub tag: String,
    /// Maximum comment length allowed by the endpoint (LUD-12)
    #[serde(default)]
    pub comment_allowed: Option<u64>,
}
/// LNURL Invoice Response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LnUrlInvoiceResponse {
    pub pr: String,
    #[serde(default)]
    pub success_action: Option<SuccessAction>,
    #[serde(default)]
    pub routes: Vec<String>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "tag")]
pub enum SuccessAction {
    #[serde(rename = "message")]
    Message { message: String },
    #[serde(rename = "url")]
    Url { url: String, description: Option<String> },
}
/// Error type for LNURL operations
#[derive(Debug)]
pub enum LnUrlError {
    InvalidLud16(String),
    InvalidLud06(String),
    InvalidUrl(String),
    UntrustedDomain(String),
    FetchError(String),
    ParseError(String),
    NostrNotSupported,
    InvalidAmount,
}
impl std::fmt::Display for LnUrlError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            LnUrlError::InvalidLud16(e) => write!(f, "Invalid lud16: {}", e),
            LnUrlError::InvalidLud06(e) => write!(f, "Invalid lud06: {}", e),
            LnUrlError::InvalidUrl(e) => write!(f, "Invalid URL: {}", e),
            LnUrlError::UntrustedDomain(e) => write!(f, "Untrusted LNURL domain: {}", e),
            LnUrlError::FetchError(e) => write!(f, "Fetch error: {}", e),
            LnUrlError::ParseError(e) => write!(f, "Parse error: {}", e),
            LnUrlError::NostrNotSupported => {
                write!(f, "LNURL endpoint does not support Nostr zaps")
            }
            LnUrlError::InvalidAmount => write!(f, "Invalid zap amount"),
        }
    }
}
/// Validate that a URL is safe for LNURL requests
/// Validates scheme and blocks private/local IP addresses to prevent SSRF attacks
fn validate_url(url: &str) -> Result<(), LnUrlError> {
    if !crate::utils::validation::is_valid_http_url(url) {
        return Err(
            LnUrlError::InvalidUrl(
                format!("URL must be a valid HTTP/HTTPS URL: {}", url),
            ),
        );
    }
    let parsed = url::Url::parse(url)
        .map_err(|e| LnUrlError::InvalidUrl(format!("Failed to parse URL: {}", e)))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| LnUrlError::InvalidUrl("URL has no host".to_string()))?;
    let host_lower = host.to_lowercase();
    if let Some(url::Host::Ipv6(addr)) = parsed.host() {
        if addr.is_loopback() {
            return Err(
                LnUrlError::UntrustedDomain(
                    format!("Private/local addresses not allowed: {}", host),
                ),
            );
        }
        let segments = addr.segments();
        if (segments[0] & 0xffc0) == 0xfe80 {
            return Err(
                LnUrlError::UntrustedDomain(
                    format!("Private/local addresses not allowed: {}", host),
                ),
            );
        }
        if (segments[0] & 0xfe00) == 0xfc00 {
            return Err(
                LnUrlError::UntrustedDomain(
                    format!("Private/local addresses not allowed: {}", host),
                ),
            );
        }
    }
    if host_lower == "localhost" || host_lower.starts_with("127.")
        || host_lower.starts_with("10.") || host_lower.starts_with("192.168.")
        || host_lower.starts_with("169.254.") || host_lower.starts_with("172.16.")
        || host_lower.starts_with("172.17.") || host_lower.starts_with("172.18.")
        || host_lower.starts_with("172.19.") || host_lower.starts_with("172.20.")
        || host_lower.starts_with("172.21.") || host_lower.starts_with("172.22.")
        || host_lower.starts_with("172.23.") || host_lower.starts_with("172.24.")
        || host_lower.starts_with("172.25.") || host_lower.starts_with("172.26.")
        || host_lower.starts_with("172.27.") || host_lower.starts_with("172.28.")
        || host_lower.starts_with("172.29.") || host_lower.starts_with("172.30.")
        || host_lower.starts_with("172.31.")
    {
        return Err(
            LnUrlError::UntrustedDomain(
                format!("Private/local addresses not allowed: {}", host),
            ),
        );
    }
    Ok(())
}
impl std::error::Error for LnUrlError {}
/// Convert lud16 (Lightning Address) to LNURL endpoint
/// Example: username@domain.com -> https://domain.com/.well-known/lnurlp/username
pub fn lud16_to_url(lud16: &str) -> Result<String, LnUrlError> {
    let parts: Vec<&str> = lud16.split('@').collect();
    if parts.len() != 2 {
        return Err(LnUrlError::InvalidLud16("Invalid format".to_string()));
    }
    let username = parts[0];
    let domain = parts[1];
    Ok(format!("https://{}/.well-known/lnurlp/{}", domain, username))
}
/// Decode lud06 (bech32-encoded LNURL) to URL
pub fn decode_lud06(lud06: &str) -> Result<String, LnUrlError> {
    let (hrp, data) = bech32::decode(lud06)
        .map_err(|e| LnUrlError::InvalidLud06(e.to_string()))?;
    if hrp.as_str() != "lnurl" {
        return Err(LnUrlError::InvalidLud06("Invalid HRP".to_string()));
    }
    let url = String::from_utf8(data)
        .map_err(|e| LnUrlError::InvalidLud06(e.to_string()))?;
    Ok(url)
}
/// Fetch LNURL pay information
pub async fn fetch_lnurl_pay_info(url: &str) -> Result<LnUrlPayResponse, LnUrlError> {
    validate_url(url)?;
    let client = http_client();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| LnUrlError::FetchError(e.to_string()))?;
    let pay_info: LnUrlPayResponse = response
        .json()
        .await
        .map_err(|e| LnUrlError::ParseError(e.to_string()))?;
    if !pay_info.allows_nostr {
        return Err(LnUrlError::NostrNotSupported);
    }
    Ok(pay_info)
}
/// Get LNURL pay info from lightning address (lud16 or lud06)
pub async fn get_lnurl_pay_info(
    lud16: Option<&str>,
    lud06: Option<&str>,
) -> Result<LnUrlPayResponse, LnUrlError> {
    let url = if let Some(lud16) = lud16 {
        lud16_to_url(lud16)?
    } else if let Some(lud06) = lud06 {
        decode_lud06(lud06)?
    } else {
        return Err(
            LnUrlError::InvalidLud16("No lightning address provided".to_string()),
        );
    };
    fetch_lnurl_pay_info(&url).await
}
/// Create a zap request event unsigned (to be signed by caller)
/// For addressable events (like Kind 36787 tracks), pass the event_coordinate
/// to include the 'a' tag per NIP-57.
pub fn create_zap_request_unsigned(
    recipient_pubkey: PublicKey,
    relays: Vec<RelayUrl>,
    amount_msats: u64,
    message: Option<String>,
    event_id: Option<EventId>,
    event_coordinate: Option<Coordinate>,
) -> EventBuilder {
    let mut data = ZapRequestData::new(recipient_pubkey, relays).amount(amount_msats);
    if let Some(msg) = message {
        data = data.message(msg);
    }
    if let Some(eid) = event_id {
        data = data.event_id(eid);
    }
    if let Some(coord) = event_coordinate {
        data = data.event_coordinate(coord);
    }
    EventBuilder::public_zap_request(data)
}
/// Request a zap invoice from LNURL callback
pub async fn request_zap_invoice(
    callback_url: &str,
    amount_msats: u64,
    zap_request_event: &Event,
    lnurl: Option<&str>,
) -> Result<LnUrlInvoiceResponse, LnUrlError> {
    validate_url(callback_url)?;
    let zap_request_json = zap_request_event.as_json();
    let nostr_param = urlencoding::encode(&zap_request_json);
    let mut url = format!(
        "{}?amount={}&nostr={}",
        callback_url,
        amount_msats,
        nostr_param,
    );
    if let Some(lnurl_value) = lnurl {
        url.push_str(&format!("&lnurl={}", urlencoding::encode(lnurl_value)));
    }
    let client = http_client();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| LnUrlError::FetchError(e.to_string()))?;
    let invoice: LnUrlInvoiceResponse = response
        .json()
        .await
        .map_err(|e| LnUrlError::ParseError(e.to_string()))?;
    Ok(invoice)
}
/// Get LNURL pay info and return callback details for zap
pub async fn prepare_zap(
    lud16: Option<&str>,
    lud06: Option<&str>,
    amount_sats: u64,
) -> Result<(LnUrlPayResponse, u64), LnUrlError> {
    if amount_sats == 0 {
        return Err(LnUrlError::InvalidAmount);
    }
    let amount_msats = amount_sats * 1000;
    let pay_info = get_lnurl_pay_info(lud16, lud06).await?;
    if amount_msats < pay_info.min_sendable || amount_msats > pay_info.max_sendable {
        return Err(LnUrlError::InvalidAmount);
    }
    Ok((pay_info, amount_msats))
}
/// Fetch LNURL pay information without requiring Nostr zap support
/// Used for simple Lightning payments (not zaps)
pub async fn fetch_lnurl_pay_info_simple(
    url: &str,
) -> Result<LnUrlPayResponse, LnUrlError> {
    validate_url(url)?;
    let client = http_client();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| LnUrlError::FetchError(e.to_string()))?;
    let pay_info: LnUrlPayResponse = response
        .json()
        .await
        .map_err(|e| LnUrlError::ParseError(e.to_string()))?;
    Ok(pay_info)
}
/// Request a simple invoice (non-zap) from LNURL callback
/// Used for merchant payments where we don't need a zap receipt
pub async fn request_simple_invoice(
    callback_url: &str,
    amount_msats: u64,
    comment: Option<&str>,
) -> Result<LnUrlInvoiceResponse, LnUrlError> {
    validate_url(callback_url)?;
    let mut url = format!("{}?amount={}", callback_url, amount_msats);
    if let Some(c) = comment {
        url.push_str(&format!("&comment={}", urlencoding::encode(c)));
    }
    let client = http_client();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| LnUrlError::FetchError(e.to_string()))?;
    let invoice: LnUrlInvoiceResponse = response
        .json()
        .await
        .map_err(|e| LnUrlError::ParseError(e.to_string()))?;
    Ok(invoice)
}
/// Get a Lightning invoice from a lud16 address for a specific amount
/// Returns the bolt11 invoice string
pub async fn get_invoice_from_lud16(
    lud16: &str,
    amount_sats: u64,
    comment: Option<&str>,
) -> Result<String, LnUrlError> {
    if amount_sats == 0 {
        return Err(LnUrlError::InvalidAmount);
    }
    let url = lud16_to_url(lud16)?;
    let pay_info = fetch_lnurl_pay_info_simple(&url).await?;
    let amount_msats = amount_sats * 1000;
    if amount_msats < pay_info.min_sendable || amount_msats > pay_info.max_sendable {
        return Err(LnUrlError::InvalidAmount);
    }
    let invoice_response = request_simple_invoice(
            &pay_info.callback,
            amount_msats,
            comment,
        )
        .await?;
    Ok(invoice_response.pr)
}
