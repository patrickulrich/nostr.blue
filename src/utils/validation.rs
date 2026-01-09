//! Validation utilities for common validation patterns across the codebase.

use dioxus::prelude::ReadableExt;
use nostr_sdk::PublicKey;
use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;
use crate::stores::signer::SIGNER_INFO;

// ============================================================================
// URL Validation Utilities
// ============================================================================

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
    Url::parse(url_str).ok().filter(|u| {
        matches!(u.scheme(), "http" | "https")
    })
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
#[allow(dead_code)] // Available for future use
pub fn try_get_current_user_pubkey() -> Option<PublicKey> {
    match get_current_user_pubkey() {
        SignerValidationResult::Ok(pk) => Some(pk),
        _ => None,
    }
}

// ============================================================================
// Lightning Invoice Validation
// ============================================================================

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
    // Lightning invoices must start with ln prefix
    let lower = invoice.to_lowercase();
    if !lower.starts_with("lnbc")  // Mainnet
        && !lower.starts_with("lntb")  // Testnet
        && !lower.starts_with("lnbcrt") // Regtest
        && !lower.starts_with("lnsb")  // Signet
    {
        return None;
    }

    // Only allow alphanumeric characters (bech32 charset)
    // This prevents injection of quotes, brackets, or script tags
    let valid = invoice.chars().all(|c| c.is_ascii_alphanumeric());
    if !valid {
        return None;
    }

    // Minimum reasonable length for a Lightning invoice
    if invoice.len() < 50 {
        return None;
    }

    Some(invoice.to_uppercase())
}

// ============================================================================
// CSS URL Validation
// ============================================================================

// Static regex for CSS dimension validation - compiled once at startup
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
    // Must be valid HTTP/HTTPS URL
    if !is_valid_http_url(url) {
        return None;
    }
    // Reject characters that could break out of CSS url() context
    // Single/double quotes, parentheses, and backslash are dangerous
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

    // Reject empty strings
    if trimmed.is_empty() {
        return None;
    }

    // Reject dangerous CSS content
    // Semicolons, braces, parentheses, quotes, backslashes are dangerous
    if trimmed.contains([';', '{', '}', '(', ')', '\'', '"', '\\', '<', '>']) {
        return None;
    }

    // Reject common CSS injection patterns (case-insensitive check)
    let lower = trimmed.to_lowercase();
    if lower.contains("expression") || lower.contains("javascript") || lower.contains("url") {
        return None;
    }

    // Must match pattern: number + optional unit
    // Allowed units: px, %, vh, vw, em, rem, pt, vmin, vmax
    if CSS_DIMENSION_PATTERN.is_match(trimmed) {
        Some(trimmed)  // Return validated trimmed slice
    } else {
        None
    }
}
