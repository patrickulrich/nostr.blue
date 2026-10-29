use std::collections::HashMap;

use nostr::types::time::Timestamp;
use nostr::types::url::RelayUrl;

pub const GAP_THRESHOLD_SECS: u64 = 3600;
const MAX_FUTURE_SKEW_SECS: u64 = 120;

pub fn is_likely_future(ts: Timestamp) -> bool {
    ts.as_secs() > Timestamp::now().as_secs().saturating_add(MAX_FUTURE_SKEW_SECS)
}

pub fn is_likely_future_secs(secs: u64) -> bool {
    secs > Timestamp::now().as_secs().saturating_add(MAX_FUTURE_SKEW_SECS)
}

pub fn safe_cursor_from_timestamps(timestamps: &[u64]) -> Option<u64> {
    if timestamps.is_empty() {
        return None;
    }
    if timestamps.len() == 1 {
        return Some(timestamps[0].saturating_sub(1));
    }
    let mut sorted: Vec<u64> = timestamps.to_vec();
    sorted.sort_by(|a, b| b.cmp(a));
    for i in 0..sorted.len() - 1 {
        if sorted[i].saturating_sub(sorted[i + 1]) >= GAP_THRESHOLD_SECS {
            return Some(sorted[i].saturating_sub(1));
        }
    }
    Some(sorted.last().copied().unwrap().saturating_sub(1))
}

/// Backward-pagination cursor over raw (pre-classification) event
/// timestamps — a global-fetch port of Amethyst's `RelayLoadingCursors`.
///
/// `reached` is the running minimum `created_at` observed across pages.
/// The next page requests `until = reached` **inclusively**: the boundary
/// second is re-sent and rejected by dedup (NIP-67 tie-safe — stepping
/// below the boundary could skip events sharing the timestamp; Amethyst's
/// mirror worker uses the same overlap: "until re-sends the boundary ts
/// (dupes rejected)").
///
/// A page that delivers nothing strictly older than `reached` makes no
/// progress and must end pagination instead of looping (Amethyst
/// `RelayLoadingCursorsTest`:
/// "a non-advancing page should end the relay, not re-loop"). This is the
/// guard against the pagination fixed point where a display-level gap
/// heuristic re-derived the same cursor forever and `load_more` re-issued
/// identical REQs on every scroll trigger.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackwardPageCursor {
    reached: Option<u64>,
}

impl BackwardPageCursor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear the session (feed switch / refresh re-arms pagination).
    pub fn reset(&mut self) {
        self.reached = None;
    }

    /// Oldest raw timestamp reached so far (for status display).
    pub fn reached(&self) -> Option<u64> {
        self.reached
    }

    /// The `until` bound for the next page: inclusive at `reached`, so the
    /// boundary second is re-requested and same-timestamp events (NIP-67
    /// ties) are never skipped. Dedup rejects the already-seen ones.
    pub fn next_until(&self) -> Option<u64> {
        self.reached
    }

    /// Fold one page's raw floor (min `created_at` of the fetched events,
    /// pre-classification) and report whether the page advanced.
    ///
    /// `request_bound` is the `until` this page was asked with: a
    /// misbehaving relay answering above the bound must never move the
    /// cursor up, so the floor is clamped to it.
    ///
    /// Returns `true` when the page delivered at least one event strictly
    /// older than everything previously observed (`false` = empty raw page
    /// or a non-advancing echo page → settle the window).
    pub fn fold_page(&mut self, page_floor: Option<u64>, request_bound: Option<u64>) -> bool {
        let Some(floor) = page_floor else {
            return false;
        };
        let floor = match request_bound {
            Some(bound) => floor.min(bound),
            None => floor,
        };
        match self.reached {
            None => {
                self.reached = Some(floor);
                true
            }
            Some(current) if floor < current => {
                self.reached = Some(floor);
                true
            }
            Some(_) => false,
        }
    }
}

/// Per-relay paging position: how far back one relay has actually
/// delivered, and whether it has bottomed out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RelayCursor {
    /// Oldest raw `created_at` this relay has delivered across settled
    /// pages (Amethyst: "reachedUntil moves in onEose" — deliveries on a
    /// stalled page never move it).
    reached: Option<u64>,
    /// Set once the relay answered an empty or non-advancing page with a
    /// clean EOSE: nothing strictly older is reachable on it.
    done: bool,
}

/// Per-relay backward-pagination cursors — a pull-model port of
/// Amethyst's `RelayLoadingCursors` (quartz) for the home feed.
///
/// Where [`BackwardPageCursor`] keeps one global floor for the union
/// fetch, `RelayCursorSet` lets every relay walk its own timeline: a
/// relay is next asked just below **its own** oldest delivery, so a
/// sparse relay stops re-serving the window a denser relay already paged
/// past (the ~40% duplicate work seen in live logs).
///
/// Deviations from Amethyst, both deliberate:
/// - **Inclusive overlap**: the next page asks `until = reached` (not
///   `reached - 1`) so events sharing the boundary second are re-sent
///   and dedup rejects the seen ones — NIP-67 tie-safe, same rule as
///   [`BackwardPageCursor::next_until`]. A relay whose next page holds
///   nothing *strictly older* is done either way.
/// - **No armed/requested split**: pages are pulled synchronously per
///   `load_more` trigger, so there is no parked REQ to re-arm — the
///   `until` for a relay's next page is derived from `reached` on
///   demand. A relay that has never delivered (or churned into the
///   pool mid-session) arms at the caller's global bound instead.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelayCursorSet {
    cursors: HashMap<RelayUrl, RelayCursor>,
}

impl RelayCursorSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear the session (feed switch / refresh re-arms pagination).
    pub fn reset(&mut self) {
        self.cursors.clear();
    }

    /// Drop entries for relays that left the connected snapshot. The
    /// pool churns constantly (relays go to sleep and reconnect); stale
    /// entries would otherwise pin `deepest_reached` and `exhausted`
    /// against relays no page will ever ask again.
    pub fn retain_live(&mut self, live: &[RelayUrl]) {
        self.cursors.retain(|url, _| live.contains(url));
    }

    /// True once this relay answered an empty or non-advancing page
    /// with a clean EOSE — nothing older to ask it for.
    pub fn is_done(&self, url: &RelayUrl) -> bool {
        self.cursors.get(url).is_some_and(|c| c.done)
    }

    /// The `until` bound for this relay's next page: **inclusive** at
    /// its own `reached` (boundary second re-sent, dedup rejects). A
    /// relay that has never delivered arms at the global bound (the
    /// initial load's floor for the first page, the current deepest
    /// point for a relay that churned in mid-session); `None` only when
    /// even the global bound is unknown (unbounded first page).
    pub fn until_for(&self, url: &RelayUrl, global_bound: Option<u64>) -> Option<u64> {
        match self.cursors.get(url) {
            Some(c) => c.reached.or(global_bound),
            None => global_bound,
        }
    }

    /// Fold one relay's page and report whether it advanced.
    ///
    /// * `floor` — min raw `created_at` this relay delivered on the
    ///   page (`None` = empty page).
    /// * `request_bound` — the `until` this page carried; a
    ///   misbehaving relay answering above the bound must never move
    ///   the cursor up.
    /// * `settled` — the page ended with a clean EOSE. `false` = the
    ///   relay stalled (CLOSE notice, silent past the deadline,
    ///   disconnected, or the subscribe failed): its deliveries still
    ///   surface in the page union, but the cursor never moves on a
    ///   stalled page and the relay is **never** marked done — a
    ///   silent relay must not be mistaken for an empty one
    ///   (Amethyst: stalled relays are "kept, retry on advance").
    pub fn fold_page(
        &mut self,
        url: &RelayUrl,
        floor: Option<u64>,
        request_bound: Option<u64>,
        settled: bool,
    ) -> bool {
        let cursor = self.cursors.entry(url.clone()).or_default();
        if !settled {
            return false;
        }
        let Some(raw_floor) = floor else {
            // Empty page at EOSE is the gap-proof stop: `until`+`limit`
            // skips gaps, so "nothing matched" means nothing older
            // exists on this relay.
            cursor.done = true;
            return false;
        };
        let floor = match request_bound {
            Some(bound) => raw_floor.min(bound),
            None => raw_floor,
        };
        match cursor.reached {
            None => {
                cursor.reached = Some(floor);
                true
            }
            Some(current) if floor < current => {
                cursor.reached = Some(floor);
                true
            }
            // Echo page (nothing strictly older than already reached —
            // with the inclusive overlap this includes pure-boundary
            // duplicates): end the relay instead of re-looping.
            Some(_) => {
                cursor.done = true;
                false
            }
        }
    }

    /// Oldest raw timestamp any relay has delivered (for status display
    /// and as the global bound for newly armed relays). Relays that
    /// never delivered don't hold it back.
    pub fn deepest_reached(&self) -> Option<u64> {
        self.cursors.values().filter_map(|c| c.reached).min()
    }

    /// True when every live relay has bottomed out. **Stalled relays do
    /// not count**: exhaustion requires answered-empty relays, so a
    /// network hiccup keeps the window open and the sentinel keeps
    /// retrying instead of permanently settling the feed (the
    /// non-advancing echo case is what actually ends sessions).
    pub fn exhausted(&self, live: &[RelayUrl]) -> bool {
        !live.is_empty() && live.iter().all(|url| self.is_done(url))
    }

    /// How many live relays are done (logging only).
    pub fn done_count(&self, live: &[RelayUrl]) -> usize {
        live.iter().filter(|url| self.is_done(url)).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_cursor_empty() {
        assert_eq!(safe_cursor_from_timestamps(&[]), None);
    }

    #[test]
    fn safe_cursor_single() {
        assert_eq!(safe_cursor_from_timestamps(&[1000]), Some(999));
    }

    #[test]
    fn safe_cursor_tight_cluster() {
        assert_eq!(
            safe_cursor_from_timestamps(&[1000, 990, 980]),
            Some(979)
        );
    }

    #[test]
    fn safe_cursor_outlier_ignored() {
        let now = 1_000_000;
        assert_eq!(
            safe_cursor_from_timestamps(&[now, now - 100, now - 50_000]),
            Some(now - 100 - 1)
        );
    }

    #[test]
    fn safe_cursor_two_clusters() {
        let now = 1_000_000;
        assert_eq!(
            safe_cursor_from_timestamps(&[now, now - 100, now - 30_000, now - 30_100]),
            Some(now - 100 - 1)
        );
    }

    #[test]
    fn safe_cursor_no_gap() {
        let now = 1_000_000;
        assert_eq!(
            safe_cursor_from_timestamps(&[now, now - 100, now - 200]),
            Some(now - 200 - 1)
        );
    }

    #[test]
    fn backward_cursor_empty_page_does_not_advance() {
        let mut c = BackwardPageCursor::new();
        assert!(!c.fold_page(None, Some(1_000)));
        assert_eq!(c.next_until(), None);
    }

    #[test]
    fn backward_cursor_first_page_sets_reached() {
        let mut c = BackwardPageCursor::new();
        assert!(c.fold_page(Some(800), Some(1_000)));
        assert_eq!(c.reached(), Some(800));
        // Inclusive overlap: the next page re-asks AT the floor, not below.
        assert_eq!(c.next_until(), Some(800));
    }

    #[test]
    fn backward_cursor_strictly_older_page_advances() {
        let mut c = BackwardPageCursor::new();
        assert!(c.fold_page(Some(800), Some(1_000)));
        assert!(c.fold_page(Some(500), Some(800)));
        assert_eq!(c.next_until(), Some(500));
    }

    #[test]
    fn backward_cursor_non_advancing_page_settles() {
        let mut c = BackwardPageCursor::new();
        assert!(c.fold_page(Some(800), Some(1_000)));
        // Echo page: floor at/above the running floor (nothing strictly older).
        assert!(!c.fold_page(Some(800), Some(800)));
        assert!(!c.fold_page(Some(900), Some(800)));
        // Cursor unchanged by the echo pages.
        assert_eq!(c.next_until(), Some(800));
    }

    #[test]
    fn backward_cursor_clamps_floors_above_request_bound() {
        let mut c = BackwardPageCursor::new();
        assert!(c.fold_page(Some(800), Some(1_000)));
        // Misbehaving relay answers above the asked bound: clamp, never climb.
        assert!(!c.fold_page(Some(950), Some(800)));
        assert_eq!(c.next_until(), Some(800));
    }

    #[test]
    fn backward_cursor_unbounded_first_page_uses_floor_verbatim() {
        let mut c = BackwardPageCursor::new();
        assert!(c.fold_page(Some(123), None));
        assert_eq!(c.next_until(), Some(123));
    }

    #[test]
    fn backward_cursor_reset_rearms_session() {
        let mut c = BackwardPageCursor::new();
        assert!(c.fold_page(Some(800), Some(1_000)));
        c.reset();
        assert_eq!(c.next_until(), None);
        // A fresh session starts from the next page's floor.
        assert!(c.fold_page(Some(900), Some(1_000)));
        assert_eq!(c.next_until(), Some(900));
    }

    // ── RelayCursorSet: ported from Amethyst RelayLoadingCursorsTest ──

    fn url(name: &str) -> RelayUrl {
        RelayUrl::parse(name).unwrap()
    }

    /// "unarmedRelayIsNotRequestedAndSitsAtTheFloor" — a relay that has
    /// never delivered arms at the global bound.
    #[test]
    fn relay_cursors_first_arm_uses_global_bound() {
        let cursors = RelayCursorSet::new();
        let a = url("wss://a.relay");
        assert_eq!(cursors.until_for(&a, Some(1_000)), Some(1_000));
        assert_eq!(cursors.until_for(&a, None), None);
        assert!(!cursors.is_done(&a));
    }

    /// "firstAdvanceRequestsTheFloorThenSubsequentPagesStepBelowReached",
    /// with our inclusive-overlap deviation: the next page re-asks AT
    /// the reached floor (boundary second re-sent, dedup rejects), not
    /// Amethyst's `reached - 1`.
    #[test]
    fn relay_cursors_next_until_is_inclusive_at_reached() {
        let mut cursors = RelayCursorSet::new();
        let a = url("wss://a.relay");
        assert!(cursors.fold_page(&a, Some(800), Some(1_000), true));
        assert_eq!(cursors.until_for(&a, Some(1_000)), Some(800));
    }

    /// "emptyPageMarksRelayDoneAndBlocksFurtherAdvance".
    #[test]
    fn relay_cursors_empty_settled_page_marks_done() {
        let mut cursors = RelayCursorSet::new();
        let a = url("wss://a.relay");
        assert!(cursors.fold_page(&a, Some(800), Some(1_000), true));
        // Empty page at EOSE: done, and by definition not an advance.
        assert!(!cursors.fold_page(&a, None, Some(800), true));
        assert!(cursors.is_done(&a));
        // A done relay stays done; folding another empty page is a no-op.
        assert!(!cursors.fold_page(&a, None, Some(800), true));
        assert!(cursors.is_done(&a));
    }

    /// "aPageThatDoesNotStepOlderEndsTheRelayInsteadOfLooping" — the
    /// guard against the pagination fixed point, per relay.
    #[test]
    fn relay_cursors_non_advancing_page_ends_relay() {
        let mut cursors = RelayCursorSet::new();
        let a = url("wss://a.relay");
        assert!(cursors.fold_page(&a, Some(800), Some(1_000), true));
        // Echo page: floor at/above the reached floor (with the
        // inclusive overlap this is the pure-boundary-duplicate case).
        assert!(!cursors.fold_page(&a, Some(800), Some(800), true));
        assert!(!cursors.fold_page(&a, Some(900), Some(800), true));
        assert!(cursors.is_done(&a));
        assert_eq!(cursors.until_for(&a, Some(1_000)), Some(800));
    }

    /// "relaysAreTrackedIndependently" + "deepestReachedIsNullWhenNoRelays".
    #[test]
    fn relay_cursors_relays_tracked_independently() {
        let mut cursors = RelayCursorSet::new();
        let a = url("wss://a.relay");
        let b = url("wss://b.relay");
        assert!(cursors.fold_page(&a, Some(500), Some(1_000), true));
        // B never delivered: deepest is A's floor, B arms at the bound.
        assert_eq!(cursors.deepest_reached(), Some(500));
        assert_eq!(cursors.until_for(&b, Some(1_000)), Some(1_000));
        // B advancing does not disturb A's cursor.
        assert!(cursors.fold_page(&b, Some(300), Some(1_000), true));
        assert_eq!(cursors.until_for(&a, Some(1_000)), Some(500));
        assert_eq!(cursors.until_for(&b, Some(1_000)), Some(300));
        assert_eq!(cursors.deepest_reached(), Some(300));
    }

    #[test]
    fn relay_cursors_deepest_reached_none_when_nothing_delivered() {
        let cursors = RelayCursorSet::new();
        assert_eq!(cursors.deepest_reached(), None);
    }

    /// Amethyst's stalled semantics: a page that ends without a clean
    /// EOSE (CLOSE notice, silence, disconnect, failed subscribe) never
    /// moves the cursor and never ends the relay — "kept, retry".
    #[test]
    fn relay_cursors_stalled_page_neither_advances_nor_ends() {
        let mut cursors = RelayCursorSet::new();
        let a = url("wss://a.relay");
        assert!(cursors.fold_page(&a, Some(800), Some(1_000), true));
        // Stalled page that still delivered events: union keeps them,
        // but `reached` must not move (Amethyst: reachedUntil moves in
        // onEose only) and the relay must stay retryable.
        assert!(!cursors.fold_page(&a, Some(500), Some(800), false));
        assert!(!cursors.is_done(&a));
        assert_eq!(cursors.until_for(&a, Some(1_000)), Some(800));
        // An entirely silent stalled page likewise ends nothing.
        assert!(!cursors.fold_page(&a, None, Some(800), false));
        assert!(!cursors.is_done(&a));
        // The relay resumes normally on its next settled page.
        assert!(cursors.fold_page(&a, Some(500), Some(800), true));
        assert_eq!(cursors.until_for(&a, Some(1_000)), Some(500));
    }

    /// Relay churn: entries for relays outside the connected snapshot
    /// are pruned; churned-in relays arm at the global bound.
    #[test]
    fn relay_cursors_retain_live_prunes_churned_relays() {
        let mut cursors = RelayCursorSet::new();
        let a = url("wss://a.relay");
        let b = url("wss://b.relay");
        assert!(cursors.fold_page(&a, Some(500), Some(1_000), true));
        assert!(cursors.fold_page(&b, Some(600), Some(1_000), true));
        cursors.retain_live(std::slice::from_ref(&a));
        // B gone: its floor no longer pins deepest, and it re-arms at
        // the global bound if it churns back in.
        assert_eq!(cursors.deepest_reached(), Some(500));
        assert_eq!(cursors.until_for(&b, Some(700)), Some(700));
    }

    /// Exhaustion needs every live relay answered-empty; stalled relays
    /// keep the window open.
    #[test]
    fn relay_cursors_exhausted_requires_all_live_done() {
        let mut cursors = RelayCursorSet::new();
        let a = url("wss://a.relay");
        let b = url("wss://b.relay");
        let live = vec![a.clone(), b.clone()];
        assert!(!cursors.exhausted(&live));
        // A bottoms out; B still paging.
        assert!(cursors.fold_page(&a, Some(800), Some(1_000), true));
        // A's empty page marks it done (and is not an advance).
        assert!(!cursors.fold_page(&a, None, Some(800), true));
        assert_eq!(cursors.done_count(&live), 1);
        assert!(!cursors.exhausted(&live));
        // B stalls (not done) — still not exhausted.
        assert!(!cursors.fold_page(&b, None, Some(1_000), false));
        assert!(!cursors.exhausted(&live));
        // B answers empty at EOSE — now the window is settled.
        assert!(!cursors.fold_page(&b, None, Some(1_000), true));
        assert!(cursors.exhausted(&live));
    }

    #[test]
    fn relay_cursors_exhausted_empty_live_set_is_false() {
        let mut cursors = RelayCursorSet::new();
        let a = url("wss://a.relay");
        // Empty first page settles the relay (done, not advanced).
        assert!(!cursors.fold_page(&a, None, Some(1_000), true));
        assert!(cursors.is_done(&a));
        // The caller guards "no connected relays" before consulting
        // exhaustion (vacuous truth would settle a live network blip).
        assert!(!cursors.exhausted(&[]));
    }

    /// Misbehaving relay answering above the asked bound is clamped and
    /// ends the relay rather than climbing the cursor (same rule as
    /// `BackwardPageCursor`).
    #[test]
    fn relay_cursors_clamps_floors_above_request_bound() {
        let mut cursors = RelayCursorSet::new();
        let a = url("wss://a.relay");
        assert!(cursors.fold_page(&a, Some(800), Some(1_000), true));
        assert!(!cursors.fold_page(&a, Some(950), Some(800), true));
        assert!(cursors.is_done(&a));
        assert_eq!(cursors.until_for(&a, Some(1_000)), Some(800));
    }

    /// An unbounded first page (no global bound known) uses the floor
    /// verbatim — mirrors `BackwardPageCursor`'s rule.
    #[test]
    fn relay_cursors_unbounded_first_page_uses_floor_verbatim() {
        let mut cursors = RelayCursorSet::new();
        let a = url("wss://a.relay");
        assert!(cursors.fold_page(&a, Some(123), None, true));
        assert_eq!(cursors.until_for(&a, None), Some(123));
    }

    #[test]
    fn relay_cursors_reset_rearms_session() {
        let mut cursors = RelayCursorSet::new();
        let a = url("wss://a.relay");
        assert!(cursors.fold_page(&a, Some(800), Some(1_000), true));
        cursors.reset();
        assert!(!cursors.is_done(&a));
        assert_eq!(cursors.deepest_reached(), None);
        // Fresh session arms at the global bound again.
        assert_eq!(cursors.until_for(&a, Some(1_000)), Some(1_000));
    }
}
