use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use nostr_sdk::Timestamp;

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
pub fn format_relative_time_ex(timestamp: Timestamp, include_ago: bool, use_long_format: bool) -> String {
    let now = Utc::now().timestamp() as u64;
    let ts = timestamp.as_secs();

    if now < ts {
        return "just now".to_string();
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
                .unwrap_or_else(|| Utc::now());
            dt.format("%b %d").to_string()
        }
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
        .unwrap_or_else(|| Utc::now());
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
