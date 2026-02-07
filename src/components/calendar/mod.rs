//! Calendar components (NIP-52)
//!
//! This module contains components for Nostr calendar events
//! including calendar views, event cards, event maps, and mini calendars.
pub mod event_card;
pub mod event_map;
pub mod mini;
pub mod view;
#[allow(unused_imports)]
pub use event_card::{EventCard, EventCardCompact, EventCardCompactSkeleton, EventCardSkeleton};
pub use event_map::EventMap;
pub use mini::MiniCalendar;
pub use view::{CalendarView, CalendarViewMode, CalendarViewSkeleton};
