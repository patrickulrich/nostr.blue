use dioxus::prelude::*;

use crate::platform::storage;
use crate::services::weather::LocationCandidate;

const LOCATIONS_KEY: &str = "nostr_blue_weather_locations";
const SELECTED_KEY: &str = "nostr_blue_weather_selected";

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SavedLocation {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub timezone: Option<String>,
    pub country: Option<String>,
    pub admin1: Option<String>,
    pub is_current_gps: bool,
}

pub static LOCATIONS: GlobalSignal<Vec<SavedLocation>> = Signal::global(Vec::new);
pub static SELECTED_LOCATION_INDEX: GlobalSignal<usize> = Signal::global(|| 0);

pub fn init_from_cache() {
    if let Ok(locs) = storage::get::<Vec<SavedLocation>>(LOCATIONS_KEY) {
        if !locs.is_empty() {
            *LOCATIONS.write() = locs;
        }
    }
    if let Ok(idx) = storage::get::<usize>(SELECTED_KEY) {
        if idx < LOCATIONS.read().len() {
            *SELECTED_LOCATION_INDEX.write() = idx;
        }
    }
}

fn save_to_cache() {
    let _ = storage::set(LOCATIONS_KEY, &*LOCATIONS.read());
    let _ = storage::set(SELECTED_KEY, &*SELECTED_LOCATION_INDEX.read());
}

pub fn add_location(loc: SavedLocation) {
    let exists = LOCATIONS.read().iter().any(|l| {
        (l.lat - loc.lat).abs() < 0.01 && (l.lon - loc.lon).abs() < 0.01
    });
    if exists {
        return;
    }
    LOCATIONS.write().push(loc);
    save_to_cache();
}

pub fn add_location_from_candidate(candidate: &LocationCandidate) -> SavedLocation {
    let loc = SavedLocation {
        id: format!("{}_{}", candidate.id, candidate.latitude),
        name: format_location_name(&candidate.name, candidate.admin1.as_deref(), candidate.country.as_deref()),
        lat: candidate.latitude,
        lon: candidate.longitude,
        timezone: candidate.timezone.clone(),
        country: candidate.country.clone(),
        admin1: candidate.admin1.clone(),
        is_current_gps: false,
    };
    add_location(loc.clone());
    loc
}

pub fn remove_location(id: &str) {
    let mut locs = LOCATIONS.write();
    if let Some(pos) = locs.iter().position(|l| l.id == id) {
        locs.remove(pos);
        let mut idx = SELECTED_LOCATION_INDEX.write();
        if *idx >= locs.len() && !locs.is_empty() {
            *idx = locs.len() - 1;
        }
        drop(idx);
        drop(locs);
        save_to_cache();
    }
}

pub fn select_location(index: usize) {
    *SELECTED_LOCATION_INDEX.write() = index;
    save_to_cache();
}

pub fn get_selected() -> Option<SavedLocation> {
    let locs = LOCATIONS.read();
    let idx = *SELECTED_LOCATION_INDEX.read();
    locs.get(idx).cloned()
}

pub async fn init_gps_location() -> Result<SavedLocation, String> {
    let (lat, lon) = crate::platform::geolocation::get_current_position().await?;

    let locs = LOCATIONS.write();
    if let Some(existing) = locs.iter().find(|l| l.is_current_gps) {
        let id = existing.id.clone();
        drop(locs);
        return Ok(SavedLocation {
            id,
            lat,
            lon,
            ..get_selected().unwrap_or_else(|| SavedLocation {
                id: "gps".to_string(),
                name: "Current Location".to_string(),
                lat,
                lon,
                timezone: None,
                country: None,
                admin1: None,
                is_current_gps: true,
            })
        });
    }
    drop(locs);

    let loc = SavedLocation {
        id: "gps".to_string(),
        name: "Current Location".to_string(),
        lat,
        lon,
        timezone: None,
        country: None,
        admin1: None,
        is_current_gps: true,
    };
    add_location(loc.clone());
    Ok(loc)
}

fn format_location_name(name: &str, admin1: Option<&str>, country: Option<&str>) -> String {
    match (admin1, country) {
        (Some(a), Some(c)) => format!("{}, {}, {}", name, a, c),
        (None, Some(c)) => format!("{}, {}", name, c),
        (Some(a), None) => format!("{}, {}", name, a),
        _ => name.to_string(),
    }
}
