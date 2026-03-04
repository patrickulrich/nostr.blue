//! Date Helper Utilities
//!
//! Shared date manipulation functions for calendar components.
//! All functions work with YYYY-MM-DD format strings.
use crate::stores::calendar_store::UnifiedEvent;
use chrono::{Datelike, NaiveDate, TimeZone, Utc};

/// Check if a year is a leap year
#[allow(dead_code)]
pub fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Get the number of days in a month (1-indexed months)
/// Returns 0 for invalid month values (outside 1-12 range)
#[allow(dead_code)]
pub fn days_in_month(year: i32, month: i32) -> i32 {
    match month {
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    }
}

/// Get date string (YYYY-MM-DD) from a UnifiedEvent
pub fn get_event_date(event: &UnifiedEvent) -> String {
    let ts = event.start_timestamp();
    if ts == 0 {
        return String::new();
    }
    const MAX_TIMESTAMP: i64 = 253_402_300_799; // Year 9999
    let ts_i64 = i64::try_from(ts)
        .unwrap_or(MAX_TIMESTAMP)
        .min(MAX_TIMESTAMP);
    let Some(dt) = Utc.timestamp_opt(ts_i64, 0).single() else {
        return String::new();
    };
    dt.format("%Y-%m-%d").to_string()
}
/// Get today's date as YYYY-MM-DD string
pub fn get_today() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}
/// Get month number (1-12) from a date string in YYYY-MM-DD format
pub fn get_month_from_date(date: &str) -> u32 {
    date.split('-')
        .nth(1)
        .and_then(|m| {
            m.parse::<u32>()
                .ok()
                .and_then(|v| if (1..=12).contains(&v) { Some(v) } else { None })
        })
        .unwrap_or(0)
}

/// Convert a date to days since civil epoch (Hinnant algorithm)
/// Used for efficient date arithmetic without chrono
#[allow(dead_code)]
pub fn days_from_civil(y: i32, m: i32, d: i32) -> i32 {
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    365 * y + y / 4 - y / 100 + y / 400 + (m * 153 + 2) / 5 + d - 1
}

/// Convert days since civil epoch back to date (Hinnant algorithm)
#[allow(dead_code)]
pub fn civil_from_days(g: i32) -> (i32, i32, i32) {
    let y = (10000 * (g as i64) + 14780) / 3652425;
    let mut y = y as i32;
    let mut doy = g - (365 * y + y / 4 - y / 100 + y / 400);
    if doy < 0 {
        y -= 1;
        doy = g - (365 * y + y / 4 - y / 100 + y / 400);
    }
    let mi = (100 * doy + 52) / 3060;
    let m = if mi < 10 { mi + 3 } else { mi - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let d = doy - (mi * 306 + 5) / 10 + 1;
    (y, m, d)
}
/// Get day number as display string (leading zeros stripped) from a date string
pub fn get_day_number(date: &str) -> String {
    date.split('-')
        .nth(2)
        .map(|d| {
            let trimmed = d.trim_start_matches('0');
            if trimmed.is_empty() {
                "?"
            } else {
                trimmed
            }
        })
        .unwrap_or("?")
        .to_string()
}
/// Generate a 6-week calendar grid (42 dates) starting from the Sunday
/// before the first day of the month containing the given date.
///
/// Returns Vec of date strings in YYYY-MM-DD format.
pub fn get_month_dates(date: &str) -> Vec<String> {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() < 2 {
        return vec![];
    }
    let (Ok(year), Ok(month)) = (parts[0].parse::<i32>(), parts[1].parse::<u32>()) else {
        return vec![];
    };
    let first = match NaiveDate::from_ymd_opt(year, month, 1) {
        Some(d) => d,
        None => return vec![],
    };
    // Find Sunday before (or on) the first day of the month
    let first_weekday = first.weekday().num_days_from_sunday() as i64;
    let sunday = match first.checked_sub_signed(chrono::Duration::days(first_weekday)) {
        Some(d) => d,
        None => return vec![],
    };
    let mut dates = Vec::with_capacity(42);
    let mut last_valid_date = sunday;
    for i in 0..42 {
        let d = match sunday.checked_add_signed(chrono::Duration::days(i)) {
            Some(date) => {
                last_valid_date = date;
                date
            }
            None => {
                // Use the last valid date as saturation point
                last_valid_date
            }
        };
        dates.push(d.format("%Y-%m-%d").to_string());
    }
    dates
}
