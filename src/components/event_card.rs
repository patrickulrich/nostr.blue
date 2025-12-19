//! Event Card Component
//!
//! Display card for NIP-52 calendar events and NIP-53 live activities

use dioxus::prelude::*;
use nostr::Timestamp;
use crate::routes::Route;
use crate::stores::calendar_store::{UnifiedEvent, get_rsvp_count};
use crate::utils::time::format_relative_time;
use crate::utils::nip52::is_online_location;

/// Event Card for grid/list display
/// `from` parameter indicates source page for back navigation ("events" or "calendar")
#[component]
pub fn EventCard(event: UnifiedEvent, #[props(default)] from: Option<String>) -> Element {
    // Format time display
    let time_display = format_event_time(&event);

    // Get location info
    let location_info = get_location_info(&event);

    // Get RSVP count for calendar events
    let rsvp_count = get_rsvp_count(event.coordinate());

    // Limit hashtags shown
    let hashtags: Vec<&str> = event.hashtags().into_iter().take(3).collect();
    let extra_tags = event.hashtags().len().saturating_sub(3);

    // Build route based on event type
    // - Livestreams (30311) go to /videos/live/:naddr
    // - Calendar events (31922/31923) go to /calendar/:naddr
    // - Meeting rooms (30313) go to /calendar/:naddr
    let detail_route = if event.is_livestream() {
        Route::LiveStreamDetail { note_id: event.naddr().to_string() }
    } else {
        Route::CalendarEventDetail { naddr: event.naddr().to_string(), from }
    };

    rsx! {
        Link {
            to: detail_route,
            class: "block rounded-lg border border-border bg-card overflow-hidden hover:shadow-md transition group",

            // Event image (if available)
            if let Some(image_url) = event.image() {
                div {
                    class: "relative h-40 overflow-hidden bg-muted",
                    img {
                        src: "{image_url}",
                        alt: "{event.title()}",
                        class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-300",
                        loading: "lazy",
                    }

                    // Live badge
                    if event.is_live() {
                        div {
                            class: "absolute top-2 left-2 px-2 py-1 bg-red-500 text-white text-xs font-bold rounded animate-pulse",
                            "LIVE"
                        }
                    }

                    // Private badge
                    if event.is_private() {
                        div {
                            class: "absolute top-2 right-2 px-2 py-1 bg-purple-600 text-white text-xs rounded flex items-center gap-1",
                            svg {
                                class: "w-3 h-3",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"
                                }
                            }
                            "Private"
                        }
                    }
                }
            } else {
                // Placeholder for events without image
                div {
                    class: "relative h-24 bg-gradient-to-br from-primary/20 to-secondary/20 flex items-center justify-center",
                    svg {
                        class: "w-12 h-12 text-muted-foreground/50",
                        xmlns: "http://www.w3.org/2000/svg",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "1.5",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M6.75 3v2.25M17.25 3v2.25M3 18.75V7.5a2.25 2.25 0 012.25-2.25h13.5A2.25 2.25 0 0121 7.5v11.25m-18 0A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75m-18 0v-7.5A2.25 2.25 0 015.25 9h13.5A2.25 2.25 0 0121 11.25v7.5"
                        }
                    }

                    // Live badge on placeholder
                    if event.is_live() {
                        div {
                            class: "absolute top-2 left-2 px-2 py-1 bg-red-500 text-white text-xs font-bold rounded animate-pulse",
                            "LIVE"
                        }
                    }

                    // Private badge on placeholder
                    if event.is_private() {
                        div {
                            class: "absolute top-2 right-2 px-2 py-1 bg-purple-600 text-white text-xs rounded flex items-center gap-1",
                            svg {
                                class: "w-3 h-3",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"
                                }
                            }
                            "Private"
                        }
                    }
                }
            }

            // Content
            div {
                class: "p-4",

                // Title
                h3 {
                    class: "font-semibold text-foreground line-clamp-2 mb-2 group-hover:text-primary transition-colors",
                    "{event.title()}"
                }

                // Time display
                div {
                    class: "flex items-center gap-2 text-sm text-muted-foreground mb-2",
                    svg {
                        class: "w-4 h-4 flex-shrink-0",
                        xmlns: "http://www.w3.org/2000/svg",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z"
                        }
                    }
                    span {
                        class: "truncate",
                        "{time_display}"
                    }
                }

                // Location
                if let Some((location, is_online)) = &location_info {
                    div {
                        class: "flex items-center gap-2 text-sm text-muted-foreground mb-3",
                        if *is_online {
                            // Video/online icon
                            svg {
                                class: "w-4 h-4 flex-shrink-0 text-blue-500",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"
                                }
                            }
                        } else {
                            // Map pin icon
                            svg {
                                class: "w-4 h-4 flex-shrink-0 text-green-500",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M17.657 16.657L13.414 20.9a1.998 1.998 0 01-2.827 0l-4.244-4.243a8 8 0 1111.314 0z"
                                }
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M15 11a3 3 0 11-6 0 3 3 0 016 0z"
                                }
                            }
                        }
                        span {
                            class: "truncate",
                            "{location}"
                        }
                    }
                }

                // Hashtags
                if !hashtags.is_empty() {
                    div {
                        class: "flex flex-wrap gap-1 mb-3",
                        for tag in hashtags {
                            span {
                                class: "px-2 py-0.5 text-xs bg-muted text-muted-foreground rounded-full",
                                "#{tag}"
                            }
                        }
                        if extra_tags > 0 {
                            span {
                                class: "px-2 py-0.5 text-xs text-muted-foreground",
                                "+{extra_tags}"
                            }
                        }
                    }
                }

                // Bottom row: RSVP count + Author avatar placeholder
                div {
                    class: "flex items-center justify-between text-xs",

                    // RSVP count
                    if rsvp_count > 0 {
                        div {
                            class: "flex items-center gap-1 text-muted-foreground",
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
                                    d: "M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"
                                }
                            }
                            span {
                                "{rsvp_count} attending"
                            }
                        }
                    } else {
                        div {}
                    }

                    // All-day indicator
                    if event.is_all_day() {
                        span {
                            class: "px-2 py-0.5 bg-accent text-accent-foreground rounded text-xs",
                            "All day"
                        }
                    }
                }
            }
        }
    }
}

/// Compact Event Card for sidebar/list views
#[component]
pub fn EventCardCompact(event: UnifiedEvent, #[props(default)] from: Option<String>) -> Element {
    let time_display = format_event_time_short(&event);
    let detail_route = if event.is_livestream() {
        Route::LiveStreamDetail { note_id: event.naddr().to_string() }
    } else {
        Route::CalendarEventDetail { naddr: event.naddr().to_string(), from }
    };

    rsx! {
        Link {
            to: detail_route,
            class: "flex items-center gap-3 p-3 hover:bg-accent/50 rounded-lg transition",

            // Thumbnail or date box
            if let Some(image_url) = event.image() {
                img {
                    src: "{image_url}",
                    alt: "{event.title()}",
                    class: "w-12 h-12 rounded object-cover flex-shrink-0",
                    loading: "lazy",
                }
            } else {
                div {
                    class: "w-12 h-12 rounded bg-primary/10 flex items-center justify-center flex-shrink-0",
                    svg {
                        class: "w-6 h-6 text-primary",
                        xmlns: "http://www.w3.org/2000/svg",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
                        }
                    }
                }
            }

            // Content
            div {
                class: "flex-1 min-w-0",

                // Title
                h4 {
                    class: "font-medium text-sm truncate",
                    "{event.title()}"
                }

                // Time
                p {
                    class: "text-xs text-muted-foreground",
                    "{time_display}"
                }
            }

            // Live indicator
            if event.is_live() {
                div {
                    class: "w-2 h-2 bg-red-500 rounded-full animate-pulse flex-shrink-0",
                }
            }
        }
    }
}

/// Skeleton loader for event cards
#[component]
pub fn EventCardSkeleton() -> Element {
    rsx! {
        div {
            class: "rounded-lg border border-border bg-card overflow-hidden animate-pulse",

            // Image placeholder
            div {
                class: "h-40 bg-muted",
            }

            // Content
            div {
                class: "p-4",

                // Title
                div {
                    class: "h-5 bg-muted rounded w-3/4 mb-2",
                }

                // Time
                div {
                    class: "flex items-center gap-2 mb-2",
                    div { class: "h-4 w-4 bg-muted rounded" }
                    div { class: "h-4 bg-muted rounded w-32" }
                }

                // Location
                div {
                    class: "flex items-center gap-2 mb-3",
                    div { class: "h-4 w-4 bg-muted rounded" }
                    div { class: "h-4 bg-muted rounded w-24" }
                }

                // Hashtags
                div {
                    class: "flex gap-1 mb-3",
                    div { class: "h-5 w-14 bg-muted rounded-full" }
                    div { class: "h-5 w-16 bg-muted rounded-full" }
                    div { class: "h-5 w-12 bg-muted rounded-full" }
                }

                // Bottom
                div {
                    class: "flex items-center justify-between",
                    div { class: "h-4 w-20 bg-muted rounded" }
                    div { class: "h-4 w-12 bg-muted rounded" }
                }
            }
        }
    }
}

/// Skeleton loader for compact event cards
#[allow(dead_code)]
#[component]
pub fn EventCardCompactSkeleton() -> Element {
    rsx! {
        div {
            class: "flex items-center gap-3 p-3 animate-pulse",

            // Thumbnail
            div {
                class: "w-12 h-12 rounded bg-muted flex-shrink-0",
            }

            // Content
            div {
                class: "flex-1 min-w-0",
                div { class: "h-4 bg-muted rounded w-3/4 mb-1" }
                div { class: "h-3 bg-muted rounded w-1/2" }
            }
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Format event time for display
fn format_event_time(event: &UnifiedEvent) -> String {
    let ts = event.start_timestamp();
    if ts == 0 {
        return "TBD".to_string();
    }

    let date = js_sys::Date::new(&(ts as f64 * 1000.0).into());

    if event.is_all_day() {
        // All-day event - just show date
        let month_names = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let weekday_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

        let month = date.get_month() as usize;
        let day = date.get_date();
        let weekday = date.get_day() as usize;

        format!("{}, {} {}", weekday_names[weekday], month_names[month], day)
    } else {
        // Time-based event - show date and time
        let month_names = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

        let month = date.get_month() as usize;
        let day = date.get_date();
        let hours = date.get_hours();
        let minutes = date.get_minutes();

        let am_pm = if hours >= 12 { "PM" } else { "AM" };
        let hour_12 = if hours == 0 { 12 } else if hours > 12 { hours - 12 } else { hours };

        format!("{} {} at {}:{:02} {}", month_names[month], day, hour_12, minutes, am_pm)
    }
}

/// Format event time (short version for compact cards)
fn format_event_time_short(event: &UnifiedEvent) -> String {
    let ts = event.start_timestamp();
    if ts == 0 {
        return "TBD".to_string();
    }

    // Use relative time for recent/upcoming events
    let now_secs = (js_sys::Date::now() / 1000.0) as u64;
    let diff = if ts > now_secs { ts - now_secs } else { now_secs - ts };

    // Within a week, use relative time
    if diff < 7 * 86400 {
        format_relative_time(Timestamp::from(ts))
    } else {
        // Farther out, use date
        let date = js_sys::Date::new(&(ts as f64 * 1000.0).into());
        let month_names = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let month = date.get_month() as usize;
        let day = date.get_date();
        format!("{} {}", month_names[month], day)
    }
}

/// Get location info (location string, is_online flag)
fn get_location_info(event: &UnifiedEvent) -> Option<(String, bool)> {
    let locations = event.locations();

    if locations.is_empty() {
        return None;
    }

    let first_loc = &locations[0];
    let is_online = is_online_location(first_loc);

    // Truncate long locations (use chars() for safe UTF-8 handling)
    let display_loc = if first_loc.chars().count() > 50 {
        format!("{}...", first_loc.chars().take(47).collect::<String>())
    } else {
        first_loc.clone()
    };

    Some((display_loc, is_online))
}
