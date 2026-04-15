use dioxus::prelude::*;

use crate::components::blobbi::actions::care_actions::execute_blobbi_action;
use crate::components::blobbi::actions::BlobbiActionType;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::stores::blobbi_store;
use crate::utils::nip_bb::BlobbiState;

#[component]
pub fn ActionMenu(x: f32, y: f32, blobbi: BlobbiCompanion, on_close: EventHandler<()>) -> Element {
    let actions: Vec<BlobbiActionType> = if blobbi.is_egg() {
        vec![
            BlobbiActionType::Warm,
            BlobbiActionType::Check,
            BlobbiActionType::Sing,
        ]
    } else {
        vec![
            BlobbiActionType::Feed,
            BlobbiActionType::Play,
            BlobbiActionType::Clean,
            BlobbiActionType::Rest,
        ]
    };

    let menu_x = x;
    let menu_y = (y - 80.0).max(0.0);
    let sleeping = blobbi.is_sleeping();

    rsx! {
        div {
            class: "fixed inset-0 z-[101]",
            onclick: move |_| on_close.call(()),

            div {
                class: "absolute bg-card border border-border rounded-2xl shadow-2xl p-3 flex gap-2",
                style: "left: {menu_x}px; top: {menu_y}px; transform: translateX(-25%);",
                onclick: move |e| e.stop_propagation(),

                for action in &actions {
                    {render_action_button(*action, blobbi.clone(), on_close)}
                }

                button {
                    class: "flex flex-col items-center gap-0.5 p-2 rounded-xl hover:bg-accent transition",
                    onclick: {
                        move |_| {
                            toggle_blobbi_sleep_state();
                            on_close.call(());
                        }
                    },
                    span { class: "text-lg",
                        if sleeping { "\u{2600}\u{FE0F}" } else { "\u{1F319}" }
                    }
                    span { class: "text-[8px] text-muted-foreground",
                        if sleeping { "Wake" } else { "Sleep" }
                    }
                }
            }
        }
    }
}

fn toggle_blobbi_sleep_state() {
    if let Some(mut blobbi) = blobbi_store::get_selected_blobbi() {
        blobbi.state = if blobbi.is_sleeping() {
            BlobbiState::Active
        } else {
            BlobbiState::Sleeping
        };
        let b = blobbi.clone();
        spawn(async move {
            let _ = crate::components::blobbi::core::builders::publish_blobbi_state(&b).await;
        });
        blobbi_store::update_blobbi_in_collection(&blobbi);
    }
}

fn render_action_button(
    action: BlobbiActionType,
    blobbi: BlobbiCompanion,
    on_close: EventHandler<()>,
) -> Element {
    let icon = action.icon();
    let label = action.label();

    rsx! {
        button {
            class: "flex flex-col items-center gap-0.5 p-2 rounded-xl hover:bg-accent transition",
            onclick: {
                let blobbi = blobbi.clone();
                move |_| {
                    let b = blobbi.clone();
                    spawn(async move {
                        match execute_blobbi_action(&b, action).await {
                            Ok(updated) => blobbi_store::update_blobbi_in_collection(&updated),
                            Err(e) => log::error!("Companion action failed: {}", e),
                        }
                    });
                    on_close.call(());
                }
            },
            span { class: "text-lg", "{icon}" }
            span { class: "text-[8px] text-muted-foreground", "{label}" }
        }
    }
}
