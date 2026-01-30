//! ICS (iCalendar) Import/Export Utilities
//!
//! Provides functionality to import and export calendar events
//! in the standard iCalendar format (RFC 5545).
use crate::utils::nip52::{CalendarEvent, CalendarEventType, EventTime};
/// Generate ICS content for a single calendar event
pub fn export_event_to_ics(event: &CalendarEvent) -> String {
    let mut ics = String::new();
    ics.push_str("BEGIN:VCALENDAR\r\n");
    ics.push_str("VERSION:2.0\r\n");
    ics.push_str("PRODID:-//nostr.blue//Calendar//EN\r\n");
    ics.push_str("CALSCALE:GREGORIAN\r\n");
    ics.push_str("METHOD:PUBLISH\r\n");
    ics.push_str(&format_vevent(event));
    ics.push_str("END:VCALENDAR\r\n");
    ics
}
/// Generate ICS content for multiple calendar events
pub fn export_events_to_ics(events: &[CalendarEvent]) -> String {
    let mut ics = String::new();
    ics.push_str("BEGIN:VCALENDAR\r\n");
    ics.push_str("VERSION:2.0\r\n");
    ics.push_str("PRODID:-//nostr.blue//Calendar//EN\r\n");
    ics.push_str("CALSCALE:GREGORIAN\r\n");
    ics.push_str("METHOD:PUBLISH\r\n");
    for event in events {
        ics.push_str(&format_vevent(event));
    }
    ics.push_str("END:VCALENDAR\r\n");
    ics
}
/// Format a single VEVENT block
fn format_vevent(event: &CalendarEvent) -> String {
    let mut vevent = String::new();
    vevent.push_str("BEGIN:VEVENT\r\n");
    vevent.push_str(&format!("UID:{}\r\n", escape_ics_text(&event.coordinate)));
    vevent.push_str(&format!("DTSTAMP:{}\r\n", format_timestamp(event.created_at)));
    match &event.start {
        EventTime::Date(d) => {
            vevent.push_str(&format!("DTSTART;VALUE=DATE:{}\r\n", d.replace('-', "")));
        }
        EventTime::Timestamp(ts) => {
            if let Some(tz) = &event.start_tzid {
                vevent
                    .push_str(
                        &format!(
                            "DTSTART;TZID={}:{}\r\n",
                            tz,
                            format_local_datetime(*ts),
                        ),
                    );
            } else {
                vevent.push_str(&format!("DTSTART:{}\r\n", format_timestamp(*ts)));
            }
        }
    }
    if let Some(end) = &event.end {
        match end {
            EventTime::Date(d) => {
                vevent.push_str(&format!("DTEND;VALUE=DATE:{}\r\n", d.replace('-', "")));
            }
            EventTime::Timestamp(ts) => {
                if let Some(tz) = &event.end_tzid.as_ref().or(event.start_tzid.as_ref())
                {
                    vevent
                        .push_str(
                            &format!(
                                "DTEND;TZID={}:{}\r\n",
                                tz,
                                format_local_datetime(*ts),
                            ),
                        );
                } else {
                    vevent.push_str(&format!("DTEND:{}\r\n", format_timestamp(*ts)));
                }
            }
        }
    }
    vevent.push_str(&format!("SUMMARY:{}\r\n", escape_ics_text(&event.title)));
    let description = if !event.content.is_empty() {
        &event.content
    } else if let Some(summary) = &event.summary {
        summary
    } else {
        ""
    };
    if !description.is_empty() {
        vevent.push_str(&format!("DESCRIPTION:{}\r\n", escape_ics_text(description)));
    }
    if let Some(location) = event.locations.first() {
        vevent.push_str(&format!("LOCATION:{}\r\n", escape_ics_text(location)));
    }
    if !event.naddr.is_empty() {
        vevent.push_str(&format!("URL:https://nostr.blue/events/{}\r\n", &event.naddr));
    }
    if !event.hashtags.is_empty() {
        vevent.push_str(&format!("CATEGORIES:{}\r\n", event.hashtags.join(",")));
    }
    if let Some(geohash) = &event.geohash {
        if let Some((lat, lon)) = decode_geohash_approx(geohash) {
            vevent.push_str(&format!("GEO:{};{}\r\n", lat, lon));
        }
    }
    vevent
        .push_str(
            &format!("X-NOSTR-COORDINATE:{}\r\n", escape_ics_text(&event.coordinate)),
        );
    vevent.push_str(&format!("X-NOSTR-PUBKEY:{}\r\n", &event.pubkey));
    vevent.push_str("END:VEVENT\r\n");
    vevent
}
/// Format Unix timestamp as ICS UTC datetime (YYYYMMDDTHHmmssZ)
fn format_timestamp(ts: u64) -> String {
    let date = js_sys::Date::new(&(ts as f64 * 1000.0).into());
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        date.get_utc_full_year(),
        date.get_utc_month() + 1,
        date.get_utc_date(),
        date.get_utc_hours(),
        date.get_utc_minutes(),
        date.get_utc_seconds(),
    )
}
/// Format Unix timestamp as local datetime (YYYYMMDDTHHmmss)
fn format_local_datetime(ts: u64) -> String {
    let date = js_sys::Date::new(&(ts as f64 * 1000.0).into());
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}",
        date.get_full_year(),
        date.get_month() + 1,
        date.get_date(),
        date.get_hours(),
        date.get_minutes(),
        date.get_seconds(),
    )
}
/// Escape special characters in ICS text
fn escape_ics_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "")
        .replace(',', "\\,")
        .replace(';', "\\;")
}
/// Decode geohash to approximate lat/lon (simple implementation)
fn decode_geohash_approx(geohash: &str) -> Option<(f64, f64)> {
    if geohash.is_empty() {
        return None;
    }
    const BASE32: &str = "0123456789bcdefghjkmnpqrstuvwxyz";
    let mut lat_min = -90.0;
    let mut lat_max = 90.0;
    let mut lon_min = -180.0;
    let mut lon_max = 180.0;
    let mut is_lon = true;
    for c in geohash.chars() {
        let idx = BASE32.find(c.to_ascii_lowercase())? as u8;
        for bit in (0..5).rev() {
            let val = (idx >> bit) & 1;
            if is_lon {
                let mid = (lon_min + lon_max) / 2.0;
                if val == 1 {
                    lon_min = mid;
                } else {
                    lon_max = mid;
                }
            } else {
                let mid = (lat_min + lat_max) / 2.0;
                if val == 1 {
                    lat_min = mid;
                } else {
                    lat_max = mid;
                }
            }
            is_lon = !is_lon;
        }
    }
    Some(((lat_min + lat_max) / 2.0, (lon_min + lon_max) / 2.0))
}
/// Parsed VEVENT from ICS file
#[derive(Clone, Debug, Default)]
pub struct IcsEvent {
    pub uid: String,
    pub title: String,
    pub description: String,
    pub start: Option<IcsDateTime>,
    pub end: Option<IcsDateTime>,
    pub location: String,
    pub categories: Vec<String>,
    pub url: String,
    pub geo: Option<(f64, f64)>,
}
/// ICS DateTime (either date-only or datetime)
#[derive(Clone, Debug)]
pub enum IcsDateTime {
    Date(String),
    DateTime(u64),
    DateTimeWithTz { timestamp: u64, timezone: String },
}
impl IcsDateTime {
    /// Convert to EventTime for Nostr calendar event
    pub fn to_event_time(&self) -> EventTime {
        match self {
            IcsDateTime::Date(d) => {
                if d.len() == 8 {
                    let formatted = format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8]);
                    EventTime::Date(formatted)
                } else {
                    EventTime::Date(d.clone())
                }
            }
            IcsDateTime::DateTime(ts) => EventTime::Timestamp(*ts),
            IcsDateTime::DateTimeWithTz { timestamp, .. } => {
                EventTime::Timestamp(*timestamp)
            }
        }
    }
    /// Get timezone if present
    pub fn timezone(&self) -> Option<&str> {
        match self {
            IcsDateTime::DateTimeWithTz { timezone, .. } => Some(timezone),
            _ => None,
        }
    }
}
/// Parse ICS content and extract events
pub fn parse_ics(content: &str) -> Vec<IcsEvent> {
    let mut events = Vec::new();
    let mut current_event: Option<IcsEvent> = None;
    let mut in_vevent = false;
    let unfolded = unfold_ics_lines(content);
    for line in unfolded.lines() {
        let line = line.trim_end();
        if line == "BEGIN:VEVENT" {
            in_vevent = true;
            current_event = Some(IcsEvent::default());
        } else if line == "END:VEVENT" {
            if let Some(event) = current_event.take() {
                if !event.title.is_empty() || event.start.is_some() {
                    events.push(event);
                }
            }
            in_vevent = false;
        } else if in_vevent {
            if let Some(ref mut event) = current_event {
                parse_ics_property(line, event);
            }
        }
    }
    events
}
/// Unfold ICS lines (handle line continuations)
fn unfold_ics_lines(content: &str) -> String {
    let mut result = String::new();
    let mut prev_line = String::new();
    for line in content.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            prev_line.push_str(line.trim_start());
        } else {
            if !prev_line.is_empty() {
                result.push_str(&prev_line);
                result.push('\n');
            }
            prev_line = line.to_string();
        }
    }
    if !prev_line.is_empty() {
        result.push_str(&prev_line);
    }
    result
}
/// Parse a single ICS property line
fn parse_ics_property(line: &str, event: &mut IcsEvent) {
    let (name_params, value) = match line.find(':') {
        Some(idx) => (&line[..idx], &line[idx + 1..]),
        None => return,
    };
    let (name, params) = match name_params.find(';') {
        Some(idx) => (&name_params[..idx], &name_params[idx + 1..]),
        None => (name_params, ""),
    };
    match name.to_uppercase().as_str() {
        "UID" => event.uid = unescape_ics_text(value),
        "SUMMARY" => event.title = unescape_ics_text(value),
        "DESCRIPTION" => event.description = unescape_ics_text(value),
        "LOCATION" => event.location = unescape_ics_text(value),
        "URL" => event.url = value.to_string(),
        "CATEGORIES" => {
            event.categories = value
                .split(',')
                .map(|s| unescape_ics_text(s.trim()))
                .collect();
        }
        "GEO" => {
            let parts: Vec<&str> = value.split(';').collect();
            if parts.len() == 2 {
                if let (Ok(lat), Ok(lon)) = (
                    parts[0].parse::<f64>(),
                    parts[1].parse::<f64>(),
                ) {
                    event.geo = Some((lat, lon));
                }
            }
        }
        "DTSTART" => {
            event.start = parse_ics_datetime(value, params);
        }
        "DTEND" => {
            event.end = parse_ics_datetime(value, params);
        }
        _ => {}
    }
}
/// Parse ICS datetime value
fn parse_ics_datetime(value: &str, params: &str) -> Option<IcsDateTime> {
    let is_date_only = params.to_uppercase().contains("VALUE=DATE");
    let tzid = params
        .split(';')
        .find(|p| p.to_uppercase().starts_with("TZID="))
        .map(|p| p[5..].to_string());
    if is_date_only {
        if value.len() >= 8 {
            return Some(IcsDateTime::Date(value[..8].to_string()));
        }
    } else if value.ends_with('Z') {
        if let Some(ts) = parse_ics_utc_datetime(value) {
            return Some(IcsDateTime::DateTime(ts));
        }
    } else if let Some(tz) = tzid {
        if let Some(ts) = parse_ics_local_datetime(value) {
            return Some(IcsDateTime::DateTimeWithTz {
                timestamp: ts,
                timezone: tz,
            });
        }
    } else {
        if let Some(ts) = parse_ics_local_datetime(value) {
            return Some(IcsDateTime::DateTime(ts));
        }
    }
    None
}
/// Parse UTC datetime: YYYYMMDDTHHmmssZ
fn parse_ics_utc_datetime(value: &str) -> Option<u64> {
    if value.len() < 15 {
        return None;
    }
    let year: i32 = value[0..4].parse().ok()?;
    let month: u32 = value[4..6].parse().ok()?;
    let day: u32 = value[6..8].parse().ok()?;
    let hour: u32 = value[9..11].parse().ok()?;
    let minute: u32 = value[11..13].parse().ok()?;
    let second: u32 = value[13..15].parse().ok()?;
    let date = js_sys::Date::new_with_year_month_day_hr_min_sec(
        year as u32,
        (month - 1) as i32,
        day as i32,
        hour as i32,
        minute as i32,
        second as i32,
    );
    Some((date.get_time() / 1000.0) as u64)
}
/// Parse local datetime: YYYYMMDDTHHmmss
fn parse_ics_local_datetime(value: &str) -> Option<u64> {
    if value.len() < 15 {
        return None;
    }
    let year: i32 = value[0..4].parse().ok()?;
    let month: u32 = value[4..6].parse().ok()?;
    let day: u32 = value[6..8].parse().ok()?;
    let hour: u32 = value[9..11].parse().ok()?;
    let minute: u32 = value[11..13].parse().ok()?;
    let second: u32 = value[13..15].parse().ok()?;
    let date = js_sys::Date::new_with_year_month_day_hr_min_sec(
        year as u32,
        (month - 1) as i32,
        day as i32,
        hour as i32,
        minute as i32,
        second as i32,
    );
    Some((date.get_time() / 1000.0) as u64)
}
/// Unescape ICS text
fn unescape_ics_text(text: &str) -> String {
    text.replace("\\n", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}
/// Convert parsed ICS event to data for creating a Nostr calendar event
pub struct IcsImportData {
    pub title: String,
    pub summary: Option<String>,
    pub content: String,
    pub event_type: CalendarEventType,
    pub start: EventTime,
    pub end: Option<EventTime>,
    pub start_tzid: Option<String>,
    pub end_tzid: Option<String>,
    pub locations: Vec<String>,
    pub hashtags: Vec<String>,
}
impl From<IcsEvent> for Option<IcsImportData> {
    fn from(event: IcsEvent) -> Self {
        let start = event.start.as_ref()?.to_event_time();
        let event_type = match &start {
            EventTime::Date(_) => CalendarEventType::DateBased,
            EventTime::Timestamp(_) => CalendarEventType::TimeBased,
        };
        let end = event.end.as_ref().map(|e| e.to_event_time());
        let start_tzid = event
            .start
            .as_ref()
            .and_then(|dt| dt.timezone().map(String::from));
        let end_tzid = event.end.as_ref().and_then(|dt| dt.timezone().map(String::from));
        let locations = if event.location.is_empty() {
            vec![]
        } else {
            vec![event.location]
        };
        Some(IcsImportData {
            title: event.title,
            summary: None,
            content: event.description,
            event_type,
            start,
            end,
            start_tzid,
            end_tzid,
            locations,
            hashtags: event.categories,
        })
    }
}
/// Trigger browser download of ICS file
pub fn download_ics(filename: &str, content: &str) {
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = URL, js_name = createObjectURL)]
        fn create_object_url(blob: &web_sys::Blob) -> String;
        #[wasm_bindgen(js_namespace = URL, js_name = revokeObjectURL)]
        fn revoke_object_url(url: &str);
    }
    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");
    let parts = js_sys::Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(content));
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"type".into(), &"text/calendar;charset=utf-8".into())
        .ok();
    let blob = web_sys::Blob::new_with_str_sequence_and_options(
            &parts,
            &options.unchecked_into(),
        )
        .expect("failed to create blob");
    let url = create_object_url(&blob);
    let a = document.create_element("a").expect("failed to create anchor");
    a.set_attribute("href", &url).ok();
    a.set_attribute("download", filename).ok();
    let click_fn = js_sys::Reflect::get(&a, &"click".into()).expect("no click method");
    let click_fn: js_sys::Function = click_fn.unchecked_into();
    click_fn.call0(&a).ok();
    revoke_object_url(&url);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_escape_ics_text() {
        assert_eq!(escape_ics_text("Hello, World"), "Hello\\, World");
        assert_eq!(escape_ics_text("Line1\nLine2"), "Line1\\nLine2");
        assert_eq!(escape_ics_text("Test;value"), "Test\\;value");
    }
    #[test]
    fn test_unescape_ics_text() {
        assert_eq!(unescape_ics_text("Hello\\, World"), "Hello, World");
        assert_eq!(unescape_ics_text("Line1\\nLine2"), "Line1\nLine2");
        assert_eq!(unescape_ics_text("Test\\;value"), "Test;value");
    }
    #[test]
    fn test_decode_geohash() {
        let result = decode_geohash_approx("u4pruydqqvj");
        assert!(result.is_some());
        let (lat, lon) = result.unwrap();
        assert!((lat - 57.65).abs() < 0.1);
        assert!((lon - 10.41).abs() < 0.1);
    }
    #[test]
    fn test_parse_ics_basic() {
        let ics = r#"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:test-event-1
SUMMARY:Test Event
DTSTART;VALUE=DATE:20241225
DTEND;VALUE=DATE:20241226
LOCATION:Test Location
CATEGORIES:test,event
END:VEVENT
END:VCALENDAR"#;
        let events = parse_ics(ics);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Test Event");
        assert_eq!(events[0].location, "Test Location");
        assert_eq!(events[0].categories, vec!["test", "event"]);
    }
}
