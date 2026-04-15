use dioxus::prelude::*;

use crate::components::blobbi::shop::shop_items::{self, ItemCategory, ShopItem};
use crate::stores::blobbi_profile_store;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShopTab {
    #[default]
    Food,
    Toy,
    Medicine,
    Hygiene,
}

impl ShopTab {
    pub fn category(&self) -> ItemCategory {
        match self {
            ShopTab::Food => ItemCategory::Food,
            ShopTab::Toy => ItemCategory::Toy,
            ShopTab::Medicine => ItemCategory::Medicine,
            ShopTab::Hygiene => ItemCategory::Hygiene,
        }
    }

    pub fn icon(&self) -> &'static str {
        self.category().icon()
    }

    pub fn label(&self) -> &'static str {
        self.category().label()
    }

    pub fn all() -> &'static [ShopTab] {
        &[ShopTab::Food, ShopTab::Toy, ShopTab::Medicine, ShopTab::Hygiene]
    }
}

#[component]
pub fn ShopModal(on_close: EventHandler<()>) -> Element {
    let mut active_tab = use_signal(ShopTab::default);
    let purchasing = use_signal(|| None::<String>);
    let status_msg = use_signal(|| None::<String>);

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
                        span { class: "text-xl", "🏪" }
                        h3 { class: "text-lg font-bold", "Shop" }
                    }
                    div { class: "flex items-center gap-3",
                        span { class: "text-sm text-yellow-500 font-medium",
                            "🪙 {coins}"
                        }
                        button {
                            class: "p-1.5 hover:bg-accent rounded-lg transition",
                            onclick: move |_| on_close.call(()),
                            "✕"
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
                        {render_shop_item(item, coins, purchasing, status_msg)}
                    }
                }
            }
        }
    }
}

fn render_shop_item(
    item: &ShopItem,
    coins: u64,
    mut purchasing: Signal<Option<String>>,
    mut status_msg: Signal<Option<String>>,
) -> Element {
    let can_afford = coins >= item.price;
    let is_purchasing = purchasing().as_deref() == Some(item.id);
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
                div { class: "text-[10px] text-green-500 mt-0.5",
                    "{item.stat_summary()}"
                }
            }
            button {
                class: if can_afford && !is_purchasing {
                    "px-3 py-1.5 bg-blue-500 hover:bg-blue-600 text-white text-xs font-medium rounded-lg transition shrink-0"
                } else {
                    "px-3 py-1.5 bg-muted text-muted-foreground text-xs font-medium rounded-lg shrink-0 cursor-not-allowed"
                },
                disabled: !can_afford || is_purchasing,
                onclick: {
                    let item_id = item.id;
                    let price = item.price;
                    move |_| {
                        purchasing.set(Some(item_id.to_string()));
                        let item_id = item_id.to_string();
                        spawn(async move {
                            match purchase_item(&item_id, price).await {
                                Ok(_) => {
                                    status_msg.set(Some(format!("Purchased {}!", item_id)));
                                }
                                Err(e) => {
                                    log::error!("Purchase failed: {}", e);
                                }
                            }
                            purchasing.set(None);
                        });
                    }
                },
                if is_purchasing {
                    "..."
                } else if item.price == 0 {
                    "Free"
                } else {
                    "🪙 {item.price}"
                }
            }
        }
    }
}

async fn purchase_item(item_id: &str, price: u64) -> Result<(), String> {
    let mut profile = blobbi_profile_store::get_profile()
        .ok_or("No profile found")?;

    if profile.coins < price {
        return Err("Not enough coins".to_string());
    }

    profile.coins = profile.coins.saturating_sub(price);

    if let Some(existing) = profile.storage.iter_mut().find(|i| i.item_id == item_id) {
        existing.quantity += 1;
    } else {
        profile.storage.push(crate::components::blobbi::core::types::StorageItem {
            item_id: item_id.to_string(),
            quantity: 1,
        });
    }

    crate::components::blobbi::core::builders::publish_profile(&profile).await?;
    blobbi_profile_store::set_profile(profile);
    Ok(())
}
