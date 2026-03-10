//! Event ID Hook
//!
//! Reusable hook for fetching Nostr events by ID (nevent, note, hex) with proper state management.
//! Uses `use_resource` for automatic cancellation when the id changes.
use crate::stores::nostr_client;
use dioxus::prelude::*;
use nostr_sdk::nips::nip19::Nip19;
use nostr_sdk::{Event as NostrEvent, EventId, Filter, FromBech32};
use std::time::Duration;
/// Wrapper struct for ID-based event fetching with custom not-found message.
///
/// Uses `Resource` internally for automatic cancellation of inflight fetches.
#[derive(Clone)]
pub struct UseFetchEventById {
    resource: Resource<Result<Option<NostrEvent>, String>>,
    not_found_message: &'static str,
}
impl UseFetchEventById {
    /// Returns true if the fetch is still pending
    pub fn is_loading(&self) -> bool {
        matches!(self.resource.state().cloned(), UseResourceState::Pending)
    }
    /// Returns the error message if there was an error, or the not-found message if event is None
    pub fn error(&self) -> Option<String> {
        match &*self.resource.read() {
            Some(Ok(None)) => Some(self.not_found_message.to_string()),
            Some(Err(e)) => Some(e.clone()),
            _ => None,
        }
    }
    /// Returns the fetched event if available
    pub fn event(&self) -> Option<NostrEvent> {
        match &*self.resource.read() {
            Some(Ok(Some(ev))) => Some(ev.clone()),
            _ => None,
        }
    }
}
/// Parse an event ID from various formats (nevent, note, hex)
fn parse_event_id(id: &str) -> Option<EventId> {
    let trimmed = id.trim();
    let normalized = trimmed
        .strip_prefix("nostr:")
        .or_else(|| trimmed.strip_prefix("NOSTR:"))
        .unwrap_or(trimmed);
    if let Ok(event_id) = EventId::from_hex(normalized) {
        return Some(event_id);
    }
    Nip19::from_bech32(normalized).ok().and_then(|n| match n {
        Nip19::Event(e) => Some(e.event_id),
        Nip19::EventId(id) => Some(id),
        _ => None,
    })
}
/// Async helper to fetch event by ID with optional kind validation
async fn fetch_event_by_id_inner(
    id: &str,
    valid_kinds: &[u16],
) -> Result<Option<NostrEvent>, String> {
    let event_id = match parse_event_id(id) {
        Some(eid) => eid,
        None => return Err("Invalid event ID".to_string()),
    };
    let filter = Filter::new().id(event_id).limit(1);
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(e) = events.into_iter().next() {
                if valid_kinds.is_empty() || valid_kinds.contains(&e.kind.as_u16()) {
                    Ok(Some(e))
                } else {
                    Ok(None)
                }
            } else {
                Ok(None)
            }
        }
        Err(e) => Err(e),
    }
}
/// Fetch a Nostr event by ID with kind validation and a custom not-found message.
///
/// Returns a wrapper struct with helper methods for accessing state.
/// Automatically cancels inflight fetches when id changes.
///
/// # Arguments
/// * `id` - The event ID (nevent, note, or hex)
/// * `valid_kinds` - Static slice of valid event kinds. If empty, accepts any kind.
/// * `not_found_message` - Message to display when event is not found
///
/// # Example
/// ```rust
/// let fetch = use_fetch_event_by_id(id, &[21, 22], "Video not found");
/// if fetch.is_loading() {
///     return rsx! { LoadingSkeleton {} };
/// }
/// if let Some(err) = fetch.error() {
///     return rsx! { div { "{err}" } };
/// }
/// if let Some(ev) = fetch.event() {
///     return rsx! { VideoCard { event: ev } };
/// }
/// ```
pub fn use_fetch_event_by_id(
    id: String,
    valid_kinds: &'static [u16],
    not_found_message: &'static str,
) -> UseFetchEventById {
    let mut id_signal = use_signal(|| id.clone());
    use_effect(use_reactive!(|id| {
        id_signal.set(id);
    }));
    let resource =
        use_resource(
            move || async move { fetch_event_by_id_inner(&id_signal(), valid_kinds).await },
        );
    UseFetchEventById {
        resource,
        not_found_message,
    }
}
/// Fetch a Nostr event by its ID reactively with kind validation (Resource API).
///
/// Parses nevent/note/hex IDs and fetches via `fetch_events_aggregated`.
/// Validates kinds against the provided list. Automatically cancels inflight fetches.
///
/// # Arguments
/// * `id` - Signal containing the event ID (nevent, note, or hex)
/// * `valid_kinds` - Static slice of valid event kinds. If empty, accepts any kind.
///
/// # Returns
/// A `Resource` containing `Result<Option<NostrEvent>, String>`
///
/// # Example
/// ```rust
/// // Fetch video events (kind 21 or 22)
/// let resource = use_fetch_event_by_id_resource(id.into(), &[21, 22]);
///
/// match resource.state().cloned() {
///     UseResourceState::Pending => rsx! { LoadingSkeleton {} },
///     UseResourceState::Ready => {
///         match &*resource.read() {
///             Some(Ok(Some(ev))) => rsx! { VideoCard { event: ev.clone() } },
///             Some(Ok(None)) => rsx! { "Video not found" },
///             Some(Err(e)) => rsx! { "Error: {e}" },
///             None => rsx! {},
///         }
///     }
///     _ => rsx! {},
/// }
/// ```
#[allow(dead_code)]
pub fn use_fetch_event_by_id_resource(
    id: ReadSignal<String>,
    valid_kinds: &'static [u16],
) -> Resource<Result<Option<NostrEvent>, String>> {
    use_resource(move || async move { fetch_event_by_id_inner(&id(), valid_kinds).await })
}
/// Fetch a Nostr event by its ID reactively without kind validation.
///
/// Simplified version that accepts any event kind.
#[allow(dead_code)]
pub fn use_fetch_event_by_id_any_kind(
    id: ReadSignal<String>,
) -> Resource<Result<Option<NostrEvent>, String>> {
    use_fetch_event_by_id_resource(id, &[])
}
