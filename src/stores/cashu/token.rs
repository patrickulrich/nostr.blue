//! Token Utilities
//!
//! Helper functions for working with Cashu tokens (V3 and V4 formats).
//! CDK handles both formats automatically, this module provides additional utilities.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use std::str::FromStr;

// =============================================================================
// Token Format Detection
// =============================================================================

/// Token format version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenFormat {
    /// V3 format (cashuA prefix, URL-encoded)
    V3,
    /// V4 format (cashuB prefix, compact CBOR)
    V4,
    /// Unknown format
    Unknown,
}

impl TokenFormat {
    /// Detect token format from string
    pub fn detect(token_str: &str) -> Self {
        let trimmed = token_str.trim();
        if trimmed.starts_with("cashuA") {
            TokenFormat::V3
        } else if trimmed.starts_with("cashuB") {
            TokenFormat::V4
        } else {
            TokenFormat::Unknown
        }
    }

    /// Get the prefix for this format
    pub fn prefix(&self) -> &'static str {
        match self {
            TokenFormat::V3 => "cashuA",
            TokenFormat::V4 => "cashuB",
            TokenFormat::Unknown => "",
        }
    }

    /// Check if this is the compact V4 format
    pub fn is_compact(&self) -> bool {
        matches!(self, TokenFormat::V4)
    }
}

// =============================================================================
// Token Parsing
// =============================================================================

/// Parsed token information (without full validation)
#[derive(Debug, Clone)]
pub struct TokenInfo {
    /// Token format version
    pub format: TokenFormat,
    /// Mint URL (if extractable)
    pub mint_url: Option<String>,
    /// Total value in sats (if extractable)
    pub value: Option<u64>,
    /// Number of proofs
    pub proof_count: Option<usize>,
    /// Unit (e.g., "sat")
    pub unit: Option<String>,
    /// Memo if present
    pub memo: Option<String>,
}

/// Extract basic info from a token string without full validation
///
/// This is useful for displaying token info in the UI before receiving.
pub fn get_token_info(token_str: &str) -> Result<TokenInfo, String> {
    use cdk::nuts::Token;

    let trimmed = token_str.trim();
    let format = TokenFormat::detect(trimmed);

    if matches!(format, TokenFormat::Unknown) {
        return Err("Invalid token format - must start with 'cashuA' or 'cashuB'".to_string());
    }

    // Parse with CDK to get full info
    let token = Token::from_str(trimmed)
        .map_err(|e| format!("Failed to parse token: {}", e))?;

    let mint_url = token.mint_url()
        .ok()
        .map(|u| u.to_string());

    let value = token.value()
        .ok()
        .map(|a| u64::from(a));

    let unit = token.unit()
        .map(|u| u.to_string());

    let memo = token.memo().clone();

    // Count proofs - this is a rough estimate
    let proof_count = value.map(|_| 1); // Placeholder - would need keyset info for exact count

    Ok(TokenInfo {
        format,
        mint_url,
        value,
        proof_count,
        unit,
        memo,
    })
}

// =============================================================================
// Token Creation
// =============================================================================

/// Create a token string from proofs (V4 format by default)
///
/// This is a convenience wrapper around CDK's Token::new() which creates V4 tokens.
pub fn create_token(
    mint_url: &str,
    proofs: Vec<cdk::nuts::Proof>,
    memo: Option<String>,
) -> Result<String, String> {
    use cdk::nuts::{CurrencyUnit, Token};
    use cdk::mint_url::MintUrl;

    if proofs.is_empty() {
        return Err("Cannot create token with no proofs".to_string());
    }

    let mint_url = MintUrl::from_str(mint_url)
        .map_err(|e| format!("Invalid mint URL: {}", e))?;

    // CDK's Token::new() creates V4 format by default
    let token = Token::new(
        mint_url,
        proofs.into(),
        memo,
        CurrencyUnit::Sat,
    );

    Ok(token.to_string())
}

/// Convert a V3 token to V4 format
///
/// Uses CDK's TryFrom conversion. Note that multi-mint V3 tokens cannot be
/// converted to V4 format (V4 only supports single-mint tokens).
pub fn convert_to_v4(token_str: &str) -> Result<String, String> {
    use cdk::nuts::{TokenV3, TokenV4};
    use std::convert::TryFrom;

    let trimmed = token_str.trim();

    // If already V4, just return it
    if matches!(TokenFormat::detect(trimmed), TokenFormat::V4) {
        return Ok(trimmed.to_string());
    }

    // Parse as V3 token
    let token_v3 = TokenV3::from_str(trimmed)
        .map_err(|e| format!("Failed to parse V3 token: {}", e))?;

    // Convert to V4 using CDK's TryFrom (fails for multi-mint tokens)
    let token_v4 = TokenV4::try_from(token_v3)
        .map_err(|e| format!("Cannot convert to V4 format: {} (multi-mint tokens not supported in V4)", e))?;

    // Return V4 token string (cashuB prefix with CBOR encoding)
    Ok(token_v4.to_string())
}

// =============================================================================
// Token Validation
// =============================================================================

/// Validate a token string can be parsed
pub fn validate_token(token_str: &str) -> Result<(), String> {
    use cdk::nuts::Token;

    let trimmed = token_str.trim();

    if trimmed.is_empty() {
        return Err("Token string is empty".to_string());
    }

    let format = TokenFormat::detect(trimmed);
    if matches!(format, TokenFormat::Unknown) {
        return Err("Invalid token format - must start with 'cashuA' or 'cashuB'".to_string());
    }

    // Try to parse
    Token::from_str(trimmed)
        .map_err(|e| format!("Invalid token: {}", e))?;

    Ok(())
}

/// Check if a string looks like a valid Cashu token
pub fn is_token(s: &str) -> bool {
    let trimmed = s.trim();
    (trimmed.starts_with("cashuA") || trimmed.starts_with("cashuB"))
        && trimmed.len() > 10
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_detection() {
        assert_eq!(TokenFormat::detect("cashuAabc123"), TokenFormat::V3);
        assert_eq!(TokenFormat::detect("cashuBxyz789"), TokenFormat::V4);
        assert_eq!(TokenFormat::detect("invalid"), TokenFormat::Unknown);
        assert_eq!(TokenFormat::detect("  cashuA123  "), TokenFormat::V3);
    }

    #[test]
    fn test_is_token() {
        assert!(is_token("cashuAabcdefghijk"));
        assert!(is_token("cashuBabcdefghijk"));
        assert!(!is_token("cashuA")); // Too short
        assert!(!is_token("invalid"));
    }
}
