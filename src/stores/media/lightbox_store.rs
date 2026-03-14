use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightboxImage {
    pub url: String,
    pub alt: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LightboxState {
    pub is_open: bool,
    pub images: Vec<LightboxImage>,
    pub current_index: usize,
}

pub static LIGHTBOX_STATE: GlobalSignal<LightboxState> = Signal::global(LightboxState::default);

pub fn open_lightbox(images: Vec<LightboxImage>, index: usize) {
    if images.is_empty() {
        return;
    }

    let clamped_index = index.min(images.len().saturating_sub(1));
    *LIGHTBOX_STATE.write() = LightboxState {
        is_open: true,
        images,
        current_index: clamped_index,
    };
}

pub fn close_lightbox() {
    *LIGHTBOX_STATE.write() = LightboxState::default();
}

pub fn set_index(index: usize) {
    let mut state = LIGHTBOX_STATE.write();
    if state.images.is_empty() {
        state.current_index = 0;
        return;
    }
    state.current_index = index.min(state.images.len() - 1);
}

pub fn next_image() {
    let mut state = LIGHTBOX_STATE.write();
    if state.current_index + 1 < state.images.len() {
        state.current_index += 1;
    }
}

pub fn prev_image() {
    let mut state = LIGHTBOX_STATE.write();
    if state.current_index > 0 {
        state.current_index -= 1;
    }
}
