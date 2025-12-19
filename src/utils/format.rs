/// Format satoshi amount with thousands separator (e.g., 1,234,567)
pub fn format_sats_with_separator(sats: u64) -> String {
    let s = sats.to_string();
    let mut result = String::new();

    for (count, c) in s.chars().rev().enumerate() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
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
    // Fast path for ASCII (common case for hex pubkeys)
    if pubkey.is_ascii() {
        return format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len() - 8..]);
    }
    // Safe path for non-ASCII to avoid panic on multi-byte UTF-8
    let chars: Vec<char> = pubkey.chars().collect();
    if chars.len() <= 19 {
        return pubkey.to_string();
    }
    let prefix: String = chars[..8].iter().collect();
    let suffix: String = chars[chars.len() - 8..].iter().collect();
    format!("{}...{}", prefix, suffix)
}

/// Truncates text at a word boundary to avoid breaking words
/// Returns text with "..." suffix if truncated
pub fn truncate_with_word_break(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    if let Some(last_space) = truncated.rfind(' ') {
        format!("{}...", &truncated[..last_space])
    } else {
        format!("{}...", truncated)
    }
}

/// Extracts the domain from a URL (e.g., "https://example.com/path" -> "example.com")
pub fn extract_domain(url: &str) -> Option<String> {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .and_then(|s| s.split('/').next())
        .map(|s| s.to_string())
}

/// Shortens a URL for display by stripping protocol and truncating
/// Uses UTF-8 safe character-based slicing to avoid panic on multi-byte chars
pub fn shorten_url(url: &str, max_len: usize) -> String {
    let url = url.trim_start_matches("https://").trim_start_matches("http://");

    // Handle very small max_len - return truncated URL without ellipsis
    if max_len <= 3 {
        return url.chars().take(max_len).collect();
    }

    // Fast path for ASCII (common case for URLs)
    if url.is_ascii() && url.len() > max_len {
        return format!("{}...", &url[..max_len.saturating_sub(3)]);
    }
    // Safe path for non-ASCII to avoid panic on multi-byte UTF-8
    let char_count = url.chars().count();
    if char_count > max_len {
        format!("{}...", url.chars().take(max_len.saturating_sub(3)).collect::<String>())
    } else {
        url.to_string()
    }
}
