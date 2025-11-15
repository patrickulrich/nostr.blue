/// Utilities for extracting data from NIP-57 zap receipt events (kind 9735)
use nostr_sdk::Event;

/// Result of extracting zap data from a zap receipt event
#[derive(Debug, Clone)]
pub struct ZapData {
    /// The pubkey of the actual zapper (from zap request in description tag)
    pub zapper_pubkey: Option<String>,
    /// The amount in satoshis
    pub amount_sats: Option<u64>,
    /// The event ID that was zapped (from 'e' tag)
    pub zapped_event_id: Option<String>,
}

/// Extract complete zap data from a zap receipt event (kind 9735)
pub fn extract_zap_data(event: &Event) -> ZapData {
    ZapData {
        zapper_pubkey: extract_zapper_pubkey(event),
        amount_sats: extract_zap_amount(event),
        zapped_event_id: extract_zapped_event_id(event),
    }
}

/// Extract the actual zapper's pubkey from a zap receipt event (kind 9735)
///
/// The event.pubkey is the Lightning node's pubkey, not the actual zapper.
/// The actual zapper's pubkey is in the description tag (zap request).
///
/// Per NIP-57: The uppercase P tag should contain the pubkey of the zap sender,
/// but we also fallback to parsing the description tag if needed.
pub fn extract_zapper_pubkey(event: &Event) -> Option<String> {
    // Method 1: Try uppercase P tag first (NIP-57 standard)
    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.first().map(|k| k.as_str()) == Some("P") {
            if let Some(pubkey) = tag_vec.get(1) {
                return Some(pubkey.to_string());
            }
        }
    }

    // Method 2: Fallback - parse description tag (contains zap request JSON)
    if let Some(description_tag) = event.tags.iter().find(|tag| {
        let vec = (*tag).clone().to_vec();
        vec.get(0).map(|k| k.as_str() == "description").unwrap_or(false)
    }) {
        let vec = (*description_tag).clone().to_vec();
        if let Some(description) = vec.get(1) {
            // Parse the zap request event from the description
            if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(description) {
                // Extract the pubkey from the zap request event
                if let Some(pubkey_str) = zap_request.get("pubkey").and_then(|p| p.as_str()) {
                    return Some(pubkey_str.to_string());
                }
            }
        }
    }

    None
}

/// Extract zap amount in satoshis from a zap receipt event (kind 9735)
///
/// Tries to extract from:
/// 1. bolt11 invoice (preferred)
/// 2. description tag (zap request) as fallback
pub fn extract_zap_amount(event: &Event) -> Option<u64> {
    // Method 1: Try to parse from bolt11 tag
    if let Some(bolt11_tag) = event.tags.iter().find(|tag| {
        let vec = (*tag).clone().to_vec();
        vec.get(0).map(|k| k.as_str() == "bolt11").unwrap_or(false)
    }) {
        let vec = (*bolt11_tag).clone().to_vec();
        if let Some(bolt11) = vec.get(1) {
            if let Some(amount) = parse_bolt11_amount(bolt11.as_str()) {
                return Some(amount);
            }
        }
    }

    // Method 2: Fallback - parse from description tag (zap request)
    if let Some(description_tag) = event.tags.iter().find(|tag| {
        let vec = (*tag).clone().to_vec();
        vec.get(0).map(|k| k.as_str() == "description").unwrap_or(false)
    }) {
        let vec = (*description_tag).clone().to_vec();
        if let Some(description) = vec.get(1) {
            if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(description) {
                // Look for amount in the zap request tags
                if let Some(tags) = zap_request.get("tags").and_then(|t| t.as_array()) {
                    for tag_array in tags {
                        if let Some(tag_vals) = tag_array.as_array() {
                            if tag_vals.first().and_then(|v| v.as_str()) == Some("amount") {
                                if let Some(amount_msat) = tag_vals.get(1).and_then(|v| v.as_str()) {
                                    if let Ok(msat) = amount_msat.parse::<u64>() {
                                        return Some(msat / 1000); // Convert millisats to sats
                                    }
                                }
                            }
                        }
                    }
                }

                // Alternative: check for amount field directly (some implementations)
                if let Some(amount_msat) = zap_request.get("amount").and_then(|a| a.as_u64()) {
                    return Some(amount_msat / 1000); // Convert millisats to sats
                }
            }
        }
    }

    None
}

/// Extract the event ID that was zapped from the 'e' tag
pub fn extract_zapped_event_id(event: &Event) -> Option<String> {
    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.first().map(|k| k.as_str()) == Some("e") {
            if let Some(event_id) = tag_vec.get(1) {
                return Some(event_id.to_string());
            }
        }
    }
    None
}

/// Parse amount from bolt11 invoice string
///
/// bolt11 format: ln[prefix][amount][multiplier]...
/// Example: lnbc1000n... where 1000n = 1000 nanosats
fn parse_bolt11_amount(bolt11: &str) -> Option<u64> {
    let lower = bolt11.to_lowercase();

    // Find where the amount starts (after "lnbc" or "lntb" etc)
    let prefix_end = if lower.starts_with("lnbc") {
        4
    } else if lower.starts_with("lntb") {
        4
    } else {
        return None;
    };

    // Extract the part after prefix
    let rest = &lower[prefix_end..];

    // Find the multiplier character (p, n, u, m)
    let mut amount_str = String::new();
    let mut multiplier = ' ';

    for ch in rest.chars() {
        if ch.is_ascii_digit() {
            amount_str.push(ch);
        } else if ['p', 'n', 'u', 'm'].contains(&ch) {
            multiplier = ch;
            break;
        } else {
            break;
        }
    }

    if amount_str.is_empty() {
        return None;
    }

    let amount = amount_str.parse::<u64>().ok()?;

    // Convert to satoshis based on multiplier
    // p = pico (0.001 sat), n = nano (0.1 sat), u = micro (100 sat), m = milli (100000 sat)
    let sats = match multiplier {
        'p' => amount / 1000,           // pico bitcoin (0.001 satoshis)
        'n' => amount / 10,              // nano bitcoin (0.1 satoshis)
        'u' => amount * 100,             // micro bitcoin (100 satoshis)
        'm' => amount * 100_000,         // milli bitcoin (100,000 satoshis)
        _ => return None,
    };

    Some(sats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bolt11_amount() {
        // Test nano (0.1 sat)
        assert_eq!(parse_bolt11_amount("lnbc10n1..."), Some(1)); // 10n = 1 sat
        assert_eq!(parse_bolt11_amount("lnbc1000n1..."), Some(100)); // 1000n = 100 sats

        // Test micro (100 sat)
        assert_eq!(parse_bolt11_amount("lnbc1u1..."), Some(100)); // 1u = 100 sats
        assert_eq!(parse_bolt11_amount("lnbc10u1..."), Some(1000)); // 10u = 1000 sats

        // Test milli (100,000 sat)
        assert_eq!(parse_bolt11_amount("lnbc1m1..."), Some(100_000)); // 1m = 100,000 sats

        // Test invalid
        assert_eq!(parse_bolt11_amount("invalid"), None);
        assert_eq!(parse_bolt11_amount("lnbc1x1..."), None); // invalid multiplier
    }
}
