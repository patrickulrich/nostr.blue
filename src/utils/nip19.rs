use nostr_sdk::prelude::*;

/// Normalize a pubkey string to canonical hex format
/// Accepts both npub (bech32) and hex formats
/// Returns canonical hex string or error
pub fn normalize_pubkey(pubkey_str: &str) -> Result<String, String> {
    // Try parsing as bech32 (npub) first
    if pubkey_str.starts_with("npub") {
        match Nip19::from_bech32(pubkey_str) {
            Ok(Nip19::Pubkey(pubkey)) => Ok(pubkey.to_hex()),
            Ok(_) => Err("Invalid npub format".to_string()),
            Err(e) => Err(format!("Failed to parse npub: {}", e)),
        }
    } else {
        // Try parsing as hex
        match PublicKey::from_hex(pubkey_str) {
            Ok(pubkey) => Ok(pubkey.to_hex()),
            Err(e) => Err(format!("Invalid pubkey format: {}", e)),
        }
    }
}
