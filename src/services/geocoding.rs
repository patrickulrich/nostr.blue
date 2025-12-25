//! Geocoding Service
//!
//! Uses Photon API (based on OpenStreetMap) for geocoding.
//! Includes localStorage caching to reduce API calls.

use gloo_net::http::Request;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Photon API endpoint
const PHOTON_API_URL: &str = "https://photon.komoot.io/api";

/// Cache key for localStorage
const CACHE_KEY: &str = "nostr_blue_geocode_cache";

/// Cache expiry time (7 days in seconds)
const CACHE_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;

// ============================================================================
// Types
// ============================================================================

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
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CachedGeoResult {
    /// The location data
    location: Option<GeoLocation>,
    /// Timestamp when cached (Unix seconds)
    cached_at: u64,
}

/// Photon API response
#[derive(Debug, Deserialize)]
struct PhotonResponse {
    features: Vec<PhotonFeature>,
}

#[derive(Debug, Deserialize)]
struct PhotonFeature {
    geometry: PhotonGeometry,
    properties: PhotonProperties,
}

#[derive(Debug, Deserialize)]
struct PhotonGeometry {
    coordinates: Vec<f64>, // [lon, lat]
}

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
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct GeoCache {
    entries: HashMap<String, CachedGeoResult>,
}

// ============================================================================
// Cache Functions
// ============================================================================

/// Load cache from localStorage
fn load_cache() -> GeoCache {
    LocalStorage::get(CACHE_KEY).unwrap_or_default()
}

/// Save cache to localStorage
fn save_cache(cache: &GeoCache) {
    if let Err(e) = LocalStorage::set(CACHE_KEY, cache) {
        log::warn!("Failed to save geocode cache: {}", e);
    }
}

/// Get current Unix timestamp
fn now_secs() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}

/// Check if a cached result is still valid
fn is_valid_cache(cached: &CachedGeoResult) -> bool {
    let age = now_secs().saturating_sub(cached.cached_at);
    age < CACHE_EXPIRY_SECS
}

/// Normalize query for cache key
fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}

// ============================================================================
// Public Functions
// ============================================================================

/// Geocode a location string to coordinates
///
/// Returns cached result if available, otherwise queries Photon API.
pub async fn geocode(query: &str) -> Result<Option<GeoLocation>, String> {
    let normalized = normalize_query(query);

    if normalized.is_empty() {
        return Ok(None);
    }

    // Check cache first
    let mut cache = load_cache();
    if let Some(cached) = cache.entries.get(&normalized) {
        if is_valid_cache(cached) {
            log::debug!("Geocode cache hit for: {}", query);
            return Ok(cached.location.clone());
        }
    }

    // Query Photon API
    let result = query_photon(&normalized).await;

    // Cache the result (even if None, to avoid repeated failed lookups)
    let location = result.as_ref().ok().cloned().flatten();
    cache.entries.insert(normalized, CachedGeoResult {
        location: location.clone(),
        cached_at: now_secs(),
    });
    save_cache(&cache);

    result
}

// ============================================================================
// Internal Functions
// ============================================================================

/// Query Photon API for geocoding
async fn query_photon(query: &str) -> Result<Option<GeoLocation>, String> {
    let encoded = urlencoding::encode(query);
    let url = format!(
        "{}?q={}&limit=1",
        PHOTON_API_URL, encoded
    );

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch geocode: {}", e))?;

    if !response.ok() {
        return Err(format!("Geocode API error: {}", response.status()));
    }

    let photon: PhotonResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse geocode response: {}", e))?;

    Ok(photon.features.first().map(feature_to_location))
}

/// Convert Photon feature to GeoLocation
fn feature_to_location(feature: &PhotonFeature) -> GeoLocation {
    let props = &feature.properties;
    let coords = &feature.geometry.coordinates;

    // Build display name from available parts
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

    let display_name = if name_parts.is_empty() {
        format!("{:.4}, {:.4}", coords[1], coords[0])
    } else {
        name_parts.join(", ")
    };

    GeoLocation {
        lat: coords[1],
        lon: coords[0],
        display_name,
        city: props.city.clone(),
        state: props.state.clone(),
        country: props.country.clone(),
        country_code: props.countrycode.clone(),
        place_type: props.place_type.clone(),
    }
}

/// Decode a geohash to coordinates (center point)
pub fn geohash_to_coords(geohash: &str) -> Option<(f64, f64)> {
    if geohash.is_empty() {
        return None;
    }

    // Geohash base32 alphabet
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
            let _ = mid; // suppress unused warning
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
        // New York City area
        let coords = geohash_to_coords("dr5r").unwrap();
        assert!((coords.0 - 40.7).abs() < 0.5); // lat
        assert!((coords.1 - (-74.0)).abs() < 0.5); // lon
    }

    #[test]
    fn test_normalize_query() {
        assert_eq!(normalize_query("  New York  "), "new york");
        assert_eq!(normalize_query("PARIS"), "paris");
    }
}
