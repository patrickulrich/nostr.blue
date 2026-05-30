use nostr_sdk::Timestamp;

pub const GAP_THRESHOLD_SECS: u64 = 6 * 3600;
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
}
