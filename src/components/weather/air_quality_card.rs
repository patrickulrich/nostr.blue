use dioxus::prelude::*;
use crate::services::weather::types::HourlyForecast;
use crate::services::weather::units::*;
use crate::components::weather::charts::AqiGradientBar;

#[component]
pub fn AirQualityCard(hourly: Vec<HourlyForecast>) -> Element {
    let current = hourly.first();
    let (pm25, pm10, no2, o3, so2, co) = match current {
        Some(h) => (
            h.pm2_5.unwrap_or(0.0),
            h.pm10.unwrap_or(0.0),
            h.nitrogen_dioxide.unwrap_or(0.0),
            h.ozone.unwrap_or(0.0),
            h.sulphur_dioxide.unwrap_or(0.0),
            h.carbon_monoxide.unwrap_or(0.0),
        ),
        None => (0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
    };

    let aqi = calculate_aqi(pm25, pm10, no2, o3, so2, co);
    let level = AqiLevel::from_aqi(aqi);

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col overflow-hidden relative",
            div { class: "flex items-center justify-center gap-1.5 text-sm font-medium text-foreground",
                crate::components::icons::WindIcon { class: "w-4 h-4".to_string() }
                span { class: "truncate", "Air quality" }
            }
            div { class: "flex-1 flex flex-col items-center justify-center min-h-0",
                div { class: "text-5xl font-bold text-foreground", "{aqi:.0}" }
                div { class: "w-full max-w-[100px] mt-3",
                    AqiGradientBar {
                        aqi: aqi,
                        max: 500.0,
                        width: 100.0,
                    }
                }
                div { class: "text-sm text-muted-foreground mt-2", "{level.label()}" }
            }
        }
    }
}
