use dioxus::prelude::*;
use crate::services::weather::types::HourlyForecast;
use crate::services::weather::units::*;
use crate::components::weather::charts::ArcProgress;

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
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col items-center justify-center",
            ArcProgress {
                value: aqi,
                max: 400.0,
                arc_angle: 270.0,
                color: level.color().to_string(),
                size: 100.0,
                center_text: format!("{:.0}", aqi),
                sublabel: level.label().to_string(),
            }
            div { class: "mt-1 text-xs text-muted-foreground text-center",
                "PM2.5: {pm25:.0} \u{2022} PM10: {pm10:.0}"
            }
        }
    }
}
