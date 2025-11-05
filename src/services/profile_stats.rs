use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};
use std::collections::HashMap;

const NOSTR_BAND_API: &str = "https://api.nostr.band";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileStats {
    pub pubkey: String,
    pub followers_pubkey_count: Option<u64>,
    // Add other fields if needed in the future
    // pub pub_following_pubkey_count: Option<u64>,
    // pub pub_note_count: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct NostrBandStatsResponse {
    stats: HashMap<String, ProfileStats>,
}

/// Fetch profile statistics from Nostr.Band API
/// Returns followers count and other profile statistics
pub async fn fetch_profile_stats(pubkey: &str) -> Result<ProfileStats, String> {
    let url = format!("{}/v0/stats/profile/{}", NOSTR_BAND_API, pubkey);

    // Create request
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;

    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("Failed to set header: {:?}", e))?;

    // Fetch from API
    let window = web_sys::window().ok_or("No window object")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;

    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| "Failed to cast to Response")?;

    if !resp.ok() {
        return Err(format!("API returned status: {}", resp.status()));
    }

    // Parse JSON response
    let json = JsFuture::from(resp.json().map_err(|e| format!("Failed to get JSON: {:?}", e))?)
        .await
        .map_err(|e| format!("Failed to parse JSON: {:?}", e))?;

    let response: NostrBandStatsResponse = serde_wasm_bindgen::from_value(json)
        .map_err(|e| format!("Failed to deserialize: {:?}", e))?;

    // Extract stats for this pubkey
    response.stats.get(pubkey)
        .cloned()
        .ok_or_else(|| format!("No stats found for pubkey: {}", pubkey))
}
