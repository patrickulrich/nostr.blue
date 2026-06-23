//! Unified NIP-78 encrypted preference blobs.
//!
//! Consolidates ~11 separate kind 30078 d-tags into two encrypted blobs:
//!
//! - [`blob::UserPrefsBlob`] at `nostr.blue/prefs` — encrypted via NIP-44 to
//!   self using the **main signer** (async). Contains app settings, sidebar
//!   layout, reaction presets, AI credentials, notification read-pointer,
//!   and terms-acceptance flags.
//!
//! - [`mostro_blob::MostroPrefsBlob`] at `nostr.blue/p2p` — encrypted via
//!   NIP-44 to self using the **Mostro identity key** (sync, always-local
//!   NIP-06 keypair — see `private_app_data.rs`). Contains Mostro settings,
//!   node config, and a bounded (last-50) trade history with archival
//!   spillover at `nostr.blue/p2p/trades-archive`.
//!
//! ## Architecture
//!
//! The module follows the amethyst reactive-pipeline pattern, adapted to
//! Dioxus signals:
//!
//! 1. **Cache bootstrap** (sync): read localStorage at boot for instant UI.
//! 2. **Relay fetch** (async, gated on `wait_for_user_relays`): fetch the
//!    blob from the user's NIP-65 outbox relays using quorum-EOSE early-exit.
//! 3. **Decrypt + apply**: event-id dedup → `tokio::sync::Mutex`-guarded
//!    decrypt → per-field diff into existing GlobalSignals.
//! 4. **Persistent subscription**: live updates for cross-device sync.
//! 5. **Debounced save**: 2 s coalescence window + flush-on-route-leave +
//!    flush-on-logout. Self-published events are tracked via
//!    `LAST_PUBLISHED_EVENT_ID` so the subscription handler can skip
//!    phantom-decrypt prompts on NIP-07/46/55.
//!
//! ## Encryption tracks
//!
//! The split between async (main signer) and sync (Mostro key) is forced by
//! the SDK: `nip44::encrypt_with_rng` needs a raw `SecretKey` (only the
//! Mostro identity key has one); external signers (NIP-07/46/55) must go
//! through the async `NostrSigner::nip44_encrypt` path.
//!
//! ## Migration
//!
//! Phase 1 (dual-read): the unified blob is read first; if absent, legacy
//! d-tags are fetched in parallel and assembled into a blob. Phase 2
//! (write migration): all saves write the unified blob. Phase 4 (cleanup):
//! legacy reads removed.

// Phase 0: foundational infrastructure not yet wired into the app.
// These items become live in Phase 1 (dual-read migration).
#![allow(dead_code)]

pub mod apply;
pub mod blob;
pub mod encrypt;
pub mod fetch;
pub mod mostro_blob;
pub mod save;
#[cfg(test)]
mod tests;

use dioxus::prelude::*;

/// Re-export `Nip78LoadState` for convenience. Defined in `sidebar_store`
/// today; will be hoisted here during Phase 4 cleanup.
pub use crate::stores::ui::sidebar_store::Nip78LoadState;

/// d-tag for the main unified preference blob.
pub const PREFS_D_TAG: &str = "nostr.blue/prefs";
/// d-tag for the Mostro unified preference blob.
pub const MOSTRO_PREFS_D_TAG: &str = "nostr.blue/p2p";
/// d-tag for archival trade spillover (overflow beyond the bounded 50).
pub const TRADES_ARCHIVE_D_TAG: &str = "nostr.blue/p2p/trades-archive";
/// Maximum number of trades kept in the active Mostro blob. Older trades
/// spill to [`TRADES_ARCHIVE_D_TAG`].
pub const MAX_RECENT_TRADES: usize = 50;

/// localStorage key prefix (namespaced with pubkey at runtime).
pub const PREFS_CACHE_PREFIX: &str = "nostr.blue/prefs/";
/// localStorage key prefix for Mostro blob (namespaced with pubkey).
pub const MOSTRO_PREFS_CACHE_PREFIX: &str = "nostr.blue/p2p/";

// ─── Global signals ─────────────────────────────────────────────────────

/// Event-id of the last event we successfully applied (from any source).
/// Used for event-id dedup in `apply::apply_if_newer`.
pub static USER_PREFS_EVENT_ID: GlobalSignal<Option<nostr::EventId>> = Signal::global(|| None);
/// Event-id of the most recent event *we* published. The subscription
/// handler checks this to skip phantom-decrypt prompts on NIP-07/46/55
/// (self-published events echo back via the subscription).
pub static LAST_PUBLISHED_EVENT_ID: GlobalSignal<Option<nostr::EventId>> = Signal::global(|| None);
/// Load-state for the main blob.
pub static USER_PREFS_STATE: GlobalSignal<Nip78LoadState> =
    Signal::global(Nip78LoadState::default);

/// Same signals for the Mostro blob.
pub static MOSTRO_PREFS_EVENT_ID: GlobalSignal<Option<nostr::EventId>> = Signal::global(|| None);
pub static LAST_PUBLISHED_MOSTRO_EVENT_ID: GlobalSignal<Option<nostr::EventId>> =
    Signal::global(|| None);
pub static MOSTRO_PREFS_LOAD_STATE: GlobalSignal<Nip78LoadState> =
    Signal::global(Nip78LoadState::default);
