use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::rooms::room_hero::RoomHero;

#[component]
pub fn KitchenRoom(blobbi: BlobbiCompanion) -> Element {
    rsx! {
        div { class: "flex flex-col",
            RoomHero { blobbi: blobbi.clone() }
            div { class: "px-4 mt-2",
                div { class: "text-xs text-muted-foreground mb-2",
                    "Feed your Blobbi"
                }
                div { class: "grid grid-cols-3 gap-2",
                    FoodItem { name: "Berry", icon: "🫐", hunger: "+10", cost: 0 }
                    FoodItem { name: "Apple", icon: "🍎", hunger: "+20", cost: 10 }
                    FoodItem { name: "Burger", icon: "🍔", hunger: "+30", cost: 25 }
                    FoodItem { name: "Pizza", icon: "🍕", hunger: "+25", cost: 35 }
                    FoodItem { name: "Cake", icon: "🎂", hunger: "+40", cost: 50 }
                    FoodItem { name: "Sushi", icon: "🍣", hunger: "+35", cost: 45 }
                }
            }
        }
    }
}

#[component]
fn FoodItem(name: String, icon: String, hunger: String, cost: u64) -> Element {
    rsx! {
        button {
            class: "flex flex-col items-center gap-1 p-3 rounded-xl bg-card border border-border hover:bg-accent transition",
            span { class: "text-2xl", "{icon}" }
            span { class: "text-xs font-medium", "{name}" }
            span { class: "text-[10px] text-green-500", "{hunger}" }
            if cost > 0 {
                span { class: "text-[10px] text-yellow-500", "🪙 {cost}" }
            }
        }
    }
}
