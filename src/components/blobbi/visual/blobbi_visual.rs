use dioxus::prelude::*;

use crate::components::blobbi::core::types::BlobbiCompanion;
use crate::components::blobbi::visual::recipe::*;
use crate::utils::nip_bb::*;

use super::adult_visual::AdultVisual;
use super::baby_visual::BabyVisual;
use super::egg_visual::EggVisual;
use super::status_reaction::resolve_recipe_with_override;
#[cfg(feature = "web")]
use super::eye_tracking;

#[component]
pub fn BlobbiVisual(blobbi: BlobbiCompanion, size: Option<String>, feed_mode: Option<bool>) -> Element {
    #[cfg(feature = "web")]
    {
        eye_tracking::install_eye_tracker();
    }

    let s = size.unwrap_or_else(|| "200".to_string());
    let mut recipe = resolve_recipe_with_override(&blobbi);

    if feed_mode.unwrap_or(false) {
        recipe = attenuate_for_feed(&recipe);
    }

    let eye_container_class = if !blobbi.is_egg() && !blobbi.is_sleeping() {
        "blobbi-eye-container"
    } else {
        ""
    };

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("../blobbi.css") }
        div { class: "flex items-center justify-center {eye_container_class}",
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
