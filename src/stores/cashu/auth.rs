//! NUT-21/22 Authentication Support
//!
//! Implements Clear Auth (NUT-21) and Blind Auth (NUT-22) for protected mints.
//!
//! ## Overview
//!
//! Some mints require authentication to access certain endpoints. This module provides:
//! - Type definitions for auth tokens and requirements (re-exported from CDK)
//! - Protected endpoint detection from mint info
//! - Auth header generation for HTTP requests
//!
//! ## Auth Types
//!
//! - **Clear Auth (NUT-21)**: JWT-based auth using OIDC providers
//! - **Blind Auth (NUT-22)**: Blind signature-based tokens for privacy

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use std::collections::HashMap;

// =============================================================================
// CDK Auth Types (NUT-21/22)
// =============================================================================

use dioxus::prelude::*;

// Re-export CDK's auth types for use by other modules
pub use cdk_common::{
    AuthRequired, AuthToken, BlindAuthToken, AuthProof,
    Method as HttpMethod, RoutePath, ProtectedEndpoint,
    ClearAuthSettings, BlindAuthSettings,
};

// =============================================================================
// Global Auth State Cache
// =============================================================================

/// Global cache for mint auth states
pub static MINT_AUTH_STATES: GlobalSignal<HashMap<String, MintAuthState>> =
    GlobalSignal::new(|| HashMap::new());

/// Get auth state for a mint (from cache)
pub fn get_mint_auth_state(mint_url: &str) -> Option<MintAuthState> {
    MINT_AUTH_STATES.read().get(mint_url).cloned()
}

/// Set auth state for a mint (in cache)
pub fn set_mint_auth_state(mint_url: &str, mut state: MintAuthState) {
    // Ensure protected_map is built before caching
    state.build_protected_map();
    MINT_AUTH_STATES.write().insert(mint_url.to_string(), state);
}

/// Check if a mint is known to require authentication
pub fn mint_requires_auth(mint_url: &str) -> bool {
    MINT_AUTH_STATES.read()
        .get(mint_url)
        .map(|s| s.requires_auth())
        .unwrap_or(false)
}

/// Clear auth state for a mint
pub fn clear_mint_auth_state(mint_url: &str) {
    MINT_AUTH_STATES.write().remove(mint_url);
}


// =============================================================================
// Auth State Management
// =============================================================================

/// Auth state for a single mint
#[derive(Clone, Default)]
pub struct MintAuthState {
    /// Clear auth settings (NUT-21) if supported
    pub clear_auth: Option<ClearAuthSettings>,
    /// Blind auth settings (NUT-22) if supported
    pub blind_auth: Option<BlindAuthSettings>,
    /// Current clear auth token (JWT)
    pub clear_token: Option<String>,
    /// Refresh token for clear auth
    pub refresh_token: Option<String>,
    /// Cached protected endpoints map (endpoint -> auth type required)
    pub protected_map: HashMap<ProtectedEndpoint, AuthRequired>,
}

// Manual Debug implementation to redact sensitive tokens
impl std::fmt::Debug for MintAuthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MintAuthState")
            .field("clear_auth", &self.clear_auth)
            .field("blind_auth", &self.blind_auth)
            .field("clear_token", &self.clear_token.as_ref().map(|_| "<redacted>"))
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| "<redacted>"))
            .field("protected_map", &self.protected_map)
            .finish()
    }
}

impl MintAuthState {
    /// Check if this mint requires authentication
    pub fn requires_auth(&self) -> bool {
        self.clear_auth.is_some() || self.blind_auth.is_some()
    }

    /// Check if a specific endpoint requires authentication
    pub fn endpoint_requires_auth(&self, endpoint: &ProtectedEndpoint) -> Option<AuthRequired> {
        self.protected_map.get(endpoint).copied()
    }

    /// Check if a specific method/path combination requires authentication
    pub fn requires_auth_for(&self, method: HttpMethod, path: RoutePath) -> Option<AuthRequired> {
        let endpoint = ProtectedEndpoint::new(method, path);
        self.endpoint_requires_auth(&endpoint)
    }

    /// Build protected map from settings
    pub fn build_protected_map(&mut self) {
        self.protected_map.clear();

        // Add clear auth protected endpoints
        if let Some(ref settings) = self.clear_auth {
            for ep in &settings.protected_endpoints {
                self.protected_map.insert(*ep, AuthRequired::Clear);
            }
        }

        // Add blind auth protected endpoints (these override clear auth)
        if let Some(ref settings) = self.blind_auth {
            for ep in &settings.protected_endpoints {
                self.protected_map.insert(*ep, AuthRequired::Blind);
            }
        }
    }
}

// =============================================================================
// Mint Info Auth Parsing
// =============================================================================

/// Parse auth settings from mint info response
///
/// Checks the `nuts` field for NUT-21 and NUT-22 settings.
pub fn parse_auth_from_mint_info(mint_info: &serde_json::Value) -> MintAuthState {
    let mut state = MintAuthState::default();

    if let Some(nuts) = mint_info.get("nuts") {
        // Parse NUT-21 (Clear Auth)
        if let Some(nut21) = nuts.get("21") {
            if let Ok(settings) = serde_json::from_value::<ClearAuthSettings>(nut21.clone()) {
                state.clear_auth = Some(settings);
                log::debug!("Mint supports Clear Auth (NUT-21)");
            }
        }

        // Parse NUT-22 (Blind Auth)
        if let Some(nut22) = nuts.get("22") {
            if let Ok(settings) = serde_json::from_value::<BlindAuthSettings>(nut22.clone()) {
                state.blind_auth = Some(settings);
                log::debug!("Mint supports Blind Auth (NUT-22)");
            }
        }
    }

    // Build the protected map
    state.build_protected_map();

    state
}

/// Check if mint info indicates a protected mint
pub fn is_protected_mint(mint_info: &serde_json::Value) -> bool {
    if let Some(nuts) = mint_info.get("nuts") {
        nuts.get("21").is_some() || nuts.get("22").is_some()
    } else {
        false
    }
}

// =============================================================================
// HTTP Header Helpers
// =============================================================================

/// Add auth header to a request if auth is required and available
pub fn add_auth_header(
    headers: &mut Vec<(String, String)>,
    auth_state: &MintAuthState,
    mint_url: &str,
    endpoint: &ProtectedEndpoint,
) -> Result<(), String> {
    if let Some(auth_required) = auth_state.endpoint_requires_auth(endpoint) {
        match auth_required {
            AuthRequired::Clear => {
                if let Some(ref token) = auth_state.clear_token {
                    headers.push(("Clear-auth".to_string(), token.clone()));
                    Ok(())
                } else {
                    Err("Clear auth required but no token available".to_string())
                }
            }
            AuthRequired::Blind => {
                // Get blind auth token from cache
                if let Some(token) = get_blind_auth_for_request(mint_url) {
                    if let AuthToken::BlindAuth(bat) = token {
                        headers.push(("Blind-auth".to_string(), bat.to_string()));
                        return Ok(());
                    }
                }
                Err("Blind auth required but no tokens available. Please request blind auth tokens.".to_string())
            }
        }
    } else {
        // No auth required for this endpoint
        Ok(())
    }
}

/// Add auth header using method and path (convenience function)
pub fn add_auth_header_for(
    headers: &mut Vec<(String, String)>,
    auth_state: &MintAuthState,
    mint_url: &str,
    method: HttpMethod,
    path: RoutePath,
) -> Result<(), String> {
    let endpoint = ProtectedEndpoint::new(method, path);
    add_auth_header(headers, auth_state, mint_url, &endpoint)
}

/// Check if an error response indicates auth is required
///
/// Uses CDK error codes: 30001 (ClearAuthRequired), 31001 (BlindAuthRequired)
pub fn is_auth_required_error(status: u16, body: &str) -> Option<AuthRequired> {
    // NUT-21/22 error codes
    if status == 401 || status == 403 {
        // CDK error codes: 30001=ClearAuth, 31001=BlindAuth
        if body.contains("30001") || body.contains("clear_auth_required") || body.contains("Clear-auth") {
            return Some(AuthRequired::Clear);
        }
        if body.contains("31001") || body.contains("blind_auth_required") || body.contains("Blind-auth") {
            return Some(AuthRequired::Blind);
        }
        // Ambiguous 401/403 without explicit NUT-21/22 markers - don't assume auth type
        return None;
    }
    None
}

// =============================================================================
// Blind Auth Token Management
// =============================================================================

/// Get a blind auth token for a request, consuming from cache
///
/// Returns None if no tokens are available in cache.
/// Use `request_blind_auth_tokens` to mint new tokens.
pub fn get_blind_auth_for_request(mint_url: &str) -> Option<AuthToken> {
    use super::auth_cache::get_cached_token;

    let bat = get_cached_token(mint_url)?;
    // Remove DLEQ before sending to mint (it links creation and redemption)
    Some(AuthToken::BlindAuth(bat.without_dleq()))
}

/// Check if we have blind auth tokens available for a mint
pub fn has_blind_auth_tokens(mint_url: &str) -> bool {
    use super::auth_cache::has_cached_tokens;
    has_cached_tokens(mint_url)
}

/// Get the count of available blind auth tokens for a mint
pub fn blind_auth_token_count(mint_url: &str) -> usize {
    use super::auth_cache::cached_token_count;
    cached_token_count(mint_url)
}

/// Get auth token for a protected endpoint
///
/// Checks if the endpoint requires auth and returns the appropriate token.
/// For blind auth, consumes a token from cache.
/// For clear auth, uses the stored JWT.
pub fn get_auth_for_endpoint(
    auth_state: &MintAuthState,
    mint_url: &str,
    endpoint: &ProtectedEndpoint,
) -> Result<Option<AuthToken>, String> {
    match auth_state.endpoint_requires_auth(endpoint) {
        Some(AuthRequired::Clear) => {
            if let Some(ref token) = auth_state.clear_token {
                Ok(Some(AuthToken::ClearAuth(token.clone())))
            } else {
                Err("Clear auth required but no token available. Please authenticate with the mint.".to_string())
            }
        }
        Some(AuthRequired::Blind) => {
            if let Some(token) = get_blind_auth_for_request(mint_url) {
                Ok(Some(token))
            } else {
                Err("Blind auth required but no tokens available. Please request blind auth tokens.".to_string())
            }
        }
        None => Ok(None), // No auth required
    }
}

// =============================================================================
// Mint Auth Discovery
// =============================================================================

/// Fetch and cache auth state for a mint
///
/// Makes a request to the mint's info endpoint to discover auth requirements.
pub async fn discover_mint_auth(mint_url: &str) -> Result<MintAuthState, String> {
    use super::internal::get_or_create_wallet;

    log::debug!("Discovering auth requirements for mint: {}", mint_url);

    // Get wallet to fetch mint info
    let wallet = get_or_create_wallet(mint_url).await?;

    // Fetch mint info
    let mint_info = wallet
        .fetch_mint_info()
        .await
        .map_err(|e| format!("Failed to fetch mint info: {}", e))?;

    // Parse auth settings from mint info
    let state = if let Some(info) = mint_info {
        // Convert MintInfo to JSON for parsing
        let info_json = serde_json::to_value(&info)
            .map_err(|e| format!("Failed to serialize mint info: {}", e))?;
        parse_auth_from_mint_info(&info_json)
    } else {
        MintAuthState::default()
    };

    // Cache the auth state
    set_mint_auth_state(mint_url, state.clone());

    if state.requires_auth() {
        log::info!("Mint {} requires authentication (NUT-21/22)", mint_url);
        if state.blind_auth.is_some() {
            log::info!("  - Supports Blind Auth (NUT-22)");
        }
        if state.clear_auth.is_some() {
            log::info!("  - Supports Clear Auth (NUT-21)");
        }
    } else {
        log::debug!("Mint {} does not require authentication", mint_url);
    }

    Ok(state)
}

/// Check if an operation on a mint requires authentication
///
/// Returns the auth type required, or None if no auth needed.
pub async fn check_operation_auth(
    mint_url: &str,
    method: HttpMethod,
    path: RoutePath,
) -> Result<Option<AuthRequired>, String> {
    // Try to get cached auth state first
    let state = if let Some(s) = get_mint_auth_state(mint_url) {
        s
    } else {
        // Discover auth requirements if not cached
        discover_mint_auth(mint_url).await?
    };

    Ok(state.requires_auth_for(method, path))
}

/// Ensure auth is available for an operation, returning an error if not
pub async fn ensure_auth_available(
    mint_url: &str,
    method: HttpMethod,
    path: RoutePath,
) -> Result<(), String> {
    let auth_required = check_operation_auth(mint_url, method, path).await?;

    match auth_required {
        Some(AuthRequired::Clear) => {
            let state = get_mint_auth_state(mint_url)
                .ok_or("Auth state not found")?;
            if state.clear_token.is_none() {
                return Err("This mint requires Clear Auth (NUT-21). Please authenticate with the mint's OAuth provider.".to_string());
            }
        }
        Some(AuthRequired::Blind) => {
            if !has_blind_auth_tokens(mint_url) {
                return Err("This mint requires Blind Auth (NUT-22). Please request blind auth tokens first.".to_string());
            }
        }
        None => {
            // No auth required
        }
    }

    Ok(())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mint_auth_state_default() {
        let state = MintAuthState::default();
        assert!(!state.requires_auth());
        assert!(state.clear_auth.is_none());
        assert!(state.blind_auth.is_none());
    }

    #[test]
    fn test_is_auth_required_error() {
        // Clear auth error (CDK error code 30001)
        assert_eq!(
            is_auth_required_error(401, "error code 30001"),
            Some(AuthRequired::Clear)
        );
        assert_eq!(
            is_auth_required_error(401, "Clear-auth required"),
            Some(AuthRequired::Clear)
        );

        // Blind auth error (CDK error code 31001)
        assert_eq!(
            is_auth_required_error(403, "error code 31001"),
            Some(AuthRequired::Blind)
        );
        assert_eq!(
            is_auth_required_error(403, "Blind-auth required"),
            Some(AuthRequired::Blind)
        );

        // Ambiguous 401/403 without specific markers - returns None
        assert_eq!(is_auth_required_error(401, "unauthorized"), None);
        assert_eq!(is_auth_required_error(403, "forbidden"), None);

        // No auth required (non-401/403)
        assert_eq!(is_auth_required_error(200, "ok"), None);
        assert_eq!(is_auth_required_error(500, "internal error"), None);
    }

    #[test]
    fn test_is_protected_mint() {
        // Protected mint with NUT-21
        let protected = serde_json::json!({
            "name": "Test",
            "nuts": { "21": {} }
        });
        assert!(is_protected_mint(&protected));

        // Protected mint with NUT-22
        let protected22 = serde_json::json!({
            "name": "Test",
            "nuts": { "22": {} }
        });
        assert!(is_protected_mint(&protected22));

        // Not protected
        let unprotected = serde_json::json!({
            "name": "Test",
            "nuts": {}
        });
        assert!(!is_protected_mint(&unprotected));
    }
}
