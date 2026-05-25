use serde::Deserialize;

use super::types::{
    CurrentWeather, DailyForecast, HourlyForecast,
    MinutelyForecast, WeatherData, WmoCode,
};
use crate::platform::http::http_client;

pub async fn fetch_forecast(
    lat: f64,
    lon: f64,
) -> Result<WeatherData, String> {
    let current_params = "temperature_2m,apparent_temperature,weather_code,wind_speed_10m,wind_direction_10m,wind_gusts_10m,uv_index,relative_humidity_2m,dew_point_2m,pressure_msl,cloud_cover,visibility,precipitation,rain,snowfall,is_day";
    let hourly_params = "temperature_2m,apparent_temperature,precipitation_probability,precipitation,rain,showers,snowfall,weather_code,wind_speed_10m,wind_direction_10m,wind_gusts_10m,uv_index,is_day,relative_humidity_2m,dew_point_2m,pressure_msl,cloud_cover,visibility";
    let daily_params = "weather_code,temperature_2m_max,temperature_2m_min,apparent_temperature_max,apparent_temperature_min,sunrise,sunset,daylight_duration,sunshine_duration,uv_index_max,precipitation_sum,rain_sum,snowfall_sum,precipitation_probability_max,precipitation_hours,wind_speed_10m_max,wind_gusts_10m_max,wind_direction_10m_dominant,relative_humidity_2m_mean,dew_point_2m_mean,pressure_msl_mean,cloud_cover_mean,visibility_mean";

    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current={}&hourly={}&daily={}&minutely_15=precipitation&forecast_days=16&past_days=1&timezone=auto&models=best_match",
        lat, lon, current_params, hourly_params, daily_params
    );

    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Forecast request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Forecast API error {}: {}", status, body));
    }

    let raw: OpenMeteoResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse forecast: {}", e))?;

    let current = parse_current(&raw);
    let hourly = parse_hourly(&raw);
    let daily = parse_daily(&raw);
    let minutely = parse_minutely(&raw);

    Ok(WeatherData {
        current,
        hourly,
        daily,
        minutely,
        air_quality: None,
        alerts: vec![],
        fetched_at: crate::platform::timestamp::now_secs(),
        timezone: raw.timezone,
        utc_offset_seconds: raw.utc_offset_seconds,
    })
}

fn parse_current(raw: &OpenMeteoResponse) -> CurrentWeather {
    let c = match &raw.current {
        Some(c) => c,
        None => {
            return CurrentWeather {
                time: String::new(),
                temperature: 0.0,
                feels_like: 0.0,
                weather_code: WmoCode::ClearSky,
                is_day: true,
                wind_speed: 0.0,
                wind_direction: 0,
                wind_gusts: 0.0,
                relative_humidity: 0,
                dew_point: 0.0,
                pressure: 0.0,
                cloud_cover: 0,
                visibility: 0.0,
                uv_index: 0.0,
                precipitation: 0.0,
                rain: 0.0,
                snowfall: 0.0,
            }
        }
    };

    CurrentWeather {
        time: c.time.clone().unwrap_or_default(),
        temperature: c.temperature_2m.unwrap_or(0.0),
        feels_like: c.apparent_temperature.unwrap_or(0.0),
        weather_code: WmoCode::from_code(c.weather_code.unwrap_or(0) as u8),
        is_day: c.is_day.unwrap_or(1) == 1,
        wind_speed: c.wind_speed_10m.unwrap_or(0.0),
        wind_direction: c.wind_direction_10m.unwrap_or(0.0) as i32,
        wind_gusts: c.wind_gusts_10m.unwrap_or(0.0),
        relative_humidity: c.relative_humidity_2m.unwrap_or(0.0) as i32,
        dew_point: c.dew_point_2m.unwrap_or(0.0),
        pressure: c.pressure_msl.unwrap_or(0.0),
        cloud_cover: c.cloud_cover.unwrap_or(0.0) as i32,
        visibility: c.visibility.unwrap_or(0.0),
        uv_index: c.uv_index.unwrap_or(0.0),
        precipitation: c.precipitation.unwrap_or(0.0),
        rain: c.rain.unwrap_or(0.0),
        snowfall: c.snowfall.unwrap_or(0.0),
    }
}

fn parse_hourly(raw: &OpenMeteoResponse) -> Vec<HourlyForecast> {
    let h = match &raw.hourly {
        Some(h) => h,
        None => return vec![],
    };
    let len = h.time.len().min(h.temperature_2m.len());
    (0..len)
        .map(|i| HourlyForecast {
            time: h.time.get(i).cloned().unwrap_or_default(),
            temperature: h.temperature_2m.get(i).copied().flatten().unwrap_or(0.0),
            feels_like: h.apparent_temperature.get(i).copied().flatten().unwrap_or(0.0),
            weather_code: WmoCode::from_code(
                h.weather_code.get(i).copied().flatten().unwrap_or(0) as u8,
            ),
            is_day: h.is_day.get(i).copied().flatten().unwrap_or(1) == 1,
            wind_speed: h.wind_speed_10m.get(i).copied().flatten().unwrap_or(0.0),
            wind_direction: h.wind_direction_10m.get(i).copied().flatten().unwrap_or(0.0) as i32,
            wind_gusts: h.wind_gusts_10m.get(i).copied().flatten().unwrap_or(0.0),
            relative_humidity: h.relative_humidity_2m.get(i).copied().flatten().unwrap_or(0.0) as i32,
            dew_point: h.dew_point_2m.get(i).copied().flatten().unwrap_or(0.0),
            pressure: h.pressure_msl.get(i).copied().flatten().unwrap_or(0.0),
            cloud_cover: h.cloud_cover.get(i).copied().flatten().unwrap_or(0.0) as i32,
            visibility: h.visibility.get(i).copied().flatten().unwrap_or(0.0),
            uv_index: h.uv_index.get(i).copied().flatten().unwrap_or(0.0),
            precipitation: h.precipitation.get(i).copied().flatten().unwrap_or(0.0),
            precipitation_probability: h
                .precipitation_probability
                .get(i)
                .copied()
                .flatten()
                .unwrap_or(0.0) as i32,
            rain: h.rain.get(i).copied().flatten().unwrap_or(0.0)
                + h.showers.get(i).copied().flatten().unwrap_or(0.0),
            snowfall: h.snowfall.get(i).copied().flatten().unwrap_or(0.0),
            pm10: None,
            pm2_5: None,
            carbon_monoxide: None,
            nitrogen_dioxide: None,
            sulphur_dioxide: None,
            ozone: None,
            alder_pollen: None,
            birch_pollen: None,
            grass_pollen: None,
            mugwort_pollen: None,
            olive_pollen: None,
            ragweed_pollen: None,
        })
        .collect()
}

fn parse_daily(raw: &OpenMeteoResponse) -> Vec<DailyForecast> {
    let d = match &raw.daily {
        Some(d) => d,
        None => return vec![],
    };
    let len = d.time.len().min(d.temperature_2m_max.len());
    (0..len)
        .map(|i| DailyForecast {
            date: d.time.get(i).cloned().unwrap_or_default(),
            weather_code: WmoCode::from_code(
                d.weather_code.get(i).copied().flatten().unwrap_or(0) as u8,
            ),
            temperature_max: d.temperature_2m_max.get(i).copied().flatten().unwrap_or(0.0),
            temperature_min: d.temperature_2m_min.get(i).copied().flatten().unwrap_or(0.0),
            feels_like_max: d
                .apparent_temperature_max
                .get(i)
                .copied()
                .flatten()
                .unwrap_or(0.0),
            feels_like_min: d
                .apparent_temperature_min
                .get(i)
                .copied()
                .flatten()
                .unwrap_or(0.0),
            sunrise: d.sunrise.get(i).cloned().unwrap_or_default(),
            sunset: d.sunset.get(i).cloned().unwrap_or_default(),
            sunshine_duration: d.sunshine_duration.get(i).copied().flatten().unwrap_or(0.0),
            daylight_duration: d.daylight_duration.get(i).copied().flatten().unwrap_or(0.0),
            uv_index_max: d.uv_index_max.get(i).copied().flatten().unwrap_or(0.0),
            precipitation_sum: d.precipitation_sum.get(i).copied().flatten().unwrap_or(0.0),
            rain_sum: d.rain_sum.get(i).copied().flatten().unwrap_or(0.0),
            snowfall_sum: d.snowfall_sum.get(i).copied().flatten().unwrap_or(0.0),
            precipitation_probability_max: d
                .precipitation_probability_max
                .get(i)
                .copied()
                .flatten()
                .unwrap_or(0.0) as i32,
            precipitation_hours: d.precipitation_hours.get(i).copied().flatten().unwrap_or(0.0),
            wind_speed_max: d.wind_speed_10m_max.get(i).copied().flatten().unwrap_or(0.0),
            wind_gusts_max: d.wind_gusts_10m_max.get(i).copied().flatten().unwrap_or(0.0),
            wind_direction_dominant: d
                .wind_direction_10m_dominant
                .get(i)
                .copied()
                .flatten()
                .unwrap_or(0.0) as i32,
            relative_humidity_mean: d
                .relative_humidity_2m_mean
                .get(i)
                .copied()
                .flatten()
                .unwrap_or(0.0) as i32,
            dew_point_mean: d.dew_point_2m_mean.get(i).copied().flatten().unwrap_or(0.0),
            pressure_mean: d.pressure_msl_mean.get(i).copied().flatten().unwrap_or(0.0),
            cloud_cover_mean: d.cloud_cover_mean.get(i).copied().flatten().unwrap_or(0.0) as i32,
            visibility_mean: d.visibility_mean.get(i).copied().flatten().unwrap_or(0.0),
        })
        .collect()
}

fn parse_minutely(raw: &OpenMeteoResponse) -> Vec<MinutelyForecast> {
    let m = match &raw.minutely_15 {
        Some(m) => m,
        None => return vec![],
    };
    let len = m.time.len().min(m.precipitation.len());
    (0..len)
        .map(|i| MinutelyForecast {
            time: m.time.get(i).cloned().unwrap_or_default(),
            precipitation: m.precipitation.get(i).copied().flatten().unwrap_or(0.0),
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct OpenMeteoResponse {
    #[allow(dead_code)]
    latitude: f64,
    #[allow(dead_code)]
    longitude: f64,
    timezone: String,
    #[allow(dead_code)]
    timezone_abbreviation: Option<String>,
    utc_offset_seconds: i32,
    current: Option<OpenMeteoCurrent>,
    hourly: Option<OpenMeteoHourly>,
    daily: Option<OpenMeteoDaily>,
    minutely_15: Option<OpenMeteoMinutely>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoCurrent {
    time: Option<String>,
    #[serde(default)]
    temperature_2m: Option<f64>,
    #[serde(default)]
    apparent_temperature: Option<f64>,
    #[serde(default)]
    weather_code: Option<u32>,
    #[serde(default)]
    is_day: Option<u32>,
    #[serde(default)]
    wind_speed_10m: Option<f64>,
    #[serde(default)]
    wind_direction_10m: Option<f64>,
    #[serde(default)]
    wind_gusts_10m: Option<f64>,
    #[serde(default)]
    relative_humidity_2m: Option<f64>,
    #[serde(default)]
    dew_point_2m: Option<f64>,
    #[serde(default)]
    pressure_msl: Option<f64>,
    #[serde(default)]
    cloud_cover: Option<f64>,
    #[serde(default)]
    visibility: Option<f64>,
    #[serde(default)]
    uv_index: Option<f64>,
    #[serde(default)]
    precipitation: Option<f64>,
    #[serde(default)]
    rain: Option<f64>,
    #[serde(default)]
    snowfall: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoHourly {
    #[serde(default)]
    time: Vec<String>,
    #[serde(default)]
    temperature_2m: Vec<Option<f64>>,
    #[serde(default)]
    apparent_temperature: Vec<Option<f64>>,
    #[serde(default)]
    weather_code: Vec<Option<u32>>,
    #[serde(default)]
    is_day: Vec<Option<u32>>,
    #[serde(default)]
    wind_speed_10m: Vec<Option<f64>>,
    #[serde(default)]
    wind_direction_10m: Vec<Option<f64>>,
    #[serde(default)]
    wind_gusts_10m: Vec<Option<f64>>,
    #[serde(default)]
    relative_humidity_2m: Vec<Option<f64>>,
    #[serde(default)]
    dew_point_2m: Vec<Option<f64>>,
    #[serde(default)]
    pressure_msl: Vec<Option<f64>>,
    #[serde(default)]
    cloud_cover: Vec<Option<f64>>,
    #[serde(default)]
    visibility: Vec<Option<f64>>,
    #[serde(default)]
    uv_index: Vec<Option<f64>>,
    #[serde(default)]
    precipitation: Vec<Option<f64>>,
    #[serde(default)]
    precipitation_probability: Vec<Option<f64>>,
    #[serde(default)]
    rain: Vec<Option<f64>>,
    #[serde(default)]
    showers: Vec<Option<f64>>,
    #[serde(default)]
    snowfall: Vec<Option<f64>>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoDaily {
    #[serde(default)]
    time: Vec<String>,
    #[serde(default)]
    weather_code: Vec<Option<u32>>,
    #[serde(default)]
    temperature_2m_max: Vec<Option<f64>>,
    #[serde(default)]
    temperature_2m_min: Vec<Option<f64>>,
    #[serde(default)]
    apparent_temperature_max: Vec<Option<f64>>,
    #[serde(default)]
    apparent_temperature_min: Vec<Option<f64>>,
    #[serde(default)]
    sunrise: Vec<String>,
    #[serde(default)]
    sunset: Vec<String>,
    #[serde(default)]
    daylight_duration: Vec<Option<f64>>,
    #[serde(default)]
    sunshine_duration: Vec<Option<f64>>,
    #[serde(default)]
    uv_index_max: Vec<Option<f64>>,
    #[serde(default)]
    precipitation_sum: Vec<Option<f64>>,
    #[serde(default)]
    rain_sum: Vec<Option<f64>>,
    #[serde(default)]
    snowfall_sum: Vec<Option<f64>>,
    #[serde(default)]
    precipitation_probability_max: Vec<Option<f64>>,
    #[serde(default)]
    precipitation_hours: Vec<Option<f64>>,
    #[serde(default)]
    wind_speed_10m_max: Vec<Option<f64>>,
    #[serde(default)]
    wind_gusts_10m_max: Vec<Option<f64>>,
    #[serde(default)]
    wind_direction_10m_dominant: Vec<Option<f64>>,
    #[serde(default)]
    relative_humidity_2m_mean: Vec<Option<f64>>,
    #[serde(default)]
    dew_point_2m_mean: Vec<Option<f64>>,
    #[serde(default)]
    pressure_msl_mean: Vec<Option<f64>>,
    #[serde(default)]
    cloud_cover_mean: Vec<Option<f64>>,
    #[serde(default)]
    visibility_mean: Vec<Option<f64>>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoMinutely {
    #[serde(default)]
    time: Vec<String>,
    #[serde(default)]
    precipitation: Vec<Option<f64>>,
}
