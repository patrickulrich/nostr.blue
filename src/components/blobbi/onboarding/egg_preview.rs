use dioxus::prelude::*;

use crate::components::blobbi::visual::egg_visual::EggVisual;

#[component]
pub fn EggPreview(base_color: String) -> Element {
    rsx! {
        div { class: "flex items-center justify-center py-4",
            EggVisual {
                base_color,
            }
        }
    }
}
