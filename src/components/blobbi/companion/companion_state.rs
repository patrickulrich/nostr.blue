use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GazeMode {
    #[default]
    Random,
    EntryInspect,
    AttendUi,
    ObserveTarget,
    Forward,
    FollowMouse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CompanionState {
    #[default]
    Idle,
    Wander,
    Attention,
    Sleeping,
    Dragging,
    MenuOpen,
    React,
    Follow,
    Walking,
    Watching,
    EntryFall,
    EntryRise,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum EntryPhase {
    #[default]
    None,
    Stuck,
    Pulling1,
    Pause1,
    Pulling2,
    Pause2,
    Falling,
    Landing,
    Complete,
    Rising,
    Inspecting,
    Entering,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EyeOffset {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub struct PhysicsConfig {
    pub gravity: f32,
    pub bounce: f32,
    pub drag: f32,
    pub walk_speed_min: f32,
    pub walk_speed_max: f32,
    pub wander_interval_ms: u64,
    pub attention_bounce_height: f32,
    pub bob_amplitude: f32,
    pub bob_period_ms: u64,
    pub wander_target_min_x: f32,
    pub wander_target_max_x: f32,
    pub walk_duration_ms: u64,
    pub idle_duration_ms: u64,
    pub observation_chance: f32,
    pub observation_duration_ms: u64,
    pub gaze_follow_chance: f32,
    pub gaze_follow_duration_ms: u64,
    pub gaze_interval_ms: u64,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: 3500.0,
            bounce: 0.3,
            drag: 0.95,
            walk_speed_min: 20.0,
            walk_speed_max: 80.0,
            wander_interval_ms: 3000,
            attention_bounce_height: 8.0,
            bob_amplitude: 2.0,
            bob_period_ms: 2500,
            wander_target_min_x: 80.0,
            wander_target_max_x: 330.0,
            walk_duration_ms: 3000,
            idle_duration_ms: 7000,
            observation_chance: 0.25,
            observation_duration_ms: 4500,
            gaze_follow_chance: 0.35,
            gaze_follow_duration_ms: 2500,
            gaze_interval_ms: 4000,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub struct CompanionData {
    pub visible: bool,
    pub x: f32,
    pub y: f32,
    pub companion_state: CompanionState,
    pub physics: PhysicsConfig,
    pub velocity_y: f32,
    pub velocity_x: f32,
    pub entered: bool,
    pub target_x: f32,
    pub facing_right: bool,
    pub entry_phase: EntryPhase,
    pub entry_rise: bool,
    pub eye_offset: EyeOffset,
    pub eye_gaze_target: Option<(f32, f32)>,
    pub last_wander_ms: u64,
    pub last_gaze_ms: u64,
    pub observation_target: Option<(f32, f32)>,
    pub walk_start_ms: u64,
    pub attention_until_ms: u64,
    pub float_time: f32,
    pub floor_y: f32,
    pub mouse_pos: Option<(f32, f32)>,
    pub mouse_follow_until_ms: u64,
    pub last_mouse_follow_ms: u64,
    pub float_y: f32,
    pub float_x: f32,
    pub float_rotation: f32,
    pub gaze_mode: GazeMode,
    pub last_mouse_write_ms: u64,
    pub state_before_attention: Option<CompanionState>,
    pub viewport_width: f32,
    pub pending_auto_use: Option<String>,
}

pub static BLOBBI_COMPANION: GlobalSignal<CompanionData> = Signal::global(|| CompanionData {
    floor_y: 200.0,
    facing_right: true,
    ..CompanionData::default()
});

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

pub fn set_companion_position(x: f32, y: f32) {
    let mut companion = BLOBBI_COMPANION.write();
    companion.x = x;
    companion.y = y;
}

pub fn mark_entered() {
    BLOBBI_COMPANION.write().entered = true;
}

#[allow(dead_code)]
pub fn set_eye_offset(x: f32, y: f32) {
    let mut c = BLOBBI_COMPANION.write();
    c.eye_offset = EyeOffset { x, y };
}

#[allow(dead_code)]
pub fn set_target_x(x: f32) {
    let mut c = BLOBBI_COMPANION.write();
    c.target_x = x;
    if x > c.x {
        c.facing_right = true;
    } else if x < c.x {
        c.facing_right = false;
    }
}

pub fn now_ms() -> u64 {
    crate::platform::timestamp::now_millis()
}
