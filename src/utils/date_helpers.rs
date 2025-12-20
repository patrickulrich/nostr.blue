//! Date Helper Utilities
//!
//! Shared date manipulation functions for calendar components.
//! All functions work with YYYY-MM-DD format strings.

/// Get today's date as YYYY-MM-DD string
pub fn get_today() -> String {
    let date = js_sys::Date::new_0();
    format!(
        "{:04}-{:02}-{:02}",
        date.get_full_year(),
        date.get_month() + 1,
        date.get_date()
    )
}

/// Get month number (1-12) from a date string in YYYY-MM-DD format
pub fn get_month_from_date(date: &str) -> u32 {
    date.split('-')
        .nth(1)
        .and_then(|m| m.parse().ok())
        .unwrap_or(0)
}

/// Get day number as display string (leading zeros stripped) from a date string
pub fn get_day_number(date: &str) -> String {
    date.split('-')
        .nth(2)
        .map(|d| d.trim_start_matches('0'))
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

    let year: i32 = parts[0].parse().unwrap_or(2024);
    let month: i32 = parts[1].parse::<i32>().unwrap_or(1) - 1; // JS months are 0-indexed

    // First day of month
    let first = js_sys::Date::new_with_year_month_day(year as u32, month, 1);
    let first_weekday = first.get_day() as i32;

    // Go back to Sunday of that week using milliseconds
    let ms_per_day = 24.0 * 60.0 * 60.0 * 1000.0;
    let sunday_ms = first.get_time() - (first_weekday as f64 * ms_per_day);
    first.set_time(sunday_ms);

    let mut dates = Vec::with_capacity(42);
    for _ in 0..42 {
        dates.push(format!(
            "{:04}-{:02}-{:02}",
            first.get_full_year(),
            first.get_month() + 1,
            first.get_date()
        ));
        first.set_time(first.get_time() + ms_per_day);
    }

    dates
}
