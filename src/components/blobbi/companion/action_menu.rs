use dioxus::prelude::*;

use crate::components::blobbi::actions::care_actions::execute_blobbi_action;
use crate::components::blobbi::actions::BlobbiActionType;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::hooks::blobbi::use_blobbi_sleep;
use crate::stores::blobbi_store;

const ARC_RADIUS: f32 = 85.0;
const ARC_DEGREES: f32 = 140.0;
const ARC_CENTER_DEG: f32 = 270.0;
const BUTTON_SIZE: f32 = 44.0;

struct MenuAction {
    action: Option<BlobbiActionType>,
    icon: &'static str,
    label: &'static str,
    is_sleep: bool,
}

fn menu_actions(is_egg: bool) -> Vec<MenuAction> {
    if is_egg {
        vec![
            MenuAction { action: Some(BlobbiActionType::Warm), icon: "🔥", label: "Warm", is_sleep: false },
            MenuAction { action: Some(BlobbiActionType::Check), icon: "🔍", label: "Check", is_sleep: false },
            MenuAction { action: Some(BlobbiActionType::Sing), icon: "🎤", label: "Sing", is_sleep: false },
        ]
    } else {
        vec![
            MenuAction { action: Some(BlobbiActionType::Feed), icon: "🍔", label: "Feed", is_sleep: false },
            MenuAction { action: Some(BlobbiActionType::Play), icon: "🎮", label: "Play", is_sleep: false },
            MenuAction { action: Some(BlobbiActionType::Clean), icon: "🧹", label: "Clean", is_sleep: false },
            MenuAction { action: Some(BlobbiActionType::Medicine), icon: "💊", label: "Med", is_sleep: false },
            MenuAction { action: None, icon: "🌙", label: "Sleep", is_sleep: true },
        ]
    }
}

fn arc_position(index: usize, total: usize) -> (f32, f32) {
    let start = ARC_CENTER_DEG - ARC_DEGREES / 2.0;
    let step = if total > 1 {
        ARC_DEGREES / (total - 1) as f32
    } else {
        0.0
    };
    let angle_deg = start + index as f32 * step;
    let angle_rad = angle_deg.to_radians();
    let x = ARC_RADIUS * angle_rad.cos();
    let y = ARC_RADIUS * angle_rad.sin();
    (x, y)
}

#[component]
pub fn ActionMenu(x: f32, y: f32, blobbi: BlobbiCompanion, on_close: EventHandler<()>) -> Element {
    let actions = menu_actions(blobbi.is_egg());
    let sleeping = blobbi.is_sleeping();
    let total = actions.len();

    rsx! {
        div {
            class: "fixed inset-0 z-[101]",
            onclick: move |_| on_close.call(()),

            div {
                class: "absolute",
                style: "left: {x}px; top: {y}px; transform: translate(-50%, -50%);",
                onclick: move |e| e.stop_propagation(),

                for (i, ma) in actions.iter().enumerate() {
                    {
                        let (ax, ay) = arc_position(i, total);
                        let delay_ms = i as u32 * 30;
                        let icon = ma.icon.to_string();
                        let label = ma.label.to_string();
                        let is_sleep = ma.is_sleep;
                        let action = ma.action;
                        let blobbi_clone = blobbi.clone();
                        let on_close_clone = on_close;

                        rsx! {
                            button {
                                key: "{i}",
                                class: "absolute flex flex-col items-center gap-0.5 rounded-xl hover:bg-accent transition animate-[blobbi-menu-pop_0.2s_ease-out]",
                                style: "left: {ax}px; top: {ay}px; width: {BUTTON_SIZE}px; height: {BUTTON_SIZE}px; transform: translate(-50%, -50%); animation-delay: {delay_ms}ms; animation-fill-mode: both;",

                                onclick: move |e| {
                                    e.stop_propagation();
                                    if is_sleep {
                                        toggle_blobbi_sleep_state();
                                    } else if let Some(act) = action {
                                        let b = blobbi_clone.clone();
                                        spawn(async move {
                                            match execute_blobbi_action(&b, act).await {
                                                Ok(updated) => blobbi_store::update_blobbi_in_collection(&updated),
                                                Err(e) => log::error!("Companion action failed: {}", e),
                                            }
                                        });
                                    }
                                    on_close_clone.call(());
                                },

                                div { class: "text-lg",
                                    if is_sleep && sleeping { "☀️" } else { "{icon}" }
                                }
                                span { class: "text-[7px] text-muted-foreground",
                                    if is_sleep && sleeping { "Wake" } else { "{label}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn toggle_blobbi_sleep_state() {
    if let Some(blobbi) = blobbi_store::get_selected_blobbi() {
        spawn(async move {
            let result = if blobbi.is_sleeping() {
                use_blobbi_sleep::wake_up(&blobbi).await
            } else {
                use_blobbi_sleep::put_to_sleep(&blobbi).await
            };
            if let Err(e) = result {
                log::error!("Sleep toggle failed: {}", e);
            }
        });
    }
}
