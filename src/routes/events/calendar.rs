//! Calendar Page
//!
//! NIP-52 Calendar Events with Day/Week/Month views
use dioxus::prelude::*;
use std::collections::HashSet;
use crate::components::{
    CalendarView, CalendarViewMode, CalendarViewSkeleton, ClientInitializing,
    MiniCalendar,
};
use crate::routes::Route;
use crate::stores::calendar_store::UnifiedEvent;
use crate::stores::{auth_store, calendar_store, nostr_client};
use crate::utils::date_helpers::{get_event_date, get_today};
use crate::utils::ics::{download_ics, export_events_to_ics};
use crate::utils::nip52::CalendarEvent;
/// Calendar page component
#[component]
pub fn Calendar() -> Element {
    let mut events = use_signal(Vec::<UnifiedEvent>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut selected_date = use_signal(get_today);
    let mut view_mode = use_signal(|| CalendarViewMode::Week);
    let mut show_all_day = use_signal(|| true);
    let mut show_timed = use_signal(|| true);
    let mut show_livestreams = use_signal(|| true);
    let filtered_events = use_memo(move || {
        let show_all_day = show_all_day();
        let show_timed = show_timed();
        let show_livestreams = show_livestreams();
        events
            .read()
            .iter()
            .filter(|e| {
                if e.is_livestream() {
                    show_livestreams
                } else if e.is_all_day() {
                    show_all_day
                } else {
                    show_timed
                }
            })
            .cloned()
            .collect::<Vec<_>>()
    });
    let all_day_count = use_memo(move || {
        events.read().iter().filter(|e| e.is_all_day()).count()
    });
    let timed_count = use_memo(move || {
        events
            .read()
            .iter()
            .filter(|e| !e.is_all_day() && !e.is_livestream())
            .count()
    });
    let livestream_count = use_memo(move || {
        events.read().iter().filter(|e| e.is_livestream()).count()
    });
    let filtered_event_dates = use_memo(move || {
        filtered_events.read().iter().map(get_event_date).collect::<HashSet<String>>()
    });
    let is_logged_in = auth_store::get_pubkey().is_some();
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        if auth_store::get_pubkey().is_none() {
            loading.set(false);
            return;
        }
        spawn(async move {
            loading.set(true);
            error.set(None);
            match calendar_store::fetch_personal_calendar_events().await {
                Ok(fetched_events) => {
                    events.set(fetched_events);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });
    let month_header = use_memo(move || format_month_header(&selected_date.read()));
    let go_prev = move |_| {
        let current = selected_date.read().clone();
        let new_date = match *view_mode.read() {
            CalendarViewMode::Day => add_days(&current, -1),
            CalendarViewMode::Week => add_days(&current, -7),
            CalendarViewMode::Month => add_months(&current, -1),
        };
        selected_date.set(new_date);
    };
    let go_next = move |_| {
        let current = selected_date.read().clone();
        let new_date = match *view_mode.read() {
            CalendarViewMode::Day => add_days(&current, 1),
            CalendarViewMode::Week => add_days(&current, 7),
            CalendarViewMode::Month => add_months(&current, 1),
        };
        selected_date.set(new_date);
    };
    let go_today = move |_| {
        selected_date.set(get_today());
    };
    let on_date_select = move |date: String| {
        selected_date.set(date);
    };
    let on_event_click = move |event: UnifiedEvent| {
        let naddr = event.naddr().to_string();
        if !naddr.is_empty() {
            let nav = navigator();
            nav.push(Route::CalendarEventDetail {
                naddr,
                from: Some("calendar".to_string()),
            });
        }
    };
    rsx! {
        div { class: "min-h-screen flex flex-col lg:flex-row",
            div { class: "hidden lg:block w-80 shrink-0 border-r border-border bg-background overflow-y-auto",
                div { class: "p-4 space-y-6",
                    MiniCalendar {
                        selected_date: selected_date.read().clone(),
                        event_dates: filtered_event_dates.read().clone(),
                        on_date_select,
                    }
                    div { class: "bg-card rounded-lg border border-border p-3",
                        h3 { class: "text-sm font-medium mb-3", "Event Types" }
                        div { class: "space-y-2",
                            EventTypeFilterRow {
                                label: "All-Day Events",
                                color: "#4285f4",
                                checked: show_all_day(),
                                count: *all_day_count.read(),
                                on_change: move |checked| show_all_day.set(checked),
                            }
                            EventTypeFilterRow {
                                label: "Timed Events",
                                color: "#34a853",
                                checked: show_timed(),
                                count: *timed_count.read(),
                                on_change: move |checked| show_timed.set(checked),
                            }
                            EventTypeFilterRow {
                                label: "Livestreams",
                                color: "#ef4444",
                                checked: show_livestreams(),
                                count: *livestream_count.read(),
                                on_change: move |checked| show_livestreams.set(checked),
                            }
                        }
                    }
                }
            }
            div { class: "flex-1 flex flex-col min-w-0",
                div { class: "sticky top-0 z-20 bg-background/95 backdrop-blur border-b border-border",
                    div { class: "px-4 py-3",
                        div { class: "flex items-center justify-between",
                            div { class: "flex items-center gap-2",
                                button {
                                    class: "p-2 hover:bg-accent rounded-lg transition",
                                    onclick: go_prev,
                                    svg {
                                        class: "w-5 h-5",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M15 19l-7-7 7-7",
                                        }
                                    }
                                }
                                button {
                                    class: "px-3 py-1.5 text-sm font-medium border border-border hover:bg-accent rounded-lg transition",
                                    onclick: go_today,
                                    "Today"
                                }
                                button {
                                    class: "p-2 hover:bg-accent rounded-lg transition",
                                    onclick: go_next,
                                    svg {
                                        class: "w-5 h-5",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M9 5l7 7-7 7",
                                        }
                                    }
                                }
                                h1 { class: "text-xl font-semibold ml-4", "{month_header}" }
                            }
                            div { class: "flex items-center gap-3",
                                if is_logged_in && !filtered_events.read().is_empty() {
                                    button {
                                        class: "hidden sm:flex items-center gap-2 px-3 py-1.5 text-sm font-medium border border-border rounded-lg hover:bg-accent transition",
                                        onclick: move |_| {
                                            let calendar_events: Vec<CalendarEvent> = filtered_events
                                                .read()
                                                .iter()
                                                .filter_map(|e| {
                                                    if let UnifiedEvent::Calendar(cal) = e {
                                                        Some(cal.clone())
                                                    } else {
                                                        None
                                                    }
                                                })
                                                .collect();
                                            if !calendar_events.is_empty() {
                                                let ics_content = export_events_to_ics(&calendar_events);
                                                let today = get_today();
                                                let filename = format!("nostr_calendar_{}.ics", today);
                                                if let Err(e) = download_ics(&filename, &ics_content) {
                                                    log::error!("Failed to download calendar: {}", e);
                                                }
                                            }
                                        },
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
                                                d: "M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12m4.5 4.5V3",
                                            }
                                        }
                                        "Export"
                                    }
                                    button {
                                        class: "sm:hidden p-2 border border-border rounded-lg hover:bg-accent transition",
                                        onclick: move |_| {
                                            let calendar_events: Vec<CalendarEvent> = filtered_events
                                                .read()
                                                .iter()
                                                .filter_map(|e| {
                                                    if let UnifiedEvent::Calendar(cal) = e {
                                                        Some(cal.clone())
                                                    } else {
                                                        None
                                                    }
                                                })
                                                .collect();
                                            if !calendar_events.is_empty() {
                                                let ics_content = export_events_to_ics(&calendar_events);
                                                let today = get_today();
                                                let filename = format!("nostr_calendar_{}.ics", today);
                                                if let Err(e) = download_ics(&filename, &ics_content) {
                                                    log::error!("Failed to download calendar: {}", e);
                                                }
                                            }
                                        },
                                        svg {
                                            class: "w-5 h-5",
                                            xmlns: "http://www.w3.org/2000/svg",
                                            fill: "none",
                                            view_box: "0 0 24 24",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            path {
                                                stroke_linecap: "round",
                                                stroke_linejoin: "round",
                                                d: "M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12m4.5 4.5V3",
                                            }
                                        }
                                    }
                                }
                                Link {
                                    to: Route::CalendarEventNew {},
                                    class: "hidden sm:flex items-center gap-2 px-3 py-1.5 text-sm font-medium bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
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
                                            d: "M12 4.5v15m7.5-7.5h-15",
                                        }
                                    }
                                    "Create"
                                }
                                Link {
                                    to: Route::CalendarEventNew {},
                                    class: "sm:hidden p-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                    svg {
                                        class: "w-5 h-5",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M12 4.5v15m7.5-7.5h-15",
                                        }
                                    }
                                }
                                div { class: "flex items-center gap-1 lg:hidden bg-muted rounded-lg p-1",
                                    button {
                                        class: if *view_mode.read() == CalendarViewMode::Day { "px-3 py-1 text-sm font-medium bg-background rounded shadow-xs" } else { "px-3 py-1 text-sm text-muted-foreground" },
                                        onclick: move |_| view_mode.set(CalendarViewMode::Day),
                                        "D"
                                    }
                                    button {
                                        class: if *view_mode.read() == CalendarViewMode::Week { "px-3 py-1 text-sm font-medium bg-background rounded shadow-xs" } else { "px-3 py-1 text-sm text-muted-foreground" },
                                        onclick: move |_| view_mode.set(CalendarViewMode::Week),
                                        "W"
                                    }
                                    button {
                                        class: if *view_mode.read() == CalendarViewMode::Month { "px-3 py-1 text-sm font-medium bg-background rounded shadow-xs" } else { "px-3 py-1 text-sm text-muted-foreground" },
                                        onclick: move |_| view_mode.set(CalendarViewMode::Month),
                                        "M"
                                    }
                                }
                                div { class: "hidden lg:flex items-center border border-border rounded-lg",
                                    button {
                                        class: if *view_mode.read() == CalendarViewMode::Day { "px-4 py-1.5 text-sm font-medium bg-primary text-primary-foreground rounded-l-lg" } else { "px-4 py-1.5 text-sm text-muted-foreground hover:bg-accent rounded-l-lg transition" },
                                        onclick: move |_| view_mode.set(CalendarViewMode::Day),
                                        "Day"
                                    }
                                    button {
                                        class: if *view_mode.read() == CalendarViewMode::Week { "px-4 py-1.5 text-sm font-medium bg-primary text-primary-foreground border-x border-border" } else { "px-4 py-1.5 text-sm text-muted-foreground hover:bg-accent border-x border-border transition" },
                                        onclick: move |_| view_mode.set(CalendarViewMode::Week),
                                        "Week"
                                    }
                                    button {
                                        class: if *view_mode.read() == CalendarViewMode::Month { "px-4 py-1.5 text-sm font-medium bg-primary text-primary-foreground rounded-r-lg" } else { "px-4 py-1.5 text-sm text-muted-foreground hover:bg-accent rounded-r-lg transition" },
                                        onclick: move |_| view_mode.set(CalendarViewMode::Month),
                                        "Month"
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "flex-1 overflow-auto",
                    if !*nostr_client::CLIENT_INITIALIZED.read() {
                        ClientInitializing {}
                    } else if !is_logged_in {
                        div { class: "flex-1 flex items-center justify-center p-8",
                            div { class: "text-center max-w-md",
                                div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
                                    svg {
                                        class: "w-8 h-8 text-muted-foreground",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "1.5",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M6.75 3v2.25M17.25 3v2.25M3 18.75V7.5a2.25 2.25 0 012.25-2.25h13.5A2.25 2.25 0 0121 7.5v11.25m-18 0A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75m-18 0v-7.5A2.25 2.25 0 015.25 9h13.5A2.25 2.25 0 0121 11.25v7.5",
                                        }
                                    }
                                }
                                h3 { class: "text-lg font-medium mb-2", "Your Personal Calendar" }
                                p { class: "text-muted-foreground mb-4",
                                    "Sign in to view your calendar events, RSVPs, and availability."
                                }
                                Link {
                                    to: Route::Settings {},
                                    class: "inline-flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition",
                                    "Sign In"
                                }
                                div { class: "mt-4",
                                    Link {
                                        to: Route::Events {},
                                        class: "text-sm text-primary hover:underline",
                                        "Browse public events instead"
                                    }
                                }
                            }
                        }
                    } else if *loading.read() {
                        div { class: "p-4", CalendarViewSkeleton {} }
                    } else if let Some(err) = error.read().as_ref() {
                        div { class: "p-8 text-center",
                            div { class: "text-4xl mb-4", "❌" }
                            h3 { class: "text-lg font-medium mb-2", "Failed to load calendar" }
                            p { class: "text-red-500", "{err}" }
                        }
                    } else {
                        CalendarView {
                            events: filtered_events.read().clone(),
                            selected_date: selected_date.read().clone(),
                            view_mode: *view_mode.read(),
                            on_date_select,
                            on_event_click,
                        }
                    }
                }
            }
        }
    }
}
/// Format month header
fn format_month_header(date: &str) -> String {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() < 2 {
        return date.to_string();
    }
    let year: i32 = parts[0].parse().unwrap_or(2024);
    let month: u32 = parts[1].parse().unwrap_or(1);
    let month_names = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    format!("{} {}", month_names[(month - 1) as usize], year)
}
/// Add days to a date
fn add_days(date: &str, days: i32) -> String {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return date.to_string();
    }
    let year: i32 = parts[0].parse().unwrap_or(2024);
    let month: i32 = parts[1].parse::<i32>().unwrap_or(1) - 1;
    let day: i32 = parts[2].parse().unwrap_or(1);
    #[cfg(feature = "web")]
    {
        let js_date = js_sys::Date::new_with_year_month_day(year as u32, month, day);
        let current_day = js_date.get_date() as i32;
        let new_day = current_day + days;
        let js_date = js_sys::Date::new_with_year_month_day(year as u32, month, new_day);
        format!(
            "{:04}-{:02}-{:02}",
            js_date.get_full_year(),
            js_date.get_month() + 1,
            js_date.get_date(),
        )
    }
    #[cfg(not(feature = "web"))]
    {
        // Simple date arithmetic without js_sys
        let total_days = day + days;
        let mut y = year;
        let mut m = month; // 0-indexed
        let mut d = total_days;
        while d > crate::utils::date_helpers::days_in_month(y, m + 1) {
            d -= crate::utils::date_helpers::days_in_month(y, m + 1);
            m += 1;
            if m >= 12 {
                m -= 12;
                y += 1;
            }
        }
        while d < 1 {
            m -= 1;
            if m < 0 {
                m += 12;
                y -= 1;
            }
            d += crate::utils::date_helpers::days_in_month(y, m + 1);
        }
        format!("{:04}-{:02}-{:02}", y, m + 1, d)
    }
}
/// Add months to a date
fn add_months(date: &str, months: i32) -> String {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return date.to_string();
    }
    let year: i32 = parts[0].parse().unwrap_or(2024);
    let month: i32 = parts[1].parse::<i32>().unwrap_or(1) - 1;
    let day: i32 = parts[2].parse().unwrap_or(1);
    let total_months = month + months;
    let new_month = ((total_months % 12) + 12) % 12;
    let year_adjustment = if total_months < 0 {
        (total_months - 11) / 12
    } else {
        total_months / 12
    };
    let new_year = year + year_adjustment;
    let max_day = crate::utils::date_helpers::days_in_month(new_year, new_month + 1);
    let clamped_day = day.min(max_day);
    format!("{:04}-{:02}-{:02}", new_year, new_month + 1, clamped_day)
}
/// Props for EventTypeFilterRow
#[derive(Props, Clone, PartialEq)]
struct EventTypeFilterRowProps {
    /// Label for this filter
    label: &'static str,
    /// Color dot hex value
    color: &'static str,
    /// Whether filter is checked
    checked: bool,
    /// Count of events matching this filter
    count: usize,
    /// Callback when checked state changes
    on_change: EventHandler<bool>,
}
/// A single row in the Event Types filter
#[component]
fn EventTypeFilterRow(props: EventTypeFilterRowProps) -> Element {
    rsx! {
        label { class: "flex items-center gap-2 cursor-pointer group",
            input {
                r#type: "checkbox",
                checked: props.checked,
                class: "w-4 h-4 rounded border-border accent-primary cursor-pointer",
                oninput: move |evt| props.on_change.call(evt.checked()),
            }
            div {
                class: "w-3 h-3 rounded-full shrink-0",
                style: "background-color: {props.color};",
            }
            span {
                class: "text-sm flex-1 group-hover:text-foreground transition",
                class: if props.checked { "" } else { "text-muted-foreground" },
                "{props.label}"
            }
            span {
                class: "text-xs px-1.5 py-0.5 rounded",
                class: if props.checked { "text-muted-foreground bg-muted" } else { "text-muted-foreground/50 bg-muted/50" },
                "{props.count}"
            }
        }
    }
}
