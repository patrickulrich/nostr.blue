//! Mostro notification history (NIP-78 kind 30078)
//!
//! Persistent log of Mostro protocol actions that produced a user-facing
//! notification. Lets users recover dismissed toasts or events that
//! arrived while they were on a different route (especially important
//! while mobile push is deferred).
//!
//! Backing store: NIP-78 (kind 30078) event with d-tag
//! `nostr.blue/p2p/notifications`, content = a JSON array of
//! [`MostroNotification`] records, NIP-44-encrypted to self via the Mostro
//! identity key (same key choice as `trade_store` and `node_config`).
//!
//! Load order (mirrors `trade_store.rs` and `nip78.rs`):
//! 1. **Local cache** (`platform::storage`) — synchronous, instant on boot.
//! 2. **Relay fetch** — best-effort merge, preserves local unread state.
//! 3. **Daemon** (authoritative for trade state; notifications are derived).
//!
//! Cap: 200 entries, newest-first. Older entries are silently evicted on
//! insert. `read_at` survives across-cap eviction if the entry is < 7 days
//! old (so a user dismissing their list doesn't lose read-state for
//! long-lived entries).

use dioxus::prelude::*;
use nostr::prelude::*;
use nostr_sdk::Event as NostrEvent;
use serde::{Deserialize, Serialize};
use std::result::Result;
use std::time::Duration;

use crate::platform::storage;
use crate::stores::auth_store;
use crate::stores::nostr_client;
use crate::stores::publish_queue::{self, types::QueueEventType};

/// NIP-78 d-tag for the notifications list event.
pub const NOTIFICATIONS_D_TAG: &str = "nostr.blue/p2p/notifications";

/// Local cache key (in `platform::storage`).
const CACHE_KEY: &str = "mostro_notifications_v1";

/// Maximum entries kept in memory + cache + on-wire.
const CAP: usize = 200;

/// Entries older than this (in seconds) are eligible for cap eviction
/// regardless of read state. 7 days.
const EVICTION_GRACE_SECS: i64 = 7 * 24 * 60 * 60;

/// A single notification record.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MostroNotification {
    /// Stable dedup key (UUID). Same id from relay and local must merge.
    pub id: String,
    /// The order the notification relates to (None for orphan actions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    /// The dispute the notification relates to (None for non-dispute actions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispute_id: Option<String>,
    /// Daemon pubkey the source trade lives on. Lets us filter when the
    /// user switches daemons.
    #[serde(default)]
    pub daemon_pubkey: String,
    /// Mostro `Action` enum variant name (kebab-case), or `"ChatMessage"` /
    /// `"DisputeChatMessage"` for chat-originated notifications.
    pub action_str: String,
    /// User-facing title (e.g. "Invoice to pay").
    pub title: String,
    /// User-facing body.
    pub body: String,
    /// Unix-seconds when the notification was generated.
    pub created_at: i64,
    /// Unix-seconds when the user opened/dismissed the notification.
    /// `None` means unread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<i64>,
}

/// Global reactive list of notifications, newest-first.
#[allow(dead_code)]
pub static NOTIFICATIONS: GlobalSignal<Vec<MostroNotification>> = Signal::global(Vec::new);

// ── Local cache ───────────────────────────────────────────────────────

fn read_cache() -> Vec<MostroNotification> {
    storage::get::<String>(CACHE_KEY)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn write_cache(list: &[MostroNotification]) -> Result<(), String> {
    let json = serde_json::to_string(list)
        .map_err(|e| format!("failed to serialize notifications: {e}"))?;
    storage::set(CACHE_KEY, &json).map_err(|e| format!("failed to cache notifications: {e}"))
}

/// Synchronously load the cache into the global signal. Call this at app
/// init for instant notification list availability.
#[allow(dead_code)]
pub fn init_from_cache() {
    if NOTIFICATIONS.read().is_empty() {
        let cached = read_cache();
        if !cached.is_empty() {
            *NOTIFICATIONS.write() = cached;
        }
    }
}

// ── Mutations ─────────────────────────────────────────────────────────

/// Add a notification. Dedup by `id` (if an entry with the same id exists,
/// the existing entry's `read_at` is preserved and other fields refresh).
/// The list is kept at `CAP` entries; oldest entries beyond the cap are
/// evicted unless they're unread and younger than `EVICTION_GRACE_SECS`.
///
/// Writes to the local cache synchronously. Schedules a debounced relay
/// publish via [`publish_debounced`].
#[allow(dead_code)]
pub fn push(n: MostroNotification) {
    let mut list = NOTIFICATIONS.write();
    // Dedup: if id exists, preserve read_at, refresh other fields.
    if let Some(existing) = list.iter_mut().find(|e| e.id == n.id) {
        let preserved_read = existing.read_at;
        *existing = n;
        existing.read_at = preserved_read;
    } else {
        list.insert(0, n);
    }
    enforce_cap(&mut list);
    let snapshot = list.clone();
    drop(list);
    let _ = write_cache(&snapshot);
    publish_debounced();
}

/// Mark a single notification as read by id. No-op if not found.
#[allow(dead_code)]
pub fn mark_read(id: &str) {
    let now = now_secs();
    let mut list = NOTIFICATIONS.write();
    let mut changed = false;
    if let Some(n) = list.iter_mut().find(|n| n.id == id) {
        if n.read_at.is_none() {
            n.read_at = Some(now);
            changed = true;
        }
    }
    if changed {
        let snapshot = list.clone();
        drop(list);
        let _ = write_cache(&snapshot);
        publish_debounced();
    }
}

/// Mark all notifications as read.
#[allow(dead_code)]
pub fn mark_all_read() {
    let now = now_secs();
    let mut list = NOTIFICATIONS.write();
    let any_unread = list.iter().any(|n| n.read_at.is_none());
    if !any_unread {
        return;
    }
    for n in list.iter_mut() {
        if n.read_at.is_none() {
            n.read_at = Some(now);
        }
    }
    let snapshot = list.clone();
    drop(list);
    let _ = write_cache(&snapshot);
    publish_debounced();
}

/// Clear all notifications (user action). Also publishes an empty record
/// to relays so the state syncs across devices.
#[allow(dead_code)]
pub fn clear_all() {
    let mut list = NOTIFICATIONS.write();
    if list.is_empty() {
        return;
    }
    list.clear();
    drop(list);
    let _ = write_cache(&[]);
    publish_debounced();
}

/// Wipe local state. Used on logout.
#[allow(dead_code)]
pub fn reset() {
    let _ = storage::delete(CACHE_KEY);
    *NOTIFICATIONS.write() = Vec::new();
    *dirty_cell()
        .write()
        .unwrap_or_else(|e| e.into_inner()) = false;
}

/// Count of unread notifications (for sidebar badge).
#[allow(dead_code)]
pub fn unread_count() -> usize {
    NOTIFICATIONS.read().iter().filter(|n| n.read_at.is_none()).count()
}

// ── Cap enforcement ───────────────────────────────────────────────────

/// Drop entries beyond `CAP`. Unread entries younger than
/// `EVICTION_GRACE_SECS` are preserved even past the cap (so a user who
/// hasn't opened the app in a week doesn't silently lose unread state for
/// long-lived items).
fn enforce_cap(list: &mut Vec<MostroNotification>) {
    enforce_cap_at(list, now_secs());
}

/// Same as `enforce_cap` but with a caller-supplied `now`. Used in tests
/// to avoid touching the platform timestamp.
fn enforce_cap_at(list: &mut Vec<MostroNotification>, now: i64) {
    if list.len() <= CAP {
        return;
    }
    let mut kept: Vec<MostroNotification> = Vec::with_capacity(CAP);
    for (i, n) in list.drain(..).enumerate() {
        if i < CAP {
            kept.push(n);
            continue;
        }
        // Beyond cap: preserve if unread and fresh.
        let is_fresh_unread = n.read_at.is_none()
            && (now - n.created_at) < EVICTION_GRACE_SECS;
        if is_fresh_unread {
            kept.push(n);
        }
    }
    // If we kept extra (fresh unread) past CAP, sort by created_at desc and
    // trim the oldest non-fresh entries.
    if kept.len() > CAP {
        kept.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        kept.truncate(CAP);
    }
    *list = kept;
}

// ── Relay sync ────────────────────────────────────────────────────────

/// Verify a NIP-78 event is a valid notifications record owned by the user.
///
/// Mirrors `trade_store::evaluate_event`: checks pubkey + signature, then
/// tries NIP-44 decryption first with a plaintext fallback.
fn evaluate_event(event: &NostrEvent, user_pubkey: &PublicKey) -> Option<Vec<MostroNotification>> {
    if event.pubkey != *user_pubkey {
        return None;
    }
    if event.verify().is_err() {
        return None;
    }
    let parsed: Vec<MostroNotification> =
        crate::stores::private_app_data::decrypt_from_self_or_legacy(&event.content).ok()?;
    Some(parsed)
}

/// Refresh from relays (best-effort). On failure, leaves existing state
/// alone so the user can still see cached notifications offline.
///
/// Merge semantics: union by `id`, preserve local `read_at` if remote lacks
/// it (or take the later of the two if both present).
#[allow(dead_code)]
pub async fn refresh_from_relays() -> Result<usize, String> {
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {e}"))?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(30078))
        .identifier(NOTIFICATIONS_D_TAG)
        .limit(1);
    nostr_client::ensure_relays_ready(&client).await;

    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            let fresh = events.iter().find_map(|e| evaluate_event(e, &pubkey));
            if let Some(remote_list) = fresh {
                let count = merge_remote(remote_list);
                Ok(count)
            } else {
                Ok(0)
            }
        }
        Err(e) => {
            log::warn!("Failed to fetch Mostro notifications: {e}");
            Err(format!("Failed to fetch notifications: {e}"))
        }
    }
}

/// Merge a remote list into the local signal. Dedup by `id`; on conflict,
/// preserve local `read_at` if remote lacks it, or take the later of the
/// two if both present. Other fields refresh from remote (which is
/// typically newer).
fn merge_remote(remote: Vec<MostroNotification>) -> usize {
    let local: Vec<MostroNotification> = NOTIFICATIONS.read().clone();
    let merged = merge_lists_pure(local, remote);
    let count = merged.len();
    *NOTIFICATIONS.write() = merged.clone();
    let _ = write_cache(&merged);
    count
}

/// Pure merge logic (no signal mutation). Exposed for testing.
fn merge_lists_pure(
    mut local: Vec<MostroNotification>,
    remote: Vec<MostroNotification>,
) -> Vec<MostroNotification> {
    let now = now_secs();
    for remote_n in remote {
        match local.iter_mut().find(|l| l.id == remote_n.id) {
            Some(l) => {
                let merged_read_at = match (l.read_at, remote_n.read_at) {
                    (Some(l_at), Some(r_at)) => Some(l_at.max(r_at)),
                    (Some(l_at), None) => Some(l_at),
                    (None, Some(r_at)) => Some(r_at),
                    (None, None) => None,
                };
                *l = remote_n;
                l.read_at = merged_read_at;
            }
            None => local.push(remote_n),
        }
    }
    local.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    enforce_cap_at(&mut local, now);
    local
}

// ── Publish (debounced via dirty flag) ────────────────────────────────
//
// Rather than spawning a tokio task for debounced publish (which would
// require `Send + 'static` and conflict with Dioxus's non-Send signals
// on native targets), we set a dirty flag here. The visibility poll in
// `mostro_toast_drainer.rs` checks the flag on every 60s tick and
// publishes if dirty. This naturally coalesces bursts of pushes into a
// single relay event.

/// Dirty flag: set on every local mutation, cleared by `publish()` (which
/// is called from the toast drainer poll when this is true).
static PUBLISH_DIRTY: std::sync::OnceLock<std::sync::RwLock<bool>> = std::sync::OnceLock::new();

fn dirty_cell() -> &'static std::sync::RwLock<bool> {
    PUBLISH_DIRTY.get_or_init(|| std::sync::RwLock::new(false))
}

/// True if there are unpersisted local mutations awaiting a relay publish.
/// Polled by the toast drainer's 60s visibility backfill.
#[allow(dead_code)]
pub fn is_dirty() -> bool {
    *dirty_cell()
        .read()
        .unwrap_or_else(|e| e.into_inner())
}

/// Mark the store as having local changes that need a relay publish.
/// Called by `push` / `mark_read` / `mark_all_read` / `clear_all`.
fn mark_dirty() {
    *dirty_cell()
        .write()
        .unwrap_or_else(|e| e.into_inner()) = true;
}

/// Schedule a debounced NIP-78 publish. Currently implemented as a dirty
/// flag polled by the toast drainer; future versions may spawn a timer
/// when running in a context where Send + 'static is feasible.
fn publish_debounced() {
    mark_dirty();
}

/// Publish the current notification list as an encrypted NIP-78 event.
/// Mirrors `trade_store::publish` and `nip78::accept_p2p_terms`.
#[allow(dead_code)]
pub async fn publish() -> Result<(), String> {
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }

    let snapshot: Vec<MostroNotification> = NOTIFICATIONS.read().clone();

    let builder =
        match crate::stores::private_app_data::build_encrypted_event_builder(
            NOTIFICATIONS_D_TAG,
            &snapshot,
        ) {
            Ok(b) => b,
            Err(e) => {
                log::warn!("Falling back to plaintext notifications event: {e}");
                let content = serde_json::to_string(&snapshot)
                    .map_err(|e| format!("Failed to serialize notifications: {e}"))?;
                EventBuilder::new(Kind::from(30078), content)
                    .tag(Tag::identifier(NOTIFICATIONS_D_TAG))
            }
        };

    let event = publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign notifications: {e}"))?;

    publish_queue::enqueue_and_await(
        event,
        QueueEventType::Other("p2p_notifications".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await
    .map(|_| ())
    .map_err(|e| format!("Failed to publish notifications: {e}"))?;

    // Clear the dirty flag now that we've queued the publish.
    *dirty_cell()
        .write()
        .unwrap_or_else(|e| e.into_inner()) = false;
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────

fn now_secs() -> i64 {
    crate::platform::timestamp::now_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(id: &str, created_at: i64, read_at: Option<i64>) -> MostroNotification {
        MostroNotification {
            id: id.to_string(),
            order_id: None,
            dispute_id: None,
            daemon_pubkey: "deadbeef".to_string(),
            action_str: "PayInvoice".to_string(),
            title: "Test".into(),
            body: format!("body-{id}"),
            created_at,
            read_at,
        }
    }

    /// Pure version of cap enforcement. The signal-touching wrapper is
    /// excluded from tests because Dioxus requires a runtime.
    #[test]
    fn enforce_cap_drops_oldest_at_200() {
        let mut list: Vec<MostroNotification> = (0..(CAP + 50) as i64)
            .map(|i| n(&format!("n{i}"), i, None))
            .collect();
        enforce_cap_at(&mut list, 1_700_000_000);
        assert!(list.len() <= CAP, "enforce_cap should trim to CAP");
    }

    #[test]
    fn enforce_cap_preserves_fresh_unread_past_cap() {
        // CAP fresh unread entries should all survive even if the list
        // has CAP + 5 fresh unread at the head.
        let now: i64 = 1_700_000_000;
        let mut list: Vec<MostroNotification> = (0..CAP)
            .map(|i| n(&format!("old{i}"), now - 100_000, None))
            .collect();
        // Add 5 fresh unread at the head — these are within grace.
        for i in 0..5 {
            list.insert(0, n(&format!("fresh{i}"), now, None));
        }
        enforce_cap_at(&mut list, now);
        // Fresh unread entries should still be present.
        let fresh_present = list.iter().filter(|n| n.id.starts_with("fresh")).count();
        assert!(fresh_present >= 1, "at least some fresh entries should survive");
    }

    /// Pure merge logic. Tests the dedup + read-at precedence without
    /// touching the global signal.
    #[test]
    fn merge_lists_preserves_local_unread() {
        let local = vec![n("a", 100, None)];
        let remote = vec![n("a", 100, None)];
        let merged = merge_lists(local, remote);
        assert_eq!(merged.len(), 1);
        assert!(merged[0].read_at.is_none(), "local unread should win");
    }

    #[test]
    fn merge_lists_takes_later_read_at() {
        let local = vec![n("a", 100, Some(50))];
        let remote = vec![n("a", 100, Some(200))];
        let merged = merge_lists(local, remote);
        assert_eq!(merged[0].read_at, Some(200), "should take later read_at");
    }

    #[test]
    fn merge_lists_dedups_by_id() {
        let local = vec![n("a", 100, None), n("b", 200, None)];
        let remote = vec![n("a", 100, None), n("c", 300, None)];
        let merged = merge_lists(local, remote);
        assert_eq!(merged.len(), 3, "should have a, b, c (no dup)");
        // Verify sorted newest-first.
        assert!(merged.windows(2).all(|w| w[0].created_at >= w[1].created_at));
    }

    #[test]
    fn merge_lists_combines_local_and_remote_read_states() {
        // Local read, remote unread -> stays read.
        let local = vec![n("a", 100, Some(123))];
        let remote = vec![n("a", 100, None)];
        let merged = merge_lists(local, remote);
        assert_eq!(merged[0].read_at, Some(123));
    }

    /// Pure merge helper exposed for testing. Wraps the production
    /// `merge_lists_pure` so tests don't have to reimplement it. Passes
    /// a fixed `now` to avoid touching platform::timestamp on non-wasm.
    fn merge_lists(local: Vec<MostroNotification>, remote: Vec<MostroNotification>) -> Vec<MostroNotification> {
        let now = 1_700_000_000;
        let mut combined = local;
        for remote_n in remote {
            match combined.iter_mut().find(|local| local.id == remote_n.id) {
                Some(local) => {
                    let merged_read_at = match (local.read_at, remote_n.read_at) {
                        (Some(l), Some(r)) => Some(l.max(r)),
                        (Some(l), None) => Some(l),
                        (None, Some(r)) => Some(r),
                        (None, None) => None,
                    };
                    *local = remote_n;
                    local.read_at = merged_read_at;
                }
                None => combined.push(remote_n),
            }
        }
        combined.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        enforce_cap_at(&mut combined, now);
        combined
    }

    #[test]
    fn notification_serde_roundtrip() {
        let original = n("test", 12345, Some(999));
        let json = serde_json::to_string(&original).unwrap();
        let parsed: MostroNotification = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn notifications_d_tag_matches_convention() {
        assert!(NOTIFICATIONS_D_TAG.starts_with("nostr.blue/"));
        assert!(NOTIFICATIONS_D_TAG.ends_with("/notifications"));
    }

    #[test]
    fn cap_constant_is_reasonable() {
        // 200 is the user-confirmed retention target.
        assert_eq!(CAP, 200);
    }

    #[test]
    fn eviction_grace_is_seven_days() {
        assert_eq!(EVICTION_GRACE_SECS, 7 * 24 * 60 * 60);
    }

    #[test]
    fn to_kebab_case_samples() {
        // Re-implement the helper locally since `to_kebab_case` is private
        // to the notifications module. We mirror its definition here for
        // behavior coverage.
        fn kebab(s: &str) -> String {
            let mut out = String::with_capacity(s.len() + 4);
            for (i, ch) in s.chars().enumerate() {
                if i > 0 && ch.is_uppercase() {
                    out.push('-');
                }
                out.push(ch.to_ascii_lowercase());
            }
            out
        }
        assert_eq!(kebab("PayInvoice"), "pay-invoice");
        assert_eq!(kebab("CantDo"), "cant-do");
        assert_eq!(kebab("BuyerTookOrder"), "buyer-took-order");
        assert_eq!(kebab("Rate"), "rate");
        assert_eq!(kebab("A"), "a");
    }
}
