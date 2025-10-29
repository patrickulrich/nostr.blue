use chrono::{DateTime, Utc};
use nostr_sdk::Timestamp;

/// Format a timestamp as relative time (e.g., "5m ago", "2h ago", "3d ago")
pub fn format_relative_time(timestamp: Timestamp) -> String {
    let now = Utc::now().timestamp() as u64;
    let ts = timestamp.as_u64();

    if now < ts {
        return "just now".to_string();
    }

    let diff = now - ts;

    match diff {
        0..=59 => "just now".to_string(),
        60..=3599 => {
            let minutes = diff / 60;
            format!("{}m", minutes)
        }
        3600..=86399 => {
            let hours = diff / 3600;
            format!("{}h", hours)
        }
        86400..=604799 => {
            let days = diff / 86400;
            format!("{}d", days)
        }
        _ => {
            // For older than 7 days, show the date
            let dt = DateTime::from_timestamp(ts as i64, 0)
                .unwrap_or_else(|| Utc::now());
            dt.format("%b %d").to_string()
        }
    }
}

/// Format a timestamp as a human-readable date and time
#[allow(dead_code)]
pub fn format_datetime(timestamp: Timestamp) -> String {
    let dt = DateTime::from_timestamp(timestamp.as_u64() as i64, 0)
        .unwrap_or_else(|| Utc::now());
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}
