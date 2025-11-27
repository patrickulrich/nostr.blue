/// Format satoshi amount with thousands separator (e.g., 1,234,567)
pub fn format_sats_with_separator(sats: u64) -> String {
    let s = sats.to_string();
    let mut result = String::new();
    let mut count = 0;

    for c in s.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
        count += 1;
    }

    result.chars().rev().collect()
}

/// Format satoshi amount in compact form (e.g., 1M, 234k)
pub fn format_sats_compact(sats: u64) -> String {
    if sats >= 1_000_000 {
        format!("{}M", sats / 1_000_000)
    } else if sats >= 1_000 {
        format!("{}k", sats / 1_000)
    } else {
        sats.to_string()
    }
}

/// Truncates a pubkey/hex string to show first 8 and last 8 chars
/// Returns "abcd1234...wxyz5678" format for long strings
pub fn truncate_pubkey(pubkey: &str) -> String {
    if pubkey.len() <= 19 {
        return pubkey.to_string();
    }
    format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len()-8..])
}
