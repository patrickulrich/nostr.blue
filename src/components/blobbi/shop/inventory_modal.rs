use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::shop::shop_items;
use crate::stores::blobbi_profile_store;
use crate::stores::blobbi_store;

#[component]
pub fn InventoryModal(blobbi: BlobbiCompanion, on_close: EventHandler<()>) -> Element {
    let using_item = use_signal(|| None::<String>);

    let profile = blobbi_profile_store::get_profile();
    let storage = profile.map(|p| p.storage).unwrap_or_default();

    let usable_ids: Vec<String> = storage
        .iter()
        .filter(|i| i.quantity > 0)
        .filter_map(|i| {
            let item = shop_items::find_item(&i.item_id)?;
            if is_usable_for_stage(item.id, blobbi.stage) {
                Some(i.item_id.clone())
            } else {
                None
            }
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
                div { class: "text-[10px] text-green-500 mt-0.5",
                    "{item.stat_summary()}"
                }
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
                                if let Err(e) = use_item_on_blobbi(&mut b, &item_id).await {
                                    log::error!("Use item failed: {}", e);
                                } else {
                                    blobbi_store::update_blobbi_in_collection(&b);
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
    match stage {
        crate::utils::nip_bb::BlobbiStage::Egg => false,
        _ => !matches!(item_id, "ball" | "teddy" | "kite" | "game" if false),
    }
}

async fn use_item_on_blobbi(
    blobbi: &mut BlobbiCompanion,
    item_id: &str,
) -> Result<(), String> {
    let item = shop_items::find_item(item_id)
        .ok_or_else(|| format!("Unknown item: {}", item_id))?;

    let mut profile = blobbi_profile_store::get_profile()
        .ok_or("No profile")?;

    let storage_entry = profile.storage.iter_mut()
        .find(|i| i.item_id == item_id)
        .ok_or("Item not in inventory")?;

    if storage_entry.quantity == 0 {
        return Err("No items left".to_string());
    }
    storage_entry.quantity -= 1;

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

    let now = nostr_sdk::Timestamp::now().as_secs();
    blobbi.last_interaction = Some(now);
    blobbi.experience = blobbi.experience.saturating_add(item.price.max(5));

    crate::components::blobbi::core::builders::publish_blobbi_state(blobbi).await?;
    crate::components::blobbi::core::builders::publish_profile(&profile).await?;
    blobbi_profile_store::set_profile(profile);

    Ok(())
}
