//! Validation utilities for common validation patterns across the codebase.

use dioxus::prelude::ReadableExt;
use nostr_sdk::PublicKey;
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
