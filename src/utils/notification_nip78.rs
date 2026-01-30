//! NIP-78: Application Data Storage for Notification Tracking
//!
//! This module provides functions to create and parse NIP-78 events
//! for syncing notification read status across devices.

use nostr_sdk::{Event, EventBuilder, Kind, Tag, Timestamp};

/// NIP-78 kind for arbitrary custom app data
const APP_DATA_KIND: u16 = 30078;

/// D tag identifier for notification checked_at timestamp
const NOTIFICATION_CHECKED_AT_D_TAG: &str = "notifications_checked_at";

/// Create a NIP-78 event for notification checked_at timestamp
///
/// The timestamp is encoded in the event's created_at field.
/// This allows for simple syncing: the newest event wins.
///
/// # Arguments
/// * `timestamp` - Unix timestamp in seconds when notifications were last checked
///
/// # Returns
/// EventBuilder that can be published to relays
pub fn create_checked_at_event(timestamp: i64) -> EventBuilder {
    let content = format!(
        "Notification read status as of {}. This event syncs when you last checked notifications across devices.",
        timestamp
    );

    EventBuilder::new(
        Kind::from(APP_DATA_KIND),
        content
    )
    .tag(Tag::identifier(NOTIFICATION_CHECKED_AT_D_TAG))
    .custom_created_at(Timestamp::from(timestamp as u64))
}

/// Extract the checked_at timestamp from a NIP-78 event
///
/// # Arguments
/// * `event` - The NIP-78 event to parse
///
/// # Returns
/// The timestamp from the event's created_at field, or None if invalid
pub fn parse_checked_at_event(event: &Event) -> Option<i64> {
    // Verify this is the correct event type
    if event.kind != Kind::from(APP_DATA_KIND) {
        return None;
    }

    // Verify the d-tag matches
    let has_correct_d_tag = event.tags.iter().any(|tag| {
        if let Some(identifier) = tag.as_standardized() {
            matches!(identifier, nostr_sdk::TagStandard::Identifier(d) if d == NOTIFICATION_CHECKED_AT_D_TAG)
        } else {
            false
        }
    });

    if !has_correct_d_tag {
        return None;
    }

    // Return the timestamp from created_at
    Some(event.created_at.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::Keys;

    fn test_keys() -> Keys {
        Keys::generate()
    }

    #[test]
    fn test_create_checked_at_event_content() {
        let timestamp = 1234567890i64;
        let builder = create_checked_at_event(timestamp);
        let keys = test_keys();

        // Build and sign event to verify properties
        let event = builder.sign_with_keys(&keys).unwrap();

        // Verify kind is 30078 (NIP-78 app data)
        assert_eq!(event.kind, Kind::from(APP_DATA_KIND));

        // Verify content includes the timestamp
        assert!(event.content.contains("1234567890"));

        // Verify d-tag is set correctly
        let has_d_tag = event.tags.iter().any(|tag| {
            if let Some(identifier) = tag.as_standardized() {
                matches!(identifier, nostr_sdk::TagStandard::Identifier(d) if d == NOTIFICATION_CHECKED_AT_D_TAG)
            } else {
                false
            }
        });
        assert!(has_d_tag, "Event should have d-tag for notifications_checked_at");

        // Verify created_at matches the timestamp
        assert_eq!(event.created_at.as_secs(), timestamp as u64);
    }

    #[test]
    fn test_roundtrip_create_and_parse() {
        let timestamp = 1700000000i64;
        let builder = create_checked_at_event(timestamp);
        let keys = test_keys();
        let event = builder.sign_with_keys(&keys).unwrap();

        // Parse should extract the same timestamp
        let parsed = parse_checked_at_event(&event);
        assert_eq!(parsed, Some(timestamp));
    }

    #[test]
    fn test_parse_invalid_kind() {
        let keys = test_keys();
        // Create an event with wrong kind
        let event = EventBuilder::new(Kind::TextNote, "test")
            .sign_with_keys(&keys)
            .unwrap();

        assert_eq!(parse_checked_at_event(&event), None);
    }

    #[test]
    fn test_parse_missing_d_tag() {
        let keys = test_keys();
        // Create event with correct kind but wrong d-tag
        let event = EventBuilder::new(Kind::from(APP_DATA_KIND), "test")
            .tag(Tag::identifier("wrong_d_tag"))
            .sign_with_keys(&keys)
            .unwrap();

        assert_eq!(parse_checked_at_event(&event), None);
    }
}
