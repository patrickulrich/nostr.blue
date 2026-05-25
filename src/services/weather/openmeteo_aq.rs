use serde::Deserialize;

use super::types::AirQualityData;
use crate::platform::http::http_client;

pub async fn fetch_air_quality(lat: f64, lon: f64) -> Result<AirQualityData, String> {
    let hourly_params = "pm10,pm2_5,carbon_monoxide,nitrogen_dioxide,sulphur_dioxide,ozone,alder_pollen,birch_pollen,grass_pollen,mugwort_pollen,olive_pollen,ragweed_pollen";

    let url = format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={}&longitude={}&hourly={}&forecast_days=5&past_days=1&timezone=auto",
        lat, lon, hourly_params
    );

    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Air quality request failed: {}", e))?;

    if !response.status().is_success() {
        return Ok(AirQualityData { hourly: vec![] });
    }

    let raw: AirQualityResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse air quality: {}", e))?;

    let hourly = parse_hourly_aq(&raw);
    Ok(AirQualityData { hourly })
}

fn parse_hourly_aq(raw: &AirQualityResponse) -> Vec<super::types::AirQualityHourly> {
    let h = match &raw.hourly {
        Some(h) => h,
        None => return vec![],
    };
    let len = h.time.len();
    (0..len)
        .map(|i| super::types::AirQualityHourly {
            time: h.time.get(i).cloned().unwrap_or_default(),
            pm10: h.pm10.get(i).copied().flatten(),
            pm2_5: h.pm2_5.get(i).copied().flatten(),
            carbon_monoxide: h.carbon_monoxide.get(i).copied().flatten(),
            nitrogen_dioxide: h.nitrogen_dioxide.get(i).copied().flatten(),
            sulphur_dioxide: h.sulphur_dioxide.get(i).copied().flatten(),
            ozone: h.ozone.get(i).copied().flatten(),
            alder_pollen: h.alder_pollen.get(i).copied().flatten(),
            birch_pollen: h.birch_pollen.get(i).copied().flatten(),
            grass_pollen: h.grass_pollen.get(i).copied().flatten(),
            mugwort_pollen: h.mugwort_pollen.get(i).copied().flatten(),
            olive_pollen: h.olive_pollen.get(i).copied().flatten(),
            ragweed_pollen: h.ragweed_pollen.get(i).copied().flatten(),
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct AirQualityResponse {
    hourly: Option<AirQualityHourlyRaw>,
}

#[derive(Debug, Deserialize)]
struct AirQualityHourlyRaw {
    #[serde(default)]
    time: Vec<String>,
    #[serde(default)]
    pm10: Vec<Option<f64>>,
    #[serde(default)]
    pm2_5: Vec<Option<f64>>,
    #[serde(default)]
    carbon_monoxide: Vec<Option<f64>>,
    #[serde(default)]
    nitrogen_dioxide: Vec<Option<f64>>,
    #[serde(default)]
    sulphur_dioxide: Vec<Option<f64>>,
    #[serde(default)]
    ozone: Vec<Option<f64>>,
    #[serde(default)]
    alder_pollen: Vec<Option<f64>>,
    #[serde(default)]
    birch_pollen: Vec<Option<f64>>,
    #[serde(default)]
    grass_pollen: Vec<Option<f64>>,
    #[serde(default)]
    mugwort_pollen: Vec<Option<f64>>,
    #[serde(default)]
    olive_pollen: Vec<Option<f64>>,
    #[serde(default)]
    ragweed_pollen: Vec<Option<f64>>,
}
