use nostr_sdk::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedNaddr {
    pub kind: u16,
    pub pubkey: String,
    pub identifier: String,
    pub relay_hints: Vec<String>,
}

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

pub fn parse_naddr(naddr: &str) -> Result<ParsedNaddr, String> {
    if naddr.starts_with("naddr") {
        let nip19 = Nip19::from_bech32(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
        match nip19 {
            Nip19::Coordinate(coord) => Ok(ParsedNaddr {
                kind: coord.coordinate.kind.as_u16(),
                pubkey: coord.coordinate.public_key.to_hex(),
                identifier: coord.coordinate.identifier,
                relay_hints: coord.relays.iter().map(|r| r.to_string()).collect(),
            }),
            _ => Err("Expected naddr coordinate".to_string()),
        }
    } else {
        let parts: Vec<&str> = naddr.splitn(3, ':').collect();
        if parts.len() >= 3 {
            let kind = parts[0]
                .parse::<u16>()
                .map_err(|_| format!("Invalid kind: {}", parts[0]))?;
            Ok(ParsedNaddr {
                kind,
                pubkey: parts[1].to_string(),
                identifier: parts[2].to_string(),
                relay_hints: vec![],
            })
        } else if parts.len() == 2 {
            Ok(ParsedNaddr {
                kind: 0,
                pubkey: parts[0].to_string(),
                identifier: parts[1].to_string(),
                relay_hints: vec![],
            })
        } else {
            Err(format!("Invalid naddr format: {}", naddr))
        }
    }
}
