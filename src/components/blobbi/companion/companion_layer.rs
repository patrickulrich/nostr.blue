use dioxus::prelude::*;
use dioxus_motion::prelude::*;

use crate::components::blobbi::companion::action_menu::ActionMenu;
use crate::components::blobbi::companion::attention::use_ui_attention;
use crate::components::blobbi::companion::typing_attention::use_typing_attention;
use crate::components::blobbi::companion::behavior_loop::use_companion_behavior;
use crate::components::blobbi::companion::companion_state::{
    set_companion_position, set_companion_state, set_companion_visible, CompanionState,
    BLOBBI_COMPANION,
};
use crate::components::blobbi::companion::companion_visual::CompanionVisual;
use crate::components::blobbi::companion::hanging_items::HangingItems;
use crate::stores::blobbi_store;

const REST_X: f32 = 16.0;
const REST_Y: f32 = 200.0;
const DRAG_THRESHOLD_PX: f32 = 5.0;
const CLICK_THRESHOLD_MS: u64 = 300;

#[component]
pub fn CompanionLayer() -> Element {
    let blobbi = blobbi_store::get_selected_blobbi();
    let (visible, companion_x, companion_y, state, entered) = {
        let c = BLOBBI_COMPANION.read();
        (c.visible, c.x, c.y, c.companion_state, c.entered)
    };

    if !visible || blobbi.is_none() {
        return rsx! { div {} };
    }
    let blobbi = blobbi.unwrap();

    use_companion_behavior(blobbi.clone());

    let init_x = if companion_x == 0.0 { REST_X } else { companion_x };
    let init_y = if companion_y == 0.0 { REST_Y } else { companion_y };

    let mut position = use_motion(Transform::new(init_x, init_y, 1.0, 0.0));
    let mut dragging = use_signal(|| false);
    let mut drag_offset = use_signal(|| (0.0_f32, 0.0_f32));
    let mut show_menu = use_signal(|| false);
    let mut pointer_start = use_signal(|| (0.0_f32, 0.0_f32, 0u64));
    let mut hovered = use_signal(|| false);

    if !entered {
        let should_rise = {
            let c = BLOBBI_COMPANION.read();
            c.entry_rise
        };
        if should_rise {
            {
                let mut pos = position;
                let ix = init_x;
                let iy = init_y;
                spawn(async move {
                    let below_y = iy + 500.0;

                    pos.animate_to(
                        Transform::new(ix, below_y, 1.0, 0.0),
                        AnimationConfig::new(AnimationMode::Tween(
                            Tween::new(Duration::from_millis(1)),
                        )),
                    );
                    crate::platform::timer::sleep_ms(80).await;

                    let rise_y = iy + (500.0 * 0.35);
                    pos.animate_to(
                        Transform::new(ix, rise_y, 1.0, 0.0),
                        AnimationConfig::new(AnimationMode::Tween(
                            Tween::new(Duration::from_millis(700)),
                        )),
                    );
                    crate::platform::timer::sleep_ms(750).await;

                    let dirs = [(200.0, 50.0), (-200.0, 50.0), (0.0, -80.0)];
                    for (gx, gy) in dirs {
                        crate::components::blobbi::companion::behavior_loop::set_gaze_target(ix + gx, rise_y + gy);
                        crate::platform::timer::sleep_ms(400).await;
                        crate::components::blobbi::companion::behavior_loop::clear_gaze_target();
                        crate::platform::timer::sleep_ms(150).await;
                    }

                    pos.animate_to(
                        Transform::new(ix, iy, 1.0, 0.0),
                        AnimationConfig::new(AnimationMode::Spring(Spring {
                            stiffness: 150.0,
                            damping: 14.0,
                            mass: 1.0,
                            velocity: 0.0,
                        })),
                    );
                });
            }
            set_companion_state(CompanionState::EntryRise);
        } else {
            {
                let mut pos = position;
                let ix = init_x;
                let iy = init_y;
                spawn(async move {
                    let stuck_y = iy - 250.0;
                    pos.animate_to(
                        Transform::new(ix, stuck_y, 1.0, 0.0),
                        AnimationConfig::new(AnimationMode::Tween(
                            Tween::new(Duration::from_millis(1)),
                        )),
                    );
                    crate::platform::timer::sleep_ms(80).await;

                    pos.animate_to(
                        Transform::new(ix, stuck_y + 8.0, 1.05, 1.5),
                        AnimationConfig::new(AnimationMode::Tween(
                            Tween::new(Duration::from_millis(300)),
                        )),
                    );
                    crate::platform::timer::sleep_ms(350).await;

                    pos.animate_to(
                        Transform::new(ix, stuck_y + 20.0, 1.08, -2.0),
                        AnimationConfig::new(AnimationMode::Tween(
                            Tween::new(Duration::from_millis(250)),
                        )),
                    );
                    crate::platform::timer::sleep_ms(300).await;

                    pos.animate_to(
                        Transform::new(ix, iy, 1.0, 0.0),
                        AnimationConfig::new(AnimationMode::Spring(Spring {
                            stiffness: 180.0,
                            damping: 10.0,
                            mass: 1.2,
                            velocity: 800.0,
                        })),
                    );
                    crate::platform::timer::sleep_ms(600).await;

                    pos.animate_to(
                        Transform::new(ix, iy, 1.0, 0.0),
                        AnimationConfig::new(AnimationMode::Spring(Spring {
                            stiffness: 200.0,
                            damping: 18.0,
                            mass: 1.0,
                            velocity: 0.0,
                        })),
                    );
                });
            }
            set_companion_state(CompanionState::EntryFall);
        }
        crate::components::blobbi::companion::companion_state::mark_entered();
    }

    let ui_attention = use_ui_attention();
    let typing_attention = use_typing_attention();

    {
        let ui_att = ui_attention;
        let typ_att = typing_attention;
        #[allow(clippy::redundant_closure)]
        use_effect(move || {
            let target = typ_att().or_else(|| ui_att());
            if let Some((ax, ay)) = target {
                crate::components::blobbi::companion::behavior_loop::set_gaze_target(ax, ay);
            }
        });
    }

    {
        let typ_att = typing_attention;
        let needs_attention = crate::components::blobbi::companion::need_detection::has_any_need(&blobbi);

        use_effect(move || {
            let is_typing = typ_att().is_some();
            let wants_attention = is_typing || needs_attention;
            if wants_attention
                && state != CompanionState::Dragging
                && state != CompanionState::MenuOpen
                && state != CompanionState::Attention
                && !(needs_attention && !is_typing
                    && matches!(state, CompanionState::Walking | CompanionState::Watching))
            {
                {
                    let mut c = BLOBBI_COMPANION.write();
                    c.state_before_attention = Some(state);
                }
                set_companion_state(CompanionState::Attention);
            } else if !wants_attention && state == CompanionState::Attention {
                let prev = BLOBBI_COMPANION.write().state_before_attention;
                set_companion_state(prev.unwrap_or(CompanionState::Idle));
                BLOBBI_COMPANION.write().state_before_attention = None;
            }
        });
    }

    let pos = position.current();
    let x = pos().x;
    let y = pos().y;
    let scale = pos().scale;
    let rotation = pos().rotation;

    rsx! {
        div {
            class: "fixed z-[100] select-none",
            style: "left: {x}px; top: {y}px; transform: scale({scale}) rotate({rotation}deg); touch-action: none; will-change: transform;",

            onpointerdown: move |evt: Event<PointerData>| {
                dragging.set(true);
                let cx = evt.data().client_coordinates().x as f32;
                let cy = evt.data().client_coordinates().y as f32;
                let (ox, oy) = (cx - pos().x, cy - pos().y);
                drag_offset.set((ox, oy));
                let now_ms = crate::platform::timestamp::now_millis();
                pointer_start.set((cx, cy, now_ms));
            },

            onpointermove: move |evt: Event<PointerData>| {
                let cx = evt.data().client_coordinates().x as f32;
                let cy = evt.data().client_coordinates().y as f32;
                {
                    let mut c = BLOBBI_COMPANION.write();
                    let now = crate::platform::timestamp::now_millis();
                    if now.saturating_sub(c.last_mouse_write_ms) >= 32 {
                        c.mouse_pos = Some((cx, cy));
                        c.last_mouse_write_ms = now;
                    }
                }
                if dragging() {
                    let cx = evt.data().client_coordinates().x as f32;
                    let cy = evt.data().client_coordinates().y as f32;
                    let (ox, oy) = drag_offset();
                    let new_x = (cx - ox).max(0.0);
                    let new_y = (cy - oy).max(0.0);
                    position.animate_to(
                        Transform::new(new_x, new_y, 1.05, 0.0),
                        AnimationConfig::new(AnimationMode::Tween(
                            Tween::new(Duration::from_millis(16)),
                        )),
                    );
                    set_companion_position(new_x, new_y);
                    if state != CompanionState::Dragging {
                        set_companion_state(CompanionState::Dragging);
                    }
                }
            },

            onpointerup: move |evt: Event<PointerData>| {
                if dragging() {
                    dragging.set(false);
                    let cx = evt.data().client_coordinates().x as f32;
                    let cy = evt.data().client_coordinates().y as f32;
                    let (sx, sy, st) = pointer_start();
                    let dist = ((cx - sx).powi(2) + (cy - sy).powi(2)).sqrt();
                    let now_ms = crate::platform::timestamp::now_millis();
                    let elapsed = now_ms.saturating_sub(st);

                    if dist < DRAG_THRESHOLD_PX && elapsed < CLICK_THRESHOLD_MS {
                        show_menu.set(!show_menu());
                        set_companion_state(CompanionState::MenuOpen);
                    } else {
                        position.animate_to(
                            Transform::new(pos().x, pos().y, 1.0, 0.0),
                            AnimationConfig::new(AnimationMode::Spring(Spring {
                                stiffness: 180.0,
                                damping: 15.0,
                                mass: 1.0,
                                velocity: 0.0,
                            })),
                        );
                        set_companion_state(CompanionState::Idle);
                    }
                }
            },

            onmouseenter: move |_| {
                hovered.set(true);
            },

            onmouseleave: move |_| {
                hovered.set(false);
            },

            {
                let cd = BLOBBI_COMPANION.read();
                let float_y = cd.float_y;
                let float_x = cd.float_x;
                let float_rot = cd.float_rotation;
                rsx! {
                    div {
                        class: "relative cursor-grab active:cursor-grabbing",
                        style: "transform: translate({float_x}px, {float_y}px) rotate({float_rot}deg); transition: transform 16ms linear;",
                        CompanionVisual { blobbi: blobbi.clone(), state }

                        if hovered() && !dragging() {
                            button {
                                class: "absolute -top-2 -right-2 w-5 h-5 bg-muted border border-border rounded-full flex items-center justify-center text-[10px] text-muted-foreground hover:bg-accent transition cursor-pointer",
                                onclick: move |evt| {
                                    evt.stop_propagation();
                                    set_companion_visible(false);
                                },
                                "×"
                            }
                        }
                    }
                }
            }
            }

        if show_menu() {
            ActionMenu {
                x,
                y,
                blobbi: blobbi.clone(),
                on_close: move |_| {
                    show_menu.set(false);
                    set_companion_state(CompanionState::Idle);
                },
            }
        }

        HangingItems { blobbi: blobbi.clone() }
    }
}
