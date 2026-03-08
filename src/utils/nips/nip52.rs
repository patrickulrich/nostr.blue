//! NIP-52 Calendar Event Parsing Utilities
//!
//! Handles parsing of calendar events:
//! - Kind 31922: Date-based calendar event (addressable)
//! - Kind 31923: Time-based calendar event (addressable)
//! - Kind 31924: Calendar collection (addressable)
//! - Kind 31925: Calendar Event RSVP (addressable)
//! - Kind 31926: Availability template (addressable)
//! - Kind 31927: Availability block (addressable)
//!
//! NIP-52 defines a standard for calendar events on Nostr.
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
/// Date-based calendar event (all-day events)
pub const KIND_DATE_CALENDAR_EVENT: u16 = 31922;
/// Time-based calendar event (events with specific times)
pub const KIND_TIME_CALENDAR_EVENT: u16 = 31923;
/// Calendar collection (grouping of events)
pub const KIND_CALENDAR: u16 = 31924;
/// Calendar event RSVP
pub const KIND_CALENDAR_RSVP: u16 = 31925;
/// Availability template (recurring availability)
pub const KIND_AVAILABILITY_TEMPLATE: u16 = 31926;
/// Availability block (busy time)
pub const KIND_AVAILABILITY_BLOCK: u16 = 31927;
/// Calendar event type discriminator
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CalendarEventType {
    /// Date-based event (kind 31922) - all-day events
    #[default]
    DateBased,
    /// Time-based event (kind 31923) - events with specific times
    TimeBased,
}
impl CalendarEventType {
    pub fn from_kind(kind: u16) -> Option<Self> {
        match kind {
            KIND_DATE_CALENDAR_EVENT => Some(CalendarEventType::DateBased),
            KIND_TIME_CALENDAR_EVENT => Some(CalendarEventType::TimeBased),
            _ => None,
        }
    }
    pub fn as_kind(&self) -> u16 {
        match self {
            CalendarEventType::DateBased => KIND_DATE_CALENDAR_EVENT,
            CalendarEventType::TimeBased => KIND_TIME_CALENDAR_EVENT,
        }
    }
}
/// RSVP status values
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RsvpStatus {
    #[default]
    Tentative,
    Accepted,
    Declined,
}
impl RsvpStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RsvpStatus::Tentative => "tentative",
            RsvpStatus::Accepted => "accepted",
            RsvpStatus::Declined => "declined",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "tentative" => Some(RsvpStatus::Tentative),
            "accepted" => Some(RsvpStatus::Accepted),
            "declined" => Some(RsvpStatus::Declined),
            _ => None,
        }
    }
}
impl std::fmt::Display for RsvpStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
/// Free/busy indicator for RSVPs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FreeBusy {
    #[default]
    Free,
    Busy,
}
impl FreeBusy {
    pub fn as_str(&self) -> &'static str {
        match self {
            FreeBusy::Free => "free",
            FreeBusy::Busy => "busy",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "free" => Some(FreeBusy::Free),
            "busy" => Some(FreeBusy::Busy),
            _ => None,
        }
    }
    /// Get free/busy based on RSVP status
    pub fn from_rsvp_status(status: RsvpStatus) -> Self {
        match status {
            RsvpStatus::Accepted => FreeBusy::Busy,
            RsvpStatus::Declined | RsvpStatus::Tentative => FreeBusy::Free,
        }
    }
}
impl std::fmt::Display for FreeBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
/// Event source (public vs calendar)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EventSource {
    #[default]
    Public,
    Calendar {
        calendar_id: String,
    },
}
/// Participant in a calendar event
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventParticipant {
    /// Participant pubkey (hex)
    pub pubkey: String,
    /// Relay hint for the participant
    pub relay_hint: Option<String>,
    /// Role: organizer, required, optional, speaker, moderator, participant
    pub role: Option<String>,
}
/// Event time - either ISO date string or Unix timestamp
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventTime {
    /// Date string in YYYY-MM-DD format (for kind 31922)
    Date(String),
    /// Unix timestamp in seconds (for kind 31923)
    Timestamp(u64),
    /// Local datetime with timezone (preserves TZID info).
    ///
    /// The `timestamp` field is a Unix epoch in **UTC** seconds.
    /// The `timezone` field is only used for presentation/display purposes
    /// (e.g., converting to local time for UI display), not for storage.
    /// When serializing to a tag, only the UTC timestamp is used.
    LocalDateTime {
        /// Unix epoch seconds in UTC
        timestamp: u64,
        /// Timezone identifier for display (e.g., "America/New_York")
        timezone: String,
    },
}
impl EventTime {
    /// Parse from string based on event type
    pub fn parse(s: &str, event_type: CalendarEventType) -> Option<Self> {
        match event_type {
            CalendarEventType::DateBased => {
                if s.len() == 10 && s.chars().nth(4) == Some('-') && s.chars().nth(7) == Some('-') {
                    let parts: Vec<&str> = s.split('-').collect();
                    if parts.len() != 3 {
                        return None;
                    }
                    let year = parts[0].parse::<i32>().ok()?;
                    let month = parts[1].parse::<i32>().ok()?;
                    let day = parts[2].parse::<i32>().ok()?;
                    if year < 1970 || !(1..=12).contains(&month) {
                        return None;
                    }
                    let _is_leap = is_leap_year(year);
                    let max_day = crate::utils::date_helpers::days_in_month(year, month);
                    if max_day == 0 || !(1..=max_day).contains(&day) {
                        return None;
                    }
                    Some(EventTime::Date(s.to_string()))
                } else {
                    None
                }
            }
            CalendarEventType::TimeBased => s.parse::<u64>().ok().map(EventTime::Timestamp),
        }
    }
    /// Get as string for tag value
    pub fn as_tag_value(&self) -> String {
        match self {
            EventTime::Date(d) => d.clone(),
            EventTime::Timestamp(t) => t.to_string(),
            EventTime::LocalDateTime { timestamp, .. } => timestamp.to_string(),
        }
    }
    /// Get timestamp (for sorting/comparison)
    pub fn to_timestamp(&self) -> Option<u64> {
        match self {
            EventTime::Timestamp(t) => Some(*t),
            EventTime::LocalDateTime { timestamp, .. } => Some(*timestamp),
            EventTime::Date(d) => {
                let parts: Vec<&str> = d.split('-').collect();
                if parts.len() == 3 {
                    let year: i32 = parts[0].parse().ok()?;
                    let month: u32 = parts[1].parse().ok()?;
                    let day: u32 = parts[2].parse().ok()?;
                    let days = days_since_epoch(year, month, day)?;
                    Some((days as u64) * 86400)
                } else {
                    None
                }
            }
        }
    }
}
/// Calendar Event parsed from Kind 31922 or 31923 events
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalendarEvent {
    /// Unique event identifier (d-tag)
    pub d_tag: String,
    /// Nostr event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// NIP-19 naddr for this event
    pub naddr: String,
    /// Coordinate string: "kind:pubkey:d-tag"
    pub coordinate: String,
    /// Event kind (31922 or 31923)
    pub kind: u16,
    /// Event type
    pub event_type: CalendarEventType,
    /// Created timestamp
    pub created_at: u64,
    /// Event title
    pub title: String,
    /// Start time (date or timestamp)
    pub start: EventTime,
    /// End time (date or timestamp)
    pub end: Option<EventTime>,
    /// Start timezone (IANA format, e.g., "America/New_York")
    pub start_tzid: Option<String>,
    /// End timezone (for events crossing timezones)
    pub end_tzid: Option<String>,
    /// Event summary/description
    pub summary: Option<String>,
    /// Full description (from event content)
    pub content: String,
    /// Event image URL
    pub image: Option<String>,
    /// Event locations (can have multiple)
    pub locations: Vec<String>,
    /// Geohash for physical location
    pub geohash: Option<String>,
    /// Event participants with roles
    pub participants: Vec<EventParticipant>,
    /// Hashtags
    pub hashtags: Vec<String>,
    /// Reference URLs
    pub references: Vec<String>,
    /// Calendar references (a-tags to kind 31924)
    pub calendar_refs: Vec<String>,
    /// Display color (hex)
    pub color: Option<String>,
    /// Event source (public, private, calendar)
    pub source: EventSource,
}
impl CalendarEvent {
    /// Check if event is an all-day event
    pub fn is_all_day(&self) -> bool {
        matches!(self.start, EventTime::Date(_))
    }
    /// Get start timestamp for sorting
    pub fn start_timestamp(&self) -> u64 {
        self.start.to_timestamp().unwrap_or(0)
    }
    /// Get end timestamp
    pub fn end_timestamp(&self) -> Option<u64> {
        self.end.as_ref().and_then(|e| e.to_timestamp())
    }
}
/// Calendar RSVP parsed from Kind 31925 events
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalendarRsvp {
    /// RSVP identifier (d-tag)
    pub d_tag: String,
    /// Nostr event ID (hex)
    pub event_id: String,
    /// Responder pubkey (hex)
    pub pubkey: String,
    /// Coordinate string
    pub coordinate: String,
    /// Created timestamp
    pub created_at: u64,
    /// RSVP status
    pub status: RsvpStatus,
    /// Free/busy indicator
    pub free_busy: FreeBusy,
    /// Referenced event coordinate (a-tag)
    pub event_coordinate: String,
    /// Event author pubkey (p-tag)
    pub event_author: Option<String>,
    /// Optional note (content)
    pub note: String,
}
/// Calendar collection parsed from Kind 31924 events
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Calendar {
    /// Calendar identifier (d-tag)
    pub d_tag: String,
    /// Nostr event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// NIP-19 naddr
    pub naddr: String,
    /// Coordinate string
    pub coordinate: String,
    /// Created timestamp
    pub created_at: u64,
    /// Calendar title
    pub title: String,
    /// Calendar description (content)
    pub content: String,
    /// Event references (a-tags to calendar events)
    pub event_refs: Vec<String>,
}
/// Availability template parsed from Kind 31926 events
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvailabilityTemplate {
    pub d_tag: String,
    pub event_id: String,
    pub pubkey: String,
    pub naddr: String,
    pub coordinate: String,
    pub created_at: u64,
    /// Template title
    pub title: String,
    /// Slot duration in minutes
    pub duration_minutes: u32,
    /// Interval between slot starts in minutes
    pub interval_minutes: Option<u32>,
    /// Buffer before each slot in minutes
    pub buffer_before: Option<u32>,
    /// Buffer after each slot in minutes
    pub buffer_after: Option<u32>,
    /// Timezone
    pub timezone: Option<String>,
    /// Minimum notice period in days
    pub min_notice_days: Option<u32>,
    /// Maximum advance booking in days
    pub max_advance_days: Option<u32>,
    /// Payment amount in satoshis
    pub amount_sats: Option<u64>,
    /// Schedule entries: (day, start_time, end_time)
    pub schedule: Vec<ScheduleEntry>,
}
/// Schedule entry for availability template
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScheduleEntry {
    /// Day of week: MO, TU, WE, TH, FR, SA, SU
    pub day: String,
    /// Start time: HH:MM
    pub start_time: String,
    /// End time: HH:MM
    pub end_time: String,
}
/// Availability block parsed from Kind 31927 events
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvailabilityBlock {
    pub d_tag: String,
    pub event_id: String,
    pub pubkey: String,
    pub coordinate: String,
    pub created_at: u64,
    /// Start timestamp
    pub start: u64,
    /// End timestamp
    pub end: u64,
    /// Optional title
    pub title: Option<String>,
}
/// Parse a Kind 31922 or 31923 event into a CalendarEvent
pub fn parse_calendar_event(event: &Event) -> Result<CalendarEvent, String> {
    let kind = event.kind.as_u16();
    let event_type = CalendarEventType::from_kind(kind)
        .ok_or_else(|| format!("Expected kind 31922 or 31923, got {}", kind))?;
    let pubkey = event.pubkey.to_hex();
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;
    let coordinate = format!("{}:{}:{}", kind, pubkey, d_tag);
    let naddr = build_naddr(kind, &pubkey, &d_tag).unwrap_or_default();
    let title = get_tag_value(event, "title")
        .or_else(|| get_tag_value(event, "name"))
        .ok_or("Missing required 'title' tag")?;
    let start_str = get_tag_value(event, "start").ok_or("Missing required 'start' tag")?;
    let start = EventTime::parse(&start_str, event_type)
        .ok_or_else(|| format!("Invalid start time format: {}", start_str))?;
    let end = get_tag_value(event, "end").and_then(|s| EventTime::parse(&s, event_type));
    let start_tzid =
        get_tag_value(event, "start_tzid").or_else(|| get_tag_value(event, "timezone"));
    let end_tzid = get_tag_value(event, "end_tzid");
    let summary = get_tag_value(event, "summary");
    let content = event.content.clone();
    let image = get_tag_value(event, "image");
    let locations = get_all_tag_values(event, "location");
    let geohash = get_tag_value(event, "g");
    let participants = parse_participants(event);
    let hashtags = get_all_tag_values(event, "t");
    let references = get_all_tag_values(event, "r");
    let calendar_refs = get_coordinate_tags(event)
        .into_iter()
        .filter(|c| c.starts_with("31924:"))
        .collect();
    Ok(CalendarEvent {
        d_tag,
        event_id: event.id.to_hex(),
        pubkey,
        naddr,
        coordinate,
        kind,
        event_type,
        created_at: event.created_at.as_secs(),
        title,
        start,
        end,
        start_tzid,
        end_tzid,
        summary,
        content,
        image,
        locations,
        geohash,
        participants,
        hashtags,
        references,
        calendar_refs,
        color: None,
        source: EventSource::Public,
    })
}
/// Parse a Kind 31925 event into a CalendarRsvp
pub fn parse_calendar_rsvp(event: &Event) -> Result<CalendarRsvp, String> {
    if event.kind.as_u16() != KIND_CALENDAR_RSVP {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_CALENDAR_RSVP,
            event.kind.as_u16()
        ));
    }
    let pubkey = event.pubkey.to_hex();
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;
    let coordinate = format!("{}:{}:{}", KIND_CALENDAR_RSVP, pubkey, d_tag);
    let status = get_tag_value(event, "status")
        .and_then(|s| RsvpStatus::from_str(&s))
        .ok_or("Missing or invalid 'status' tag")?;
    let free_busy = get_tag_value(event, "fb")
        .and_then(|s| FreeBusy::from_str(&s))
        .unwrap_or_else(|| FreeBusy::from_rsvp_status(status));
    let event_coordinate = get_coordinate_tags(event)
        .into_iter()
        .next()
        .ok_or("Missing required 'a' tag (event coordinate)")?;
    let event_author = get_tag_value(event, "p");
    Ok(CalendarRsvp {
        d_tag,
        event_id: event.id.to_hex(),
        pubkey,
        coordinate,
        created_at: event.created_at.as_secs(),
        status,
        free_busy,
        event_coordinate,
        event_author,
        note: event.content.clone(),
    })
}
/// Parse a Kind 31924 event into a Calendar
pub fn parse_calendar(event: &Event) -> Result<Calendar, String> {
    if event.kind.as_u16() != KIND_CALENDAR {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_CALENDAR,
            event.kind.as_u16()
        ));
    }
    let pubkey = event.pubkey.to_hex();
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;
    let coordinate = format!("{}:{}:{}", KIND_CALENDAR, pubkey, d_tag);
    let naddr = build_naddr(KIND_CALENDAR, &pubkey, &d_tag).unwrap_or_default();
    let title = get_tag_value(event, "title")
        .or_else(|| get_tag_value(event, "name"))
        .ok_or("Missing required 'title' tag")?;
    let event_refs = get_coordinate_tags(event)
        .into_iter()
        .filter(|c| c.starts_with("31922:") || c.starts_with("31923:"))
        .collect();
    Ok(Calendar {
        d_tag,
        event_id: event.id.to_hex(),
        pubkey,
        naddr,
        coordinate,
        created_at: event.created_at.as_secs(),
        title,
        content: event.content.clone(),
        event_refs,
    })
}
/// Parse a Kind 31926 event into an AvailabilityTemplate
pub fn parse_availability_template(event: &Event) -> Result<AvailabilityTemplate, String> {
    if event.kind.as_u16() != KIND_AVAILABILITY_TEMPLATE {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_AVAILABILITY_TEMPLATE,
            event.kind.as_u16(),
        ));
    }
    let pubkey = event.pubkey.to_hex();
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;
    let coordinate = format!("{}:{}:{}", KIND_AVAILABILITY_TEMPLATE, pubkey, d_tag);
    let naddr = build_naddr(KIND_AVAILABILITY_TEMPLATE, &pubkey, &d_tag).unwrap_or_default();
    let title = get_tag_value(event, "title").unwrap_or_default();
    let duration_minutes = get_tag_value(event, "duration")
        .and_then(|s| parse_iso_duration_minutes(&s))
        .ok_or("Missing or invalid 'duration' tag")?;
    let interval_minutes =
        get_tag_value(event, "interval").and_then(|s| parse_iso_duration_minutes(&s));
    let buffer_before =
        get_tag_value(event, "buffer_before").and_then(|s| parse_iso_duration_minutes(&s));
    let buffer_after =
        get_tag_value(event, "buffer_after").and_then(|s| parse_iso_duration_minutes(&s));
    let timezone = get_tag_value(event, "tzid");
    let min_notice_days =
        get_tag_value(event, "min_notice").and_then(|s| parse_iso_duration_days(&s));
    let max_advance_days =
        get_tag_value(event, "max_advance").and_then(|s| parse_iso_duration_days(&s));
    let amount_sats = get_tag_value(event, "amount").and_then(|s| s.parse::<u64>().ok());
    let schedule = parse_schedule_entries(event);
    Ok(AvailabilityTemplate {
        d_tag,
        event_id: event.id.to_hex(),
        pubkey,
        naddr,
        coordinate,
        created_at: event.created_at.as_secs(),
        title,
        duration_minutes,
        interval_minutes,
        buffer_before,
        buffer_after,
        timezone,
        min_notice_days,
        max_advance_days,
        amount_sats,
        schedule,
    })
}
/// Parse a Kind 31927 event into an AvailabilityBlock
pub fn parse_availability_block(event: &Event) -> Result<AvailabilityBlock, String> {
    if event.kind.as_u16() != KIND_AVAILABILITY_BLOCK {
        return Err(format!(
            "Expected kind {}, got {}",
            KIND_AVAILABILITY_BLOCK,
            event.kind.as_u16(),
        ));
    }
    let pubkey = event.pubkey.to_hex();
    let d_tag = get_tag_value(event, "d").ok_or("Missing required 'd' tag")?;
    let coordinate = format!("{}:{}:{}", KIND_AVAILABILITY_BLOCK, pubkey, d_tag);
    let start = get_tag_value(event, "start")
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or("Missing or invalid 'start' tag")?;
    let end = get_tag_value(event, "end")
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or("Missing or invalid 'end' tag")?;
    if start >= end {
        return Err("Start time must be before end time".to_string());
    }
    let title = get_tag_value(event, "title");
    Ok(AvailabilityBlock {
        d_tag,
        event_id: event.id.to_hex(),
        pubkey,
        coordinate,
        created_at: event.created_at.as_secs(),
        start,
        end,
        title,
    })
}
/// Get a tag value by name (first parameter after tag name)
fn get_tag_value(event: &Event, tag_name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}
/// Get all values for tags with given name
fn get_all_tag_values(event: &Event, tag_name: &str) -> Vec<String> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some(tag_name))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
        .collect()
}
/// Get all coordinate (a-tag) values
fn get_coordinate_tags(event: &Event) -> Vec<String> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("a"))
        .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
        .collect()
}
/// Parse participants from p-tags: ["p", pubkey, relay_hint?, role?]
fn parse_participants(event: &Event) -> Vec<EventParticipant> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p"))
        .filter_map(|t| {
            let slice = t.as_slice();
            let pubkey = slice.get(1)?.to_string();
            let relay_hint = slice
                .get(2)
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());
            let role = slice
                .get(3)
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());
            Some(EventParticipant {
                pubkey,
                relay_hint,
                role,
            })
        })
        .collect()
}
/// Parse schedule entries from sch tags: ["sch", day, start, end]
fn parse_schedule_entries(event: &Event) -> Vec<ScheduleEntry> {
    event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("sch"))
        .filter_map(|t| {
            let slice = t.as_slice();
            let day = slice.get(1)?.to_string();
            let start_time = slice.get(2)?.to_string();
            let end_time = slice.get(3)?.to_string();
            Some(ScheduleEntry {
                day,
                start_time,
                end_time,
            })
        })
        .collect()
}
/// Parse ISO-8601 duration in minutes (PT{m}M format)
fn parse_iso_duration_minutes(s: &str) -> Option<u32> {
    if s.starts_with("PT") && s.ends_with("M") {
        let num_str = &s[2..s.len() - 1];
        num_str.parse::<u32>().ok()
    } else {
        None
    }
}
/// Parse ISO-8601 duration in days (P{d}D format)
fn parse_iso_duration_days(s: &str) -> Option<u32> {
    if s.starts_with("P") && s.ends_with("D") {
        let num_str = &s[1..s.len() - 1];
        num_str.parse::<u32>().ok()
    } else {
        None
    }
}
/// Build naddr from kind, pubkey, and d-tag
fn build_naddr(kind: u16, pubkey: &str, d_tag: &str) -> Option<String> {
    let pk = PublicKey::from_hex(pubkey).ok()?;
    let coordinate = Coordinate::new(Kind::Custom(kind), pk).identifier(d_tag);
    let nip19 = Nip19Coordinate::new(coordinate, vec![]);
    nip19.to_bech32().ok()
}
/// Calculate days since Unix epoch (simple implementation)
fn days_since_epoch(year: i32, month: u32, day: u32) -> Option<i64> {
    if year < 1970 {
        return None;
    }
    if !(1..=12).contains(&month) {
        return None;
    }
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => return None,
    };
    if day == 0 || day > max_day {
        return None;
    }
    let mut days: i64 = 0;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    for y in year..1970 {
        days -= if is_leap_year(y) { 366 } else { 365 };
    }
    let days_in_months = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        let mut d = days_in_months[(m - 1) as usize];
        if m == 2 && is_leap_year(year) {
            d = 29;
        }
        days += d as i64;
    }
    days += (day - 1) as i64;
    Some(days)
}
fn is_leap_year(year: i32) -> bool {
    crate::utils::date_helpers::is_leap_year(year)
}
/// Online location patterns for detection
const ONLINE_PATTERNS: &[&str] = &[
    "http://",
    "https://",
    "www.",
    "zoom.us",
    "meet.google",
    "teams.microsoft",
    "discord",
    "jitsi",
    "webex",
    "gotomeeting",
    "whereby",
    "gather.town",
    "meet.jit.si",
    "online",
    "virtual",
    "remote",
];
/// Check if a location string represents an online/virtual location
pub fn is_online_location(location: &str) -> bool {
    let lower = location.to_lowercase();
    ONLINE_PATTERNS.iter().any(|p| lower.contains(p))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_rsvp_status_from_str() {
        assert_eq!(RsvpStatus::from_str("accepted"), Some(RsvpStatus::Accepted));
        assert_eq!(RsvpStatus::from_str("DECLINED"), Some(RsvpStatus::Declined));
        assert_eq!(
            RsvpStatus::from_str("tentative"),
            Some(RsvpStatus::Tentative)
        );
        assert_eq!(RsvpStatus::from_str("invalid"), None);
    }
    #[test]
    fn test_free_busy_from_rsvp_status() {
        assert_eq!(
            FreeBusy::from_rsvp_status(RsvpStatus::Accepted),
            FreeBusy::Busy
        );
        assert_eq!(
            FreeBusy::from_rsvp_status(RsvpStatus::Declined),
            FreeBusy::Free
        );
        assert_eq!(
            FreeBusy::from_rsvp_status(RsvpStatus::Tentative),
            FreeBusy::Free
        );
    }
    #[test]
    fn test_event_time_date() {
        let time = EventTime::parse("2024-12-25", CalendarEventType::DateBased);
        assert!(matches!(time, Some(EventTime::Date(_))));
        let time = EventTime::parse("invalid", CalendarEventType::DateBased);
        assert!(time.is_none());
        let impossible = EventTime::parse("2024-02-30", CalendarEventType::DateBased);
        assert!(impossible.is_none());
        let pre_epoch = EventTime::parse("1969-12-31", CalendarEventType::DateBased);
        assert!(pre_epoch.is_none());
    }
    #[test]
    fn test_event_time_timestamp() {
        let time = EventTime::parse("1735123200", CalendarEventType::TimeBased);
        assert!(matches!(time, Some(EventTime::Timestamp(1735123200))));
        let time = EventTime::parse("invalid", CalendarEventType::TimeBased);
        assert!(time.is_none());
    }
    #[test]
    fn test_event_time_local_datetime_tag_value() {
        let time = EventTime::LocalDateTime {
            timestamp: 1735123200,
            timezone: "America/New_York".to_string(),
        };
        assert_eq!(time.as_tag_value(), "1735123200");
        let time = EventTime::LocalDateTime {
            timestamp: 0,
            timezone: "UTC".to_string(),
        };
        assert_eq!(time.as_tag_value(), "0");
        let time = EventTime::LocalDateTime {
            timestamp: 9999999999,
            timezone: "Europe/London".to_string(),
        };
        assert_eq!(time.as_tag_value(), "9999999999");
    }
    #[test]
    fn test_event_time_local_datetime_to_timestamp() {
        let time = EventTime::LocalDateTime {
            timestamp: 1735123200,
            timezone: "America/New_York".to_string(),
        };
        assert_eq!(time.to_timestamp(), Some(1735123200));
        let time = EventTime::LocalDateTime {
            timestamp: 0,
            timezone: "UTC".to_string(),
        };
        assert_eq!(time.to_timestamp(), Some(0));
    }
    #[test]
    fn test_iso_duration_minutes() {
        assert_eq!(parse_iso_duration_minutes("PT30M"), Some(30));
        assert_eq!(parse_iso_duration_minutes("PT60M"), Some(60));
        assert_eq!(parse_iso_duration_minutes("invalid"), None);
    }
    #[test]
    fn test_iso_duration_days() {
        assert_eq!(parse_iso_duration_days("P1D"), Some(1));
        assert_eq!(parse_iso_duration_days("P30D"), Some(30));
        assert_eq!(parse_iso_duration_days("invalid"), None);
    }
    #[test]
    fn test_is_online_location() {
        assert!(is_online_location("https://zoom.us/j/123456"));
        assert!(is_online_location("meet.google.com/abc-def"));
        assert!(is_online_location("Online meeting"));
        assert!(is_online_location("Virtual event"));
        assert!(!is_online_location("123 Main Street, NYC"));
        assert!(!is_online_location("Conference Room A"));
    }
    #[test]
    fn test_calendar_event_type() {
        assert_eq!(
            CalendarEventType::from_kind(31922),
            Some(CalendarEventType::DateBased),
        );
        assert_eq!(
            CalendarEventType::from_kind(31923),
            Some(CalendarEventType::TimeBased),
        );
        assert_eq!(CalendarEventType::from_kind(12345), None);
    }
    #[test]
    fn test_days_since_epoch() {
        assert_eq!(days_since_epoch(1970, 1, 1), Some(0));
        assert_eq!(days_since_epoch(1970, 1, 2), Some(1));
    }
    #[test]
    fn test_parse_invalid_dates() {
        use super::CalendarEventType;
        use super::EventTime;
        assert_eq!(
            EventTime::parse("2024-02-30", CalendarEventType::DateBased),
            None,
            "Invalid day 30 in February should return None"
        );
        assert_eq!(
            EventTime::parse("2024-00-00", CalendarEventType::DateBased),
            None,
            "Invalid month 0 and day 0 should return None"
        );
        assert_eq!(
            EventTime::parse("2024-13-01", CalendarEventType::DateBased),
            None,
            "Invalid month 13 should return None"
        );
        assert_eq!(
            EventTime::parse("0001-01-01", CalendarEventType::DateBased),
            None,
            "Pre-1970 date should return None"
        );
    }
    #[test]
    fn test_to_timestamp_pre_epoch() {
        use super::CalendarEventType;
        use super::EventTime;
        let result = EventTime::parse("1969-01-01", CalendarEventType::DateBased);
        assert_eq!(result, None, "Pre-1970 dates should not parse");
    }
}
