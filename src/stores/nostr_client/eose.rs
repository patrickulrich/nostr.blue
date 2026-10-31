//! Unified EOSE threshold policies — single source of truth.
//!
//! Historically five implementations existed (`fetching.rs`,
//! `user_prefs/fetch.rs`, `feeds/realtime.rs`, `eose_tracker.rs`,
//! `aggregation/streaming.rs`) with subtly divergent thresholds. All
//! EOSE-counting code must source its threshold from here.
//!
//! Two named policies:
//! - [`realtime_liveness_threshold`] — realtime subscriptions declaring their
//!   initial page "loaded": 30% of connected relays, at least 3, capped by the
//!   number of targeted relays (dead pool relays must not make the threshold
//!   unreachable).
//! - [`fetch_quorum_threshold`] — one-shot fetches exiting early once half the
//!   relays the REQ reached have EOSE'd (the SDK's default `ExitOnEOSE` policy
//!   waits for ALL of them, so one dead relay pins the whole fetch).

/// Realtime initial-page liveness: `max(3, 30% of connected)`, capped by the
/// targeted count (never more than the number of relays actually asked).
pub fn realtime_liveness_threshold(connected_count: usize, targeted_count: usize) -> usize {
    let threshold = (connected_count as f64 * 0.3) as usize;
    threshold.max(3).min(targeted_count.max(1))
}

/// One-shot fetch quorum: half of the relays the REQ reached (rounded up),
/// always at least one.
pub fn fetch_quorum_threshold(target: usize) -> usize {
    target.max(1).div_ceil(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn realtime_uses_30_percent_with_min_3() {
        assert_eq!(realtime_liveness_threshold(10, 10), 3);
        assert_eq!(realtime_liveness_threshold(100, 100), 30);
        assert_eq!(realtime_liveness_threshold(5, 5), 3);
        // Single relay: quorum of one.
        assert_eq!(realtime_liveness_threshold(1, 1), 1);
    }

    #[test]
    fn realtime_capped_by_targeted() {
        assert_eq!(realtime_liveness_threshold(50, 5), 5);
    }

    #[test]
    fn fetch_quorum_is_half_rounded_up() {
        assert_eq!(fetch_quorum_threshold(1), 1);
        assert_eq!(fetch_quorum_threshold(2), 1);
        assert_eq!(fetch_quorum_threshold(3), 2);
        assert_eq!(fetch_quorum_threshold(4), 2);
        assert_eq!(fetch_quorum_threshold(5), 3);
        assert_eq!(fetch_quorum_threshold(0), 1);
    }
}
