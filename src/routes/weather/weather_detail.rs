use dioxus::prelude::*;

use crate::services::weather::types::WeatherData;
use crate::services::weather::units::*;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;
use crate::components::weather::charts::SvgLineChart;
use crate::components::weather::charts::color_scales;

#[component]
pub fn WeatherDetail(date: String) -> Element {
    let settings = WEATHER_SETTINGS.read();

    let weather_data: Option<WeatherData> = {
        let loc = crate::stores::weather::location_store::get_selected();
        loc.and_then(|l| crate::stores::weather::weather_store::get_weather_for_location(&l.id))
    };

    let data = match weather_data {
        Some(d) => d,
        None => {
            return rsx! {
                div { class: "min-h-screen flex items-center justify-center",
                    p { class: "text-muted-foreground", "No weather data available" }
                }
            };
        }
    };

    let day_data: Vec<&crate::services::weather::HourlyForecast> = data
        .hourly
        .iter()
        .filter(|h| h.time.starts_with(&date))
        .collect();

    if day_data.is_empty() {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center",
                p { class: "text-muted-foreground", "No hourly data for {date}" }
            }
        };
    }

    let convert_temp = |c: f64| -> f64 {
        match settings.temperature_unit {
            TemperatureUnit::Celsius => c,
            TemperatureUnit::Fahrenheit => celsius_to_fahrenheit(c),
        }
    };
    let convert_wind = |ms: f64| -> f64 {
        match settings.wind_speed_unit {
            WindSpeedUnit::Ms => ms,
            WindSpeedUnit::Kmh => ms_to_kmh(ms),
            WindSpeedUnit::Mph => ms_to_mph(ms),
            WindSpeedUnit::Knots => ms_to_knots(ms),
        }
    };
    let convert_precip = |mm: f64| -> f64 {
        match settings.precipitation_unit {
            PrecipitationUnit::Mm => mm,
            PrecipitationUnit::Inch => mm_to_inches(mm),
        }
    };

    let temp_data: Vec<(usize, f64)> = day_data.iter().enumerate().map(|(i, h)| (i, convert_temp(h.temperature))).collect();
    let temp_min = temp_data.iter().map(|(_, v)| *v).fold(f64::INFINITY, f64::min).floor() - 2.0;
    let temp_max = temp_data.iter().map(|(_, v)| *v).fold(f64::NEG_INFINITY, f64::max).ceil() + 2.0;
    let labels: Vec<String> = day_data.iter().map(|h| h.time.clone()).collect();

    let precip_data: Vec<(usize, f64)> = day_data.iter().enumerate().map(|(i, h)| (i, convert_precip(h.precipitation))).collect();
    let wind_data: Vec<(usize, f64)> = day_data.iter().enumerate().map(|(i, h)| (i, convert_wind(h.wind_speed))).collect();
    let uv_data: Vec<(usize, f64)> = day_data.iter().enumerate().map(|(i, h)| (i, h.uv_index)).collect();
    let hum_data: Vec<(usize, f64)> = day_data.iter().enumerate().map(|(i, h)| (i, h.relative_humidity as f64)).collect();

    let wind_max = wind_data.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max).ceil() + 2.0;
    let uv_max = uv_data.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max).ceil().max(5.0);
    let precip_max = precip_data.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max).ceil().max(0.1);

    let formatted_date = format_display_date(&date);
    let chart_width = day_data.len() as f64 * 50.0;
    let small_width = day_data.len() as f64 * 40.0;

    rsx! {
        div { class: "min-h-screen pb-20",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3",
                    h2 { class: "text-lg font-bold", "{formatted_date}" }
                    p { class: "text-sm text-muted-foreground", "Hourly details" }
                }
            }

            div { class: "p-4 space-y-4 max-w-4xl mx-auto",
                div { class: "bg-card border border-border rounded-2xl p-4",
                    div { class: "flex items-center gap-2 mb-3",
                        crate::components::icons::ThermometerIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        span { class: "font-semibold", "Temperature" }
                    }
                    div { class: "overflow-x-auto scrollbar-hide",
                        div { class: "min-w-[{chart_width}px]",
                            SvgLineChart {
                                data: temp_data,
                                x_labels: labels.clone(),
                                y_min: temp_min,
                                y_max: temp_max,
                                color_scale: color_scales::temperature_scale(),
                                show_area_fill: true,
                                width: chart_width,
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

                div { class: "bg-card border border-border rounded-2xl p-4",
                    div { class: "flex items-center gap-2 mb-3",
                        crate::components::icons::DropletIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        span { class: "font-semibold", "Precipitation" }
                    }
                    div { class: "overflow-x-auto scrollbar-hide",
                        div { class: "min-w-[{chart_width}px]",
                            SvgLineChart {
                                data: precip_data,
                                x_labels: labels.clone(),
                                y_min: 0.0,
                                y_max: precip_max,
                                color_scale: color_scales::precipitation_scale(),
                                show_area_fill: true,
                                width: chart_width,
                                height: 200.0,
                                padding_left: 50.0,
                                padding_right: 45.0,
                                padding_top: 10.0,
                                padding_bottom: 22.0,
                                horizontal_lines: vec![],
                            }
                        }
                    }
                }

                div { class: "bg-card border border-border rounded-2xl p-4",
                    div { class: "flex items-center gap-2 mb-3",
                        crate::components::icons::WindIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        span { class: "font-semibold", "Wind" }
                    }
                    div { class: "overflow-x-auto scrollbar-hide",
                        div { class: "min-w-[{chart_width}px]",
                            SvgLineChart {
                                data: wind_data,
                                x_labels: labels.clone(),
                                y_min: 0.0,
                                y_max: wind_max,
                                color_scale: color_scales::wind_scale(),
                                show_area_fill: true,
                                width: chart_width,
                                height: 200.0,
                                padding_left: 50.0,
                                padding_right: 45.0,
                                padding_top: 10.0,
                                padding_bottom: 22.0,
                                horizontal_lines: vec![],
                            }
                        }
                    }
                }

                div { class: "grid grid-cols-2 gap-4",
                    div { class: "bg-card border border-border rounded-2xl p-4",
                        div { class: "flex items-center gap-2 mb-3",
                            crate::components::icons::SunIcon { class: "w-4 h-4 text-muted-foreground".to_string() }
                            span { class: "text-sm font-semibold", "UV Index" }
                        }
                        div { class: "overflow-x-auto scrollbar-hide",
                            div { class: "min-w-[{small_width}px]",
                                SvgLineChart {
                                    data: uv_data,
                                    x_labels: labels.clone(),
                                    y_min: 0.0,
                                    y_max: uv_max,
                                    color_scale: color_scales::uv_scale(),
                                    show_area_fill: false,
                                    width: small_width,
                                    height: 150.0,
                                    padding_left: 40.0,
                                    padding_right: 20.0,
                                    padding_top: 5.0,
                                    padding_bottom: 20.0,
                                    horizontal_lines: vec![],
                                }
                            }
                        }
                    }

                    div { class: "bg-card border border-border rounded-2xl p-4",
                        div { class: "flex items-center gap-2 mb-3",
                            crate::components::icons::DropletIcon { class: "w-4 h-4 text-muted-foreground".to_string() }
                            span { class: "text-sm font-semibold", "Humidity" }
                        }
                        div { class: "overflow-x-auto scrollbar-hide",
                            div { class: "min-w-[{small_width}px]",
                                SvgLineChart {
                                    data: hum_data,
                                    x_labels: labels,
                                    y_min: 0.0,
                                    y_max: 105.0,
                                    color_scale: color_scales::humidity_scale(),
                                    show_area_fill: false,
                                    width: small_width,
                                    height: 150.0,
                                    padding_left: 40.0,
                                    padding_right: 20.0,
                                    padding_top: 5.0,
                                    padding_bottom: 20.0,
                                    horizontal_lines: vec![],
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn format_display_date(date: &str) -> String {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() == 3 {
        let month = short_month(parts[1].parse::<u32>().unwrap_or(1));
        let day = parts[2].parse::<u32>().unwrap_or(1);
        format!("{} {}", month, day)
    } else {
        date.to_string()
    }
}

fn short_month(m: u32) -> &'static str {
    match m {
        1 => "January", 2 => "February", 3 => "March", 4 => "April",
        5 => "May", 6 => "June", 7 => "July", 8 => "August",
        9 => "September", 10 => "October", 11 => "November", 12 => "December",
        _ => "",
    }
}
