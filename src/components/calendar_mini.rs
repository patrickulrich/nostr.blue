//! Mini Calendar Component
//!
//! A compact calendar for sidebar navigation

use dioxus::prelude::*;
use std::collections::HashSet;

// ============================================================================
// Component
// ============================================================================

/// Props for MiniCalendar
#[derive(Props, Clone, PartialEq)]
pub struct MiniCalendarProps {
    /// Currently selected date (YYYY-MM-DD)
    pub selected_date: String,
    /// Dates that have events (for highlighting)
    #[props(default)]
    pub event_dates: HashSet<String>,
    /// Callback when a date is selected
    #[props(default)]
    pub on_date_select: Option<EventHandler<String>>,
    /// Callback when month changes
    #[props(default)]
    pub on_month_change: Option<EventHandler<String>>,
}

/// Mini calendar for sidebar
#[component]
pub fn MiniCalendar(props: MiniCalendarProps) -> Element {
    // Clone props for use in closures and rsx
    let selected_date_for_effect = props.selected_date.clone();
    let selected_date_for_highlight = props.selected_date.clone();
    let event_dates = props.event_dates.clone();
    let on_date_select_handler = props.on_date_select;

    // Parse the selected date to get year/month for display
    let mut display_date = use_signal(|| props.selected_date.clone());

    // When selected_date changes, update display
    // Only re-run when selected_date prop changes (not every render)
    use_effect(use_reactive!(|selected_date_for_effect| {
        display_date.set(selected_date_for_effect.clone());
    }));

    let today = get_today();

    // Get month dates (6 weeks)
    let month_dates = use_memo(move || get_month_dates(&display_date.read()));

    // Parse display month info
    let month_info = use_memo(move || {
        let date = display_date.read();
        let parts: Vec<&str> = date.split('-').collect();
        if parts.len() >= 2 {
            let year: i32 = parts[0].parse().unwrap_or(2024);
            let month: u32 = parts[1].parse().unwrap_or(1);
            (year, month)
        } else {
            (2024, 1)
        }
    });

    let current_month = month_info.read().1;
    let (year, month) = *month_info.read();

    let month_names = ["January", "February", "March", "April", "May", "June",
                       "July", "August", "September", "October", "November", "December"];
    // Safe bounds check for month index (1-12 maps to 0-11)
    let month_name = month_names.get((month.saturating_sub(1)) as usize).unwrap_or(&"Unknown");

    // Navigation handlers
    let go_prev_month = {
        let on_month_change = props.on_month_change;
        move |_| {
            let (y, m) = *month_info.read();
            let (new_year, new_month) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
            let new_date = format!("{:04}-{:02}-01", new_year, new_month);
            display_date.set(new_date.clone());
            if let Some(handler) = &on_month_change {
                handler.call(new_date);
            }
        }
    };

    let go_next_month = {
        let on_month_change = props.on_month_change;
        move |_| {
            let (y, m) = *month_info.read();
            let (new_year, new_month) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            let new_date = format!("{:04}-{:02}-01", new_year, new_month);
            display_date.set(new_date.clone());
            if let Some(handler) = &on_month_change {
                handler.call(new_date);
            }
        }
    };

    let go_today = {
        let on_date_select = props.on_date_select;
        move |_| {
            let today = get_today();
            display_date.set(today.clone());
            if let Some(handler) = &on_date_select {
                handler.call(today);
            }
        }
    };

    rsx! {
        div {
            class: "mini-calendar bg-card rounded-lg border border-border p-3",

            // Header with navigation
            div {
                class: "flex items-center justify-between mb-3",

                // Previous month
                button {
                    class: "p-1 hover:bg-accent rounded transition",
                    onclick: go_prev_month,
                    svg {
                        class: "w-4 h-4",
                        xmlns: "http://www.w3.org/2000/svg",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M15 19l-7-7 7-7"
                        }
                    }
                }

                // Month/Year display
                button {
                    class: "text-sm font-medium hover:bg-accent px-2 py-1 rounded transition",
                    onclick: go_today,
                    "{month_name} {year}"
                }

                // Next month
                button {
                    class: "p-1 hover:bg-accent rounded transition",
                    onclick: go_next_month,
                    svg {
                        class: "w-4 h-4",
                        xmlns: "http://www.w3.org/2000/svg",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M9 5l7 7-7 7"
                        }
                    }
                }
            }

            // Weekday headers
            div {
                class: "grid grid-cols-7 mb-1",
                for day in ["S", "M", "T", "W", "T", "F", "S"] {
                    div {
                        class: "text-center text-xs text-muted-foreground font-medium",
                        "{day}"
                    }
                }
            }

            // Date grid
            div {
                class: "grid grid-cols-7 gap-0.5",
                for date in month_dates.read().iter() {
                    {
                        let is_today = *date == today;
                        let is_selected = *date == selected_date_for_highlight;
                        let is_other_month = get_month_from_date(date) != current_month;
                        let has_events = event_dates.contains(date);
                        let day_num = get_day_number(date);

                        rsx! {
                            button {
                                key: "{date}",
                                class: "w-7 h-7 text-xs rounded-full flex items-center justify-center relative transition",
                                class: if is_selected { "bg-primary text-primary-foreground" }
                                       else if is_today { "bg-accent font-bold" }
                                       else if is_other_month { "text-muted-foreground/50" }
                                       else { "hover:bg-accent" },
                                onclick: {
                                    let date = date.clone();
                                    let handler = on_date_select_handler;
                                    move |_| {
                                        if let Some(h) = &handler {
                                            h.call(date.clone());
                                        }
                                    }
                                },

                                "{day_num}"

                                // Event indicator dot
                                if has_events && !is_selected {
                                    div {
                                        class: "absolute bottom-0.5 w-1 h-1 bg-primary rounded-full"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get today's date as YYYY-MM-DD
fn get_today() -> String {
    let date = js_sys::Date::new_0();
    format!(
        "{:04}-{:02}-{:02}",
        date.get_full_year(),
        date.get_month() + 1,
        date.get_date()
    )
}

/// Get month number from date string
fn get_month_from_date(date: &str) -> u32 {
    date.split('-')
        .nth(1)
        .and_then(|m| m.parse().ok())
        .unwrap_or(0)
}

/// Get day number from date string
fn get_day_number(date: &str) -> String {
    date.split('-')
        .nth(2)
        .map(|d| d.trim_start_matches('0'))
        .unwrap_or("?")
        .to_string()
}

/// Get month dates (6 weeks grid)
fn get_month_dates(date: &str) -> Vec<String> {
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

/// Skeleton loader
#[component]
pub fn MiniCalendarSkeleton() -> Element {
    rsx! {
        div {
            class: "animate-pulse bg-card rounded-lg border border-border p-3",
            div {
                class: "h-6 bg-muted rounded mb-3"
            }
            div {
                class: "grid grid-cols-7 gap-1",
                for _ in 0..42 {
                    div {
                        class: "w-7 h-7 bg-muted rounded-full"
                    }
                }
            }
        }
    }
}
