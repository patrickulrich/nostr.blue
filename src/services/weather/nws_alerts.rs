use super::types::{LocationCandidate, WeatherAlert, AlertSeverity};
use crate::platform::http::http_client;

pub async fn fetch_active_alerts(lat: f64, lon: f64) -> Result<Vec<WeatherAlert>, String> {
    let url = format!(
        "https://api.weather.gov/alerts/active?point={},{}",
        lat, lon
    );

    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .get(&url)
        .header("User-Agent", "(nostr.blue, https://nostr.blue)")
        .header("Accept", "application/ld+json")
        .send()
        .await;

    let response = match response {
        Ok(r) => r,
        Err(_) => return Ok(vec![]),
    };

    if !response.status().is_success() {
        return Ok(vec![]);
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse NWS response: {}", e))?;

    let features = body
        .get("@graph")
        .and_then(|g| g.as_array())
        .cloned()
        .or_else(|| {
            body.get("features")
                .and_then(|f| f.as_array())
                .cloned()
        })
        .unwrap_or_default();

    Ok(features
        .iter()
        .filter_map(|f| {
            let props = f.get("properties").or_else(|| {
                if f.get("id").is_some() || f.get("event").is_some() {
                    Some(f)
                } else {
                    None
                }
            })?;
            Some(WeatherAlert {
                id: props
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                event: props
                    .get("event")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Weather Alert")
                    .to_string(),
                headline: props.get("headline").and_then(|v| v.as_str()).map(String::from),
                description: props
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                instruction: props
                    .get("instruction")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                severity: AlertSeverity::from_str(
                    props.get("severity").and_then(|v| v.as_str()).unwrap_or("Unknown"),
                ),
                urgency: props
                    .get("urgency")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string(),
                certainty: props
                    .get("certainty")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string(),
                area_desc: props
                    .get("areaDesc")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                effective: props
                    .get("effective")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                expires: props.get("expires").and_then(|v| v.as_str()).map(String::from),
                sender_name: props
                    .get("senderName")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            })
        })
        .collect())
}

pub async fn search_locations(query: &str) -> Result<Vec<LocationCandidate>, String> {
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=10&language=en&format=json",
        urlencoding::encode(query)
    );

    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Geocoding request failed: {}", e))?;

    if !response.status().is_success() {
        return Ok(vec![]);
    }

    #[derive(Deserialize)]
    struct GeoResponse {
        #[serde(default)]
        results: Option<Vec<GeoResult>>,
    }

    #[derive(Deserialize)]
    struct GeoResult {
        id: u64,
        name: String,
        latitude: f64,
        longitude: f64,
        #[serde(default)]
        country: Option<String>,
        #[serde(default)]
        admin1: Option<String>,
        #[serde(default)]
        timezone: Option<String>,
    }

    let data: GeoResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse geocoding: {}", e))?;

    Ok(data
        .results
        .unwrap_or_default()
        .into_iter()
        .map(|r| LocationCandidate {
            id: r.id,
            name: r.name,
            latitude: r.latitude,
            longitude: r.longitude,
            country: r.country,
            admin1: r.admin1,
            timezone: r.timezone,
        })
        .collect())
}

use serde::Deserialize;
