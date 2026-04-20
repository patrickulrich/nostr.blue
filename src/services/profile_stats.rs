use serde::{Deserialize, Serialize};
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
#[cfg(feature = "web")]
use wasm_bindgen_futures::JsFuture;
#[cfg(feature = "web")]
use web_sys::{Request, RequestInit, RequestMode, Response};
#[cfg(feature = "web")]
const NOSTR_ARCHIVES_API: &str = "https://api.nostrarchives.com";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileStats {
    pub pubkey: String,
    pub followers_pubkey_count: Option<u64>,
}
#[cfg(feature = "web")]
#[derive(Debug, Clone, Deserialize)]
struct SocialListResponse {
    count: i64,
    #[serde(default)]
    #[allow(dead_code)]
    pubkeys: Vec<String>,
}
#[cfg(feature = "web")]
#[derive(Debug, Clone, Deserialize)]
struct SocialGraphResponse {
    followers: SocialListResponse,
}
#[cfg(feature = "web")]
pub async fn fetch_profile_stats(pubkey: &str) -> Result<ProfileStats, String> {
    let url = format!(
        "{}/v1/social/{}?followers_limit=0&follows_limit=0",
        NOSTR_ARCHIVES_API, pubkey
    );
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);
    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("Failed to set header: {:?}", e))?;
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
    let json = JsFuture::from(
        resp.json()
            .map_err(|e| format!("Failed to get JSON: {:?}", e))?,
    )
    .await
    .map_err(|e| format!("Failed to parse JSON: {:?}", e))?;
    let response: SocialGraphResponse = serde_wasm_bindgen::from_value(json)
        .map_err(|e| format!("Failed to deserialize: {:?}", e))?;
    Ok(ProfileStats {
        pubkey: pubkey.to_string(),
        followers_pubkey_count: Some(response.followers.count as u64),
    })
}

#[cfg(not(feature = "web"))]
pub async fn fetch_profile_stats(pubkey: &str) -> Result<ProfileStats, String> {
    Err(format!(
        "Profile stats not yet supported on native for pubkey: {}",
        pubkey
    ))
}
