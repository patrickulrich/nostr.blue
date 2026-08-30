//! Workout units resolution and display formatting.
//!
//! Unit preference comes from the `workout_units` setting ("auto" |
//! "metric" | "imperial"); "auto" resolves from the viewer's locale the
//! same way Amethyst's `phonePrefersMiles` does (region in US/GB/LR/MM).
use crate::stores::ui::settings_store;
use crate::utils::nips::nip101e::{KILOGRAMS_PER_POUND, METERS_PER_FOOT, METERS_PER_MILE};
use dioxus::prelude::ReadableExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkoutUnits {
    Metric,
    Imperial,
}

/// Countries that conventionally use miles.
#[cfg(feature = "web")]
const MILES_COUNTRIES: [&str; 4] = ["US", "GB", "LR", "MM"];

/// True when the platform locale suggests imperial units.
/// Web reads `navigator.languages`; native defaults to metric.
pub fn locale_prefers_imperial() -> bool {
    #[cfg(feature = "web")]
    {
        if let Some(window) = web_sys::window() {
            let navigator = window.navigator();
            let mut tags: Vec<String> = Vec::new();
            for lang in navigator.languages().iter() {
                if let Some(s) = lang.as_string() {
                    tags.push(s);
                }
            }
            if tags.is_empty() {
                if let Some(language) = navigator.language() {
                    tags.push(language);
                }
            }
            for tag in tags {
                let region = tag
                    .rsplit(['-', '_'])
                    .next()
                    .unwrap_or("")
                    .to_uppercase();
                if region.len() == 2 && MILES_COUNTRIES.contains(&region.as_str()) {
                    return true;
                }
            }
        }
    }
    false
}

/// Resolve the viewer's effective workout units from the settings +
/// locale.
pub fn effective_units() -> WorkoutUnits {
    let pref = settings_store::SETTINGS.read().workout_units.clone();
    match pref.as_str() {
        "metric" => WorkoutUnits::Metric,
        "imperial" => WorkoutUnits::Imperial,
        _ => {
            if locale_prefers_imperial() {
                WorkoutUnits::Imperial
            } else {
                WorkoutUnits::Metric
            }
        }
    }
}

fn meters_per_unit(units: WorkoutUnits) -> f64 {
    match units {
        WorkoutUnits::Imperial => METERS_PER_MILE,
        WorkoutUnits::Metric => 1000.0,
    }
}

fn unit_name(units: WorkoutUnits) -> &'static str {
    match units {
        WorkoutUnits::Imperial => "mi",
        WorkoutUnits::Metric => "km",
    }
}

/// (value, unit) pair for big hero display; distance rounded to 2 dp.
pub fn format_distance_parts(meters: f64, units: WorkoutUnits) -> (String, &'static str) {
    let value = match units {
        WorkoutUnits::Imperial => (meters / METERS_PER_MILE * 100.0).round() / 100.0,
        WorkoutUnits::Metric => (meters / 1000.0 * 100.0).round() / 100.0,
    };
    (trim_value(value), unit_name(units))
}

/// Print a float without a trailing `.0` when it is a whole number.
fn trim_value(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// Whole-unit elevation (feet when imperial, meters otherwise).
pub fn format_elevation(meters: f64, units: WorkoutUnits) -> String {
    match units {
        WorkoutUnits::Imperial => format!("{} ft", (meters / METERS_PER_FOOT).round() as i64),
        WorkoutUnits::Metric => format!("{} m", meters.round() as i64),
    }
}

/// Weight formatted in the viewer's units at 1 dp. Inputs are kilograms
/// (exercise-group sets stay kg-canonical; convert per-viewer here).
pub fn format_weight_kg(kg: f64, units: WorkoutUnits) -> String {
    match units {
        WorkoutUnits::Imperial => {
            let lbs = (kg / KILOGRAMS_PER_POUND * 10.0).round() / 10.0;
            format!("{} lbs", trim_value(lbs))
        }
        WorkoutUnits::Metric => {
            let kg = (kg * 10.0).round() / 10.0;
            format!("{} kg", trim_value(kg))
        }
    }
}

/// Pace as `M:SS /km` (or `/mi`) with integer seconds per unit,
/// matching Amethyst's display semantics.
pub fn pace_label(duration_seconds: u64, meters: f64, units: WorkoutUnits) -> String {
    if meters <= 0.0 {
        return String::new();
    }
    let spu = (duration_seconds as f64 / (meters / meters_per_unit(units))) as u64;
    format!("{}:{:02} /{}", spu / 60, spu % 60, unit_name(units))
}

/// Average speed label, e.g. `24.5 km/h` / `14.8 mph`.
pub fn speed_label(duration_seconds: u64, meters: f64, units: WorkoutUnits) -> String {
    let hours = duration_seconds as f64 / 3600.0;
    if hours <= 0.0 {
        return String::new();
    }
    let value = meters / meters_per_unit(units) / hours;
    format!(
        "{} {}/h",
        trim_value((value * 10.0).round() / 10.0),
        unit_name(units)
    )
}

