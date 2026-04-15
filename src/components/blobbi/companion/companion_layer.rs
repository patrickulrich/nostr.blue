use dioxus::prelude::*;

use crate::components::blobbi::companion::action_menu::ActionMenu;
use crate::components::blobbi::companion::companion_state::{CompanionState, BLOBBI_COMPANION};
use crate::components::blobbi::companion::companion_visual::CompanionVisual;
use crate::components::blobbi::companion::hanging_items::HangingItems;
use crate::stores::blobbi_store;

const REST_X: f32 = 16.0;
const REST_Y: f32 = 200.0;

#[component]
pub fn CompanionLayer() -> Element {
    let blobbi = blobbi_store::get_selected_blobbi();
    let companion = BLOBBI_COMPANION.read();
    let state = companion.companion_state;

    if !companion.visible || blobbi.is_none() {
        return rsx! { div {} };
    }
    let blobbi = blobbi.unwrap();

    let mut pos_x = use_signal(|| {
        let x = companion.x;
        if x == 0.0 {
            REST_X
        } else {
            x
        }
    });
    let mut pos_y = use_signal(|| {
        let y = companion.y;
        if y == 0.0 {
            REST_Y
        } else {
            y
        }
    });
    let mut dragging = use_signal(|| false);
    let mut drag_offset = use_signal(|| (0.0_f32, 0.0_f32));
    let mut show_menu = use_signal(|| false);

    let x = pos_x();
    let y = pos_y();

    rsx! {
        div {
            class: "fixed z-[100] select-none",
            style: "left: {x}px; top: {y}px; touch-action: none;",

            onpointerdown: move |evt: Event<PointerData>| {
                dragging.set(true);
                let cx = evt.data().client_coordinates().x as f32;
                let cy = evt.data().client_coordinates().y as f32;
                drag_offset.set((cx - pos_x(), cy - pos_y()));
            },

            onpointermove: move |evt: Event<PointerData>| {
                if dragging() {
                    let cx = evt.data().client_coordinates().x as f32;
                    let cy = evt.data().client_coordinates().y as f32;
                    let (ox, oy) = drag_offset();
                    let new_x = (cx - ox).max(0.0);
                    let new_y = (cy - oy).max(0.0);
                    pos_x.set(new_x);
                    pos_y.set(new_y);
                    BLOBBI_COMPANION.write().x = new_x;
                    BLOBBI_COMPANION.write().y = new_y;
                    if state != CompanionState::Dragging {
                        crate::components::blobbi::companion::companion_state::set_companion_state(CompanionState::Dragging);
                    }
                }
            },

            onpointerup: move |_| {
                if dragging() {
                    dragging.set(false);
                    crate::components::blobbi::companion::companion_state::set_companion_state(CompanionState::Idle);
                }
            },

            onclick: move |_| {
                if !dragging() {
                    show_menu.set(!show_menu());
                }
            },

            div { class: "cursor-grab active:cursor-grabbing",
                CompanionVisual { blobbi: blobbi.clone(), state }
            }
        }

        if show_menu() {
            ActionMenu {
                x,
                y,
                blobbi: blobbi.clone(),
                on_close: move |_| show_menu.set(false),
            }
        }

        HangingItems { blobbi: blobbi.clone() }
    }
}
