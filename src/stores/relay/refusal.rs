//! Relay refusal skip-list (plan F1).
//!
//! Some relays (observed: ditto.pub) answer every backfill REQ with
//! `["CLOSED", <id>, "error: too many subscriptions"]` instead of serving
//! it. The SDK surfaces that as `ClientNotification::Message` carrying a
//! `RelayMessage::Closed`, which the notification dispatcher already
//! consumes — so the dispatcher stamps refused relays here, and fetch
//! paths skip recently-refused relays instead of re-issuing REQs that are
//! guaranteed to be refused (wasted round-trips + error spam on every
//! concurrent backfill).
//!
//! This is a TTL cache, not a blocklist: entries expire after
//! [`REFUSAL_TTL_MS`] and the relay is retried normally. Normal CLOSE
//! acknowledgements ("going offline", empty messages, duplicate-sub
//! notices) do not match the refusal classification and never stamp.

use std::collections::HashMap;
use std::sync::RwLock;

use nostr::types::url::RelayUrl;

/// How long a refusal stamp keeps a relay out of fetch targets.
pub const REFUSAL_TTL_MS: u64 = 3 * 60 * 1000;

/// Classify a `CLOSED` message as a fetch refusal (vs a normal close ack).
///
/// Refusal shapes seen in the wild:
/// - `error: too many subscriptions` (ditto.pub)
/// - `auth-required: ...` (paywall/auth-gated relays)
/// - `blocked: ...` (pubkey/IP blocks)
///
/// Case-insensitive; deliberately narrow so routine close reasons
/// ("going offline", "", "duplicate subscription") never match.
pub fn is_refusal_message(message: &str) -> bool {
    let m = message.trim().to_lowercase();
    if m.is_empty() {
        return false;
    }
    m.starts_with("error:")
        || m.starts_with("auth-required")
        || m.starts_with("blocked:")
        || m.contains("too many subscriptions")
}

struct RefusalEntry {
    stamp_ms: u64,
    #[allow(dead_code)] // kept for future diagnostics; equality via stamp
    reason: String,
}

/// Clock seam: production reads the platform clock; tests pin it (the
/// wasm clock path panics in native test runs). A nonzero `TEST_CLOCK`
/// short-circuits the platform call.
#[cfg(test)]
static TEST_CLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn clock_now_ms() -> u64 {
    #[cfg(test)]
    {
        let pinned = TEST_CLOCK.load(std::sync::atomic::Ordering::Relaxed);
        if pinned != 0 {
            return pinned;
        }
    }
    crate::platform::timestamp::now_millis()
}

static REFUSALS: std::sync::LazyLock<RwLock<HashMap<RelayUrl, RefusalEntry>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Stamp a relay as refused now (testable core: [`mark_refused_at`]).
pub fn mark_refused(relay_url: &RelayUrl, reason: &str) {
    mark_refused_at(relay_url, reason, crate::platform::timestamp::now_millis());
}

/// Testable stamp core: record the refusal at an explicit clock value.
fn mark_refused_at(relay_url: &RelayUrl, reason: &str, now_ms: u64) {
    let mut map = REFUSALS.write().unwrap_or_else(|e| e.into_inner());
    // Lazy prune: the map is tiny (bounded by pool size), but don't let
    // expired stamps accumulate across a long session.
    map.retain(|_, e| now_ms.saturating_sub(e.stamp_ms) < REFUSAL_TTL_MS);
    let re_stamp = map
        .get(relay_url)
        .is_some_and(|e| now_ms.saturating_sub(e.stamp_ms) < 1000);
    map.insert(
        relay_url.clone(),
        RefusalEntry {
            stamp_ms: now_ms,
            reason: reason.to_string(),
        },
    );
    // Log once per distinct refusal episode, not on every re-refusal
    // within the TTL window (a refused relay keeps answering our other
    // concurrent REQs with the same error).
    if !re_stamp {
        log::info!(
            "Relay {} refused a subscription (\"{}\") — skipping it for {}s",
            relay_url,
            reason.trim(),
            REFUSAL_TTL_MS / 1000
        );
    }
}

/// Whether `relay_url` refused a subscription within the TTL.
pub fn is_recently_refused(relay_url: &RelayUrl) -> bool {
    let now_ms = clock_now_ms();
    REFUSALS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(relay_url)
        .is_some_and(|e| now_ms.saturating_sub(e.stamp_ms) < REFUSAL_TTL_MS)
}

/// Split `urls` into `(ok, refused)` by refusal TTL — the fetch-path
/// filter helper. Order is preserved in both outputs.
pub fn partition_refused(
    urls: Vec<RelayUrl>,
) -> (Vec<RelayUrl>, Vec<RelayUrl>) {
    let mut ok = Vec::with_capacity(urls.len());
    let mut refused = Vec::new();
    for url in urls {
        if is_recently_refused(&url) {
            refused.push(url);
        } else {
            ok.push(url);
        }
    }
    (ok, refused)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> RelayUrl {
        RelayUrl::parse(s).unwrap()
    }

    #[test]
    fn classifies_refusal_shapes() {
        assert!(is_refusal_message("error: too many subscriptions"));
        assert!(is_refusal_message("ERROR: something broke"));
        assert!(is_refusal_message("auth-required: please auth"));
        assert!(is_refusal_message("Auth-Required"));
        assert!(is_refusal_message("blocked: policy"));
        assert!(is_refusal_message("sorry, too many subscriptions right now"));
        assert!(is_refusal_message("  error: leading whitespace\n"));
    }

    #[test]
    fn normal_closes_are_not_refusals() {
        assert!(!is_refusal_message(""));
        assert!(!is_refusal_message("   "));
        assert!(!is_refusal_message("going offline"));
        assert!(!is_refusal_message("duplicate subscription 0x1234"));
        assert!(!is_refusal_message("error")); // no colon: avoid false hit
    }

    #[test]
    fn refused_within_ttl_then_expires() {
        let relay = url("wss://refused.example.com");
        mark_refused_at(&relay, "error: too many subscriptions", 10_000);
        // Fresh stamp: refused at now=10_000+ε … but is_recently_refused
        // uses the real clock, so drive the stamp-relative checks through
        // partition at a controlled "now" instead: re-stamp near t0 and
        // check TTL math directly via a second stamp far in the future.
        assert!(REFUSALS
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(&relay));
        // Expire it by stamping at a much later clock value then verifying
        // the prune on the NEXT write drops it.
        let later = 10_000 + REFUSAL_TTL_MS + 1;
        mark_refused_at(&url("wss://other.example.com"), "blocked: x", later);
        assert!(!REFUSALS
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(&relay));
    }

    #[test]
    fn partition_splits_by_ttl() {
        // Pin the clock (see clock_now_ms): deterministic TTL check without
        // the platform clock, which panics off-web in native test runs.
        // Only this module's tests read it, so there is no cross-test race.
        TEST_CLOCK.store(500_000, std::sync::atomic::Ordering::Relaxed);
        let refused = url("wss://skip-me.example.com");
        let ok = url("wss://keep-me.example.com");
        // Stamped 100ms before the pinned clock: within the 3-min TTL.
        mark_refused_at(&refused, "error: too many subscriptions", 499_900);
        let (kept, skipped) = partition_refused(vec![ok.clone(), refused]);
        assert_eq!(kept, vec![ok]);
        assert_eq!(skipped.len(), 1);
        TEST_CLOCK.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}
