use dioxus::prelude::*;
use crate::services::weather::units::*;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;

#[component]
pub fn VisibilityCard(visibility: f64) -> Element {
    let settings = WEATHER_SETTINGS.read();
    let display = meters_to_display(visibility, settings.distance_unit);

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col overflow-hidden relative",
            div { class: "flex items-center justify-center gap-1.5 text-sm font-medium text-foreground",
                crate::components::icons::EyeIcon { class: "w-4 h-4".to_string() }
                span { class: "truncate", "Visibility" }
            }
            div { class: "flex-1 flex flex-col items-center justify-center min-h-0",
                div { class: "text-5xl font-bold text-foreground", "{display}" }
            }
        }
    }
}
