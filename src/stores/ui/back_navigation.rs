use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

pub static MOBILE_SIDEBAR_OPEN: GlobalSignal<bool> = Signal::global(|| false);
pub static MOBILE_SIDEBAR_PAGE: GlobalSignal<usize> = Signal::global(|| 0);
pub static RADIAL_MENU_OPEN: GlobalSignal<bool> = Signal::global(|| false);
pub static SIDEBAR_CUSTOMIZER_OPEN: GlobalSignal<bool> = Signal::global(|| false);
pub static MOBILE_SEARCH_OPEN: GlobalSignal<bool> = Signal::global(|| false);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveNoteBackContext {
    pub note_id: Option<String>,
    pub parent_note_ids: Vec<String>,
    pub is_voice_note: bool,
}

pub static ACTIVE_NOTE_BACK_CONTEXT: GlobalSignal<ActiveNoteBackContext> =
    Signal::global(ActiveNoteBackContext::default);

#[cfg_attr(not(feature = "mobile_platform"), allow(dead_code))]
static ANDROID_BACK_REQUESTS: AtomicU64 = AtomicU64::new(0);

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn request_android_back_from_platform() {
    ANDROID_BACK_REQUESTS.fetch_add(1, Ordering::SeqCst);
}

#[cfg_attr(not(feature = "mobile_platform"), allow(dead_code))]
pub fn platform_android_back_request_count() -> u64 {
    ANDROID_BACK_REQUESTS.load(Ordering::SeqCst)
}

#[cfg_attr(not(feature = "mobile_platform"), allow(dead_code))]
pub fn close_topmost_mobile_overlay() -> bool {
    if crate::stores::media::LIGHTBOX_STATE.read().is_open {
        crate::stores::media::close_lightbox();
        return true;
    }

    {
        let player = crate::stores::music_player::MUSIC_PLAYER.read();
        if player.is_visible
            && matches!(
                player.view_mode,
                crate::stores::music_player::PlayerViewMode::Expanded
            )
        {
            drop(player);
            crate::stores::music_player::set_view_mode(
                crate::stores::music_player::PlayerViewMode::Bar,
            );
            return true;
        }
    }

    if *SIDEBAR_CUSTOMIZER_OPEN.read() {
        *SIDEBAR_CUSTOMIZER_OPEN.write() = false;
        return true;
    }

    if *MOBILE_SEARCH_OPEN.read() {
        *MOBILE_SEARCH_OPEN.write() = false;
        return true;
    }

    if *MOBILE_SIDEBAR_OPEN.read() {
        if *MOBILE_SIDEBAR_PAGE.read() > 0 {
            *MOBILE_SIDEBAR_PAGE.write() = 0;
        } else {
            *MOBILE_SIDEBAR_OPEN.write() = false;
        }
        return true;
    }

    if *RADIAL_MENU_OPEN.read() {
        *RADIAL_MENU_OPEN.write() = false;
        return true;
    }

    false
}

pub fn set_active_note_back_context(
    note_id: String,
    parent_note_ids: Vec<String>,
    is_voice_note: bool,
) {
    *ACTIVE_NOTE_BACK_CONTEXT.write() = ActiveNoteBackContext {
        note_id: Some(note_id),
        parent_note_ids,
        is_voice_note,
    };
}

pub fn clear_active_note_back_context(note_id: &str) {
    let current = ACTIVE_NOTE_BACK_CONTEXT.read().clone();
    let note_matches = current.note_id.as_deref().is_some_and(|ctx_id| {
        crate::stores::nostr_client::parse_event_id(ctx_id).map(|p| p.event_id)
            == crate::stores::nostr_client::parse_event_id(note_id).map(|p| p.event_id)
    });
    if note_matches {
        *ACTIVE_NOTE_BACK_CONTEXT.write() = ActiveNoteBackContext::default();
    }
}
