//! Incremental feed-list view: an append-optimized, dedup-aware ordered
//! list of [`FeedItem`]s (plan wave B1).
//!
//! Replaces the previous merge pattern (clone the whole feed, extend,
//! re-sort O(n log n), write the signal unconditionally) with:
//!
//! - **Incremental insert** — [`FeedList::insert_batch`] filters the batch
//!   through a seen-id set and merge-inserts the survivors into the sorted
//!   list in O(n + m), no re-sort.
//! - **Change suppression** — `insert_batch` returns the number of items
//!   that actually landed. Dioxus signals have no equality short-circuit
//!   (every write re-runs every subscriber), so callers skip the signal
//!   write entirely when nothing landed: `if landed == 0 { return; }`.
//!   This is the Amethyst `equalImmutableLists` lesson translated to
//!   dioxus-signals semantics.
//! - **Bounded size** — [`FeedList::trim_oldest`] drops the oldest tail
//!   beyond a cap (used when accepting prepended posts, where the user is
//!   by definition at the top of the list; never during pagination, where
//!   the tail is exactly what the user is reading).
//!
//! Dedup keys on the *visible* event id (`FeedItem::event().id`), matching
//! the historical merge semantics: a repost of a note already in the feed
//! deduplicates against the original.
//!
//! This module is pure and I/O-free (Phase-1 style): unit-testable without
//! a client, relay pool, or database.

use crate::feeds::types::FeedItem;
use nostr::event::EventId;
use std::collections::HashSet;

/// Default live-item cap for feed surfaces (Amethyst trims to 500; we keep
/// headroom because our pagination model refetches instead of rescanning a
/// local store).
pub const DEFAULT_FEED_CAP: usize = 1000;

/// An ordered, deduplicated view of feed items.
#[derive(Debug, Clone, Default)]
pub struct FeedList {
    /// Items sorted newest-first by `sort_timestamp`.
    items: Vec<FeedItem>,
    /// Visible-event ids already in `items` (repost-aware: keyed by the
    /// original event's id, so a repost dedups against its original).
    seen: HashSet<EventId>,
}

impl FeedList {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build from an unsorted/duplicate-carrying batch (initial load,
    /// cache restore). Later duplicates lose to earlier entries; the
    /// result is sorted newest-first.
    pub fn from_items(items: Vec<FeedItem>) -> Self {
        let mut list = Self::new();
        list.insert_batch(items);
        list
    }

    /// Build from a batch already sorted newest-first and deduplicated
    /// (e.g. a feed signal's current contents). Skips the sort; only
    /// builds the seen-id index.
    pub fn from_sorted_items(items: Vec<FeedItem>) -> Self {
        let seen = items.iter().map(|item| item.event().id).collect();
        Self { items, seen }
    }

    /// Consume the list, returning the owned items.
    pub fn into_items(self) -> Vec<FeedItem> {
        self.items
    }

    /// The ordered items, newest first.
    pub fn items(&self) -> &[FeedItem] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The gap-proof backward pagination cursor for the current contents
    /// (walks sorted timestamps downward, stops at the first ≥1h gap).
    pub fn cursor(&self) -> Option<u64> {
        let timestamps: Vec<u64> = self
            .items
            .iter()
            .map(|item| item.sort_timestamp().as_secs())
            .collect();
        crate::utils::pagination::safe_cursor_from_timestamps(&timestamps)
    }

    /// Incrementally merge a batch into the list, returning the items that
    /// actually landed (in insertion order, newest-first after the internal
    /// sort). See [`FeedList::insert_batch`] for the semantics.
    pub fn insert_batch_returning(&mut self, batch: Vec<FeedItem>) -> Vec<FeedItem> {
        // Filter to genuinely-new items first (also dedups within the batch).
        let mut landed: Vec<FeedItem> = Vec::with_capacity(batch.len());
        let mut batch_seen = HashSet::new();
        for item in batch {
            let id = item.event().id;
            if self.seen.contains(&id) || !batch_seen.insert(id) {
                continue;
            }
            landed.push(item);
        }
        if landed.is_empty() {
            return landed;
        }
        for item in &landed {
            self.seen.insert(item.event().id);
        }
        landed.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
        // The merge consumes the landed items; clone them for the return
        // value (bounded by batch size — callers use it for prefetch/count
        // bookkeeping and would clone anyway).
        let landed_ret = landed.clone();
        // Merge-insert two newest-first sequences in O(n + m) — no re-sort
        // of the surviving prefix, no re-ordering of untouched items, and
        // (via the peekable iterators) no deep clones of moved items.
        let old = std::mem::take(&mut self.items);
        let mut old_iter = old.into_iter().peekable();
        let mut new_iter = landed.into_iter().peekable();
        let mut merged = Vec::with_capacity(old_iter.len() + new_iter.len());
        loop {
            match (old_iter.peek(), new_iter.peek()) {
                (Some(o), Some(n)) => {
                    if o.sort_timestamp() >= n.sort_timestamp() {
                        merged.push(old_iter.next().unwrap());
                    } else {
                        merged.push(new_iter.next().unwrap());
                    }
                }
                (Some(_), None) => merged.push(old_iter.next().unwrap()),
                (None, Some(_)) => merged.push(new_iter.next().unwrap()),
                (None, None) => break,
            }
        }
        self.items = merged;
        landed_ret
    }

    /// Incrementally merge a batch into the list.
    ///
    /// Returns how many items actually landed (not already present). When
    /// the return value is 0, the list is byte-for-byte unchanged and the
    /// caller should skip any downstream signal write.
    ///
    /// Survivors are merge-inserted (both sides sorted newest-first), so
    /// insertion of a page of older items costs O(n + m) — no full re-sort
    /// and no re-allocation of the surviving prefix's order.
    pub fn insert_batch(&mut self, batch: Vec<FeedItem>) -> usize {
        self.insert_batch_returning(batch).len()
    }

    /// Drop the oldest entries beyond `cap`; returns how many were trimmed.
    pub fn trim_oldest(&mut self, cap: usize) -> usize {
        if self.items.len() <= cap {
            return 0;
        }
        let trimmed = self.items.len() - cap;
        let removed: Vec<FeedItem> = self.items.drain(cap..).collect();
        for item in &removed {
            self.seen.remove(&item.event().id);
        }
        trimmed
    }

    /// Oldest item's sort timestamp (the pre-trim pagination `until`).
    pub fn oldest_timestamp(&self) -> Option<u64> {
        self.items.last().map(|item| item.sort_timestamp().as_secs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::event::{EventBuilder, FinalizeEvent, Kind};
    use nostr_sdk::prelude::{Keys, Timestamp};

    fn post(secs: u64, content: &str) -> FeedItem {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, content)
            .custom_created_at(Timestamp::from(secs))
            .finalize(&keys)
            .expect("signed test note");
        FeedItem::OriginalPost(event)
    }

    fn repost_at(secs: u64, original: &FeedItem) -> FeedItem {
        let keys = Keys::generate();
        let event = match original {
            FeedItem::OriginalPost(e) => e,
            _ => panic!("test helper: expected OriginalPost"),
        };
        let repost = EventBuilder::new(Kind::Repost, event.as_json())
            .custom_created_at(Timestamp::from(secs))
            .finalize(&keys)
            .expect("signed repost");
        FeedItem::Repost {
            original: event.clone(),
            reposted_by: repost.pubkey,
            repost_timestamp: repost.created_at,
        }
    }

    fn stamps(list: &FeedList) -> Vec<u64> {
        list.items()
            .iter()
            .map(|i| i.sort_timestamp().as_secs())
            .collect()
    }

    #[test]
    fn from_items_sorts_descending_and_dedups() {
        let list = FeedList::from_items(vec![
            post(100, "old"),
            post(300, "new"),
            post(200, "mid"),
        ]);
        assert_eq!(stamps(&list), vec![300, 200, 100]);
        assert_eq!(list.len(), 3);
    }

    fn item_from(event: &nostr::event::Event) -> FeedItem {
        FeedItem::OriginalPost(event.clone())
    }

    fn event_at(secs: u64, content: &str) -> nostr::event::Event {
        let keys = Keys::generate();
        EventBuilder::new(Kind::TextNote, content)
            .custom_created_at(Timestamp::from(secs))
            .finalize(&keys)
            .expect("signed test note")
    }

    #[test]
    fn insert_batch_skips_already_seen() {
        let a = event_at(100, "a");
        let b = event_at(200, "b");
        let mut list = FeedList::from_items(vec![item_from(&a), item_from(&b)]);
        let landed = list.insert_batch(vec![item_from(&a), post(150, "new")]);
        assert_eq!(landed, 1);
        assert_eq!(stamps(&list), vec![200, 150, 100]);
    }

    #[test]
    fn insert_batch_zero_landed_leaves_list_unchanged() {
        let a = event_at(100, "a");
        let mut list = FeedList::from_items(vec![item_from(&a)]);
        let before = stamps(&list);
        let landed = list.insert_batch(vec![item_from(&a)]);
        assert_eq!(landed, 0);
        assert_eq!(stamps(&list), before);
    }

    #[test]
    fn insert_batch_dedups_repost_against_original() {
        let original = post(100, "original");
        let mut list = FeedList::from_items(vec![original.clone()]);
        // A repost of the same visible event dedups by the ORIGINAL's id.
        let landed = list.insert_batch(vec![repost_at(300, &original)]);
        assert_eq!(landed, 0);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn insert_batch_dedups_within_batch() {
        let event = match post(100, "x") {
            FeedItem::OriginalPost(e) => e,
            _ => unreachable!(),
        };
        let mut list = FeedList::new();
        let landed = list.insert_batch(vec![
            FeedItem::OriginalPost(event.clone()),
            FeedItem::OriginalPost(event),
        ]);
        assert_eq!(landed, 1);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn insert_batch_merges_older_page_in_order() {
        let mut list = FeedList::from_items(vec![post(300, "c"), post(200, "b")]);
        let landed = list.insert_batch(vec![post(250, "between"), post(150, "older")]);
        assert_eq!(landed, 2);
        assert_eq!(stamps(&list), vec![300, 250, 200, 150]);
    }

    #[test]
    fn cursor_walks_past_gap() {
        // 300, 200 with a 2h gap between them: cursor is 199.
        let list = FeedList::from_items(vec![post(300, "a"), post(200, "b")]);
        assert_eq!(list.cursor(), Some(199));
    }

    #[test]
    fn trim_oldest_drops_tail_and_forgets_ids() {
        let mut list = FeedList::from_items(vec![
            post(400, "a"),
            post(300, "b"),
            post(200, "c"),
            post(100, "d"),
        ]);
        let trimmed = list.trim_oldest(2);
        assert_eq!(trimmed, 2);
        assert_eq!(stamps(&list), vec![400, 300]);
        // Trimmed ids are forgotten: re-inserting one lands again.
        let landed = list.insert_batch(vec![post(100, "d")]);
        assert_eq!(landed, 1);
    }

    #[test]
    fn oldest_timestamp_tracks_tail() {
        let list = FeedList::from_items(vec![post(400, "a"), post(300, "b")]);
        assert_eq!(list.oldest_timestamp(), Some(300));
        assert_eq!(FeedList::new().oldest_timestamp(), None);
    }
}
