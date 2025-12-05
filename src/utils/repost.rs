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

    #[test]
    fn test_is_repost() {
        // These tests would require creating mock Event objects
        // Left as a framework for future testing
    }
}
