//! Duration formatting utilities
//!
//! Consolidated duration formatting functions for consistent display across the app.
//! Three different formats are provided for different use cases:
//! - `format_duration_compact`: Short abbreviated format ("2h", "30m", "5d")
//! - `format_duration_timecode`: Media timecode format ("01:24:35" or "02:15")
//! - `format_duration_verbose`: Human-readable format ("2 hours 30 minutes")

// Time unit constants in seconds
const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 3600;
const SECONDS_PER_DAY: u64 = 86400;

/// Format duration in seconds to compact abbreviated format
///
/// Examples:
/// - 45 seconds -> "45s"
/// - 5 minutes -> "5m"
/// - 2 hours -> "2h"
/// - 3 days -> "3d"
///
/// Used for: countdown timers, expiration badges, compact displays
pub fn format_duration_compact(seconds: u64) -> String {
    if seconds < SECONDS_PER_MINUTE {
        format!("{}s", seconds)
    } else if seconds < SECONDS_PER_HOUR {
        format!("{}m", seconds / SECONDS_PER_MINUTE)
    } else if seconds < SECONDS_PER_DAY {
        format!("{}h", seconds / SECONDS_PER_HOUR)
    } else {
        format!("{}d", seconds / SECONDS_PER_DAY)
    }
}

/// Format duration in seconds to media timecode format
///
/// Examples:
/// - 45 seconds -> "0:45"
/// - 90 seconds -> "1:30"
/// - 3661 seconds -> "1:01:01"
///
/// Used for: video duration, podcast duration, audio players
pub fn format_duration_timecode(seconds: u64) -> String {
    let hours = seconds / SECONDS_PER_HOUR;
    let minutes = (seconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
    let secs = seconds % SECONDS_PER_MINUTE;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{}:{:02}", minutes, secs)
    }
}

/// Format duration in seconds to media timecode with zero-padded hours
///
/// Examples:
/// - 45 seconds -> "00:45"
/// - 90 seconds -> "01:30"
/// - 3661 seconds -> "01:01:01"
///
/// Used for: video cards with consistent width display
pub fn format_duration_timecode_padded(seconds: u64) -> String {
    let hours = seconds / SECONDS_PER_HOUR;
    let minutes = (seconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
    let secs = seconds % SECONDS_PER_MINUTE;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{:02}:{:02}", minutes, secs)
    }
}

/// Format duration in seconds to human-readable verbose format
///
/// Examples:
/// - 45 seconds -> "45 seconds"
/// - 300 seconds -> "5 minutes"
/// - 7200 seconds -> "2 hours"
/// - 7380 seconds -> "2h 3m"
/// - 90000 seconds -> "1d 1h"
///
/// Used for: expiration countdowns, time remaining displays
pub fn format_duration_verbose(seconds: u64) -> String {
    if seconds < SECONDS_PER_MINUTE {
        format!("{} seconds", seconds)
    } else if seconds < SECONDS_PER_HOUR {
        format!("{} minutes", seconds / SECONDS_PER_MINUTE)
    } else if seconds < SECONDS_PER_DAY {
        let hours = seconds / SECONDS_PER_HOUR;
        let mins = (seconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
        if mins > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{} hours", hours)
        }
    } else {
        let days = seconds / SECONDS_PER_DAY;
        let hours = (seconds % SECONDS_PER_DAY) / SECONDS_PER_HOUR;
        if hours > 0 {
            format!("{}d {}h", days, hours)
        } else {
            format!("{} days", days)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_compact() {
        assert_eq!(format_duration_compact(45), "45s");
        assert_eq!(format_duration_compact(60), "1m");
        assert_eq!(format_duration_compact(3600), "1h");
        assert_eq!(format_duration_compact(86400), "1d");
        assert_eq!(format_duration_compact(90), "1m"); // Truncates, doesn't round
    }

    #[test]
    fn test_format_duration_timecode() {
        assert_eq!(format_duration_timecode(45), "0:45");
        assert_eq!(format_duration_timecode(90), "1:30");
        assert_eq!(format_duration_timecode(3600), "1:00:00");
        assert_eq!(format_duration_timecode(3661), "1:01:01");
    }

    #[test]
    fn test_format_duration_timecode_padded() {
        assert_eq!(format_duration_timecode_padded(45), "00:45");
        assert_eq!(format_duration_timecode_padded(90), "01:30");
        assert_eq!(format_duration_timecode_padded(3600), "01:00:00");
    }

    #[test]
    fn test_format_duration_verbose() {
        assert_eq!(format_duration_verbose(45), "45 seconds");
        assert_eq!(format_duration_verbose(300), "5 minutes");
        assert_eq!(format_duration_verbose(7200), "2 hours");
        assert_eq!(format_duration_verbose(7380), "2h 3m");
        assert_eq!(format_duration_verbose(90000), "1d 1h");
        assert_eq!(format_duration_verbose(86400), "1 days");
    }
}
