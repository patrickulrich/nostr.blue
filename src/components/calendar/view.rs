//! Calendar View Component
//!
//! Displays events in Day, Week, or Month views
use dioxus::prelude::*;
use std::collections::BTreeMap;
use crate::routes::Route;
use crate::stores::calendar_store::UnifiedEvent;
use crate::utils::date_helpers::{
    get_day_number, get_event_date, get_month_dates, get_month_from_date, get_today,
};
const HOUR_HEIGHT_PX: f32 = 64.0;
const MIN_EVENT_HEIGHT_PX: f32 = 30.0;
/// Calendar view mode
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalendarViewMode {
    Day,
    #[default]
    Week,
    Month,
}
/// Position data for event rendering
#[derive(Clone, Debug, PartialEq)]
struct EventPosition {
    top: f32,
    height: f32,
    left: f32,
    width: f32,
    column: usize,
    total_columns: usize,
}
/// Event with calculated position
#[derive(Clone, Debug, PartialEq)]
struct PositionedEvent {
    event: UnifiedEvent,
    position: EventPosition,
}
/// Props for CalendarView
#[derive(Props, Clone, PartialEq)]
pub struct CalendarViewProps {
    /// Events to display
    pub events: Vec<UnifiedEvent>,
    /// Currently selected date (YYYY-MM-DD)
    pub selected_date: String,
    /// View mode
    #[props(default = CalendarViewMode::Week)]
    pub view_mode: CalendarViewMode,
    /// Callback when date is selected
    #[props(default)]
    pub on_date_select: Option<EventHandler<String>>,
    /// Callback when event is clicked
    #[props(default)]
    pub on_event_click: Option<EventHandler<UnifiedEvent>>,
}
/// Main calendar view component
#[component]
pub fn CalendarView(props: CalendarViewProps) -> Element {
    match props.view_mode {
        CalendarViewMode::Day => {
            rsx! {
                DayView {
                    events: props.events.clone(),
                    date: props.selected_date.clone(),
                    on_event_click: props.on_event_click,
                }
            }
        }
        CalendarViewMode::Week => {
            rsx! {
                WeekView {
                    events: props.events.clone(),
                    selected_date: props.selected_date.clone(),
                    on_date_select: props.on_date_select,
                    on_event_click: props.on_event_click,
                }
            }
        }
        CalendarViewMode::Month => {
            rsx! {
                MonthView {
                    events: props.events.clone(),
                    selected_date: props.selected_date.clone(),
                    on_date_select: props.on_date_select,
                    on_event_click: props.on_event_click,
                }
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
struct DayViewProps {
    events: Vec<UnifiedEvent>,
    date: String,
    on_event_click: Option<EventHandler<UnifiedEvent>>,
}
#[component]
fn DayView(props: DayViewProps) -> Element {
    let day_events: Vec<UnifiedEvent> = props
        .events
        .iter()
        .filter(|e| get_event_date(e) == props.date)
        .cloned()
        .collect();
    let positioned = position_day_events(&day_events, &props.date);
    let day_header = format_day_header(&props.date);
    rsx! {
        div { class: "calendar-day-view",
            div { class: "sticky top-0 z-10 bg-background border-b border-border p-3",
                h2 { class: "text-lg font-semibold", "{day_header}" }
            }
            {render_all_day_events(&day_events, props.on_event_click)}
            div {
                class: "relative",
                style: "height: {24.0 * HOUR_HEIGHT_PX}px;",
                for hour in 0..24 {
                    div {
                        key: "hour-{hour}",
                        class: "absolute w-full border-t border-border/50 flex",
                        style: "top: {hour as f32 * HOUR_HEIGHT_PX}px; height: {HOUR_HEIGHT_PX}px;",
                        div { class: "w-16 pr-2 text-right text-xs text-muted-foreground shrink-0",
                            "{format_hour(hour)}"
                        }
                        div { class: "flex-1 border-l border-border/30" }
                    }
                }
                div { class: "absolute inset-0 left-16",
                    for pe in positioned.iter() {
                        {render_positioned_event(pe, props.on_event_click)}
                    }
                }
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
struct WeekViewProps {
    events: Vec<UnifiedEvent>,
    selected_date: String,
    on_date_select: Option<EventHandler<String>>,
    on_event_click: Option<EventHandler<UnifiedEvent>>,
}
#[component]
fn WeekView(props: WeekViewProps) -> Element {
    let week_dates = get_week_dates(&props.selected_date);
    let mut events_by_date: BTreeMap<String, Vec<UnifiedEvent>> = BTreeMap::new();
    for event in props.events.iter() {
        let date = get_event_date(event);
        events_by_date.entry(date).or_default().push(event.clone());
    }
    let today = get_today();
    rsx! {
        div { class: "calendar-week-view h-full flex flex-col",
            div { class: "flex border-b border-border",
                div { class: "w-16 shrink-0" }
                for date in week_dates.iter() {
                    div {
                        key: "{date}",
                        class: "flex-1 text-center py-2 border-l border-border first:border-l-0",
                        class: if *date == today { "bg-primary/10" } else { "" },
                        div { class: "text-xs text-muted-foreground", "{get_weekday_short(date)}" }
                        div {
                            class: "text-lg font-semibold",
                            class: if *date == today { "text-primary" } else { "" },
                            "{get_day_number(date)}"
                        }
                    }
                }
            }
            div { class: "flex border-b border-border bg-muted/30",
                div { class: "w-16 shrink-0 p-1 text-xs text-muted-foreground", "All day" }
                for date in week_dates.iter() {
                    div {
                        key: "allday-{date}",
                        class: "flex-1 min-h-[40px] border-l border-border/50 p-1",
                        {
                            let day_events = events_by_date.get(date).cloned().unwrap_or_default();
                            rsx! {
                                for event in day_events.iter().filter(|e| e.is_all_day()) {
                                    {
                                        let bg_color = get_event_color(event);
                                        let style = format!("background-color: {}; opacity: 0.9;", bg_color);
                                        rsx! {
                                            div {
                                                key: "{event.coordinate()}",
                                                class: "text-xs text-white rounded px-1 py-0.5 truncate mb-1 cursor-pointer hover:opacity-100 transition",
                                                style: "{style}",
                                                onclick: {
                                                    let event = event.clone();
                                                    let handler = props.on_event_click;
                                                    move |e| {
                                                        e.stop_propagation();
                                                        if let Some(h) = &handler {
                                                            h.call(event.clone());
                                                        }
                                                    }
                                                },
                                                "{event.title()}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "flex-1 overflow-auto",
                div {
                    class: "relative flex",
                    style: "min-height: {24.0 * HOUR_HEIGHT_PX}px;",
                    div { class: "w-16 shrink-0",
                        for hour in 0..24 {
                            div {
                                key: "time-{hour}",
                                class: "h-16 pr-2 text-right text-xs text-muted-foreground",
                                "{format_hour(hour)}"
                            }
                        }
                    }
                    for date in week_dates.iter() {
                        div {
                            key: "col-{date}",
                            class: "flex-1 relative border-l border-border/50",
                            class: if *date == today { "bg-primary/5" } else { "" },
                            for hour in 0..24 {
                                div {
                                    key: "line-{hour}",
                                    class: "absolute w-full border-t border-border/30",
                                    style: "top: {hour as f32 * HOUR_HEIGHT_PX}px;",
                                }
                            }
                            {
                                let day_events = events_by_date.get(date).cloned().unwrap_or_default();
                                let positioned = position_day_events(&day_events, date);
                                rsx! {
                                    for pe in positioned.iter() {
                                        {render_positioned_event(pe, props.on_event_click)}
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
#[derive(Props, Clone, PartialEq)]
struct MonthViewProps {
    events: Vec<UnifiedEvent>,
    selected_date: String,
    on_date_select: Option<EventHandler<String>>,
    on_event_click: Option<EventHandler<UnifiedEvent>>,
}
#[component]
fn MonthView(props: MonthViewProps) -> Element {
    let month_dates = get_month_dates(&props.selected_date);
    let mut events_by_date: BTreeMap<String, Vec<UnifiedEvent>> = BTreeMap::new();
    for event in props.events.iter() {
        let date = get_event_date(event);
        events_by_date.entry(date).or_default().push(event.clone());
    }
    let today = get_today();
    let current_month = get_month_from_date(&props.selected_date);
    rsx! {
        div { class: "calendar-month-view",
            div { class: "grid grid-cols-7 border-b border-border",
                for day in ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] {
                    div { class: "p-2 text-center text-sm font-medium text-muted-foreground",
                        "{day}"
                    }
                }
            }
            div { class: "grid grid-cols-7",
                for date in month_dates.iter() {
                    {
                        let is_today = *date == today;
                        let is_other_month = get_month_from_date(date) != current_month;
                        let day_events = events_by_date.get(date).cloned().unwrap_or_default();
                        let events_count = day_events.len();
                        rsx! {
                            div {
                                key: "{date}",
                                class: "min-h-[100px] border-b border-r border-border p-1 cursor-pointer hover:bg-accent/50 transition",
                                class: if is_other_month { "bg-muted/30" } else { "" },
                                onclick: {
                                    let date = date.clone();
                                    let handler = props.on_date_select;
                                    move |_| {
                                        if let Some(h) = &handler {
                                            h.call(date.clone());
                                        }
                                    }
                                },
                                div {
                                    class: "flex items-center justify-center w-7 h-7 mb-1",
                                    class: if is_today { "bg-primary text-primary-foreground rounded-full" } else if is_other_month { "text-muted-foreground" } else { "" },
                                    "{get_day_number(date)}"
                                }
                                for event in day_events.iter().take(3) {
                                    {
                                        let bg_color = get_event_color(event);
                                        let style = format!("background-color: {}; opacity: 0.85;", bg_color);
                                        rsx! {
                                            div {
                                                key: "{event.coordinate()}",
                                                class: "text-xs text-white truncate px-1 py-0.5 mb-0.5 rounded cursor-pointer hover:opacity-100 transition",
                                                style: "{style}",
                                                onclick: {
                                                    let event = event.clone();
                                                    let handler = props.on_event_click;
                                                    move |e| {
                                                        e.stop_propagation();
                                                        if let Some(h) = &handler {
                                                            h.call(event.clone());
                                                        }
                                                    }
                                                },
                                                "{format_month_event(event)}"
                                            }
                                        }
                                    }
                                }
                                if events_count > 3 {
                                    div { class: "text-xs text-muted-foreground", "+{events_count - 3} more" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Get short weekday name
fn get_weekday_short(date_str: &str) -> String {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return "?".to_string();
    }
    #[cfg(feature = "web")]
    {
        let year: u32 = parts[0].parse().unwrap_or(2024);
        let month: i32 = parts[1].parse().unwrap_or(1);
        let day: i32 = parts[2].parse().unwrap_or(1);
        let date = js_sys::Date::new_with_year_month_day(year, month - 1, day);
        let weekday = date.get_day() as usize;
        const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        WEEKDAYS.get(weekday).unwrap_or(&"???").to_string()
    }
    #[cfg(not(feature = "web"))]
    {
        // Simple day-of-week calculation using Zeller-like approach
        let year: i32 = parts[0].parse().unwrap_or(2024);
        let month: i32 = parts[1].parse().unwrap_or(1);
        let day: i32 = parts[2].parse().unwrap_or(1);
        let (y, m) = if month <= 2 { (year - 1, month + 12) } else { (year, month) };
        let weekday = ((day + (13 * (m + 1)) / 5 + y + y / 4 - y / 100 + y / 400) % 7 + 6) % 7;
        const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        WEEKDAYS.get(weekday as usize).unwrap_or(&"???").to_string()
    }
}
/// Format hour for display
fn format_hour(hour: u32) -> String {
    if hour == 0 {
        "12 AM".to_string()
    } else if hour < 12 {
        format!("{} AM", hour)
    } else if hour == 12 {
        "12 PM".to_string()
    } else {
        format!("{} PM", hour - 12)
    }
}
/// Format day header
fn format_day_header(date_str: &str) -> String {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return date_str.to_string();
    }
    let month: i32 = parts[1].parse().unwrap_or(1);
    let day: i32 = parts[2].parse().unwrap_or(1);
    #[cfg(feature = "web")]
    {
        let year: u32 = parts[0].parse().unwrap_or(2024);
        let js_date = js_sys::Date::new_with_year_month_day(year, month - 1, day);
        let weekday = js_date.get_day() as usize;
        const WEEKDAY_NAMES: [&str; 7] = [
            "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
        ];
        const MONTH_NAMES: [&str; 12] = [
            "January", "February", "March", "April", "May", "June",
            "July", "August", "September", "October", "November", "December",
        ];
        let weekday_name = WEEKDAY_NAMES.get(weekday).unwrap_or(&"");
        let month_idx = month.saturating_sub(1) as usize;
        let month_name = MONTH_NAMES.get(month_idx).unwrap_or(&"");
        let year = parts[0].parse::<u32>().unwrap_or(2024);
        format!("{}, {} {}, {}", weekday_name, month_name, day, year)
    }
    #[cfg(not(feature = "web"))]
    {
        let year: i32 = parts[0].parse().unwrap_or(2024);
        let (y, m) = if month <= 2 { (year - 1, month + 12) } else { (year, month) };
        let weekday = ((day + (13 * (m + 1)) / 5 + y + y / 4 - y / 100 + y / 400) % 7 + 6) % 7;
        const WEEKDAY_NAMES: [&str; 7] = [
            "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
        ];
        const MONTH_NAMES: [&str; 12] = [
            "January", "February", "March", "April", "May", "June",
            "July", "August", "September", "October", "November", "December",
        ];
        let weekday_name = WEEKDAY_NAMES.get(weekday as usize).unwrap_or(&"");
        let month_idx = month.saturating_sub(1) as usize;
        let month_name = MONTH_NAMES.get(month_idx).unwrap_or(&"");
        format!("{}, {} {}, {}", weekday_name, month_name, day, year)
    }
}
/// Get week dates for a given date
#[cfg(feature = "web")]
fn get_week_dates(date: &str) -> Vec<String> {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return vec![];
    }
    let year: u32 = parts[0].parse().unwrap_or(2024);
    let month: i32 = parts[1].parse::<i32>().unwrap_or(1).clamp(1, 12) - 1;
    let day: i32 = parts[2].parse().unwrap_or(1);
    let js_date = js_sys::Date::new_with_year_month_day(year, month, day);
    let current_weekday = js_date.get_day();
    const MS_PER_DAY: f64 = 24.0 * 60.0 * 60.0 * 1000.0;
    js_date.set_time(js_date.get_time() - (current_weekday as f64 * MS_PER_DAY));
    let mut dates = Vec::with_capacity(7);
    for _ in 0..7 {
        dates
            .push(
                format!(
                    "{:04}-{:02}-{:02}",
                    js_date.get_full_year(),
                    js_date.get_month() + 1,
                    js_date.get_date(),
                ),
            );
        js_date.set_date(js_date.get_date() + 1);
    }
    dates
}

#[cfg(not(feature = "web"))]
fn get_week_dates(date: &str) -> Vec<String> {
    // Simple fallback: parse date and compute week using chrono-free arithmetic
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return vec![];
    }
    let year: i32 = parts[0].parse().unwrap_or(2024);
    let month: i32 = parts[1].parse().unwrap_or(1);
    let day: i32 = parts[2].parse().unwrap_or(1);

    // Convert to days since epoch using a simplified calculation
    fn days_from_civil(y: i32, m: i32, d: i32) -> i32 {
        let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
        365 * y + y / 4 - y / 100 + y / 400 + (m * 153 + 2) / 5 + d - 1
    }
    fn civil_from_days(g: i32) -> (i32, i32, i32) {
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

    let total_days = days_from_civil(year, month, day);
    let dow = ((total_days % 7) + 7) % 7; // 0=Wed in Hinnant's civil calendar
    let sunday_offset = (dow + 3) % 7; // days since previous Sunday
    let sunday_days = total_days - sunday_offset;

    let mut dates = Vec::with_capacity(7);
    for i in 0..7 {
        let (y, m, d) = civil_from_days(sunday_days + i);
        dates.push(format!("{:04}-{:02}-{:02}", y, m, d));
    }
    dates
}
/// Position events for a day with overlap handling
fn position_day_events(events: &[UnifiedEvent], _date: &str) -> Vec<PositionedEvent> {
    let mut timed: Vec<_> = events.iter().filter(|e| !e.is_all_day()).cloned().collect();
    if timed.is_empty() {
        return vec![];
    }
    timed.sort_by_key(|e| e.start_timestamp());
    let mut positioned: Vec<PositionedEvent> = Vec::new();
    let mut columns: Vec<Vec<usize>> = vec![];
    for (idx, event) in timed.iter().enumerate() {
        let ts = event.start_timestamp();
        let start_minutes = {
            #[cfg(feature = "web")]
            {
                let date = js_sys::Date::new(&(ts as f64 * 1000.0).into());
                (date.get_hours() * 60 + date.get_minutes()) as f32
            }
            #[cfg(not(feature = "web"))]
            {
                use chrono::{Local, TimeZone, Timelike};
                let dt = match Local.timestamp_opt(ts as i64, 0).single() {
                    Some(dt) => dt,
                    None => {
                        log::warn!("Invalid timestamp {} fell back to epoch", ts);
                        Local.timestamp_opt(0, 0).single().unwrap_or_default()
                    }
                };
                (dt.hour() * 60 + dt.minute()) as f32
            }
        };
        let duration_minutes = get_event_duration(event);
        let top = (start_minutes / 60.0) * HOUR_HEIGHT_PX;
        let height = f32::max(
            (duration_minutes / 60.0) * HOUR_HEIGHT_PX,
            MIN_EVENT_HEIGHT_PX,
        );
        let mut placed = false;
        for (col_idx, col) in columns.iter_mut().enumerate() {
            let overlaps = col
                .iter()
                .any(|&other_idx| {
                    let other = &positioned[other_idx];
                    let other_end = other.position.top + other.position.height;
                    let this_end = top + height;
                    !(other_end <= top || this_end <= other.position.top)
                });
            if !overlaps {
                col.push(idx);
                positioned
                    .push(PositionedEvent {
                        event: event.clone(),
                        position: EventPosition {
                            top,
                            height,
                            left: 0.0,
                            width: 100.0,
                            column: col_idx,
                            total_columns: 1,
                        },
                    });
                placed = true;
                break;
            }
        }
        if !placed {
            let col_idx = columns.len();
            columns.push(vec![idx]);
            positioned
                .push(PositionedEvent {
                    event: event.clone(),
                    position: EventPosition {
                        top,
                        height,
                        left: 0.0,
                        width: 100.0,
                        column: col_idx,
                        total_columns: 1,
                    },
                });
        }
    }
    let n = positioned.len();
    if n == 0 {
        return positioned;
    }
    let events_overlap = |a: &PositionedEvent, b: &PositionedEvent| -> bool {
        let a_end = a.position.top + a.position.height;
        let b_end = b.position.top + b.position.height;
        !(a_end <= b.position.top || b_end <= a.position.top)
    };
    let mut visited = vec![false; n];
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    for i in 0..n {
        if visited[i] {
            continue;
        }
        let mut cluster = Vec::new();
        let mut stack = vec![i];
        while let Some(idx) = stack.pop() {
            if visited[idx] {
                continue;
            }
            visited[idx] = true;
            cluster.push(idx);
            for j in 0..n {
                if !visited[j] && events_overlap(&positioned[idx], &positioned[j]) {
                    stack.push(j);
                }
            }
        }
        clusters.push(cluster);
    }
    for cluster in clusters {
        let max_col = cluster
            .iter()
            .map(|&i| positioned[i].position.column)
            .max()
            .unwrap_or(0);
        let cluster_total = max_col + 1;
        let col_width = 95.0 / cluster_total as f32;
        for &i in &cluster {
            positioned[i].position.total_columns = cluster_total;
            positioned[i].position.left = (positioned[i].position.column as f32
                * col_width) + 2.0;
            positioned[i].position.width = col_width - 2.0;
        }
    }
    positioned
}
/// Get event duration in minutes
fn get_event_duration(event: &UnifiedEvent) -> f32 {
    match event {
        UnifiedEvent::Calendar(e) => {
            if let (Some(end), start) = (e.end_timestamp(), e.start_timestamp()) {
                if end > start {
                    return (end - start) as f32 / 60.0;
                }
            }
            60.0
        }
        UnifiedEvent::Live(_) => 120.0,
    }
}
/// Render all-day events section
fn render_all_day_events(
    events: &[UnifiedEvent],
    on_click: Option<EventHandler<UnifiedEvent>>,
) -> Element {
    let all_day: Vec<_> = events.iter().filter(|e| e.is_all_day()).collect();
    if all_day.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "border-b border-border p-2 bg-muted/30",
            div { class: "flex items-center gap-2 flex-wrap",
                for event in all_day {
                    {
                        let bg_color = get_event_color(event);
                        let style = format!("background-color: {}; opacity: 0.9;", bg_color);
                        rsx! {
                            div {
                                key: "{event.coordinate()}",
                                class: "text-sm text-white rounded px-2 py-1 cursor-pointer hover:opacity-100 transition",
                                style: "{style}",
                                onclick: {
                                    let event = (*event).clone();
                                    let handler = on_click;
                                    move |e: Event<MouseData>| {
                                        e.stop_propagation();
                                        if let Some(h) = &handler {
                                            h.call(event.clone());
                                        }
                                    }
                                },
                                "{event.title()}"
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Get event color based on type (matches Event Types filter)
fn get_event_color(event: &UnifiedEvent) -> &'static str {
    if event.is_livestream() {
        "#ef4444"
    } else if event.is_all_day() {
        "#4285f4"
    } else {
        "#34a853"
    }
}
/// Render a positioned event
fn render_positioned_event(
    pe: &PositionedEvent,
    on_event_click: Option<EventHandler<UnifiedEvent>>,
) -> Element {
    let bg_color = get_event_color(&pe.event);
    let style = format!(
        "position: absolute; top: {}px; left: {}%; width: {}%; height: {}px; background-color: {};",
        pe.position.top,
        pe.position.left,
        pe.position.width,
        pe.position.height,
        bg_color,
    );
    if let Some(handler) = on_event_click {
        let event = pe.event.clone();
        return rsx! {
            div {
                class: "block text-white rounded-md p-1 overflow-hidden text-xs cursor-pointer hover:opacity-90 transition shadow-xs",
                style: "{style}",
                onclick: move |_| handler.call(event.clone()),
                div { class: "font-medium truncate", "{pe.event.title()}" }
                if pe.position.height > 40.0 {
                    div { class: "opacity-90 truncate", "{format_event_time(&pe.event)}" }
                }
            }
        };
    }
    let naddr = pe.event.naddr();
    if naddr.is_empty() {
        return rsx! {
            div {
                class: "block text-white rounded-md p-1 overflow-hidden text-xs cursor-default opacity-70 shadow-xs",
                style: "{style}",
                div { class: "font-medium truncate", "{pe.event.title()}" }
                if pe.position.height > 40.0 {
                    div { class: "opacity-90 truncate", "{format_event_time(&pe.event)}" }
                }
            }
        };
    }
    let detail_route = if pe.event.is_livestream() {
        Route::LiveStreamDetail {
            note_id: naddr.to_string(),
        }
    } else {
        Route::CalendarEventDetail {
            naddr: naddr.to_string(),
            from: Some("calendar".to_string()),
        }
    };
    rsx! {
        Link {
            to: detail_route,
            class: "block text-white rounded-md p-1 overflow-hidden text-xs cursor-pointer hover:opacity-90 transition shadow-xs",
            style: "{style}",
            div { class: "font-medium truncate", "{pe.event.title()}" }
            if pe.position.height > 40.0 {
                div { class: "opacity-90 truncate", "{format_event_time(&pe.event)}" }
            }
        }
    }
}
/// Format event time for display
fn format_event_time(event: &UnifiedEvent) -> String {
    let ts = event.start_timestamp();
    if ts == 0 {
        return "Time TBD".to_string();
    }
    #[cfg(feature = "web")]
    {
        let date = js_sys::Date::new(&(ts as f64 * 1000.0).into());
        let hours = date.get_hours();
        let minutes = date.get_minutes();
        let am_pm = if hours >= 12 { "PM" } else { "AM" };
        let hour_12 = if hours == 0 { 12 } else if hours > 12 { hours - 12 } else { hours };
        if minutes == 0 {
            format!("{} {}", hour_12, am_pm)
        } else {
            format!("{}:{:02} {}", hour_12, minutes, am_pm)
        }
    }
    #[cfg(not(feature = "web"))]
    {
        use chrono::{Local, TimeZone, Timelike};
        let dt = Local.timestamp_opt(ts as i64, 0).single().unwrap_or_default();
        let hours = dt.hour();
        let minutes = dt.minute();
        let am_pm = if hours >= 12 { "PM" } else { "AM" };
        let hour_12 = if hours == 0 { 12 } else if hours > 12 { hours - 12 } else { hours };
        if minutes == 0 {
            format!("{} {}", hour_12, am_pm)
        } else {
            format!("{}:{:02} {}", hour_12, minutes, am_pm)
        }
    }
}
/// Format event for month view
fn format_month_event(event: &UnifiedEvent) -> String {
    if event.is_all_day() {
        event.title().to_string()
    } else {
        let time = format_event_time(event);
        format!("{} {}", time, event.title())
    }
}
/// Skeleton loader for calendar view
#[component]
pub fn CalendarViewSkeleton() -> Element {
    rsx! {
        div { class: "animate-pulse",
            div { class: "h-12 bg-muted rounded mb-2" }
            div { class: "grid grid-cols-7 gap-1",
                for _ in 0..42 {
                    div { class: "h-24 bg-muted rounded" }
                }
            }
        }
    }
}
