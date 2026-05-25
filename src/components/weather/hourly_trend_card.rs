use dioxus::prelude::*;
use crate::services::weather::types::HourlyForecast;
use crate::services::weather::units::*;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;
use crate::components::weather::charts::SvgLineChart;
use crate::components::weather::charts::color_scales;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HourlyTab {
    Temperature,
    Precipitation,
    Wind,
    Humidity,
    UV,
    Pressure,
}

#[component]
pub fn HourlyTrendCard(hourly: Vec<HourlyForecast>, utc_offset_seconds: i32) -> Element {
    let mut active_tab = use_signal(|| HourlyTab::Temperature);
    let settings = WEATHER_SETTINGS.read();

    let display: Vec<HourlyForecast> = {
        let now_str = {
            let now_secs = crate::platform::timestamp::now_secs();
            let utc_ts = now_secs as i64 + utc_offset_seconds as i64;
            let local = chrono::DateTime::from_timestamp(utc_ts, 0)
                .unwrap_or_default();
            local.format("%Y-%m-%dT%H:00").to_string()
        };
        let start = hourly.iter().position(|h| h.time >= now_str).unwrap_or(0);
        hourly[start..].to_vec()
    };

    let count = display.len().min(48);
    let display = &display[..count];

    let data: Vec<(usize, f64)> = match active_tab() {
        HourlyTab::Temperature => display.iter().enumerate().map(|(i, h)| (i, match settings.temperature_unit {
            TemperatureUnit::Celsius => h.temperature,
            TemperatureUnit::Fahrenheit => celsius_to_fahrenheit(h.temperature),
        })).collect(),
        HourlyTab::Precipitation => display.iter().enumerate().map(|(i, h)| (i, match settings.precipitation_unit {
            PrecipitationUnit::Mm => h.precipitation,
            PrecipitationUnit::Inch => mm_to_inches(h.precipitation),
        })).collect(),
        HourlyTab::Wind => display.iter().enumerate().map(|(i, h)| (i, match settings.wind_speed_unit {
            WindSpeedUnit::Ms => h.wind_speed,
            WindSpeedUnit::Kmh => ms_to_kmh(h.wind_speed),
            WindSpeedUnit::Mph => ms_to_mph(h.wind_speed),
            WindSpeedUnit::Knots => ms_to_knots(h.wind_speed),
        })).collect(),
        HourlyTab::Humidity => display.iter().enumerate().map(|(i, h)| (i, h.relative_humidity as f64)).collect(),
        HourlyTab::UV => display.iter().enumerate().map(|(i, h)| (i, h.uv_index)).collect(),
        HourlyTab::Pressure => display.iter().enumerate().map(|(i, h)| (i, match settings.pressure_unit {
            PressureUnit::Hpa => h.pressure,
            PressureUnit::Mmhg => hpa_to_mmhg(h.pressure),
            PressureUnit::Inhg => hpa_to_inhg(h.pressure),
        })).collect(),
    };

    let (y_min, y_max, scale) = match active_tab() {
        HourlyTab::Temperature => {
            let vals: Vec<f64> = data.iter().map(|(_, v)| *v).collect();
            (vals.iter().cloned().fold(f64::INFINITY, f64::min).floor() - 2.0,
             vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max).ceil() + 2.0,
             color_scales::temperature_scale())
        }
        HourlyTab::Precipitation => (0.0, data.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max).ceil().max(0.1),
            color_scales::precipitation_scale()),
        HourlyTab::Wind => (0.0, data.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max).ceil() + 2.0, color_scales::wind_scale()),
        HourlyTab::Humidity => (0.0, 105.0, color_scales::humidity_scale()),
        HourlyTab::UV => (0.0, data.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max).ceil().max(5.0), color_scales::uv_scale()),
        HourlyTab::Pressure => {
            let vals: Vec<f64> = data.iter().map(|(_, v)| *v).collect();
            (vals.iter().cloned().fold(f64::INFINITY, f64::min).floor() - 2.0,
             vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max).ceil() + 2.0,
             color_scales::pressure_scale())
        }
    };

    let labels: Vec<String> = display.iter().map(|h| h.time.clone()).collect();

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-4",
            div { class: "flex items-center gap-2 mb-3",
                crate::components::icons::WindIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                span { class: "font-semibold", "Hourly forecast" }
            }
            div { class: "flex gap-1 mb-3 overflow-x-auto scrollbar-hide",
                for (tab, label) in [
                    (HourlyTab::Temperature, "Temp"),
                    (HourlyTab::Precipitation, "Precip"),
                    (HourlyTab::Wind, "Wind"),
                    (HourlyTab::Humidity, "Hum"),
                    (HourlyTab::UV, "UV"),
                    (HourlyTab::Pressure, "Press"),
                ] {
                    button {
                        key: "{label}",
                        class: if tab == active_tab() { "px-3 py-1 rounded-full text-xs font-medium bg-primary text-primary-foreground" } else { "px-3 py-1 rounded-full text-xs font-medium text-muted-foreground hover:bg-accent" },
                        onclick: move |_| active_tab.set(tab),
                        "{label}"
                    }
                }
            }
            div { class: "overflow-x-auto scrollbar-hide",
                div { class: "min-w-[{count * 70}px]",
                    SvgLineChart {
                        data: data,
                        x_labels: labels,
                        y_min: y_min,
                        y_max: y_max,
                        color_scale: scale,
                        show_area_fill: true,
                        width: count as f64 * 70.0,
                        height: 250.0,
                        padding_left: 50.0,
                        padding_right: 45.0,
                        padding_top: 10.0,
                        padding_bottom: 22.0,
                        horizontal_lines: vec![],
                    }
                }
            }
        }
    }
}
