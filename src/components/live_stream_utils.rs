//! Shared utilities for NIP-53 live streaming events

use nostr_sdk::Event as NostrEvent;
use nostr_sdk::nips::nip53::LiveEvent;

/// Parse a NIP-53 Kind 30311 live streaming event into a LiveEvent struct.
/// This is the canonical parser used by all live stream components.
///
/// Returns `Some(LiveEvent)` on successful parse, `None` on failure (with warning logged).
pub fn parse_nip53_live_event(event: &NostrEvent) -> Option<LiveEvent> {
    match LiveEvent::try_from(event.tags.clone().to_vec()) {
        Ok(le) => Some(le),
        Err(e) => {
            log::warn!("Failed to parse LiveEvent from tags: {}", e);
            None
        }
    }
}
