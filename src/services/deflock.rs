use crate::platform::http::http_client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AlprCamera {
    pub osm_id: u64,
    pub lat: f64,
    pub lon: f64,
    pub operator: Option<String>,
    pub brand: Option<String>,
    pub direction: Option<f64>,
    pub direction_cardinal: Option<String>,
    pub surveillance_zone: Option<String>,
    pub mount_type: Option<String>,
    pub ref_id: Option<String>,
    pub start_date: Option<String>,
    pub osm_timestamp: Option<String>,
    pub osm_version: Option<u32>,
    pub wikimedia_commons: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct BoundingBox {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

impl BoundingBox {
    pub fn from_center_radius(lat: f64, lon: f64, radius_km: f64) -> Self {
        let lat_delta = radius_km / 111.0;
        let lon_delta = radius_km / (111.0 * lat.to_radians().cos().max(0.01));
        Self {
            south: (lat - lat_delta).max(-90.0),
            north: (lat + lat_delta).min(90.0),
            west: (lon - lon_delta).max(-180.0),
            east: (lon + lon_delta).min(180.0),
        }
    }

    #[allow(dead_code)]
    pub fn overlaps(&self, other: &BoundingBox) -> bool {
        self.south <= other.north
            && self.north >= other.south
            && self.west <= other.east
            && self.east >= other.west
    }
}

#[derive(Clone, Debug, Deserialize)]
struct OverpassResponse {
    elements: Vec<OverpassElement>,
}

#[derive(Clone, Debug, Deserialize)]
struct OverpassElement {
    id: u64,
    lat: f64,
    lon: f64,
    #[serde(default)]
    tags: Option<HashMap<String, String>>,
}

fn degree_to_cardinal(deg: f64) -> String {
    let dirs = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    let idx = (((deg % 360.0) + 360.0) % 360.0 / 45.0).round() as usize % 8;
    dirs[idx].to_string()
}

fn parse_direction(tags: &HashMap<String, String>) -> (Option<f64>, Option<String>) {
    let dir_str = tags
        .get("direction")
        .or_else(|| tags.get("camera:direction"))
        .or_else(|| tags.get("surveillance:direction"));

    match dir_str {
        Some(s) => {
            if let Ok(deg) = s.trim().parse::<f64>() {
                return (Some(deg), Some(degree_to_cardinal(deg)));
            }
            let upper = s.trim().to_uppercase();
            let cardinals = [
                ("N", 0.0), ("NNE", 22.5), ("NE", 45.0), ("ENE", 67.5),
                ("E", 90.0), ("ESE", 112.5), ("SE", 135.0), ("SSE", 157.5),
                ("S", 180.0), ("SSW", 202.5), ("SW", 225.0), ("WSW", 247.5),
                ("W", 270.0), ("WNW", 292.5), ("NW", 315.0), ("NNW", 337.5),
            ];
            for (card, deg) in &cardinals {
                if upper == *card {
                    return (Some(*deg), Some(card.to_string()));
                }
            }
            (None, Some(s.clone()))
        }
        None => (None, None),
    }
}

fn parse_element(elem: OverpassElement) -> AlprCamera {
    let tags = elem.tags.unwrap_or_default();
    let (direction, direction_cardinal) = parse_direction(&tags);

    AlprCamera {
        osm_id: elem.id,
        lat: elem.lat,
        lon: elem.lon,
        operator: tags.get("operator").or_else(|| tags.get("surveillance:operator")).cloned(),
        brand: tags.get("brand").or_else(|| tags.get("surveillance:brand")).cloned(),
        direction,
        direction_cardinal,
        surveillance_zone: tags.get("surveillance:zone").cloned(),
        mount_type: tags.get("camera:mount").cloned(),
        ref_id: tags.get("ref").cloned(),
        start_date: tags.get("start_date").cloned(),
        osm_timestamp: None,
        osm_version: None,
        wikimedia_commons: tags.get("wikimedia_commons").cloned(),
    }
}

const OVERPASS_ENDPOINTS: &[&str] = &[
    "https://overpass-api.de/api/interpreter",
    "https://overpass.kumi.systems/api/interpreter",
];

pub async fn fetch_cameras_in_bbox(bbox: BoundingBox) -> Result<Vec<AlprCamera>, String> {
    let query = format!(
        r#"[out:json][timeout:25];
node["man_made"="surveillance"]["surveillance:type"="ALPR"]({south},{west},{north},{east});
out body;"#,
        south = bbox.south,
        west = bbox.west,
        north = bbox.north,
        east = bbox.east,
    );

    let client = http_client().map_err(|e| format!("HTTP client error: {e}"))?;

    let mut last_error = String::new();
    for endpoint in OVERPASS_ENDPOINTS {
        let url = format!("{}?data={}", endpoint, urlencoding::encode(&query));
        match client
            .get(&url)
            .header("User-Agent", "nostr.blue/deflock")
            .send()
            .await
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    last_error = format!("Overpass returned {}", resp.status());
                    continue;
                }
                match resp.json::<OverpassResponse>().await {
                    Ok(data) => {
                        let cameras: Vec<AlprCamera> = data.elements.into_iter().map(parse_element).collect();
                        log::info!(
                            "Deflock: fetched {} ALPR cameras from {} for bbox ({:.3},{:.3})-({:.3},{:.3})",
                            cameras.len(),
                            endpoint,
                            bbox.south, bbox.west,
                            bbox.north, bbox.east,
                        );
                        return Ok(cameras);
                    }
                    Err(e) => {
                        last_error = format!("Overpass parse error: {e}");
                        continue;
                    }
                }
            }
            Err(e) => {
                last_error = format!("Overpass fetch error: {e}");
                continue;
            }
        }
    }

    Err(last_error)
}

pub async fn fetch_camera_count() -> Result<u64, String> {
    let client = http_client().map_err(|e| format!("HTTP client error: {e}"))?;
    let resp = client
        .get("https://cdn.deflock.me/alpr-counts.json")
        .header("User-Agent", "nostr.blue/deflock")
        .send()
        .await
        .map_err(|e| format!("Count fetch error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Count API returned {}", resp.status()));
    }

    #[derive(Deserialize)]
    struct CountResponse {
        #[serde(default)]
        us: Option<u64>,
        #[serde(default)]
        worldwide: Option<u64>,
    }

    let data: CountResponse = resp
        .json()
        .await
        .map_err(|e| format!("Count parse error: {e}"))?;

    Ok(data.us.or(data.worldwide).unwrap_or(0))
}

/// Haversine distance in km — re-exported from places service for convenience.
#[allow(dead_code)]
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    crate::services::places::haversine_km(lat1, lon1, lat2, lon2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bbox_from_center_radius() {
        let bbox = BoundingBox::from_center_radius(40.0, -85.0, 50.0);
        assert!(bbox.south < 40.0);
        assert!(bbox.north > 40.0);
        assert!(bbox.west < -85.0);
        assert!(bbox.east > -85.0);
    }

    #[test]
    fn test_bbox_overlaps() {
        let a = BoundingBox { south: 30.0, west: -90.0, north: 40.0, east: -80.0 };
        let b = BoundingBox { south: 35.0, west: -85.0, north: 45.0, east: -75.0 };
        assert!(a.overlaps(&b));
        let c = BoundingBox { south: 50.0, west: -90.0, north: 60.0, east: -80.0 };
        assert!(!a.overlaps(&c));
    }

    #[test]
    fn test_degree_to_cardinal() {
        assert_eq!(degree_to_cardinal(0.0), "N");
        assert_eq!(degree_to_cardinal(90.0), "E");
        assert_eq!(degree_to_cardinal(180.0), "S");
        assert_eq!(degree_to_cardinal(270.0), "W");
        assert_eq!(degree_to_cardinal(45.0), "NE");
        assert_eq!(degree_to_cardinal(360.0), "N");
        assert_eq!(degree_to_cardinal(-90.0), "W");
    }

    #[test]
    fn test_parse_direction_numeric() {
        let mut tags = HashMap::new();
        tags.insert("direction".to_string(), "180".to_string());
        let (deg, card) = parse_direction(&tags);
        assert_eq!(deg, Some(180.0));
        assert_eq!(card.as_deref(), Some("S"));
    }

    #[test]
    fn test_parse_direction_cardinal() {
        let mut tags = HashMap::new();
        tags.insert("camera:direction".to_string(), "NE".to_string());
        let (deg, card) = parse_direction(&tags);
        assert_eq!(deg, Some(45.0));
        assert_eq!(card.as_deref(), Some("NE"));
    }

    #[test]
    fn test_parse_element_extracts_all_fields() {
        let elem = OverpassElement {
            id: 12345,
            lat: 40.4,
            lon: -85.0,
            tags: Some(HashMap::from([
                ("operator".to_string(), "Flock Safety".to_string()),
                ("surveillance:zone".to_string(), "traffic".to_string()),
                ("camera:mount".to_string(), "pole".to_string()),
                ("direction".to_string(), "90".to_string()),
            ])),
        };
        let camera = parse_element(elem);
        assert_eq!(camera.osm_id, 12345);
        assert_eq!(camera.operator.as_deref(), Some("Flock Safety"));
        assert_eq!(camera.surveillance_zone.as_deref(), Some("traffic"));
        assert_eq!(camera.direction, Some(90.0));
        assert_eq!(camera.direction_cardinal.as_deref(), Some("E"));
    }
}
