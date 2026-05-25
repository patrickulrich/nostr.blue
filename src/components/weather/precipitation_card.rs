use dioxus::prelude::*;
use crate::services::weather::units::*;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;

#[component]
pub fn PrecipitationCard(sum: f64, probability: i32, rain: f64, snow: f64) -> Element {
    let settings = WEATHER_SETTINGS.read();
    let total = format_precipitation(sum, settings.precipitation_unit);
    let prob = format!("{}%", probability);

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col items-center justify-center",
            crate::components::icons::DropletIcon { class: "w-6 h-6 text-muted-foreground mb-1".to_string() }
            div { class: "text-2xl font-bold text-foreground", "{total}" }
            div { class: "text-sm text-muted-foreground", "{prob} chance" }
            if rain > 0.1 {
                { let rain_str = format_precipitation(rain, settings.precipitation_unit); rsx! {
                    div { class: "text-xs text-muted-foreground mt-1", "Rain: {rain_str}" }
                } }
            }
            if snow > 0.1 {
                { let snow_str = format_precipitation(snow, settings.precipitation_unit); rsx! {
                    div { class: "text-xs text-muted-foreground", "Snow: {snow_str}" }
                } }
            }
        }
    }
}
