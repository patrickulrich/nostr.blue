use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::visual::recipe::*;
use crate::utils::nip_bb::*;

use super::adult_visual::AdultVisual;
use super::baby_visual::BabyVisual;
use super::egg_visual::EggVisual;

#[component]
pub fn BlobbiVisual(blobbi: BlobbiCompanion, size: Option<String>) -> Element {
    let s = size.unwrap_or_else(|| "200".to_string());
    let recipe = resolve_recipe(
        &blobbi.stats,
        blobbi.is_sleeping(),
        &blobbi.personality.mood,
    );

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("../blobbi.css") }
        div { class: "flex items-center justify-center",
            style: "width: {s}px; height: {s}px;",
            match blobbi.stage {
                BlobbiStage::Egg => rsx! {
                    EggVisual {
                        base_color: blobbi.visual_traits.base_color.clone(),
                    }
                },
                BlobbiStage::Baby => rsx! {
                    BabyVisual { blobbi: blobbi.clone(), recipe: recipe.clone() }
                },
                BlobbiStage::Adult => rsx! {
                    AdultVisual { blobbi: blobbi.clone(), recipe: recipe.clone() }
                },
            }
        }
    }
}
