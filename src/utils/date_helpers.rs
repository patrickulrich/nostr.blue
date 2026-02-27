//! Date Helper Utilities
//!
//! Shared date manipulation functions for calendar components.
//! All functions work with YYYY-MM-DD format strings.
use crate::stores::calendar_store::UnifiedEvent;
use chrono::{Datelike, NaiveDate, TimeZone, Utc};

/// Get date string (YYYY-MM-DD) from a UnifiedEvent
pub fn get_event_date(event: &UnifiedEvent) -> String {
    let ts = event.start_timestamp();
    if ts == 0 {
        return String::new();
    }
    let dt = Utc.timestamp_opt(ts as i64, 0).single().unwrap_or_default();
    dt.format("%Y-%m-%d").to_string()
}
/// Get today's date as YYYY-MM-DD string
pub fn get_today() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}
/// Get month number (1-12) from a date string in YYYY-MM-DD format
pub fn get_month_from_date(date: &str) -> u32 {
    date.split('-').nth(1).and_then(|m| m.parse().ok()).unwrap_or(0)
}
/// Get day number as display string (leading zeros stripped) from a date string
pub fn get_day_number(date: &str) -> String {
    date.split('-').nth(2).map(|d| d.trim_start_matches('0')).unwrap_or("?").to_string()
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
    let year: i32 = parts[0].parse().unwrap_or(2024);
    let month: u32 = parts[1].parse::<u32>().unwrap_or(1);
    let first = match NaiveDate::from_ymd_opt(year, month, 1) {
        Some(d) => d,
        None => return vec![],
    };
    // Find Sunday before (or on) the first day of the month
    let first_weekday = first.weekday().num_days_from_sunday() as i64;
    let sunday = first - chrono::Duration::days(first_weekday);
    let mut dates = Vec::with_capacity(42);
    for i in 0..42 {
        let d = sunday + chrono::Duration::days(i);
        dates.push(d.format("%Y-%m-%d").to_string());
    }
    dates
}
