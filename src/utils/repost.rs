use nostr_sdk::{Event, Kind, JsonUtil, PublicKey, Timestamp};

/// Check if an event is a repost (Kind 6 or Kind 16)
pub fn is_repost(event: &Event) -> bool {
    event.kind == Kind::Repost || event.kind == Kind::GenericRepost
}

/// Extract the original event from a repost's content field
///
/// According to NIP-18, repost events contain the stringified JSON of the
/// original event in their content field
pub fn extract_reposted_event(repost: &Event) -> Result<Event, String> {
    if !is_repost(repost) {
        return Err(format!("Event is not a repost (kind {})", repost.kind.as_u16()));
    }

    // Parse the content field as JSON using SDK's built-in method
    Event::from_json(&repost.content).map_err(|e| {
        format!("Failed to parse repost content as event JSON: {}", e)
    })
}

/// Represents a feed item that could be either an original post or a repost
#[derive(Clone, Debug)]
pub enum FeedItem {
    /// A regular post from the feed
    OriginalPost(Event),
    /// A repost with the original event and repost metadata
    Repost {
        /// The original event that was reposted
        original: Event,
        /// Public key of the user who reposted it
        reposted_by: PublicKey,
        /// Timestamp when the repost was made
        repost_timestamp: Timestamp,
    },
}

impl FeedItem {
    /// Get the underlying event (original or reposted)
    pub fn event(&self) -> &Event {
        match self {
            FeedItem::OriginalPost(event) => event,
            FeedItem::Repost { original, .. } => original,
        }
    }

    /// Get the timestamp to use for sorting (repost time for reposts, created_at for originals)
    pub fn sort_timestamp(&self) -> Timestamp {
        match self {
            FeedItem::OriginalPost(event) => event.created_at,
            FeedItem::Repost { repost_timestamp, .. } => *repost_timestamp,
        }
    }

    /// Get repost metadata if this is a repost
    pub fn repost_info(&self) -> Option<(PublicKey, Timestamp)> {
        match self {
            FeedItem::OriginalPost(_) => None,
            FeedItem::Repost { reposted_by, repost_timestamp, .. } => {
                Some((*reposted_by, *repost_timestamp))
            }
        }
    }
}

/// Expand events to include original authors from reposts for metadata prefetching.
/// For each repost, includes both the repost event and the original event so that
/// metadata for both the reposter and original author can be prefetched.
pub fn expand_events_for_prefetch(events: &[Event]) -> Vec<Event> {
    events
        .iter()
        .flat_map(|e| {
            if is_repost(e) {
                match extract_reposted_event(e) {
                    Ok(original) => vec![e.clone(), original],
                    Err(_) => vec![e.clone()],
                }
            } else {
                vec![e.clone()]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::{EventBuilder, Keys};

    fn test_keys() -> Keys {
        Keys::generate()
    }

    fn create_test_event(kind: Kind, content: &str) -> Event {
        let keys = test_keys();
        EventBuilder::new(kind, content)
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn test_is_repost_kind_6() {
        let repost = create_test_event(Kind::Repost, "");
        assert!(is_repost(&repost));
    }

    #[test]
    fn test_is_repost_kind_16() {
        let repost = create_test_event(Kind::GenericRepost, "");
        assert!(is_repost(&repost));
    }

    #[test]
    fn test_is_repost_text_note() {
        let note = create_test_event(Kind::TextNote, "Hello world");
        assert!(!is_repost(&note));
    }

    #[test]
    fn test_extract_reposted_event_success() {
        // Create an original event
        let original = create_test_event(Kind::TextNote, "Original post content");
        let original_json = original.as_json();

        // Create a repost with the original event as content
        let repost = create_test_event(Kind::Repost, &original_json);

        let extracted = extract_reposted_event(&repost).unwrap();
        assert_eq!(extracted.content, "Original post content");
        assert_eq!(extracted.kind, Kind::TextNote);
    }

    #[test]
    fn test_extract_reposted_event_not_a_repost() {
        let note = create_test_event(Kind::TextNote, "Not a repost");
        let result = extract_reposted_event(&note);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a repost"));
    }

    #[test]
    fn test_extract_reposted_event_invalid_json() {
        let repost = create_test_event(Kind::Repost, "not valid json");
        let result = extract_reposted_event(&repost);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse"));
    }

    #[test]
    fn test_feed_item_original_post() {
        let event = create_test_event(Kind::TextNote, "Hello");
        let item = FeedItem::OriginalPost(event.clone());

        assert_eq!(item.event().id, event.id);
        assert_eq!(item.sort_timestamp(), event.created_at);
        assert!(item.repost_info().is_none());
    }

    #[test]
    fn test_feed_item_repost() {
        let original = create_test_event(Kind::TextNote, "Original");
        let reposter = PublicKey::from_slice(&[2u8; 32]).unwrap();
        let repost_time = Timestamp::from(1234567890);

        let item = FeedItem::Repost {
            original: original.clone(),
            reposted_by: reposter,
            repost_timestamp: repost_time,
        };

        assert_eq!(item.event().id, original.id);
        assert_eq!(item.sort_timestamp(), repost_time);

        let (by, time) = item.repost_info().unwrap();
        assert_eq!(by, reposter);
        assert_eq!(time, repost_time);
    }

    #[test]
    fn test_expand_events_for_prefetch() {
        // Create an original event
        let original = create_test_event(Kind::TextNote, "Original post");
        let original_json = original.as_json();

        // Create a repost
        let repost = create_test_event(Kind::Repost, &original_json);

        // Create a regular note
        let note = create_test_event(Kind::TextNote, "Regular note");

        let events = vec![repost.clone(), note.clone()];
        let expanded = expand_events_for_prefetch(&events);

        // Repost expands to 2 events (repost + original), note stays as 1
        assert_eq!(expanded.len(), 3);
    }
}
