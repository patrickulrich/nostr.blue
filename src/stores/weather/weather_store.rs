use dioxus::prelude::*;

use super::location_store;
use crate::services::weather::{fetch_all_weather, WeatherData};

pub static WEATHER_DATA: GlobalSignal<std::collections::HashMap<String, WeatherData>> =
    Signal::global(std::collections::HashMap::new);
pub static WEATHER_LOADING: GlobalSignal<bool> = Signal::global(|| false);
pub static WEATHER_ERROR: GlobalSignal<Option<String>> = Signal::global(|| None);

pub async fn fetch_weather_for_current_location() -> Result<(), String> {
    let loc = location_store::get_selected().ok_or("No location selected")?;
    *WEATHER_LOADING.write() = true;
    *WEATHER_ERROR.write() = None;

    match fetch_all_weather(loc.lat, loc.lon).await {
        Ok(data) => {
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
