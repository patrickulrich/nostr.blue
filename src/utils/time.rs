use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use nostr_sdk::Timestamp;

// ============================================================================
// Simple Relative Time (WASM-compatible, u64 input)
// ============================================================================

/// Format a Unix timestamp as relative time (e.g., "2h ago", "3d ago")
///
/// This version takes a raw `u64` timestamp and is WASM-compatible,
/// using `js_sys::Date::now()` in WASM and `std::time::SystemTime` otherwise.
///
/// # Examples
/// - Recent: "just now"
/// - Minutes: "5m ago"
/// - Hours: "2h ago"
/// - Days: "3d ago"
/// - Weeks: "2w ago"
/// - Months: "3mo ago"
pub fn format_time_ago(timestamp: u64) -> String {
    #[cfg(target_family = "wasm")]
    let now = (js_sys::Date::now() / 1000.0) as u64;

    #[cfg(not(target_family = "wasm"))]
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let diff = now.saturating_sub(timestamp);

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else if diff < 604800 {
        format!("{}d ago", diff / 86400)
    } else if diff < 2592000 {
        format!("{}w ago", diff / 604800)
    } else {
        format!("{}mo ago", diff / 2592000)
    }
}

// ============================================================================
// Duration Safety Utilities
// ============================================================================

/// Convert a duration in seconds (f64) to milliseconds (u32) with overflow protection.
///
/// Uses saturating arithmetic to prevent overflow/underflow when converting
/// potentially large or negative durations to milliseconds for use with timers.
///
/// # Arguments
/// * `seconds` - Duration in seconds as f64
///
/// # Returns
/// * `u32` - Milliseconds, clamped to valid range [0, u32::MAX]
///
/// # Examples
/// ```
/// assert_eq!(safe_duration_millis(1.5), 1500);
/// assert_eq!(safe_duration_millis(-1.0), 0);
/// assert_eq!(safe_duration_millis(5_000_000.0), u32::MAX); // Clamped
/// ```
pub fn safe_duration_millis(seconds: f64) -> u32 {
    if seconds <= 0.0 {
        return 0;
    }
    let millis = seconds * 1000.0;
    if millis > u32::MAX as f64 {
        u32::MAX
    } else {
        millis as u32
    }
}

/// Format a timestamp as relative time
///
/// # Arguments
/// * `timestamp` - The timestamp to format
/// * `include_ago` - Whether to include " ago" suffix (e.g., "5m ago" vs "5m")
/// * `use_long_format` - Whether to include months/years for old timestamps instead of dates
///
/// # Examples
/// - `format_relative_time(ts, false, false)` returns "5m", "2h", "3d" or "Jan 15"
/// - `format_relative_time(ts, true, true)` returns "5m ago", "2h ago", "3mo ago", "2y ago"
/// - Future timestamps return "in Xm", "in Xh", "in Xd", etc.
pub fn format_relative_time_ex(timestamp: Timestamp, include_ago: bool, use_long_format: bool) -> String {
    let now = Utc::now().timestamp() as u64;
    let ts = timestamp.as_secs();

    // Handle future timestamps
    if ts > now {
        let diff = ts - now;
        return format_future_time(diff, use_long_format);
    }

    let diff = now - ts;
    let ago_suffix = if include_ago { " ago" } else { "" };

    match diff {
        0..=59 => "just now".to_string(),
        60..=3599 => {
            let minutes = diff / 60;
            format!("{}m{}", minutes, ago_suffix)
        }
        3600..=86399 => {
            let hours = diff / 3600;
            format!("{}h{}", hours, ago_suffix)
        }
        86400..=2591999 if use_long_format => {
            let days = diff / 86400;
            format!("{}d{}", days, ago_suffix)
        }
        2592000..=31535999 if use_long_format => {
            let months = diff / 2592000;
            format!("{}mo{}", months, ago_suffix)
        }
        31536000.. if use_long_format => {
            let years = diff / 31536000;
            format!("{}y{}", years, ago_suffix)
        }
        86400..=604799 => {
            let days = diff / 86400;
            format!("{}d{}", days, ago_suffix)
        }
        _ => {
            // For older than 7 days, show the date
            let dt = DateTime::from_timestamp(ts as i64, 0)
                .unwrap_or_else(Utc::now);
            dt.format("%b %d").to_string()
        }
    }
}

/// Format a future time difference
fn format_future_time(diff: u64, use_long: bool) -> String {
    if diff < 60 {
        return "in a moment".to_string();
    }
    if diff < 3600 {
        let mins = diff / 60;
        return if use_long {
            format!("in {} minute{}", mins, if mins == 1 { "" } else { "s" })
        } else {
            format!("in {}m", mins)
        };
    }
    if diff < 86400 {
        let hours = diff / 3600;
        return if use_long {
            format!("in {} hour{}", hours, if hours == 1 { "" } else { "s" })
        } else {
            format!("in {}h", hours)
        };
    }
    let days = diff / 86400;
    if use_long {
        format!("in {} day{}", days, if days == 1 { "" } else { "s" })
    } else {
        format!("in {}d", days)
    }
}

/// Format a timestamp as relative time (e.g., "5m", "2h", "3d")
/// This is a convenience wrapper around format_relative_time_ex with default parameters
pub fn format_relative_time(timestamp: Timestamp) -> String {
    format_relative_time_ex(timestamp, false, false)
}

/// Format a timestamp as a human-readable date and time
#[allow(dead_code)]
pub fn format_datetime(timestamp: Timestamp) -> String {
    let dt = DateTime::from_timestamp(timestamp.as_secs() as i64, 0)
        .unwrap_or_else(Utc::now);
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Calculate end timestamp based on preset or custom time
///
/// Supported presets:
/// - "1hour": 1 hour from now
/// - "1day": 1 day from now
/// - "3days": 3 days from now
/// - "1week": 1 week from now
/// - "custom": parse datetime-local format (YYYY-MM-DDTHH:MM)
/// - Any other value defaults to 1 day
///
/// Uses `.earliest()` for DST-safe local time conversion.
pub fn calculate_end_time(preset: &str, custom_time: &str) -> Option<Timestamp> {
    let now = Timestamp::now();

    match preset {
        "1hour" => Some(Timestamp::from(now.as_secs() + 3600)),
        "1day" => Some(Timestamp::from(now.as_secs() + 86400)),
        "3days" => Some(Timestamp::from(now.as_secs() + 259200)),
        "1week" => Some(Timestamp::from(now.as_secs() + 604800)),
        "custom" => {
            if custom_time.is_empty() {
                return None;
            }
            // Parse datetime-local format (YYYY-MM-DDTHH:MM)
            if let Ok(naive_dt) = NaiveDateTime::parse_from_str(custom_time, "%Y-%m-%dT%H:%M") {
                // Convert from local time to UTC
                // Use .earliest() for deterministic behavior during DST transitions
                if let Some(local_dt) = Local.from_local_datetime(&naive_dt).earliest() {
                    let utc_dt = local_dt.with_timezone(&Utc);
                    let timestamp = utc_dt.timestamp();

                    // Verify timestamp is valid (non-negative and in the future)
                    if timestamp >= 0 && timestamp > Utc::now().timestamp() {
                        Some(Timestamp::from(timestamp as u64))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => Some(Timestamp::from(now.as_secs() + 86400)), // Default to 1 day
    }
}
