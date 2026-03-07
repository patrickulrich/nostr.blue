//! NIP-98: HTTP Auth
//!
//! Utility functions for creating NIP-98 authorization headers.
//!
//! ## References
//! - NIP-98: https://github.com/nostr-protocol/nips/blob/master/98.md
use nostr_sdk::prelude::*;
use crate::stores::nostr_client;
/// Result of creating a NIP-98 auth header, includes the signed URL for consistency
pub struct AuthResult {
    /// The authorization header value: `Nostr <base64>`
    pub header: String,
    /// The exact URL that was signed (use this for the HTTP request)
    pub signed_url: String,
}
/// Create a NIP-98 authorization header for HTTP requests.
///
/// This creates a kind 27235 event with the URL and method, signs it with the
/// current user's signer, and returns both the header and the exact URL that was signed.
///
/// IMPORTANT: Use `result.signed_url` for the HTTP request to ensure URL consistency
/// with the signed NIP-98 event. URL parsing can normalize URLs slightly differently.
///
/// # Arguments
/// * `url` - The absolute URL being requested
/// * `method` - The HTTP method (GET, POST, etc.)
///
/// # Returns
/// * `Ok(AuthResult)` - The authorization header and signed URL
/// * `Err(String)` - Error message if auth creation fails
pub async fn create_auth_header(
    url: &str,
    method: nip98::HttpMethod,
) -> Result<AuthResult, String> {
    let signer = nostr_client::get_signer()
        .ok_or_else(|| {
            "Not authenticated. Please sign in to access this feature.".to_string()
        })?;
    let parsed_url = Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
    let signed_url = parsed_url.to_string();
    let http_data = nip98::HttpData::new(parsed_url, method);
    let header = match signer {
        crate::stores::signer::SignerType::Keys(keys) => {
            http_data
                .to_authorization(&keys)
                .await
                .map_err(|e| format!("Failed to create NIP-98 auth: {}", e))?
        }
        #[cfg(target_family = "wasm")]
        crate::stores::signer::SignerType::BrowserExtension(browser_signer) => {
            http_data
                .to_authorization(browser_signer.as_ref())
                .await
                .map_err(|e| format!("Failed to create NIP-98 auth: {}", e))?
        }
        crate::stores::signer::SignerType::NostrConnect(nostr_connect) => {
            http_data
                .to_authorization(nostr_connect.as_ref())
                .await
                .map_err(|e| format!("Failed to create NIP-98 auth: {}", e))?
        }
        #[cfg(feature = "mobile")]
        crate::stores::signer::SignerType::AndroidSigner(android_signer) => {
            http_data
                .to_authorization(android_signer.as_ref())
                .await
                .map_err(|e| format!("Failed to create NIP-98 auth: {}", e))?
        }
    };
    Ok(AuthResult { header, signed_url })
}
/// Create a NIP-98 authorization header with a payload hash.
///
/// Used for POST/PUT/PATCH requests that include a body.
///
/// IMPORTANT: Use `result.signed_url` for the HTTP request to ensure URL consistency
/// with the signed NIP-98 event.
///
/// # Arguments
/// * `url` - The absolute URL being requested
/// * `method` - The HTTP method
/// * `payload_hash` - SHA-256 hash of the request body
///
/// # Returns
/// * `Ok(AuthResult)` - The authorization header and signed URL
/// * `Err(String)` - Error message if auth creation fails
#[allow(dead_code)]
pub async fn create_auth_header_with_payload(
    url: &str,
    method: nip98::HttpMethod,
    payload_hash: hashes::sha256::Hash,
) -> Result<AuthResult, String> {
    let signer = nostr_client::get_signer()
        .ok_or_else(|| {
            "Not authenticated. Please sign in to access this feature.".to_string()
        })?;
    let parsed_url = Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
    let signed_url = parsed_url.to_string();
    let http_data = nip98::HttpData::new(parsed_url, method).payload(payload_hash);
    let header = match signer {
        crate::stores::signer::SignerType::Keys(keys) => {
            http_data
                .to_authorization(&keys)
                .await
                .map_err(|e| format!("Failed to create NIP-98 auth: {}", e))?
        }
        #[cfg(target_family = "wasm")]
        crate::stores::signer::SignerType::BrowserExtension(browser_signer) => {
            http_data
                .to_authorization(browser_signer.as_ref())
                .await
                .map_err(|e| format!("Failed to create NIP-98 auth: {}", e))?
        }
        crate::stores::signer::SignerType::NostrConnect(nostr_connect) => {
            http_data
                .to_authorization(nostr_connect.as_ref())
                .await
                .map_err(|e| format!("Failed to create NIP-98 auth: {}", e))?
        }
        #[cfg(feature = "mobile")]
        crate::stores::signer::SignerType::AndroidSigner(android_signer) => {
            http_data
                .to_authorization(android_signer.as_ref())
                .await
                .map_err(|e| format!("Failed to create NIP-98 auth: {}", e))?
        }
    };
    Ok(AuthResult { header, signed_url })
}
