use dioxus::prelude::*;

use crate::components::blobbi::shop::shop_items::{self, ItemCategory, ShopItem};
use crate::components::blobbi::shop::item_effect_display::{EffectDisplay, EffectDisplayMode};
use crate::stores::blobbi_profile_store;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShopTab {
    #[default]
    Food,
    Toy,
    Medicine,
    Hygiene,
    Accessory,
}

impl ShopTab {
    pub fn category(&self) -> ItemCategory {
        match self {
            ShopTab::Food => ItemCategory::Food,
            ShopTab::Toy => ItemCategory::Toy,
            ShopTab::Medicine => ItemCategory::Medicine,
            ShopTab::Hygiene => ItemCategory::Hygiene,
            ShopTab::Accessory => ItemCategory::Accessory,
        }
    }

    pub fn icon(&self) -> &'static str {
        self.category().icon()
    }

    pub fn label(&self) -> &'static str {
        self.category().label()
    }

    pub fn all() -> &'static [ShopTab] {
        &[
            ShopTab::Food,
            ShopTab::Toy,
            ShopTab::Medicine,
            ShopTab::Hygiene,
            ShopTab::Accessory,
        ]
    }
}

#[component]
pub fn ShopModal(on_close: EventHandler<()>) -> Element {
    let mut active_tab = use_signal(ShopTab::default);
    let status_msg = use_signal(|| None::<String>);
    let buy_dialog = use_signal(|| None::<String>);

    let coins = blobbi_profile_store::get_coins();
    let items = shop_items::items_by_category(active_tab().category());

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-card border border-border rounded-2xl w-full max-w-md mx-4 shadow-2xl max-h-[85vh] flex flex-col",
                onclick: move |e| e.stop_propagation(),

                div { class: "flex items-center justify-between p-4 border-b border-border",
                    div { class: "flex items-center gap-2",
                        span { class: "text-xl", "\u{1F3EA}" }
                        h3 { class: "text-lg font-bold", "Shop" }
                    }
                    div { class: "flex items-center gap-3",
                        span { class: "text-sm text-yellow-500 font-medium",
                            "\u{1FA99} {coins}"
                        }
                        button {
                            class: "p-1.5 hover:bg-accent rounded-lg transition",
                            onclick: move |_| on_close.call(()),
                            "\u{2715}"
                        }
                    }
                }

                div { class: "flex border-b border-border",
                    for tab in ShopTab::all() {
                        button {
                            class: if active_tab() == *tab {
                                "flex-1 flex items-center justify-center gap-1 py-2.5 text-sm font-medium border-b-2 border-blue-500 text-foreground"
                            } else {
                                "flex-1 flex items-center justify-center gap-1 py-2.5 text-sm text-muted-foreground hover:text-foreground transition"
                            },
                            onclick: move |_| active_tab.set(*tab),
                            span { "{tab.icon()}" }
                            span { "{tab.label()}" }
                        }
                    }
                }

                if let Some(msg) = status_msg() {
                    div { class: "px-4 py-2 text-sm text-center text-green-500 bg-green-500/10",
                        "{msg}"
                    }
                }

                div { class: "flex-1 overflow-y-auto p-4 space-y-2",
                    for item in &items {
                        {render_shop_item(item, coins, buy_dialog)}
                    }
                }

                if let Some(ref dialog_id) = *buy_dialog.read() {
                    {render_quantity_dialog(dialog_id.clone(), coins, buy_dialog, status_msg)}
                }
            }
        }
    }
}

fn render_shop_item(
    item: &ShopItem,
    coins: u64,
    mut buy_dialog: Signal<Option<String>>,
) -> Element {
    let can_afford = coins >= item.price;
    let owned = blobbi_profile_store::get_item_quantity(item.id);

    rsx! {
        div {
            class: "flex items-center gap-3 p-3 rounded-xl bg-background border border-border",
            span { class: "text-2xl", "{item.icon}" }
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center gap-2",
                    span { class: "text-sm font-medium", "{item.name}" }
                    if owned > 0 {
                        span { class: "text-[10px] px-1.5 py-0.5 rounded-full bg-blue-500/20 text-blue-400",
                            "x{owned}"
                        }
                    }
                }
                span { class: "text-xs text-muted-foreground", "{item.description}" }
                EffectDisplay { item: item.clone(), mode: EffectDisplayMode::Badges }
            }
            button {
                class: if can_afford {
                    "px-3 py-1.5 bg-blue-500 hover:bg-blue-600 text-white text-xs font-medium rounded-lg transition shrink-0"
                } else {
                    "px-3 py-1.5 bg-muted text-muted-foreground text-xs font-medium rounded-lg shrink-0 cursor-not-allowed"
                },
                disabled: !can_afford,
                onclick: {
                    let item_id = item.id.to_string();
                    move |_| buy_dialog.set(Some(item_id.clone()))
                },
                if item.price == 0 {
                    "Free"
                } else {
                    "\u{1FA99} {item.price}"
                }
            }
        }
    }
}

fn render_quantity_dialog(
    item_id: String,
    coins: u64,
    mut buy_dialog: Signal<Option<String>>,
    mut status_msg: Signal<Option<String>>,
) -> Element {
    let item = match shop_items::find_item(&item_id) {
        Some(i) => i,
        None => return rsx! { div {} },
    };

    let max_qty = if item.price > 0 {
        coins.checked_div(item.price).unwrap_or(0) as u32
    } else {
        99
    };

    let mut qty = use_signal(|| 1u32);
    let total = item.price.saturating_mul(qty() as u64);

    rsx! {
        div {
            class: "absolute inset-0 z-10 flex items-center justify-center bg-black/30 rounded-2xl",

            div {
                class: "bg-card border border-border rounded-xl p-4 mx-8 w-full max-w-xs shadow-xl",
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                div { class: "flex items-center gap-2 mb-3",
                    span { class: "text-xl", "{item.icon}" }
                    span { class: "text-sm font-medium", "{item.name}" }
                }

                div { class: "flex items-center justify-center gap-4 mb-3",
                    button {
                        class: "w-8 h-8 flex items-center justify-center rounded-lg bg-muted hover:bg-accent transition text-lg font-bold",
                        disabled: qty() <= 1,
                        onclick: move |_| qty.set(qty().saturating_sub(1)),
                        "\u{2212}"
                    }
                    span { class: "text-2xl font-bold w-12 text-center", "{qty}" }
                    button {
                        class: "w-8 h-8 flex items-center justify-center rounded-lg bg-muted hover:bg-accent transition text-lg font-bold",
                        disabled: qty() >= max_qty,
                        onclick: move |_| qty.set(qty().saturating_add(1).min(max_qty)),
                        "+"
                    }
                }

                div { class: "text-center text-sm text-muted-foreground mb-3",
                    "Total: \u{1FA99} {total}"
                }

                div { class: "flex gap-2",
                    button {
                        class: "flex-1 py-2 bg-muted rounded-lg text-sm hover:bg-accent transition",
                        onclick: move |_| buy_dialog.set(None),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 py-2 bg-blue-500 hover:bg-blue-600 text-white text-sm font-medium rounded-lg transition",
                        disabled: total > coins || qty() == 0,
                        onclick: {
                            let item_id = item_id.clone();
                            move |_| {
                                let item_id = item_id.clone();
                                let q = qty();
                                let price = item.price;
                                buy_dialog.set(None);
                                spawn(async move {
                                    match purchase_item(&item_id, price, q).await {
                                        Ok(_) => {
                                            status_msg.set(Some(format!("Bought x{}!", q)));
                                            let toast = consume_toast();
                                            toast.success(
                                                format!("Purchased x{}!", q),
                                                ToastOptions::new()
                                                    .duration(Duration::from_secs(2)),
                                            );
                                        }
                                        Err(e) => {
                                            log::error!("Purchase failed: {}", e);
                                            let toast = consume_toast();
                                            toast.error(
                                                format!("Purchase failed: {}", e),
                                                ToastOptions::new()
                                                    .duration(Duration::from_secs(3)),
                                            );
                                        }
                                    }
                                });
                            }
                        },
                        "Buy x{qty}"
                    }
                }
            }
        }
    }
}

async fn purchase_item(item_id: &str, price: u64, quantity: u32) -> Result<(), String> {
    let catalog_item = shop_items::find_item(item_id)
        .ok_or_else(|| format!("Unknown item: {}", item_id))?;

    if catalog_item.price != price {
        return Err("Price mismatch — please refresh".to_string());
    }

    let mut profile = blobbi_profile_store::get_profile()
        .ok_or("No profile found")?;

    let total = price.saturating_mul(quantity as u64);
    if profile.coins < total {
        return Err("Not enough coins".to_string());
    }

    profile.coins = profile.coins.saturating_sub(total);

    if let Some(existing) = profile.storage.iter_mut().find(|i| i.item_id == item_id) {
        existing.quantity += quantity;
    } else {
        profile.storage.push(crate::components::blobbi::core::types::StorageItem {
            item_id: item_id.to_string(),
            quantity,
        });
    }

    crate::components::blobbi::core::builders::publish_profile(&profile).await?;
    blobbi_profile_store::set_profile(profile);
    Ok(())
}
