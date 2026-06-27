//! Per-room state for the active Nest (NIP-53 live audio room).
//!
//! Modeled on `MusicPlayerState` — a singleton `GlobalStore<T>` so audio
//! survives navigation across the app (mirrors how the music player keeps
//! playing when the user browses away from a track). Only one nest can be
//! active at a time (microphone capture is exclusive per app; PiP is one
//! room at a time; the Android foreground service is one notification).
//!
//! Action functions are colocated with state (matches `music_player.rs`).
//! Field-level surgical writes go through `NEST_ROOM.resolve().field().write()`
//! so background tasks (presence heartbeat, cliff detector, level poll) can
//! update one field without invalidating subscribers of sibling fields.

use crate::services::nests_audio::ConnectionState;
use crate::utils::nips::nip53::{MeetingSpace, RoomPresence};
use dioxus::prelude::*;
use dioxus_stores::Store;
use std::collections::{HashMap, HashSet};

/// Singleton state for the currently active nest room.
///
/// `naddr` / `coordinate` / `publisher_id` identify which room the state
/// belongs to. When the user navigates to a different room, `init_for_room`
/// replaces the whole state.
#[derive(Clone, Debug, PartialEq, Store)]
pub struct NestRoomState {
    // Identity (populated by `init_for_room`)
    pub naddr: String,
    pub coordinate: String,
    pub publisher_id: String,
    /// Effective relay set for the active room (NIP-65 ∪ naddr hints ∪
    /// room `relays` tag). Populated after `parse_meeting_space` succeeds.
    /// Used by all room subscriptions (presence, room updates, admin
    /// commands, chat) so edits and stage promotions on a room-specific
    /// relay are received. Empty until the room event loads.
    pub room_relays: Vec<String>,

    // Room data
    pub space: Option<MeetingSpace>,
    pub loading: bool,
    pub error: Option<String>,

    // Audio session
    pub is_joined: bool,
    pub is_muted: bool,
    pub is_publishing: bool,
    pub hand_raised: bool,
    pub onstage: bool,
    pub audio_error: Option<String>,
    pub connection_state: ConnectionState,
    /// Persists across reconnects once the user has been demoted from speaker.
    /// Cleared only when the host re-promotes the user (Phase 1.4 detects the
    /// role transition). Mirrors Amethyst's `declinedPublish`.
    pub declined_publish: bool,

    // Participants
    pub participants: Vec<RoomPresence>,
    pub subscribed_pubkeys: HashSet<String>,
    /// Remote pubkeys whose decoded audio level is currently above threshold.
    pub speaking_now: HashSet<String>,

    // Speaking detection (local mic)
    pub mic_level: f32,
    pub local_speaking: bool,

    // Cliff detector state. Instant isn't serde-friendly and the store is
    // never persisted, so we store unix seconds.
    pub last_frame_at: HashMap<String, f64>,
    pub cliff_backoff_step: u32,

    // UI ephemera
    pub show_host_leave_confirm: bool,
    /// Active tab in the room view. Mirrors Amethyst's
    /// `NestFullScreen.selectedTabIndex`. Stored on the singleton so the
    /// selection survives PiP transitions and component re-mounts.
    pub active_room_tab: RoomTab,
}

/// Tabs in the room view. Matches Amethyst's `NestTab` enum at
/// `NestFullScreen.kt:501-505`. Hands is host-only and only shown when
/// there's at least one raised hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RoomTab {
    #[default]
    Chat,
    Audience,
    Hands,
}

impl Default for NestRoomState {
    fn default() -> Self {
        Self {
            naddr: String::new(),
            coordinate: String::new(),
            publisher_id: String::new(),
            room_relays: Vec::new(),
            space: None,
            loading: true,
            error: None,
            is_joined: false,
            // Match prior behavior: enter the room muted by default so the
            // user must explicitly unmute. Avoids hot-mic on join.
            is_muted: true,
            is_publishing: false,
            hand_raised: false,
            onstage: false,
            audio_error: None,
            connection_state: ConnectionState::Disconnected,
            declined_publish: false,
            participants: Vec::new(),
            subscribed_pubkeys: HashSet::new(),
            speaking_now: HashSet::new(),
            mic_level: 0.0,
            local_speaking: false,
            last_frame_at: HashMap::new(),
            cliff_backoff_step: 0,
            show_host_leave_confirm: false,
            active_room_tab: RoomTab::Chat,
        }
    }
}

/// The singleton nest room state. Reads/writes follow the same pattern as
/// `MUSIC_PLAYER` (`src/stores/audio/music_player.rs:496`).
pub static NEST_ROOM: GlobalStore<NestRoomState> = Global::new(NestRoomState::default);

// Sibling task-handle signals. Pattern from `stores/ui/notifications.rs:16`:
// the store holds the state; task handles live in sibling globals so they can
// be cancelled on reset without coupling state writes to task lifecycle.
static HEARTBEAT_TASK: GlobalSignal<Option<dioxus_core::Task>> = Signal::global(|| None);
static CLIFF_TASK: GlobalSignal<Option<dioxus_core::Task>> = Signal::global(|| None);
static JWT_REFRESH_TASK: GlobalSignal<Option<dioxus_core::Task>> = Signal::global(|| None);
static LEVEL_POLL_TASK: GlobalSignal<Option<dioxus_core::Task>> = Signal::global(|| None);

/// Replace the whole state with defaults and cancel all background tasks.
/// Called when the viewer unmounts (`use_drop`) or when switching rooms.
#[allow(dead_code)]
pub fn reset() {
    for task_sig in [&HEARTBEAT_TASK, &CLIFF_TASK, &JWT_REFRESH_TASK, &LEVEL_POLL_TASK] {
        if let Some(t) = task_sig.write().take() {
            t.cancel();
        }
    }
    *NEST_ROOM.write() = NestRoomState::default();
}

/// Initialize state for a new room. Wipes whatever was there before.
/// Called from `NestViewer::use_effect` keyed on the naddr prop.
pub fn init_for_room(naddr: &str) {
    // Cancel any tasks tied to the previous room.
    for task_sig in [&HEARTBEAT_TASK, &CLIFF_TASK, &JWT_REFRESH_TASK, &LEVEL_POLL_TASK] {
        if let Some(t) = task_sig.write().take() {
            t.cancel();
        }
    }
    let parsed = crate::utils::nip19::parse_naddr(naddr).ok();
    let coordinate = parsed
        .as_ref()
        .map(|p| format!("{}:{}:{}", p.kind, p.pubkey, p.identifier))
        .unwrap_or_default();
    let my_pk = crate::stores::auth_store::get_pubkey().unwrap_or_default();
    let publisher_id = format!("nest-{}-{my_pk}", naddr);
    let mut state = NEST_ROOM.write();
    *state = NestRoomState::default();
    state.naddr = naddr.to_string();
    state.coordinate = coordinate;
    state.publisher_id = publisher_id;
}

/// Initialize the store for `naddr` only if the store isn't already holding
/// state for that exact room. Returns true if a (re)init happened.
///
/// This lets the user navigate away from the viewer and back without losing
/// audio session state (mirrors `MUSIC_PLAYER` surviving navigation).
pub fn ensure_initialized_for(naddr: &str) -> bool {
    let current = NEST_ROOM.read().naddr.clone();
    if current == naddr {
        return false;
    }
    init_for_room(naddr);
    true
}

// ---------------------------------------------------------------------------
// Surgical per-field setters (match MUSIC_PLAYER.resolve().field().write()).
// ---------------------------------------------------------------------------

pub fn set_space(s: Option<MeetingSpace>) {
    *NEST_ROOM.resolve().space().write() = s;
}

/// Replace the effective relay set for the active room. Computed by
/// `relay::effective_room_relays` after the room event loads; re-computed
/// reactively in `NestViewer` whenever the user's NIP-65 pool changes.
pub fn set_room_relays(relays: Vec<String>) {
    *NEST_ROOM.resolve().room_relays().write() = relays;
}

pub fn set_loading(v: bool) {
    *NEST_ROOM.resolve().loading().write() = v;
}

pub fn set_error(e: Option<String>) {
    *NEST_ROOM.resolve().error().write() = e;
}

pub fn set_joined(v: bool) {
    *NEST_ROOM.resolve().is_joined().write() = v;
}

pub fn set_muted(v: bool) {
    *NEST_ROOM.resolve().is_muted().write() = v;
}

pub fn set_publishing(v: bool) {
    *NEST_ROOM.resolve().is_publishing().write() = v;
}

pub fn set_hand_raised(v: bool) {
    *NEST_ROOM.resolve().hand_raised().write() = v;
}

pub fn set_onstage(v: bool) {
    *NEST_ROOM.resolve().onstage().write() = v;
}

pub fn set_audio_error(e: Option<String>) {
    *NEST_ROOM.resolve().audio_error().write() = e;
}

#[allow(dead_code)]
pub fn set_connection_state(cs: ConnectionState) {
    *NEST_ROOM.resolve().connection_state().write() = cs;
}

pub fn set_declined_publish(v: bool) {
    *NEST_ROOM.resolve().declined_publish().write() = v;
}

pub fn set_show_host_leave_confirm(v: bool) {
    *NEST_ROOM.resolve().show_host_leave_confirm().write() = v;
}

pub fn set_active_room_tab(tab: RoomTab) {
    *NEST_ROOM.resolve().active_room_tab().write() = tab;
}

pub fn set_mic_level(v: f32) {
    *NEST_ROOM.resolve().mic_level().write() = v;
}

pub fn set_local_speaking(v: bool) {
    *NEST_ROOM.resolve().local_speaking().write() = v;
}

/// Upsert a participant by pubkey (replaces existing entry).
pub fn upsert_participant(p: RoomPresence) {
    let mut binding = NEST_ROOM.resolve().participants();
    let mut list = binding.write();
    if let Some(idx) = list.iter().position(|x| x.pubkey == p.pubkey) {
        list[idx] = p;
    } else {
        list.push(p);
    }
}

/// Replace the entire participant list.
#[allow(dead_code)]
pub fn set_participants(v: Vec<RoomPresence>) {
    *NEST_ROOM.resolve().participants().write() = v;
}

pub fn mark_subscribed(pk: String) {
    NEST_ROOM.resolve().subscribed_pubkeys().write().insert(pk);
}

pub fn mark_unsubscribed(pk: &str) {
    NEST_ROOM
        .resolve()
        .subscribed_pubkeys()
        .write()
        .remove(pk);
}

pub fn record_frame(pubkey: String, unix_secs: f64) {
    NEST_ROOM
        .resolve()
        .last_frame_at()
        .write()
        .insert(pubkey, unix_secs);
}

pub fn set_cliff_backoff_step(v: u32) {
    *NEST_ROOM.resolve().cliff_backoff_step().write() = v;
}

pub fn mark_speaking(pubkey: String) {
    NEST_ROOM.resolve().speaking_now().write().insert(pubkey);
}

pub fn mark_not_speaking(pubkey: &str) {
    NEST_ROOM
        .resolve()
        .speaking_now()
        .write()
        .remove(pubkey);
}

// ---------------------------------------------------------------------------
// Background-task registration. Used by `nest_viewer.rs` and the audio layer.
// ---------------------------------------------------------------------------

pub fn set_heartbeat_task(t: dioxus_core::Task) {
    if let Some(old) = HEARTBEAT_TASK.write().replace(t) {
        old.cancel();
    }
}

pub fn set_cliff_task(t: dioxus_core::Task) {
    if let Some(old) = CLIFF_TASK.write().replace(t) {
        old.cancel();
    }
}

pub fn set_jwt_refresh_task(t: dioxus_core::Task) {
    if let Some(old) = JWT_REFRESH_TASK.write().replace(t) {
        old.cancel();
    }
}

pub fn set_level_poll_task(t: dioxus_core::Task) {
    if let Some(old) = LEVEL_POLL_TASK.write().replace(t) {
        old.cancel();
    }
}

/// Cancel just the heartbeat task (used when leaving audio without unmounting).
#[allow(dead_code)]
pub fn cancel_heartbeat() {
    if let Some(t) = HEARTBEAT_TASK.write().take() {
        t.cancel();
    }
}
