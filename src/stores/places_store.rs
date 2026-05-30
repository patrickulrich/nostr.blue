use crate::services::places::{BtcMapPlace, Place};
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq)]
pub enum MapMode {
    View,
    #[allow(dead_code)]
    Add,
    #[allow(dead_code)]
    Chat,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub enum FeedType {
    Global,
    Following,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct DirectionsInfo {
    pub distance_km: f64,
    pub duration_min: f64,
    pub dest_name: String,
    pub dest_lat: f64,
    pub dest_lng: f64,
}

pub static PLACES: GlobalSignal<Vec<Place>> = Signal::global(Vec::new);
pub static BTCMAP_PLACES: GlobalSignal<Vec<BtcMapPlace>> = Signal::global(Vec::new);
pub static MAP_MODE: GlobalSignal<MapMode> = Signal::global(|| MapMode::View);
#[allow(dead_code)]
pub static PLACES_FEED_TYPE: GlobalSignal<FeedType> = Signal::global(|| FeedType::Global);
#[allow(dead_code)]
pub static SELECTED_PLACE: GlobalSignal<Option<usize>> = Signal::global(|| None);
#[allow(dead_code)]
pub static PLACES_LOADING: GlobalSignal<bool> = Signal::global(|| false);
pub static BTCMAP_LOADING: GlobalSignal<bool> = Signal::global(|| false);
pub static LOC_LOADING: GlobalSignal<bool> = Signal::global(|| true);
pub static SHOW_BTCMAP: GlobalSignal<bool> = Signal::global(|| true);
pub static USER_LOCATION: GlobalSignal<Option<(f64, f64)>> = Signal::global(|| None);
pub static DIRECTIONS: GlobalSignal<Option<DirectionsInfo>> = Signal::global(|| None);
#[allow(dead_code)]
pub static GEOCHAT_HASH: GlobalSignal<Option<String>> = Signal::global(|| None);

pub static VIEWPORT: GlobalSignal<Option<(f64, f64, f64)>> = Signal::global(|| None);
pub static VIEWPORT_ZOOM: GlobalSignal<Option<f64>> = Signal::global(|| None);
pub static LAST_BTCMAP_FETCH: GlobalSignal<Option<(f64, f64, f64)>> = Signal::global(|| None);
pub static FETCHED_GEOHASHES: GlobalSignal<HashSet<String>> = Signal::global(HashSet::new);
pub static PENDING_PLACE_COORDS: GlobalSignal<Option<(f64, f64)>> = Signal::global(|| None);

#[allow(dead_code)]
pub fn get_selected_place() -> Option<Place> {
    let idx = SELECTED_PLACE.read();
    let places = PLACES.read();
    idx.and_then(|i| places.get(i).cloned())
}

pub fn merge_place(place: Place) {
    let mut places = PLACES.write();
    let key = format!("{}:{}", place.pubkey, place.d_tag);
    match places
        .iter_mut()
        .find(|p| format!("{}:{}", p.pubkey, p.d_tag) == key)
    {
        Some(existing) if existing.created_at >= place.created_at => {}
        Some(existing) => {
            *existing = place;
        }
        None => {
            places.push(place);
        }
    }
}

pub fn cross_ref_places_with_btcmap() {
    let osm_map: HashMap<String, BtcMapPlace> = BTCMAP_PLACES
        .read()
        .iter()
        .filter_map(|bp| bp.osm_id.as_ref().map(|osm| (osm.clone(), bp.clone())))
        .collect();

    let mut places = PLACES.write();
    for place in places.iter_mut() {
        if let Some(ref osm_ref) = place.osm_ref {
            if let Some(btcmap_place) = osm_map.get(osm_ref) {
                place.btcmap_match = Some(btcmap_place.clone());
            }
        }
    }
}

pub fn viewport_needs_refetch(
    current: (f64, f64, f64),
    last: (f64, f64, f64),
) -> bool {
    let (clat, clng, crad) = current;
    let (llat, llng, lrad) = last;
    let dist = crate::services::places::haversine_km(clat, clng, llat, llng);
    if dist > lrad * 0.4 {
        return true;
    }
    if crad > lrad * 1.3 {
        return true;
    }
    false
}

pub fn mark_geohash_fetched(prefix: &str) {
    FETCHED_GEOHASHES.write().insert(prefix.to_string());
}

pub fn is_geohash_fetched(prefix: &str) -> bool {
    FETCHED_GEOHASHES.read().contains(prefix)
}
