use dioxus::prelude::*;
use crate::services::weather::types::*;
use crate::services::weather::units::*;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;

#[component]
pub fn WeatherHeader(current: CurrentWeather, today: Option<DailyForecast>) -> Element {
    let settings = WEATHER_SETTINGS.read();
    let gradient = current.weather_code.gradient_classes(current.is_day);
    let temp = format_temperature(current.temperature, settings.temperature_unit);
    let feels = format_temperature_brief(current.feels_like, settings.temperature_unit);
    let high = today.as_ref().map(|t| format_temperature_brief(t.temperature_max, settings.temperature_unit)).unwrap_or_default();
    let low = today.as_ref().map(|t| format_temperature_brief(t.temperature_min, settings.temperature_unit)).unwrap_or_default();

    rsx! {
        div { class: "bg-gradient-to-b {gradient} rounded-2xl p-6 text-white transition-all duration-700",
            div { class: "flex items-center justify-between",
                div { class: "text-sm opacity-80", "{current.weather_code.description()}" }
            }
            div { class: "mt-2",
                span { class: "text-7xl font-bold tracking-tighter", "{temp}" }
            }
            div { class: "mt-2 text-lg opacity-90",
                "Feels like {feels}"
            }
            div { class: "mt-1 text-sm opacity-75",
                "H: {high}  L: {low}"
            }
        }
    }
}
