//! Validation utilities for common validation patterns across the codebase.
use crate::stores::signer::SIGNER_INFO;
use dioxus::prelude::ReadableExt;
use nostr_sdk::PublicKey;
use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;
/// Check if a string is a valid HTTP or HTTPS URL.
///
/// Uses the `url` crate for proper URL parsing and validates that the scheme
/// is either `http` or `https`. This prevents injection of other URL schemes
/// like `javascript:`, `data:`, or `file:`.
///
/// # Arguments
/// * `url_str` - The URL string to validate
///
/// # Returns
/// * `true` if the URL is valid and uses http/https scheme
/// * `false` otherwise
///
/// # Examples
/// ```
/// assert!(is_valid_http_url("https://example.com"));
/// assert!(is_valid_http_url("http://localhost:3000/path"));
/// assert!(!is_valid_http_url("javascript:alert(1)"));
/// assert!(!is_valid_http_url("not a url"));
/// ```
pub fn is_valid_http_url(url_str: &str) -> bool {
    parse_http_url(url_str).is_some()
}
/// Parse a string as an HTTP/HTTPS URL.
///
/// Returns `Some(Url)` if the string is a valid URL with http/https scheme,
/// `None` otherwise. Use this when you need both validation and the parsed URL.
///
/// # Arguments
/// * `url_str` - The URL string to parse
///
/// # Returns
/// * `Some(Url)` - Valid parsed URL with http/https scheme
/// * `None` - Invalid URL or non-http/https scheme
pub fn parse_http_url(url_str: &str) -> Option<Url> {
    Url::parse(url_str)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))
}
/// Result type for signer validation operations
pub enum SignerValidationResult {
    /// Successfully retrieved user's public key
    Ok(PublicKey),
    /// No signer info available (user not signed in)
    NotSignedIn,
    /// Signer info present but public key is invalid
    InvalidPubkey,
}
/// Get the current user's public key from signer info if available.
///
/// This is a common pattern used in composers and other components that need
/// to validate the user is signed in before performing actions.
///
/// # Returns
/// - `SignerValidationResult::Ok(pubkey)` - User is signed in with valid pubkey
/// - `SignerValidationResult::NotSignedIn` - No signer info (user should sign in)
/// - `SignerValidationResult::InvalidPubkey` - Signer info present but malformed
pub fn get_current_user_pubkey() -> SignerValidationResult {
    match SIGNER_INFO.read().as_ref() {
        Some(info) => match PublicKey::from_hex(&info.public_key) {
            Ok(pk) => SignerValidationResult::Ok(pk),
            Err(_) => SignerValidationResult::InvalidPubkey,
        },
        None => SignerValidationResult::NotSignedIn,
    }
}
/// Get user's pubkey as Option for simpler cases where error details aren't needed.
#[allow(dead_code)]
pub fn try_get_current_user_pubkey() -> Option<PublicKey> {
    match get_current_user_pubkey() {
        SignerValidationResult::Ok(pk) => Some(pk),
        _ => None,
    }
}
/// Sanitize and validate a Lightning invoice for safe embedding in HTML/JS.
///
/// Lightning invoices (BOLT11) should only contain bech32 characters:
/// alphanumeric (excluding 1, b, i, o) but in practice invoices use
/// a broader alphanumeric set. This function ensures the invoice is safe
/// to embed in JavaScript strings to prevent XSS attacks.
///
/// # Arguments
/// * `invoice` - The Lightning invoice string to validate
///
/// # Returns
/// * `Some(String)` - Sanitized invoice (uppercase) if valid
/// * `None` - If invoice contains invalid characters or format
///
/// # Examples
/// ```
/// // Valid invoice
/// assert!(sanitize_lightning_invoice("lnbc100...").is_some());
///
/// // XSS attempt rejected
/// assert!(sanitize_lightning_invoice("lnbc'; alert('xss')").is_none());
/// ```
pub fn sanitize_lightning_invoice(invoice: &str) -> Option<String> {
    let lower = invoice.to_lowercase();
    if !lower.starts_with("lnbc")
        && !lower.starts_with("lntb")
        && !lower.starts_with("lnbcrt")
        && !lower.starts_with("lnsb")
    {
        return None;
    }
    let valid = invoice.chars().all(|c| c.is_ascii_alphanumeric());
    if !valid {
        return None;
    }
    if invoice.len() < 50 {
        return None;
    }
    Some(invoice.to_uppercase())
}
pub fn validate_blossom_server_url(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("URL cannot be empty".to_string());
    }
    let url_str = if !trimmed.starts_with("https://") {
        if trimmed.starts_with("http://") {
            return Err("Blossom servers must use HTTPS".to_string());
        }
        format!("https://{}", trimmed)
    } else {
        trimmed.to_string()
    };
    let url = url::Url::parse(&url_str).map_err(|e| format!("Invalid URL: {}", e))?;
    if url.scheme() != "https" {
        return Err("Blossom servers must use HTTPS".to_string());
    }
    if url_str.matches("://").count() > 1 {
        return Err("URL must not contain multiple scheme separators".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URL must not contain embedded credentials".to_string());
    }
    let host = url.host_str().ok_or("URL must have a host")?;
    if host.is_empty() {
        return Err("URL must have a host".to_string());
    }
    let host_lower = host.to_lowercase();
    if host_lower == "localhost"
        || host_lower.starts_with("127.")
        || host_lower == "0.0.0.0"
        || host_lower.starts_with("10.")
        || host_lower.starts_with("192.168.")
        || host_lower.starts_with("169.254.")
    {
        return Err("Private/local addresses not allowed".to_string());
    }
    if let Some(url::Host::Ipv4(addr)) = url.host() {
        let octets = addr.octets();
        if octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31 {
            return Err("Private/local addresses not allowed".to_string());
        }
        if octets[0] == 100 && (octets[1] & 0xc0) == 64 {
            return Err("Private/local addresses not allowed".to_string());
        }
    }
    if let Some(url::Host::Ipv6(addr)) = url.host() {
        if addr.is_loopback() {
            return Err("Private/local addresses not allowed".to_string());
        }
        let segments = addr.segments();
        if (segments[0] & 0xffc0) == 0xfe80 {
            return Err("Private/local addresses not allowed".to_string());
        }
        if (segments[0] & 0xfe00) == 0xfc00 {
            return Err("Private/local addresses not allowed".to_string());
        }
        if segments[0] == 0 && segments[1] == 0 && segments[2] == 0 && segments[3] == 0
            && segments[4] == 0 && segments[5] == 0xffff
        {
            let v4_octets = (segments[6] as u32 >> 8, segments[6] as u32 & 0xff, segments[7] as u32 >> 8, segments[7] as u32 & 0xff);
            if v4_octets.0 == 127 || v4_octets.0 == 10
                || (v4_octets.0 == 172 && v4_octets.1 >= 16 && v4_octets.1 <= 31)
                || (v4_octets.0 == 192 && v4_octets.1 == 168)
            {
                return Err("Private/local addresses not allowed".to_string());
            }
        }
    }
    let mut normalized = format!("{}://{}", url.scheme(), host);
    if let Some(port) = url.port() {
        normalized.push_str(&format!(":{}", port));
    }
    Ok(normalized)
}

static CSS_DIMENSION_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^-?[0-9]*\.?[0-9]+(px|%|vh|vw|em|rem|pt|vmin|vmax)?$")
        .expect("Failed to compile CSS dimension regex")
});
/// Validate a URL for safe embedding in CSS `url()` context.
///
/// This function checks that:
/// 1. The URL is a valid HTTP/HTTPS URL (prevents javascript: etc.)
/// 2. The URL doesn't contain characters that could break out of CSS context
///
/// Use this when embedding user-provided URLs in inline styles.
///
/// # Arguments
/// * `url` - The URL string to validate
///
/// # Returns
/// * `Some(&str)` - The original URL if safe for CSS embedding
/// * `None` - If URL is invalid or contains dangerous characters
///
/// # Examples
/// ```
/// assert!(css_safe_url("https://example.com/image.jpg").is_some());
/// assert!(css_safe_url("'); background: url(javascript:").is_none());
/// ```
pub fn css_safe_url(url: &str) -> Option<&str> {
    if !is_valid_http_url(url) {
        return None;
    }
    if url.contains(['\'', '"', ')', '(', '\\']) {
        return None;
    }
    Some(url)
}
/// Validates a CSS dimension value to prevent CSS injection attacks.
///
/// Accepts numeric values with allowed units: px, %, vh, vw, em, rem, pt.
/// Also accepts pure numeric values (treated as pixels).
///
/// # Arguments
/// * `dimension` - The dimension string to validate
///
/// # Returns
/// * `Some(&str)` - The validated dimension if safe
/// * `None` - If the dimension contains potentially dangerous content
///
/// # Examples
/// ```
/// assert!(validate_css_dimension("400px").is_some());
/// assert!(validate_css_dimension("100%").is_some());
/// assert!(validate_css_dimension("50vh").is_some());
/// assert!(validate_css_dimension("expression(alert())").is_none());
/// assert!(validate_css_dimension("100px; background: red").is_none());
/// ```
pub fn validate_css_dimension(dimension: &str) -> Option<&str> {
    let trimmed = dimension.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains([';', '{', '}', '(', ')', '\'', '"', '\\', '<', '>']) {
        return None;
    }
    let lower = trimmed.to_lowercase();
    if lower.contains("expression") || lower.contains("javascript") || lower.contains("url") {
        return None;
    }
    if CSS_DIMENSION_PATTERN.is_match(trimmed) {
        Some(trimmed)
    } else {
        None
    }
}
