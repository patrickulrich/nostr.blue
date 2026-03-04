//! Geocoding Service
//!
//! Uses Photon API (based on OpenStreetMap) for geocoding.
//! Includes localStorage caching to reduce API calls.
#[cfg(feature = "web")]
use crate::platform::http::http_client;
#[cfg(feature = "web")]
use crate::platform::storage;
use serde::{Deserialize, Serialize};
#[cfg(feature = "web")]
use std::collections::HashMap;

/// Photon API endpoint
#[cfg(feature = "web")]
const PHOTON_API_URL: &str = "https://photon.komoot.io/api";
/// Cache key for localStorage
#[cfg(feature = "web")]
const CACHE_KEY: &str = "nostr_blue_geocode_cache";
/// Cache expiry time (7 days in seconds)
#[cfg(feature = "web")]
const CACHE_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;
/// Suggestion cache key for localStorage
#[cfg(feature = "web")]
const SUGGEST_CACHE_KEY: &str = "nostr_blue_suggest_cache";
/// Suggestion cache expiry time (1 hour in seconds)
#[cfg(feature = "web")]
const SUGGEST_CACHE_EXPIRY_SECS: u64 = 60 * 60;
/// Minimum interval between Nominatim requests (1 second in ms)
#[cfg(feature = "web")]
const NOMINATIM_THROTTLE_MS: f64 = 1000.0;
/// Geocoding result with coordinates
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeoLocation {
    /// Latitude
    pub lat: f64,
    /// Longitude
    pub lon: f64,
    /// Display name
    pub display_name: String,
    /// City/Town
    pub city: Option<String>,
    /// State/Region
    pub state: Option<String>,
    /// Country
    pub country: Option<String>,
    /// Country code (ISO 3166-1 alpha-2)
    pub country_code: Option<String>,
    /// Type of place (city, street, etc.)
    pub place_type: Option<String>,
}
/// Cached geocoding result
#[cfg(feature = "web")]
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CachedGeoResult {
    /// The location data
    location: Option<GeoLocation>,
    /// Timestamp when cached (Unix seconds)
    cached_at: u64,
}
/// Photon API response
#[cfg(feature = "web")]
#[derive(Debug, Deserialize)]
struct PhotonResponse {
    features: Vec<PhotonFeature>,
}
#[cfg(feature = "web")]
#[derive(Debug, Deserialize)]
struct PhotonFeature {
    geometry: PhotonGeometry,
    properties: PhotonProperties,
}
#[cfg(feature = "web")]
#[derive(Debug, Deserialize)]
struct PhotonGeometry {
    coordinates: Vec<f64>,
}
#[cfg(feature = "web")]
#[derive(Debug, Deserialize)]
struct PhotonProperties {
    name: Option<String>,
    city: Option<String>,
    state: Option<String>,
    country: Option<String>,
    countrycode: Option<String>,
    #[serde(rename = "type")]
    place_type: Option<String>,
    street: Option<String>,
    housenumber: Option<String>,
}
/// Geocoding cache
#[cfg(feature = "web")]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct GeoCache {
    entries: HashMap<String, CachedGeoResult>,
}
/// Cached suggestion results
#[cfg(feature = "web")]
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CachedSuggestions {
    locations: Vec<GeoLocation>,
    cached_at: u64,
}
/// Suggestion cache
#[cfg(feature = "web")]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct SuggestCache {
    entries: HashMap<String, CachedSuggestions>,
}
/// Track last Nominatim request time (module-level via thread_local)
#[cfg(feature = "web")]
use std::cell::Cell;
#[cfg(feature = "web")]
thread_local! {
    static LAST_NOMINATIM_REQUEST: Cell<f64> = const { Cell::new(0.0) };
}
/// Load cache from localStorage
#[cfg(feature = "web")]
fn load_cache() -> GeoCache {
    storage::get(CACHE_KEY).unwrap_or_default()
}
/// Save cache to localStorage
#[cfg(feature = "web")]
fn save_cache(cache: &GeoCache) {
    if let Err(e) = storage::set(CACHE_KEY, cache) {
        log::warn!("Failed to save geocode cache: {}", e);
    }
}
/// Get current Unix timestamp
#[allow(dead_code)]
fn now_secs() -> u64 {
    crate::platform::timestamp::now_secs()
}
/// Check if a cached result is still valid
#[cfg(feature = "web")]
fn is_valid_cache(cached: &CachedGeoResult) -> bool {
    let age = now_secs().saturating_sub(cached.cached_at);
    age < CACHE_EXPIRY_SECS
}
/// Normalize query for cache key
#[allow(dead_code)]
fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}
/// Geocode a location string to coordinates
///
/// Returns cached result if available, otherwise queries Photon API.
#[cfg(feature = "web")]
pub async fn geocode(query: &str) -> Result<Option<GeoLocation>, String> {
    let normalized = normalize_query(query);
    if normalized.is_empty() {
        return Ok(None);
    }
    let mut cache = load_cache();
    if let Some(cached) = cache.entries.get(&normalized) {
        if is_valid_cache(cached) {
            log::debug!("Geocode cache hit for: {}", query);
            return Ok(cached.location.clone());
        }
    }
    let result = query_photon(&normalized).await;
    // Only cache successful responses (including Ok(None) real misses)
    if let Ok(ref location) = result {
        cache
            .entries
            .insert(
                normalized,
                CachedGeoResult {
                    location: location.clone(),
                    cached_at: now_secs(),
                },
            );
        save_cache(&cache);
    }
    result
}

/// Stub implementation for non-web platforms
#[cfg(not(feature = "web"))]
#[allow(dead_code)]
pub async fn geocode(_query: &str) -> Result<Option<GeoLocation>, String> {
    Err("Geocoding is only available on web".to_string())
}

/// Query Photon API for geocoding
#[cfg(feature = "web")]
async fn query_photon(query: &str) -> Result<Option<GeoLocation>, String> {
    let encoded = urlencoding::encode(query);
    let url = format!("{}?q={}&limit=1", PHOTON_API_URL, encoded);
    let response = http_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch geocode: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("Geocode API error: {}", response.status()));
    }
    let photon: PhotonResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse geocode response: {}", e))?;
    Ok(photon.features.first().and_then(feature_to_location))
}
/// Convert Photon feature to GeoLocation
#[cfg(feature = "web")]
fn feature_to_location(feature: &PhotonFeature) -> Option<GeoLocation> {
    let props = &feature.properties;
    let coords = &feature.geometry.coordinates;
    let mut name_parts = Vec::new();
    if let Some(name) = &props.name {
        name_parts.push(name.clone());
    }
    if let Some(street) = &props.street {
        if let Some(num) = &props.housenumber {
            name_parts.push(format!("{} {}", num, street));
        } else {
            name_parts.push(street.clone());
        }
    }
    if let Some(city) = &props.city {
        if !name_parts.iter().any(|p| p == city) {
            name_parts.push(city.clone());
        }
    }
    if let Some(state) = &props.state {
        if !name_parts.iter().any(|p| p == state) {
            name_parts.push(state.clone());
        }
    }
    if let Some(country) = &props.country {
        if !name_parts.iter().any(|p| p == country) {
            name_parts.push(country.clone());
        }
    }
    let lat = *coords.get(1)?; // GeoJSON is [lon, lat]
    let lon = *coords.first()?;
    let display_name = if name_parts.is_empty() {
        format!("{:.4}, {:.4}", lat, lon)
    } else {
        name_parts.join(", ")
    };
    Some(GeoLocation {
        lat,
        lon,
        display_name,
        city: props.city.clone(),
        state: props.state.clone(),
        country: props.country.clone(),
        country_code: props.countrycode.clone(),
        place_type: props.place_type.clone(),
    })
}
/// Nominatim API endpoint (OpenStreetMap — better venue/business name search than Photon)
#[cfg(feature = "web")]
const NOMINATIM_API_URL: &str = "https://nominatim.openstreetmap.org/search";

/// Nominatim address details (returned when `addressdetails=1`)
#[cfg(feature = "web")]
#[derive(Debug, Deserialize)]
struct NominatimAddress {
    city: Option<String>,
    town: Option<String>,
    village: Option<String>,
    state: Option<String>,
    country: Option<String>,
    country_code: Option<String>,
}

/// Nominatim API response item
#[cfg(feature = "web")]
#[derive(Debug, Deserialize)]
struct NominatimResult {
    display_name: String,
    lat: String,
    lon: String,
    #[serde(rename = "type")]
    place_type: Option<String>,
    address: Option<NominatimAddress>,
}

/// Load suggestion cache from localStorage
#[cfg(feature = "web")]
fn load_suggest_cache() -> SuggestCache {
    storage::get(SUGGEST_CACHE_KEY).unwrap_or_default()
}
/// Save suggestion cache to localStorage
#[cfg(feature = "web")]
fn save_suggest_cache(cache: &SuggestCache) {
    if let Err(e) = storage::set(SUGGEST_CACHE_KEY, cache) {
        log::warn!("Failed to save suggest cache: {}", e);
    }
}
/// Check if a cached suggestion result is still valid
#[cfg(feature = "web")]
fn is_valid_suggest_cache(cached: &CachedSuggestions) -> bool {
    let age = now_secs().saturating_sub(cached.cached_at);
    age < SUGGEST_CACHE_EXPIRY_SECS
}
/// Search for location suggestions (for autocomplete)
///
/// Uses Nominatim (OpenStreetMap) which handles venue/business name queries
/// much better than Photon. Includes caching (1-hour TTL) and 1-second
/// throttle to comply with Nominatim usage policy.
#[cfg(feature = "web")]
pub async fn geocode_suggestions(query: &str, limit: u8) -> Result<Vec<GeoLocation>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() || limit == 0 {
        return Ok(vec![]);
    }
    let normalized = format!("{}|{}", normalize_query(trimmed), limit);
    // Check suggestion cache first
    let mut cache = load_suggest_cache();
    if let Some(cached) = cache.entries.get(&normalized) {
        if is_valid_suggest_cache(cached) {
            log::debug!("Suggest cache hit for: {}", trimmed);
            return Ok(cached.locations.clone());
        }
    }
    // Enforce 1-second throttle between Nominatim requests
    let now_ms = crate::platform::timestamp::now_millis() as f64;
    let elapsed = LAST_NOMINATIM_REQUEST.with(|last| {
        let prev = last.get();
        now_ms - prev
    });
    if elapsed < NOMINATIM_THROTTLE_MS {
        // Too soon — return cached stale results if available, or empty
        if let Some(cached) = cache.entries.get(&normalized) {
            return Ok(cached.locations.clone());
        }
        return Ok(vec![]);
    }
    LAST_NOMINATIM_REQUEST.with(|last| last.set(now_ms));
    let encoded = urlencoding::encode(trimmed);
    let url = format!(
        "{}?format=json&q={}&limit={}&addressdetails=1&email=contact@nostr.blue",
        NOMINATIM_API_URL, encoded, limit
    );
    let response = http_client()
        .get(&url)
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("User-Agent", "nostr.blue/0.8 (https://nostr.blue)")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch suggestions: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("Geocode API error: {}", response.status()));
    }
    let results: Vec<NominatimResult> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    let locations: Vec<GeoLocation> = results
        .into_iter()
        .filter_map(|r| {
            let lat = r.lat.parse::<f64>().ok()?;
            let lon = r.lon.parse::<f64>().ok()?;
            let (city, state, country, country_code) = match &r.address {
                Some(addr) => (
                    addr.city
                        .clone()
                        .or_else(|| addr.town.clone())
                        .or_else(|| addr.village.clone()),
                    addr.state.clone(),
                    addr.country.clone(),
                    addr.country_code.clone(),
                ),
                None => (None, None, None, None),
            };
            Some(GeoLocation {
                lat,
                lon,
                display_name: r.display_name,
                city,
                state,
                country,
                country_code,
                place_type: r.place_type,
            })
        })
        .collect();
    // Cache the results
    cache.entries.insert(
        normalized,
        CachedSuggestions {
            locations: locations.clone(),
            cached_at: now_secs(),
        },
    );
    save_suggest_cache(&cache);
    Ok(locations)
}

/// Stub implementation for non-web platforms
#[cfg(not(feature = "web"))]
pub async fn geocode_suggestions(_query: &str, _limit: u8) -> Result<Vec<GeoLocation>, String> {
    Err("Geocoding suggestions are only available on web".to_string())
}

/// Decode a geohash to coordinates (center point)
#[allow(dead_code)]
pub fn geohash_to_coords(geohash: &str) -> Option<(f64, f64)> {
    if geohash.is_empty() {
        return None;
    }
    const BASE32: &[u8] = b"0123456789bcdefghjkmnpqrstuvwxyz";
    let mut lat_range = (-90.0, 90.0);
    let mut lon_range = (-180.0, 180.0);
    let mut is_lon = true;
    for c in geohash.chars() {
        let idx = BASE32.iter().position(|&b| b == c as u8)?;
        for bit in (0..5).rev() {
            let mid = if is_lon {
                let m = (lon_range.0 + lon_range.1) / 2.0;
                if idx & (1 << bit) != 0 {
                    lon_range.0 = m;
                } else {
                    lon_range.1 = m;
                }
                m
            } else {
                let m = (lat_range.0 + lat_range.1) / 2.0;
                if idx & (1 << bit) != 0 {
                    lat_range.0 = m;
                } else {
                    lat_range.1 = m;
                }
                m
            };
            let _ = mid;
            is_lon = !is_lon;
        }
    }
    let lat = (lat_range.0 + lat_range.1) / 2.0;
    let lon = (lon_range.0 + lon_range.1) / 2.0;
    Some((lat, lon))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_geohash_decode() {
        let coords = geohash_to_coords("dr5r").unwrap();
        assert!((coords.0 - 40.7).abs() < 0.5);
        assert!((coords.1 - (-74.0)).abs() < 0.5);
    }
    #[test]
    fn test_normalize_query() {
        assert_eq!(normalize_query("  New York  "), "new york");
        assert_eq!(normalize_query("PARIS"), "paris");
    }
}
