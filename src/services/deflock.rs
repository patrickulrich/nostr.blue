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
    pub directions: Vec<f64>,
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
    #[allow(dead_code)]
    pub fn overlaps(&self, other: &BoundingBox) -> bool {
        self.south <= other.north
            && self.north >= other.south
            && self.west <= other.east
            && self.east >= other.west
    }
}

/// Decompose a center/radius viewport into one or two Overpass bboxes.
///
/// Near the antimeridian the radius wraps past ±180°, which a single bbox
/// cannot express — a naively clamped query would silently miss the wrapped
/// portion (while `is_viewport_covered` still marks the area fetched). The
/// center longitude is normalized into [-180, 180] and, when the radius
/// crosses the line, a companion bbox is emitted for the wrapped portion.
pub fn bboxes_for_center_radius(lat: f64, lon: f64, radius_km: f64) -> Vec<BoundingBox> {
    // Normalize the center into [-180, 180].
    let lon = ((lon + 180.0) % 360.0 + 360.0) % 360.0 - 180.0;
    let lat_delta = radius_km / 111.0;
    let lon_delta = radius_km / (111.0 * lat.to_radians().cos().max(0.01));
    let south = (lat - lat_delta).max(-90.0);
    let north = (lat + lat_delta).min(90.0);
    let west = lon - lon_delta;
    let east = lon + lon_delta;
    let mut bboxes = vec![BoundingBox {
        south,
        north,
        west: west.max(-180.0),
        east: east.min(180.0),
    }];
    if west < -180.0 {
        bboxes.push(BoundingBox {
            south,
            north,
            west: west + 360.0,
            east: 180.0,
        });
    }
    if east > 180.0 {
        bboxes.push(BoundingBox {
            south,
            north,
            west: -180.0,
            east: east - 360.0,
        });
    }
    bboxes
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

fn cardinal_to_degrees(cardinal: &str) -> Option<f64> {
    let cardinals: &[(&str, f64)] = &[
        ("N", 0.0), ("NNE", 22.5), ("NE", 45.0), ("ENE", 67.5),
        ("E", 90.0), ("ESE", 112.5), ("SE", 135.0), ("SSE", 157.5),
        ("S", 180.0), ("SSW", 202.5), ("SW", 225.0), ("WSW", 247.5),
        ("W", 270.0), ("WNW", 292.5), ("NW", 315.0), ("NNW", 337.5),
    ];
    let upper = cardinal.trim().to_uppercase();
    cardinals.iter().find(|(c, _)| *c == upper).map(|(_, d)| *d)
}

/// Parses a single direction token (numeric degrees or cardinal) to degrees.
/// Returns None if the value can't be parsed.
fn parse_direction_single(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(deg) = trimmed.parse::<f64>() {
        return Some(deg);
    }
    cardinal_to_degrees(trimmed)
}

/// Computes the midpoint angle between two bearings, handling wrap-around.
/// Mirrors DeFlock's `calculateMidpointAngle` algorithm.
fn midpoint_angle(start: f64, end: f64) -> f64 {
    let start = ((start % 360.0) + 360.0) % 360.0;
    let end = ((end % 360.0) + 360.0) % 360.0;
    let mut diff = end - start;
    if diff < 0.0 {
        diff += 360.0;
    }
    if diff > 180.0 {
        diff -= 360.0;
    }
    let midpoint = start + diff / 2.0;
    ((midpoint % 360.0) + 360.0) % 360.0
}

/// Parses a direction value, which may be a single bearing, a cardinal (N/NE/...),
/// or a range like "180-270" (returns the midpoint).
fn parse_direction_value(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('-') && !trimmed.starts_with('-') {
        let parts: Vec<&str> = trimmed.splitn(2, '-').collect();
        if parts.len() == 2 {
            let start = parse_direction_single(parts[0])?;
            let end = parse_direction_single(parts[1])?;
            return Some(midpoint_angle(start, end));
        }
    }
    parse_direction_single(trimmed)
}

/// Parses the direction tags (direction / camera:direction / surveillance:direction).
/// Supports:
///   - single numeric ("180")
///   - single cardinal ("NE", "NNE", ...)
///   - range ("180-270" → midpoint)
///   - multi-value ("0;90;180" → 3 angles)
///
/// Returns (Vec<all parsed angles>, cardinal of the first one for display).
fn parse_directions(tags: &HashMap<String, String>) -> (Vec<f64>, Option<String>) {
    let dir_str = tags
        .get("direction")
        .or_else(|| tags.get("camera:direction"))
        .or_else(|| tags.get("surveillance:direction"));

    let Some(s) = dir_str else {
        return (Vec::new(), None);
    };

    let angles: Vec<f64> = s
        .split(';')
        .filter_map(parse_direction_value)
        .collect();

    let cardinal = angles.first().map(|d| degree_to_cardinal(*d));
    (angles, cardinal)
}

fn parse_element(elem: OverpassElement) -> AlprCamera {
    let tags = elem.tags.unwrap_or_default();
    let (directions, direction_cardinal) = parse_directions(&tags);

    AlprCamera {
        osm_id: elem.id,
        lat: elem.lat,
        lon: elem.lon,
        operator: tags.get("operator").or_else(|| tags.get("surveillance:operator")).cloned(),
        brand: tags.get("brand").or_else(|| tags.get("surveillance:brand")).cloned(),
        directions,
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
    "https://overpass.private.at/api/interpreter",
];

/// Queries a single Overpass endpoint, returning parsed cameras on success.
async fn query_endpoint<'a>(
    client: &'a reqwest::Client,
    endpoint: &'a str,
    query: &'a str,
) -> Result<(Vec<AlprCamera>, &'a str), String> {
    let url = format!("{}?data={}", endpoint, urlencoding::encode(query));
    let resp = client
        .get(&url)
        .header("User-Agent", "nostr.blue/deflock")
        .send()
        .await
        .map_err(|e| format!("fetch error: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("status {}", resp.status()));
    }
    let data = resp
        .json::<OverpassResponse>()
        .await
        .map_err(|e| format!("parse error: {e}"))?;
    Ok((data.elements.into_iter().map(parse_element).collect(), endpoint))
}

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

    // Race all endpoints in parallel; first successful response wins.
    // `select_all` resolves as soon as the FIRST future completes (Ok or Err),
    // then we drain the rest looking for an Ok.
    let futures: Vec<_> = OVERPASS_ENDPOINTS
        .iter()
        .map(|endpoint| Box::pin(query_endpoint(client, endpoint, &query)))
        .collect();
    let mut remaining = futures;
    let mut last_error = String::new();
    while !remaining.is_empty() {
        let (result, _idx, rest) = futures::future::select_all(remaining).await;
        remaining = rest;
        match result {
            Ok((cameras, endpoint)) => {
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
                last_error = e;
            }
        }
    }
    Err(last_error)
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
    fn test_bboxes_normal_viewport() {
        let bboxes = bboxes_for_center_radius(40.0, -85.0, 50.0);
        assert_eq!(bboxes.len(), 1);
        let bbox = &bboxes[0];
        assert!(bbox.south < 40.0);
        assert!(bbox.north > 40.0);
        assert!(bbox.west < -85.0);
        assert!(bbox.east > -85.0);
        assert!(bbox.west < bbox.east);
    }

    #[test]
    fn test_bboxes_normalize_out_of_range_lon() {
        // lon 185° is the same viewport as -175°: single bbox straddling -175.
        let bboxes = bboxes_for_center_radius(0.0, 185.0, 50.0);
        assert_eq!(bboxes.len(), 1);
        assert!(bboxes[0].west < -175.0 && bboxes[0].west >= -180.0);
        assert!(bboxes[0].east > -175.0 && bboxes[0].east <= 180.0);
        assert!(bboxes[0].west < bboxes[0].east);
    }

    #[test]
    fn test_bboxes_antimeridian_east_wrap() {
        // Center near +180: the eastern half wraps past the line.
        let bboxes = bboxes_for_center_radius(-40.0, 179.5, 100.0);
        assert_eq!(bboxes.len(), 2);
        let primary = &bboxes[0];
        assert!((primary.east - 180.0).abs() < 1e-9);
        assert!(primary.west > 178.0 && primary.west < 179.5);
        let wrapped = &bboxes[1];
        assert!((wrapped.west + 180.0).abs() < 1e-9);
        assert!(wrapped.east < -179.0 && wrapped.east > -180.0);
        assert!(wrapped.west < wrapped.east);
    }

    #[test]
    fn test_bboxes_antimeridian_west_wrap() {
        // Center near -180: the western half wraps past the line.
        let bboxes = bboxes_for_center_radius(-40.0, -179.5, 100.0);
        assert_eq!(bboxes.len(), 2);
        let primary = &bboxes[0];
        assert!((primary.west + 180.0).abs() < 1e-9);
        assert!(primary.east > -179.0);
        let wrapped = &bboxes[1];
        assert!((wrapped.east - 180.0).abs() < 1e-9);
        assert!(wrapped.west > 179.0 && wrapped.west < 180.0);
        assert!(wrapped.west < wrapped.east);
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
        let (angles, card) = parse_directions(&tags);
        assert_eq!(angles, vec![180.0]);
        assert_eq!(card.as_deref(), Some("S"));
    }

    #[test]
    fn test_parse_direction_cardinal() {
        let mut tags = HashMap::new();
        tags.insert("camera:direction".to_string(), "NE".to_string());
        let (angles, card) = parse_directions(&tags);
        assert_eq!(angles, vec![45.0]);
        assert_eq!(card.as_deref(), Some("NE"));
    }

    #[test]
    fn test_parse_direction_range() {
        let mut tags = HashMap::new();
        tags.insert("direction".to_string(), "180-270".to_string());
        let (angles, _card) = parse_directions(&tags);
        assert_eq!(angles, vec![225.0]);
    }

    #[test]
    fn test_parse_direction_multi_value() {
        let mut tags = HashMap::new();
        tags.insert("direction".to_string(), "0;90;180".to_string());
        let (angles, card) = parse_directions(&tags);
        assert_eq!(angles, vec![0.0, 90.0, 180.0]);
        assert_eq!(card.as_deref(), Some("N"));
    }

    #[test]
    fn test_parse_direction_empty() {
        let tags = HashMap::new();
        let (angles, card) = parse_directions(&tags);
        assert!(angles.is_empty());
        assert!(card.is_none());
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
        assert_eq!(camera.directions, vec![90.0]);
        assert_eq!(camera.direction_cardinal.as_deref(), Some("E"));
    }
}
