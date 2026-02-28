//! Calendar Event Creation Page
//!
//! Create new calendar events (NIP-52 kinds 31922/31923)
use crate::components::MediaUploader;
use crate::routes::Route;
use crate::services::geocoding::{self, GeoLocation};
use crate::services::profile_search::{
    get_contact_pubkeys, search_cached_profiles, search_profiles, ProfileSearchResult,
};
use crate::stores::{auth_store, calendar_store};
use crate::utils::date_helpers::get_today;
#[cfg(feature = "web")]
use crate::utils::ics::parse_ics;
use crate::utils::ics::{IcsDateTime, IcsEvent};
use crate::utils::validation::is_valid_http_url;
use dioxus::events::MouseData;
use dioxus::prelude::*;
use nostr_sdk::prelude::ToBech32;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
#[cfg(feature = "web")]
use web_sys::HtmlInputElement;
/// Maximum ICS file size (1MB)
#[cfg(feature = "web")]
const MAX_ICS_FILE_SIZE: u64 = 1_048_576;
/// Valid participant roles per NIP-52
const VALID_ROLES: &[&str] = &["participant", "speaker", "organizer", "moderator"];
/// Event type selection
#[derive(Clone, Copy, PartialEq, Default)]
pub enum EventType {
    #[default]
    TimeBased,
    DateBased,
}
#[component]
pub fn CalendarEventNew() -> Element {
    let navigator = navigator();
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
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    #[allow(unused_mut)]
    let mut ics_events = use_signal(Vec::<IcsEvent>::new);
    let mut show_ics_selector = use_signal(|| false);
    let mut participants = use_signal(Vec::<(String, String, String)>::new);
    let mut participant_input = use_signal(String::new);
    // Fix 1: End date/time auto-sync
    let mut end_manually_set = use_signal(|| false);
    // Fix 2: Location autocomplete
    let mut location_suggestions = use_signal(Vec::<GeoLocation>::new);
    let mut show_location_dropdown = use_signal(|| false);
    let mut location_debounce = use_signal(|| 0u32);
    // Fix 3: Participant search
    let mut participant_results = use_signal(Vec::<ProfileSearchResult>::new);
    let mut show_participant_dropdown = use_signal(|| false);
    let mut participant_debounce = use_signal(|| 0u32);
    let mut contact_pubkeys = use_signal(Vec::<nostr_sdk::prelude::PublicKey>::new);
    let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
    let can_publish = use_memo(move || {
        let title_val = title();
        !title_val.trim().is_empty() && !is_publishing()
    });
    // Fix 1: Auto-sync end date/time when start changes
    use_effect(move || {
        let sd = start_date.read().clone();
        let st = start_time.read().clone();
        let et = *event_type.read();
        if !*end_manually_set.peek() {
            match et {
                EventType::TimeBased => {
                    let start_ts = parse_datetime_to_timestamp(&sd, &st);
                    if start_ts > 0 {
                        let (new_date, new_time) = timestamp_to_date_time(start_ts + 3600);
                        end_date.set(new_date);
                        end_time.set(new_time);
                    }
                }
                EventType::DateBased => {
                    end_date.set(sd);
                }
            }
        }
    });
    // Fix 3: Load contacts on mount for participant search
    use_effect(move || {
        spawn(async move {
            let contacts = get_contact_pubkeys().await;
            contact_pubkeys.set(contacts);
        });
    });
    let add_location = move |_| {
        let loc = location.read().trim().to_string();
        if !loc.is_empty() {
            let mut locs = locations.read().clone();
            if !locs.contains(&loc) {
                locs.push(loc);
                locations.set(locs);
            }
            location.set(String::new());
        }
    };
    let mut remove_location = move |idx: usize| {
        let mut locs = locations.read().clone();
        if idx < locs.len() {
            locs.remove(idx);
            locations.set(locs);
        }
    };
    let mut do_add_participant = move || {
        let input = participant_input.read().trim().to_string();
        if input.is_empty() {
            return;
        }
        let pk = match nostr_sdk::prelude::PublicKey::parse(&input) {
            Ok(pk) => pk,
            Err(_) => {
                error_message
                    .set(
                        Some(
                            "Invalid pubkey. Use hex, npub, or nostr:npub format"
                                .to_string(),
                        ),
                    );
                return;
            }
        };
        let pubkey_hex = pk.to_hex();
        let display = pk
            .to_bech32()
            .map(|s| format!("{}...", &s[..12]))
            .unwrap_or_else(|_| format!("{}...", &pubkey_hex[..8]));
        let mut parts = participants.read().clone();
        if parts.iter().any(|(pk, _, _)| pk == &pubkey_hex) {
            error_message.set(Some("Participant already added".to_string()));
            return;
        }
        parts.push((pubkey_hex, display, "participant".to_string()));
        participants.set(parts);
        participant_input.set(String::new());
        error_message.set(None);
    };
    let add_participant = move |_: Event<MouseData>| {
        do_add_participant();
    };
    let mut remove_participant = move |pubkey_to_remove: String| {
        let mut parts = participants.read().clone();
        parts.retain(|(pk, _, _)| pk != &pubkey_to_remove);
        participants.set(parts);
    };
    let handle_ics_upload = move |_evt: Event<FormData>| {
        #[cfg(feature = "web")]
        {
            let window = match web_sys::window() {
                Some(w) => w,
                None => {
                    error_message.set(Some("Failed to access window".to_string()));
                    return;
                }
            };
            let document = match window.document() {
                Some(d) => d,
                None => {
                    error_message.set(Some("Failed to access document".to_string()));
                    return;
                }
            };
            if let Some(input) = document
                .get_element_by_id("ics-file-input")
                .and_then(|e| e.dyn_into::<HtmlInputElement>().ok())
            {
                if let Some(files) = input.files() {
                    if let Some(file) = files.get(0) {
                        let size = file.size() as u64;
                        if size > MAX_ICS_FILE_SIZE {
                            error_message
                                .set(
                                    Some(
                                        format!(
                                            "ICS file too large ({:.1} MB). Maximum size is 1 MB.",
                                            size as f64 / 1_048_576.0,
                                        ),
                                    ),
                                );
                            clear_file_input("ics-file-input");
                            return;
                        }
                    }
                }
            }
            spawn(async move {
                if let Ok(content) = read_ics_file_content("ics-file-input").await {
                    let events = parse_ics(&content);
                    if events.is_empty() {
                        error_message.set(Some("No events found in ICS file".to_string()));
                        clear_file_input("ics-file-input");
                    } else {
                        error_message.set(None);
                        ics_events.set(events);
                        show_ics_selector.set(true);
                        clear_file_input("ics-file-input");
                    }
                } else {
                    error_message.set(Some("Failed to read ICS file".to_string()));
                    clear_file_input("ics-file-input");
                }
            });
        }
        #[cfg(not(feature = "web"))]
        {
            error_message.set(Some("ICS upload is not supported on this platform".to_string()));
        }
    };
    let mut apply_ics_event = move |evt: &IcsEvent| {
        title.set(evt.title.clone());
        content.set(evt.description.clone());
        let desc = &evt.description;
        if desc.len() > 200 {
            let safe_boundary = desc
                .char_indices()
                .take_while(|(i, _)| *i < 200)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(desc.len().min(200));
            let truncate_at = desc[..safe_boundary].rfind(' ').unwrap_or(safe_boundary);
            summary.set(format!("{}...", &desc[..truncate_at]));
        } else {
            summary.set(desc.clone());
        }
        if let Some(ref start) = evt.start {
            match start {
                IcsDateTime::Date(date_str) => {
                    event_type.set(EventType::DateBased);
                    start_date.set(date_str.clone());
                }
                IcsDateTime::DateTime(ts) => {
                    event_type.set(EventType::TimeBased);
                    let (date, time) = timestamp_to_date_time(*ts);
                    start_date.set(date);
                    start_time.set(time);
                }
                IcsDateTime::DateTimeWithTz { timestamp: ts, timezone: tz } => {
                    event_type.set(EventType::TimeBased);
                    let (date, time) = timestamp_to_date_time_in_tz(*ts, tz);
                    start_date.set(date);
                    start_time.set(time);
                    timezone.set(tz.to_string());
                }
            }
        }
        if let Some(ref end_dt) = evt.end {
            end_manually_set.set(true);
            match end_dt {
                IcsDateTime::Date(date_str) => {
                    end_date.set(date_str.clone());
                }
                IcsDateTime::DateTime(ts) => {
                    let (date, time) = timestamp_to_date_time(*ts);
                    end_date.set(date);
                    end_time.set(time);
                }
                IcsDateTime::DateTimeWithTz { timestamp: ts, timezone: tz } => {
                    let (date, time) = timestamp_to_date_time_in_tz(*ts, tz);
                    end_date.set(date);
                    end_time.set(time);
                }
            }
        }
        if !evt.location.is_empty() {
            location.set(evt.location.clone());
        }
        if !evt.url.is_empty() {
            let mut locs = locations.read().clone();
            if !locs.contains(&evt.url) {
                locs.push(evt.url.clone());
            }
            locations.set(locs);
        }
        show_ics_selector.set(false);
    };
    let handle_close = move |_| {
        navigator.go_back();
    };
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
        let participants_val: Vec<(String, String)> = participants
            .read()
            .iter()
            .filter_map(|(pk, _, role)| {
                if VALID_ROLES.contains(&role.as_str()) {
                    Some((pk.clone(), role.clone()))
                } else {
                    log::warn!("Skipping participant with invalid role: {}", role);
                    None
                }
            })
            .collect();
        is_publishing.set(true);
        error_message.set(None);
        let nav = navigator;
        spawn(async move {
            let mut all_locations = locations_val;
            if !single_location.trim().is_empty() {
                all_locations.push(single_location.trim().to_string());
            }
            let hashtags: Vec<String> = hashtags_val
                .split([',', ' ', '#'])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
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
                            if summary_val.is_empty() {
                                None
                            } else {
                                Some(&summary_val)
                            },
                            if content_val.is_empty() {
                                None
                            } else {
                                Some(&content_val)
                            },
                            if image_val.is_empty() { None } else { Some(&image_val) },
                            &all_locations,
                            &hashtags,
                            &participants_val,
                        )
                        .await
                }
                EventType::TimeBased => {
                    let start_ts = parse_datetime_to_timestamp(
                        &start_date_val,
                        &start_time_val,
                    );
                    let end_ts = parse_datetime_to_timestamp(
                        &end_date_val,
                        &end_time_val,
                    );
                    calendar_store::publish_time_event(
                            &title_val,
                            start_ts,
                            if end_ts > start_ts { Some(end_ts) } else { None },
                            if summary_val.is_empty() {
                                None
                            } else {
                                Some(&summary_val)
                            },
                            if content_val.is_empty() {
                                None
                            } else {
                                Some(&content_val)
                            },
                            if image_val.is_empty() { None } else { Some(&image_val) },
                            &all_locations,
                            &hashtags,
                            &participants_val,
                            if timezone_val.is_empty() {
                                None
                            } else {
                                Some(&timezone_val)
                            },
                        )
                        .await
                }
            };
            match result {
                Ok(naddr) => {
                    nav.push(Route::CalendarEventDetail {
                        naddr,
                        from: Some("calendar".to_string()),
                    });
                }
                Err(e) => {
                    error_message.set(Some(e));
                    is_publishing.set(false);
                }
            }
        });
    };
    rsx! {
        div { class: "min-h-screen bg-background",
            div { class: "sticky top-0 z-20 bg-background/95 backdrop-blur border-b border-border",
                div { class: "px-4 py-3 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        button {
                            class: "p-1 hover:bg-accent rounded transition",
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
                                    d: "M6 18L18 6M6 6l12 12",
                                }
                            }
                        }
                        h1 { class: "text-lg font-bold", "Create Event" }
                    }
                    button {
                        class: if *can_publish.read() { "px-4 py-2 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition" } else { "px-4 py-2 bg-muted text-muted-foreground rounded-lg font-medium cursor-not-allowed" },
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
            if !is_authenticated {
                div { class: "p-8 text-center",
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
                div { class: "p-4 max-w-2xl mx-auto",
                    if let Some(err) = error_message.read().as_ref() {
                        div { class: "mb-4 p-4 bg-red-500/10 border border-red-500/20 rounded-lg text-red-500",
                            "{err}"
                        }
                    }
                    div { class: "mb-6 p-4 bg-muted/50 rounded-lg border border-border",
                        div { class: "flex items-center gap-2 mb-2",
                            span { class: "text-xl", "📅" }
                            span { class: "font-medium", "Import from Calendar" }
                        }
                        p { class: "text-sm text-muted-foreground mb-3",
                            "Import event details from an .ics file (iCalendar format)"
                        }
                        label { class: "inline-flex items-center gap-2 px-4 py-2 bg-accent hover:bg-accent/80 rounded-lg cursor-pointer transition",
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
                    if *show_ics_selector.read() && !ics_events.read().is_empty() {
                        div {
                            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
                            onclick: move |_| show_ics_selector.set(false),
                            div {
                                class: "bg-background rounded-lg shadow-xl max-w-lg w-full mx-4 max-h-[80vh] overflow-hidden",
                                onclick: move |evt| evt.stop_propagation(),
                                div { class: "p-4 border-b border-border flex items-center justify-between",
                                    h3 { class: "font-bold text-lg", "Select Event to Import" }
                                    button {
                                        class: "p-1 hover:bg-muted rounded",
                                        onclick: move |_| show_ics_selector.set(false),
                                        "✕"
                                    }
                                }
                                div { class: "p-4 overflow-y-auto max-h-[60vh]",
                                    for (idx , evt) in ics_events.read().iter().enumerate() {
                                        {
                                            let evt_clone = evt.clone();
                                            let key_str = format!("{}-{}", idx, evt.title);
                                            rsx! {
                                                button {
                                                    key: "{key_str}",
                                                    class: "w-full p-3 mb-2 text-left bg-muted/50 hover:bg-muted rounded-lg transition",
                                                    onclick: move |_| apply_ics_event(&evt_clone),
                                                    div { class: "font-medium", "{evt.title}" }
                                                    if let Some(ref start) = evt.start {
                                                        div { class: "text-sm text-muted-foreground", {format_ics_datetime(start)} }
                                                    }
                                                    if !evt.location.is_empty() {
                                                        div { class: "text-sm text-muted-foreground", "📍 {evt.location}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "mb-6",
                        label { class: "block text-sm font-medium mb-2", "Event Type" }
                        div { class: "flex gap-2",
                            button {
                                class: if *event_type.read() == EventType::TimeBased { "flex-1 py-2 px-4 bg-primary text-primary-foreground rounded-lg font-medium" } else { "flex-1 py-2 px-4 bg-muted hover:bg-accent rounded-lg font-medium transition" },
                                onclick: move |_| event_type.set(EventType::TimeBased),
                                "Specific Time"
                            }
                            button {
                                class: if *event_type.read() == EventType::DateBased { "flex-1 py-2 px-4 bg-primary text-primary-foreground rounded-lg font-medium" } else { "flex-1 py-2 px-4 bg-muted hover:bg-accent rounded-lg font-medium transition" },
                                onclick: move |_| event_type.set(EventType::DateBased),
                                "All Day"
                            }
                        }
                    }
                    div { class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Event Title *" }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition",
                            placeholder: "What's happening?",
                            value: "{title}",
                            oninput: move |e| title.set(e.value()),
                        }
                    }
                    div { class: "mb-4 grid grid-cols-1 sm:grid-cols-2 gap-4",
                        div {
                            label { class: "block text-sm font-medium mb-2", "Start Date" }
                            input {
                                r#type: "date",
                                class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition",
                                value: "{start_date}",
                                oninput: move |e| start_date.set(e.value()),
                            }
                        }
                        if *event_type.read() == EventType::TimeBased {
                            div {
                                label { class: "block text-sm font-medium mb-2", "Start Time" }
                                input {
                                    r#type: "time",
                                    class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition",
                                    value: "{start_time}",
                                    oninput: move |e| start_time.set(e.value()),
                                }
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-2", "End Date" }
                            input {
                                r#type: "date",
                                class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition",
                                value: "{end_date}",
                                oninput: move |e| {
                                    end_manually_set.set(true);
                                    end_date.set(e.value());
                                },
                            }
                        }
                        if *event_type.read() == EventType::TimeBased {
                            div {
                                label { class: "block text-sm font-medium mb-2", "End Time" }
                                input {
                                    r#type: "time",
                                    class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition",
                                    value: "{end_time}",
                                    oninput: move |e| {
                                        end_manually_set.set(true);
                                        end_time.set(e.value());
                                    },
                                }
                            }
                        }
                    }
                    if *event_type.read() == EventType::TimeBased {
                        div { class: "mb-4",
                            label { class: "block text-sm font-medium mb-2", "Timezone" }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition",
                                placeholder: "e.g., America/New_York",
                                value: "{timezone}",
                                oninput: move |e| timezone.set(e.value()),
                            }
                        }
                    }
                    div { class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Location" }
                        div { class: "relative",
                            div { class: "flex gap-2",
                                input {
                                    r#type: "text",
                                    class: "flex-1 px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition",
                                    placeholder: "Search address or enter online link",
                                    value: "{location}",
                                    oninput: move |e| {
                                        let val = e.value();
                                        location.set(val.clone());
                                        if val.trim().len() >= 3 {
                                            show_location_dropdown.set(true);
                                            let current_id = *location_debounce.peek() + 1;
                                            location_debounce.set(current_id);
                                            let query = val.trim().to_string();
                                            spawn(async move {
                                                crate::platform::timer::sleep_ms(300).await;
                                                if *location_debounce.peek() != current_id { return; }
                                                if let Ok(results) = geocoding::geocode_suggestions(&query, 10).await {
                                                    if *location_debounce.peek() != current_id { return; }
                                                    location_suggestions.set(results);
                                                } else {
                                                    location_suggestions.set(Vec::new());
                                                }
                                            });
                                        } else {
                                            show_location_dropdown.set(false);
                                            location_suggestions.set(vec![]);
                                        }
                                    },
                                    onfocusout: move |_| {
                                        // Delay hiding to allow click on suggestion
                                        spawn(async move {
                                            crate::platform::timer::sleep_ms(200).await;
                                            show_location_dropdown.set(false);
                                        });
                                    },
                                }
                                button {
                                    class: "px-4 py-3 bg-muted hover:bg-accent rounded-lg transition",
                                    onclick: add_location,
                                    "+"
                                }
                            }
                            if *show_location_dropdown.read() && !location_suggestions.read().is_empty() {
                                div { class: "absolute left-0 right-0 top-full mt-1 z-50 bg-background border border-border rounded-lg shadow-lg max-h-60 overflow-y-auto",
                                    for (idx, suggestion) in location_suggestions.read().iter().enumerate() {
                                        {
                                            let display = suggestion.display_name.clone();
                                            rsx! {
                                                button {
                                                    key: "{idx}-{display}",
                                                    class: "w-full text-left px-4 py-3 hover:bg-accent transition text-sm border-b border-border last:border-b-0",
                                                    onmousedown: move |e| e.prevent_default(),
                                                    onclick: {
                                                        let display = display.clone();
                                                        move |_| {
                                                            location.set(display.clone());
                                                            show_location_dropdown.set(false);
                                                            location_suggestions.set(vec![]);
                                                        }
                                                    },
                                                    "{display}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !locations.read().is_empty() {
                            div { class: "mt-2 flex flex-wrap gap-2",
                                for (idx , loc) in locations.read().iter().enumerate() {
                                    div {
                                        key: "{loc}",
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
                    div { class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Summary" }
                        textarea {
                            class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition resize-none",
                            rows: "2",
                            placeholder: "Brief description of the event",
                            value: "{summary}",
                            oninput: move |e| summary.set(e.value()),
                        }
                    }
                    div { class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Details" }
                        textarea {
                            class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition resize-none",
                            rows: "5",
                            placeholder: "Full event description (markdown supported)",
                            value: "{content}",
                            oninput: move |e| content.set(e.value()),
                        }
                    }
                    div { class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Cover Image" }
                        MediaUploader {
                            on_upload: move |url: String| {
                                image_url.set(url);
                            },
                            button_label: "Upload Event Image".to_string(),
                            input_id: "event-image-upload".to_string(),
                            show_server_selector: true,
                        }
                        if !image_url.read().is_empty() {
                            div { class: "mt-3 relative",
                                img {
                                    src: "{image_url}",
                                    alt: "Event cover",
                                    class: "max-h-40 rounded-lg object-cover",
                                }
                                button {
                                    class: "absolute top-2 right-2 px-2 py-1 bg-red-500/80 text-white text-xs rounded hover:bg-red-600 transition",
                                    onclick: move |_| image_url.set(String::new()),
                                    "Remove"
                                }
                            }
                        }
                    }
                    div { class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Participants" }
                        p { class: "text-xs text-muted-foreground mb-2",
                            "Search by name or paste npub"
                        }
                        div { class: "relative mb-2",
                            div { class: "flex gap-2",
                                input {
                                    r#type: "text",
                                    class: "flex-1 px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition",
                                    placeholder: "Search by name or paste npub",
                                    value: "{participant_input}",
                                    oninput: move |e| {
                                        let val = e.value();
                                        participant_input.set(val.clone());
                                        let q = val.trim().to_string();
                                        if q.len() >= 2 {
                                            show_participant_dropdown.set(true);
                                            let contacts = contact_pubkeys.peek().clone();
                                            let cached = search_cached_profiles(&q, 8, &contacts, &[]);
                                            participant_results.set(cached.clone());
                                            if q.len() >= 3 && cached.len() < 5 {
                                                let current_id = *participant_debounce.peek() + 1;
                                                participant_debounce.set(current_id);
                                                let query_snapshot = q.clone();
                                                spawn(async move {
                                                    crate::platform::timer::sleep_ms(300).await;
                                                    if *participant_debounce.peek() != current_id { return; }
                                                    if let Ok(results) = search_profiles(&query_snapshot, 8, true).await {
                                                        if *participant_debounce.peek() != current_id { return; }
                                                        participant_results.set(results);
                                                    } else {
                                                        if *participant_debounce.peek() != current_id { return; }
                                                        participant_results.set(Vec::new());
                                                    }
                                                });
                                            }
                                        } else {
                                            show_participant_dropdown.set(false);
                                            participant_results.set(vec![]);
                                        }
                                    },
                                    onkeydown: move |e| {
                                        if e.key() == Key::Enter {
                                            e.prevent_default();
                                            show_participant_dropdown.set(false);
                                            do_add_participant();
                                        }
                                    },
                                    onfocusout: move |_| {
                                        spawn(async move {
                                            crate::platform::timer::sleep_ms(200).await;
                                            show_participant_dropdown.set(false);
                                        });
                                    },
                                }
                                button {
                                    class: "px-4 py-2 bg-accent hover:bg-accent/80 rounded-lg font-medium transition",
                                    r#type: "button",
                                    onclick: add_participant,
                                    "Add"
                                }
                            }
                            if *show_participant_dropdown.read() && !participant_results.read().is_empty() {
                                div { class: "absolute left-0 right-0 top-full mt-1 z-50 bg-background border border-border rounded-lg shadow-lg max-h-60 overflow-y-auto",
                                    for result in participant_results.read().iter() {
                                        {
                                            let pubkey_hex = result.pubkey.to_hex();
                                            let display = result.get_display_name();
                                            let username = result.get_username();
                                            let picture = result.picture.clone();
                                            let is_contact = result.is_contact;
                                            let initial = display.chars().next().unwrap_or('?').to_uppercase().to_string();
                                            rsx! {
                                                button {
                                                    key: "{pubkey_hex}",
                                                    class: "w-full text-left px-4 py-2 hover:bg-accent transition flex items-center gap-3 border-b border-border last:border-b-0",
                                                    onmousedown: move |e| e.prevent_default(),
                                                    onclick: {
                                                        let pubkey_hex = pubkey_hex.clone();
                                                        let display = display.clone();
                                                        move |_| {
                                                            let mut parts = participants.read().clone();
                                                            if parts.iter().any(|(pk, _, _)| pk == &pubkey_hex) {
                                                                error_message.set(Some("Participant already added".to_string()));
                                                            } else {
                                                                parts.push((pubkey_hex.clone(), display.clone(), "participant".to_string()));
                                                                participants.set(parts);
                                                                error_message.set(None);
                                                            }
                                                            participant_input.set(String::new());
                                                            show_participant_dropdown.set(false);
                                                            participant_results.set(vec![]);
                                                        }
                                                    },
                                                    if let Some(ref pic) = picture.as_ref().filter(|u| is_valid_http_url(u)) {
                                                        img {
                                                            src: "{pic}",
                                                            alt: "{display}",
                                                            class: "w-8 h-8 rounded-full object-cover",
                                                        }
                                                    } else {
                                                        div { class: "w-8 h-8 rounded-full bg-primary/20 flex items-center justify-center text-sm font-medium",
                                                            "{initial}"
                                                        }
                                                    }
                                                    div { class: "flex-1 min-w-0",
                                                        div { class: "text-sm font-medium truncate", "{display}" }
                                                        if let Some(ref uname) = username {
                                                            div { class: "text-xs text-muted-foreground truncate", "@{uname}" }
                                                        }
                                                    }
                                                    if is_contact {
                                                        span { class: "text-xs bg-primary/10 text-primary px-2 py-0.5 rounded-full", "Following" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !participants.read().is_empty() {
                            div { class: "space-y-2",
                                for (_idx , (pubkey , display , role)) in participants.read().iter().enumerate() {
                                    div {
                                        class: "flex items-center gap-2 p-2 bg-muted/50 rounded-lg",
                                        key: "{pubkey}",
                                        div { class: "flex-1",
                                            div { class: "text-sm font-medium", "{display}" }
                                            div { class: "text-xs text-muted-foreground",
                                                "{role}"
                                            }
                                        }
                                        select {
                                            class: "px-2 py-1 text-xs bg-background border border-border rounded",
                                            value: "{role}",
                                            onchange: {
                                                let pubkey = pubkey.clone();
                                                move |e: Event<FormData>| {
                                                    let new_role = e.value();
                                                    if !VALID_ROLES.contains(&new_role.as_str()) {
                                                        log::warn!("Invalid role value received: {}", new_role);
                                                        return;
                                                    }
                                                    let mut parts = participants.read().clone();
                                                    if let Some(p) = parts.iter_mut().find(|(pk, _, _)| pk == &pubkey) {
                                                        p.2 = new_role;
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
                                            onclick: {
                                                let pubkey_for_remove = pubkey.clone();
                                                move |_| remove_participant(pubkey_for_remove.clone())
                                            },
                                            "✕"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "mb-4",
                        label { class: "block text-sm font-medium mb-2", "Hashtags" }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-3 bg-muted rounded-lg border border-border focus:border-primary focus:outline-hidden transition",
                            placeholder: "conference, meetup, bitcoin (comma or space separated)",
                            value: "{hashtags_input}",
                            oninput: move |e| hashtags_input.set(e.value()),
                        }
                    }
                }
            }
        }
    }
}
#[cfg(feature = "web")]
fn get_local_timezone() -> String {
    let formatter = js_sys::Intl::DateTimeFormat::new(
        &js_sys::Array::new(),
        &js_sys::Object::new(),
    );
    let resolved = formatter.resolved_options();
    js_sys::Reflect::get(&resolved, &"timeZone".into())
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| "UTC".to_string())
}

#[cfg(not(feature = "web"))]
fn get_local_timezone() -> String {
    "UTC".to_string()
}
#[cfg(feature = "web")]
fn parse_datetime_to_timestamp(date: &str, time: &str) -> u64 {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return 0;
    }
    let year: i32 = parts[0].parse().unwrap_or(2024);
    let month: i32 = parts[1].parse::<i32>().unwrap_or(1) - 1;
    let day: i32 = parts[2].parse().unwrap_or(1);
    let time_parts: Vec<&str> = time.split(':').collect();
    let hours: u32 = time_parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minutes: u32 = time_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let js_date = js_sys::Date::new_with_year_month_day(year as u32, month, day);
    js_date.set_hours(hours);
    js_date.set_minutes(minutes);
    (js_date.get_time() / 1000.0) as u64
}

#[cfg(not(feature = "web"))]
fn parse_datetime_to_timestamp(date: &str, time: &str) -> u64 {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return 0;
    }
    let year: i64 = parts[0].parse().unwrap_or(2024);
    let month: i64 = parts[1].parse::<i64>().unwrap_or(1);
    let day: i64 = parts[2].parse().unwrap_or(1);
    let time_parts: Vec<&str> = time.split(':').collect();
    let hours: i64 = time_parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minutes: i64 = time_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    // Accurate days since Unix epoch using Hinnant's civil calendar algorithm
    let (y, m) = if month <= 2 { (year - 1, month + 9) } else { (year, month - 3) };
    let era_days = 365 * y + y / 4 - y / 100 + y / 400 + (m * 153 + 2) / 5 + day - 1;
    let days = era_days - 719468; // days_from_civil(1970, 1, 1) = 719468
    let total_secs = days * 86400 + hours * 3600 + minutes * 60;
    if total_secs < 0 { 0u64 } else { total_secs as u64 }
}
/// Convert Unix timestamp to date (YYYY-MM-DD) and time (HH:MM) strings in local time
#[cfg(feature = "web")]
fn timestamp_to_date_time(ts: u64) -> (String, String) {
    let date = js_sys::Date::new(&(ts as f64 * 1000.0).into());
    let year = date.get_full_year();
    let month = date.get_month() + 1;
    let day = date.get_date();
    let hours = date.get_hours();
    let minutes = date.get_minutes();
    let date_str = format!("{:04}-{:02}-{:02}", year, month, day);
    let time_str = format!("{:02}:{:02}", hours, minutes);
    (date_str, time_str)
}

/// Convert Unix timestamp to date (YYYY-MM-DD) and time (HH:MM) strings (UTC fallback)
#[cfg(not(feature = "web"))]
fn timestamp_to_date_time(ts: u64) -> (String, String) {
    let secs = ts;
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    // Approximate date from days since epoch
    let mut y = 1970i32;
    let mut d = days as i32;
    loop {
        let days_in_year = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 };
        if d < days_in_year { break; }
        d -= days_in_year;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days_in_months = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0u32;
    for dim in &days_in_months {
        if d < *dim { break; }
        d -= dim;
        m += 1;
    }
    (format!("{:04}-{:02}-{:02}", y, m + 1, d + 1), format!("{:02}:{:02}", hours, minutes))
}
/// Convert Unix timestamp to date (YYYY-MM-DD) and time (HH:MM) strings in a specific timezone
/// Uses JavaScript's Intl.DateTimeFormat for proper timezone handling
#[cfg(feature = "web")]
fn timestamp_to_date_time_in_tz(ts: u64, tz: &str) -> (String, String) {
    use wasm_bindgen::JsValue;
    if tz.is_empty() || tz.len() > 64
        || !tz
            .chars()
            .all(|c| {
                c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '-' || c == '+'
            })
    {
        log::debug!("Invalid timezone format: {}, falling back to local time", tz);
        return timestamp_to_date_time(ts);
    }
    let date = js_sys::Date::new(&(ts as f64 * 1000.0).into());
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"timeZone".into(), &JsValue::from_str(tz)).ok();
    js_sys::Reflect::set(&options, &"year".into(), &"numeric".into()).ok();
    js_sys::Reflect::set(&options, &"month".into(), &"2-digit".into()).ok();
    js_sys::Reflect::set(&options, &"day".into(), &"2-digit".into()).ok();
    js_sys::Reflect::set(&options, &"hour".into(), &"2-digit".into()).ok();
    js_sys::Reflect::set(&options, &"minute".into(), &"2-digit".into()).ok();
    js_sys::Reflect::set(&options, &"hour12".into(), &JsValue::FALSE).ok();
    let formatter = {
        use wasm_bindgen::JsCast;
        let try_create_formatter = js_sys::Function::new_with_args(
            "options",
            "try { return new Intl.DateTimeFormat([], options); } catch(e) { return null; }",
        );
        match try_create_formatter.call1(&JsValue::NULL, &options) {
            Ok(result) if !result.is_null() => {
                match result.dyn_into::<js_sys::Intl::DateTimeFormat>() {
                    Ok(f) => f,
                    Err(_) => {
                        log::debug!("Failed to cast DateTimeFormat for tz: {}", tz);
                        return timestamp_to_date_time(ts);
                    }
                }
            }
            _ => {
                log::debug!(
                    "Invalid timezone rejected by browser: {}, falling back to local time",
                    tz
                );
                return timestamp_to_date_time(ts);
            }
        }
    };
    let parts = formatter.format_to_parts(&date);
    let mut year = String::new();
    let mut month = String::new();
    let mut day = String::new();
    let mut hour = String::new();
    let mut minute = String::new();
    for i in 0..parts.length() {
        if let Some(part) = parts.get(i).dyn_ref::<js_sys::Object>() {
            let part_type = js_sys::Reflect::get(part, &"type".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            let part_value = js_sys::Reflect::get(part, &"value".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            match part_type.as_str() {
                "year" => year = part_value,
                "month" => month = part_value,
                "day" => day = part_value,
                "hour" => hour = part_value,
                "minute" => minute = part_value,
                _ => {}
            }
        }
    }
    if !year.is_empty() && !month.is_empty() && !day.is_empty() {
        return (format!("{}-{}-{}", year, month, day), format!("{}:{}", hour, minute));
    }
    timestamp_to_date_time(ts)
}

#[cfg(not(feature = "web"))]
fn timestamp_to_date_time_in_tz(ts: u64, tz: &str) -> (String, String) {
    log::debug!("Timezone '{}' ignored on native, using UTC for ts={}", tz, ts);
    timestamp_to_date_time(ts)
}

/// Format ICS datetime for display
#[cfg(feature = "web")]
fn format_ics_datetime(dt: &IcsDateTime) -> String {
    match dt {
        IcsDateTime::Date(d) => d.clone(),
        IcsDateTime::DateTime(ts)
        | IcsDateTime::DateTimeWithTz { timestamp: ts, .. } => {
            let date = js_sys::Date::new(&(*ts as f64 * 1000.0).into());
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                date.get_full_year(),
                date.get_month() + 1,
                date.get_date(),
                date.get_hours(),
                date.get_minutes(),
            )
        }
    }
}

#[cfg(not(feature = "web"))]
fn format_ics_datetime(dt: &IcsDateTime) -> String {
    match dt {
        IcsDateTime::Date(d) => d.clone(),
        IcsDateTime::DateTime(ts)
        | IcsDateTime::DateTimeWithTz { timestamp: ts, .. } => {
            let (date_str, time_str) = timestamp_to_date_time(*ts);
            format!("{} {}", date_str, time_str)
        }
    }
}

/// Read ICS file content from file input
#[cfg(feature = "web")]
async fn read_ics_file_content(element_id: &str) -> Result<String, String> {
    use wasm_bindgen_futures::JsFuture;
    use web_sys::window;
    let window = window().ok_or("No window")?;
    let document = window.document().ok_or("No document")?;
    let input = document
        .get_element_by_id(element_id)
        .ok_or("Input not found")?
        .dyn_into::<HtmlInputElement>()
        .map_err(|_| "Not an input element")?;
    let file_list = input.files().ok_or("No files")?;
    let file = file_list.get(0).ok_or("No file selected")?;
    let promise = file.text();
    let result = JsFuture::from(promise).await.map_err(|_| "Failed to read file")?;
    result.as_string().ok_or("Could not convert to string".to_string())
}
/// Clear file input value to allow re-selecting the same file
#[cfg(feature = "web")]
fn clear_file_input(input_id: &str) {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(element) = document.get_element_by_id(input_id) {
                if let Ok(input) = element.dyn_into::<HtmlInputElement>() {
                    input.set_value("");
                }
            }
        }
    }
}

#[cfg(not(feature = "web"))]
#[allow(dead_code)]
fn clear_file_input(_input_id: &str) {}

#[cfg(not(feature = "web"))]
#[allow(dead_code)]
async fn read_ics_file_content(_element_id: &str) -> Result<String, String> {
    Err("ICS file reading requires web platform".to_string())
}
