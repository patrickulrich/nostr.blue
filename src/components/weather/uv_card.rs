use dioxus::prelude::*;
use crate::services::weather::units::*;
use crate::components::weather::charts::ArcProgress;

#[component]
pub fn UvCard(uv_index: f64) -> Element {
    let level = UvLevel::from_index(uv_index);

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col items-center justify-center",
            ArcProgress {
                value: uv_index,
                max: 12.0,
                arc_angle: 270.0,
                color: level.color().to_string(),
                size: 100.0,
                center_text: format!("{:.1}", uv_index),
                sublabel: level.label().to_string(),
            }
        }
    }
}
