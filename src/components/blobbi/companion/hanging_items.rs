use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;

#[component]
pub fn HangingItems(blobbi: BlobbiCompanion) -> Element {
    let needs = detect_needs(&blobbi);

    if needs.is_empty() {
        return rsx! { div {} };
    }

    rsx! {
        div { class: "fixed top-4 right-4 z-[99] flex flex-col gap-1",
            for (icon, label) in &needs {
                div {
                    class: "flex items-center gap-1 px-2 py-1 rounded-lg bg-card border border-border shadow-sm text-xs animate-[float-up_2s_ease-in-out_infinite]",
                    span { "{icon}" }
                    span { class: "text-muted-foreground", "{label}" }
                }
            }
        }
    }
}

fn detect_needs(blobbi: &BlobbiCompanion) -> Vec<(&'static str, &'static str)> {
    let mut needs = Vec::new();

    if blobbi.stats.hunger < 30.0 {
        needs.push(("🍔", "Hungry"));
    }
    if blobbi.stats.happiness < 30.0 {
        needs.push(("😢", "Unhappy"));
    }
    if blobbi.stats.hygiene < 30.0 {
        needs.push(("🧹", "Dirty"));
    }
    if blobbi.stats.energy < 20.0 {
        needs.push(("😴", "Tired"));
    }
    if blobbi.stats.health < 40.0 {
        needs.push(("💊", "Sick"));
    }

    needs
}
