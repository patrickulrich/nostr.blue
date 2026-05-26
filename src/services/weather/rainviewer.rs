use serde::Deserialize;

use crate::platform::http::http_client;

#[derive(Clone, Debug, Deserialize)]
pub struct RainviewerMaps {
    pub host: String,
    pub radar: RadarFrames,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RadarFrames {
    pub past: Vec<RadarFrame>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RadarFrame {
    pub time: u64,
    pub path: String,
}

pub async fn fetch_radar_maps() -> Result<RainviewerMaps, String> {
    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .get("https://api.rainviewer.com/public/weather-maps.json")
        .send()
        .await
        .map_err(|e| format!("RainViewer request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("RainViewer API returned status {}", response.status()));
    }

    response
        .json::<RainviewerMaps>()
        .await
        .map_err(|e| format!("RainViewer parse error: {}", e))
}
