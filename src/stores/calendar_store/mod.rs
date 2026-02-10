//! Calendar Store
//! Handles NIP-52 calendar events and NIP-53 meeting events
//! - Caching, filtering, and state management
//! - Private events via NIP-59 gift wraps
//! - RSVPs and availability
//!
//! ## Submodules
//! - `filters`: Event filter types and filtering logic
//! - `fetch`: Async fetch/subscribe functions for events, RSVPs, availability
//! - `publish`: Event creation, publishing, and RSVP submission
#![allow(dead_code)]
#![allow(unused_imports)]

mod filters;
mod fetch;
mod publish;

pub use filters::*;
pub use fetch::*;
pub use publish::*;

use dioxus::prelude::*;
use lru::LruCache;
use nostr::Event as NostrEvent;
use nostr_sdk::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::num::NonZeroUsize;
use std::time::Duration;
type StdResult<T, E> = std::result::Result<T, E>;
use crate::utils::nip52::{
    parse_calendar_event, parse_calendar_rsvp, AvailabilityBlock, AvailabilityTemplate,
    CalendarEvent, CalendarRsvp, EventSource, EventTime, KIND_AVAILABILITY_BLOCK,
    KIND_AVAILABILITY_TEMPLATE, KIND_CALENDAR_RSVP, KIND_DATE_CALENDAR_EVENT,
    KIND_TIME_CALENDAR_EVENT,
};
use crate::utils::nip53::{
    parse_meeting_room_event, parse_meeting_space, parse_room_presence,
    LiveActivityEvent, RoomPresence, KIND_MEETING_ROOM, KIND_MEETING_SPACE,
    KIND_ROOM_PRESENCE,
};

const EVENT_CACHE_SIZE: usize = 500;
const RSVP_CACHE_SIZE: usize = 200;
const PRIVATE_EVENT_CACHE_SIZE: usize = 100;

/// Calendar events cache (keyed by coordinate string)
pub static CALENDAR_EVENTS_CACHE: GlobalSignal<LruCache<String, CalendarEvent>> = GlobalSignal::new(||
LruCache::new(NonZeroUsize::new(EVENT_CACHE_SIZE).unwrap()));
/// Live activity events cache (meetings, streams) - keyed by coordinate
pub static LIVE_EVENTS_CACHE: GlobalSignal<LruCache<String, LiveActivityEvent>> = GlobalSignal::new(||
LruCache::new(NonZeroUsize::new(EVENT_CACHE_SIZE).unwrap()));
/// Private calendar events (NIP-59 gift wraps)
pub static PRIVATE_EVENTS_CACHE: GlobalSignal<Vec<CalendarEvent>> = GlobalSignal::new(
    Vec::new,
);
/// RSVPs cache (keyed by event coordinate)
pub static RSVPS_CACHE: GlobalSignal<HashMap<String, Vec<CalendarRsvp>>> = GlobalSignal::new(
    HashMap::new,
);
/// User's own RSVPs (keyed by event coordinate -> status)
pub static MY_RSVPS_CACHE: GlobalSignal<HashMap<String, CalendarRsvp>> = GlobalSignal::new(
    HashMap::new,
);
/// Availability templates (keyed by coordinate)
pub static AVAILABILITY_TEMPLATES_CACHE: GlobalSignal<
    LruCache<String, AvailabilityTemplate>,
> = GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(50).unwrap()));
/// Availability blocks (keyed by coordinate)
pub static AVAILABILITY_BLOCKS_CACHE: GlobalSignal<
    LruCache<String, AvailabilityBlock>,
> = GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(100).unwrap()));
/// Events sorted by date for efficient calendar rendering
pub static EVENTS_BY_DATE: GlobalSignal<BTreeMap<String, Vec<String>>> = GlobalSignal::new(
    BTreeMap::new,
);
/// Whether the calendar store has been initialized
pub static CALENDAR_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Currently loading events
pub static LOADING_EVENTS: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// All unique hashtags from events
pub static ALL_EVENT_HASHTAGS: GlobalSignal<HashSet<String>> = GlobalSignal::new(
    HashSet::new,
);
/// Calendars cache
pub static CALENDARS_CACHE: GlobalSignal<
    LruCache<String, crate::utils::nip52::Calendar>,
> = GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(50).unwrap()));

/// Get a calendar event from cache by coordinate
pub fn get_cached_event(coordinate: &str) -> Option<CalendarEvent> {
    CALENDAR_EVENTS_CACHE.read().peek(coordinate).cloned()
}
/// Get a calendar event from cache by naddr
pub fn get_cached_event_by_naddr(naddr: &str) -> Option<CalendarEvent> {
    let cache = CALENDAR_EVENTS_CACHE.read();
    cache.iter().find(|(_, event)| event.naddr == naddr).map(|(_, event)| event.clone())
}
/// Cache a calendar event
pub fn cache_event(event: CalendarEvent) {
    {
        let mut hashtags = ALL_EVENT_HASHTAGS.write();
        for tag in &event.hashtags {
            hashtags.insert(tag.clone());
        }
    }
    {
        let date_key = get_date_key(&event.start);
        let mut by_date = EVENTS_BY_DATE.write();
        let entry = by_date.entry(date_key).or_default();
        if !entry.contains(&event.coordinate) {
            entry.push(event.coordinate.clone());
        }
    }
    CALENDAR_EVENTS_CACHE.write().put(event.coordinate.clone(), event);
}
/// Get date key (YYYY-MM-DD) from EventTime
fn get_date_key(time: &EventTime) -> String {
    match time {
        EventTime::Date(d) => d.clone(),
        EventTime::Timestamp(ts) => {
            let date = js_sys::Date::new(&(*ts as f64 * 1000.0).into());
            format!(
                "{:04}-{:02}-{:02}",
                date.get_full_year(),
                date.get_month() + 1,
                date.get_date(),
            )
        }
    }
}
/// Parse and cache calendar events from nostr events
/// Uses HashSet for O(1) deduplication lookups instead of O(n) Vec::contains
pub fn cache_calendar_events(events: &[NostrEvent]) {
    use std::collections::HashSet;
    let mut cache = CALENDAR_EVENTS_CACHE.write();
    let mut hashtags = ALL_EVENT_HASHTAGS.write();
    let mut by_date = EVENTS_BY_DATE.write();
    let mut existing_by_date: HashMap<String, HashSet<String>> = HashMap::new();
    for event in events {
        if let Ok(cal_event) = parse_calendar_event(event) {
            for tag in &cal_event.hashtags {
                hashtags.insert(tag.clone());
            }
            let date_key = get_date_key(&cal_event.start);
            let existing = existing_by_date
                .entry(date_key.clone())
                .or_insert_with(|| {
                    by_date
                        .get(&date_key)
                        .map(|v| v.iter().cloned().collect())
                        .unwrap_or_default()
                });
            if existing.insert(cal_event.coordinate.clone()) {
                by_date.entry(date_key).or_default().push(cal_event.coordinate.clone());
            }
            cache.put(cal_event.coordinate.clone(), cal_event);
        }
    }
}
/// Cache live activity events
pub fn cache_live_events(events: &[NostrEvent]) {
    let mut cache = LIVE_EVENTS_CACHE.write();
    let mut hashtags = ALL_EVENT_HASHTAGS.write();
    for event in events {
        let kind = event.kind.as_u16();
        let activity = match kind {
            KIND_MEETING_ROOM => {
                parse_meeting_room_event(event).ok().map(LiveActivityEvent::Meeting)
            }
            KIND_MEETING_SPACE => {
                parse_meeting_space(event).ok().map(LiveActivityEvent::Space)
            }
            _ => None,
        };
        if let Some(activity) = activity {
            for tag in activity.hashtags() {
                hashtags.insert(tag.clone());
            }
            cache.put(activity.coordinate().to_string(), activity);
        }
    }
}
/// Get all cached calendar events
pub fn get_all_cached_events() -> Vec<CalendarEvent> {
    let cache = CALENDAR_EVENTS_CACHE.read();
    cache.iter().map(|(_, event)| event.clone()).collect()
}
/// Get all cached live activities
pub fn get_all_live_events() -> Vec<LiveActivityEvent> {
    let cache = LIVE_EVENTS_CACHE.read();
    cache.iter().map(|(_, event)| event.clone()).collect()
}
/// Get all events (calendar + live) combined
pub fn get_all_events_combined() -> Vec<UnifiedEvent> {
    let mut all = Vec::new();
    let calendar_cache = CALENDAR_EVENTS_CACHE.read();
    for (_, event) in calendar_cache.iter() {
        all.push(UnifiedEvent::Calendar(event.clone()));
    }
    let private = PRIVATE_EVENTS_CACHE.read();
    for event in private.iter() {
        all.push(UnifiedEvent::Calendar(event.clone()));
    }
    let live_cache = LIVE_EVENTS_CACHE.read();
    for (_, event) in live_cache.iter() {
        all.push(UnifiedEvent::Live(event.clone()));
    }
    all
}
/// Get events for a specific date
pub fn get_events_for_date(date: &str) -> Vec<CalendarEvent> {
    let by_date = EVENTS_BY_DATE.read();
    let cache = CALENDAR_EVENTS_CACHE.read();
    by_date
        .get(date)
        .map(|coords| {
            coords.iter().filter_map(|coord| cache.peek(coord).cloned()).collect()
        })
        .unwrap_or_default()
}
/// Get events for a date range
pub fn get_events_in_range(start_date: &str, end_date: &str) -> Vec<CalendarEvent> {
    let by_date = EVENTS_BY_DATE.read();
    let cache = CALENDAR_EVENTS_CACHE.read();
    by_date
        .range(start_date.to_string()..=end_date.to_string())
        .flat_map(|(_, coords)| {
            coords.iter().filter_map(|coord| cache.peek(coord).cloned())
        })
        .collect()
}

/// Unified event type for display (calendar events + live activities)
#[derive(Clone, Debug, PartialEq)]
pub enum UnifiedEvent {
    Calendar(CalendarEvent),
    Live(LiveActivityEvent),
}
impl UnifiedEvent {
    pub fn title(&self) -> &str {
        match self {
            UnifiedEvent::Calendar(e) => &e.title,
            UnifiedEvent::Live(e) => e.title(),
        }
    }
    pub fn coordinate(&self) -> &str {
        match self {
            UnifiedEvent::Calendar(e) => &e.coordinate,
            UnifiedEvent::Live(e) => e.coordinate(),
        }
    }
    pub fn naddr(&self) -> &str {
        match self {
            UnifiedEvent::Calendar(e) => &e.naddr,
            UnifiedEvent::Live(e) => e.naddr(),
        }
    }
    pub fn pubkey(&self) -> &str {
        match self {
            UnifiedEvent::Calendar(e) => &e.pubkey,
            UnifiedEvent::Live(e) => e.pubkey(),
        }
    }
    pub fn start_timestamp(&self) -> u64 {
        match self {
            UnifiedEvent::Calendar(e) => e.start_timestamp(),
            UnifiedEvent::Live(e) => e.start_timestamp(),
        }
    }
    pub fn end_timestamp(&self) -> Option<u64> {
        match self {
            UnifiedEvent::Calendar(e) => e.end_timestamp(),
            UnifiedEvent::Live(e) => e.end_timestamp(),
        }
    }
    /// Get the effective end timestamp for filtering purposes
    /// Uses end_timestamp if available, otherwise estimates from start
    pub fn effective_end_timestamp(&self) -> u64 {
        self.end_timestamp()
            .unwrap_or_else(|| {
                match self {
                    UnifiedEvent::Calendar(_) => self.start_timestamp() + 86400,
                    UnifiedEvent::Live(_) => self.start_timestamp() + 7200,
                }
            })
    }
    pub fn image(&self) -> Option<&str> {
        match self {
            UnifiedEvent::Calendar(e) => e.image.as_deref(),
            UnifiedEvent::Live(e) => e.image(),
        }
    }
    pub fn hashtags(&self) -> Vec<&str> {
        match self {
            UnifiedEvent::Calendar(e) => e.hashtags.iter().map(|s| s.as_str()).collect(),
            UnifiedEvent::Live(e) => e.hashtags().iter().map(|s| s.as_str()).collect(),
        }
    }
    pub fn is_private(&self) -> bool {
        match self {
            UnifiedEvent::Calendar(e) => e.is_private(),
            UnifiedEvent::Live(_) => false,
        }
    }
    pub fn is_all_day(&self) -> bool {
        match self {
            UnifiedEvent::Calendar(e) => e.is_all_day(),
            UnifiedEvent::Live(_) => false,
        }
    }
    pub fn locations(&self) -> &[String] {
        match self {
            UnifiedEvent::Calendar(e) => &e.locations,
            UnifiedEvent::Live(_) => &[],
        }
    }
    pub fn is_live(&self) -> bool {
        match self {
            UnifiedEvent::Calendar(_) => false,
            UnifiedEvent::Live(e) => e.is_live(),
        }
    }
    /// Get the live activity status (for NIP-53 events)
    pub fn live_status(&self) -> Option<crate::utils::nip53::LiveStatus> {
        match self {
            UnifiedEvent::Calendar(_) => None,
            UnifiedEvent::Live(e) => Some(e.status()),
        }
    }
    /// Get the first location (if any)
    pub fn location(&self) -> Option<&str> {
        match self {
            UnifiedEvent::Calendar(e) => e.locations.first().map(|s| s.as_str()),
            UnifiedEvent::Live(e) => e.location(),
        }
    }
    /// Get geohash (if any)
    pub fn geohash(&self) -> Option<&str> {
        match self {
            UnifiedEvent::Calendar(e) => e.geohash.as_deref(),
            UnifiedEvent::Live(e) => e.geohash(),
        }
    }
    /// Check if this is a calendar event (NIP-52) vs live activity (NIP-53)
    pub fn is_calendar_event(&self) -> bool {
        matches!(self, UnifiedEvent::Calendar(_))
    }
    /// Check if this is a livestream event (kind 30311)
    /// Note: Livestreams are handled separately at /videos/live, not through UnifiedEvent
    pub fn is_livestream(&self) -> bool {
        false
    }
    /// Get URL to join this event (for meetings)
    pub fn join_url(&self) -> Option<&str> {
        match self {
            UnifiedEvent::Calendar(_) => None,
            UnifiedEvent::Live(e) => e.join_url(),
        }
    }
    /// Get event summary/description
    pub fn summary(&self) -> Option<&str> {
        match self {
            UnifiedEvent::Calendar(e) => e.summary.as_deref(),
            UnifiedEvent::Live(e) => e.summary(),
        }
    }
    /// Get event content (full description)
    pub fn content(&self) -> &str {
        match self {
            UnifiedEvent::Calendar(e) => &e.content,
            UnifiedEvent::Live(LiveActivityEvent::Meeting(e)) => &e.content,
            UnifiedEvent::Live(LiveActivityEvent::Space(_)) => "",
        }
    }
}

/// Cache RSVPs for an event
pub fn cache_rsvps(event_coordinate: &str, rsvps: Vec<CalendarRsvp>) {
    RSVPS_CACHE.write().insert(event_coordinate.to_string(), rsvps);
}
/// Get RSVPs for an event
pub fn get_rsvps(event_coordinate: &str) -> Vec<CalendarRsvp> {
    RSVPS_CACHE.read().get(event_coordinate).cloned().unwrap_or_default()
}
/// Get RSVP count for an event
pub fn get_rsvp_count(event_coordinate: &str) -> usize {
    RSVPS_CACHE.read().get(event_coordinate).map(|r| r.len()).unwrap_or(0)
}
/// Cache user's own RSVP
pub fn cache_my_rsvp(rsvp: CalendarRsvp) {
    MY_RSVPS_CACHE.write().insert(rsvp.event_coordinate.clone(), rsvp);
}
/// Get user's RSVP for an event
pub fn get_my_rsvp(event_coordinate: &str) -> Option<CalendarRsvp> {
    MY_RSVPS_CACHE.read().get(event_coordinate).cloned()
}

/// Get cached availability template
pub fn get_cached_availability_template(
    coordinate: &str,
) -> Option<AvailabilityTemplate> {
    AVAILABILITY_TEMPLATES_CACHE.read().peek(coordinate).cloned()
}
/// Get cached availability block
pub fn get_cached_availability_block(coordinate: &str) -> Option<AvailabilityBlock> {
    AVAILABILITY_BLOCKS_CACHE.read().peek(coordinate).cloned()
}
/// Get all cached availability templates
pub fn get_all_cached_templates() -> Vec<AvailabilityTemplate> {
    AVAILABILITY_TEMPLATES_CACHE.read().iter().map(|(_, t)| t.clone()).collect()
}
/// Get all cached availability blocks
pub fn get_all_cached_blocks() -> Vec<AvailabilityBlock> {
    AVAILABILITY_BLOCKS_CACHE.read().iter().map(|(_, b)| b.clone()).collect()
}
/// Get cached calendar
pub fn get_cached_calendar(coordinate: &str) -> Option<crate::utils::nip52::Calendar> {
    CALENDARS_CACHE.read().peek(coordinate).cloned()
}
/// Get all cached calendars
pub fn get_all_cached_calendars() -> Vec<crate::utils::nip52::Calendar> {
    CALENDARS_CACHE.read().iter().map(|(_, c)| c.clone()).collect()
}

/// Clear all calendar caches
pub fn clear_caches() {
    CALENDAR_EVENTS_CACHE.write().clear();
    LIVE_EVENTS_CACHE.write().clear();
    PRIVATE_EVENTS_CACHE.write().clear();
    RSVPS_CACHE.write().clear();
    MY_RSVPS_CACHE.write().clear();
    AVAILABILITY_TEMPLATES_CACHE.write().clear();
    AVAILABILITY_BLOCKS_CACHE.write().clear();
    CALENDARS_CACHE.write().clear();
    EVENTS_BY_DATE.write().clear();
    ALL_EVENT_HASHTAGS.write().clear();
    *CALENDAR_INITIALIZED.write() = false;
}

/// Calendar statistics
pub struct CalendarStats {
    pub total_events: usize,
    pub calendar_events: usize,
    pub live_activities: usize,
    pub private_events: usize,
    pub upcoming_events: usize,
    pub unique_hashtags: usize,
}
/// Get calendar statistics
pub fn get_calendar_stats() -> CalendarStats {
    let now_secs = (js_sys::Date::now() / 1000.0) as u64;
    let cal_cache = CALENDAR_EVENTS_CACHE.read();
    let live_cache = LIVE_EVENTS_CACHE.read();
    let private = PRIVATE_EVENTS_CACHE.read();
    let hashtags = ALL_EVENT_HASHTAGS.read();
    let upcoming = cal_cache
        .iter()
        .filter(|(_, e)| e.start_timestamp() >= now_secs)
        .count()
        + live_cache.iter().filter(|(_, e)| e.start_timestamp() >= now_secs).count();
    CalendarStats {
        total_events: cal_cache.len() + live_cache.len() + private.len(),
        calendar_events: cal_cache.len(),
        live_activities: live_cache.len(),
        private_events: private.len(),
        upcoming_events: upcoming,
        unique_hashtags: hashtags.len(),
    }
}
