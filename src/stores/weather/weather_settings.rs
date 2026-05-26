use dioxus::prelude::*;

use crate::platform::storage;
use crate::services::weather::units::*;

const SETTINGS_KEY: &str = "nostr_blue_weather_settings";

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct WeatherSettings {
    #[serde(default)]
    pub temperature_unit: TemperatureUnit,
    #[serde(default)]
    pub wind_speed_unit: WindSpeedUnit,
    #[serde(default)]
    pub pressure_unit: PressureUnit,
    #[serde(default)]
    pub precipitation_unit: PrecipitationUnit,
    #[serde(default)]
    pub distance_unit: DistanceUnit,
}

pub static WEATHER_SETTINGS: GlobalSignal<WeatherSettings> = Signal::global(WeatherSettings::default);

pub fn init_settings() {
    if let Ok(settings) = storage::get::<WeatherSettings>(SETTINGS_KEY) {
        *WEATHER_SETTINGS.write() = settings;
    }
}

pub fn save_settings(settings: &WeatherSettings) {
    *WEATHER_SETTINGS.write() = settings.clone();
    let _ = storage::set(SETTINGS_KEY, settings);
}
