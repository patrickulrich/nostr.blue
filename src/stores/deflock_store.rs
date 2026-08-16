use crate::services::deflock::{AlprCamera, BoundingBox};
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
pub static FILTERS: GlobalSignal<CameraFilters> = Signal::global(CameraFilters::default);
pub static CAMERAS_LOADING: GlobalSignal<bool> = Signal::global(|| false);
pub static LAST_ERROR: GlobalSignal<Option<String>> = Signal::global(|| None);
/// Every bbox we've successfully fetched from Overpass. Used by `is_viewport_covered`
/// to skip refetching areas we already have. Persisted to IndexedDB on insertion.
pub static FETCHED_BBOXES: GlobalSignal<Vec<BoundingBox>> = Signal::global(Vec::new);

/// True when `inner` is fully contained by `outer`.
fn contains(outer: &BoundingBox, inner: &BoundingBox) -> bool {
    inner.south >= outer.south
        && inner.north <= outer.north
        && inner.west >= outer.west
        && inner.east <= outer.east
}

/// Returns true if `viewport` is fully contained by any single fetched bbox.
/// Cheaper than rectangle-subtraction; sufficient for the common case where
/// the user pans back to a previously-fetched area.
pub fn is_viewport_covered(viewport: &BoundingBox) -> bool {
    FETCHED_BBOXES.read().iter().any(|f| contains(f, viewport))
}

/// Record a fetched bbox with containment-merge: skip insertion when an
/// existing bbox already covers it, and drop stored bboxes fully contained
/// by the new one (their coverage adds nothing). Without this the vec grew
/// without bound during long panning sessions — every 500ms poll tick then
/// linearly scanned all boxes and re-persisted them to IndexedDB.
pub fn record_bbox(bbox: BoundingBox) {
    let mut bboxes = FETCHED_BBOXES.write();
    if bboxes.iter().any(|f| contains(f, &bbox)) {
        return;
    }
    bboxes.retain(|f| !contains(&bbox, f));
    bboxes.push(bbox);
}

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
    FETCHED_BBOXES.write().clear();
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
            directions: vec![],
            direction_cardinal: None,
            surveillance_zone: zone.map(|s| s.to_string()),
            mount_type: None,
            ref_id: None,
            start_date: None,
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

#[cfg(test)]
mod bbox_merge_tests {
    use super::*;

    fn bbox(s: f64, w: f64, n: f64, e: f64) -> BoundingBox {
        BoundingBox { south: s, west: w, north: n, east: e }
    }

    /// record_bbox must containment-merge: a contained new bbox is skipped,
    /// stored bboxes contained by the new one are dropped, and overlapping
    /// (non-contained) boxes coexist. Keeps the coverage scan + IDB rows
    /// flat during long panning sessions.
    #[test]
    fn record_bbox_containment_merges() {
        // GlobalSignal access needs a Dioxus runtime on this thread.
        let vdom = dioxus::prelude::VirtualDom::new(|| dioxus::prelude::rsx! { div {} });
        let _rt_guard = dioxus_core::RuntimeGuard::new(vdom.runtime());

        *FETCHED_BBOXES.write() = Vec::new();

        // Base coverage.
        record_bbox(bbox(30.0, -100.0, 40.0, -90.0));
        assert_eq!(FETCHED_BBOXES.read().len(), 1);

        // Smaller bbox inside it: skipped entirely.
        record_bbox(bbox(33.0, -97.0, 37.0, -93.0));
        assert_eq!(FETCHED_BBOXES.read().len(), 1);

        // Larger bbox covering it plus more: replaces (drops) the base.
        record_bbox(bbox(25.0, -105.0, 45.0, -85.0));
        let bboxes = FETCHED_BBOXES.read().clone();
        assert_eq!(bboxes.len(), 1);
        assert_eq!(bboxes[0].south, 25.0);
        assert_eq!(bboxes[0].east, -85.0);

        // Partially overlapping bbox: coexists.
        record_bbox(bbox(35.0, -95.0, 55.0, -75.0));
        assert_eq!(FETCHED_BBOXES.read().len(), 2);

        // Coverage checks still hold for the merged set.
        assert!(is_viewport_covered(&bbox(30.0, -100.0, 40.0, -90.0)));
        assert!(!is_viewport_covered(&bbox(0.0, 10.0, 5.0, 15.0)));

        *FETCHED_BBOXES.write() = Vec::new();
    }
}
