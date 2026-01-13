//! Calendar Store
//! Handles NIP-52 calendar events and NIP-53 meeting events
//! - Caching, filtering, and state management
//! - Private events via NIP-59 gift wraps
//! - RSVPs and availability

#![allow(dead_code)]

use dioxus::prelude::*;
use lru::LruCache;
use nostr_sdk::prelude::*;
use nostr::Event as NostrEvent;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::num::NonZeroUsize;
use std::time::Duration;

// Use explicit Result to avoid ambiguity
type StdResult<T, E> = std::result::Result<T, E>;

use crate::utils::nip52::{
    parse_calendar_event, parse_calendar_rsvp,
    CalendarEvent, CalendarRsvp, AvailabilityTemplate, AvailabilityBlock, EventSource, EventTime,
    KIND_DATE_CALENDAR_EVENT, KIND_TIME_CALENDAR_EVENT, KIND_CALENDAR_RSVP,
    KIND_AVAILABILITY_TEMPLATE, KIND_AVAILABILITY_BLOCK,
};
use crate::utils::nip53::{
    parse_meeting_room_event, parse_meeting_space, parse_room_presence,
    LiveActivityEvent, RoomPresence,
    KIND_MEETING_ROOM, KIND_MEETING_SPACE, KIND_ROOM_PRESENCE,
};

// ============================================================================
// Cache sizes
// ============================================================================

const EVENT_CACHE_SIZE: usize = 500;
const RSVP_CACHE_SIZE: usize = 200;
const PRIVATE_EVENT_CACHE_SIZE: usize = 100;

// ============================================================================
// Global Signals
// ============================================================================

/// Calendar events cache (keyed by coordinate string)
pub static CALENDAR_EVENTS_CACHE: GlobalSignal<LruCache<String, CalendarEvent>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(EVENT_CACHE_SIZE).unwrap()));

/// Live activity events cache (meetings, streams) - keyed by coordinate
pub static LIVE_EVENTS_CACHE: GlobalSignal<LruCache<String, LiveActivityEvent>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(EVENT_CACHE_SIZE).unwrap()));

/// Private calendar events (NIP-59 gift wraps)
pub static PRIVATE_EVENTS_CACHE: GlobalSignal<Vec<CalendarEvent>> =
    GlobalSignal::new(Vec::new);

/// RSVPs cache (keyed by event coordinate)
pub static RSVPS_CACHE: GlobalSignal<HashMap<String, Vec<CalendarRsvp>>> =
    GlobalSignal::new(HashMap::new);

/// User's own RSVPs (keyed by event coordinate -> status)
pub static MY_RSVPS_CACHE: GlobalSignal<HashMap<String, CalendarRsvp>> =
    GlobalSignal::new(HashMap::new);

/// Availability templates (keyed by coordinate)
pub static AVAILABILITY_TEMPLATES_CACHE: GlobalSignal<LruCache<String, AvailabilityTemplate>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(50).unwrap()));

/// Availability blocks (keyed by coordinate)
pub static AVAILABILITY_BLOCKS_CACHE: GlobalSignal<LruCache<String, AvailabilityBlock>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(100).unwrap()));

/// Events sorted by date for efficient calendar rendering
pub static EVENTS_BY_DATE: GlobalSignal<BTreeMap<String, Vec<String>>> =
    GlobalSignal::new(BTreeMap::new);

/// Whether the calendar store has been initialized
pub static CALENDAR_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// Currently loading events
pub static LOADING_EVENTS: GlobalSignal<bool> = GlobalSignal::new(|| false);

/// All unique hashtags from events
pub static ALL_EVENT_HASHTAGS: GlobalSignal<HashSet<String>> = GlobalSignal::new(HashSet::new);

// ============================================================================
// Event Functions
// ============================================================================

/// Get a calendar event from cache by coordinate
pub fn get_cached_event(coordinate: &str) -> Option<CalendarEvent> {
    CALENDAR_EVENTS_CACHE.read().peek(coordinate).cloned()
}

/// Get a calendar event from cache by naddr
pub fn get_cached_event_by_naddr(naddr: &str) -> Option<CalendarEvent> {
    let cache = CALENDAR_EVENTS_CACHE.read();
    cache
        .iter()
        .find(|(_, event)| event.naddr == naddr)
        .map(|(_, event)| event.clone())
}

/// Cache a calendar event
pub fn cache_event(event: CalendarEvent) {
    // Update hashtags
    {
        let mut hashtags = ALL_EVENT_HASHTAGS.write();
        for tag in &event.hashtags {
            hashtags.insert(tag.clone());
        }
    }

    // Update events by date index - deduplicate (nostr-sdk pattern)
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
            // Convert timestamp to date string
            let date = js_sys::Date::new(&(*ts as f64 * 1000.0).into());
            format!(
                "{:04}-{:02}-{:02}",
                date.get_full_year(),
                date.get_month() + 1,
                date.get_date()
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

    // Build lookup map for O(1) deduplication (nostr-sdk pattern)
    let mut existing_by_date: HashMap<String, HashSet<String>> = HashMap::new();

    for event in events {
        if let Ok(cal_event) = parse_calendar_event(event) {
            // Update hashtags
            for tag in &cal_event.hashtags {
                hashtags.insert(tag.clone());
            }

            // Update by date index - O(1) deduplication via HashSet
            let date_key = get_date_key(&cal_event.start);
            let existing = existing_by_date
                .entry(date_key.clone())
                .or_insert_with(|| {
                    by_date.get(&date_key)
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
                parse_meeting_room_event(event)
                    .ok()
                    .map(LiveActivityEvent::Meeting)
            }
            KIND_MEETING_SPACE => {
                parse_meeting_space(event)
                    .ok()
                    .map(LiveActivityEvent::Space)
            }
            _ => None,
        };

        if let Some(activity) = activity {
            // Update hashtags
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

    // Add calendar events
    let calendar_cache = CALENDAR_EVENTS_CACHE.read();
    for (_, event) in calendar_cache.iter() {
        all.push(UnifiedEvent::Calendar(event.clone()));
    }

    // Add private events
    let private = PRIVATE_EVENTS_CACHE.read();
    for event in private.iter() {
        all.push(UnifiedEvent::Calendar(event.clone()));
    }

    // Add live activities
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
            coords
                .iter()
                .filter_map(|coord| cache.peek(coord).cloned())
                .collect()
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
            coords
                .iter()
                .filter_map(|coord| cache.peek(coord).cloned())
        })
        .collect()
}

// ============================================================================
// Unified Event Type
// ============================================================================

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
        self.end_timestamp().unwrap_or_else(|| {
            // If no end time, assume event lasts 2 hours for meetings/streams, 1 day for calendar events
            match self {
                UnifiedEvent::Calendar(_) => self.start_timestamp() + 86400, // 1 day
                UnifiedEvent::Live(_) => self.start_timestamp() + 7200,      // 2 hours
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
            UnifiedEvent::Calendar(_) => None, // Calendar events use locations
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

// ============================================================================
// RSVP Functions
// ============================================================================

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

// ============================================================================
// Filter State
// ============================================================================

/// Time filter for events
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeFilter {
    #[default]
    All,
    Today,
    ThisWeek,
    ThisMonth,
    Upcoming,
    Past,
}

impl TimeFilter {
    pub fn label(&self) -> &'static str {
        match self {
            TimeFilter::All => "All",
            TimeFilter::Today => "Today",
            TimeFilter::ThisWeek => "This Week",
            TimeFilter::ThisMonth => "This Month",
            TimeFilter::Upcoming => "Upcoming",
            TimeFilter::Past => "Past",
        }
    }
}

/// Location filter for events
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LocationFilter {
    #[default]
    All,
    InPerson,
    Online,
}

impl LocationFilter {
    pub fn label(&self) -> &'static str {
        match self {
            LocationFilter::All => "All",
            LocationFilter::InPerson => "In-Person",
            LocationFilter::Online => "Online",
        }
    }
}

/// Event type filter (Calendar vs Meetings)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventTypeFilter {
    #[default]
    All,
    Calendar,
    Meetings,
}

impl EventTypeFilter {
    pub fn label(&self) -> &'static str {
        match self {
            EventTypeFilter::All => "All Events",
            EventTypeFilter::Calendar => "Calendar Events",
            EventTypeFilter::Meetings => "Meetings",
        }
    }
}

/// Filter state for events
#[derive(Clone, Debug)]
pub struct EventFilterState {
    pub search_term: String,
    pub time_filter: TimeFilter,
    pub location_filter: LocationFilter,
    pub event_type_filter: EventTypeFilter,
    pub hashtag: Option<String>,
    pub include_private: bool,
    /// Hide events that have ended (default: true)
    pub hide_ended: bool,
}

impl Default for EventFilterState {
    fn default() -> Self {
        Self {
            search_term: String::new(),
            time_filter: TimeFilter::default(),
            location_filter: LocationFilter::default(),
            event_type_filter: EventTypeFilter::default(),
            hashtag: None,
            include_private: false,
            hide_ended: true, // Hide ended events by default
        }
    }
}

impl EventFilterState {
    pub fn is_empty(&self) -> bool {
        self.search_term.is_empty()
            && self.time_filter == TimeFilter::All
            && self.location_filter == LocationFilter::All
            && self.event_type_filter == EventTypeFilter::All
            && self.hashtag.is_none()
            && !self.include_private // false means using default (not including private)
            && self.hide_ended // true means using default (hiding ended)
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Filter unified events
/// Set `from_nip50` to true when results came from NIP-50 relay search to skip client-side search filter
pub fn filter_events(events: &[UnifiedEvent], filters: &EventFilterState) -> Vec<UnifiedEvent> {
    filter_events_with_nip50(events, filters, false)
}

/// Filter unified events with option to skip search term filter for NIP-50 results
pub fn filter_events_with_nip50(events: &[UnifiedEvent], filters: &EventFilterState, from_nip50: bool) -> Vec<UnifiedEvent> {
    let now_secs = (js_sys::Date::now() / 1000.0) as u64;
    let today_start = now_secs - (now_secs % 86400);
    let week_end = today_start + (7 * 86400);
    let month_end = today_start + (30 * 86400);

    events
        .iter()
        .filter(|event| {
            // Hide ended events filter (skip when explicitly viewing past events)
            if filters.hide_ended && filters.time_filter != TimeFilter::Past {
                let end_ts = event.effective_end_timestamp();
                if end_ts < now_secs {
                    return false;
                }
            }

            // Search term filter (client-side fallback only - skip for NIP-50 results)
            if !from_nip50 && !filters.search_term.is_empty() {
                let term = filters.search_term.to_lowercase();
                let matches = event.title().to_lowercase().contains(&term);
                if !matches {
                    return false;
                }
            }

            // Event type filter
            match filters.event_type_filter {
                EventTypeFilter::All => {}
                EventTypeFilter::Calendar => {
                    if !event.is_calendar_event() {
                        return false;
                    }
                }
                EventTypeFilter::Meetings => {
                    if event.is_calendar_event() {
                        return false;
                    }
                }
            }

            // Time filter
            let start_ts = event.start_timestamp();
            let end_ts = event.effective_end_timestamp();
            match filters.time_filter {
                TimeFilter::All => {
                    // "All" still excludes events that have completely ended (more than 24h ago)
                    // to avoid showing very old events
                    if end_ts < now_secs.saturating_sub(86400) {
                        return false;
                    }
                }
                TimeFilter::Today => {
                    // Event overlaps with today if it starts before tomorrow and ends after today started
                    if start_ts >= today_start + 86400 || end_ts < today_start {
                        return false;
                    }
                }
                TimeFilter::ThisWeek => {
                    // Event overlaps with this week
                    if start_ts >= week_end || end_ts < today_start {
                        return false;
                    }
                }
                TimeFilter::ThisMonth => {
                    // Event overlaps with this month
                    if start_ts >= month_end || end_ts < today_start {
                        return false;
                    }
                }
                TimeFilter::Upcoming => {
                    // Show events that haven't ended yet (includes currently happening)
                    if end_ts < now_secs {
                        return false;
                    }
                }
                TimeFilter::Past => {
                    // Show events that have completely ended
                    if end_ts >= now_secs {
                        return false;
                    }
                }
            }

            // Location filter
            match filters.location_filter {
                LocationFilter::All => {}
                LocationFilter::InPerson => {
                    let locs = event.locations();
                    if locs.is_empty() || locs.iter().all(|l| crate::utils::nip52::is_online_location(l)) {
                        return false;
                    }
                }
                LocationFilter::Online => {
                    let locs = event.locations();
                    if locs.is_empty() || locs.iter().all(|l| !crate::utils::nip52::is_online_location(l)) {
                        return false;
                    }
                }
            }

            // Hashtag filter
            if let Some(tag) = &filters.hashtag {
                if !event.hashtags().contains(&tag.as_str()) {
                    return false;
                }
            }

            // Private events filter
            if !filters.include_private && event.is_private() {
                return false;
            }

            true
        })
        .cloned()
        .collect()
}

/// Sort events (events with images first, then by date)
pub fn sort_events_for_display(events: &mut [UnifiedEvent]) {
    events.sort_by(|a, b| {
        // Events with images come first
        let a_has_image = a.image().is_some();
        let b_has_image = b.image().is_some();

        match (a_has_image, b_has_image) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                // Then sort by start time (ascending)
                a.start_timestamp().cmp(&b.start_timestamp())
            }
        }
    });
}

// ============================================================================
// Filter Builders (Nostr queries)
// ============================================================================

/// Build filter for calendar events (date and time-based)
pub fn calendar_events_filter(limit: usize) -> Filter {
    Filter::new()
        .kinds([Kind::Custom(KIND_DATE_CALENDAR_EVENT), Kind::Custom(KIND_TIME_CALENDAR_EVENT)])
        .limit(limit)
}

/// Build filter for calendar events since a timestamp
pub fn calendar_events_filter_since(since: u64, limit: usize) -> Filter {
    Filter::new()
        .kinds([Kind::Custom(KIND_DATE_CALENDAR_EVENT), Kind::Custom(KIND_TIME_CALENDAR_EVENT)])
        .since(Timestamp::from(since))
        .limit(limit)
}

/// Build filter for meetings (spaces and rooms)
pub fn meetings_filter(limit: usize) -> Filter {
    Filter::new()
        .kinds([Kind::Custom(KIND_MEETING_SPACE), Kind::Custom(KIND_MEETING_ROOM)])
        .limit(limit)
}

/// Build filter for calendar events until a timestamp (for pagination)
pub fn calendar_events_filter_until(until: u64, limit: usize) -> Filter {
    Filter::new()
        .kinds([Kind::Custom(KIND_DATE_CALENDAR_EVENT), Kind::Custom(KIND_TIME_CALENDAR_EVENT)])
        .until(Timestamp::from(until))
        .limit(limit)
}

/// Build filter for meetings until a timestamp (for pagination)
pub fn meetings_filter_until(until: u64, limit: usize) -> Filter {
    Filter::new()
        .kinds([Kind::Custom(KIND_MEETING_SPACE), Kind::Custom(KIND_MEETING_ROOM)])
        .until(Timestamp::from(until))
        .limit(limit)
}

/// Build filter for RSVPs to a specific event
pub fn rsvps_filter(event_coordinate: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_CALENDAR_RSVP))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), event_coordinate)
}

/// Build filter for user's RSVPs
pub fn my_rsvps_filter(pubkey: PublicKey) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_CALENDAR_RSVP))
        .author(pubkey)
}

/// Build filter for private calendar events (gift wraps to user)
pub fn private_events_filter(pubkey: PublicKey) -> Filter {
    Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(pubkey)
}

/// Build filter for availability templates by author
pub fn availability_templates_filter(pubkey: PublicKey) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_AVAILABILITY_TEMPLATE))
        .author(pubkey)
}

/// Build filter for availability blocks by author
pub fn availability_blocks_filter(pubkey: PublicKey) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_AVAILABILITY_BLOCK))
        .author(pubkey)
}

/// Build filter for specific event by coordinate
pub fn event_by_coordinate_filter(pubkey: PublicKey, kind: u16, identifier: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(kind))
        .author(pubkey)
        .identifier(identifier)
}

// ============================================================================
// Fetch Functions
// ============================================================================

/// Fetch calendar events
pub async fn fetch_calendar_events(limit: usize) -> StdResult<Vec<CalendarEvent>, String> {
    *LOADING_EVENTS.write() = true;

    let filter = calendar_events_filter(limit);
    let result = crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await;

    *LOADING_EVENTS.write() = false;

    match result {
        Ok(events) => {
            cache_calendar_events(&events);
            let cal_events: Vec<CalendarEvent> = events
                .iter()
                .filter_map(|e| parse_calendar_event(e).ok())
                .collect();
            Ok(cal_events)
        }
        Err(e) => Err(e),
    }
}

/// Fetch meetings (spaces and rooms)
pub async fn fetch_meetings(limit: usize) -> StdResult<Vec<LiveActivityEvent>, String> {
    let filter = meetings_filter(limit);
    let events = crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await?;

    cache_live_events(&events);

    let activities: Vec<LiveActivityEvent> = events
        .iter()
        .filter_map(|e| {
            let kind = e.kind.as_u16();
            match kind {
                KIND_MEETING_ROOM => parse_meeting_room_event(e).ok().map(LiveActivityEvent::Meeting),
                KIND_MEETING_SPACE => parse_meeting_space(e).ok().map(LiveActivityEvent::Space),
                _ => None,
            }
        })
        .collect();

    Ok(activities)
}

/// Fetch all events (calendar + meetings) for the events page
pub async fn fetch_all_events(limit: usize) -> StdResult<Vec<UnifiedEvent>, String> {
    *LOADING_EVENTS.write() = true;

    // Fetch calendar events and meetings in parallel
    let cal_filter = calendar_events_filter(limit);
    let meetings_filter = meetings_filter(limit);

    let (cal_result, meetings_result) = futures::join!(
        crate::stores::nostr_client::fetch_events_aggregated(cal_filter, Duration::from_secs(15)),
        crate::stores::nostr_client::fetch_events_aggregated(meetings_filter, Duration::from_secs(15))
    );

    *LOADING_EVENTS.write() = false;

    let mut all_events = Vec::new();

    // Process calendar events
    if let Ok(events) = cal_result {
        cache_calendar_events(&events);
        for event in events {
            if let Ok(cal_event) = parse_calendar_event(&event) {
                all_events.push(UnifiedEvent::Calendar(cal_event));
            }
        }
    }

    // Process meetings
    if let Ok(events) = meetings_result {
        cache_live_events(&events);
        for event in events {
            let kind = event.kind.as_u16();
            let activity = match kind {
                KIND_MEETING_ROOM => parse_meeting_room_event(&event).ok().map(LiveActivityEvent::Meeting),
                KIND_MEETING_SPACE => parse_meeting_space(&event).ok().map(LiveActivityEvent::Space),
                _ => None,
            };
            if let Some(activity) = activity {
                all_events.push(UnifiedEvent::Live(activity));
            }
        }
    }

    Ok(all_events)
}

/// Fetch all events (calendar + meetings) with pagination support
/// Returns (events, oldest_timestamp) where oldest_timestamp can be used for next page
pub async fn fetch_all_events_paginated(limit: usize, until: Option<u64>) -> StdResult<(Vec<UnifiedEvent>, Option<u64>), String> {
    // Use pagination filters if until is provided
    let (cal_filter, mtg_filter) = match until {
        Some(ts) => (
            calendar_events_filter_until(ts, limit),
            meetings_filter_until(ts, limit)
        ),
        None => (
            calendar_events_filter(limit),
            meetings_filter(limit)
        ),
    };

    let (cal_result, mtg_result) = futures::join!(
        crate::stores::nostr_client::fetch_events_aggregated(cal_filter, Duration::from_secs(15)),
        crate::stores::nostr_client::fetch_events_aggregated(mtg_filter, Duration::from_secs(15))
    );

    let mut all_events = Vec::new();
    let mut oldest_ts: Option<u64> = None;

    // Process calendar events
    if let Ok(events) = cal_result {
        cache_calendar_events(&events);
        for event in &events {
            // Track oldest timestamp for pagination
            let ts = event.created_at.as_secs();
            oldest_ts = Some(oldest_ts.map_or(ts, |o| o.min(ts)));

            if let Ok(cal_event) = parse_calendar_event(event) {
                all_events.push(UnifiedEvent::Calendar(cal_event));
            }
        }
    }

    // Process meetings
    if let Ok(events) = mtg_result {
        cache_live_events(&events);
        for event in &events {
            // Track oldest timestamp for pagination
            let ts = event.created_at.as_secs();
            oldest_ts = Some(oldest_ts.map_or(ts, |o| o.min(ts)));

            let kind = event.kind.as_u16();
            let activity = match kind {
                KIND_MEETING_ROOM => parse_meeting_room_event(event).ok().map(LiveActivityEvent::Meeting),
                KIND_MEETING_SPACE => parse_meeting_space(event).ok().map(LiveActivityEvent::Space),
                _ => None,
            };
            if let Some(activity) = activity {
                all_events.push(UnifiedEvent::Live(activity));
            }
        }
    }

    Ok((all_events, oldest_ts))
}

/// Fetch personal calendar events for the signed-in user
/// Includes:
/// - Events created by user
/// - Events where user is invited (p tag)
/// - Events user has RSVPed to (accepted)
/// - User's availability blocks
/// - Private events (gift wrapped to user)
pub async fn fetch_personal_calendar_events() -> StdResult<Vec<UnifiedEvent>, String> {
    let pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let pk = PublicKey::from_hex(&pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("[calendar_store] Fetching personal calendar events for {}", pubkey);

    // Filter 1: Calendar events authored by user
    let authored_filter = Filter::new()
        .kinds([Kind::Custom(KIND_DATE_CALENDAR_EVENT), Kind::Custom(KIND_TIME_CALENDAR_EVENT)])
        .author(pk)
        .limit(200);

    // Filter 2: Calendar events where user is invited (p tag)
    let invited_filter = Filter::new()
        .kinds([Kind::Custom(KIND_DATE_CALENDAR_EVENT), Kind::Custom(KIND_TIME_CALENDAR_EVENT)])
        .pubkey(pk)
        .limit(200);

    // Filter 3: User's availability blocks
    let blocks_filter = Filter::new()
        .kind(Kind::Custom(KIND_AVAILABILITY_BLOCK))
        .author(pk)
        .limit(100);

    // Filter 4: User's RSVPs
    let rsvps_filter = Filter::new()
        .kind(Kind::Custom(KIND_CALENDAR_RSVP))
        .author(pk)
        .limit(200);

    // Filter 5: Private events (gift wraps to user)
    let private_filter = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(pk)
        .limit(200);

    // Fetch all in parallel
    let (authored_result, invited_result, blocks_result, rsvps_result, private_result) = futures::join!(
        crate::stores::nostr_client::fetch_events_aggregated(authored_filter, Duration::from_secs(15)),
        crate::stores::nostr_client::fetch_events_aggregated(invited_filter, Duration::from_secs(15)),
        crate::stores::nostr_client::fetch_events_aggregated(blocks_filter, Duration::from_secs(15)),
        crate::stores::nostr_client::fetch_events_aggregated(rsvps_filter, Duration::from_secs(15)),
        crate::stores::nostr_client::fetch_events_aggregated(private_filter, Duration::from_secs(15))
    );

    use crate::utils::nip52::RsvpStatus;

    let mut all_events = Vec::new();
    let mut seen_coords = std::collections::HashSet::new();

    // Process authored events
    if let Ok(events) = authored_result {
        log::info!("[calendar_store] Found {} authored events", events.len());
        cache_calendar_events(&events);
        for event in events {
            if let Ok(cal_event) = parse_calendar_event(&event) {
                let coord = cal_event.coordinate.clone();
                if seen_coords.insert(coord) {
                    all_events.push(UnifiedEvent::Calendar(cal_event));
                }
            }
        }
    }

    // Process invited events (avoid duplicates)
    if let Ok(events) = invited_result {
        log::info!("[calendar_store] Found {} invited events", events.len());
        cache_calendar_events(&events);
        for event in events {
            if let Ok(cal_event) = parse_calendar_event(&event) {
                let coord = cal_event.coordinate.clone();
                if seen_coords.insert(coord) {
                    all_events.push(UnifiedEvent::Calendar(cal_event));
                }
            }
        }
    }

    // Process availability blocks - skip for now, they use a different structure
    // TODO: Add AvailabilityBlock to UnifiedEvent enum to properly support these
    if let Ok(events) = blocks_result {
        log::info!("[calendar_store] Found {} availability blocks (not displayed yet)", events.len());
    }

    // Process RSVPs - fetch the events they reference
    if let Ok(rsvp_events) = rsvps_result {
        log::info!("[calendar_store] Found {} RSVPs", rsvp_events.len());
        for rsvp_event in rsvp_events {
            if let Ok(rsvp) = parse_calendar_rsvp(&rsvp_event) {
                // Only include accepted RSVPs
                if !matches!(rsvp.status, RsvpStatus::Accepted) {
                    continue;
                }

                // Get the event coordinate from the 'a' tag
                let coord = rsvp.event_coordinate.clone();
                if !coord.is_empty() && seen_coords.insert(coord.clone()) {
                    // Try to fetch the referenced event
                    if let Ok(Some(referenced_event)) = fetch_event_by_coordinate(&coord).await {
                        all_events.push(referenced_event);
                    }
                }
            }
        }
    }

    // Process private events (gift wraps)
    if let Ok(wraps) = private_result {
        log::info!("[calendar_store] Found {} gift wraps", wraps.len());
        let client = match crate::stores::nostr_client::get_client() {
            Some(c) => c,
            None => {
                log::warn!("[calendar_store] Client not available for gift wrap decryption");
                return Ok(all_events);
            }
        };

        for wrap in wraps {
            match client.unwrap_gift_wrap(&wrap).await {
                Ok(unwrapped) => {
                    let rumor = unwrapped.rumor;
                    let kind = rumor.kind.as_u16();
                    if kind == KIND_DATE_CALENDAR_EVENT || kind == KIND_TIME_CALENDAR_EVENT {
                        let wrap_id = wrap.id.to_string();
                        if let Ok(mut cal_event) = parse_calendar_rumor(&rumor, &wrap_id) {
                            cal_event.pubkey = unwrapped.sender.to_string();
                            cal_event.source = EventSource::Private { gift_wrap_id: wrap_id.clone() };
                            let coord = cal_event.coordinate.clone();
                            if seen_coords.insert(coord) {
                                all_events.push(UnifiedEvent::Calendar(cal_event));
                            }
                        }
                    }
                }
                Err(e) => {
                    log::debug!("[calendar_store] Failed to unwrap gift wrap: {}", e);
                }
            }
        }
    }

    log::info!("[calendar_store] Total personal calendar events: {}", all_events.len());
    Ok(all_events)
}

/// Helper to fetch a single event by its coordinate (kind:pubkey:d-tag)
async fn fetch_event_by_coordinate(coordinate: &str) -> StdResult<Option<UnifiedEvent>, String> {
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() < 3 {
        return Ok(None);
    }

    let kind: u16 = parts[0].parse().map_err(|_| "Invalid kind")?;
    let pubkey = parts[1];
    let d_tag = parts[2..].join(":");

    let pk = PublicKey::from_hex(pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::Custom(kind))
        .author(pk)
        .identifier(&d_tag)
        .limit(1);

    let events = crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;

    if let Some(event) = events.first() {
        if let Ok(cal_event) = parse_calendar_event(event) {
            return Ok(Some(UnifiedEvent::Calendar(cal_event)));
        }
    }

    Ok(None)
}

/// Fetch private calendar events (NIP-59 gift wraps)
pub async fn fetch_private_calendar_events() -> StdResult<Vec<CalendarEvent>, String> {
    let pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let pk = PublicKey::from_hex(&pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Fetch gift wraps addressed to us
    let filter = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(pk)
        .limit(200);

    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    let events = client.fetch_events(filter, Duration::from_secs(15))
        .await
        .map_err(|e| format!("Failed to fetch gift wraps: {}", e))?;

    let mut private_events = Vec::new();

    for gift_wrap in events.iter() {
        // Try to unwrap the gift wrap
        match client.unwrap_gift_wrap(gift_wrap).await {
            Ok(unwrapped) => {
                let rumor = unwrapped.rumor;
                let rumor_kind = rumor.kind.as_u16();

                // Check if it's a calendar event
                if rumor_kind == KIND_DATE_CALENDAR_EVENT || rumor_kind == KIND_TIME_CALENDAR_EVENT {
                    // Convert rumor to Event-like structure for parsing
                    let gift_wrap_id = gift_wrap.id.to_string();
                    if let Ok(mut cal_event) = parse_calendar_rumor(&rumor, &gift_wrap_id) {
                        cal_event.pubkey = unwrapped.sender.to_string();
                        private_events.push(cal_event);
                    }
                }
            }
            Err(e) => {
                log::debug!("Failed to unwrap gift wrap: {}", e);
                continue;
            }
        }
    }

    // Cache private events (PRIVATE_EVENTS_CACHE is a Vec)
    *PRIVATE_EVENTS_CACHE.write() = private_events.clone();

    Ok(private_events)
}

/// Parse a calendar event from an unsigned rumor (for NIP-59)
fn parse_calendar_rumor(rumor: &UnsignedEvent, gift_wrap_id: &str) -> StdResult<CalendarEvent, String> {
    use crate::utils::nip52::{EventTime, CalendarEventType};

    let kind = rumor.kind.as_u16();

    // Extract tags
    let mut d_tag = String::new();
    let mut title = String::new();
    let mut start = None;
    let mut end = None;
    let mut summary = None;
    let mut image = None;
    let mut locations = Vec::new();
    let mut geohash = None;
    let mut start_tzid = None;
    let mut end_tzid = None;
    let mut hashtags = Vec::new();
    let mut participants = Vec::new();
    let mut references = Vec::new();
    let mut calendar_refs = Vec::new();

    for tag in rumor.tags.iter() {
        let values: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
        if values.is_empty() {
            continue;
        }

        match values[0] {
            "d" if values.len() > 1 => d_tag = values[1].to_string(),
            "title" if values.len() > 1 => title = values[1].to_string(),
            "start" if values.len() > 1 => {
                if kind == KIND_DATE_CALENDAR_EVENT {
                    start = Some(EventTime::Date(values[1].to_string()));
                } else if let Ok(ts) = values[1].parse::<u64>() {
                    start = Some(EventTime::Timestamp(ts));
                }
            },
            "end" if values.len() > 1 => {
                if kind == KIND_DATE_CALENDAR_EVENT {
                    end = Some(EventTime::Date(values[1].to_string()));
                } else if let Ok(ts) = values[1].parse::<u64>() {
                    end = Some(EventTime::Timestamp(ts));
                }
            },
            "summary" if values.len() > 1 => summary = Some(values[1].to_string()),
            "image" if values.len() > 1 => image = Some(values[1].to_string()),
            "location" if values.len() > 1 => locations.push(values[1].to_string()),
            "g" if values.len() > 1 => geohash = Some(values[1].to_string()),
            "start_tzid" if values.len() > 1 => start_tzid = Some(values[1].to_string()),
            "end_tzid" if values.len() > 1 => end_tzid = Some(values[1].to_string()),
            "t" if values.len() > 1 => hashtags.push(values[1].to_string()),
            "r" if values.len() > 1 => references.push(values[1].to_string()),
            "a" if values.len() > 1 && values[1].starts_with("31924:") => {
                calendar_refs.push(values[1].to_string());
            },
            "p" if values.len() > 1 => {
                participants.push(crate::utils::nip52::EventParticipant {
                    pubkey: values[1].to_string(),
                    relay_hint: values.get(2).map(|s| s.to_string()),
                    role: values.get(3).map(|s| s.to_string()),
                });
            },
            _ => {}
        }
    }

    if title.is_empty() {
        return Err("Missing title".to_string());
    }

    let coordinate = format!("{}:{}:{}", kind, rumor.pubkey, d_tag);
    let event_type = if kind == KIND_DATE_CALENDAR_EVENT {
        CalendarEventType::DateBased
    } else {
        CalendarEventType::TimeBased
    };

    Ok(CalendarEvent {
        event_id: String::new(), // Rumor has no id
        pubkey: rumor.pubkey.to_string(),
        d_tag,
        coordinate: coordinate.clone(),
        naddr: String::new(), // Private events don't have naddr
        kind,
        event_type,
        title,
        start: start.unwrap_or(EventTime::Timestamp(0)),
        end,
        summary,
        content: rumor.content.clone(),
        image,
        locations,
        geohash,
        start_tzid,
        end_tzid,
        participants,
        hashtags,
        references,
        calendar_refs,
        color: None,
        source: EventSource::Private { gift_wrap_id: gift_wrap_id.to_string() },
        created_at: rumor.created_at.as_secs(),
    })
}

/// Fetch RSVPs for a specific event
pub async fn fetch_event_rsvps(event_coordinate: &str) -> StdResult<Vec<CalendarRsvp>, String> {
    let filter = rsvps_filter(event_coordinate);
    let events = crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;

    let rsvps: Vec<CalendarRsvp> = events
        .iter()
        .filter_map(|e| parse_calendar_rsvp(e).ok())
        .collect();

    cache_rsvps(event_coordinate, rsvps.clone());
    Ok(rsvps)
}

// ============================================================================
// Event Comments (NIP-22)
// ============================================================================

/// A comment on a calendar event
#[derive(Clone, Debug)]
pub struct CalendarEventComment {
    pub event_id: String,
    pub pubkey: String,
    pub content: String,
    pub created_at: u64,
}

/// Fetch comments for a calendar event by naddr
/// Comments reference the event via 'a' tag (coordinate)
/// Uses fetch_events_from_relays to bypass cache and get fresh data
pub async fn fetch_event_comments(coordinate: &str) -> StdResult<Vec<CalendarEventComment>, String> {
    // Use kind 1111 (NIP-22 comment) with 'a' tag referencing the event coordinate
    let filter = Filter::new()
        .kind(Kind::Custom(1111))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), coordinate.to_string())
        .limit(100);

    // Use fetch_events_from_relays to bypass cache for fresh comment data
    let events = crate::stores::nostr_client::fetch_events_from_relays(filter, Duration::from_secs(10)).await?;

    let mut comments: Vec<CalendarEventComment> = events
        .iter()
        .map(|e| CalendarEventComment {
            event_id: e.id.to_hex(),
            pubkey: e.pubkey.to_hex(),
            content: e.content.clone(),
            created_at: e.created_at.as_secs(),
        })
        .collect();

    // Sort by created_at (newest first)
    comments.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(comments)
}

/// Publish a comment on a calendar event
/// Uses proper NIP-22 threading tags (A/K/P for root, a/k/p for parent)
pub async fn publish_event_comment(
    coordinate: &str,
    event_author: &str,
    content: &str,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;

    // Validate and normalize event_author to canonical hex
    // PublicKey::parse() accepts hex, bech32 (npub), and NIP21 URIs
    let author_pubkey = PublicKey::parse(event_author)
        .map_err(|e| format!("Invalid event author pubkey '{}': {}", event_author, e))?;
    let author_hex = author_pubkey.to_hex();

    // Parse coordinate to extract kind (format: "kind:pubkey:d-tag")
    // Validate format following nostr-sdk nip01.rs pattern
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() < 3 {
        return Err(format!(
            "Invalid coordinate format '{}': expected 'kind:pubkey:d-tag'",
            coordinate
        ));
    }
    let event_kind: u16 = parts[0].parse()
        .map_err(|_| format!("Invalid kind in coordinate: {}", parts[0]))?;

    // Build NIP-22 comment (kind 1111) with full tag set
    let builder = EventBuilder::new(Kind::Custom(1111), content)
        // Root scope tags (uppercase) - for the calendar event being commented on
        .tag(Tag::custom(TagKind::SingleLetter(SingleLetterTag::uppercase(Alphabet::A)), vec![coordinate.to_string()]))
        .tag(Tag::custom(TagKind::SingleLetter(SingleLetterTag::uppercase(Alphabet::K)), vec![event_kind.to_string()]))
        .tag(Tag::custom(TagKind::SingleLetter(SingleLetterTag::uppercase(Alphabet::P)), vec![author_hex.clone()]))
        // Parent scope tags (lowercase) - same as root for top-level comments
        .tag(Tag::custom(TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::A)), vec![coordinate.to_string()]))
        .tag(Tag::custom(TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::K)), vec![event_kind.to_string()]))
        .tag(Tag::custom(TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::P)), vec![author_hex]));

    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish comment: {}", e))?;

    Ok(output.id().to_string())
}

// ============================================================================
// NIP-50 Search
// ============================================================================

/// Maximum search result limit to prevent excessive relay load
const MAX_SEARCH_LIMIT: usize = 500;

/// Maximum query length to prevent abuse
const MAX_QUERY_LEN: usize = 256;

/// Search calendar events using NIP-50 relay search
/// Searches across title, description, and content fields
/// Uses fetch_events_from_relays to bypass cache for fresh search results
pub async fn search_calendar_events(query: &str, limit: usize) -> StdResult<Vec<UnifiedEvent>, String> {
    // Validate and clamp inputs
    let query = &query[..query.len().min(MAX_QUERY_LEN)];
    let limit = limit.min(MAX_SEARCH_LIMIT);

    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    // NIP-50 search filter for calendar event kinds (31922, 31923)
    let filter = Filter::new()
        .kinds([Kind::Custom(KIND_DATE_CALENDAR_EVENT), Kind::Custom(KIND_TIME_CALENDAR_EVENT)])
        .search(query)
        .limit(limit);

    // Use fetch_events_from_relays to bypass cache for fresh search results
    let events = crate::stores::nostr_client::fetch_events_from_relays(filter, Duration::from_secs(10)).await?;

    // Parse events into UnifiedEvent
    let mut results: Vec<UnifiedEvent> = events
        .iter()
        .filter_map(|e| {
            match parse_calendar_event(e) {
                Ok(cal_event) => Some(UnifiedEvent::Calendar(cal_event)),
                Err(_) => None,
            }
        })
        .collect();

    // Cache the raw events
    cache_calendar_events(&events);

    // Sort by relevance (start time for calendar events)
    results.sort_by_key(|a| a.start_timestamp());

    Ok(results)
}

/// Search all events (calendar + meetings) using NIP-50 relay search
/// Searches across title, description, and content fields for both calendar events and meetings
/// Uses fetch_events_from_relays to bypass cache for fresh search results
pub async fn search_all_events(query: &str, limit: usize) -> StdResult<Vec<UnifiedEvent>, String> {
    use crate::utils::nip53::{parse_meeting_room_event, parse_meeting_space, LiveActivityEvent, KIND_MEETING_ROOM, KIND_MEETING_SPACE};

    // Validate and clamp inputs
    let query = &query[..query.len().min(MAX_QUERY_LEN)];
    let limit = limit.min(MAX_SEARCH_LIMIT);

    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    // Build filters for both calendar and meeting kinds
    let cal_filter = Filter::new()
        .kinds([Kind::Custom(KIND_DATE_CALENDAR_EVENT), Kind::Custom(KIND_TIME_CALENDAR_EVENT)])
        .search(query)
        .limit(limit);

    let meeting_filter = Filter::new()
        .kinds([Kind::Custom(KIND_MEETING_SPACE), Kind::Custom(KIND_MEETING_ROOM)])
        .search(query)
        .limit(limit);

    // Fetch in parallel
    let (cal_result, meeting_result) = futures::join!(
        crate::stores::nostr_client::fetch_events_from_relays(cal_filter, Duration::from_secs(10)),
        crate::stores::nostr_client::fetch_events_from_relays(meeting_filter, Duration::from_secs(10))
    );

    // Only fail if BOTH fetches failed (following Nostr SDK graceful degradation pattern)
    if cal_result.is_err() && meeting_result.is_err() {
        return Err(format!(
            "Search failed - calendar: {}, meetings: {}",
            cal_result.as_ref().unwrap_err(),
            meeting_result.as_ref().unwrap_err()
        ));
    }

    let mut results = Vec::new();

    // Process calendar events (only if Ok)
    if let Ok(events) = cal_result {
        cache_calendar_events(&events);
        for event in &events {
            if let Ok(cal_event) = parse_calendar_event(event) {
                results.push(UnifiedEvent::Calendar(cal_event));
            }
        }
    }

    // Process meeting events
    if let Ok(events) = meeting_result {
        cache_live_events(&events);
        for event in &events {
            let kind = event.kind.as_u16();
            let activity = match kind {
                KIND_MEETING_ROOM => parse_meeting_room_event(event).ok().map(LiveActivityEvent::Meeting),
                KIND_MEETING_SPACE => parse_meeting_space(event).ok().map(LiveActivityEvent::Space),
                _ => None,
            };
            if let Some(activity) = activity {
                results.push(UnifiedEvent::Live(activity));
            }
        }
    }

    // Sort by start time
    results.sort_by_key(|e| e.start_timestamp());
    Ok(results)
}

/// Fetch user's RSVPs
pub async fn fetch_my_rsvps(pubkey: &str) -> StdResult<Vec<CalendarRsvp>, String> {
    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = my_rsvps_filter(pk);
    let events = crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;

    let rsvps: Vec<CalendarRsvp> = events
        .iter()
        .filter_map(|e| parse_calendar_rsvp(e).ok())
        .collect();

    // Cache user's RSVPs
    for rsvp in &rsvps {
        cache_my_rsvp(rsvp.clone());
    }

    Ok(rsvps)
}

/// Fetch private calendar events (NIP-59 gift wraps)
pub async fn fetch_private_events() -> StdResult<Vec<CalendarEvent>, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let pk = PublicKey::from_hex(&pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = private_events_filter(pk);
    let events = crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await?;

    let mut private_events = Vec::new();

    for gift_wrap in events {
        // Only process gift wraps
        if gift_wrap.kind != Kind::GiftWrap {
            continue;
        }

        // Try to unwrap the gift wrap
        match client.unwrap_gift_wrap(&gift_wrap).await {
            Ok(unwrapped) => {
                let rumor = unwrapped.rumor;
                let kind = rumor.kind.as_u16();

                // Check if it's a calendar event
                if kind == KIND_DATE_CALENDAR_EVENT || kind == KIND_TIME_CALENDAR_EVENT {
                    // Parse the rumor as a calendar event (need to convert UnsignedEvent)
                    if let Ok(mut cal_event) = parse_unsigned_calendar_event(&rumor) {
                        // Mark as private with gift wrap ID
                        cal_event.source = EventSource::Private {
                            gift_wrap_id: gift_wrap.id.to_hex(),
                        };
                        private_events.push(cal_event);
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to unwrap gift wrap: {}", e);
            }
        }
    }

    // Update cache
    *PRIVATE_EVENTS_CACHE.write() = private_events.clone();

    Ok(private_events)
}

/// Parse calendar event from unsigned event (rumor)
fn parse_unsigned_calendar_event(rumor: &UnsignedEvent) -> StdResult<CalendarEvent, String> {
    use crate::utils::nip52::{CalendarEventType, EventParticipant};

    let kind = rumor.kind.as_u16();
    let event_type = CalendarEventType::from_kind(kind)
        .ok_or_else(|| format!("Expected kind 31922 or 31923, got {}", kind))?;

    let pubkey = rumor.pubkey.to_hex();

    // Helper to get tag value
    let get_tag = |name: &str| -> Option<String> {
        rumor.tags.iter()
            .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(name))
            .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
    };

    let get_all_tags = |name: &str| -> Vec<String> {
        rumor.tags.iter()
            .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some(name))
            .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
            .collect()
    };

    let d_tag = get_tag("d").ok_or("Missing required 'd' tag")?;
    let coordinate = format!("{}:{}:{}", kind, pubkey, d_tag);
    let naddr = "".to_string(); // Can't build naddr for unsigned event

    let title = get_tag("title")
        .or_else(|| get_tag("name"))
        .ok_or("Missing required 'title' tag")?;

    let start_str = get_tag("start").ok_or("Missing required 'start' tag")?;
    let start = EventTime::parse(&start_str, event_type)
        .ok_or_else(|| format!("Invalid start time: {}", start_str))?;

    let end = get_tag("end").and_then(|s| EventTime::parse(&s, event_type));

    // Parse participants
    let participants: Vec<EventParticipant> = rumor.tags.iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p"))
        .filter_map(|t| {
            let slice = t.as_slice();
            let pk = slice.get(1)?.to_string();
            let relay = slice.get(2).map(|s| s.to_string()).filter(|s| !s.is_empty());
            let role = slice.get(3).map(|s| s.to_string()).filter(|s| !s.is_empty());
            Some(EventParticipant { pubkey: pk, relay_hint: relay, role })
        })
        .collect();

    Ok(CalendarEvent {
        d_tag,
        event_id: "".to_string(), // No ID for unsigned
        pubkey,
        naddr,
        coordinate,
        kind,
        event_type,
        created_at: rumor.created_at.as_secs(),
        title,
        start,
        end,
        start_tzid: get_tag("start_tzid").or_else(|| get_tag("timezone")),
        end_tzid: get_tag("end_tzid"),
        summary: get_tag("summary"),
        content: rumor.content.clone(),
        image: get_tag("image"),
        locations: get_all_tags("location"),
        geohash: get_tag("g"),
        participants,
        hashtags: get_all_tags("t"),
        references: get_all_tags("r"),
        calendar_refs: Vec::new(),
        color: None,
        source: EventSource::Public, // Will be overwritten
    })
}

/// Fetch specific event by naddr
pub async fn fetch_event_by_naddr(naddr: &str) -> StdResult<Option<CalendarEvent>, String> {
    log::info!("[calendar_store] fetch_event_by_naddr called with: {}", naddr);

    // Check cache first
    if let Some(cached) = get_cached_event_by_naddr(naddr) {
        log::info!("[calendar_store] Found in cache: {}", cached.title);
        return Ok(Some(cached));
    }

    // Parse naddr
    let nip19 = Nip19Coordinate::from_bech32(naddr)
        .map_err(|e| {
            log::error!("[calendar_store] Invalid naddr '{}': {}", naddr, e);
            format!("Invalid naddr: {}", e)
        })?;

    let coord = nip19.coordinate;
    let pk = coord.public_key;
    let kind = coord.kind.as_u16();
    let identifier = coord.identifier.clone();

    log::info!("[calendar_store] Parsed naddr - kind: {}, pubkey: {}, identifier: {}", kind, pk, identifier);

    let filter = event_by_coordinate_filter(pk, kind, &identifier);
    log::info!("[calendar_store] Fetching from relays with 10s timeout...");

    let events = crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;
    log::info!("[calendar_store] Received {} events from relays", events.len());

    if let Some(event) = events.first() {
        log::info!("[calendar_store] Attempting to parse event kind {}", event.kind.as_u16());
        if let Ok(cal_event) = parse_calendar_event(event) {
            log::info!("[calendar_store] Successfully parsed event: {}", cal_event.title);
            cache_event(cal_event.clone());
            return Ok(Some(cal_event));
        } else {
            log::warn!("[calendar_store] Failed to parse event as CalendarEvent");
        }
    }

    log::warn!("[calendar_store] No matching event found for naddr: {}", naddr);
    Ok(None)
}

/// Fetch unified event by naddr (handles both calendar events and meetings)
pub async fn fetch_unified_event_by_naddr(naddr: &str) -> StdResult<Option<UnifiedEvent>, String> {
    log::info!("[calendar_store] fetch_unified_event_by_naddr called with: {}", naddr);

    // Check calendar cache first
    if let Some(cached) = get_cached_event_by_naddr(naddr) {
        log::info!("[calendar_store] Found calendar event in cache: {}", cached.title);
        return Ok(Some(UnifiedEvent::Calendar(cached)));
    }

    // Check live events cache
    {
        let cache = LIVE_EVENTS_CACHE.read();
        if let Some((_, activity)) = cache.iter().find(|(_, e)| e.naddr() == naddr) {
            log::info!("[calendar_store] Found meeting in cache: {}", activity.title());
            return Ok(Some(UnifiedEvent::Live(activity.clone())));
        }
    }

    // Parse naddr
    let nip19 = Nip19Coordinate::from_bech32(naddr)
        .map_err(|e| {
            log::error!("[calendar_store] Invalid naddr '{}': {}", naddr, e);
            format!("Invalid naddr: {}", e)
        })?;

    let coord = nip19.coordinate;
    let pk = coord.public_key;
    let kind = coord.kind.as_u16();
    let identifier = coord.identifier.clone();

    log::info!("[calendar_store] Parsed naddr - kind: {}, pubkey: {}, identifier: {}", kind, pk, identifier);

    let filter = event_by_coordinate_filter(pk, kind, &identifier);
    log::info!("[calendar_store] Fetching from relays with 10s timeout...");

    let events = crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;
    log::info!("[calendar_store] Received {} events from relays", events.len());

    if let Some(event) = events.first() {
        let event_kind = event.kind.as_u16();
        log::info!("[calendar_store] Attempting to parse event kind {}", event_kind);

        // Try parsing based on kind
        match event_kind {
            // Calendar events (NIP-52)
            KIND_DATE_CALENDAR_EVENT | KIND_TIME_CALENDAR_EVENT => {
                if let Ok(cal_event) = parse_calendar_event(event) {
                    log::info!("[calendar_store] Successfully parsed calendar event: {}", cal_event.title);
                    cache_event(cal_event.clone());
                    return Ok(Some(UnifiedEvent::Calendar(cal_event)));
                }
            }
            // Meeting Space (NIP-53 kind 30312)
            KIND_MEETING_SPACE => {
                if let Ok(space) = parse_meeting_space(event) {
                    log::info!("[calendar_store] Successfully parsed meeting space: {}", space.room_name);
                    let activity = LiveActivityEvent::Space(space);
                    LIVE_EVENTS_CACHE.write().put(activity.coordinate().to_string(), activity.clone());
                    return Ok(Some(UnifiedEvent::Live(activity)));
                }
            }
            // Meeting Room (NIP-53 kind 30313)
            KIND_MEETING_ROOM => {
                if let Ok(meeting) = parse_meeting_room_event(event) {
                    log::info!("[calendar_store] Successfully parsed meeting room: {}", meeting.title);
                    let activity = LiveActivityEvent::Meeting(meeting);
                    LIVE_EVENTS_CACHE.write().put(activity.coordinate().to_string(), activity.clone());
                    return Ok(Some(UnifiedEvent::Live(activity)));
                }
            }
            _ => {
                log::warn!("[calendar_store] Unknown event kind: {}", event_kind);
            }
        }
    }

    log::warn!("[calendar_store] No matching event found for naddr: {}", naddr);
    Ok(None)
}

// ============================================================================
// Publish Functions
// ============================================================================

/// Publish a date-based calendar event (kind 31922)
/// participants: slice of (pubkey_hex, role) tuples
#[allow(clippy::too_many_arguments)]
pub async fn publish_date_event(
    title: &str,
    start_date: &str,  // YYYY-MM-DD
    end_date: Option<&str>,
    summary: Option<&str>,
    content: Option<&str>,
    image: Option<&str>,
    locations: &[String],
    hashtags: &[String],
    participants: &[(String, String)], // (pubkey_hex, role)
    is_private: bool,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;

    // Generate unique d-tag
    let d_tag = format!("event-{}", js_sys::Date::now() as u64);

    // Build the event
    let mut builder = EventBuilder::new(Kind::Custom(KIND_DATE_CALENDAR_EVENT), content.unwrap_or(""))
        .tag(Tag::identifier(&d_tag))
        .tag(Tag::custom(TagKind::Custom("title".into()), vec![title.to_string()]))
        .tag(Tag::custom(TagKind::Custom("start".into()), vec![start_date.to_string()]));

    // Optional end date
    if let Some(end) = end_date {
        builder = builder.tag(Tag::custom(TagKind::Custom("end".into()), vec![end.to_string()]));
    }

    // Optional summary
    if let Some(s) = summary {
        builder = builder.tag(Tag::custom(TagKind::Custom("summary".into()), vec![s.to_string()]));
    }

    // Optional image
    if let Some(img) = image {
        builder = builder.tag(Tag::custom(TagKind::Custom("image".into()), vec![img.to_string()]));
    }

    // Locations
    for loc in locations {
        builder = builder.tag(Tag::custom(TagKind::Custom("location".into()), vec![loc.to_string()]));
    }

    // Hashtags
    for tag in hashtags {
        builder = builder.tag(Tag::hashtag(tag));
    }

    // Participants (p tags with optional relay and role)
    // Use PublicKey::parse() to accept hex, bech32 (npub), and NIP21 URIs
    for (pubkey, role) in participants {
        match PublicKey::parse(pubkey) {
            Ok(pk) => {
                // Format: ["p", pubkey, relay_hint, role]
                builder = builder.tag(Tag::custom(
                    TagKind::p(),
                    vec![pk.to_hex(), "".to_string(), role.clone()]
                ));
            }
            Err(e) => {
                log::warn!("Invalid participant pubkey '{}': {}", pubkey, e);
            }
        }
    }

    if is_private {
        // TODO: Implement NIP-59 gift wrap for private events
        return Err("Private events not yet implemented".to_string());
    }

    let _output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish event: {}", e))?;

    // Build nostr:naddr1... URI (NIP-21)
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let naddr = format!("nostr:{}", encode_naddr(KIND_DATE_CALENDAR_EVENT, &pubkey, &d_tag));

    Ok(naddr)
}

/// Publish a time-based calendar event (kind 31923)
/// participants: slice of (pubkey_hex, role) tuples
#[allow(clippy::too_many_arguments)]
pub async fn publish_time_event(
    title: &str,
    start_timestamp: u64,
    end_timestamp: Option<u64>,
    summary: Option<&str>,
    content: Option<&str>,
    image: Option<&str>,
    locations: &[String],
    hashtags: &[String],
    participants: &[(String, String)], // (pubkey_hex, role)
    timezone: Option<&str>,
    is_private: bool,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;

    // Generate unique d-tag
    let d_tag = format!("event-{}", js_sys::Date::now() as u64);

    // Build the event
    let mut builder = EventBuilder::new(Kind::Custom(KIND_TIME_CALENDAR_EVENT), content.unwrap_or(""))
        .tag(Tag::identifier(&d_tag))
        .tag(Tag::custom(TagKind::Custom("title".into()), vec![title.to_string()]))
        .tag(Tag::custom(TagKind::Custom("start".into()), vec![start_timestamp.to_string()]));

    // Optional end timestamp
    if let Some(end) = end_timestamp {
        builder = builder.tag(Tag::custom(TagKind::Custom("end".into()), vec![end.to_string()]));
    }

    // Optional summary
    if let Some(s) = summary {
        builder = builder.tag(Tag::custom(TagKind::Custom("summary".into()), vec![s.to_string()]));
    }

    // Optional image
    if let Some(img) = image {
        builder = builder.tag(Tag::custom(TagKind::Custom("image".into()), vec![img.to_string()]));
    }

    // Timezone
    if let Some(tz) = timezone {
        builder = builder.tag(Tag::custom(TagKind::Custom("start_tzid".into()), vec![tz.to_string()]));
    }

    // Locations
    for loc in locations {
        builder = builder.tag(Tag::custom(TagKind::Custom("location".into()), vec![loc.to_string()]));
    }

    // Hashtags
    for tag in hashtags {
        builder = builder.tag(Tag::hashtag(tag));
    }

    // Participants (p tags with optional relay and role)
    // Use PublicKey::parse() to accept hex, bech32 (npub), and NIP21 URIs
    for (pubkey, role) in participants {
        match PublicKey::parse(pubkey) {
            Ok(pk) => {
                // Format: ["p", pubkey, relay_hint, role]
                builder = builder.tag(Tag::custom(
                    TagKind::p(),
                    vec![pk.to_hex(), "".to_string(), role.clone()]
                ));
            }
            Err(e) => {
                log::warn!("Invalid participant pubkey '{}': {}", pubkey, e);
            }
        }
    }

    if is_private {
        // TODO: Implement NIP-59 gift wrap for private events
        return Err("Private events not yet implemented".to_string());
    }

    let _output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish event: {}", e))?;

    // Build nostr:naddr1... URI (NIP-21)
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let naddr = format!("nostr:{}", encode_naddr(KIND_TIME_CALENDAR_EVENT, &pubkey, &d_tag));

    Ok(naddr)
}

/// Helper to encode naddr (simplified)
fn encode_naddr(kind: u16, pubkey: &str, d_tag: &str) -> String {
    use nostr::nips::nip01::Coordinate;
    use nostr::nips::nip19::ToBech32;

    if let Ok(pk) = PublicKey::from_hex(pubkey) {
        let coord = Coordinate::new(Kind::Custom(kind), pk).identifier(d_tag);
        if let Ok(bech32) = coord.to_bech32() {
            return bech32;
        }
    }

    // Fallback: just return coordinate string
    format!("{}:{}:{}", kind, pubkey, d_tag)
}

/// Publish a calendar event RSVP
pub async fn publish_rsvp(
    event_coordinate: &str,
    event_author: &str,
    status: crate::utils::nip52::RsvpStatus,
    note: &str,
) -> StdResult<String, String> {
    use crate::utils::nip52::FreeBusy;

    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;

    // Generate unique d-tag for RSVP
    let d_tag = format!("rsvp-{}", js_sys::Date::now() as u64);

    let free_busy = FreeBusy::from_rsvp_status(status);

    let event = EventBuilder::new(Kind::Custom(KIND_CALENDAR_RSVP), note)
        .tag(Tag::identifier(&d_tag))
        .tag(Tag::custom(TagKind::a(), vec![event_coordinate.to_string()]))
        .tag(Tag::public_key(
            PublicKey::from_hex(event_author)
                .map_err(|e| format!("Invalid author pubkey: {}", e))?,
        ))
        .tag(Tag::custom(TagKind::Custom("status".into()), vec![status.as_str().to_string()]))
        .tag(Tag::custom(TagKind::Custom("fb".into()), vec![free_busy.as_str().to_string()]));

    let output = client
        .send_event_builder(event)
        .await
        .map_err(|e| format!("Failed to publish RSVP: {}", e))?;

    Ok(output.id().to_string())
}

// ============================================================================
// Cache Management
// ============================================================================

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

/// Initialize calendar store
pub async fn init_calendar_store() -> StdResult<(), String> {
    if *CALENDAR_INITIALIZED.read() {
        return Ok(());
    }

    // Fetch initial events
    fetch_all_events(200).await?;

    // Fetch user's RSVPs if logged in
    if let Some(pubkey) = crate::stores::auth_store::get_pubkey() {
        if let Err(e) = fetch_my_rsvps(&pubkey).await {
            log::warn!("Failed to fetch user RSVPs: {}", e);
        }
    }

    *CALENDAR_INITIALIZED.write() = true;
    Ok(())
}

// ============================================================================
// Availability (Booking) Functions
// ============================================================================

/// Fetch availability templates for a user (kind 31926)
pub async fn fetch_availability_templates(pubkey: &str) -> StdResult<Vec<AvailabilityTemplate>, String> {
    use crate::utils::nip52::parse_availability_template;

    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = availability_templates_filter(pk);
    let events = crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;

    let templates: Vec<AvailabilityTemplate> = events
        .iter()
        .filter_map(|e| parse_availability_template(e).ok())
        .collect();

    // Cache templates
    {
        let mut cache = AVAILABILITY_TEMPLATES_CACHE.write();
        for template in &templates {
            cache.put(template.coordinate.clone(), template.clone());
        }
    }

    Ok(templates)
}

/// Fetch availability blocks for a user (kind 31927)
pub async fn fetch_availability_blocks(pubkey: &str) -> StdResult<Vec<AvailabilityBlock>, String> {
    use crate::utils::nip52::parse_availability_block;

    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = availability_blocks_filter(pk);
    let events = crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;

    let blocks: Vec<AvailabilityBlock> = events
        .iter()
        .filter_map(|e| parse_availability_block(e).ok())
        .collect();

    // Cache blocks
    {
        let mut cache = AVAILABILITY_BLOCKS_CACHE.write();
        for block in &blocks {
            cache.put(block.coordinate.clone(), block.clone());
        }
    }

    Ok(blocks)
}

/// Fetch all booking data (templates + blocks) for a user
pub async fn fetch_booking_data(pubkey: &str) -> StdResult<(Vec<AvailabilityTemplate>, Vec<AvailabilityBlock>), String> {
    let (templates, blocks) = futures::join!(
        fetch_availability_templates(pubkey),
        fetch_availability_blocks(pubkey)
    );

    Ok((templates?, blocks?))
}

/// Publish an availability template (kind 31926)
#[allow(clippy::too_many_arguments)]
pub async fn publish_availability_template(
    title: &str,
    duration_minutes: u32,
    interval_minutes: Option<u32>,
    buffer_before: Option<u32>,
    buffer_after: Option<u32>,
    timezone: Option<&str>,
    min_notice_days: Option<u32>,
    max_advance_days: Option<u32>,
    amount_sats: Option<u64>,
    schedule: &[(String, String, String)],  // [(day, start_time, end_time)]
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;

    let d_tag = format!("avail-{}", js_sys::Date::now() as u64);

    let mut builder = EventBuilder::new(Kind::Custom(KIND_AVAILABILITY_TEMPLATE), "")
        .tag(Tag::identifier(&d_tag))
        .tag(Tag::custom(TagKind::Custom("title".into()), vec![title.to_string()]))
        .tag(Tag::custom(TagKind::Custom("duration".into()), vec![format!("PT{}M", duration_minutes)]));

    if let Some(interval) = interval_minutes {
        builder = builder.tag(Tag::custom(TagKind::Custom("interval".into()), vec![format!("PT{}M", interval)]));
    }

    if let Some(before) = buffer_before {
        builder = builder.tag(Tag::custom(TagKind::Custom("buffer_before".into()), vec![format!("PT{}M", before)]));
    }

    if let Some(after) = buffer_after {
        builder = builder.tag(Tag::custom(TagKind::Custom("buffer_after".into()), vec![format!("PT{}M", after)]));
    }

    if let Some(tz) = timezone {
        builder = builder.tag(Tag::custom(TagKind::Custom("tzid".into()), vec![tz.to_string()]));
    }

    if let Some(notice) = min_notice_days {
        builder = builder.tag(Tag::custom(TagKind::Custom("min_notice".into()), vec![format!("P{}D", notice)]));
    }

    if let Some(advance) = max_advance_days {
        builder = builder.tag(Tag::custom(TagKind::Custom("max_advance".into()), vec![format!("P{}D", advance)]));
    }

    if let Some(amount) = amount_sats {
        builder = builder.tag(Tag::custom(TagKind::Custom("amount".into()), vec![amount.to_string()]));
    }

    // Add schedule entries
    for (day, start, end) in schedule {
        builder = builder.tag(Tag::custom(
            TagKind::Custom("sch".into()),
            vec![day.clone(), start.clone(), end.clone()]
        ));
    }

    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish availability template: {}", e))?;

    Ok(output.id().to_string())
}

/// Publish an availability block (kind 31927)
pub async fn publish_availability_block(
    start: u64,
    end: u64,
    title: Option<&str>,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;

    let d_tag = format!("block-{}", js_sys::Date::now() as u64);

    let mut builder = EventBuilder::new(Kind::Custom(KIND_AVAILABILITY_BLOCK), "")
        .tag(Tag::identifier(&d_tag))
        .tag(Tag::custom(TagKind::Custom("start".into()), vec![start.to_string()]))
        .tag(Tag::custom(TagKind::Custom("end".into()), vec![end.to_string()]));

    if let Some(t) = title {
        builder = builder.tag(Tag::custom(TagKind::Custom("title".into()), vec![t.to_string()]));
    }

    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish availability block: {}", e))?;

    Ok(output.id().to_string())
}

/// Get cached availability template
pub fn get_cached_availability_template(coordinate: &str) -> Option<AvailabilityTemplate> {
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

// ============================================================================
// Calendar Collections (Kind 31924)
// ============================================================================

/// Calendars cache
pub static CALENDARS_CACHE: GlobalSignal<LruCache<String, crate::utils::nip52::Calendar>> =
    GlobalSignal::new(|| LruCache::new(NonZeroUsize::new(50).unwrap()));

/// Build filter for calendars by author
pub fn calendars_filter(pubkey: PublicKey) -> Filter {
    use crate::utils::nip52::KIND_CALENDAR;
    Filter::new()
        .kind(Kind::Custom(KIND_CALENDAR))
        .author(pubkey)
}

/// Fetch calendars for a user (kind 31924)
pub async fn fetch_calendars(pubkey: &str) -> StdResult<Vec<crate::utils::nip52::Calendar>, String> {
    use crate::utils::nip52::parse_calendar;

    let pk = PublicKey::from_hex(pubkey)
        .or_else(|_| PublicKey::from_bech32(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = calendars_filter(pk);
    let events = crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;

    let calendars: Vec<crate::utils::nip52::Calendar> = events
        .iter()
        .filter_map(|e| parse_calendar(e).ok())
        .collect();

    // Cache calendars
    {
        let mut cache = CALENDARS_CACHE.write();
        for calendar in &calendars {
            cache.put(calendar.coordinate.clone(), calendar.clone());
        }
    }

    Ok(calendars)
}

/// Publish a calendar collection (kind 31924)
pub async fn publish_calendar(
    title: &str,
    description: &str,
    event_coordinates: &[String],
) -> StdResult<String, String> {
    use crate::utils::nip52::KIND_CALENDAR;

    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;

    let d_tag = format!("calendar-{}", js_sys::Date::now() as u64);

    let mut builder = EventBuilder::new(Kind::Custom(KIND_CALENDAR), description)
        .tag(Tag::identifier(&d_tag))
        .tag(Tag::custom(TagKind::Custom("title".into()), vec![title.to_string()]));

    // Add event references
    for coord in event_coordinates {
        builder = builder.tag(Tag::custom(TagKind::a(), vec![coord.clone()]));
    }

    let _output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish calendar: {}", e))?;

    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let naddr = encode_naddr(KIND_CALENDAR, &pubkey, &d_tag);

    Ok(naddr)
}

/// Get cached calendar
pub fn get_cached_calendar(coordinate: &str) -> Option<crate::utils::nip52::Calendar> {
    CALENDARS_CACHE.read().peek(coordinate).cloned()
}

/// Get all cached calendars
pub fn get_all_cached_calendars() -> Vec<crate::utils::nip52::Calendar> {
    CALENDARS_CACHE.read().iter().map(|(_, c)| c.clone()).collect()
}

// ============================================================================
// Room Presence (NIP-53)
// ============================================================================

/// Fetch room presence for a meeting/space coordinate
/// Returns list of users present in the room within the last `max_age_secs` seconds
pub async fn fetch_room_presence(room_coordinate: &str, max_age_secs: u64) -> StdResult<Vec<RoomPresence>, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;

    let now_secs = (js_sys::Date::now() / 1000.0) as u64;
    let since_ts = now_secs.saturating_sub(max_age_secs);

    // Filter for kind 10312 presence events referencing this room
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_ROOM_PRESENCE))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), room_coordinate.to_string())
        .since(Timestamp::from(since_ts));

    log::info!("[calendar_store] Fetching room presence for: {}", room_coordinate);

    let events = client
        .fetch_events(filter, Duration::from_secs(5))
        .await
        .map_err(|e| format!("Failed to fetch presence: {}", e))?;

    log::info!("[calendar_store] Found {} presence events", events.len());

    // Parse and deduplicate by pubkey (keep most recent per user)
    let mut presence_by_user: std::collections::HashMap<String, RoomPresence> = std::collections::HashMap::new();

    for event in events.iter() {
        if let Ok(presence) = parse_room_presence(event) {
            let existing = presence_by_user.get(&presence.pubkey);
            if existing.is_none() || existing.unwrap().created_at < presence.created_at {
                presence_by_user.insert(presence.pubkey.clone(), presence);
            }
        }
    }

    let mut result: Vec<RoomPresence> = presence_by_user.into_values().collect();
    // Sort by created_at descending (most recent first)
    result.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(result)
}

// ============================================================================
// Statistics
// ============================================================================

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
        + live_cache
            .iter()
            .filter(|(_, e)| e.start_timestamp() >= now_secs)
            .count();

    CalendarStats {
        total_events: cal_cache.len() + live_cache.len() + private.len(),
        calendar_events: cal_cache.len(),
        live_activities: live_cache.len(),
        private_events: private.len(),
        upcoming_events: upcoming,
        unique_hashtags: hashtags.len(),
    }
}
