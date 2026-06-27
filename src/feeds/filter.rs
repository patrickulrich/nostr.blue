//! Feed filter construction: one builder per feed type.
//!
//! Replaces the duplicated inline filter construction across the 6+ loaders
//! in `routes/home/feed_loaders.rs`.
//!
//! ## Limit scaling (amethyst pattern)
//!
//! `scaled_limit(author_count)` returns `min(authors * 10, 500)` for following
//! feeds, so a user following 100 accounts gets a page depth of 1000 (capped
//! at 500), not a constant 33. For global feeds, a fixed page size of 50 is
//! used.
//!
//! ## Since-optimization (notedeck pattern)
//!
//! When the local database has accumulated at least `limit` notes for a feed,
//! the next relay REQ can include `since = latest_local - 60s` to avoid
//! re-fetching what we already have. The 60-second overlap absorbs relay
//! clock skew and late-arriving events.

use nostr_sdk::{Filter, Kind, PublicKey, Timestamp};

/// Kinds included in the home/primary feeds.
pub const HOME_FEED_KINDS: [Kind; 3] = [Kind::TextNote, Kind::Repost, Kind::Comment];

/// Default page size for the global firehose and relay feeds.
pub const GLOBAL_PAGE_LIMIT: usize = 50;

/// Default page size for backward pagination (the `limit` in `until`+`limit`).
pub const DEFAULT_PAGE_LIMIT: usize = 50;

/// The overlap buffer (seconds) applied when since-optimizing.
pub const SINCE_GAP_SECS: u64 = 60;

/// Maximum page size cap (amethyst's limit cap).
pub const MAX_PAGE_LIMIT: usize = 500;

/// Minimum page size floor.
pub const MIN_PAGE_LIMIT: usize = 50;

/// Compute the page limit for a following feed, scaling with author count.
///
/// Returns `min(authors * 10, 500)`, with a minimum of 50. This ensures a
/// user following 100 accounts gets the same per-author page depth as a user
/// following 10, but prevents unbounded result sets.
pub fn scaled_limit(author_count: usize) -> usize {
    (author_count * 10).clamp(MIN_PAGE_LIMIT, MAX_PAGE_LIMIT)
}

/// Decide whether to apply since-optimization.
///
/// Returns `true` only when the local database has at least `limit` notes
/// for this feed (a heuristic that we've saturated the first page and don't
/// need to backfill).
pub fn should_since_optimize(limit: usize, local_count: usize) -> bool {
    local_count >= limit
}

/// Apply since-optimization to a filter: set `since = latest_local - gap`.
///
/// The `gap_secs` overlap (default 60s) absorbs relay clock skew and
/// late-arriving events.
pub fn with_since_optimization(
    filter: Filter,
    latest_local: Timestamp,
    gap_secs: u64,
) -> Filter {
    let since = Timestamp::from(latest_local.as_secs().saturating_sub(gap_secs));
    filter.since(since)
}

// ─── Per-feed-type filter builders ──────────────────────────────────────────

/// Build a filter for the "following" feed (posts from followed authors).
///
/// Uses `HOME_FEED_KINDS` and `scaled_limit(authors.len())`.
/// Either `until` (pagination) or `since` (initial/since-optimized) should be
/// set; both can be set simultaneously if needed.
pub fn following_filter(
    authors: &[PublicKey],
    until: Option<Timestamp>,
    since: Option<Timestamp>,
) -> Filter {
    let mut filter = Filter::new()
        .kinds(HOME_FEED_KINDS)
        .authors(authors.iter().copied())
        .limit(scaled_limit(authors.len()));
    if let Some(ts) = until {
        filter = filter.until(ts);
    }
    if let Some(ts) = since {
        filter = filter.since(ts);
    }
    filter
}

/// Build a filter for the "following with replies" feed.
///
/// Same as `following_filter` but callers do NOT filter out replies in
/// post-processing (the filter itself is identical — the difference is in
/// how events are classified downstream).
pub fn following_with_replies_filter(
    authors: &[PublicKey],
    until: Option<Timestamp>,
    since: Option<Timestamp>,
) -> Filter {
    // Same construction; the "with replies" behavior is a post-filter decision.
    following_filter(authors, until, since)
}

/// Build a filter for the global firehose feed.
pub fn global_filter(until: Option<Timestamp>, since: Option<Timestamp>) -> Filter {
    let mut filter = Filter::new().kinds(HOME_FEED_KINDS).limit(GLOBAL_PAGE_LIMIT);
    if let Some(ts) = until {
        filter = filter.until(ts);
    }
    if let Some(ts) = since {
        filter = filter.since(ts);
    }
    filter
}

/// Build a filter for a people-list feed (NIP-51 curated follow set).
pub fn people_list_filter(
    members: &[PublicKey],
    until: Option<Timestamp>,
    since: Option<Timestamp>,
) -> Filter {
    // People lists are typically small (5-50 members); use scaled_limit
    // which clamps to 50 minimum.
    let mut filter = Filter::new()
        .kinds(HOME_FEED_KINDS)
        .authors(members.iter().copied())
        .limit(scaled_limit(members.len()));
    if let Some(ts) = until {
        filter = filter.until(ts);
    }
    if let Some(ts) = since {
        filter = filter.since(ts);
    }
    filter
}

/// Build a filter for a specific relay's feed (no author filter; queries
/// the relay's full firehose).
pub fn relay_feed_filter(until: Option<Timestamp>, since: Option<Timestamp>) -> Filter {
    // Same shape as global but used with subscribe_to(specific_relay_urls).
    global_filter(until, since)
}

/// Build a filter for a notifications feed (events mentioning or reacting
/// to the user's posts).
pub fn notifications_filter(pubkey: &PublicKey, since: Option<Timestamp>) -> Filter {
    const NOTIF_KINDS: [Kind; 4] = [
        Kind::TextNote,
        Kind::Reaction,
        Kind::Repost,
        Kind::ZapReceipt,
    ];
    let mut filter = Filter::new()
        .kinds(NOTIF_KINDS)
        .pubkey(*pubkey)
        .limit(500);
    if let Some(ts) = since {
        filter = filter.since(ts);
    }
    filter
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::{Keys, PublicKey, Timestamp};

    fn pks(n: usize) -> Vec<PublicKey> {
        (0..n).map(|_| Keys::generate().public_key()).collect()
    }

    #[test]
    fn scaled_limit_zero_authors() {
        assert_eq!(scaled_limit(0), MIN_PAGE_LIMIT);
    }

    #[test]
    fn scaled_limit_few_authors() {
        // 5 authors → 50, clamped to minimum
        assert_eq!(scaled_limit(5), MIN_PAGE_LIMIT);
    }

    #[test]
    fn scaled_limit_medium_authors() {
        // 20 authors → 200
        assert_eq!(scaled_limit(20), 200);
    }

    #[test]
    fn scaled_limit_many_authors() {
        // 100 authors → 1000, capped at 500
        assert_eq!(scaled_limit(100), MAX_PAGE_LIMIT);
    }

    #[test]
    fn scaled_limit_capped_at_max() {
        assert_eq!(scaled_limit(1000), MAX_PAGE_LIMIT);
    }

    #[test]
    fn following_filter_has_correct_kinds_authors_limit() {
        let authors = pks(10);
        let filter = following_filter(&authors, None, None);
        assert_eq!(filter.kinds.as_ref().map(|k| k.len()), Some(3));
        assert_eq!(filter.authors.as_ref().map(|a| a.len()), Some(10));
        assert_eq!(filter.limit, Some(100)); // 10 * 10 = 100
        assert!(filter.until.is_none());
        assert!(filter.since.is_none());
    }

    #[test]
    fn following_filter_with_until() {
        let authors = pks(3);
        let until = Timestamp::from(9999);
        let filter = following_filter(&authors, Some(until), None);
        assert_eq!(filter.until, Some(until));
    }

    #[test]
    fn global_filter_uses_fixed_limit() {
        let filter = global_filter(None, None);
        assert_eq!(filter.limit, Some(GLOBAL_PAGE_LIMIT));
        assert_eq!(filter.kinds.as_ref().map(|k| k.len()), Some(3));
        // No authors filter for global
        assert!(filter.authors.is_none());
    }

    #[test]
    fn should_since_optimize_triggers_when_local_ge_limit() {
        assert!(should_since_optimize(50, 50));
        assert!(should_since_optimize(50, 100));
    }

    #[test]
    fn should_since_optimize_skips_when_local_lt_limit() {
        assert!(!should_since_optimize(50, 49));
        assert!(!should_since_optimize(50, 0));
    }

    #[test]
    fn with_since_optimization_adds_gap_buffer() {
        let base = Filter::new().kind(Kind::TextNote);
        let latest = Timestamp::from(10_000);
        let optimized = with_since_optimization(base, latest, SINCE_GAP_SECS);
        assert_eq!(
            optimized.since,
            Some(Timestamp::from(10_000 - SINCE_GAP_SECS))
        );
    }
}
