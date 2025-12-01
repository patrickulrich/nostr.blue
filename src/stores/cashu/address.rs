//! BIP353 and Lightning Address Parsing
//!
//! Supports human-readable payment addresses like user@domain.com
//! which can be resolved to Lightning invoices or LNURL endpoints.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

// =============================================================================
// Address Types
// =============================================================================

/// Type of payment address
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressType {
    /// Lightning Address (user@domain.com)
    Lightning,
    /// BIP353 DNS-based address (user@domain)
    Bip353,
    /// LNURL (lnurl1...)
    Lnurl,
    /// Lightning Invoice (lnbc...)
    Invoice,
    /// Unknown format
    Unknown,
}

/// Parsed payment address
#[derive(Debug, Clone)]
pub struct PaymentAddress {
    /// Original address string
    pub original: String,
    /// Address type
    pub address_type: AddressType,
    /// Username portion (for user@domain formats)
    pub username: Option<String>,
    /// Domain portion (for user@domain formats)
    pub domain: Option<String>,
}

impl PaymentAddress {
    /// Parse a payment address string
    pub fn parse(address: &str) -> Self {
        let trimmed = address.trim().to_lowercase();

        // Check for Lightning invoice
        if trimmed.starts_with("lnbc") || trimmed.starts_with("lntb") || trimmed.starts_with("lnbcrt") {
            return Self {
                original: address.to_string(),
                address_type: AddressType::Invoice,
                username: None,
                domain: None,
            };
        }

        // Check for LNURL
        if trimmed.starts_with("lnurl") {
            return Self {
                original: address.to_string(),
                address_type: AddressType::Lnurl,
                username: None,
                domain: None,
            };
        }

        // Check for user@domain format (Lightning Address / BIP353)
        if let Some((user, domain)) = trimmed.split_once('@') {
            if !user.is_empty() && !domain.is_empty() && domain.contains('.') {
                return Self {
                    original: address.to_string(),
                    address_type: AddressType::Lightning,
                    username: Some(user.to_string()),
                    domain: Some(domain.to_string()),
                };
            }
        }

        Self {
            original: address.to_string(),
            address_type: AddressType::Unknown,
            username: None,
            domain: None,
        }
    }

    /// Check if this is a resolvable address (not already an invoice)
    pub fn is_resolvable(&self) -> bool {
        matches!(self.address_type, AddressType::Lightning | AddressType::Bip353 | AddressType::Lnurl)
    }

    /// Check if this is a Lightning Address (user@domain)
    pub fn is_lightning_address(&self) -> bool {
        matches!(self.address_type, AddressType::Lightning)
    }

    /// Get the LNURL-pay endpoint URL for a Lightning Address
    pub fn lnurlp_url(&self) -> Option<String> {
        if let (Some(user), Some(domain)) = (&self.username, &self.domain) {
            Some(format!("https://{}/.well-known/lnurlp/{}", domain, user))
        } else {
            None
        }
    }
}

// =============================================================================
// Lightning Address Resolution
// =============================================================================

/// LNURL-pay response (minimal fields we need)
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LnurlPayResponse {
    /// Callback URL to get invoice
    pub callback: String,
    /// Minimum sendable amount in millisats
    #[serde(rename = "minSendable")]
    pub min_sendable: u64,
    /// Maximum sendable amount in millisats
    #[serde(rename = "maxSendable")]
    pub max_sendable: u64,
    /// Comment allowed length (0 if not allowed)
    #[serde(rename = "commentAllowed", default)]
    pub comment_allowed: u16,
    /// Metadata string
    #[serde(default)]
    pub metadata: String,
    /// Tag (should be "payRequest")
    #[serde(default)]
    pub tag: String,
}

/// Invoice response from LNURL callback
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LnurlInvoiceResponse {
    /// Payment request (bolt11 invoice)
    pub pr: String,
    /// Routes (optional)
    #[serde(default)]
    pub routes: Vec<serde_json::Value>,
}

/// Resolve a Lightning Address to get payment info
///
/// Returns the LNURL-pay response which can be used to request an invoice.
pub async fn resolve_lightning_address(address: &PaymentAddress) -> Result<LnurlPayResponse, String> {
    let url = address.lnurlp_url()
        .ok_or("Not a Lightning Address")?;

    log::info!("Resolving Lightning Address: {}", address.original);

    let response = gloo_net::http::Request::get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let lnurl_response: LnurlPayResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    if lnurl_response.tag != "payRequest" {
        return Err(format!("Invalid LNURL response tag: {}", lnurl_response.tag));
    }

    Ok(lnurl_response)
}

/// Request an invoice from a Lightning Address
///
/// Amount is in millisats.
pub async fn request_invoice(
    lnurl_pay: &LnurlPayResponse,
    amount_msats: u64,
    comment: Option<&str>,
) -> Result<String, String> {
    // Validate amount
    if amount_msats < lnurl_pay.min_sendable {
        return Err(format!(
            "Amount {} msats is below minimum {} msats",
            amount_msats, lnurl_pay.min_sendable
        ));
    }
    if amount_msats > lnurl_pay.max_sendable {
        return Err(format!(
            "Amount {} msats exceeds maximum {} msats",
            amount_msats, lnurl_pay.max_sendable
        ));
    }

    // Build callback URL with amount
    let mut url = format!("{}?amount={}", lnurl_pay.callback, amount_msats);

    // Add comment if allowed and provided
    if let Some(comment_text) = comment {
        if lnurl_pay.comment_allowed > 0 {
            let truncated: String = comment_text.chars().take(lnurl_pay.comment_allowed as usize).collect();
            url.push_str(&format!("&comment={}", urlencoding::encode(&truncated)));
        }
    }

    log::info!("Requesting invoice from: {}", url);

    let response = gloo_net::http::Request::get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let invoice_response: LnurlInvoiceResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse invoice response: {}", e))?;

    Ok(invoice_response.pr)
}

/// Resolve a Lightning Address and get an invoice in one step
///
/// Amount is in sats (converted to msats internally).
pub async fn get_invoice_for_address(
    address: &str,
    amount_sats: u64,
    comment: Option<&str>,
) -> Result<String, String> {
    let parsed = PaymentAddress::parse(address);

    if !parsed.is_lightning_address() {
        return Err("Not a valid Lightning Address (expected user@domain.com)".to_string());
    }

    let lnurl_pay = resolve_lightning_address(&parsed).await?;

    let amount_msats = amount_sats
        .checked_mul(1000)
        .ok_or("Amount overflow when converting to millisatoshis")?;
    request_invoice(&lnurl_pay, amount_msats, comment).await
}

// =============================================================================
// Validation
// =============================================================================

/// Check if a string looks like a Lightning Address
pub fn is_lightning_address(s: &str) -> bool {
    let parsed = PaymentAddress::parse(s);
    parsed.is_lightning_address()
}

/// Check if a string looks like a Lightning invoice
pub fn is_lightning_invoice(s: &str) -> bool {
    let lower = s.trim().to_lowercase();
    lower.starts_with("lnbc") || lower.starts_with("lntb") || lower.starts_with("lnbcrt")
}

/// Check if a string looks like an LNURL
pub fn is_lnurl(s: &str) -> bool {
    s.trim().to_lowercase().starts_with("lnurl")
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lightning_address() {
        let addr = PaymentAddress::parse("user@example.com");
        assert!(addr.is_lightning_address());
        assert_eq!(addr.username.as_deref(), Some("user"));
        assert_eq!(addr.domain.as_deref(), Some("example.com"));
    }

    #[test]
    fn test_parse_invoice() {
        let addr = PaymentAddress::parse("lnbc100n1...");
        assert_eq!(addr.address_type, AddressType::Invoice);
    }

    #[test]
    fn test_lnurlp_url() {
        let addr = PaymentAddress::parse("satoshi@bitcoin.org");
        assert_eq!(
            addr.lnurlp_url().as_deref(),
            Some("https://bitcoin.org/.well-known/lnurlp/satoshi")
        );
    }

    #[test]
    fn test_is_lightning_address() {
        assert!(is_lightning_address("user@domain.com"));
        assert!(!is_lightning_address("invalid"));
        assert!(!is_lightning_address("lnbc123..."));
    }
}
