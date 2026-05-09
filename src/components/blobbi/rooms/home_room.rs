use dioxus::prelude::*;

use crate::components::blobbi::actions::care_actions::execute_blobbi_action;
use crate::components::blobbi::actions::BlobbiActionType;
use crate::components::blobbi::core::progression::ProgressionState;
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::social::photo_modal::PhotoModal;
use crate::components::blobbi::social::post_modal::BlobbiPostModal;
use crate::components::blobbi::dev::dev_tools::DevTools;
use crate::components::blobbi::rooms::item_carousel::{CarouselEntry, ItemCarousel};
use crate::components::blobbi::rooms::room_action_button::RoomActionButton;
use crate::components::blobbi::rooms::room_hero::RoomHero;
use crate::components::blobbi::rooms::room_config::BlobbiRoomId;
use crate::components::blobbi::shop::shop_items;
use crate::stores::blobbi_store;

#[component]
pub fn HomeRoom(blobbi: BlobbiCompanion, on_navigate_to_room: EventHandler<BlobbiRoomId>) -> Element {
    let acting = use_signal(|| false);
    let mut show_shop = use_signal(|| false);
    let mut show_inventory = use_signal(|| false);
    let mut show_photo = use_signal(|| false);
    let mut show_post = use_signal(|| false);

    let progression = ProgressionState::compute(blobbi.experience, blobbi.care_streak);
    let is_egg = blobbi.is_egg();

    let carousel_items = build_carousel_items(&blobbi);

    let reaction = crate::components::blobbi::rooms::reaction_state::reaction_string();

    rsx! {
        div { class: "flex flex-col min-h-full",
            RoomHero { blobbi: blobbi.clone(), reaction: reaction.clone() }

            if is_egg {
                div { class: "mx-4 mt-3 p-3 rounded-xl bg-purple-500/10 border border-purple-500/20",
                    div { class: "flex items-center gap-2 mb-1.5",
                        span { class: "text-sm", "🥚" }
                        span { class: "text-sm font-medium text-purple-400", "Hatching Progress" }
                    }
                    p { class: "text-xs text-muted-foreground mb-2",
                        "Care for your egg and complete tasks to hatch it!"
                    }
                    if is_egg {
                        button {
                            class: "text-xs font-medium text-purple-400 hover:text-purple-300 transition",
                            onclick: move |_| on_navigate_to_room.call(BlobbiRoomId::Hatchery),
                            "Go to Hatchery →"
                        }
                    }
                }
            }

            div { class: "px-4 mt-2",
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

                if blobbi.care_streak > 0 {
                    div { class: "flex items-center gap-2 mt-2",
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
            }

            div { class: "flex-1" }

            div { class: "px-4 pb-4 pt-3 border-t border-border/50",
                div { class: "flex items-center justify-between gap-2",

                    if !is_egg {
                        RoomActionButton {
                            icon: rsx! { span { class: "text-2xl sm:text-3xl", "\u{1F4DD}" } },
                            label: "Post".to_string(),
                            color: "bg-green-500/10".to_string(),
                            glow_hex: "#22c55e".to_string(),
                            onclick: move |_| show_post.set(true),
                            disabled: Some(acting()),
                        }
                        RoomActionButton {
                            icon: rsx! { span { class: "text-2xl sm:text-3xl", "\u{1F4F8}" } },
                            label: "Photo".to_string(),
                            color: "bg-purple-500/10".to_string(),
                            glow_hex: "#a855f7".to_string(),
                            onclick: move |_| show_photo.set(true),
                            disabled: Some(acting()),
                        }
                    } else {
                        RoomActionButton {
                            icon: rsx! { span { class: "text-2xl sm:text-3xl", "🔥" } },
                            label: "Warm".to_string(),
                            color: "bg-orange-500/10".to_string(),
                            glow_hex: "#f97316".to_string(),
                            onclick: {
                                let mut acting = acting;
                                move |_| {
                                    acting.set(true);
                                    spawn(async move {
                                        if let Some(blobbi) = blobbi_store::get_selected_blobbi() {
                                            match execute_blobbi_action(&blobbi, BlobbiActionType::Warm).await {
                                                Ok(updated) => blobbi_store::update_blobbi_in_collection(&updated),
                                                Err(e) => log::error!("Action failed: {}", e),
                                            }
                                        }
                                        acting.set(false);
                                    });
                                }
                            },
                            disabled: Some(acting()),
                        }
                    }

                    div { class: "flex-1 min-w-0",
                        ItemCarousel {
                            items: carousel_items,
                            on_use: {
                                let mut acting = acting;
                                move |id: String| {
                                    let action = match id.as_str() {
                                        "warm" => BlobbiActionType::Warm,
                                        "clean" => BlobbiActionType::Clean,
                                        "check" => BlobbiActionType::Check,
                                        "music" => BlobbiActionType::PlayMusic,
                                        "sing" => BlobbiActionType::Sing,
                                        "talk" => BlobbiActionType::Talk,
                                        _ => BlobbiActionType::Play,
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
                            on_focus_change: None,
                        }
                    }

                    RoomActionButton {
                        icon: rsx! { span { class: "text-2xl sm:text-3xl", "🎒" } },
                        label: "Items".to_string(),
                        color: "bg-blue-500/10".to_string(),
                        glow_hex: "#3b82f6".to_string(),
                        onclick: move |_| show_inventory.set(true),
                        disabled: Some(false),
                    }
                }
            }

            div { class: "px-4 pb-3",
                button {
                    class: "w-full flex items-center justify-center gap-1.5 py-2 rounded-xl bg-yellow-500/10 border border-yellow-500/20 text-yellow-500 text-sm font-medium hover:bg-yellow-500/20 transition",
                    onclick: move |_| show_shop.set(true),
                    span { "🏪" }
                    span { "Shop" }
                }
            }

            DevTools { blobbi: blobbi.clone() }
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

        if show_inventory() {
            {
                let mut show_inventory = show_inventory;
                rsx! {
                    crate::components::blobbi::shop::inventory_modal::InventoryModal {
                        blobbi: blobbi.clone(),
                        on_close: move |_| show_inventory.set(false),
                    }
                }
            }
        }

        if show_photo() {
            {
                let mut show_photo = show_photo;
                rsx! {
                    PhotoModal {
                        blobbi: blobbi.clone(),
                        on_close: move |_| show_photo.set(false),
                    }
                }
            }
        }

        if show_post() {
            {
                let mut show_post = show_post;
                rsx! {
                    BlobbiPostModal {
                        blobbi: blobbi.clone(),
                        on_close: move |_| show_post.set(false),
                    }
                }
            }
        }
    }
}

fn build_carousel_items(blobbi: &BlobbiCompanion) -> Vec<CarouselEntry> {
    let mut items: Vec<CarouselEntry> = Vec::new();

    if blobbi.is_egg() {
        items.push(CarouselEntry {
            id: "sing".to_string(),
            label: "Sing".to_string(),
            icon: Some("\u{1F3A4}".to_string()),
            meta: Some("sing".to_string()),
        });
        items.push(CarouselEntry {
            id: "music".to_string(),
            label: "Music".to_string(),
            icon: Some("\u{1F3B5}".to_string()),
            meta: Some("music".to_string()),
        });
        items.push(CarouselEntry {
            id: "clean".to_string(),
            label: "Clean".to_string(),
            icon: Some("\u{1FAFB}".to_string()),
            meta: Some("clean".to_string()),
        });
        items.push(CarouselEntry {
            id: "talk".to_string(),
            label: "Talk".to_string(),
            icon: Some("\u{1F4AC}".to_string()),
            meta: Some("talk".to_string()),
        });
    } else {
        let toys = shop_items::items_by_category(shop_items::ItemCategory::Toy);
        for toy in &toys {
            items.push(CarouselEntry {
                id: toy.id.to_string(),
                label: toy.name.to_string(),
                icon: Some(toy.icon.to_string()),
                meta: None,
            });
        }
        items.push(CarouselEntry {
            id: "music".to_string(),
            label: "Music".to_string(),
            icon: Some("\u{1F3B5}".to_string()),
            meta: Some("music".to_string()),
        });
        items.push(CarouselEntry {
            id: "sing".to_string(),
            label: "Sing".to_string(),
            icon: Some("\u{1F3A4}".to_string()),
            meta: Some("sing".to_string()),
        });
    }

    items
}
