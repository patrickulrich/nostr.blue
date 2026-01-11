//! Calendar Event Creation Page
//!
//! Create new calendar events (NIP-52 kinds 31922/31923)

use dioxus::prelude::*;
use dioxus::events::MouseData;
use crate::stores::{auth_store, calendar_store};
use crate::routes::Route;
use crate::utils::date_helpers::get_today;
use crate::utils::ics::{parse_ics, IcsEvent, IcsDateTime};
use crate::components::MediaUploader;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

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

    // ICS import state
    let mut ics_events = use_signal(Vec::<IcsEvent>::new);
    let mut show_ics_selector = use_signal(|| false);

    // Participant state
    // Each participant is (pubkey, display_name, role)
    let mut participants = use_signal(Vec::<(String, String, String)>::new);
    let mut participant_input = use_signal(String::new);

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

    // Add participant (closure for direct call)
    let mut do_add_participant = move || {
        let input = participant_input.read().trim().to_string();
        if input.is_empty() {
            return;
        }

        // Try to parse as npub or hex pubkey
        let (pubkey_hex, display) = if input.starts_with("npub1") {
            // Parse npub
            if let Ok(pk) = nostr_sdk::prelude::PublicKey::parse(&input) {
                (pk.to_hex(), format!("{}...", &input[..12]))
            } else {
                error_message.set(Some("Invalid npub".to_string()));
                return;
            }
        } else if input.len() == 64 && input.chars().all(|c| c.is_ascii_hexdigit()) {
            // Hex pubkey
            (input.clone(), format!("{}...", &input[..8]))
        } else {
            error_message.set(Some("Enter npub or hex pubkey".to_string()));
            return;
        };

        // Check for duplicates
        let mut parts = participants.read().clone();
        if parts.iter().any(|(pk, _, _)| pk == &pubkey_hex) {
            error_message.set(Some("Participant already added".to_string()));
            return;
        }

        // Add with default role "participant"
        parts.push((pubkey_hex, display, "participant".to_string()));
        participants.set(parts);
        participant_input.set(String::new());
        error_message.set(None);
    };

    // Add participant onclick handler
    let add_participant = move |_: Event<MouseData>| {
        do_add_participant();
    };

    // Remove participant
    let mut remove_participant = move |idx: usize| {
        let mut parts = participants.read().clone();
        if idx < parts.len() {
            parts.remove(idx);
            participants.set(parts);
        }
    };

    // Handle ICS file upload
    let handle_ics_upload = move |_evt: Event<FormData>| {
        spawn(async move {
            // Read file content from file input
            if let Ok(content) = read_ics_file_content("ics-file-input").await {
                let events = parse_ics(&content);
                if events.is_empty() {
                    error_message.set(Some("No events found in ICS file".to_string()));
                } else {
                    ics_events.set(events);
                    show_ics_selector.set(true);
                }
            } else {
                error_message.set(Some("Failed to read ICS file".to_string()));
            }
        });
    };

    // Apply selected ICS event to form
    let mut apply_ics_event = move |evt: &IcsEvent| {
        title.set(evt.title.clone());
        summary.set(evt.description.clone());

        // Parse start time
        if let Some(ref start) = evt.start {
            match start {
                IcsDateTime::Date(date_str) => {
                    event_type.set(EventType::DateBased);
                    start_date.set(date_str.clone());
                }
                IcsDateTime::DateTime(ts) | IcsDateTime::DateTimeWithTz { timestamp: ts, .. } => {
                    event_type.set(EventType::TimeBased);
                    let (date, time) = timestamp_to_date_time(*ts);
                    start_date.set(date);
                    start_time.set(time);
                }
            }
        }

        // Parse end time
        if let Some(ref end_dt) = evt.end {
            match end_dt {
                IcsDateTime::Date(date_str) => {
                    end_date.set(date_str.clone());
                }
                IcsDateTime::DateTime(ts) | IcsDateTime::DateTimeWithTz { timestamp: ts, .. } => {
                    let (date, time) = timestamp_to_date_time(*ts);
                    end_date.set(date);
                    end_time.set(time);
                }
            }
        }

        // Location
        if !evt.location.is_empty() {
            location.set(evt.location.clone());
        }

        // URL
        if !evt.url.is_empty() {
            let mut locs = locations.read().clone();
            locs.push(evt.url.clone());
            locations.set(locs);
        }

        // Close selector
        show_ics_selector.set(false);
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
        // Convert participants to (pubkey, role) tuples
        let participants_val: Vec<(String, String)> = participants.read()
            .iter()
            .map(|(pk, _, role)| (pk.clone(), role.clone()))
            .collect();

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
                        &participants_val,
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
                        &participants_val,
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

                    // ICS Import Section
                    div {
                        class: "mb-6 p-4 bg-muted/50 rounded-lg border border-border",
                        div {
                            class: "flex items-center gap-2 mb-2",
                            span { class: "text-xl", "📅" }
                            span { class: "font-medium", "Import from Calendar" }
                        }
                        p {
                            class: "text-sm text-muted-foreground mb-3",
                            "Import event details from an .ics file (iCalendar format)"
                        }
                        label {
                            class: "inline-flex items-center gap-2 px-4 py-2 bg-accent hover:bg-accent/80 rounded-lg cursor-pointer transition",
                            input {
                                r#type: "file",
                                accept: ".ics,text/calendar",
                                class: "hidden",
                                id: "ics-file-input",
                                onchange: handle_ics_upload,
                            }
                            span { class: "text-lg", "📁" }
                            span { "Choose .ics File" }
                        }
                    }

                    // ICS Event Selector Modal
                    if *show_ics_selector.read() && !ics_events.read().is_empty() {
                        div {
                            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50",
                            onclick: move |_| show_ics_selector.set(false),

                            div {
                                class: "bg-background rounded-lg shadow-xl max-w-lg w-full mx-4 max-h-[80vh] overflow-hidden",
                                onclick: move |evt| evt.stop_propagation(),

                                // Modal header
                                div {
                                    class: "p-4 border-b border-border flex items-center justify-between",
                                    h3 { class: "font-bold text-lg", "Select Event to Import" }
                                    button {
                                        class: "p-1 hover:bg-muted rounded",
                                        onclick: move |_| show_ics_selector.set(false),
                                        "✕"
                                    }
                                }

                                // Event list
                                div {
                                    class: "p-4 overflow-y-auto max-h-[60vh]",
                                    for evt in ics_events.read().iter() {
                                        {
                                            let evt_clone = evt.clone();
                                            rsx! {
                                                button {
                                                    class: "w-full p-3 mb-2 text-left bg-muted/50 hover:bg-muted rounded-lg transition",
                                                    onclick: move |_| apply_ics_event(&evt_clone),
                                                    div {
                                                        class: "font-medium",
                                                        "{evt.title}"
                                                    }
                                                    if let Some(ref start) = evt.start {
                                                        div {
                                                            class: "text-sm text-muted-foreground",
                                                            {format_ics_datetime(start)}
                                                        }
                                                    }
                                                    if !evt.location.is_empty() {
                                                        div {
                                                            class: "text-sm text-muted-foreground",
                                                            "📍 {evt.location}"
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

                    // Cover Image
                    div {
                        class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Cover Image" }

                        MediaUploader {
                            on_upload: move |url: String| {
                                image_url.set(url);
                            },
                            button_label: "Upload Event Image".to_string(),
                            input_id: "event-image-upload".to_string(),
                            show_server_selector: true,
                        }

                        // Preview uploaded image
                        if !image_url.read().is_empty() {
                            div {
                                class: "mt-3 relative",
                                img {
                                    src: "{image_url}",
                                    alt: "Event cover",
                                    class: "max-h-40 rounded-lg object-cover"
                                }
                                button {
                                    class: "absolute top-2 right-2 px-2 py-1 bg-red-500/80 text-white text-xs rounded hover:bg-red-600 transition",
                                    onclick: move |_| image_url.set(String::new()),
                                    "Remove"
                                }
                            }
                        }
                    }

                    // Participants
                    div {
                        class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Participants" }
                        p {
                            class: "text-xs text-muted-foreground mb-2",
                            "Invite people to this event (enter npub or hex pubkey)"
                        }
                        div {
                            class: "flex gap-2 mb-2",
                            input {
                                r#type: "text",
                                class: "flex-1 px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-none transition",
                                placeholder: "npub1... or hex pubkey",
                                value: "{participant_input}",
                                oninput: move |e| participant_input.set(e.value()),
                                onkeydown: move |e| {
                                    if e.key() == Key::Enter {
                                        do_add_participant();
                                    }
                                }
                            }
                            button {
                                class: "px-4 py-2 bg-accent hover:bg-accent/80 rounded-lg font-medium transition",
                                r#type: "button",
                                onclick: add_participant,
                                "Add"
                            }
                        }
                        // Participant list
                        if !participants.read().is_empty() {
                            div {
                                class: "space-y-2",
                                for (idx, (pubkey, display, role)) in participants.read().iter().enumerate() {
                                    div {
                                        class: "flex items-center gap-2 p-2 bg-muted/50 rounded-lg",
                                        key: "{pubkey}",
                                        div {
                                            class: "flex-1",
                                            div { class: "text-sm font-medium", "{display}" }
                                            div { class: "text-xs text-muted-foreground", "{role}" }
                                        }
                                        select {
                                            class: "px-2 py-1 text-xs bg-background border border-border rounded",
                                            value: "{role}",
                                            onchange: {
                                                let pubkey = pubkey.clone();
                                                move |e: Event<FormData>| {
                                                    let mut parts = participants.read().clone();
                                                    if let Some(p) = parts.iter_mut().find(|(pk, _, _)| pk == &pubkey) {
                                                        p.2 = e.value();
                                                    }
                                                    participants.set(parts);
                                                }
                                            },
                                            option { value: "participant", "Participant" }
                                            option { value: "speaker", "Speaker" }
                                            option { value: "organizer", "Organizer" }
                                            option { value: "moderator", "Moderator" }
                                        }
                                        button {
                                            class: "p-1 text-red-500 hover:text-red-600",
                                            onclick: move |_| remove_participant(idx),
                                            "✕"
                                        }
                                    }
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

/// Convert Unix timestamp to date (YYYY-MM-DD) and time (HH:MM) strings
fn timestamp_to_date_time(ts: u64) -> (String, String) {
    let date = js_sys::Date::new(&(ts as f64 * 1000.0).into());
    let year = date.get_full_year();
    let month = date.get_month() + 1; // JS months 0-indexed
    let day = date.get_date();
    let hours = date.get_hours();
    let minutes = date.get_minutes();

    let date_str = format!("{:04}-{:02}-{:02}", year, month, day);
    let time_str = format!("{:02}:{:02}", hours, minutes);

    (date_str, time_str)
}

/// Format ICS datetime for display
fn format_ics_datetime(dt: &IcsDateTime) -> String {
    match dt {
        IcsDateTime::Date(d) => d.clone(),
        IcsDateTime::DateTime(ts) | IcsDateTime::DateTimeWithTz { timestamp: ts, .. } => {
            let date = js_sys::Date::new(&(*ts as f64 * 1000.0).into());
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                date.get_full_year(),
                date.get_month() + 1,
                date.get_date(),
                date.get_hours(),
                date.get_minutes()
            )
        }
    }
}

/// Read ICS file content from file input
async fn read_ics_file_content(_file_name: &str) -> Result<String, String> {
    use wasm_bindgen_futures::JsFuture;
    use web_sys::window;

    let window = window().ok_or("No window")?;
    let document = window.document().ok_or("No document")?;

    // Get the file input element
    let input = document
        .get_element_by_id("ics-file-input")
        .ok_or("Input not found")?
        .dyn_into::<HtmlInputElement>()
        .map_err(|_| "Not an input element")?;

    let file_list = input.files().ok_or("No files")?;
    let file = file_list.get(0).ok_or("No file selected")?;

    // Read file as text
    let promise = file.text();
    let result = JsFuture::from(promise)
        .await
        .map_err(|_| "Failed to read file")?;

    result.as_string().ok_or("Could not convert to string".to_string())
}
