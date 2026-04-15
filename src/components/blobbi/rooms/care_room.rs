use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::rooms::room_hero::RoomHero;

#[component]
pub fn CareRoom(blobbi: BlobbiCompanion) -> Element {
    rsx! {
        div { class: "flex flex-col",
            RoomHero { blobbi: blobbi.clone() }
            div { class: "px-4 mt-2",
                div { class: "text-xs text-muted-foreground mb-2",
                    "Care Items"
                }
                div { class: "grid grid-cols-3 gap-2",
                    CareItem { name: "Soap", icon: "🧼", effect: "Clean +20", cost: 15 }
                    CareItem { name: "Shampoo", icon: "🧴", effect: "Clean +30", cost: 25 }
                    CareItem { name: "Bubble Bath", icon: "🛁", effect: "Clean +40", cost: 40 }
                    CareItem { name: "Bandage", icon: "🩹", effect: "Health +15", cost: 20 }
                    CareItem { name: "Vitamins", icon: "💊", effect: "Health +25", cost: 40 }
                    CareItem { name: "Health Elixir", icon: "⚗️", effect: "Health +50", cost: 150 }
                }
            }
        }
    }
}

#[component]
fn CareItem(name: String, icon: String, effect: String, cost: u64) -> Element {
    rsx! {
        button {
            class: "flex flex-col items-center gap-1 p-3 rounded-xl bg-card border border-border hover:bg-accent transition",
            span { class: "text-2xl", "{icon}" }
            span { class: "text-xs font-medium", "{name}" }
            span { class: "text-[10px] text-blue-500", "{effect}" }
            span { class: "text-[10px] text-yellow-500", "🪙 {cost}" }
        }
    }
}
