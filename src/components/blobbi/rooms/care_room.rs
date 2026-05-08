use dioxus::prelude::*;

use crate::components::blobbi::actions::care_actions::execute_blobbi_action;
use crate::components::blobbi::actions::BlobbiActionType;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::rooms::item_carousel::{CarouselEntry, ItemCarousel};
use crate::components::blobbi::rooms::room_action_button::RoomActionButton;
use crate::components::blobbi::rooms::room_hero::RoomHero;
use crate::components::blobbi::shop::shop_items;
use crate::stores::blobbi_store;

#[derive(Clone, Debug, Default, PartialEq)]
enum CareCategory {
    #[default]
    Hygiene,
    Medicine,
}

#[component]
pub fn CareRoom(blobbi: BlobbiCompanion) -> Element {
    let acting = use_signal(|| false);
    let focused_category = use_signal(CareCategory::default);
    let mut show_shop = use_signal(|| false);
    let carousel_items = build_care_carousel();

    let left_action = match focused_category() {
        CareCategory::Hygiene => Some(("🧴", "Towel", BlobbiActionType::Clean)),
        CareCategory::Medicine => Some(("🍬", "Treat", BlobbiActionType::Medicine)),
    };

    let right_action = match focused_category() {
        CareCategory::Hygiene => Some(("🚿", "Shower", BlobbiActionType::Clean)),
        CareCategory::Medicine => None,
    };

    let reaction = crate::components::blobbi::rooms::reaction_state::reaction_string();

    rsx! {
        div { class: "flex flex-col min-h-full",
            RoomHero { blobbi: blobbi.clone(), reaction: reaction.clone() }

            div { class: "flex-1" }

            // Bottom bar
            div { class: "px-4 pb-4 pt-3 border-t border-border/50",
                div { class: "flex items-center justify-between gap-2",

                    // Left side action
                    if let Some((icon, label, action)) = left_action {
                        {
                            let mut acting = acting;
                            rsx! {
                                RoomActionButton {
                                    icon: rsx! { span { class: "text-2xl sm:text-3xl", "{icon}" } },
                                    label: label.to_string(),
                                    color: "bg-green-500/10".to_string(),
                                    glow_hex: "#22c55e".to_string(),
                                    onclick: move |_| {
                                        let action = action;
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
                                    disabled: Some(acting()),
                                }
                            }
                        }
                    } else {
                        div { class: "w-14 sm:w-20" }
                    }

                    // Center — Hygiene + Medicine carousel
                    div { class: "flex-1 min-w-0",
                        ItemCarousel {
                            items: carousel_items,
                            on_use: {
                                let mut acting = acting;
                                move |id: String| {
                                    let action = if id.starts_with("hyg_") {
                                        BlobbiActionType::Clean
                                    } else {
                                        BlobbiActionType::Medicine
                                    };
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
                                }
                            },
                            active_item_id: None,
                            disabled: acting(),
                            on_focus_change: {
                                let mut focused_category = focused_category;
                                move |entry: CarouselEntry| {
                                    if entry.id.starts_with("med_") {
                                        focused_category.set(CareCategory::Medicine);
                                    } else {
                                        focused_category.set(CareCategory::Hygiene);
                                    }
                                }
                            },
                        }
                    }

                    // Right side action
                    if let Some((icon, label, action)) = right_action {
                        {
                            let mut acting = acting;
                            rsx! {
                                RoomActionButton {
                                    icon: rsx! { span { class: "text-2xl sm:text-3xl", "{icon}" } },
                                    label: label.to_string(),
                                    color: "bg-blue-500/10".to_string(),
                                    glow_hex: "#3b82f6".to_string(),
                                    onclick: move |_| {
                                        let action = action;
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
                                    disabled: Some(acting()),
                                }
                            }
                        }
                    } else {
                        div { class: "w-14 sm:w-20" }
                    }
                }
            }

            // Shop button
            div { class: "px-4 pb-3",
                button {
                    class: "w-full flex items-center justify-center gap-1.5 py-2 rounded-xl bg-yellow-500/10 border border-yellow-500/20 text-yellow-500 text-sm font-medium hover:bg-yellow-500/20 transition",
                    onclick: move |_| show_shop.set(true),
                    span { "🏪" }
                    span { "Shop" }
                }
            }
        }

        if show_shop() {
            {
                let mut show_shop = show_shop;
                rsx! {
                    crate::components::blobbi::shop::shop_modal::ShopModal {
                        on_close: move |_| show_shop.set(false),
                    }
                }
            }
        }
    }
}

fn build_care_carousel() -> Vec<CarouselEntry> {
    let mut items: Vec<CarouselEntry> = Vec::new();
    let hygiene = shop_items::items_by_category(shop_items::ItemCategory::Hygiene);
    for item in &hygiene {
        items.push(CarouselEntry {
            id: item.id.to_string(),
            label: item.name.to_string(),
            icon: Some(item.icon.to_string()),
            meta: None,
        });
    }
    let medicine = shop_items::items_by_category(shop_items::ItemCategory::Medicine);
    for item in &medicine {
        items.push(CarouselEntry {
            id: item.id.to_string(),
            label: item.name.to_string(),
            icon: Some(item.icon.to_string()),
            meta: None,
        });
    }
    items
}
