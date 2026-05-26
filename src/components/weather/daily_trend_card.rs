use dioxus::prelude::*;
use crate::services::weather::types::DailyForecast;
use crate::services::weather::units::*;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;
use crate::components::weather::charts::SvgLineChart;
use crate::components::weather::charts::color_scales;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DailyTab {
    Temperature,
    Wind,
    Precipitation,
    UV,
    FeelsLike,
    CloudCover,
}

fn convert_temp(c: f64, unit: TemperatureUnit) -> f64 {
    match unit {
        TemperatureUnit::Celsius => c,
        TemperatureUnit::Fahrenheit => celsius_to_fahrenheit(c),
    }
}

fn convert_wind(ms: f64, unit: WindSpeedUnit) -> f64 {
    match unit {
        WindSpeedUnit::Ms => ms,
        WindSpeedUnit::Kmh => ms_to_kmh(ms),
        WindSpeedUnit::Mph => ms_to_mph(ms),
        WindSpeedUnit::Knots => ms_to_knots(ms),
    }
}

fn convert_precip(mm: f64, unit: PrecipitationUnit) -> f64 {
    match unit {
        PrecipitationUnit::Mm => mm,
        PrecipitationUnit::Inch => mm_to_inches(mm),
    }
}

#[component]
pub fn DailyTrendCard(daily: Vec<DailyForecast>) -> Element {
    let mut active_tab = use_signal(|| DailyTab::Temperature);
    let settings = WEATHER_SETTINGS.read();

    let data: Vec<(usize, f64)> = match active_tab() {
        DailyTab::Temperature => daily.iter().enumerate().map(|(i, d)| (i, convert_temp(d.temperature_max, settings.temperature_unit))).collect(),
        DailyTab::Wind => daily.iter().enumerate().map(|(i, d)| (i, convert_wind(d.wind_speed_max, settings.wind_speed_unit))).collect(),
        DailyTab::Precipitation => daily.iter().enumerate().map(|(i, d)| (i, convert_precip(d.precipitation_sum, settings.precipitation_unit))).collect(),
        DailyTab::UV => daily.iter().enumerate().map(|(i, d)| (i, d.uv_index_max)).collect(),
        DailyTab::FeelsLike => daily.iter().enumerate().map(|(i, d)| (i, convert_temp(d.feels_like_max, settings.temperature_unit))).collect(),
        DailyTab::CloudCover => daily.iter().enumerate().map(|(i, d)| (i, d.cloud_cover_mean as f64)).collect(),
    };

    let (y_min, y_max) = match active_tab() {
        DailyTab::Temperature => {
            let mn = daily.iter().map(|d| convert_temp(d.temperature_min, settings.temperature_unit)).fold(f64::INFINITY, f64::min);
            let mx = daily.iter().map(|d| convert_temp(d.temperature_max, settings.temperature_unit)).fold(f64::NEG_INFINITY, f64::max);
            (mn.floor() - 2.0, mx.ceil() + 2.0)
        }
        DailyTab::Wind => (0.0, data.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max).ceil() + 2.0),
        DailyTab::Precipitation => (0.0, data.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max).ceil().max(0.5)),
        DailyTab::UV => (0.0, data.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max).ceil().max(5.0)),
        DailyTab::FeelsLike => {
            let mn = daily.iter().map(|d| convert_temp(d.feels_like_min, settings.temperature_unit)).fold(f64::INFINITY, f64::min);
            let mx = daily.iter().map(|d| convert_temp(d.feels_like_max, settings.temperature_unit)).fold(f64::NEG_INFINITY, f64::max);
            (mn.floor() - 2.0, mx.ceil() + 2.0)
        }
        DailyTab::CloudCover => (0.0, 105.0),
    };

    let scale = match active_tab() {
        DailyTab::Temperature | DailyTab::FeelsLike => color_scales::temperature_scale(),
        DailyTab::Wind => color_scales::wind_scale(),
        DailyTab::Precipitation => color_scales::precipitation_scale(),
        DailyTab::UV => color_scales::uv_scale(),
        DailyTab::CloudCover => color_scales::cloud_cover_scale(),
    };

    let day_labels: Vec<String> = daily.iter().map(|d| {
        let parts: Vec<&str> = d.date.split('-').collect();
        if parts.len() == 3 {
            let month = parts[1].parse::<u32>().unwrap_or(1);
            let day = parts[2].parse::<u32>().unwrap_or(1);
            format!("{} {}", short_month(month), day)
        } else {
            d.date.clone()
        }
    }).collect();

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-4",
            div { class: "flex items-center gap-2 mb-3",
                crate::components::icons::CalendarIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                span { class: "font-semibold", "Daily forecast" }
            }
            div { class: "flex gap-1 mb-3 overflow-x-auto scrollbar-hide",
                for (tab, label) in [(DailyTab::Temperature, "Temp"), (DailyTab::Wind, "Wind"), (DailyTab::Precipitation, "Precip"), (DailyTab::UV, "UV"), (DailyTab::FeelsLike, "Feels"), (DailyTab::CloudCover, "Cloud")] {
                    button {
                        key: "{label}",
                        class: if tab == active_tab() { "px-3 py-1 rounded-full text-xs font-medium bg-primary text-primary-foreground" } else { "px-3 py-1 rounded-full text-xs font-medium text-muted-foreground hover:bg-accent" },
                        onclick: move |_| active_tab.set(tab),
                        "{label}"
                    }
                }
            }
            div { class: "overflow-x-auto scrollbar-hide",
                div { class: "min-w-[{daily.len() * 70}px]",
                    SvgLineChart {
                        data: data,
                        x_labels: day_labels,
                        y_min: y_min,
                        y_max: y_max,
                        color_scale: scale,
                        show_area_fill: true,
                        width: daily.len() as f64 * 70.0,
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

fn short_month(m: u32) -> &'static str {
    match m {
        1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr",
        5 => "May", 6 => "Jun", 7 => "Jul", 8 => "Aug",
        9 => "Sep", 10 => "Oct", 11 => "Nov", 12 => "Dec",
        _ => "",
    }
}
