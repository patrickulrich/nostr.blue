use dioxus::prelude::*;
use crate::services::weather::units::*;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;

#[component]
pub fn VisibilityCard(visibility: f64) -> Element {
    let settings = WEATHER_SETTINGS.read();
    let display = meters_to_display(visibility, settings.distance_unit);
    let desc = visibility_description(visibility);

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col items-center justify-center",
            crate::components::icons::EyeIcon { class: "w-6 h-6 text-muted-foreground mb-1".to_string() }
            div { class: "text-2xl font-bold text-foreground", "{display}" }
            div { class: "text-xs text-muted-foreground mt-1 text-center", "{desc}" }
        }
    }
}
