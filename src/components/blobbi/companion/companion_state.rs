use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CompanionState {
    #[default]
    Idle,
    Attention,
    Sleeping,
    Dragging,
    MenuOpen,
}

#[derive(Clone, Debug, Default)]
pub struct CompanionData {
    pub visible: bool,
    pub x: f32,
    pub y: f32,
    pub companion_state: CompanionState,
}

pub static BLOBBI_COMPANION: GlobalSignal<CompanionData> = Signal::global(CompanionData::default);

pub fn companion_visible() -> bool {
    BLOBBI_COMPANION.read().visible
}

pub fn set_companion_visible(visible: bool) {
    BLOBBI_COMPANION.write().visible = visible;
}

pub fn set_companion_state(state: CompanionState) {
    BLOBBI_COMPANION.write().companion_state = state;
}

#[allow(dead_code)]
pub fn toggle_companion_sleep() {
    let current = BLOBBI_COMPANION.read().companion_state;
    let new_state = match current {
        CompanionState::Sleeping => CompanionState::Idle,
        _ => CompanionState::Sleeping,
    };
    BLOBBI_COMPANION.write().companion_state = new_state;
}
