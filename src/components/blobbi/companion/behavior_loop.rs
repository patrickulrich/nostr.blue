use dioxus::prelude::*;

use crate::components::blobbi::companion::companion_state::{
    now_ms, CompanionState, GazeMode, BLOBBI_COMPANION,
};
use crate::components::blobbi::core::types::BlobbiCompanion;

pub fn use_companion_behavior(blobbi: BlobbiCompanion) {
    let mut tick = use_signal(|| 0u64);

    use_future(move || {
        let blobbi = blobbi.clone();
        async move {
            loop {
                crate::platform::timer::sleep_ms(16).await;
                tick.set(tick() + 1);

                if tick() % 60 == 1 {
                    update_viewport_width().await;
                }

                let state = {
                    let c = BLOBBI_COMPANION.read();
                    c.companion_state
                };

                if matches!(
                    state,
                    CompanionState::Dragging | CompanionState::MenuOpen | CompanionState::Sleeping
                ) {
                    update_float();
                    continue;
                }

                update_physics_step(&blobbi);
                update_wandering(&blobbi);
                update_gaze();
                update_float();
            }
        }
    });
}

async fn update_viewport_width() {
    let js = "return window.innerWidth || document.documentElement.clientWidth || 360";
    if let Ok(width) = dioxus::document::eval(js).await {
        if let Some(w) = width.as_f64() {
            BLOBBI_COMPANION.write().viewport_width = w as f32;
        }
    }
}

fn update_physics_step(blobbi: &BlobbiCompanion) {
    let mut c = BLOBBI_COMPANION.write();
    let dt = 0.016;
    let state = c.companion_state;

    match state {
        CompanionState::Walking => {
            let speed = c.physics.walk_speed_min
                + (c.physics.walk_speed_max - c.physics.walk_speed_min)
                    * (blobbi.stats.energy as f32 / 100.0).min(1.0);
            let dx = c.target_x - c.x;
            let direction = dx.signum();
            let step = direction * speed * dt;

            if dx.abs() < 3.0 {
                if let Some(item_id) = c.pending_auto_use.take() {
                    let item_id_clone = item_id.clone();
                    c.companion_state = CompanionState::Idle;
                    drop(c);
                    spawn(async move {
                        if let Some(mut b) = crate::stores::blobbi_store::get_selected_blobbi() {
                            match crate::components::blobbi::shop::inventory_modal::use_item_on_blobbi_public(&mut b, &item_id_clone).await {
                                Ok(()) => {
                                    crate::stores::blobbi_store::update_blobbi_in_collection(&b);
                                }
                                Err(e) => {
                                    log::error!("Auto-use failed: {}", e);
                                }
                            }
                        }
                    });
                    return;
                } else if c.observation_target.is_some() {
                    c.companion_state = CompanionState::Watching;
                } else {
                    c.companion_state = CompanionState::Idle;
                }
            } else {
                c.x += step;
                c.facing_right = direction > 0.0;
            }
        }
        CompanionState::EntryFall => {
            c.velocity_y += c.physics.gravity * dt;
            c.y += c.velocity_y * dt;

            if c.y >= c.floor_y {
                c.y = c.floor_y;
                c.velocity_y = -c.velocity_y * c.physics.bounce;

                if c.velocity_y.abs() < 15.0 {
                    c.velocity_y = 0.0;
                    let hash = (now_ms().wrapping_mul(2654435761) >> 16) as f32 / 65536.0;
                    let min_x = 40.0_f32;
                    let max_x = (c.viewport_width - 40.0).max(min_x + 50.0);
                    c.target_x = min_x + hash * (max_x - min_x);
                    c.companion_state = CompanionState::Walking;
                    c.walk_start_ms = now_ms();
                    c.eye_gaze_target = Some((200.0, 150.0));
                }
            }
        }
        CompanionState::EntryRise => {
            if c.y >= c.floor_y {
                c.y = c.floor_y;
                c.velocity_y = 0.0;
                let hash = (now_ms().wrapping_mul(2654435761) >> 16) as f32 / 65536.0;
                let min_x = 40.0_f32;
                let max_x = (c.viewport_width - 40.0).max(min_x + 50.0);
                c.target_x = min_x + hash * (max_x - min_x);
                c.companion_state = CompanionState::Walking;
                c.walk_start_ms = now_ms();
                c.eye_gaze_target = Some((200.0, 150.0));
            }
        }
        CompanionState::Dragging => {
            c.velocity_y = 0.0;
            c.velocity_x = 0.0;
        }
        _ => {
            c.velocity_y *= c.physics.drag;
            c.velocity_x *= c.physics.drag;
        }
    }

    c.x = c.x.max(0.0);
}

fn update_wandering(_blobbi: &BlobbiCompanion) {
    let mut c = BLOBBI_COMPANION.write();
    let now = now_ms();
    let state = c.companion_state;

    if !matches!(state, CompanionState::Idle | CompanionState::Watching) {
        return;
    }

    let elapsed = now.saturating_sub(c.last_wander_ms);
    let idle_dur = c.physics.idle_duration_ms;

    if state == CompanionState::Watching {
        if elapsed >= c.physics.observation_duration_ms {
            c.companion_state = CompanionState::Idle;
            c.observation_target = None;
            c.last_wander_ms = now;
        }
        return;
    }

    if elapsed < idle_dur {
        return;
    }

    let min_x = 40.0_f32;
    let max_x = (c.viewport_width - 40.0).max(min_x + 50.0);

    let roll = (now % 100) as f32 / 100.0;

    if roll < 0.30 {
        let hash = (now.wrapping_mul(2654435761) >> 16) as f32 / 65536.0;
        c.target_x = min_x + hash * (max_x - min_x);
        c.companion_state = CompanionState::Walking;
        c.walk_start_ms = now;
        c.last_wander_ms = now;
    } else if roll < 0.30 + c.physics.observation_chance {
        let hash_x = min_x + ((now.wrapping_mul(1103515245) >> 16) as f32 / 65536.0) * (max_x - min_x);
        let hash_y = ((now.wrapping_mul(6364136223) >> 16) % 300) as f32 + 50.0;
        c.observation_target = Some((hash_x, hash_y));
        c.target_x = (hash_x - 40.0).max(min_x);
        c.companion_state = CompanionState::Walking;
        c.walk_start_ms = now;
        c.last_wander_ms = now;
    } else {
        c.last_wander_ms = now;
    }
}

fn update_gaze() {
    let mut c = BLOBBI_COMPANION.write();
    let now = now_ms();
    let state = c.companion_state;

    let target = match state {
        CompanionState::Watching => {
            c.gaze_mode = GazeMode::ObserveTarget;
            c.observation_target
        }
        CompanionState::Walking => {
            c.gaze_mode = GazeMode::Forward;
            let dx = if c.facing_right { 200.0 } else { -200.0 };
            Some((c.x + dx, c.y - 50.0))
        }
        _ => {
            if now < c.mouse_follow_until_ms {
                c.gaze_mode = GazeMode::FollowMouse;
                c.mouse_pos
            } else if c.eye_gaze_target.is_some() {
                c.gaze_mode = GazeMode::AttendUi;
                c.eye_gaze_target
            } else {
                c.gaze_mode = GazeMode::Random;
                None
            }
        }
    };

    let smooth = match c.gaze_mode {
        GazeMode::EntryInspect => 0.20,
        GazeMode::AttendUi => 0.18,
        GazeMode::ObserveTarget => 0.12,
        GazeMode::Forward => 0.12,
        GazeMode::FollowMouse => 0.15,
        GazeMode::Random => 0.06,
    };

    if let Some((tx, ty)) = target {
        let dx = tx - c.x;
        let dy = ty - c.y;
        let max_h = 350.0_f32;
        let max_v_up = 350.0_f32;
        let max_v_down = 500.0_f32;
        let ox = (dx / max_h).clamp(-1.0, 1.0);
        let oy = if dy < 0.0 {
            (dy / max_v_up).clamp(-1.0, 0.0)
        } else {
            (dy / max_v_down).clamp(0.0, 1.0)
        };

        c.eye_offset.x += (ox - c.eye_offset.x) * smooth;
        c.eye_offset.y += (oy - c.eye_offset.y) * smooth;
    } else {
        if now.saturating_sub(c.last_gaze_ms) > c.physics.gaze_interval_ms {
            let roll = (now % 100) as f32 / 100.0;
            if roll < c.physics.gaze_follow_chance
                && now.saturating_sub(c.last_mouse_follow_ms) > 4000
                && c.mouse_pos.is_some()
            {
                c.mouse_follow_until_ms = now + c.physics.gaze_follow_duration_ms;
                c.last_mouse_follow_ms = now;
                return;
            }
            let hash = (now.wrapping_mul(2246822519) >> 16) as f32 / 65536.0;
            let rx = (hash - 0.5) * 2.0 * 0.8;
            let ry = ((now.wrapping_mul(3367900313) >> 16) as f32 / 65536.0 - 0.5) * 1.1 - 0.05;
            c.eye_offset.x += (rx - c.eye_offset.x) * smooth;
            c.eye_offset.y += (ry - c.eye_offset.y) * smooth;
            c.last_gaze_ms = now;
        }
    }
}

fn update_float() {
    let mut c = BLOBBI_COMPANION.write();
    c.float_time += 0.016;
    let t = c.float_time;
    let state = c.companion_state;

    let disabled = matches!(
        state,
        CompanionState::Dragging
            | CompanionState::Sleeping
            | CompanionState::EntryFall
            | CompanionState::EntryRise
            | CompanionState::MenuOpen
    );

    if disabled {
        c.float_y *= 0.9;
        c.float_x *= 0.9;
        c.float_rotation *= 0.9;
        return;
    }

    let is_moving = matches!(state, CompanionState::Walking | CompanionState::Wander);

    if is_moving {
        let amp = c.physics.bob_amplitude * 1.5;
        let y_bob = (t * 12.0).sin() * amp * 0.7 + (t * 5.0).sin() * amp * 0.3;
        let x_sway = (t * 6.0).sin() * amp * 0.4 + (t * 2.5).sin() * amp * 0.15;
        let rot_tilt = (t * 6.0).sin() * 1.5 + (t * 2.2).sin() * 0.5;
        c.float_y = y_bob;
        c.float_x = x_sway;
        c.float_rotation = rot_tilt;
    } else {
        let amp = c.physics.bob_amplitude;
        let breathe = (t * 1.2).sin() * amp * 0.5
            + (t * 0.7).sin() * amp * 0.3
            + (t * 2.1).sin() * amp * 0.2;
        let drift_x = (t * 0.8).sin() * amp * 0.2 + (t * 0.4).sin() * amp * 0.1;
        c.float_y = breathe;
        c.float_x = drift_x;
        c.float_rotation *= 0.95;
    }
}

#[allow(dead_code)]
pub fn trigger_attention(duration_ms: u64) {
    let mut c = BLOBBI_COMPANION.write();
    c.attention_until_ms = now_ms() + duration_ms;
    c.companion_state = CompanionState::Attention;
}

#[allow(dead_code)]
pub fn trigger_walk_to(x: f32) {
    let mut c = BLOBBI_COMPANION.write();
    c.target_x = x;
    c.companion_state = CompanionState::Walking;
    c.walk_start_ms = now_ms();
}

#[allow(dead_code)]
pub fn set_gaze_target(x: f32, y: f32) {
    BLOBBI_COMPANION.write().eye_gaze_target = Some((x, y));
}

#[allow(dead_code)]
pub fn clear_gaze_target() {
    BLOBBI_COMPANION.write().eye_gaze_target = None;
}
