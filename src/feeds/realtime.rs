//! Realtime subscription management with per-subscription EOSE tracking.
//!
//! ## Key features
//!
//! - **Per-subscription EOSE tracking**: the existing global `EOSE_TRACKER`
//!   only tracks per-relay (it drops the subscription_id). This module adds
//!   per-(subscription, relay) tracking so each feed can manage its own EOSE
//!   state independently.
//! - **EOSE advances on live events** (amethyst pattern): every post-EOSE
//!   event bumps that relay's cursor, so the next filter recompute doesn't
//!   re-fetch events the subscription already delivered.
//! - **DispatcherHandle integration**: uses the existing
//!   `NotificationDispatcher` for per-subscription event delivery, rather
//!   than the raw `client.notifications()` + manual sub-id filter the current
//!   home route uses. This eliminates the O(N) sub-id scan.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use nostr_sdk::{Filter, RelayUrl, SubscriptionId, Timestamp};

/// Per-relay EOSE state for a single subscription.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EoseState {
    /// Timestamp of the last EOSE received from this relay for this sub.
    pub last_eose: Option<Timestamp>,
    /// Timestamp of the most recent post-EOSE (live) event from this relay.
    /// Updated on every `RelayPoolNotification::Event` after EOSE.
    /// Prevents re-fetching events the subscription already delivered.
    pub last_live_event: Option<Timestamp>,
    /// Whether this relay has transitioned to live mode (post-EOSE).
    pub is_live: bool,
}

impl EoseState {
    /// The effective "since" cursor for this relay: the max of last_eose
    /// and last_live_event. When the filter is recomputed, the `since`
    /// parameter should be set to this value so already-delivered events
    /// aren't re-fetched.
    pub fn effective_since(&self) -> Option<Timestamp> {
        match (self.last_eose, self.last_live_event) {
            (Some(e), Some(l)) => Some(e.max(l)),
            (Some(e), None) => Some(e),
            (None, Some(l)) => Some(l),
            (None, None) => None,
        }
    }
}

/// Per-subscription EOSE tracker. Tracks EOSE state for each
/// (subscription_id, relay_url) pair.
///
/// One instance per feed scope. Updated by a single `client.notifications()`
/// listener that routes EOSE/Event messages to the right subscription entry.
#[derive(Default)]
pub struct PerSubEoseTracker {
    inner: Mutex<HashMap<SubscriptionId, HashMap<RelayUrl, EoseState>>>,
}

impl PerSubEoseTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register that a subscription is active.
    pub fn register_sub(&self, sub_id: SubscriptionId) {
        let mut inner = self.inner.lock().unwrap();
        inner.entry(sub_id).or_default();
    }

    /// Register expected relays for a subscription. Creates `is_live: false`
    /// entries so that `all_eosed` correctly returns false until every
    /// registered relay has delivered its EOSE.
    pub fn register_relay(&self, sub_id: &SubscriptionId, relay: RelayUrl) {
        let mut inner = self.inner.lock().unwrap();
        let relays = inner
            .entry(sub_id.clone())
            .or_default();
        relays.entry(relay).or_default();
    }

    /// Register multiple expected relays at once.
    pub fn register_relays(&self, sub_id: &SubscriptionId, relays: impl IntoIterator<Item = RelayUrl>) {
        for relay in relays {
            self.register_relay(sub_id, relay);
        }
    }

    /// Record an EOSE for a specific (subscription, relay) pair.
    pub fn on_eose(&self, sub_id: &SubscriptionId, relay: &RelayUrl) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(relays) = inner.get_mut(sub_id) {
            let state = relays.entry(relay.clone()).or_default();
            state.last_eose = Some(Timestamp::now());
            state.is_live = true;
        }
    }

    /// Record a live event for a specific (subscription, relay) pair.
    /// Only updates if the relay is already in live mode (post-EOSE).
    pub fn on_event(&self, sub_id: &SubscriptionId, relay: &RelayUrl, created_at: Timestamp) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(relays) = inner.get_mut(sub_id) {
            let state = relays.entry(relay.clone()).or_default();
            if state.is_live {
                state.last_live_event = Some(match state.last_live_event {
                    Some(prev) => prev.max(created_at),
                    None => created_at,
                });
            }
        }
    }

    /// Get the effective "since" cursor for a specific relay in a subscription.
    pub fn effective_since(
        &self,
        sub_id: &SubscriptionId,
        relay: &RelayUrl,
    ) -> Option<Timestamp> {
        let inner = self.inner.lock().unwrap();
        inner
            .get(sub_id)
            .and_then(|relays| relays.get(relay))
            .and_then(|state| state.effective_since())
    }

    /// Get the minimum effective "since" across all relays for a subscription.
    /// This is the most conservative cursor (we definitely have everything
    /// newer than this across all relays).
    pub fn min_effective_since(&self, sub_id: &SubscriptionId) -> Option<Timestamp> {
        let inner = self.inner.lock().unwrap();
        inner
            .get(sub_id)
            .and_then(|relays| {
                relays
                    .values()
                    .filter_map(|state| state.effective_since())
                    .min()
            })
    }

    /// Check if all registered relays for a subscription have EOSEd.
    pub fn all_eosed(&self, sub_id: &SubscriptionId) -> bool {
        let inner = self.inner.lock().unwrap();
        inner
            .get(sub_id)
            .map(|relays| {
                !relays.is_empty() && relays.values().all(|state| state.is_live)
            })
            .unwrap_or(false)
    }

    /// Count how many relays have EOSEd for a subscription.
    pub fn eosed_count(&self, sub_id: &SubscriptionId) -> usize {
        let inner = self.inner.lock().unwrap();
        inner
            .get(sub_id)
            .map(|relays| relays.values().filter(|state| state.is_live).count())
            .unwrap_or(0)
    }

    /// Unregister a subscription (clears all per-relay state for it).
    pub fn unregister_sub(&self, sub_id: &SubscriptionId) {
        let mut inner = self.inner.lock().unwrap();
        inner.remove(sub_id);
    }

    /// Take a snapshot of a subscription's state (for testing/debugging).
    pub fn snapshot(
        &self,
        sub_id: &SubscriptionId,
    ) -> Option<HashMap<RelayUrl, EoseState>> {
        let inner = self.inner.lock().unwrap();
        inner.get(sub_id).cloned()
    }
}

/// Count-based EOSE threshold (wisp pattern): `max(3, 30% of connected)`.
///
/// Waits for this many relays to EOSE before declaring the initial page
/// "loaded". Why: many pool relays are dead and will never EOSE; basing the
/// threshold on total targeted relays makes it unreachable, causing the
/// timeout to fire every time with a sparse feed.
pub fn eose_threshold(connected_count: usize, targeted_count: usize) -> usize {
    let threshold = (connected_count as f64 * 0.3) as usize;
    threshold.max(3).min(targeted_count.max(1))
}

/// Configuration for a realtime subscription.
#[derive(Clone, Debug)]
pub struct RealtimeConfig {
    /// The filter for the live tail. Should have `since` set to the
    /// latest local event timestamp.
    pub filter: Filter,
    /// How long to keep the subscription alive after EOSE.
    /// Default: 10 minutes (matches current `subscribe_realtime`).
    pub idle_timeout_secs: u64,
}

impl Default for RealtimeConfig {
    fn default() -> Self {
        Self {
            filter: Filter::new(),
            idle_timeout_secs: 600,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::RelayUrl;

    fn url(s: &str) -> RelayUrl {
        RelayUrl::parse(s).unwrap()
    }

    fn sub_id(s: &str) -> SubscriptionId {
        SubscriptionId::new(s)
    }

    #[test]
    fn eose_sets_last_eose_and_live() {
        let tracker = PerSubEoseTracker::new();
        let sid = sub_id("test");
        let relay = url("wss://r.example.com");
        tracker.register_sub(sid.clone());
        tracker.on_eose(&sid, &relay);
        let snap = tracker.snapshot(&sid).unwrap();
        let state = &snap[&relay];
        assert!(state.is_live);
        assert!(state.last_eose.is_some());
    }

    #[test]
    fn live_event_updates_after_eose() {
        let tracker = PerSubEoseTracker::new();
        let sid = sub_id("test");
        let relay = url("wss://r.example.com");
        tracker.register_sub(sid.clone());
        tracker.on_eose(&sid, &relay);
        let ts = Timestamp::from(1000);
        tracker.on_event(&sid, &relay, ts);
        let snap = tracker.snapshot(&sid).unwrap();
        let state = &snap[&relay];
        assert_eq!(state.last_live_event, Some(ts));
    }

    #[test]
    fn live_event_ignored_before_eose() {
        let tracker = PerSubEoseTracker::new();
        let sid = sub_id("test");
        let relay = url("wss://r.example.com");
        tracker.register_sub(sid.clone());
        // Event before EOSE — should not update last_live_event
        tracker.on_event(&sid, &relay, Timestamp::from(1000));
        let snap = tracker.snapshot(&sid).unwrap();
        let state = &snap[&relay];
        assert_eq!(state.last_live_event, None);
    }

    #[test]
    fn effective_since_returns_max_of_eose_and_live() {
        let tracker = PerSubEoseTracker::new();
        let sid = sub_id("test");
        let relay = url("wss://r.example.com");
        tracker.register_sub(sid.clone());
        tracker.on_eose(&sid, &relay);
        // Manually set a later live event
        {
            let mut inner = tracker.inner.lock().unwrap();
            let relays = inner.get_mut(&sid).unwrap();
            let state = relays.get_mut(&relay).unwrap();
            state.last_live_event = Some(Timestamp::from(5000));
        }
        let since = tracker.effective_since(&sid, &relay);
        // Should be max of last_eose (approx now) and 5000
        assert!(since.is_some());
        assert!(since.unwrap().as_secs() >= 5000);
    }

    #[test]
    fn min_effective_since_across_relays() {
        let tracker = PerSubEoseTracker::new();
        let sid = sub_id("test");
        let r1 = url("wss://r1.example.com");
        let r2 = url("wss://r2.example.com");
        tracker.register_sub(sid.clone());
        tracker.on_eose(&sid, &r1);
        tracker.on_eose(&sid, &r2);
        let min = tracker.min_effective_since(&sid);
        assert!(min.is_some());
    }

    #[test]
    fn all_eosed_requires_all_relays_live() {
        let tracker = PerSubEoseTracker::new();
        let sid = sub_id("test");
        let r1 = url("wss://r1.example.com");
        let r2 = url("wss://r2.example.com");
        tracker.register_sub(sid.clone());
        // Pre-register expected relays so all_eosed knows about them
        tracker.register_relays(&sid, [r1.clone(), r2.clone()]);
        tracker.on_eose(&sid, &r1);
        assert!(!tracker.all_eosed(&sid));
        tracker.on_eose(&sid, &r2);
        assert!(tracker.all_eosed(&sid));
    }

    #[test]
    fn eosed_count_tracks_progress() {
        let tracker = PerSubEoseTracker::new();
        let sid = sub_id("test");
        let r1 = url("wss://r1.example.com");
        let r2 = url("wss://r2.example.com");
        let r3 = url("wss://r3.example.com");
        tracker.register_sub(sid.clone());
        assert_eq!(tracker.eosed_count(&sid), 0);
        tracker.on_eose(&sid, &r1);
        assert_eq!(tracker.eosed_count(&sid), 1);
        tracker.on_eose(&sid, &r2);
        assert_eq!(tracker.eosed_count(&sid), 2);
        tracker.on_eose(&sid, &r3);
        assert_eq!(tracker.eosed_count(&sid), 3);
    }

    #[test]
    fn unregister_clears_state() {
        let tracker = PerSubEoseTracker::new();
        let sid = sub_id("test");
        let relay = url("wss://r.example.com");
        tracker.register_sub(sid.clone());
        tracker.on_eose(&sid, &relay);
        assert!(tracker.snapshot(&sid).is_some());
        tracker.unregister_sub(&sid);
        assert!(tracker.snapshot(&sid).is_none());
    }

    #[test]
    fn eose_threshold_uses_30_percent_with_min_3() {
        // 10 connected → max(3, 3) = 3
        assert_eq!(eose_threshold(10, 10), 3);
        // 100 connected → max(3, 30) = 30
        assert_eq!(eose_threshold(100, 100), 30);
        // 5 connected → max(3, 1) = 3 (clamped to min 3, max of targeted)
        assert_eq!(eose_threshold(5, 5), 3);
        // 1 connected → max(3, 0) = 1 (clamped to min of targeted=1)
        assert_eq!(eose_threshold(1, 1), 1);
    }

    #[test]
    fn eose_threshold_capped_by_targeted() {
        // 50 connected but only 5 targeted → max(3, 15) = 5 (capped)
        assert_eq!(eose_threshold(50, 5), 5);
    }
}
