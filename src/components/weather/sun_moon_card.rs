use dioxus::prelude::*;
use chrono::Timelike;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;
use crate::services::weather::units::{moon_phase_name, moon_emoji};
use crate::components::weather::charts::SunArc;

#[component]
pub fn SunMoonCard(sunrise: String, sunset: String, utc_offset_seconds: i32) -> Element {
    let _settings = WEATHER_SETTINGS.read();

    let now_secs = crate::platform::timestamp::now_secs();
    let local_secs = (now_secs as i64) + (utc_offset_seconds as i64);
    let now_local = chrono::DateTime::from_timestamp(local_secs, 0).unwrap_or_default();
    let is_day = is_daytime(&sunrise, &sunset, &now_local);

    let rise_hours = parse_iso_to_hours(&sunrise);
    let set_hours = parse_iso_to_hours(&sunset);
    let current_hours = now_local.time().num_seconds_from_midnight() as f64 / 3600.0;

    let moon_phase = {
        let synodic_month = 29.53058867;
        let known_new_moon = chrono::NaiveDate::from_ymd_opt(2000, 1, 6).unwrap_or_default();
        let today = now_local.date_naive();
        let days_since = (today - known_new_moon).num_days() as f64;
        let phase_fraction = (days_since % synodic_month) / synodic_month;
        phase_fraction * 360.0
    };
    let moon_emo = moon_emoji(moon_phase);
    let moon_name = moon_phase_name(moon_phase);

    let rise_time = format_time(rise_hours);
    let set_time = format_time(set_hours);

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col overflow-hidden relative",
            div { class: "flex items-center justify-center gap-1.5 text-sm font-medium text-foreground",
                crate::components::icons::SunIcon { class: "w-4 h-4".to_string() }
                span { class: "truncate", "Sunrise & sunset" }
            }
            div { class: "flex-1 flex flex-col items-center justify-center min-h-0",
                div { class: "w-full max-h-[70px] flex items-center justify-center",
                    SunArc {
                        sunrise_hour: rise_hours,
                        sunset_hour: set_hours,
                        current_hour: current_hours,
                        is_day: is_day,
                        size: 100.0,
                    }
                }
                div { class: "w-full flex flex-col items-center gap-0.5 text-sm text-foreground mt-1",
                    div { class: "flex items-center gap-1.5",
                        crate::components::icons::SunriseIcon { class: "w-4 h-4 text-muted-foreground".to_string() }
                        span { "{rise_time}" }
                    }
                    div { class: "flex items-center gap-1.5",
                        crate::components::icons::SunsetIcon { class: "w-4 h-4 text-muted-foreground".to_string() }
                        span { "{set_time}" }
                    }
                }
                div { class: "text-xs text-muted-foreground mt-1",
                    "{moon_emo} {moon_name}"
                }
            }
        }
    }
}

fn parse_iso_to_hours(iso: &str) -> f64 {
    if let Some(time_part) = iso.split('T').nth(1) {
        let parts: Vec<&str> = time_part.split(':').collect();
        if parts.len() >= 2 {
            if let (Ok(h), Ok(m)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                return h + m / 60.0;
            }
        }
    }
    12.0
}

fn format_time(hours: f64) -> String {
    let h = hours.floor() as u32;
    let m = ((hours - h as f64) * 60.0).round() as u32;
    format!("{:02}:{:02}", h, m)
}

fn is_daytime(sunrise: &str, sunset: &str, now: &chrono::DateTime<chrono::Utc>) -> bool {
    let parse_time = |s: &str| -> Option<chrono::NaiveTime> {
        let time_str = s.split('T').nth(1)?;
        chrono::NaiveTime::parse_from_str(time_str, "%H:%M").ok()
    };
    let rise = parse_time(sunrise);
    let set = parse_time(sunset);
    match (rise, set) {
        (Some(r), Some(s)) => {
            let now_time = now.time().with_second(0).unwrap_or(now.time());
            now_time >= r && now_time <= s
        }
        _ => true,
    }
}
