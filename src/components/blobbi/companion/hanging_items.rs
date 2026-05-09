use dioxus::prelude::*;

use crate::components::blobbi::companion::companion_state::{now_ms, BLOBBI_COMPANION};
use crate::components::blobbi::companion::behavior_loop::{trigger_walk_to, set_gaze_target, clear_gaze_target};
use crate::components::blobbi::companion::need_detection::{check_item_category_need, needed_categories};
use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::shop::shop_items::{self, ItemCategory, ShopItem};
use crate::stores::blobbi::blobbi_profile_store;

#[derive(Clone, Debug, PartialEq)]
struct HangingItem {
    id: String,
    icon: &'static str,
    name: String,
    category: ItemCategory,
    spawn_ms: u64,
    state: ItemState,
}

#[derive(Clone, Debug, PartialEq)]
enum ItemState {
    Hanging,
    Falling,
    Landed,
    Using,
}

impl HangingItem {
    fn from_shop(item: &ShopItem) -> Self {
        Self {
            id: item.id.to_string(),
            icon: item.icon,
            name: item.name.to_string(),
            category: item.category,
            spawn_ms: now_ms(),
            state: ItemState::Hanging,
        }
    }
}

fn resolve_inventory_items(blobbi: &BlobbiCompanion) -> Vec<ShopItem> {
    let profile = match blobbi_profile_store::get_profile() {
        Some(p) => p,
        None => return Vec::new(),
    };

    let needed_cats = needed_categories(blobbi);
    if needed_cats.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    for entry in &profile.storage {
        if entry.quantity == 0 {
            continue;
        }
        if let Some(item) = shop_items::find_item(&entry.item_id) {
            if needed_cats.contains(&item.category) && !result.iter().any(|r: &ShopItem| r.id == item.id) {
                result.push(item);
            }
        }
        if result.len() >= 4 {
            break;
        }
    }

    result
}

#[component]
pub fn HangingItems(blobbi: BlobbiCompanion) -> Element {
    let mut items: Signal<Vec<HangingItem>> = use_signal(Vec::new);
    let mut last_spawn: Signal<u64> = use_signal(|| 0u64);
    let mut using_items: Signal<std::collections::HashSet<String>> = use_signal(std::collections::HashSet::new);

    {
        let blobbi_c = blobbi.clone();
        use_effect(move || {
            let now = now_ms();

            let alive: Vec<HangingItem> = items()
                .iter()
                .filter(|i| {
                    !matches!(i.state, ItemState::Using)
                        && now.saturating_sub(i.spawn_ms) < 20000
                })
                .cloned()
                .collect();

            if alive.len() != items().len() {
                items.set(alive);
            }

            for item in items().iter() {
                if matches!(item.state, ItemState::Falling)
                    && now.saturating_sub(item.spawn_ms) >= 600
                {
                    let mut cur = items();
                    if let Some(i) = cur.iter_mut().find(|i| i.id == item.id && i.spawn_ms == item.spawn_ms) {
                        i.state = ItemState::Landed;
                        let need_result = check_item_category_need(&blobbi_c, item.category);
                        if need_result.needs_item {
                            let target_x = (BLOBBI_COMPANION.read().x + 40.0).clamp(50.0, 350.0);
                            trigger_walk_to(target_x);
                            BLOBBI_COMPANION.write().pending_auto_use = Some(item.id.clone());
                        } else {
                            set_gaze_target(
                                BLOBBI_COMPANION.read().x + 80.0,
                                BLOBBI_COMPANION.read().y - 50.0,
                            );
                            spawn(async move {
                                crate::platform::timer::sleep_ms(1200).await;
                                clear_gaze_target();
                            });
                        }
                    }
                    items.set(cur);
                    break;
                }
            }

            if now.saturating_sub(last_spawn()) > 1200 && items().len() < 4 {
                let available = resolve_inventory_items(&blobbi_c);
                let existing_ids: Vec<String> = items().iter().map(|i| i.id.clone()).collect();
                if let Some(shop_item) = available.iter().find(|i| !existing_ids.contains(&i.id.to_string())) {
                    let mut cur = items();
                    cur.push(HangingItem::from_shop(shop_item));
                    items.set(cur);
                    last_spawn.set(now);
                }
            }
        });
    }

    if items().is_empty() {
        return rsx! { div {} };
    }

    let spacing = 70.0_f32;
    let total = items().len() as f32;
    let start_x = -(total - 1.0) * spacing / 2.0;

    rsx! {
        div {
            class: "fixed top-0 right-16 z-[99] flex items-start gap-0",
            style: "animation: blobbi-hanging-slide 0.35s ease-out both;",

            for (idx, item) in items().iter().enumerate() {
                {
                    let item_id = item.id.clone();
                    let item_state = item.state.clone();
                    let delay = (idx * 80) as u32;
                    let x_offset = start_x + idx as f32 * spacing;

                    let (line_class, circle_class) = match &item_state {
                        ItemState::Hanging => (
                            "animate-[hanging-sway_3s_ease-in-out_infinite]".to_string(),
                            "hover:scale-110 cursor-pointer".to_string(),
                        ),
                        ItemState::Falling => (
                            String::new(),
                            "animate-[blobbi-item-drop_0.6s_ease-in_forwards]".to_string(),
                        ),
                        ItemState::Landed => (
                            String::new(),
                            "cursor-pointer animate-[blobbi-item-bounce_0.4s_ease-out]".to_string(),
                        ),
                        ItemState::Using => (
                            String::new(),
                            "animate-pulse opacity-60".to_string(),
                        ),
                    };

                    let is_interactive = matches!(item_state, ItemState::Hanging | ItemState::Landed);

                    rsx! {
                        div {
                            key: "{item_id}-{item.spawn_ms}",
                            class: "flex flex-col items-center",
                            style: "transform: translateX({x_offset}px);",

                            div {
                                class: "w-px h-[90px] bg-gradient-to-b from-transparent via-border to-border/60 {line_class}",
                                style: "animation-delay: {delay}ms;",
                            },

                            button {
                                class: "flex items-center justify-center w-12 h-12 rounded-full bg-card border border-border shadow-md transition-all {circle_class}",
                                disabled: !is_interactive,
                                style: "animation-delay: {delay}ms;",
                                onclick: {
                                    let item_id_c = item_id.clone();
                                    let item_state_c = item_state.clone();
                                    move |_| {
                                        match &item_state_c {
                                            ItemState::Hanging => {
                                                let mut cur = items();
                                                if let Some(i) = cur.iter_mut().find(|i| i.id == item_id_c) {
                                                    i.state = ItemState::Falling;
                                                }
                                                items.set(cur);
                                            }
                                            ItemState::Landed => {
                                                let iid = item_id_c.clone();
                                                if using_items.read().contains(&iid) {
                                                    return;
                                                }
                                                using_items.write().insert(iid.clone());
                                                let mut cur = items();
                                                if let Some(i) = cur.iter_mut().find(|i| i.id == iid) {
                                                    i.state = ItemState::Using;
                                                }
                                                items.set(cur);

                                                let mut using_items_clone = using_items;
                                                let iid_clone = iid.clone();
                                                spawn(async move {
                                                    if let Some(mut b) = crate::stores::blobbi_store::get_selected_blobbi() {
                                                        match crate::components::blobbi::shop::inventory_modal::use_item_on_blobbi_public(&mut b, &iid).await {
                                                            Ok(()) => {
                                                                crate::stores::blobbi_store::update_blobbi_in_collection(&b);
                                                            }
                                                            Err(e) => {
                                                                log::error!("Hanging item use failed: {}", e);
                                                            }
                                                        }
                                                    }
                                                    using_items_clone.write().remove(&iid_clone);
                                                });
                                            }
                                            _ => {}
                                        }
                                    }
                                },

                                span { class: "text-xl", "{item.icon}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
