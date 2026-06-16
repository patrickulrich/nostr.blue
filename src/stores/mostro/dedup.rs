use dioxus::prelude::*;
use lru::LruCache;
use nostr::EventId;
use std::num::NonZeroUsize;

/// Maximum number of event IDs to track in the dedup LRU.
///
/// Sized to comfortably cover a 3-day backfill window (the maximum gift-wrap
/// envelope randomization range per NIP-59 — see
/// `nostr::nips::nip59::RANGE_RANDOM_TIMESTAMP_TWEAK = 0..172800`). A typical
/// user has <50 active trades, each receiving a few events per day; 5000
/// entries gives ample headroom for high-traffic dispute-chat sessions and
/// repeated app restarts that re-fetch the 3-day window.
const MAX_ENTRIES: usize = 5000;

pub static SEEN_EVENTS: GlobalSignal<LruCache<EventId, ()>> =
    Signal::global(|| LruCache::new(NonZeroUsize::new(MAX_ENTRIES).unwrap()));

pub fn is_seen(id: &EventId) -> bool {
    SEEN_EVENTS.read().peek(id).is_some()
}

pub fn mark_seen(id: EventId) {
    SEEN_EVENTS.write().put(id, ());
}
