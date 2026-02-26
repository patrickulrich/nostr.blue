//! Reviews Service
//!
//! Handles fetching persisted PR reviews (Kind 9807) from Nostr relays.
use nostr_sdk::prelude::*;
use std::time::Duration;
use crate::stores::content::code_store::cache_persisted_review_events;
use crate::stores::nostr_client::fetch_events_aggregated;
use crate::utils::nip34::PersistedReview;

const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Fetch persisted reviews for a PR by hex event ID
pub async fn fetch_pr_reviews(pr_event_id_hex: &str) -> Result<Vec<PersistedReview>, String> {
    let event_id = EventId::from_hex(pr_event_id_hex)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(PersistedReview::KIND))
        .event(event_id);
    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch reviews: {}", e))?;
    cache_persisted_review_events(&events);
    Ok(events.iter().filter_map(PersistedReview::from_event).collect())
}
