use dioxus::prelude::*;

use crate::components::blobbi::actions::care_actions::execute_blobbi_action;
use crate::components::blobbi::actions::BlobbiActionType;
use crate::components::blobbi::core::progression::ProgressionState;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::dev::dev_tools::DevTools;
use crate::components::blobbi::rooms::room_hero::RoomHero;
use crate::components::blobbi::shop::inventory_modal::InventoryModal;
use crate::components::blobbi::shop::shop_modal::ShopModal;
use crate::stores::blobbi_store;

#[component]
pub fn HomeRoom(blobbi: BlobbiCompanion) -> Element {
    let acting = use_signal(|| false);
    let mut show_shop = use_signal(|| false);
    let mut show_inventory = use_signal(|| false);

    let actions: Vec<BlobbiActionType> = if blobbi.is_egg() {
        vec![
            BlobbiActionType::Warm,
            BlobbiActionType::Check,
            BlobbiActionType::Sing,
            BlobbiActionType::Talk,
        ]
    } else if blobbi.is_baby() {
        vec![
            BlobbiActionType::Feed,
            BlobbiActionType::Play,
            BlobbiActionType::Clean,
            BlobbiActionType::Rest,
        ]
    } else {
        vec![
            BlobbiActionType::Feed,
            BlobbiActionType::Play,
            BlobbiActionType::Clean,
            BlobbiActionType::Rest,
            BlobbiActionType::Talk,
        ]
    };

    let progression = ProgressionState::compute(blobbi.experience, blobbi.care_streak);

    rsx! {
        div { class: "flex flex-col",
            RoomHero { blobbi: blobbi.clone() }

            div { class: "px-4 mt-2",
                div { class: "text-xs text-muted-foreground mb-2",
                    "Quick Actions"
                }
                div { class: "grid grid-cols-2 gap-2",
                    for action in actions {
                        {render_action_button(action, acting)}
                    }
                }
            }

            div { class: "px-4 mt-3 flex gap-2",
                button {
                    class: "flex-1 flex items-center justify-center gap-1.5 py-2 rounded-xl bg-yellow-500/10 border border-yellow-500/20 text-yellow-500 text-sm font-medium hover:bg-yellow-500/20 transition",
                    onclick: move |_| show_shop.set(true),
                    span { "🏪" }
                    span { "Shop" }
                }
                button {
                    class: "flex-1 flex items-center justify-center gap-1.5 py-2 rounded-xl bg-blue-500/10 border border-blue-500/20 text-blue-500 text-sm font-medium hover:bg-blue-500/20 transition",
                    onclick: move |_| show_inventory.set(true),
                    span { "🎒" }
                    span { "Items" }
                }
            }

            div { class: "px-4 mt-4",
                div { class: "flex items-center justify-between text-xs mb-1",
                    span { class: "text-muted-foreground",
                        "Level {progression.level}"
                    }
                    span { class: "text-muted-foreground",
                        "⭐ {progression.xp} XP"
                    }
                }
                div { class: "w-full h-2 bg-muted rounded-full overflow-hidden",
                    div {
                        class: "h-full bg-blue-500 rounded-full transition-all duration-500",
                        style: "width: {progression.level_progress_pct()}%",
                    }
                }
            }

            if blobbi.care_streak > 0 {
                div { class: "px-4 mt-3 flex items-center gap-2",
                    span { class: "text-sm", "🔥" }
                    span { class: "text-sm text-muted-foreground",
                        "{blobbi.care_streak} day streak"
                    }
                    if progression.streak_bonus_pct > 0.0 {
                        span { class: "text-[10px] text-green-500",
                            "+{progression.streak_bonus_pct:.0}%"
                        }
                    }
                }
            }

            DevTools { blobbi: blobbi.clone() }
        }

        if show_shop() {
            ShopModal {
                on_close: move |_| show_shop.set(false),
            }
        }

        if show_inventory() {
            InventoryModal {
                blobbi: blobbi.clone(),
                on_close: move |_| show_inventory.set(false),
            }
        }
    }
}

fn render_action_button(action: BlobbiActionType, mut acting: Signal<bool>) -> Element {
    let icon = action.icon();
    let label = action.label();
    let changes = action.stat_changes();
    let preview = changes
        .iter()
        .map(|(stat, delta)| {
            let c = stat
                .chars()
                .next()
                .unwrap_or('?')
                .to_uppercase()
                .next()
                .unwrap_or('?');
            if *delta >= 0.0 {
                format!("{}+{:.0}", c, delta)
            } else {
                format!("{}{:.0}", c, delta)
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    rsx! {
        button {
            class: "flex flex-col items-center gap-1 p-3 rounded-xl bg-card border border-border hover:bg-accent transition disabled:opacity-50",
            disabled: acting(),
            onclick: move |_| {
                acting.set(true);
                spawn(async move {
                    if let Some(blobbi) = blobbi_store::get_selected_blobbi() {
                        match execute_blobbi_action(&blobbi, action).await {
                            Ok(updated) => blobbi_store::update_blobbi_in_collection(&updated),
                            Err(e) => log::error!("Action failed: {}", e),
                        }
                    }
                    acting.set(false);
                });
            },
            span { class: "text-2xl", "{icon}" }
            span { class: "text-xs font-medium", "{label}" }
            span { class: "text-[10px] text-muted-foreground", "{preview}" }
        }
    }
}
