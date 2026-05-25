use dioxus::prelude::*;
use chrono::Timelike;
use crate::stores::weather::weather_settings::WEATHER_SETTINGS;
use crate::services::weather::units::{moon_phase_name, moon_emoji};

#[component]
pub fn SunMoonCard(sunrise: String, sunset: String) -> Element {
    let _settings = WEATHER_SETTINGS.read();
    let rise_time = format_time(&sunrise);
    let set_time = format_time(&sunset);

    let now_secs = crate::platform::timestamp::now_secs();
    let now_dt = chrono::DateTime::from_timestamp(now_secs as i64, 0).unwrap_or_default();
    let is_day = is_daytime(&sunrise, &sunset, &now_dt);

    let sun_emoji = if is_day { "\u{2600}\u{FE0F}" } else { "\u{1F319}" };

    let moon_phase = {
        let synodic_month = 29.53058867;
        let known_new_moon = chrono::NaiveDate::from_ymd_opt(2000, 1, 6).unwrap_or_default();
        let today = now_dt.date_naive();
        let days_since = (today - known_new_moon).num_days() as f64;
        let phase_fraction = (days_since % synodic_month) / synodic_month ;
        phase_fraction * 360.0
    };
    let moon_emo = moon_emoji(moon_phase);
    let moon_name = moon_phase_name(moon_phase);

    rsx! {
        div { class: "bg-card border border-border rounded-2xl p-3 aspect-square flex flex-col items-center justify-center",
            div { class: "text-2xl mb-1", "{sun_emoji}" }
            div { class: "text-xs text-muted-foreground",
                "\u{2191} {rise_time}  \u{2193} {set_time}"
            }
            div { class: "text-xs text-muted-foreground mt-2",
                "{moon_emo} {moon_name}"
            }
        }
    }
}

fn format_time(iso: &str) -> String {
    if let Some(time_part) = iso.split('T').nth(1) {
        let cleaned = time_part.trim_end_matches(":00");
        if let Some(hour_min) = cleaned.split(':').take(2).collect::<Vec<_>>().as_slice().chunks(2).next() {
            if hour_min.len() == 2 {
                if let Ok(h) = hour_min[0].parse::<u32>() {
                    if let Ok(m) = hour_min[1].parse::<u32>() {
                        return format!("{:02}:{:02}", h, m);
                    }
                }
            }
        }
        cleaned.to_string()
    } else {
        iso.to_string()
    }
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
