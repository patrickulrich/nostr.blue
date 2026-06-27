//! Debounced save infrastructure for unified preference blobs.
//!
//! Two separate queues:
//! - **Main blob**: 2 s debounce (signer round-trip is expensive on
//!   NIP-07/46/55). Coalesces rapid edits into one encrypt + publish.
//! - **Mostro blob**: 500 ms debounce (sync encryption, no signer
//!   round-trip, so we can afford a tighter window).
//!
//! Both queues support [`flush_pending`] for:
//! - flush-on-route-leave (mounted from Layout via `use_drop`)
//! - flush-on-logout (before clearing auth state)

use std::sync::OnceLock;
use std::time::Duration;

use dioxus::prelude::*;
use nostr::EventBuilder;
use tokio::sync::Mutex;

use crate::stores::user_prefs::blob::UserPrefsBlob;
use crate::stores::user_prefs::mostro_blob::MostroPrefsBlob;

// ─── Debounce state ─────────────────────────────────────────────────────

#[derive(Default)]
struct PendingSave<T> {
    /// True while a save is in-flight. New snapshots wait in `latest`.
    in_flight: bool,
    /// The most recent snapshot waiting to be saved.
    latest: Option<(T, String)>, // (blob, d_tag)
}

static MAIN_QUEUE: OnceLock<Mutex<PendingSave<UserPrefsBlob>>> = OnceLock::new();
static MOSTRO_QUEUE: OnceLock<Mutex<PendingSave<MostroPrefsBlob>>> = OnceLock::new();

fn main_queue() -> &'static Mutex<PendingSave<UserPrefsBlob>> {
    MAIN_QUEUE.get_or_init(|| Mutex::new(PendingSave::default()))
}

fn mostro_queue() -> &'static Mutex<PendingSave<MostroPrefsBlob>> {
    MOSTRO_QUEUE.get_or_init(|| Mutex::new(PendingSave::default()))
}

/// Debounce delay for the main blob (async signer encrypt).
pub const MAIN_DEBOUNCE: Duration = Duration::from_millis(2000);
/// Debounce delay for the Mostro blob (sync encrypt).
pub const MOSTRO_DEBOUNCE: Duration = Duration::from_millis(500);

// ─── Queue operations ───────────────────────────────────────────────────

/// Enqueue a snapshot of the main blob for save. Returns `true` if this
/// is the first pending save (caller should start the debounce timer).
pub async fn enqueue_main(blob: UserPrefsBlob) -> bool {
    let mut q = main_queue().lock().await;
    q.latest = Some((blob, crate::stores::user_prefs::PREFS_D_TAG.to_string()));
    if q.in_flight {
        false
    } else {
        q.in_flight = true;
        true
    }
}

/// Enqueue a snapshot of the Mostro blob for save.
pub async fn enqueue_mostro(blob: MostroPrefsBlob) -> bool {
    let mut q = mostro_queue().lock().await;
    q.latest = Some((blob, crate::stores::user_prefs::MOSTRO_PREFS_D_TAG.to_string()));
    if q.in_flight {
        false
    } else {
        q.in_flight = true;
        true
    }
}

/// Take the next pending main blob snapshot, or clear `in_flight` if none.
pub async fn take_main() -> Option<UserPrefsBlob> {
    let mut q = main_queue().lock().await;
    if let Some((blob, _)) = q.latest.take() {
        Some(blob)
    } else {
        q.in_flight = false;
        None
    }
}

/// Take the next pending Mostro blob snapshot, or clear `in_flight`.
pub async fn take_mostro() -> Option<MostroPrefsBlob> {
    let mut q = mostro_queue().lock().await;
    if let Some((blob, _)) = q.latest.take() {
        Some(blob)
    } else {
        q.in_flight = false;
        None
    }
}

// ─── Publish helpers ────────────────────────────────────────────────────

/// Serialize, encrypt (main signer), sign, and enqueue the main blob.
/// Also updates `LAST_PUBLISHED_EVENT_ID` for phantom-decrypt prevention.
pub async fn publish_main(blob: &UserPrefsBlob) -> Result<(), String> {
    let builder = crate::stores::user_prefs::encrypt::build_encrypted_signer_event_builder(
        crate::stores::user_prefs::PREFS_D_TAG,
        blob,
    )
    .await?;
    publish_and_track(builder, &crate::stores::user_prefs::LAST_PUBLISHED_EVENT_ID).await
}

/// Serialize, encrypt (Mostro key), sign, and enqueue the Mostro blob.
pub async fn publish_mostro(blob: &MostroPrefsBlob) -> Result<(), String> {
    let builder = crate::stores::user_prefs::encrypt::build_encrypted_mostro_event_builder(
        crate::stores::user_prefs::MOSTRO_PREFS_D_TAG,
        blob,
    )?;
    publish_and_track(
        builder,
        &crate::stores::user_prefs::LAST_PUBLISHED_MOSTRO_EVENT_ID,
    )
    .await
}

/// Sign an event builder via the publish queue and record the event id
/// in the provided `last_published` signal for phantom-decrypt prevention.
async fn publish_and_track(
    builder: EventBuilder,
    last_published: &GlobalSignal<Option<nostr::EventId>>,
) -> Result<(), String> {
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder).await?;
    *last_published.write() = Some(event.id);
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("user_prefs".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await;
    Ok(())
}

// ─── Flush (for route-leave + logout) ───────────────────────────────────

/// Flush any pending main blob save immediately (bypassing the debounce).
pub async fn flush_main() {
    if let Some(blob) = take_main().await {
        if let Err(e) = publish_main(&blob).await {
            log::warn!("flush_main: publish failed: {e}");
        }
    }
}

/// Flush any pending Mostro blob save immediately.
pub async fn flush_mostro() {
    if let Some(blob) = take_mostro().await {
        if let Err(e) = publish_mostro(&blob).await {
            log::warn!("flush_mostro: publish failed: {e}");
        }
    }
}

/// Flush both pending saves (convenience for logout).
pub async fn flush_all() {
    let ((), ()) = tokio::join!(flush_main(), flush_mostro());
}
