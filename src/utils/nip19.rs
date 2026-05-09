use nostr_sdk::prelude::*;
/// Normalize a pubkey string to canonical hex format
/// Accepts both npub (bech32) and hex formats
/// Returns canonical hex string or error
pub fn normalize_pubkey(pubkey_str: &str) -> Result<String, String> {
    if pubkey_str.starts_with("npub") {
        match Nip19::from_bech32(pubkey_str) {
            Ok(Nip19::Pubkey(pubkey)) => Ok(pubkey.to_hex()),
            Ok(_) => Err("Invalid npub format".to_string()),
            Err(e) => Err(format!("Failed to parse npub: {}", e)),
        }
    } else {
        match PublicKey::from_hex(pubkey_str) {
            Ok(pubkey) => Ok(pubkey.to_hex()),
            Err(e) => Err(format!("Invalid pubkey format: {}", e)),
        }
    }
}

pub fn parse_naddr(naddr: &str) -> Result<(String, String), String> {
    if naddr.starts_with("naddr") {
        let nip19 = Nip19::from_bech32(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
        match nip19 {
            Nip19::Coordinate(coord) => {
                let pubkey = coord.coordinate.public_key.to_hex();
                let d_tag = coord.coordinate.identifier;
                Ok((pubkey, d_tag))
            }
            _ => Err("Expected naddr coordinate".to_string()),
        }
    } else {
        let parts: Vec<&str> = naddr.splitn(3, ':').collect();
        if parts.len() >= 3 {
            Ok((parts[1].to_string(), parts[2].to_string()))
        } else if parts.len() == 2 {
            Ok((parts[0].to_string(), parts[1].to_string()))
        } else {
            Err(format!("Invalid naddr format: {}", naddr))
        }
    }
}
