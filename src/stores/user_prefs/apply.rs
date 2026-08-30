//! Apply a remote blob to local GlobalSignals with three-layer change
//! suppression:
//!
//! 1. **Event-id dedup**: skip entirely if the event id matches the
//!    last-applied id (prevents redundant decrypt + apply when the same
//!    event arrives via multiple paths — bootstrap + subscription + echo).
//! 2. **Phantom-decrypt prevention**: skip decrypt when the event id
//!    matches `LAST_PUBLISHED_EVENT_ID` (our own just-published event
//!    echoing back via the persistent subscription — would cause a phantom
//!    signer prompt on NIP-07/46/55).
//! 3. **Per-field diff**: the caller's `apply_fn` writes to individual
//!    GlobalSignals; the diff is done at the signal level (each signal's
//!    subscribers are only notified if the value actually changes).
//!
//! All apply operations are guarded by a `tokio::sync::Mutex<()>` to close
//! the bootstrap race explicitly (it can otherwise be won by the fast path
//! while the relay fetch is still in flight).

use std::sync::OnceLock;

use dioxus::prelude::*;
use nostr::EventId;
use tokio::sync::Mutex;

use crate::stores::user_prefs::Nip78LoadState;

/// Mutex guarding the decrypt → apply critical section.
static APPLY_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

fn apply_mutex() -> &'static Mutex<()> {
    APPLY_MUTEX.get_or_init(|| Mutex::new(()))
}

/// Source of a blob application — used for logging and cache decisions.
#[derive(Clone, Debug, PartialEq)]
pub enum BlobSource {
    /// localStorage cache bootstrap (instant UI, may be stale).
    Cache,
    /// nostr-sdk local database query (no network).
    LocalDb,
    /// Relay fetch result (authoritative for replaceable events).
    Relay,
    /// Live subscription delivery (cross-device realtime sync).
    LiveSubscription,
}

/// Guard struct returned by [`check_and_lock`] — ensures the dedup check
/// and the apply happen within the same mutex scope.
pub struct ApplyGuard {
    _guard: tokio::sync::MutexGuard<'static, ()>,
    event_id: EventId,
    source: BlobSource,
}

/// Check whether this event should be applied, and acquire the mutex if so.
///
/// Returns `None` if the event should be skipped (dedup or phantom-self).
/// Returns `Some(guard)` if the event should be applied.
///
/// Callers must pass the event_id and the appropriate dedup/pub signals.
pub async fn check_and_lock(
    event_id: EventId,
    source: BlobSource,
    last_applied: &GlobalSignal<Option<EventId>>,
    last_published: &GlobalSignal<Option<EventId>>,
) -> Option<ApplyGuard> {
    // Layer 1: event-id dedup (same event arriving via multiple paths).
    if last_applied.peek().as_ref() == Some(&event_id) {
        log::debug!(
            "apply_blob: skipping duplicate event {event_id} (already applied)"
        );
        return None;
    }
    // Layer 2: phantom-decrypt prevention (our own published event
    // echoing back via the subscription). For Cache/LocalDb/Relay sources
    // this check is irrelevant; only LiveSubscription sees our own echo.
    if source == BlobSource::LiveSubscription {
        if let Some(ref published_id) = *last_published.peek() {
            if published_id == &event_id {
                log::debug!(
                    "apply_blob: skipping self-published event {event_id} \
                     (phantom-decrypt prevention)"
                );
                return None;
            }
        }
    }
    let guard = apply_mutex().lock().await;
    Some(ApplyGuard {
        _guard: guard,
        event_id,
        source,
    })
}

impl ApplyGuard {
    /// The event id being applied.
    pub fn event_id(&self) -> EventId {
        self.event_id
    }

    /// The source of this blob.
    pub fn source(&self) -> &BlobSource {
        &self.source
    }
}

/// Mark a blob as successfully applied — update the last-applied event-id
/// signal and the load-state.
pub fn mark_applied(
    guard: ApplyGuard,
    last_applied: &GlobalSignal<Option<EventId>>,
    state: &GlobalSignal<Nip78LoadState>,
) {
    *last_applied.write() = Some(guard.event_id);
    *state.write() = Nip78LoadState::Loaded;
}

/// Mark the load as failed (network error, parse error, etc.).
pub fn mark_failed(
    error: impl Into<String>,
    state: &GlobalSignal<Nip78LoadState>,
) {
    let msg = error.into();
    log::warn!("NIP-78 load failed: {msg}");
    *state.write() = Nip78LoadState::Failed(msg);
}
