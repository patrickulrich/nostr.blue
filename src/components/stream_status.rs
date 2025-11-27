use nostr_sdk::nips::nip53::LiveEventStatus;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum StreamStatus {
    Planned,
    Live,
    Ended,
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
