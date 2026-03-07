use super::*;

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
#[derive(Clone, Debug, PartialEq)]
pub struct EventFilterState {
    pub search_term: String,
    pub time_filter: TimeFilter,
    pub location_filter: LocationFilter,
    pub event_type_filter: EventTypeFilter,
    pub hashtag: Option<String>,
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
            hide_ended: true,
        }
    }
}
impl EventFilterState {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
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
pub fn filter_events_with_nip50(
    events: &[UnifiedEvent],
    filters: &EventFilterState,
    from_nip50: bool,
) -> Vec<UnifiedEvent> {
    let now_secs = crate::platform::timestamp::now_secs();
    let today_start = now_secs - (now_secs % 86400);
    let week_end = today_start + (7 * 86400);
    let month_end = today_start + (30 * 86400);
    events
        .iter()
        .filter(|event| {
            if filters.hide_ended && filters.time_filter != TimeFilter::Past {
                let end_ts = event.effective_end_timestamp();
                let grace_period = 86400;
                if end_ts < now_secs.saturating_sub(grace_period) {
                    return false;
                }
            }
            if !from_nip50 && !filters.search_term.is_empty() {
                let term = filters.search_term.to_lowercase();
                let matches = event.title().to_lowercase().contains(&term);
                if !matches {
                    return false;
                }
            }
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
            let start_ts = event.start_timestamp();
            let end_ts = event.effective_end_timestamp();
            match filters.time_filter {
                TimeFilter::All => {
                    if filters.hide_ended && end_ts < now_secs.saturating_sub(86400) {
                        return false;
                    }
                }
                TimeFilter::Today => {
                    if start_ts >= today_start + 86400 || end_ts < today_start {
                        return false;
                    }
                }
                TimeFilter::ThisWeek => {
                    if start_ts >= week_end || end_ts < today_start {
                        return false;
                    }
                }
                TimeFilter::ThisMonth => {
                    if start_ts >= month_end || end_ts < today_start {
                        return false;
                    }
                }
                TimeFilter::Upcoming => {
                    if end_ts < now_secs {
                        return false;
                    }
                }
                TimeFilter::Past => {
                    if end_ts >= now_secs {
                        return false;
                    }
                }
            }
            match filters.location_filter {
                LocationFilter::All => {}
                LocationFilter::InPerson => {
                    let locs = event.locations();
                    if locs.is_empty()
                        || locs
                            .iter()
                            .all(|l| crate::utils::nip52::is_online_location(l))
                    {
                        return false;
                    }
                }
                LocationFilter::Online => {
                    let locs = event.locations();
                    if locs.is_empty()
                        || locs
                            .iter()
                            .all(|l| !crate::utils::nip52::is_online_location(l))
                    {
                        return false;
                    }
                }
            }
            if let Some(tag) = &filters.hashtag {
                if !event.hashtags().contains(&tag.as_str()) {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect()
}
/// Sort events (events with images first, then by date, then by coordinate for stability)
pub fn sort_events_for_display(events: &mut [UnifiedEvent]) {
    events.sort_by(|a, b| {
        let a_has_image = a.image().is_some();
        let b_has_image = b.image().is_some();
        match (a_has_image, b_has_image) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .start_timestamp()
                .cmp(&b.start_timestamp())
                .then_with(|| a.coordinate().cmp(b.coordinate())),
        }
    });
}

/// Build filter for calendar events (date and time-based)
pub fn calendar_events_filter(limit: usize) -> Filter {
    Filter::new()
        .kinds([
            Kind::Custom(KIND_DATE_CALENDAR_EVENT),
            Kind::Custom(KIND_TIME_CALENDAR_EVENT),
        ])
        .limit(limit)
}
/// Build filter for calendar events since a timestamp
pub fn calendar_events_filter_since(since: u64, limit: usize) -> Filter {
    Filter::new()
        .kinds([
            Kind::Custom(KIND_DATE_CALENDAR_EVENT),
            Kind::Custom(KIND_TIME_CALENDAR_EVENT),
        ])
        .since(Timestamp::from(since))
        .limit(limit)
}
/// Build filter for meetings (spaces and rooms)
pub fn meetings_filter(limit: usize) -> Filter {
    Filter::new()
        .kinds([
            Kind::Custom(KIND_MEETING_SPACE),
            Kind::Custom(KIND_MEETING_ROOM),
        ])
        .limit(limit)
}
/// Build filter for calendar events until a timestamp (for pagination)
pub fn calendar_events_filter_until(until: u64, limit: usize) -> Filter {
    Filter::new()
        .kinds([
            Kind::Custom(KIND_DATE_CALENDAR_EVENT),
            Kind::Custom(KIND_TIME_CALENDAR_EVENT),
        ])
        .until(Timestamp::from(until))
        .limit(limit)
}
/// Build filter for meetings until a timestamp (for pagination)
pub fn meetings_filter_until(until: u64, limit: usize) -> Filter {
    Filter::new()
        .kinds([
            Kind::Custom(KIND_MEETING_SPACE),
            Kind::Custom(KIND_MEETING_ROOM),
        ])
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
/// Build filter for calendars by author
pub fn calendars_filter(pubkey: PublicKey) -> Filter {
    use crate::utils::nip52::KIND_CALENDAR;
    Filter::new()
        .kind(Kind::Custom(KIND_CALENDAR))
        .author(pubkey)
}
