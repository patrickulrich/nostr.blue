use nostr_sdk::nips::nip53::LiveEventStatus;
use nostr_sdk::Timestamp;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum StreamStatus {
    Planned,
    Live,
    Ended,
}

impl StreamStatus {
    /// Get effective status accounting for stale "live" streams.
    ///
    /// Per NIP-53: Clients MAY choose to consider `status=live` events after 1hr
    /// without any update as `ended`.
    ///
    /// This function checks if a "live" stream hasn't been updated in over an hour
    /// and returns `Ended` in that case.
    pub fn effective_status(status: Self, last_updated: Timestamp) -> Self {
        if status == StreamStatus::Live {
            let now = Timestamp::now();
            let age_secs = now.as_secs().saturating_sub(last_updated.as_secs());

            // NIP-53: consider live events stale after 1 hour without update
            if age_secs > 3600 {
                return StreamStatus::Ended;
            }
        }
        status
    }
}

impl From<&LiveEventStatus> for StreamStatus {
    fn from(status: &LiveEventStatus) -> Self {
        match status {
            LiveEventStatus::Live => StreamStatus::Live,
            LiveEventStatus::Ended => StreamStatus::Ended,
            LiveEventStatus::Planned => StreamStatus::Planned,
            LiveEventStatus::Custom(_) => StreamStatus::Planned,
        }
    }
}
