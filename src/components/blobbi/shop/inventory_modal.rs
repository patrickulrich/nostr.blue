use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::shop::item_effect_display::{EffectDisplay, EffectDisplayMode};
use crate::components::blobbi::shop::shop_items;
use crate::stores::blobbi_profile_store;
use crate::stores::blobbi_store;
use crate::components::blobbi::actions::item_cooldown::is_on_cooldown;
use crate::components::blobbi::actions::mission_tracker;
use crate::components::blobbi::actions::action_types::BlobbiActionType;
use crate::components::blobbi::visual::status_reaction::trigger_action_emotion;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use std::time::Duration;

#[component]
pub fn InventoryModal(blobbi: BlobbiCompanion, on_close: EventHandler<()>, #[props(default)] action_filter: Option<String>) -> Element {
    let using_item = use_signal(|| None::<String>);

    let profile = blobbi_profile_store::get_profile();
    let storage = profile.map(|p| p.storage).unwrap_or_default();

    let filter_cats: Option<Vec<shop_items::ItemCategory>> = action_filter.as_deref().map(|action| match action {
        "feed" => vec![shop_items::ItemCategory::Food],
        "play" => vec![shop_items::ItemCategory::Toy],
        "clean" => vec![shop_items::ItemCategory::Hygiene],
        "heal" => vec![shop_items::ItemCategory::Medicine],
        _ => vec![],
    });

    let usable_ids: Vec<String> = storage
        .iter()
        .filter(|i| i.quantity > 0)
        .filter_map(|i| {
            let item = shop_items::find_item(&i.item_id)?;
            if !is_usable_for_stage(item.id, blobbi.stage) {
                return None;
            }
            if let Some(ref cats) = filter_cats {
                if !cats.contains(&item.category) {
                    return None;
                }
            }
            Some(i.item_id.clone())
        })
        .collect();

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-card border border-border rounded-2xl w-full max-w-md mx-4 shadow-2xl max-h-[85vh] flex flex-col",
                onclick: move |e| e.stop_propagation(),

                div { class: "flex items-center justify-between p-4 border-b border-border",
                    div { class: "flex items-center gap-2",
                        span { class: "text-xl", "🎒" }
                        h3 { class: "text-lg font-bold", "Inventory" }
                    }
                    button {
                        class: "p-1.5 hover:bg-accent rounded-lg transition",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }

                div { class: "flex-1 overflow-y-auto p-4 space-y-2",
                    if storage.is_empty() {
                        div { class: "text-center text-muted-foreground py-8",
                            "No items yet. Visit the shop!"
                        }
                    }
                    for item_id in &usable_ids {
                        {render_inventory_item(item_id, blobbi.clone(), using_item)}
                    }
                }
            }
        }
    }
}

fn render_inventory_item(
    item_id: &str,
    blobbi: BlobbiCompanion,
    mut using_item: Signal<Option<String>>,
) -> Element {
    let item = match shop_items::find_item(item_id) {
        Some(i) => i,
        None => return rsx! { div {} },
    };
    let quantity = blobbi_profile_store::get_item_quantity(item_id);
    let is_using = using_item().as_deref() == Some(item_id);

    rsx! {
        div { class: "flex items-center gap-3 p-3 rounded-xl bg-background border border-border",
            span { class: "text-2xl", "{item.icon}" }
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center gap-2",
                    span { class: "text-sm font-medium", "{item.name}" }
                    span { class: "text-[10px] px-1.5 py-0.5 rounded-full bg-blue-500/20 text-blue-400",
                        "x{quantity}"
                    }
                }
                EffectDisplay { item: item.clone(), mode: EffectDisplayMode::Inline }
            }
            button {
                class: if is_using {
                    "px-3 py-1.5 bg-muted text-muted-foreground text-xs font-medium rounded-lg cursor-not-allowed"
                } else {
                    "px-3 py-1.5 bg-green-500 hover:bg-green-600 text-white text-xs font-medium rounded-lg transition"
                },
                disabled: is_using,
                onclick: {
                    let item_id = item_id.to_string();
                    move |_| {
                        using_item.set(Some(item_id.clone()));
                        let _d = blobbi.d.clone();
                        let item_id = item_id.clone();
                        spawn(async move {
                            if let Some(mut b) = blobbi_store::get_selected_blobbi() {
                                match use_item_on_blobbi_public(&mut b, &item_id).await {
                                    Ok(()) => {
                                        blobbi_store::update_blobbi_in_collection(&b);
                                        let toast = consume_toast();
                                        let item_name = shop_items::find_item(&item_id)
                                            .map(|i| i.name.to_string())
                                            .unwrap_or_else(|| "Item".to_string());
                                        toast.success(
                                            format!("{} used!", item_name),
                                            ToastOptions::new()
                                                .duration(Duration::from_secs(2)),
                                        );
                                    }
                                    Err(e) => {
                                        log::error!("Use item failed: {}", e);
                                        let toast = consume_toast();
                                        toast.error(
                                            format!("Failed: {}", e),
                                            ToastOptions::new()
                                                .duration(Duration::from_secs(3)),
                                        );
                                    }
                                }
                            }
                            using_item.set(None);
                        });
                    }
                },
                if is_using { "..." } else { "Use" }
            }
        }
    }
}

fn is_usable_for_stage(item_id: &str, stage: crate::utils::nip_bb::BlobbiStage) -> bool {
    use crate::components::blobbi::shop::shop_items::{find_item, ItemCategory};
    let Some(item) = find_item(item_id) else {
        return false;
    };
    match stage {
        crate::utils::nip_bb::BlobbiStage::Egg => {
            matches!(item.category, ItemCategory::Medicine | ItemCategory::Hygiene)
        }
        crate::utils::nip_bb::BlobbiStage::Baby | crate::utils::nip_bb::BlobbiStage::Adult => {
            !matches!(item.category, ItemCategory::Accessory)
        }
    }
}

pub async fn use_item_on_blobbi_public(
    blobbi: &mut BlobbiCompanion,
    item_id: &str,
) -> Result<(), String> {
    let item = shop_items::find_item(item_id)
        .ok_or_else(|| format!("Unknown item: {}", item_id))?;

    {
        let cooldowns = crate::components::blobbi::actions::item_cooldown::use_item_cooldowns();
        let cooldowns_read = cooldowns.read();
        if is_on_cooldown(&cooldowns_read, item_id) {
            return Err("Item is on cooldown".to_string());
        }
    }

    crate::components::blobbi::core::migration::ensure_canonical_before_action(blobbi);

    let now = nostr_sdk::Timestamp::now().as_secs();
    *blobbi = crate::components::blobbi::core::decay::apply_decay(blobbi, now);

    let mut profile = blobbi_profile_store::get_profile()
        .ok_or("No profile")?;

    let original_quantity = match profile.storage.iter_mut()
        .find(|i| i.item_id == item_id)
    {
        Some(entry) if entry.quantity > 0 => {
            let q = entry.quantity;
            entry.quantity -= 1;
            q
        }
        Some(_) => return Err("No items left".to_string()),
        None => return Err("Item not in inventory".to_string()),
    };

    for (stat, delta) in &item.stat_changes {
        let new_val = blobbi.stat_value(stat) + delta;
        match *stat {
            "hunger" => blobbi.stats.hunger = new_val.clamp(1.0, 100.0),
            "happiness" => blobbi.stats.happiness = new_val.clamp(1.0, 100.0),
            "health" => blobbi.stats.health = new_val.clamp(1.0, 100.0),
            "hygiene" => blobbi.stats.hygiene = new_val.clamp(1.0, 100.0),
            "energy" => blobbi.stats.energy = new_val.clamp(1.0, 100.0),
            _ => {}
        }
    }

    blobbi.last_interaction = Some(now);
    blobbi.last_decay_at = Some(now);

    let xp = match item.category {
        shop_items::ItemCategory::Food => 5,
        shop_items::ItemCategory::Toy => 8,
        shop_items::ItemCategory::Hygiene => 5,
        shop_items::ItemCategory::Medicine => 10,
        shop_items::ItemCategory::Accessory => 3,
    };
    blobbi.experience = blobbi.experience.saturating_add(xp);

    crate::components::blobbi::core::streak::record_care_action(blobbi, "use_item");

    trigger_action_emotion(BlobbiActionType::UseItem);
    mission_tracker::track_mission_progress(BlobbiActionType::UseItem);
    crate::components::blobbi::actions::hatch_tasks::update_task_progress(blobbi, "use_item");

    if let Err(e) = crate::components::blobbi::core::builders::publish_profile(&profile).await {
        log::error!("Failed to publish profile (inventory decrement): {}", e);
        return Err(e);
    }

    if let Err(e) = crate::components::blobbi::core::builders::publish_blobbi_state(blobbi).await {
        log::error!("Failed to publish blobbi state after inventory use: {}", e);
        if let Some(entry) = profile.storage.iter_mut().find(|i| i.item_id == item_id) {
            entry.quantity = original_quantity;
        }
        let _ = crate::components::blobbi::core::builders::publish_profile(&profile).await;
        return Err(e);
    }

    blobbi_profile_store::set_profile(profile);

    {
        let mut cooldowns = crate::components::blobbi::actions::item_cooldown::use_item_cooldowns();
        crate::components::blobbi::actions::item_cooldown::apply_cooldown_success(
            &mut cooldowns.write(),
            item_id,
        );
    }

    Ok(())
}
