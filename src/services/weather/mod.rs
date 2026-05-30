pub mod openmeteo;
pub mod openmeteo_aq;
pub mod nws_alerts;
pub mod rainviewer;
pub mod types;
pub mod units;

pub use types::*;

const MAX_RETRIES: u32 = 2;
const RETRY_DELAY_MS: u32 = 2000;

pub async fn fetch_all_weather(
    lat: f64,
    lon: f64,
) -> Result<WeatherData, String> {
    let (forecast_res, aq_res, alerts_res) = futures::join!(
        retry_request(|| openmeteo::fetch_forecast(lat, lon)),
        retry_request(|| openmeteo_aq::fetch_air_quality(lat, lon)),
        retry_request(|| nws_alerts::fetch_active_alerts(lat, lon)),
    );

    let mut weather = forecast_res?;
    weather.air_quality = aq_res.ok();
    weather.alerts = alerts_res.unwrap_or_default();

    if let Some(ref aq) = weather.air_quality {
        merge_aq_into_hourly(&mut weather.hourly, &aq.hourly);
    }

    Ok(weather)
}

async fn retry_request<F, Fut, T>(f: F) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let mut last_err = String::new();
    for attempt in 0..=MAX_RETRIES {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_err = e;
                if attempt < MAX_RETRIES {
                    let delay = RETRY_DELAY_MS * (1 << attempt);
                    crate::platform::timer::sleep_ms(delay).await;
                }
            }
        }
    }
    Err(last_err)
}

fn merge_aq_into_hourly(
    hourly: &mut [HourlyForecast],
    aq_hourly: &[AirQualityHourly],
) {
    let aq_map: std::collections::HashMap<&str, &AirQualityHourly> = aq_hourly
        .iter()
        .map(|a| (a.time.as_str(), a))
        .collect();

    for h in hourly.iter_mut() {
        let key = h.time.strip_suffix(":00").unwrap_or(&h.time);
        let key_full = format!("{}:00", key);
        if let Some(aq) = aq_map.get(h.time.as_str()).or_else(|| aq_map.get(key_full.as_str())) {
            h.pm10 = aq.pm10;
            h.pm2_5 = aq.pm2_5;
            h.carbon_monoxide = aq.carbon_monoxide;
            h.nitrogen_dioxide = aq.nitrogen_dioxide;
            h.sulphur_dioxide = aq.sulphur_dioxide;
            h.ozone = aq.ozone;
            h.alder_pollen = aq.alder_pollen;
            h.birch_pollen = aq.birch_pollen;
            h.grass_pollen = aq.grass_pollen;
            h.mugwort_pollen = aq.mugwort_pollen;
            h.olive_pollen = aq.olive_pollen;
            h.ragweed_pollen = aq.ragweed_pollen;
        }
    }
}
