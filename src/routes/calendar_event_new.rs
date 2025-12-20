//! Calendar Event Creation Page
//!
//! Create new calendar events (NIP-52 kinds 31922/31923)

use dioxus::prelude::*;
use crate::stores::{auth_store, calendar_store};
use crate::routes::Route;
use crate::utils::date_helpers::get_today;

/// Event type selection
#[derive(Clone, Copy, PartialEq, Default)]
pub enum EventType {
    #[default]
    TimeBased, // Kind 31923 - specific time
    DateBased, // Kind 31922 - all day
}

#[component]
pub fn CalendarEventNew() -> Element {
    let navigator = navigator();

    // Form state
    let mut title = use_signal(String::new);
    let mut summary = use_signal(String::new);
    let mut content = use_signal(String::new);
    let mut event_type = use_signal(|| EventType::TimeBased);
    let mut start_date = use_signal(get_today);
    let mut start_time = use_signal(|| "09:00".to_string());
    let mut end_date = use_signal(get_today);
    let mut end_time = use_signal(|| "10:00".to_string());
    let mut location = use_signal(String::new);
    let mut locations = use_signal(Vec::<String>::new);
    let mut image_url = use_signal(String::new);
    let mut hashtags_input = use_signal(String::new);
    let mut timezone = use_signal(get_local_timezone);
    let mut is_private = use_signal(|| false);

    // Publishing state
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);

    // Check authentication
    let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;

    // Validation - use signal() calls for proper reactivity
    let can_publish = use_memo(move || {
        let title_val = title();
        !title_val.trim().is_empty() && !is_publishing()
    });

    // Add location to list
    let add_location = move |_| {
        let loc = location.read().trim().to_string();
        if !loc.is_empty() {
            let mut locs = locations.read().clone();
            locs.push(loc);
            locations.set(locs);
            location.set(String::new());
        }
    };

    // Remove location from list
    let mut remove_location = move |idx: usize| {
        let mut locs = locations.read().clone();
        if idx < locs.len() {
            locs.remove(idx);
            locations.set(locs);
        }
    };

    // Handle close
    let handle_close = move |_| {
        navigator.go_back();
    };

    // Handle publish
    let handle_publish = move |_| {
        if !*can_publish.read() {
            return;
        }

        let title_val = title.read().clone();
        let summary_val = summary.read().clone();
        let content_val = content.read().clone();
        let event_type_val = *event_type.read();
        let start_date_val = start_date.read().clone();
        let start_time_val = start_time.read().clone();
        let end_date_val = end_date.read().clone();
        let end_time_val = end_time.read().clone();
        let locations_val = locations.read().clone();
        let single_location = location.read().clone();
        let image_val = image_url.read().clone();
        let hashtags_val = hashtags_input.read().clone();
        let timezone_val = timezone.read().clone();
        let is_private_val = *is_private.read();

        is_publishing.set(true);
        error_message.set(None);

        let nav = navigator;
        spawn(async move {
            // Combine locations
            let mut all_locations = locations_val;
            if !single_location.trim().is_empty() {
                all_locations.push(single_location.trim().to_string());
            }

            // Parse hashtags
            let hashtags: Vec<String> = hashtags_val
                .split([',', ' ', '#'])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            // Publish based on event type
            let result = match event_type_val {
                EventType::DateBased => {
                    let end_date_opt = if end_date_val != start_date_val {
                        Some(end_date_val.as_str())
                    } else {
                        None
                    };

                    calendar_store::publish_date_event(
                        &title_val,
                        &start_date_val,
                        end_date_opt,
                        if summary_val.is_empty() { None } else { Some(&summary_val) },
                        if content_val.is_empty() { None } else { Some(&content_val) },
                        if image_val.is_empty() { None } else { Some(&image_val) },
                        &all_locations,
                        &hashtags,
                        is_private_val,
                    ).await
                }
                EventType::TimeBased => {
                    // Convert date/time to timestamps
                    let start_ts = parse_datetime_to_timestamp(&start_date_val, &start_time_val);
                    let end_ts = parse_datetime_to_timestamp(&end_date_val, &end_time_val);

                    calendar_store::publish_time_event(
                        &title_val,
                        start_ts,
                        if end_ts > start_ts { Some(end_ts) } else { None },
                        if summary_val.is_empty() { None } else { Some(&summary_val) },
                        if content_val.is_empty() { None } else { Some(&content_val) },
                        if image_val.is_empty() { None } else { Some(&image_val) },
                        &all_locations,
                        &hashtags,
                        if timezone_val.is_empty() { None } else { Some(&timezone_val) },
                        is_private_val,
                    ).await
                }
            };

            match result {
                Ok(naddr) => {
                    // Navigate to the new event
                    nav.push(Route::CalendarEventDetail { naddr, from: Some("calendar".to_string()) });
                }
                Err(e) => {
                    error_message.set(Some(e));
                    is_publishing.set(false);
                }
            }
        });
    };

    rsx! {
        div {
            class: "min-h-screen bg-background",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/95 backdrop-blur border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center justify-between",
                    div {
                        class: "flex items-center gap-3",
                        button {
                            class: "p-2 -ml-2 hover:bg-accent rounded-lg transition",
                            onclick: handle_close,
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
                                    d: "M6 18L18 6M6 6l12 12"
                                }
                            }
                        }
                        h1 {
                            class: "text-lg font-bold",
                            "Create Event"
                        }
                    }
                    button {
                        class: if *can_publish.read() {
                            "px-4 py-2 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition"
                        } else {
                            "px-4 py-2 bg-muted text-muted-foreground rounded-lg font-medium cursor-not-allowed"
                        },
                        disabled: !*can_publish.read(),
                        onclick: handle_publish,
                        if *is_publishing.read() {
                            "Publishing..."
                        } else {
                            "Publish"
                        }
                    }
                }
            }

            // Content
            if !is_authenticated {
                div {
                    class: "p-8 text-center",
                    div { class: "text-4xl mb-4", "🔒" }
                    h3 { class: "text-lg font-medium mb-2", "Sign in required" }
                    p { class: "text-muted-foreground mb-4", "You need to sign in to create events" }
                    Link {
                        to: Route::Settings {},
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg inline-block",
                        "Sign In"
                    }
                }
            } else {
                div {
                    class: "p-4 max-w-2xl mx-auto",

                    // Error message
                    if let Some(err) = error_message.read().as_ref() {
                        div {
                            class: "mb-4 p-4 bg-red-500/10 border border-red-500/20 rounded-lg text-red-500",
                            "{err}"
                        }
                    }

                    // Event type selector
                    div {
                        class: "mb-6",
                        label { class: "block text-sm font-medium mb-2", "Event Type" }
                        div {
                            class: "flex gap-2",
                            button {
                                class: if *event_type.read() == EventType::TimeBased {
                                    "flex-1 py-2 px-4 bg-primary text-primary-foreground rounded-lg font-medium"
                                } else {
                                    "flex-1 py-2 px-4 bg-muted hover:bg-accent rounded-lg font-medium transition"
                                },
                                onclick: move |_| event_type.set(EventType::TimeBased),
                                "Specific Time"
                            }
                            button {
                                class: if *event_type.read() == EventType::DateBased {
                                    "flex-1 py-2 px-4 bg-primary text-primary-foreground rounded-lg font-medium"
                                } else {
                                    "flex-1 py-2 px-4 bg-muted hover:bg-accent rounded-lg font-medium transition"
                                },
                                onclick: move |_| event_type.set(EventType::DateBased),
                                "All Day"
                            }
                        }
                    }

                    // Title
                    div {
                        class: "mb-4",
                        label {
                            class: "block text-sm font-medium mb-2",
                            "Event Title *"
                        }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition",
                            placeholder: "What's happening?",
                            value: "{title}",
                            oninput: move |e| title.set(e.value())
                        }
                    }

                    // Date/Time section
                    div {
                        class: "mb-4 grid grid-cols-1 sm:grid-cols-2 gap-4",

                        // Start date
                        div {
                            label { class: "block text-sm font-medium mb-2", "Start Date" }
                            input {
                                r#type: "date",
                                class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition",
                                value: "{start_date}",
                                oninput: move |e| start_date.set(e.value())
                            }
                        }

                        // Start time (only for time-based)
                        if *event_type.read() == EventType::TimeBased {
                            div {
                                label { class: "block text-sm font-medium mb-2", "Start Time" }
                                input {
                                    r#type: "time",
                                    class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition",
                                    value: "{start_time}",
                                    oninput: move |e| start_time.set(e.value())
                                }
                            }
                        }

                        // End date
                        div {
                            label { class: "block text-sm font-medium mb-2", "End Date" }
                            input {
                                r#type: "date",
                                class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition",
                                value: "{end_date}",
                                oninput: move |e| end_date.set(e.value())
                            }
                        }

                        // End time (only for time-based)
                        if *event_type.read() == EventType::TimeBased {
                            div {
                                label { class: "block text-sm font-medium mb-2", "End Time" }
                                input {
                                    r#type: "time",
                                    class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition",
                                    value: "{end_time}",
                                    oninput: move |e| end_time.set(e.value())
                                }
                            }
                        }
                    }

                    // Timezone (for time-based)
                    if *event_type.read() == EventType::TimeBased {
                        div {
                            class: "mb-4",
                            label { class: "block text-sm font-medium mb-2", "Timezone" }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition",
                                placeholder: "e.g., America/New_York",
                                value: "{timezone}",
                                oninput: move |e| timezone.set(e.value())
                            }
                        }
                    }

                    // Location
                    div {
                        class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Location" }
                        div {
                            class: "flex gap-2",
                            input {
                                r#type: "text",
                                class: "flex-1 px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition",
                                placeholder: "Address or online link",
                                value: "{location}",
                                oninput: move |e| location.set(e.value())
                            }
                            button {
                                class: "px-4 py-3 bg-muted hover:bg-accent rounded-lg transition",
                                onclick: add_location,
                                "+"
                            }
                        }
                        // Show added locations
                        if !locations.read().is_empty() {
                            div {
                                class: "mt-2 flex flex-wrap gap-2",
                                for (idx, loc) in locations.read().iter().enumerate() {
                                    div {
                                        key: "{idx}",
                                        class: "flex items-center gap-2 px-3 py-1 bg-muted rounded-full text-sm",
                                        span { "{loc}" }
                                        button {
                                            class: "hover:text-red-500 transition",
                                            onclick: move |_| remove_location(idx),
                                            "×"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Summary
                    div {
                        class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Summary" }
                        textarea {
                            class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition resize-none",
                            rows: "2",
                            placeholder: "Brief description of the event",
                            value: "{summary}",
                            oninput: move |e| summary.set(e.value())
                        }
                    }

                    // Full description
                    div {
                        class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Details" }
                        textarea {
                            class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition resize-none",
                            rows: "5",
                            placeholder: "Full event description (markdown supported)",
                            value: "{content}",
                            oninput: move |e| content.set(e.value())
                        }
                    }

                    // Image URL
                    div {
                        class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Cover Image URL" }
                        input {
                            r#type: "url",
                            class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition",
                            placeholder: "https://...",
                            value: "{image_url}",
                            oninput: move |e| image_url.set(e.value())
                        }
                        // Preview
                        if !image_url.read().is_empty() {
                            div {
                                class: "mt-2",
                                img {
                                    src: "{image_url}",
                                    alt: "Preview",
                                    class: "max-h-32 rounded-lg object-cover"
                                }
                            }
                        }
                    }

                    // Hashtags
                    div {
                        class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Hashtags" }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition",
                            placeholder: "conference, meetup, bitcoin (comma or space separated)",
                            value: "{hashtags_input}",
                            oninput: move |e| hashtags_input.set(e.value())
                        }
                    }

                    // Private event toggle
                    div {
                        class: "mb-6",
                        label {
                            class: "flex items-center gap-3 cursor-pointer",
                            input {
                                r#type: "checkbox",
                                class: "w-5 h-5 rounded border-border",
                                checked: *is_private.read(),
                                oninput: move |e| is_private.set(e.checked())
                            }
                            div {
                                div { class: "font-medium", "Private Event" }
                                div { class: "text-sm text-muted-foreground", "Only visible to you and invited participants (NIP-59)" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// Helper functions

fn get_local_timezone() -> String {
    // Try to get timezone from JS using a simpler approach
    let result = js_sys::eval("Intl.DateTimeFormat().resolvedOptions().timeZone");
    if let Ok(tz) = result {
        if let Some(s) = tz.as_string() {
            return s;
        }
    }
    "UTC".to_string()
}

fn parse_datetime_to_timestamp(date: &str, time: &str) -> u64 {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return 0;
    }

    let year: i32 = parts[0].parse().unwrap_or(2024);
    let month: i32 = parts[1].parse::<i32>().unwrap_or(1) - 1; // JS months 0-indexed
    let day: i32 = parts[2].parse().unwrap_or(1);

    let time_parts: Vec<&str> = time.split(':').collect();
    let hours: u32 = time_parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minutes: u32 = time_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

    let js_date = js_sys::Date::new_with_year_month_day(year as u32, month, day);
    js_date.set_hours(hours);
    js_date.set_minutes(minutes);

    (js_date.get_time() / 1000.0) as u64
}
