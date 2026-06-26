use dioxus::prelude::*;
use crate::services::weather::units::*;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;
use crate::components::weather::charts::WaterFill;

#[component]
pub fn HumidityCard(humidity: i32, dew_point: f64) -> Element {
    let settings = WEATHER_SETTINGS.read();
    let dp_brief = format_temperature_brief(dew_point, settings.temperature_unit);

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col overflow-hidden relative",
            div { class: "absolute inset-0 pointer-events-none",
                WaterFill {
                    percent: humidity as f64,
                    width: 200.0,
                    height: 200.0,
                }
            }
            div { class: "flex items-center justify-center gap-1.5 text-sm font-medium text-foreground relative z-10",
                crate::components::icons::DropletIcon { class: "w-4 h-4".to_string() }
                span { class: "truncate", "Humidity" }
            }
            div { class: "flex-1 flex flex-col items-center justify-center min-h-0 relative z-10",
                div { class: "text-5xl font-bold text-foreground", "{humidity}%" }
            }
            div { class: "absolute bottom-2 left-2 z-10 bg-background/70 backdrop-blur-sm rounded-full px-2.5 py-1 text-xs font-medium text-foreground",
                "{dp_brief} Dew point"
            }
        }
    }
}
