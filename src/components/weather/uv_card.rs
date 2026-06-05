use dioxus::prelude::*;
use crate::services::weather::units::*;

#[component]
pub fn UvCard(uv_index: f64) -> Element {
    let level = UvLevel::from_index(uv_index);

    let dot_colors = [
        "#4CAF50", "#FFEB3B", "#FF9800", "#F44336", "#9C27B0",
    ];
    let active_dots = match level {
        UvLevel::Low => 1,
        UvLevel::Moderate => 2,
        UvLevel::High => 3,
        UvLevel::VeryHigh => 4,
        UvLevel::Extreme => 5,
    };

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col overflow-hidden relative",
            div { class: "flex items-center justify-center gap-1.5 text-sm font-medium text-foreground",
                crate::components::icons::SunIcon { class: "w-4 h-4".to_string() }
                span { class: "truncate", "UV index" }
            }
            div { class: "flex-1 flex flex-col items-center justify-center min-h-0",
                div { class: "text-5xl font-bold text-foreground", "{uv_index:.1}" }
                div { class: "text-sm text-muted-foreground mt-1", "{level.label()}" }
                div { class: "flex items-center gap-1.5 mt-3",
                    for (i, color) in dot_colors.iter().enumerate() {
                        {
                            let active = (i + 1) <= active_dots;
                            let opacity = if active { "1" } else { "0.25" };
                            rsx! {
                                div {
                                    class: "w-2.5 h-2.5 rounded-full transition-opacity",
                                    style: "background-color: {color}; opacity: {opacity};",
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
