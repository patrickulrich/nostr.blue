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
use futures::StreamExt;
use std::time::Duration;

/// Relays added to the pool for the duration of a nest room view.
/// Returned by [`ensure_room_relays`]; pass `newly_added` back to
/// [`cleanup_room_relays`] when the room viewer unmounts.
#[derive(Clone, Debug, Default)]
pub struct RoomRelayMembership {
    /// Only the URLs we added to the pool — use for cleanup.
    pub newly_added: Vec<String>,
}

/// Ensure a nest room's relay URLs (naddr hints + the 30312 `relays` tag)
/// are pool members so targeted subscriptions (`subscribe_to`) can reach
/// them.
///
/// The SDK's `subscribe_targeted` hard-fails the ENTIRE call with
/// `RelayNotFound` when any URL is not a pool member — and Amethyst-hosted
/// rooms publish `relays` tags pointing at the host's outbox, which our
/// pool doesn't contain. This adds the missing relays with GOSSIP-only
/// flags: reachable by targeted calls (`can_read()` includes GOSSIP) but
/// invisible to generic broadcast snapshots (same rationale as
/// `coverage::connect_ephemeral_relays`), and DURABLE for the room view —
/// `reconnect(true)` with no idle timeout so quiet rooms don't lose their
/// relay mid-session.
pub async fn ensure_room_relays(client: &nostr_sdk::Client, urls: &[String]) -> RoomRelayMembership {
    // Diff against the FULL pool membership (`all_relays`) — GOSSIP-only
    // members are excluded from `client.relays()` snapshots.
    let members: std::collections::HashSet<String> = client
        .pool()
        .all_relays()
        .await
        .keys()
        .map(|u| crate::utils::relay::normalize_known_relay_url(u.as_str()))
        .collect();

    let mut seen = std::collections::HashSet::new();
    let mut to_add: Vec<nostr::Url> = Vec::new();
    for url in urls {
        if !is_valid_wss_url(url) {
            continue;
        }
        let normalized = crate::utils::relay::normalize_known_relay_url(url);
        if !seen.insert(normalized.clone()) || members.contains(&normalized) {
            continue;
        }
        let upgraded = crate::utils::relay::upgrade_to_secure_relay_url(&normalized);
        if let Ok(parsed) = nostr::Url::parse(&upgraded) {
            to_add.push(parsed);
        }
    }
    if to_add.is_empty() {
        return RoomRelayMembership::default();
    }

    let opts = nostr_sdk::RelayOptions::new()
        .reconnect(true)
        .flags(nostr_sdk::RelayServiceFlags::GOSSIP);
    for url in &to_add {
        let _ = client.pool().add_relay(url.clone(), opts.clone()).await;
    }

    // Connect with bounded concurrency (mirrors connect_ephemeral_relays).
    // Unlike the ephemeral variant, relays that fail to connect STAY in the
    // pool: `reconnect(true)` lets the SDK retry, and a dormant member is
    // harmless (relay-level failures land in `Output::failed`, not fatal).
    let connected: Vec<String> = futures::stream::iter(to_add.clone())
        .map(|url| {
            let client = client.clone();
            async move {
                if client
                    .pool()
                    .try_connect_relay(url.clone(), Duration::from_secs(3))
                    .await
                    .is_ok()
                {
                    Some(url.to_string())
                } else {
                    None
                }
            }
        })
        .buffer_unordered(5)
        .filter_map(|r| async { r })
        .collect()
        .await;
    log::info!(
        "Room-scoped relays: {} added to pool, {}/{} connected",
        to_add.len(),
        connected.len(),
        to_add.len()
    );

    // Canonical Url form (trailing "/" on bare hosts) so cleanup's
    // force_remove matches the pool keys — same convention as
    // connect_ephemeral_relays.
    RoomRelayMembership {
        newly_added: to_add.iter().map(|u| u.to_string()).collect(),
    }
}

/// Remove room-scoped relays added by [`ensure_room_relays`] when the room
/// viewer unmounts. Only force-removes relays that are still GOSSIP-only —
/// if the user adopted one as their own mid-session (READ/WRITE flags), it
/// is left alone.
pub async fn cleanup_room_relays(client: &nostr_sdk::Client, urls: &[String]) {
    let all = client.pool().all_relays().await;
    let gossip_only: std::collections::HashSet<String> = all
        .iter()
        .filter(|(_, relay)| {
            let flags = relay.flags();
            flags.has_gossip() && !(flags.has_read() || flags.has_write() || flags.has_discovery())
        })
        .map(|(url, _)| url.as_str().to_string())
        .collect();
    for url in urls {
        if gossip_only.contains(url.as_str()) {
            let _ = client.force_remove_relay(url.as_str()).await;
        }
    }
}

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
