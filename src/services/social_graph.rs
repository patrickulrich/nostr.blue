use serde::{Deserialize, Serialize};
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
#[cfg(feature = "web")]
use wasm_bindgen_futures::JsFuture;
#[cfg(feature = "web")]
use web_sys::{Request, RequestInit, RequestMode, Response};

const NOSTR_ARCHIVES_API: &str = "https://api.nostrarchives.com";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SocialListResponse {
    pub count: i64,
    #[serde(default)]
    pub pubkeys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialGraphResponse {
    #[serde(default)]
    pub follows: SocialListResponse,
    #[serde(default)]
    pub followers: SocialListResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMetadata {
    pub pubkey: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub picture: Option<String>,
    #[serde(default)]
    pub about: Option<String>,
    #[serde(default)]
    pub nip05: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BatchMetadataRequest {
    pubkeys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BatchMetadataResponse {
    profiles: Vec<ProfileMetadata>,
}

#[cfg(feature = "web")]
pub async fn fetch_social_graph(
    pubkey: &str,
    follows_limit: i64,
    follows_offset: i64,
    followers_limit: i64,
    followers_offset: i64,
) -> Result<SocialGraphResponse, String> {
    let url = format!(
        "{}/v1/social/{}?follows_limit={}&follows_offset={}&followers_limit={}&followers_offset={}",
        NOSTR_ARCHIVES_API, pubkey, follows_limit, follows_offset, followers_limit, followers_offset
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
    Ok(response)
}

#[cfg(not(feature = "web"))]
pub async fn fetch_social_graph(
    pubkey: &str,
    follows_limit: i64,
    follows_offset: i64,
    followers_limit: i64,
    followers_offset: i64,
) -> Result<SocialGraphResponse, String> {
    let url = format!(
        "{}/v1/social/{}?follows_limit={}&follows_offset={}&followers_limit={}&followers_offset={}",
        NOSTR_ARCHIVES_API, pubkey, follows_limit, follows_offset, followers_limit, followers_offset
    );
    let client = crate::platform::http::http_client()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Fetch failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("API returned status: {}", resp.status()));
    }
    let response: SocialGraphResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to deserialize: {}", e))?;
    Ok(response)
}

#[cfg(feature = "web")]
pub async fn fetch_profiles_metadata(
    pubkeys: Vec<String>,
) -> Result<Vec<ProfileMetadata>, String> {
    if pubkeys.is_empty() {
        return Ok(Vec::new());
    }
    let url = format!("{}/v1/profiles/metadata", NOSTR_ARCHIVES_API);
    let body = serde_json::to_string(&BatchMetadataRequest { pubkeys })
        .map_err(|e| format!("Failed to serialize: {:?}", e))?;
    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body));
    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("Failed to set header: {:?}", e))?;
    request
        .headers()
        .set("Content-Type", "application/json")
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
    let response: BatchMetadataResponse = serde_wasm_bindgen::from_value(json)
        .map_err(|e| format!("Failed to deserialize: {:?}", e))?;
    Ok(response.profiles)
}

#[cfg(not(feature = "web"))]
pub async fn fetch_profiles_metadata(
    pubkeys: Vec<String>,
) -> Result<Vec<ProfileMetadata>, String> {
    if pubkeys.is_empty() {
        return Ok(Vec::new());
    }
    let url = format!("{}/v1/profiles/metadata", NOSTR_ARCHIVES_API);
    let client = crate::platform::http::http_client()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    let resp = client
        .post(&url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .json(&BatchMetadataRequest { pubkeys })
        .send()
        .await
        .map_err(|e| format!("Fetch failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("API returned status: {}", resp.status()));
    }
    let response: BatchMetadataResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to deserialize: {}", e))?;
    Ok(response.profiles)
}
