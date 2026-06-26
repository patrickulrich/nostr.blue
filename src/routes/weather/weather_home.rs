use dioxus::prelude::*;

use crate::components::weather::*;
use crate::stores::weather::location_store;
use crate::stores::weather::weather_store;

#[component]
pub fn WeatherHome() -> Element {
    let mut fetch_gen = use_signal(|| 0u32);
    let mut refetch_trigger = use_signal(|| 0u32);
    let mut show_picker = use_signal(|| false);
    let mut show_settings = use_signal(|| false);
    let expanded_alert = use_signal(|| None::<usize>);
    let mut show_search = use_signal(|| false);

    use_hook(|| {});

    use_effect(move || {
        let _ = refetch_trigger.read();
        let loc = location_store::get_selected();
        if loc.is_none() {
            return;
        }

        let gen = *fetch_gen.peek() + 1;
        fetch_gen.set(gen);
        spawn(async move {
            let result = weather_store::fetch_weather_for_current_location().await;
            if *fetch_gen.peek() != gen {
                return;
            }
            if let Err(e) = result {
                log::error!("Weather fetch failed: {}", e);
            }
        });
    });

    let loading = *weather_store::WEATHER_LOADING.read();
    let error = weather_store::WEATHER_ERROR.read().clone();
    let locations = location_store::LOCATIONS.read();
    let selected_idx = *location_store::SELECTED_LOCATION_INDEX.read();
    let loc_name = locations
        .get(selected_idx)
        .map(|l| l.name.clone())
        .unwrap_or_else(|| "Weather".to_string());
    drop(locations);

    rsx! {
        div { class: "min-h-screen pb-20",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "flex items-center justify-between px-4 py-3",
                    button {
                        class: "flex items-center gap-2 hover:bg-accent rounded-lg px-2 py-1 transition",
                        onclick: move |_| show_picker.set(true),
                        h2 { class: "text-xl font-bold", "{loc_name}" }
                        span { class: "text-muted-foreground text-sm", "\u{25BE}" }
                    }
                    div { class: "flex items-center gap-1",
                        button {
                            class: "p-2 hover:bg-accent rounded-lg transition",
                            onclick: move |_| show_settings.set(true),
                            crate::components::icons::SettingsIcon { class: "w-5 h-5".to_string() }
                        }
                        button {
                            class: "p-2 hover:bg-accent rounded-lg transition",
                            onclick: move |_| {
                                let next = *refetch_trigger.peek() + 1;
                                refetch_trigger.set(next);
                            },
                            crate::components::icons::CompassIcon { class: "w-5 h-5".to_string() }
                        }
                    }
                }
            }

            div { class: "p-4 space-y-4 max-w-4xl mx-auto",
                if location_store::LOCATIONS.read().is_empty() {
                    { empty_state(show_search) }
                } else if loading && weather_store::WEATHER_DATA.read().is_empty() {
                    { loading_skeleton() }
                } else if let Some(err) = &error {
                    {
                        let stale_data = weather_store::get_current_weather();
                        rsx! {
                            div { class: "p-3 bg-yellow-100 dark:bg-yellow-900/30 text-yellow-800 dark:text-yellow-200 rounded-lg mb-4",
                                p { class: "text-sm", "Weather data temporarily unavailable: {err}" }
                                button {
                                    class: "text-sm underline mt-1",
                                    onclick: move |_| {
                                        let next = *refetch_trigger.peek() + 1;
                                        refetch_trigger.set(next);
                                    },
                                    "Retry"
                                }
                            }
                            if let Some(data) = stale_data {
                                { weather_content(data, expanded_alert, refetch_trigger) }
                            }
                        }
                    }
                } else if let Some(data) = weather_store::get_current_weather() {
                    { weather_content(data, expanded_alert, refetch_trigger) }
                } else {
                    { loading_skeleton() }
                }
            }

            if *show_picker.read() {
                { rsx! {
                    LocationPicker {
                        on_close: move |_| show_picker.set(false),
                        on_search: move |_| {
                            show_picker.set(false);
                            show_search.set(true);
                        },
                    }
                } }
            }

            if *show_search.read() {
                { rsx! {
                    crate::routes::weather::weather_search::WeatherSearchInline {
                        on_close: move |_| show_search.set(false),
                    }
                } }
            }

            if *show_settings.read() {
                { settings_modal(show_settings) }
            }
        }
    }
}

fn weather_content(
    data: crate::services::weather::WeatherData,
    mut expanded_alert: Signal<Option<usize>>,
    _refetch_trigger: Signal<u32>,
) -> Element {
    let today = data.daily.first().cloned();
    let current_hourly: Vec<crate::services::weather::HourlyForecast> = {
        let offset = data.utc_offset_seconds as i64;
        let cutoff_utc = crate::platform::timestamp::now_secs() as i64 - 3600;
        data.hourly
            .iter()
            .filter(|h| {
                chrono::NaiveDateTime::parse_from_str(&h.time, "%Y-%m-%dT%H:%M")
                    .ok()
                    .map(|ndt| ndt.and_utc().timestamp() + offset)
                    .unwrap_or(i64::MIN) > cutoff_utc
            })
            .take(48)
            .cloned()
            .collect()
    };

    let hourly_for_pressure: Vec<f64> = current_hourly.iter().map(|h| h.pressure).collect();

    rsx! {
        WeatherHeader { current: data.current.clone(), today: today.clone() }

        if !data.alerts.is_empty() {
            AlertBanner {
                alerts: data.alerts.clone(),
                on_expand: move |i: usize| {
                    expanded_alert.set(Some(i));
                },
            }
        }

        if let Some(expanded_idx) = *expanded_alert.read() {
            if let Some(alert) = data.alerts.get(expanded_idx) {
                div { class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm flex items-end lg:items-center justify-center",
                    div { class: "bg-background rounded-t-2xl lg:rounded-2xl max-w-lg w-full max-h-[80vh] overflow-auto p-6",
                        div { class: "flex items-center justify-between mb-4",
                            h3 { class: "font-semibold text-lg {alert.severity.text_class()}", "{alert.event}" }
                            button {
                                class: "p-2 hover:bg-accent rounded-lg",
                                onclick: move |_| expanded_alert.set(None),
                                "\u{2715}"
                            }
                        }
                        if let Some(headline) = &alert.headline {
                            p { class: "font-medium mb-2", "{headline}" }
                        }
                        if let Some(desc) = &alert.description {
                            div { class: "text-sm text-muted-foreground mb-3 whitespace-pre-wrap max-h-60 overflow-auto", "{desc}" }
                        }
                        if let Some(instr) = &alert.instruction {
                            div { class: "bg-accent rounded-lg p-3 mb-3",
                                p { class: "text-xs font-semibold mb-1", "Instructions" }
                                p { class: "text-sm", "{instr}" }
                            }
                        }
                        if let Some(sender) = &alert.sender_name {
                            p { class: "text-xs text-muted-foreground", "Source: {sender}" }
                        }
                    }
                }
            }
        }

        if let Some(loc) = location_store::get_selected() {
            WeatherRadar { lat: loc.lat, lon: loc.lon }
        }

        DailyTrendCard { daily: data.daily.clone() }
        HourlyTrendCard { hourly: data.hourly.clone(), utc_offset_seconds: data.utc_offset_seconds }

        div { class: "grid grid-cols-2 gap-3",
            WindCard { speed: data.current.wind_speed, direction: data.current.wind_direction, gusts: data.current.wind_gusts }
            AirQualityCard { hourly: current_hourly.clone() }
            UvCard { uv_index: data.current.uv_index }
            HumidityCard { humidity: data.current.relative_humidity, dew_point: data.current.dew_point }
            PressureCard { pressure: data.current.pressure, hourly_pressure: hourly_for_pressure }
            VisibilityCard { visibility: data.current.visibility }

            if let Some(today) = &today {
                PrecipitationCard { sum: today.precipitation_sum, probability: today.precipitation_probability_max, rain: today.rain_sum, snow: today.snowfall_sum }
                SunMoonCard { sunrise: today.sunrise.clone(), sunset: today.sunset.clone(), utc_offset_seconds: data.utc_offset_seconds }
            }
        }
    }
}

fn empty_state(mut show_search: Signal<bool>) -> Element {
    rsx! {
        div { class: "text-center py-20",
            div { class: "text-6xl mb-4", "\u{1F326}\u{FE0F}" }
            h2 { class: "text-xl font-semibold mb-2", "Add a location" }
            p { class: "text-muted-foreground mb-6", "Search for a city or use your current location" }
            button {
                class: "px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                onclick: move |_| show_search.set(true),
                "Search locations"
            }
        }
    }
}

fn loading_skeleton() -> Element {
    rsx! {
        div { class: "animate-pulse space-y-4",
            div { class: "h-48 bg-muted rounded-2xl" }
            div { class: "h-64 bg-muted rounded-2xl" }
            div { class: "h-48 bg-muted rounded-2xl" }
            div { class: "grid grid-cols-2 gap-3",
                for _ in 0..6 {
                    div { class: "aspect-square bg-muted rounded-2xl" }
                }
            }
        }
    }
}

fn settings_modal(mut show_settings: Signal<bool>) -> Element {
    use crate::stores::weather::weather_settings::*;
    use crate::services::weather::units::*;

    let mut temp_unit = use_signal(|| WEATHER_SETTINGS.read().temperature_unit);
    let mut wind_unit = use_signal(|| WEATHER_SETTINGS.read().wind_speed_unit);
    let mut pressure_unit = use_signal(|| WEATHER_SETTINGS.read().pressure_unit);

    rsx! {
        div { class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm",
            div { class: "absolute bottom-0 left-0 right-0 bg-background rounded-t-2xl p-6 max-w-lg mx-auto",
                div { class: "flex items-center justify-between mb-6",
                    h3 { class: "text-lg font-semibold", "Weather Settings" }
                    button {
                        class: "p-2 hover:bg-accent rounded-lg",
                        onclick: move |_| show_settings.set(false),
                        "\u{2715}"
                    }
                }
                div { class: "space-y-4",
                    div {
                        label { class: "text-sm font-medium block mb-2", "Temperature" }
                        div { class: "flex gap-2",
                            for (unit, label) in [(TemperatureUnit::Celsius, "\u{00B0}C"), (TemperatureUnit::Fahrenheit, "\u{00B0}F")] {
                                button {
                                    key: "{label}",
                                    class: if unit == temp_unit() { "px-4 py-1.5 rounded-full text-sm bg-primary text-primary-foreground" } else { "px-4 py-1.5 rounded-full text-sm bg-muted text-muted-foreground hover:bg-accent" },
                                    onclick: move |_| {
                                        temp_unit.set(unit);
                                        let mut s = WEATHER_SETTINGS.read().clone();
                                        s.temperature_unit = unit;
                                        save_settings(&s);
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                    div {
                        label { class: "text-sm font-medium block mb-2", "Wind Speed" }
                        div { class: "flex gap-2 flex-wrap",
                            for (unit, label) in [(WindSpeedUnit::Ms, "m/s"), (WindSpeedUnit::Kmh, "km/h"), (WindSpeedUnit::Mph, "mph"), (WindSpeedUnit::Knots, "kn")] {
                                button {
                                    key: "{label}",
                                    class: if unit == wind_unit() { "px-3 py-1.5 rounded-full text-xs bg-primary text-primary-foreground" } else { "px-3 py-1.5 rounded-full text-xs bg-muted text-muted-foreground hover:bg-accent" },
                                    onclick: move |_| {
                                        wind_unit.set(unit);
                                        let mut s = WEATHER_SETTINGS.read().clone();
                                        s.wind_speed_unit = unit;
                                        save_settings(&s);
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                    div {
                        label { class: "text-sm font-medium block mb-2", "Pressure" }
                        div { class: "flex gap-2",
                            for (unit, label) in [(PressureUnit::Hpa, "hPa"), (PressureUnit::Mmhg, "mmHg"), (PressureUnit::Inhg, "inHg")] {
                                button {
                                    key: "{label}",
                                    class: if unit == pressure_unit() { "px-3 py-1.5 rounded-full text-xs bg-primary text-primary-foreground" } else { "px-3 py-1.5 rounded-full text-xs bg-muted text-muted-foreground hover:bg-accent" },
                                    onclick: move |_| {
                                        pressure_unit.set(unit);
                                        let mut s = WEATHER_SETTINGS.read().clone();
                                        s.pressure_unit = unit;
                                        save_settings(&s);
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
