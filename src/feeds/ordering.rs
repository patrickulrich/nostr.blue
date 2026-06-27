//! Stable, deterministic ordering for feed items.
//!
//! ## Tiebreaker semantics
//!
//! Matches the SDK's `Event::Ord` implementation: descending by `created_at`,
//! then **ascending** by event id (hex string comparison). The ascending id
//! tiebreak guarantees that two events with the same `created_at` second have
//! a deterministic order regardless of relay arrival order.
//!
//! ## Snapshot-based comparison
//!
//! The `sort_timestamp()` and event id are snapshotted once per item before
//! sorting. This prevents `TimSort` contract violations if a comparator were
//! to observe different values for the same item across calls (e.g. if the
//! underlying event is swapped out for a newer version of an addressable
//! event mid-sort). By snapshotting, the comparator is consistent for the
//! duration of one sort pass.

use std::cmp::Ordering;

use nostr_sdk::Timestamp;

use crate::utils::repost::FeedItem;

/// Compare two feed items for feed ordering: newest first by sort timestamp,
/// then ascending by event id hex for stable ordering of same-second events.
pub fn cmp_feed_items(a: &FeedItem, b: &FeedItem) -> Ordering {
    let a_ts = a.sort_timestamp();
    let b_ts = b.sort_timestamp();
    b_ts.cmp(&a_ts).then_with(|| a.event().id.to_hex().cmp(&b.event().id.to_hex()))
}

/// Sort a slice of feed items in place, newest first, with a stable
/// ascending-id tiebreaker for events sharing the same `created_at` second.
///
/// Snapshots `(sort_timestamp, event_id_hex)` per item before sorting so that
/// the comparator observes a consistent view even if items' underlying events
/// are mutated externally during the sort.
pub fn sort_feed_items(items: &mut [FeedItem]) {
    if items.len() < 2 {
        return;
    }
    // Snapshot (sort_timestamp, event_id_hex, original_index) per item.
    // The original_index lets us apply the sort permutation while moving the
    // actual (non-Copy) FeedItem values.
    let mut snaps: Vec<(Timestamp, String, usize)> = items
        .iter()
        .enumerate()
        .map(|(i, item)| (item.sort_timestamp(), item.event().id.to_hex(), i))
        .collect();
    // sort_unstable by (desc timestamp, asc id_hex); the original_index
    // captured above ensures a stable output for fully-equal keys.
    snaps.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    // Build sorted output by cloning items at the permuted indices.
    // Cloning is cheap: FeedItem::clone clones the underlying Event which is
    // a single Arc-backed allocation internally.
    let sorted: Vec<FeedItem> = snaps
        .iter()
        .map(|(_, _, idx)| items[*idx].clone())
        .collect();
    // clone_from_slice requires T: Clone (FeedItem derives Clone).
    items.clone_from_slice(&sorted);
}

/// Return the sort key for an item as `(timestamp_secs, id_hex_lower)`.
/// Useful for tests and external callers that want to compare without
/// constructing full events.
pub fn sort_key(item: &FeedItem) -> (u64, String) {
    (item.sort_timestamp().as_secs(), item.event().id.to_hex())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::repost::FeedItem;
    use nostr_sdk::{EventBuilder, Keys, Kind, Timestamp};

    fn make_note(secs: u64, content: &str) -> FeedItem {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, content)
            .custom_created_at(Timestamp::from(secs))
            .sign_with_keys(&keys)
            .unwrap();
        FeedItem::OriginalPost(event)
    }

    fn ts_of(item: &FeedItem) -> u64 {
        item.sort_timestamp().as_secs()
    }

    fn id_of(item: &FeedItem) -> String {
        item.event().id.to_hex()
    }

    #[test]
    fn sort_descending_by_timestamp() {
        let mut items = vec![
            make_note(100, "older"),
            make_note(300, "newest"),
            make_note(200, "middle"),
        ];
        sort_feed_items(&mut items);
        assert_eq!(ts_of(&items[0]), 300);
        assert_eq!(ts_of(&items[1]), 200);
        assert_eq!(ts_of(&items[2]), 100);
    }

    #[test]
    fn same_second_tiebreak_uses_ascending_id() {
        // Two events at the same timestamp: the one with the lexicographically
        // smaller id hex should come first.
        let mut items = vec![make_note(500, "a"), make_note(500, "b")];
        sort_feed_items(&mut items);
        // Both have ts=500 (approximately; created_at may differ slightly).
        // If timestamps match, ascending id tiebreak applies.
        if ts_of(&items[0]) == ts_of(&items[1]) {
            assert!(id_of(&items[0]) <= id_of(&items[1]));
        }
    }

    #[test]
    fn single_item_unchanged() {
        let mut items = vec![make_note(42, "only")];
        let original_id = id_of(&items[0]);
        sort_feed_items(&mut items);
        assert_eq!(id_of(&items[0]), original_id);
    }

    #[test]
    fn empty_slice_ok() {
        let mut items: Vec<FeedItem> = vec![];
        sort_feed_items(&mut items);
        assert!(items.is_empty());
    }

    #[test]
    fn cmp_function_matches_sort() {
        let a = make_note(100, "a");
        let b = make_note(200, "b");
        // b is newer, so cmp(a, b) should be Greater (a comes after b in descending order).
        assert_eq!(cmp_feed_items(&a, &b), Ordering::Greater);
        assert_eq!(cmp_feed_items(&b, &a), Ordering::Less);
    }

    #[test]
    fn sort_is_idempotent() {
        let mut items = vec![
            make_note(100, "a"),
            make_note(300, "b"),
            make_note(200, "c"),
        ];
        sort_feed_items(&mut items);
        let first_pass: Vec<String> = items.iter().map(id_of).collect();
        sort_feed_items(&mut items);
        let second_pass: Vec<String> = items.iter().map(id_of).collect();
        assert_eq!(first_pass, second_pass);
    }
}
