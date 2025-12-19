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
/// Fully char-aware implementation - no byte slicing for UTF-8 safety
pub fn truncate_with_word_break(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }

    // Find the last space within the first max_chars characters
    let last_space_pos = chars[..max_chars]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, c)| **c == ' ')
        .map(|(i, _)| i);

    // Truncate at word boundary if found, otherwise at max_chars
    let truncate_at = last_space_pos.unwrap_or(max_chars);
    let result: String = chars[..truncate_at].iter().collect();
    format!("{}...", result)
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

/// Format a Unix timestamp as a relative time string (e.g., "5m ago", "2d ago")
///
/// Returns `None` for invalid timestamps (0 or far in the future).
/// Uses WASM-compatible `js_sys::Date::now()` for current time.
///
/// # Examples
/// ```
/// let ts = 1700000000; // Some past timestamp
/// let relative = format_relative_time(ts);
/// // Returns Some("5d ago") or similar based on current time
/// ```
pub fn format_relative_time(timestamp: u64) -> Option<String> {
    // Invalid timestamp: zero
    if timestamp == 0 {
        return None;
    }

    // Use js_sys for WASM compatibility
    let now = (js_sys::Date::now() / 1000.0) as u64;

    // Invalid timestamp: more than 1 day in the future
    if timestamp > now.saturating_add(86400) {
        return None;
    }

    let diff = now.saturating_sub(timestamp);

    Some(match diff {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", diff / 60),
        3600..=86399 => format!("{}h ago", diff / 3600),
        86400..=604799 => format!("{}d ago", diff / 86400),
        604800..=2591999 => format!("{}w ago", diff / 604800),
        2592000..=31535999 => format!("{}mo ago", diff / 2592000),
        _ => format!("{}y ago", diff / 31536000),
    })
}

/// Format a Unix timestamp as relative time, with a fallback for invalid timestamps
///
/// This is a convenience wrapper around `format_relative_time()` that provides
/// a default string for invalid timestamps instead of returning `None`.
///
/// # Arguments
/// * `timestamp` - Unix timestamp in seconds
/// * `default` - Fallback string to use for invalid timestamps
///
/// # Examples
/// ```
/// let ts = 0; // Invalid
/// let relative = format_relative_time_or(ts, "Unknown");
/// // Returns "Unknown"
/// ```
pub fn format_relative_time_or(timestamp: u64, default: &str) -> String {
    format_relative_time(timestamp).unwrap_or_else(|| default.to_string())
}
