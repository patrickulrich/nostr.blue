use dioxus::prelude::*;
use crate::services::weather::units::*;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;

#[component]
pub fn HumidityCard(humidity: i32, dew_point: f64) -> Element {
    let settings = WEATHER_SETTINGS.read();
    let desc = humidity_description(humidity);
    let dp = format_temperature(dew_point, settings.temperature_unit);

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col items-center justify-center",
            div { class: "text-3xl font-bold text-foreground", "{humidity}%" }
            div { class: "text-xs text-muted-foreground mt-1", "{desc}" }
            div { class: "text-xs text-muted-foreground mt-2", "Dew point: {dp}" }
        }
    }
}
