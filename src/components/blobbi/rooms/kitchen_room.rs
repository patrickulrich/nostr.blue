use dioxus::prelude::*;

use crate::components::blobbi::actions::care_actions::execute_blobbi_action;
use crate::components::blobbi::actions::BlobbiActionType;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::rooms::item_carousel::{CarouselEntry, ItemCarousel};
use crate::components::blobbi::rooms::room_action_button::RoomActionButton;
use crate::components::blobbi::rooms::room_hero::RoomHero;
use crate::components::blobbi::rooms::poop_system::PoopState;
use crate::components::blobbi::shop::inventory_modal::InventoryModal;
use crate::components::blobbi::shop::shop_items;
use crate::stores::blobbi_store;

#[component]
pub fn KitchenRoom(blobbi: BlobbiCompanion) -> Element {
    let acting = use_signal(|| false);
    let mut show_fridge = use_signal(|| false);
    let poop_state = use_signal(PoopState::default);

    let food_items = build_food_carousel();
    let has_poops = poop_state.read().has_any();

    let reaction = crate::components::blobbi::rooms::reaction_state::reaction_string();

    rsx! {
        div { class: "flex flex-col min-h-full",
            RoomHero { blobbi: blobbi.clone(), reaction: reaction.clone() }

            // Poop overlays
            for poop in poop_state.read().poops_in_room("kitchen") {
                {
                    let poop_id = poop.id.clone();
                    let on_remove = {
                        let mut poop_state = poop_state;
                        move |id: String| {
                            poop_state.write().remove(&id);
                        }
                    };
                    rsx! {
                        crate::components::blobbi::rooms::poop_system::PoopButton {
                            key: "{poop_id}",
                            poop: poop.clone(),
                            shovel_mode: true,
                            on_remove: on_remove,
                        }
                    }
                }
            }

            div { class: "flex-1" }

            // Bottom bar
            div { class: "px-4 pb-4 pt-3 border-t border-border/50",
                div { class: "flex items-center justify-between gap-2",

                    // Left — Shovel (only if poops exist)
                    if has_poops {
                        RoomActionButton {
                            icon: rsx! { span { class: "text-2xl sm:text-3xl", "🧹" } },
                            label: "Shovel".to_string(),
                            color: "bg-amber-500/10".to_string(),
                            glow_hex: "#f59e0b".to_string(),
                            onclick: {
                                let mut poop_state = poop_state;
                                move |_| {
                                    let count = poop_state.read().poops.len() as u64;
                                    poop_state.write().poops.clear();
                                    if count > 0 {
                                        let xp = count.saturating_mul(2);
                                        spawn(async move {
                                            if let Some(mut blobbi) = blobbi_store::get_selected_blobbi() {
                                                blobbi.experience = blobbi.experience.saturating_add(xp);
                                                let _ = crate::components::blobbi::core::builders::publish_blobbi_state(&blobbi).await;
                                                blobbi_store::update_blobbi_in_collection(&blobbi);
                                            }
                                        });
                                    }
                                }
                            },
                            disabled: Some(false),
                        }
                    } else {
                        div { class: "w-14 sm:w-20" }
                    }

                    // Center — Food carousel
                    div { class: "flex-1 min-w-0",
                        ItemCarousel {
                            items: food_items,
                            on_use: {
                                let mut acting = acting;
                                move |_id: String| {
                                    acting.set(true);
                                    spawn(async move {
                                        if let Some(blobbi) = blobbi_store::get_selected_blobbi() {
                                            match execute_blobbi_action(&blobbi, BlobbiActionType::Feed).await {
                                                Ok(updated) => {
                                                    blobbi_store::update_blobbi_in_collection(&updated);
                                                    let mut ps = PoopState::default();
                                                    ps.maybe_generate(
                                                        updated.stats.hunger,
                                                        updated.last_meal,
                                                        nostr_sdk::Timestamp::now().as_secs(),
                                                        "kitchen",
                                                    );
                                                }
                                                Err(e) => log::error!("Feed failed: {}", e),
                                            }
                                        }
                                        acting.set(false);
                                    });
                                }
                            },
                            active_item_id: None,
                            disabled: acting(),
                            on_focus_change: None,
                        }
                    }

                    // Right — Fridge / Inventory
                    RoomActionButton {
                        icon: rsx! { span { class: "text-2xl sm:text-3xl", "🧊" } },
                        label: "Fridge".to_string(),
                        color: "bg-cyan-500/10".to_string(),
                        glow_hex: "#06b6d4".to_string(),
                        onclick: move |_| show_fridge.set(true),
                        disabled: Some(false),
                    }
                }
            }
        }

        if show_fridge() {
            {
                let mut show_fridge = show_fridge;
                rsx! {
                    InventoryModal {
                        blobbi: blobbi.clone(),
                        on_close: move |_| show_fridge.set(false),
                    }
                }
            }
        }
    }
}

fn build_food_carousel() -> Vec<CarouselEntry> {
    shop_items::items_by_category(shop_items::ItemCategory::Food)
        .iter()
        .map(|item| CarouselEntry {
            id: item.id.to_string(),
            label: item.name.to_string(),
            icon: Some(item.icon.to_string()),
            meta: None,
        })
        .collect()
}
