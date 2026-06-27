use dioxus::prelude::*;
use crate::services::weather::units::*;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;

#[component]
pub fn PrecipitationCard(sum: f64, probability: i32, rain: f64, snow: f64) -> Element {
    let settings = WEATHER_SETTINGS.read();
    let total = format_precipitation(sum, settings.precipitation_unit);
    let prob = format!("{}%", probability);

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col overflow-hidden relative",
            div { class: "flex items-center justify-center gap-1.5 text-sm font-medium text-foreground",
                crate::components::icons::DropletIcon { class: "w-4 h-4".to_string() }
                span { class: "truncate", "Precipitation" }
            }
            div { class: "flex-1 flex flex-col items-center justify-center min-h-0",
                div { class: "text-4xl font-bold text-foreground", "{total}" }
                div { class: "text-sm text-muted-foreground mt-1", "Total rain for the day" }
                div { class: "text-xs text-muted-foreground", "{prob} chance" }
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
}
