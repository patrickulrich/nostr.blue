//! Gap-proof backward pagination cursors.
//!
//! Ports the design from amethyst's `RelayLoadingCursors.kt` as a pure state
//! machine (no I/O). This replaces the brittle 1-hour gap heuristic in
//! `utils/pagination.rs::safe_cursor_from_timestamps` with a principled
//! per-relay cursor model.
//!
//! ## Design
//!
//! One `RelayLoadingCursors` instance per **scope** (e.g. one feed view),
//! holding a `Mutex<RelayCursor>` per relay. Each relay's cursor tracks:
//!
//! - `requested_until` — the `until` value carried in the last REQ for this
//!   relay; moves only in `advance()`.
//! - `reached_until` — the oldest `created_at` actually delivered by this
//!   relay; moves on EOSE. The next `advance()` starts at `reached_until - 1`.
//! - `done` — set when an empty page + EOSE arrives (gap-proof stop) OR when
//!   the relay returns events but none older than already reached (misbehaving
//!   relay guard).
//! - `stalled` — set on auth-close / cannot-connect / silence-watchdog. A
//!   stalled relay is NOT done; its subscription stays open for retry.
//!
//! ## Why `until` + `limit` instead of `since`/`until` windows
//!
//! A `since`/`until` slice that returns empty can't distinguish "nothing older
//! here" from "just a quiet gap". `until` + `limit` returns the N newest events
//! older than `until`, skipping gaps — so an **empty page + EOSE is the
//! gap-proof stop**.

use std::collections::HashMap;
use std::sync::Mutex;

use nostr_sdk::RelayUrl;

/// Default page size for backward pagination.
pub const DEFAULT_PAGE_LIMIT: usize = 50;

/// The live-tail boundary: paging starts at `now - DEFAULT_LIVE_TAIL_SECS`
/// and walks backward. Events newer than this are handled by the always-on
/// realtime subscription.
pub const DEFAULT_LIVE_TAIL_SECS: u64 = 4 * 3600;

/// Per-relay cursor state. Wrapped in `Mutex` inside `RelayLoadingCursors`
/// to provide consistent snapshots (the Kotlin original used `@Volatile`
/// fields which can tear under Rust's memory model).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RelayCursor {
    /// The `until` value sent in the last REQ for this relay.
    /// Moves only in `advance()`.
    pub requested_until: Option<u64>,
    /// The oldest `created_at` this relay has actually delivered.
    /// Moves on EOSE. The next `advance()` starts at `reached_until - 1`.
    pub reached_until: Option<u64>,
    /// Set when an empty page + EOSE arrives (gap-proof stop), or when the
    /// relay returns events but none older than already reached.
    pub done: bool,
    /// Set on auth-close / cannot-connect / silence-watchdog.
    /// A stalled relay is NOT done; its subscription stays open for retry.
    pub stalled: bool,
    /// Events seen since the last `advance()` for this relay.
    pub page_event_count: usize,
    /// Oldest event timestamp in the current page.
    pub page_oldest: Option<u64>,
}

impl RelayCursor {
    fn reset_page(&mut self) {
        self.page_event_count = 0;
        self.page_oldest = None;
    }
}

/// Per-scope cursor state. One instance per feed view (home, profile, etc.).
pub struct RelayLoadingCursors {
    inner: HashMap<RelayUrl, Mutex<RelayCursor>>,
    /// Live-tail boundary; paging won't cross below this.
    floor: u64,
}

impl RelayLoadingCursors {
    /// Create a new cursor set with the given live-tail floor.
    /// `floor` is typically `now - DEFAULT_LIVE_TAIL_SECS`.
    pub fn new(floor: u64) -> Self {
        Self {
            inner: HashMap::new(),
            floor,
        }
    }

    /// Ensure a cursor entry exists for the given relay.
    fn entry(&mut self, relay: &RelayUrl) -> &Mutex<RelayCursor> {
        self.inner
            .entry(relay.clone())
            .or_insert_with(|| Mutex::new(RelayCursor::default()))
    }

    /// Register that a relay is participating in this scope (creates cursor
    /// if not present). Called when the subscription is established.
    pub fn register_relay(&mut self, relay: RelayUrl) {
        self.entry(&relay);
    }

    /// Move the relay's `requested_until` to `reached_until - 1` (or `start`
    /// for the first page). Returns `false` if the relay is already `done`.
    ///
    /// Resets page counters so the next batch of `on_event` calls tracks the
    /// new page correctly.
    ///
    /// Note: the `floor` (live-tail boundary) is NOT enforced here — the
    /// caller is responsible for passing an appropriate `start` value on the
    /// first advance (typically `now - DEFAULT_LIVE_TAIL_SECS`). The floor is
    /// only used as a ceiling in `rewind_to()`.
    pub fn advance(&mut self, relay: &RelayUrl, start: u64) -> bool {
        let cursor_mutex = self.entry(relay);
        let mut c = cursor_mutex.lock().unwrap();
        if c.done {
            return false;
        }
        let next = match c.reached_until {
            Some(reached) => reached.saturating_sub(1),
            None => start,
        };
        c.requested_until = Some(next);
        c.reset_page();
        true
    }

    /// Track a newly-arrived event for this relay. Called per EVENT message.
    pub fn on_event(&mut self, relay: &RelayUrl, created_at: u64) {
        let cursor_mutex = self.entry(relay);
        let mut c = cursor_mutex.lock().unwrap();
        c.page_event_count += 1;
        c.page_oldest = Some(match c.page_oldest {
            Some(oldest) => oldest.min(created_at),
            None => created_at,
        });
    }

    /// Process EOSE for this relay.
    ///
    /// - **Empty page** (0 events since last advance): `done = true`.
    /// - **Non-empty page where `page_oldest >= reached_until`**: the relay
    ///   returned events but none older than already reached. This is the
    ///   misbehaving-relay guard — mark `done` to prevent infinite loops.
    /// - **Normal case**: drop `reached_until` to `page_oldest`.
    pub fn on_eose(&mut self, relay: &RelayUrl) {
        let cursor_mutex = self.entry(relay);
        let mut c = cursor_mutex.lock().unwrap();
        if c.done {
            return;
        }
        if c.page_event_count == 0 {
            c.done = true;
            return;
        }
        let new_reached = c.page_oldest;
        match (c.reached_until, new_reached) {
            (_, None) => {
                // Shouldn't happen if page_event_count > 0, but be safe.
            }
            (None, Some(oldest)) => {
                c.reached_until = Some(oldest);
            }
            (Some(prev), Some(oldest)) => {
                if oldest < prev {
                    c.reached_until = Some(oldest);
                } else {
                    // Misbehaving relay: returned events but none older than
                    // what we already had. Mark done to prevent loops.
                    c.done = true;
                }
            }
        }
    }

    /// Mark relay as stalled (CLOSED, cannot-connect, silence-watchdog).
    /// Stalled relays keep their subscription open; they're not counted as done.
    pub fn mark_stalled(&mut self, relay: &RelayUrl) {
        let cursor_mutex = self.entry(relay);
        let mut c = cursor_mutex.lock().unwrap();
        c.stalled = true;
    }

    /// Clear stalled status (e.g. relay reconnected and delivered events).
    pub fn clear_stalled(&mut self, relay: &RelayUrl) {
        let cursor_mutex = self.entry(relay);
        let mut c = cursor_mutex.lock().unwrap();
        c.stalled = false;
    }

    /// Realign cursors when the local database prunes events under memory
    /// pressure (relevant when using `WebDatabase::open_bounded`).
    ///
    /// Resets `done`, pulls `reached_until` back up to `pruned_until + 1`
    /// for relays whose reached point is below the prune boundary, and
    /// un-arms them for re-advance. Never climbs above `floor`.
    pub fn rewind_to(&mut self, pruned_until: u64) {
        let ceiling = self.floor;
        for cursor_mutex in self.inner.values() {
            let mut c = cursor_mutex.lock().unwrap();
            if let Some(reached) = c.reached_until {
                if reached < pruned_until {
                    let target = (pruned_until + 1).min(ceiling);
                    c.reached_until = Some(target);
                    c.requested_until = None;
                    c.done = false;
                    c.stalled = false;
                    c.reset_page();
                }
            }
        }
    }

    /// True if this relay is marked done (not stalled).
    pub fn is_done(&self, relay: &RelayUrl) -> bool {
        self.inner
            .get(relay)
            .map(|m| m.lock().unwrap().done)
            .unwrap_or(false)
    }

    /// True if this relay is marked stalled.
    pub fn is_stalled(&self, relay: &RelayUrl) -> bool {
        self.inner
            .get(relay)
            .map(|m| m.lock().unwrap().stalled)
            .unwrap_or(false)
    }

    /// True when every known relay is done or stalled.
    pub fn exhausted(&self) -> bool {
        if self.inner.is_empty() {
            return true;
        }
        self.inner.values().all(|m| {
            let c = m.lock().unwrap();
            c.done || c.stalled
        })
    }

    /// True when exhausted AND no relays are stalled (genuine completion).
    pub fn fully_complete(&self) -> bool {
        self.exhausted() && !self.inner.values().any(|m| m.lock().unwrap().stalled)
    }

    /// The next REQ's `until` value for this relay.
    pub fn next_until(&self, relay: &RelayUrl) -> Option<u64> {
        self.inner
            .get(relay)
            .and_then(|m| m.lock().unwrap().requested_until)
    }

    /// Aggregate `next_until` across all relays (minimum, for broadcasting).
    pub fn min_next_until(&self) -> Option<u64> {
        self.inner
            .values()
            .filter_map(|m| m.lock().unwrap().requested_until)
            .min()
    }

    /// The live-tail floor.
    pub fn floor(&self) -> u64 {
        self.floor
    }

    /// Number of registered relays.
    pub fn relay_count(&self) -> usize {
        self.inner.len()
    }

    /// Number of relays marked done.
    pub fn done_count(&self) -> usize {
        self.inner
            .values()
            .filter(|m| m.lock().unwrap().done)
            .count()
    }

    /// Number of relays marked stalled.
    pub fn stalled_count(&self) -> usize {
        self.inner
            .values()
            .filter(|m| m.lock().unwrap().stalled)
            .count()
    }

    /// Take a consistent snapshot of a relay's cursor (for testing/debugging).
    pub fn cursor_snapshot(&self, relay: &RelayUrl) -> Option<RelayCursor> {
        self.inner.get(relay).map(|m| m.lock().unwrap().clone())
    }
}

/// Backward-compatible wrapper around the original gap-detection algorithm
/// from `utils/pagination.rs`. Preserved for call sites that haven't been
/// migrated to `RelayLoadingCursors` yet.
pub fn safe_cursor_from_timestamps_compat(timestamps: &[u64]) -> Option<u64> {
    crate::utils::pagination::safe_cursor_from_timestamps(timestamps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::RelayUrl;

    fn url(s: &str) -> RelayUrl {
        RelayUrl::parse(s).unwrap()
    }

    #[test]
    fn empty_page_plus_eose_marks_done() {
        let mut cursors = RelayLoadingCursors::new(0);
        let relay = url("wss://relay.example.com");
        cursors.register_relay(relay.clone());
        assert!(cursors.advance(&relay, 10_000));
        // No events arrive, then EOSE
        cursors.on_eose(&relay);
        assert!(cursors.is_done(&relay));
        // Advancing a done relay returns false
        assert!(!cursors.advance(&relay, 10_000));
    }

    #[test]
    fn non_empty_page_advances_reached_until() {
        let mut cursors = RelayLoadingCursors::new(0);
        let relay = url("wss://relay.example.com");
        cursors.register_relay(relay.clone());
        assert!(cursors.advance(&relay, 10_000));
        // Relay returns 3 events with timestamps 9500, 9000, 8500
        cursors.on_event(&relay, 9500);
        cursors.on_event(&relay, 9000);
        cursors.on_event(&relay, 8500);
        cursors.on_eose(&relay);
        let snap = cursors.cursor_snapshot(&relay).unwrap();
        assert_eq!(snap.reached_until, Some(8500));
        assert!(!snap.done);
        // Next advance moves requested_until to reached - 1
        assert!(cursors.advance(&relay, 10_000));
        let snap = cursors.cursor_snapshot(&relay).unwrap();
        assert_eq!(snap.requested_until, Some(8499));
    }

    #[test]
    fn misbehaving_relay_marked_done() {
        // Relay returns events but none older than already reached.
        let mut cursors = RelayLoadingCursors::new(0);
        let relay = url("wss://bad.example.com");
        cursors.register_relay(relay.clone());
        assert!(cursors.advance(&relay, 10_000));
        cursors.on_event(&relay, 9000);
        cursors.on_eose(&relay);
        // reached_until is now 9000
        // Next page: relay returns same events (9000, 9100) — none < 9000
        cursors.advance(&relay, 10_000);
        cursors.on_event(&relay, 9100);
        cursors.on_event(&relay, 9000);
        cursors.on_eose(&relay);
        let snap = cursors.cursor_snapshot(&relay).unwrap();
        assert!(
            snap.done,
            "misbehaving relay should be marked done, got: {:?}",
            snap
        );
    }

    #[test]
    fn first_page_uses_start() {
        let mut cursors = RelayLoadingCursors::new(0);
        let relay = url("wss://relay.example.com");
        cursors.register_relay(relay.clone());
        assert!(cursors.advance(&relay, 50_000));
        let snap = cursors.cursor_snapshot(&relay).unwrap();
        assert_eq!(snap.requested_until, Some(50_000));
    }

    #[test]
    fn floor_used_as_ceiling_in_rewind() {
        // Floor is used as a ceiling in rewind_to: reached_until can't climb
        // above floor. Verify with floor=5000 and a prune boundary of 3000.
        let mut cursors = RelayLoadingCursors::new(5_000);
        let relay = url("wss://relay.example.com");
        cursors.register_relay(relay.clone());
        cursors.advance(&relay, 10_000);
        cursors.on_event(&relay, 1_000);
        cursors.on_eose(&relay);
        // reached_until = 1000
        cursors.rewind_to(3_000);
        let snap = cursors.cursor_snapshot(&relay).unwrap();
        // rewind_to(3000) should set reached_until = min(3001, floor=5000) = 3001
        assert_eq!(snap.reached_until, Some(3_001));
        assert!(!snap.done);
    }

    #[test]
    fn rewind_resets_done_and_pulls_reached() {
        let mut cursors = RelayLoadingCursors::new(100_000);
        let relay = url("wss://relay.example.com");
        cursors.register_relay(relay.clone());
        cursors.advance(&relay, 100_000);
        cursors.on_event(&relay, 1_000);
        cursors.on_eose(&relay);
        // reached_until is 1000, done may be false
        // Now simulate DB prune at 5000: rewind pulls reached back up.
        cursors.rewind_to(5_000);
        let snap = cursors.cursor_snapshot(&relay).unwrap();
        assert!(!snap.done);
        assert_eq!(snap.reached_until, Some(5_001));
        assert_eq!(snap.requested_until, None);
    }

    #[test]
    fn exhausted_requires_all_done_or_stalled() {
        let mut cursors = RelayLoadingCursors::new(0);
        let r1 = url("wss://r1.example.com");
        let r2 = url("wss://r2.example.com");
        cursors.register_relay(r1.clone());
        cursors.register_relay(r2.clone());
        // Not exhausted: neither is done or stalled
        assert!(!cursors.exhausted());
        // Mark r1 done
        cursors.advance(&r1, 1000);
        cursors.on_eose(&r1); // empty page → done
        assert!(!cursors.exhausted());
        // Mark r2 stalled
        cursors.mark_stalled(&r2);
        assert!(cursors.exhausted());
        assert!(!cursors.fully_complete(), "stalled means not fully complete");
    }

    #[test]
    fn fully_complete_distinguishes_done_from_stalled() {
        let mut cursors = RelayLoadingCursors::new(0);
        let r1 = url("wss://r1.example.com");
        let r2 = url("wss://r2.example.com");
        cursors.register_relay(r1.clone());
        cursors.register_relay(r2.clone());
        cursors.advance(&r1, 1000);
        cursors.on_eose(&r1); // done
        cursors.advance(&r2, 1000);
        cursors.on_eose(&r2); // done
        assert!(cursors.exhausted());
        assert!(cursors.fully_complete());
    }

    #[test]
    fn on_event_updates_page_oldest() {
        let mut cursors = RelayLoadingCursors::new(0);
        let relay = url("wss://relay.example.com");
        cursors.register_relay(relay.clone());
        cursors.advance(&relay, 10_000);
        cursors.on_event(&relay, 9_000);
        cursors.on_event(&relay, 7_000);
        cursors.on_event(&relay, 8_000);
        let snap = cursors.cursor_snapshot(&relay).unwrap();
        assert_eq!(snap.page_event_count, 3);
        assert_eq!(snap.page_oldest, Some(7_000));
    }

    #[test]
    fn multi_relay_convergence() {
        let mut cursors = RelayLoadingCursors::new(0);
        let r1 = url("wss://r1.example.com");
        let r2 = url("wss://r2.example.com");
        cursors.register_relay(r1.clone());
        cursors.register_relay(r2.clone());
        cursors.advance(&r1, 10_000);
        cursors.advance(&r2, 10_000);
        cursors.on_event(&r1, 8_000);
        cursors.on_event(&r2, 6_000);
        cursors.on_eose(&r1);
        cursors.on_eose(&r2);
        assert_eq!(cursors.cursor_snapshot(&r1).unwrap().reached_until, Some(8_000));
        assert_eq!(cursors.cursor_snapshot(&r2).unwrap().reached_until, Some(6_000));
    }

    #[test]
    fn min_next_until_aggregates() {
        let mut cursors = RelayLoadingCursors::new(0);
        let r1 = url("wss://r1.example.com");
        let r2 = url("wss://r2.example.com");
        cursors.register_relay(r1.clone());
        cursors.register_relay(r2.clone());
        cursors.advance(&r1, 10_000);
        cursors.advance(&r2, 5_000);
        // r1: requested=10000, r2: requested=5000
        assert_eq!(cursors.min_next_until(), Some(5_000));
    }

    #[test]
    fn clear_stalled() {
        let mut cursors = RelayLoadingCursors::new(0);
        let relay = url("wss://relay.example.com");
        cursors.register_relay(relay.clone());
        cursors.mark_stalled(&relay);
        assert!(cursors.is_stalled(&relay));
        cursors.clear_stalled(&relay);
        assert!(!cursors.is_stalled(&relay));
    }
}
