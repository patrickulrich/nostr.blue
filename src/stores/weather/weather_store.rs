use dioxus::prelude::*;

use super::location_store;
use crate::platform::storage;
use crate::services::weather::{fetch_all_weather, WeatherData};

const WEATHER_CACHE_KEY_PREFIX: &str = "nostr_blue_weather_data_";
const WEATHER_CACHE_TTL_SECS: u64 = 3600;

pub static WEATHER_DATA: GlobalSignal<std::collections::HashMap<String, WeatherData>> =
    Signal::global(std::collections::HashMap::new);
pub static WEATHER_LOADING: GlobalSignal<bool> = Signal::global(|| false);
pub static WEATHER_ERROR: GlobalSignal<Option<String>> = Signal::global(|| None);

pub fn init_from_cache() {
    let locs = location_store::LOCATIONS.read();
    for loc in locs.iter() {
        let key = format!("{}{}", WEATHER_CACHE_KEY_PREFIX, loc.id);
        if let Ok(data) = storage::get::<WeatherData>(&key) {
            let age = crate::platform::timestamp::now_secs().saturating_sub(data.fetched_at);
            if age < WEATHER_CACHE_TTL_SECS * 3 {
                WEATHER_DATA.write().insert(loc.id.clone(), data);
            } else {
                let _ = storage::delete(&key);
            }
        }
    }
}

pub async fn fetch_weather_for_current_location() -> Result<(), String> {
    let loc = location_store::get_selected().ok_or("No location selected")?;
    *WEATHER_LOADING.write() = true;
    *WEATHER_ERROR.write() = None;

    match fetch_all_weather(loc.lat, loc.lon).await {
        Ok(data) => {
            let key = format!("{}{}", WEATHER_CACHE_KEY_PREFIX, loc.id);
            let _ = storage::set(&key, &data);
            WEATHER_DATA.write().insert(loc.id.clone(), data);
            *WEATHER_LOADING.write() = false;
            Ok(())
        }
        Err(e) => {
            *WEATHER_ERROR.write() = Some(e);
            *WEATHER_LOADING.write() = false;
            Err(WEATHER_ERROR.read().clone().unwrap_or_default())
        }
    }
}

pub fn get_current_weather() -> Option<WeatherData> {
    let loc = location_store::get_selected()?;
    WEATHER_DATA.read().get(&loc.id).cloned()
}

pub fn get_weather_for_location(id: &str) -> Option<WeatherData> {
    WEATHER_DATA.read().get(id).cloned()
}
