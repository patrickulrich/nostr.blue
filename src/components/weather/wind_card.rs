use dioxus::prelude::*;
use crate::services::weather::units::*;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;
use crate::components::weather::charts::WindCompass;

#[component]
pub fn WindCard(speed: f64, direction: i32, gusts: f64) -> Element {
    let settings = WEATHER_SETTINGS.read();
    let _speed_str = format_wind_speed(speed, settings.wind_speed_unit);
    let gusts_str = format_wind_speed(gusts, settings.wind_speed_unit);
    let dir_label = wind_direction_label(direction);
    let bft = ms_to_beaufort(speed);
    let bft_desc = beaufort_description(bft);
    let unit_short = match settings.wind_speed_unit {
        WindSpeedUnit::Ms => "m/s",
        WindSpeedUnit::Kmh => "km/h",
        WindSpeedUnit::Mph => "mph",
        WindSpeedUnit::Knots => "kn",
    };

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col overflow-hidden relative",
            div { class: "flex items-center justify-center gap-1.5 text-sm font-medium text-foreground",
                crate::components::icons::WindIcon { class: "w-4 h-4".to_string() }
                span { class: "truncate", "Wind" }
            }
            div { class: "flex-1 flex flex-col items-center justify-center min-h-0",
                WindCompass {
                    direction: direction as f64,
                    speed: match settings.wind_speed_unit {
                        WindSpeedUnit::Ms => speed,
                        WindSpeedUnit::Kmh => ms_to_kmh(speed),
                        WindSpeedUnit::Mph => ms_to_mph(speed),
                        WindSpeedUnit::Knots => ms_to_knots(speed),
                    },
                    speed_unit_label: unit_short.to_string(),
                    size: 100.0,
                }
                div { class: "mt-1 text-xs text-muted-foreground text-center",
                    "{dir_label} \u{2022} {bft_desc}"
                }
                if gusts > speed + 1.0 {
                    div { class: "text-xs text-muted-foreground",
                        "Gusts: {gusts_str}"
                    }
                }
            }
        }
    }
}
