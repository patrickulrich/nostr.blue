//! Room-scoped relay computation.
//!
//! A nest room's effective relay set is the union of:
//!   1. The local user's NIP-65 read relays (durable baseline)
//!   2. The naddr's relay hints (where the room event was found)
//!   3. The room event's `relays` tag (where the host publishes updates)
//!
//! This mirrors `NestsUI-v2/src/hooks/useRoomNostr.ts` and Amethyst's
//! room-relay scoping. Subscriptions and fetches for room presence (10312),
//! room updates (30312), admin commands (4312), and chat (1311) target
//! this set so edits and stage promotions on a room-specific relay are
//! received without manual relay addition.
//!
//! All URLs are normalized to strings; invalid URLs and non-wss schemes
//! are dropped so a malformed hint can't widen the subscription to an
//! insecure transport.

use super::signals::{RelayPoolStoreStoreExt, RelaySource, RELAY_POOL};
use dioxus::prelude::ReadableExt;

/// Compute the effective relay set for a room from its component sources.
///
/// Pure function over the three input slices — callers pass `user_relays`
/// sourced reactively from `RELAY_POOL` (see [`user_nip65_relays`]) so the
/// result tracks changes to the user's relay list.
pub fn effective_room_relays(
    user_relays: &[String],
    naddr_hints: &[String],
    room_relays: &[String],
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for url in user_relays.iter().chain(naddr_hints).chain(room_relays) {
        if !is_valid_wss_url(url) {
            continue;
        }
        let normalized = url.trim_end_matches('/').to_string();
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
}

/// Snapshot of the user's own NIP-65 read relays from the global pool signal.
///
/// Reads `RELAY_POOL` and filters to entries whose `source` is `UserNip65`
/// and whose `has_read` flag is set. Disconnected relays are kept — the
/// subscription layer will skip them if it can't connect, but the user
/// explicitly listed them so we honor the intent.
pub fn user_nip65_relays() -> Vec<String> {
    let pool = RELAY_POOL.read();
    pool.data()
        .read()
        .iter()
        .filter(|r| r.source == RelaySource::UserNip65 && r.has_read)
        .map(|r| r.url.trim_end_matches('/').to_string())
        .collect()
}

/// Validate that `url` is a `wss://` (or localhost `ws://`) URL.
///
/// `ws://` is permitted only on localhost for dev relays; production nests
/// relays are always TLS. Bare URLs without a scheme are rejected.
fn is_valid_wss_url(url: &str) -> bool {
    if url.starts_with("wss://") {
        return true;
    }
    if url.starts_with("ws://") {
        let host_part = url.trim_start_matches("ws://");
        let host = host_part.split([':', '/']).next().unwrap_or("");
        return host == "localhost" || host == "127.0.0.1";
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_and_trim_trailing_slash() {
        let result = effective_room_relays(
            &["wss://a.com".into(), "wss://b.com/".into()],
            &["wss://b.com".into()],
            &["wss://c.com".into()],
        );
        assert_eq!(result, vec!["wss://a.com", "wss://b.com", "wss://c.com"]);
    }

    #[test]
    fn test_rejects_non_wss() {
        let result = effective_room_relays(
            &["https://a.com".into(), "wss://b.com".into()],
            &[],
            &[],
        );
        assert_eq!(result, vec!["wss://b.com"]);
    }

    #[test]
    fn test_allows_localhost_ws() {
        let result = effective_room_relays(
            &["ws://localhost:8080".into(), "ws://127.0.0.1:8080".into()],
            &[],
            &[],
        );
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_rejects_remote_ws() {
        let result = effective_room_relays(&["ws://example.com".into()], &[], &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_preserves_order_user_then_hints_then_room() {
        let result = effective_room_relays(
            &["wss://user.com".into()],
            &["wss://hint.com".into()],
            &["wss://room.com".into()],
        );
        assert_eq!(
            result,
            vec!["wss://user.com", "wss://hint.com", "wss://room.com"]
        );
    }
}
