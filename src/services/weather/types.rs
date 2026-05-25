use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeatherData {
    pub current: CurrentWeather,
    pub hourly: Vec<HourlyForecast>,
    pub daily: Vec<DailyForecast>,
    pub minutely: Vec<MinutelyForecast>,
    pub air_quality: Option<AirQualityData>,
    pub alerts: Vec<WeatherAlert>,
    pub fetched_at: u64,
    pub timezone: String,
    pub utc_offset_seconds: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurrentWeather {
    pub time: String,
    pub temperature: f64,
    pub feels_like: f64,
    pub weather_code: WmoCode,
    pub is_day: bool,
    pub wind_speed: f64,
    pub wind_direction: i32,
    pub wind_gusts: f64,
    pub relative_humidity: i32,
    pub dew_point: f64,
    pub pressure: f64,
    pub cloud_cover: i32,
    pub visibility: f64,
    pub uv_index: f64,
    pub precipitation: f64,
    pub rain: f64,
    pub snowfall: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HourlyForecast {
    pub time: String,
    pub temperature: f64,
    pub feels_like: f64,
    pub weather_code: WmoCode,
    pub is_day: bool,
    pub wind_speed: f64,
    pub wind_direction: i32,
    pub wind_gusts: f64,
    pub relative_humidity: i32,
    pub dew_point: f64,
    pub pressure: f64,
    pub cloud_cover: i32,
    pub visibility: f64,
    pub uv_index: f64,
    pub precipitation: f64,
    pub precipitation_probability: i32,
    pub rain: f64,
    pub snowfall: f64,
    pub pm10: Option<f64>,
    pub pm2_5: Option<f64>,
    pub carbon_monoxide: Option<f64>,
    pub nitrogen_dioxide: Option<f64>,
    pub sulphur_dioxide: Option<f64>,
    pub ozone: Option<f64>,
    pub alder_pollen: Option<f64>,
    pub birch_pollen: Option<f64>,
    pub grass_pollen: Option<f64>,
    pub mugwort_pollen: Option<f64>,
    pub olive_pollen: Option<f64>,
    pub ragweed_pollen: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DailyForecast {
    pub date: String,
    pub weather_code: WmoCode,
    pub temperature_max: f64,
    pub temperature_min: f64,
    pub feels_like_max: f64,
    pub feels_like_min: f64,
    pub sunrise: String,
    pub sunset: String,
    pub sunshine_duration: f64,
    pub daylight_duration: f64,
    pub uv_index_max: f64,
    pub precipitation_sum: f64,
    pub rain_sum: f64,
    pub snowfall_sum: f64,
    pub precipitation_probability_max: i32,
    pub precipitation_hours: f64,
    pub wind_speed_max: f64,
    pub wind_gusts_max: f64,
    pub wind_direction_dominant: i32,
    pub relative_humidity_mean: i32,
    pub dew_point_mean: f64,
    pub pressure_mean: f64,
    pub cloud_cover_mean: i32,
    pub visibility_mean: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MinutelyForecast {
    pub time: String,
    pub precipitation: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AirQualityData {
    pub hourly: Vec<AirQualityHourly>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AirQualityHourly {
    pub time: String,
    pub pm10: Option<f64>,
    pub pm2_5: Option<f64>,
    pub carbon_monoxide: Option<f64>,
    pub nitrogen_dioxide: Option<f64>,
    pub sulphur_dioxide: Option<f64>,
    pub ozone: Option<f64>,
    pub alder_pollen: Option<f64>,
    pub birch_pollen: Option<f64>,
    pub grass_pollen: Option<f64>,
    pub mugwort_pollen: Option<f64>,
    pub olive_pollen: Option<f64>,
    pub ragweed_pollen: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeatherAlert {
    pub id: String,
    pub event: String,
    pub headline: Option<String>,
    pub description: Option<String>,
    pub instruction: Option<String>,
    pub severity: AlertSeverity,
    pub urgency: String,
    pub certainty: String,
    pub area_desc: Option<String>,
    pub effective: Option<String>,
    pub expires: Option<String>,
    pub sender_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Extreme,
    Severe,
    Moderate,
    Minor,
    Unknown,
}

impl AlertSeverity {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "extreme" => Self::Extreme,
            "severe" => Self::Severe,
            "moderate" => Self::Moderate,
            "minor" => Self::Minor,
            _ => Self::Unknown,
        }
    }

    pub fn color_class(&self) -> &'static str {
        match self {
            Self::Extreme => "bg-red-500",
            Self::Severe => "bg-orange-500",
            Self::Moderate => "bg-yellow-500",
            Self::Minor => "bg-blue-500",
            Self::Unknown => "bg-gray-500",
        }
    }

    pub fn border_class(&self) -> &'static str {
        match self {
            Self::Extreme => "border-l-red-500",
            Self::Severe => "border-l-orange-500",
            Self::Moderate => "border-l-yellow-500",
            Self::Minor => "border-l-blue-500",
            Self::Unknown => "border-l-gray-500",
        }
    }

    pub fn text_class(&self) -> &'static str {
        match self {
            Self::Extreme => "text-red-600 dark:text-red-400",
            Self::Severe => "text-orange-600 dark:text-orange-400",
            Self::Moderate => "text-yellow-600 dark:text-yellow-400",
            Self::Minor => "text-blue-600 dark:text-blue-400",
            Self::Unknown => "text-gray-600 dark:text-gray-400",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum WmoCode {
    ClearSky,
    MainlyClear,
    PartlyCloudy,
    Overcast,
    Fog,
    DepositingRimeFog,
    DrizzleLight,
    DrizzleModerate,
    DrizzleDense,
    FreezingDrizzleLight,
    FreezingDrizzleDense,
    RainSlight,
    RainModerate,
    RainHeavy,
    FreezingRainLight,
    FreezingRainHeavy,
    SnowSlight,
    SnowModerate,
    SnowHeavy,
    SnowGrains,
    RainShowersSlight,
    RainShowersModerate,
    RainShowersViolent,
    SnowShowersSlight,
    SnowShowersHeavy,
    Thunderstorm,
    ThunderstormSlightHail,
    ThunderstormHeavyHail,
}

impl WmoCode {
    pub fn from_code(code: u8) -> Self {
        match code {
            0 => Self::ClearSky,
            1 => Self::MainlyClear,
            2 => Self::PartlyCloudy,
            3 => Self::Overcast,
            45 => Self::Fog,
            48 => Self::DepositingRimeFog,
            51 => Self::DrizzleLight,
            53 => Self::DrizzleModerate,
            55 => Self::DrizzleDense,
            56 => Self::FreezingDrizzleLight,
            57 => Self::FreezingDrizzleDense,
            61 => Self::RainSlight,
            63 => Self::RainModerate,
            65 => Self::RainHeavy,
            66 => Self::FreezingRainLight,
            67 => Self::FreezingRainHeavy,
            71 => Self::SnowSlight,
            73 => Self::SnowModerate,
            75 => Self::SnowHeavy,
            77 => Self::SnowGrains,
            80 => Self::RainShowersSlight,
            81 => Self::RainShowersModerate,
            82 => Self::RainShowersViolent,
            85 => Self::SnowShowersSlight,
            86 => Self::SnowShowersHeavy,
            95 => Self::Thunderstorm,
            96 => Self::ThunderstormSlightHail,
            99 => Self::ThunderstormHeavyHail,
            _ => Self::ClearSky,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::ClearSky => "Clear sky",
            Self::MainlyClear => "Mainly clear",
            Self::PartlyCloudy => "Partly cloudy",
            Self::Overcast => "Overcast",
            Self::Fog => "Fog",
            Self::DepositingRimeFog => "Rime fog",
            Self::DrizzleLight => "Light drizzle",
            Self::DrizzleModerate => "Moderate drizzle",
            Self::DrizzleDense => "Dense drizzle",
            Self::FreezingDrizzleLight => "Light freezing drizzle",
            Self::FreezingDrizzleDense => "Dense freezing drizzle",
            Self::RainSlight => "Slight rain",
            Self::RainModerate => "Moderate rain",
            Self::RainHeavy => "Heavy rain",
            Self::FreezingRainLight => "Light freezing rain",
            Self::FreezingRainHeavy => "Heavy freezing rain",
            Self::SnowSlight => "Slight snow",
            Self::SnowModerate => "Moderate snow",
            Self::SnowHeavy => "Heavy snow",
            Self::SnowGrains => "Snow grains",
            Self::RainShowersSlight => "Slight rain showers",
            Self::RainShowersModerate => "Moderate rain showers",
            Self::RainShowersViolent => "Violent rain showers",
            Self::SnowShowersSlight => "Slight snow showers",
            Self::SnowShowersHeavy => "Heavy snow showers",
            Self::Thunderstorm => "Thunderstorm",
            Self::ThunderstormSlightHail => "Thunderstorm with hail",
            Self::ThunderstormHeavyHail => "Thunderstorm with heavy hail",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            Self::ClearSky => "\u{2600}\u{FE0F}",
            Self::MainlyClear => "\u{1F324}\u{FE0F}",
            Self::PartlyCloudy => "\u{26C5}",
            Self::Overcast => "\u{2601}\u{FE0F}",
            Self::Fog | Self::DepositingRimeFog => "\u{1F32B}\u{FE0F}",
            Self::DrizzleLight | Self::DrizzleModerate | Self::DrizzleDense => "\u{1F327}\u{FE0F}",
            Self::FreezingDrizzleLight | Self::FreezingDrizzleDense => "\u{1F328}\u{FE0F}",
            Self::RainSlight | Self::RainModerate => "\u{1F327}\u{FE0F}",
            Self::RainHeavy => "\u{1F327}\u{FE0F}",
            Self::FreezingRainLight | Self::FreezingRainHeavy => "\u{1F328}\u{FE0F}",
            Self::SnowSlight | Self::SnowModerate | Self::SnowHeavy | Self::SnowGrains => "\u{2744}\u{FE0F}",
            Self::RainShowersSlight | Self::RainShowersModerate => "\u{1F326}\u{FE0F}",
            Self::RainShowersViolent => "\u{1F327}\u{FE0F}",
            Self::SnowShowersSlight | Self::SnowShowersHeavy => "\u{1F328}\u{FE0F}",
            Self::Thunderstorm | Self::ThunderstormSlightHail | Self::ThunderstormHeavyHail => "\u{26C8}\u{FE0F}",
        }
    }

    pub fn category(&self) -> WeatherCategory {
        match self {
            Self::ClearSky | Self::MainlyClear => WeatherCategory::Clear,
            Self::PartlyCloudy | Self::Overcast => WeatherCategory::Cloudy,
            Self::Fog | Self::DepositingRimeFog => WeatherCategory::Fog,
            Self::DrizzleLight
            | Self::DrizzleModerate
            | Self::DrizzleDense
            | Self::RainSlight
            | Self::RainModerate
            | Self::RainHeavy
            | Self::RainShowersSlight
            | Self::RainShowersModerate
            | Self::RainShowersViolent => WeatherCategory::Rain,
            Self::FreezingDrizzleLight
            | Self::FreezingDrizzleDense
            | Self::FreezingRainLight
            | Self::FreezingRainHeavy => WeatherCategory::FreezingRain,
            Self::SnowSlight
            | Self::SnowModerate
            | Self::SnowHeavy
            | Self::SnowGrains
            | Self::SnowShowersSlight
            | Self::SnowShowersHeavy => WeatherCategory::Snow,
            Self::ThunderstormSlightHail | Self::ThunderstormHeavyHail => WeatherCategory::Thunderstorm,
            Self::Thunderstorm => WeatherCategory::Thunder,
        }
    }

    pub fn gradient_classes(&self, is_day: bool) -> &'static str {
        match (self.category(), is_day) {
            (WeatherCategory::Clear, true) => "from-blue-400 to-sky-200",
            (WeatherCategory::Clear, false) => "from-indigo-900 to-blue-900",
            (WeatherCategory::Cloudy, true) => "from-gray-400 to-gray-300",
            (WeatherCategory::Cloudy, false) => "from-gray-700 to-gray-600",
            (WeatherCategory::Fog, true) => "from-gray-300 to-gray-200",
            (WeatherCategory::Fog, false) => "from-gray-600 to-gray-500",
            (WeatherCategory::Rain, true) => "from-gray-500 to-blue-600",
            (WeatherCategory::Rain, false) => "from-gray-700 to-blue-900",
            (WeatherCategory::FreezingRain, _) => "from-gray-400 to-blue-300",
            (WeatherCategory::Snow, true) => "from-gray-200 to-blue-100",
            (WeatherCategory::Snow, false) => "from-gray-500 to-blue-800",
            (WeatherCategory::Thunder, _) => "from-gray-600 to-yellow-700",
            (WeatherCategory::Thunderstorm, _) => "from-gray-800 to-purple-900",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherCategory {
    Clear,
    Cloudy,
    Fog,
    Rain,
    FreezingRain,
    Snow,
    Thunder,
    Thunderstorm,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LocationCandidate {
    pub id: u64,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub country: Option<String>,
    pub admin1: Option<String>,
    pub timezone: Option<String>,
}
