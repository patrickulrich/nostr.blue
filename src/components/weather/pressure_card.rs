use dioxus::prelude::*;
use crate::services::weather::units::*;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;
use crate::components::weather::charts::ArcProgress;

#[component]
pub fn PressureCard(pressure: f64, hourly_pressure: Vec<f64>) -> Element {
    let settings = WEATHER_SETTINGS.read();
    let trend = pressure_trend(&hourly_pressure);

    let (display_val, decimals, unit_label) = match settings.pressure_unit {
        PressureUnit::Hpa => (pressure, 0, "hPa".to_string()),
        PressureUnit::Mmhg => (hpa_to_mmhg(pressure), 0, "mmHg".to_string()),
        PressureUnit::Inhg => (hpa_to_inhg(pressure), 2, "inHg".to_string()),
    };
    let center = if decimals == 0 {
        format!("{:.0}", display_val)
    } else {
        format!("{:.2}", display_val)
    };

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col overflow-hidden relative",
            div { class: "flex items-center justify-center gap-1.5 text-sm font-medium text-foreground",
                crate::components::icons::GaugeIcon { class: "w-4 h-4".to_string() }
                span { class: "truncate", "Pressure" }
            }
            div { class: "flex-1 flex flex-col items-center justify-center min-h-0",
                ArcProgress {
                    value: pressure,
                    max: 1080.0,
                    arc_angle: 320.0,
                    color: "#7c93c3".to_string(),
                    size: 100.0,
                    center_text: center,
                    sublabel: unit_label,
                }
                div { class: "text-xs text-muted-foreground mt-1", "{trend}" }
            }
        }
    }
}
