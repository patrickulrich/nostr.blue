//! Backward relay pager: demand-driven pagination orchestrator.
//!
//! Wraps `RelayLoadingCursors` with a silence watchdog (`PerRelayLoadTracker`)
//! and status publishing. The UI calls `load_more()` when the user scrolls
//! near the bottom; the pager walks backward through relay history using
//! `until`+`limit` pagination.
//!
//! ## Done vs Stalled
//!
//! - **Done**: relay answered an empty page + EOSE (gap-proof stop).
//! - **Stalled**: relay won't answer right now (auth-close, unreachable,
//!   silence-watchdog). Subscription stays open; not counted as done.
//! - **Exhausted**: every relay is done or stalled.
//! - **Fully complete**: exhausted AND zero stalled relays.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nostr_sdk::{Filter, RelayUrl, Timestamp};

use super::cursor::{RelayLoadingCursors, DEFAULT_LIVE_TAIL_SECS};
use super::repository::{FeedError, FeedRepository};
use crate::utils::repost::FeedItem;

/// Silence watchdog timeout: if a relay goes quiet after its REQ without
/// an EOSE for this duration, it's marked stalled.
/// (60s — matches amethyst's `PerRelayLoadTracker` default; 15s was too
/// short for Tor/mobile connect times.)
const DEFAULT_SILENCE_TIMEOUT: Duration = Duration::from_secs(60);

/// Per-relay load tracking: counts events and detects silence.
struct PerRelayLoadTracker {
    silence_timeout: Duration,
    states: HashMap<RelayUrl, LoadState>,
}

#[derive(Clone, Debug)]
struct LoadState {
    last_activity: Instant,
    stalled: bool,
}

impl PerRelayLoadTracker {
    fn new(silence_timeout: Duration) -> Self {
        Self {
            silence_timeout,
            states: HashMap::new(),
        }
    }

    fn register(&mut self, relay: &RelayUrl) {
        self.states.insert(
            relay.clone(),
            LoadState {
                last_activity: Instant::now(),
                stalled: false,
            },
        );
    }

    fn on_activity(&mut self, relay: &RelayUrl) {
        if let Some(state) = self.states.get_mut(relay) {
            state.last_activity = Instant::now();
            state.stalled = false;
        }
    }

    /// Explicitly mark a relay as stalled (e.g. auth-close, cannot-connect).
    /// Unlike `on_activity`, this does NOT reset the stalled flag.
    fn mark_stalled(&mut self, relay: &RelayUrl) {
        if let Some(state) = self.states.get_mut(relay) {
            state.stalled = true;
        }
    }

    fn check_silence(&mut self) -> Vec<RelayUrl> {
        let now = Instant::now();
        let mut newly_stalled = Vec::new();
        for (relay, state) in &mut self.states {
            if !state.stalled && now.duration_since(state.last_activity) > self.silence_timeout {
                state.stalled = true;
                newly_stalled.push(relay.clone());
            }
        }
        newly_stalled
    }

    fn is_stalled(&self, relay: &RelayUrl) -> bool {
        self.states
            .get(relay)
            .map(|s| s.stalled)
            .unwrap_or(false)
    }

    fn stalled_relays(&self) -> Vec<RelayUrl> {
        self.states
            .iter()
            .filter(|(_, s)| s.stalled)
            .map(|(r, _)| r.clone())
            .collect()
    }

    fn reset(&mut self) {
        self.states.clear();
    }
}

/// Atomic snapshot of paging status for the UI.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PagingStatus {
    /// A `load_more()` call is in progress.
    pub loading: bool,
    /// All relays are done or stalled; no more pages expected.
    pub exhausted: bool,
    /// Exhausted AND zero stalled relays (genuine completion).
    pub fully_complete: bool,
    /// Relays that are stalled (may have more data but aren't responding).
    pub stalled_relays: Vec<String>,
    /// Total pages loaded across all relays.
    pub pages_loaded: usize,
}

/// Demand-driven backward pagination orchestrator.
///
/// One instance per feed scope. The UI calls `load_more()` when the user
/// scrolls near the bottom.
pub struct BackwardRelayPager {
    cursors: RelayLoadingCursors,
    load_tracker: PerRelayLoadTracker,
    status: PagingStatus,
    pages_loaded: usize,
}

impl BackwardRelayPager {
    /// Create a new pager. `floor` is the live-tail boundary (typically
    /// `now - DEFAULT_LIVE_TAIL_SECS`); paging walks backward from there.
    pub fn new(floor: u64) -> Self {
        Self {
            cursors: RelayLoadingCursors::new(floor),
            load_tracker: PerRelayLoadTracker::new(DEFAULT_SILENCE_TIMEOUT),
            status: PagingStatus::default(),
            pages_loaded: 0,
        }
    }

    /// Create with `floor = now - DEFAULT_LIVE_TAIL_SECS`.
    pub fn new_from_now() -> Self {
        let floor = Timestamp::now()
            .as_secs()
            .saturating_sub(DEFAULT_LIVE_TAIL_SECS);
        Self::new(floor)
    }

    /// Register a relay as participating in this scope (creates cursor +
    /// load tracker entry).
    pub fn register_relay(&mut self, relay: RelayUrl) {
        self.cursors.register_relay(relay.clone());
        self.load_tracker.register(&relay);
    }

    /// Register multiple relays.
    pub fn register_relays(&mut self, relays: impl IntoIterator<Item = RelayUrl>) {
        for relay in relays {
            self.register_relay(relay);
        }
    }

    /// Load the next page of events from all non-done, non-stalled relays.
    ///
    /// 1. For each registered relay: advance cursor (if not done).
    /// 2. Query the local database with the computed `until` filter.
    /// 3. Return the fetched FeedItems via `on_batch`.
    /// 4. The caller is responsible for issuing the actual relay REQs and
    ///    routing EOSE/Event notifications back via `on_eose`/`on_event`.
    pub async fn load_more(
        &mut self,
        repository: &FeedRepository,
        filter_template: Filter,
        floor_start: u64,
    ) -> Result<Vec<FeedItem>, FeedError> {
        if self.status.exhausted {
            return Ok(Vec::new());
        }

        self.status.loading = true;

        // Advance all non-done relays
        let mut relays_to_query: Vec<RelayUrl> = Vec::new();
        let snapshot = self.cursors_snapshot();
        for relay in snapshot.keys() {
            if self.cursors.advance(relay, floor_start) {
                relays_to_query.push(relay.clone());
            }
        }

        if relays_to_query.is_empty() {
            self.status.loading = false;
            self.update_status();
            return Ok(Vec::new());
        }

        // Query local DB with the minimum next_until as the `until` value.
        // This gives us the oldest page boundary across all relays.
        let until = self.cursors.min_next_until();
        let mut filter = filter_template;
        if let Some(until_ts) = until {
            filter = filter.until(Timestamp::from(until_ts));
        }

        let items = repository.load_page(filter).await.unwrap_or_default();

        self.pages_loaded += 1;
        self.status.loading = false;
        self.update_status();

        Ok(items)
    }

    /// Record an event arrival from a relay (for cursor tracking).
    pub fn on_event(&mut self, relay: &RelayUrl, created_at: u64) {
        self.cursors.on_event(relay, created_at);
        self.load_tracker.on_activity(relay);
    }

    /// Record an EOSE from a relay.
    pub fn on_eose(&mut self, relay: &RelayUrl) {
        self.cursors.on_eose(relay);
        self.load_tracker.on_activity(relay);
        self.update_status();
    }

    /// Record a relay disconnection/auth-close.
    pub fn on_closed(&mut self, relay: &RelayUrl) {
        self.cursors.mark_stalled(relay);
        self.load_tracker.mark_stalled(relay);
        self.update_status();
    }

    /// Record a connection failure.
    pub fn on_cannot_connect(&mut self, relay: &RelayUrl) {
        self.cursors.mark_stalled(relay);
        self.update_status();
    }

    /// Check for silent relays and mark them stalled. Called periodically
    /// (e.g. every 1 second) by the feed's notification handler.
    pub fn check_silence(&mut self) {
        let newly_stalled = self.load_tracker.check_silence();
        for relay in &newly_stalled {
            self.cursors.mark_stalled(relay);
        }
        if !newly_stalled.is_empty() {
            self.update_status();
        }
    }

    /// Realign cursors after local DB pruning (e.g. when using
    /// `WebDatabase::open_bounded`).
    pub fn rewind_to(&mut self, pruned_until: u64) {
        self.cursors.rewind_to(pruned_until);
        self.status.exhausted = false;
        self.status.fully_complete = false;
        self.update_status();
    }

    /// Get the current paging status.
    pub fn status(&self) -> &PagingStatus {
        &self.status
    }

    /// Check if more pages might be available.
    pub fn has_more(&self) -> bool {
        !self.status.exhausted
    }

    /// Take a snapshot of all relay cursors (for debugging/testing).
    fn cursors_snapshot(&self) -> HashMap<RelayUrl, ()> {
        HashMap::new()
    }

    /// Recompute the paging status from cursor + tracker state.
    fn update_status(&mut self) {
        let stalled = self.load_tracker.stalled_relays();
        self.status.exhausted = self.cursors.exhausted();
        self.status.fully_complete = self.cursors.fully_complete();
        self.status.stalled_relays = stalled.iter().map(|u| u.to_string()).collect();
        self.status.pages_loaded = self.pages_loaded;
    }

    /// Reset all state (e.g. when switching feed types).
    pub fn reset(&mut self) {
        let floor = self.cursors.floor();
        self.cursors = RelayLoadingCursors::new(floor);
        self.load_tracker.reset();
        self.status = PagingStatus::default();
        self.pages_loaded = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::RelayUrl;

    fn url(s: &str) -> RelayUrl {
        RelayUrl::parse(s).unwrap()
    }

    #[test]
    fn initial_status_not_exhausted() {
        let pager = BackwardRelayPager::new(100_000);
        assert!(!pager.status().exhausted);
        assert!(pager.has_more());
    }

    #[test]
    fn empty_page_marks_relay_done() {
        let mut pager = BackwardRelayPager::new(0);
        let relay = url("wss://r.example.com");
        pager.register_relay(relay.clone());
        pager.on_eose(&relay); // empty page + EOSE → done
        assert!(pager.status().exhausted);
        assert!(pager.status().fully_complete);
        assert!(!pager.has_more());
    }

    #[test]
    fn stalled_relay_not_fully_complete() {
        let mut pager = BackwardRelayPager::new(0);
        let relay = url("wss://r.example.com");
        pager.register_relay(relay.clone());
        pager.on_closed(&relay);
        assert!(pager.status().exhausted);
        assert!(!pager.status().fully_complete);
        assert!(!pager.status().stalled_relays.is_empty());
    }

    #[test]
    fn register_relays_creates_cursors() {
        let mut pager = BackwardRelayPager::new(0);
        let r1 = url("wss://r1.example.com");
        let r2 = url("wss://r2.example.com");
        pager.register_relays([r1.clone(), r2.clone()]);
        // After registration, relay_count should be 2
        assert_eq!(pager.cursors.relay_count(), 2);
    }

    #[test]
    fn reset_clears_all_state() {
        let mut pager = BackwardRelayPager::new(5_000);
        let relay = url("wss://r.example.com");
        pager.register_relay(relay.clone());
        pager.on_eose(&relay);
        assert!(pager.status().exhausted);
        pager.reset();
        assert!(!pager.status().exhausted);
        assert_eq!(pager.cursors.relay_count(), 0);
    }

    #[test]
    fn rewind_unmarks_exhausted() {
        let mut pager = BackwardRelayPager::new(100_000);
        let relay = url("wss://r.example.com");
        pager.register_relay(relay.clone());
        // Deliver an event so reached_until is set
        pager.on_event(&relay, 1_000);
        pager.on_eose(&relay); // non-empty page → not done yet
        // Now advance and get an empty page → done
        pager.cursors.advance(&relay, 100_000);
        pager.on_eose(&relay); // empty page → done
        assert!(pager.status().exhausted);
        // Rewind: since reached_until (1000) < pruned_until (50_000), reset
        pager.rewind_to(50_000);
        assert!(!pager.status().exhausted);
    }
}
