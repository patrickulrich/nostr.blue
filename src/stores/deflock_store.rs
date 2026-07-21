use crate::services::deflock::AlprCamera;
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct CameraFilters {
    pub operators: HashSet<String>,
    pub brands: HashSet<String>,
    pub zones: HashSet<String>,
    pub search_query: String,
}

impl CameraFilters {
    pub fn matches(&self, cam: &AlprCamera) -> bool {
        if !self.operators.is_empty() {
            match &cam.operator {
                Some(op) if self.operators.contains(op) => {}
                _ => return false,
            }
        }
        if !self.brands.is_empty() {
            match &cam.brand {
                Some(b) if self.brands.contains(b) => {}
                _ => return false,
            }
        }
        if !self.zones.is_empty() {
            match &cam.surveillance_zone {
                Some(z) if self.zones.contains(z) => {}
                _ => return false,
            }
        }
        if !self.search_query.is_empty() {
            let q = self.search_query.to_lowercase();
            let matches = cam.operator.as_deref().unwrap_or("").to_lowercase().contains(&q)
                || cam.brand.as_deref().unwrap_or("").to_lowercase().contains(&q)
                || cam.surveillance_zone.as_deref().unwrap_or("").to_lowercase().contains(&q)
                || cam.ref_id.as_deref().unwrap_or("").to_lowercase().contains(&q);
            if !matches {
                return false;
            }
        }
        true
    }
}

pub static CAMERAS: GlobalSignal<HashMap<u64, AlprCamera>> = Signal::global(HashMap::new);
pub static VIEWPORT: GlobalSignal<Option<(f64, f64, f64, f64)>> = Signal::global(|| None);
pub static FETCHED_GEOHASHES: GlobalSignal<HashSet<String>> = Signal::global(HashSet::new);
pub static FILTERS: GlobalSignal<CameraFilters> = Signal::global(CameraFilters::default);
pub static CAMERAS_LOADING: GlobalSignal<bool> = Signal::global(|| false);
pub static LAST_ERROR: GlobalSignal<Option<String>> = Signal::global(|| None);
pub static WORLDWIDE_COUNT: GlobalSignal<Option<u64>> = Signal::global(|| None);

#[allow(dead_code)]
pub fn merge_camera(camera: AlprCamera) {
    CAMERAS.write().insert(camera.osm_id, camera);
}

pub fn merge_cameras(new_cameras: Vec<AlprCamera>) {
    let mut cameras = CAMERAS.write();
    for cam in new_cameras {
        cameras.insert(cam.osm_id, cam);
    }
}

pub fn mark_geohash_fetched(prefix: &str) {
    FETCHED_GEOHASHES.write().insert(prefix.to_string());
}

pub fn is_geohash_fetched(prefix: &str) -> bool {
    FETCHED_GEOHASHES.read().contains(prefix)
}

pub fn get_unique_operators() -> Vec<String> {
    let cameras = CAMERAS.read();
    let mut ops: HashSet<String> = HashSet::new();
    for cam in cameras.values() {
        if let Some(op) = &cam.operator {
            ops.insert(op.clone());
        }
    }
    let mut result: Vec<String> = ops.into_iter().collect();
    result.sort();
    result
}

#[allow(dead_code)]
pub fn get_unique_brands() -> Vec<String> {
    let cameras = CAMERAS.read();
    let mut brands: HashSet<String> = HashSet::new();
    for cam in cameras.values() {
        if let Some(brand) = &cam.brand {
            brands.insert(brand.clone());
        }
    }
    let mut result: Vec<String> = brands.into_iter().collect();
    result.sort();
    result
}

pub fn get_filtered_cameras() -> Vec<AlprCamera> {
    let cameras = CAMERAS.read();
    let filters = FILTERS.read();
    cameras.values().filter(|c| filters.matches(c)).cloned().collect()
}

#[allow(dead_code)]
pub fn clear_cameras() {
    CAMERAS.write().clear();
    FETCHED_GEOHASHES.write().clear();
    LAST_ERROR.write().take();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_camera(id: u64, operator: Option<&str>, brand: Option<&str>, zone: Option<&str>) -> AlprCamera {
        AlprCamera {
            osm_id: id,
            lat: 40.0,
            lon: -85.0,
            operator: operator.map(|s| s.to_string()),
            brand: brand.map(|s| s.to_string()),
            direction: None,
            direction_cardinal: None,
            surveillance_zone: zone.map(|s| s.to_string()),
            mount_type: None,
            ref_id: None,
            start_date: None,
            osm_timestamp: None,
            osm_version: None,
            wikimedia_commons: None,
        }
    }

    #[test]
    fn test_filter_matches_all_when_empty() {
        let filters = CameraFilters::default();
        let cam = make_camera(1, Some("Flock"), None, None);
        assert!(filters.matches(&cam));
    }

    #[test]
    fn test_filter_by_operator() {
        let mut filters = CameraFilters::default();
        filters.operators.insert("Flock Safety".to_string());
        let matching = make_camera(1, Some("Flock Safety"), None, None);
        let non_matching = make_camera(2, Some("Other Corp"), None, None);
        assert!(filters.matches(&matching));
        assert!(!filters.matches(&non_matching));
    }

    #[test]
    fn test_filter_by_search_query() {
        let mut filters = CameraFilters::default();
        filters.search_query = "flock".to_string();
        let matching = make_camera(1, Some("Flock Safety"), None, None);
        let non_matching = make_camera(2, Some("Other"), None, None);
        assert!(filters.matches(&matching));
        assert!(!filters.matches(&non_matching));
    }
}
