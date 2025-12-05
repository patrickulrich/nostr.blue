//! Validation utilities for common validation patterns across the codebase.

use dioxus::prelude::ReadableExt;
use nostr_sdk::PublicKey;
use crate::stores::signer::SIGNER_INFO;

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
