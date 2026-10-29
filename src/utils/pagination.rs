use nostr::types::time::Timestamp;

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
}
