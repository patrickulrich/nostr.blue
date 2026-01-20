//! Event Coordinate Hook
//!
//! Reusable hook for fetching Nostr events by coordinate (naddr) with proper state management.
//! Uses `use_resource` for automatic cancellation when the naddr changes.

use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, FromBech32};
use nostr_sdk::nips::nip19::Nip19;

use crate::stores::nostr_client;

/// Wrapper struct for coordinate-based event fetching with custom not-found message.
///
/// Uses `Resource` internally for automatic cancellation of inflight fetches.
#[derive(Clone)]
pub struct UseFetchEventByCoordinate {
    resource: Resource<Result<Option<NostrEvent>, String>>,
    not_found_message: &'static str,
}

impl UseFetchEventByCoordinate {
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

/// Fetch a Nostr event by coordinate with a custom not-found message.
///
/// Returns a wrapper struct with helper methods for accessing state.
/// Automatically cancels inflight fetches when naddr changes.
///
/// # Arguments
/// * `naddr` - The naddr bech32 string to fetch
/// * `not_found_message` - Message to display when event is not found
///
/// # Example
/// ```rust
/// let fetch = use_fetch_event_by_coordinate_with_message(id, "Livestream not found");
/// if fetch.is_loading() {
///     return rsx! { LoadingSkeleton {} };
/// }
/// if let Some(err) = fetch.error() {
///     return rsx! { div { "{err}" } };
/// }
/// if let Some(ev) = fetch.event() {
///     return rsx! { MyCard { event: ev } };
/// }
/// ```
pub fn use_fetch_event_by_coordinate_with_message(
    naddr: String,
    not_found_message: &'static str,
) -> UseFetchEventByCoordinate {
    // Create signal to track the prop
    let mut naddr_signal = use_signal(|| naddr.clone());

    // Sync prop changes to the signal using use_reactive! (Dioxus best practice)
    use_effect(use_reactive!(|naddr| {
        naddr_signal.set(naddr);
    }));

    // use_resource auto-tracks naddr_signal reads and re-runs when it changes
    let resource = use_resource(move || async move { fetch_event_by_coordinate(&naddr_signal()).await });
    UseFetchEventByCoordinate {
        resource,
        not_found_message,
    }
}

/// Fetch a Nostr event by its coordinate (naddr) reactively.
///
/// Parses the bech32 naddr, extracts relay hints, and fetches the event.
/// Automatically cancels inflight fetches when naddr changes.
///
/// # Arguments
/// * `naddr` - Signal containing the naddr bech32 string to fetch
///
/// # Returns
/// A `Resource` containing `Result<Option<NostrEvent>, String>`
///
/// # Example
/// ```rust
/// let resource = use_fetch_event_by_coordinate(naddr.clone());
///
/// match resource.state().cloned() {
///     UseResourceState::Pending => rsx! { LoadingSkeleton {} },
///     UseResourceState::Ready => {
///         match &*resource.read() {
///             Some(Ok(Some(ev))) => rsx! { MyCard { event: ev.clone() } },
///             Some(Ok(None)) => rsx! { "Not found" },
///             Some(Err(e)) => rsx! { "Error: {e}" },
///             None => rsx! {},
///         }
///     }
///     _ => rsx! {},
/// }
/// ```
#[allow(dead_code)]
pub fn use_fetch_event_by_coordinate(
    naddr: ReadSignal<String>,
) -> Resource<Result<Option<NostrEvent>, String>> {
    use_resource(move || async move { fetch_event_by_coordinate(&naddr()).await })
}

/// Async helper to fetch event by coordinate
async fn fetch_event_by_coordinate(naddr: &str) -> Result<Option<NostrEvent>, String> {
    // Normalize: trim whitespace, strip nostr: prefix (NIP-21)
    // Note: bech32 handles case internally
    let trimmed = naddr.trim();
    let normalized = trimmed
        .strip_prefix("nostr:")
        .or_else(|| trimmed.strip_prefix("NOSTR:"))
        .unwrap_or(trimmed);

    match Nip19::from_bech32(normalized) {
        Ok(Nip19::Coordinate(coord)) => {
            let relay_hints: Vec<String> = coord.relays.iter().map(|r| r.to_string()).collect();

            nostr_client::fetch_event_by_coordinate_with_relays(
                coord.kind.as_u16(),
                coord.public_key.to_hex(),
                coord.identifier.clone(),
                relay_hints,
            )
            .await
            .map_err(|e| e.to_string())
        }
        Ok(_) => Err("Invalid coordinate address format".to_string()),
        Err(e) => Err(format!("Failed to parse address: {}", e)),
    }
}
