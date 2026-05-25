use dioxus::prelude::*;
use crate::services::weather::types::MinutelyForecast;
use crate::services::weather::units::*;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;
use crate::components::weather::charts::SvgBarChart;
use crate::components::weather::charts::color_scales::HorizontalLine;

#[component]
pub fn PrecipitationNowcast(minutely: Vec<MinutelyForecast>) -> Element {
    if minutely.is_empty() {
        return rsx! { div {} };
    }

    let settings = WEATHER_SETTINGS.read();
    let to_display = |mm: f64| -> f64 {
        match settings.precipitation_unit {
            PrecipitationUnit::Mm => mm,
            PrecipitationUnit::Inch => mm_to_inches(mm),
        }
    };

    let has_precip = minutely.iter().any(|m| m.precipitation > 0.0);
    if !has_precip {
        return rsx! {
            div { class: "bg-card border border-border rounded-2xl p-4",
                div { class: "flex items-center gap-2 mb-2",
                    crate::components::icons::DropletIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                    span { class: "font-semibold", "Precipitation" }
                }
                p { class: "text-sm text-muted-foreground", "No precipitation expected in the next 2 hours" }
            }
        };
    }

    let data: Vec<f64> = minutely.iter().map(|m| to_display(m.precipitation * 4.0)).collect();
    let labels: Vec<String> = minutely.iter().map(|m| {
        m.time.split('T').nth(1).unwrap_or(&m.time).trim_end_matches(":00").to_string()
    }).collect();
    let y_max = data.iter().cloned().fold(0.0_f64, f64::max).max(to_display(2.0)).ceil();

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-4",
            div { class: "flex items-center gap-2 mb-3",
                crate::components::icons::DropletIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                span { class: "font-semibold", "Precipitation" }
                span { class: "text-xs text-muted-foreground ml-auto", "Next 2 hours" }
            }
            div { class: "overflow-x-auto scrollbar-hide",
                div { class: "min-w-[{minutely.len() * 70}px]",
                    SvgBarChart {
                        data: data,
                        x_labels: labels,
                        y_max: y_max,
                        bar_color: "#4dabf7".to_string(),
                        threshold_lines: vec![
                            HorizontalLine { value: to_display(0.5), label: "Light".to_string(), color: "#69db7c".to_string(), dashed: true },
                            HorizontalLine { value: to_display(2.5), label: "Moderate".to_string(), color: "#ffd43b".to_string(), dashed: true },
                            HorizontalLine { value: to_display(10.0), label: "Heavy".to_string(), color: "#ff6b6b".to_string(), dashed: true },
                        ],
                        width: minutely.len() as f64 * 70.0,
                        height: 250.0,
                        padding_left: 50.0,
                        padding_right: 45.0,
                        padding_top: 10.0,
                        padding_bottom: 22.0,
                    }
                }
            }
        }
    }
}
