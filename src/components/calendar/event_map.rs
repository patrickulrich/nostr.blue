//! Event Map Component
//!
//! Displays calendar events on an interactive map using Leaflet.
use dioxus::prelude::*;
use dioxus_core::use_drop;
#[cfg(feature = "web")]
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;
use crate::services::geocoding::GeoLocation;
#[cfg(feature = "web")]
use crate::services::geocoding::{geocode, geohash_to_coords};
use crate::stores::calendar_store::UnifiedEvent;
use crate::utils::validation::validate_css_dimension;
/// Global counter for unique EventMap container IDs
static EVENT_MAP_COUNTER: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "web")]
#[wasm_bindgen(
    inline_js = r#"
// Store for map instances
window.leafletMaps = window.leafletMaps || new Map();

// Load Leaflet from CDN if not already loaded
// Uses shared Promise pattern to prevent concurrent loads
export async function loadLeaflet() {
    if (window.L) {
        return;
    }

    // Return existing loading promise if one is in progress
    if (window.leafletLoadingPromise) {
        return window.leafletLoadingPromise;
    }

    window.leafletLoadingPromise = new Promise((resolve, reject) => {
        // Load CSS
        const link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.css';
        link.integrity = 'sha256-p4NxAoJBhIIN+hmNHrzRCf9tD/miZyoHS5obTRR9BMY=';
        link.crossOrigin = '';
        link.onerror = () => {
            window.leafletLoadingPromise = null;
            reject(new Error('Failed to load Leaflet CSS'));
        };
        document.head.appendChild(link);

        // Load JS
        const script = document.createElement('script');
        script.src = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.js';
        script.integrity = 'sha256-20nQCchB9co0qIjJZRGuk2/Z9VM+kNiyxNV1lvTlZBo=';
        script.crossOrigin = '';
        script.onload = () => {
            console.log('Leaflet loaded successfully');
            resolve();
        };
        script.onerror = () => {
            window.leafletLoadingPromise = null;
            reject(new Error('Failed to load Leaflet JS'));
        };
        document.head.appendChild(script);
    });

    return window.leafletLoadingPromise;
}

// Initialize map
export function initMap(containerId, lat, lng, zoom) {
    if (!window.L) {
        console.error('Leaflet not loaded');
        return false;
    }

    // Destroy existing map if any
    if (window.leafletMaps.has(containerId)) {
        window.leafletMaps.get(containerId).remove();
    }

    const container = document.getElementById(containerId);
    if (!container) {
        console.error('Map container not found:', containerId);
        return false;
    }

    const map = L.map(containerId).setView([lat, lng], zoom);

    // Add OpenStreetMap tiles
    L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
        attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors',
        maxZoom: 19,
    }).addTo(map);

    window.leafletMaps.set(containerId, map);
    return true;
}

// Add a marker to the map
export function addMarker(containerId, lat, lng, title, popupHtml, eventId) {
    const map = window.leafletMaps.get(containerId);
    if (!map) return null;

    const marker = L.marker([lat, lng])
        .addTo(map)
        .bindPopup(popupHtml);

    marker.eventId = eventId;
    return marker._leaflet_id;
}

// Add multiple markers and fit bounds
export function addMarkersAndFit(containerId, markersJson) {
    const map = window.leafletMaps.get(containerId);
    if (!map) return;

    const markers = JSON.parse(markersJson);
    if (markers.length === 0) return;

    const bounds = L.latLngBounds();

    markers.forEach(m => {
        const marker = L.marker([m.lat, m.lng])
            .addTo(map)
            .bindPopup(m.popup);
        bounds.extend([m.lat, m.lng]);
    });

    // Fit bounds with padding
    map.fitBounds(bounds, { padding: [50, 50] });
}

// Clear all markers
export function clearMarkers(containerId) {
    const map = window.leafletMaps.get(containerId);
    if (!map) return;

    map.eachLayer(layer => {
        if (layer instanceof L.Marker) {
            map.removeLayer(layer);
        }
    });
}

// Destroy map
export function destroyMap(containerId) {
    if (window.leafletMaps.has(containerId)) {
        window.leafletMaps.get(containerId).remove();
        window.leafletMaps.delete(containerId);
    }
}

// Set map view
export function setMapView(containerId, lat, lng, zoom) {
    const map = window.leafletMaps.get(containerId);
    if (map) {
        map.setView([lat, lng], zoom);
    }
}

// Invalidate map size (for when container becomes visible)
export function invalidateSize(containerId) {
    const map = window.leafletMaps.get(containerId);
    if (map) {
        map.invalidateSize();
    }
}
"#
)]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn loadLeaflet() -> Result<(), JsValue>;
    fn initMap(container_id: &str, lat: f64, lng: f64, zoom: u32) -> bool;
    #[allow(dead_code)]
    fn addMarker(
        container_id: &str,
        lat: f64,
        lng: f64,
        title: &str,
        popup_html: &str,
        event_id: &str,
    ) -> Option<u32>;
    fn addMarkersAndFit(container_id: &str, markers_json: &str);
    fn clearMarkers(container_id: &str);
    fn destroyMap(container_id: &str);
    #[allow(dead_code)]
    fn setMapView(container_id: &str, lat: f64, lng: f64, zoom: u32);
    #[allow(dead_code)]
    fn invalidateSize(container_id: &str);
}
/// Marker data for JS
#[cfg(feature = "web")]
#[derive(Clone, Debug, Serialize, Deserialize)]
struct MarkerData {
    lat: f64,
    lng: f64,
    popup: String,
    event_id: String,
}
/// Event with resolved location
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct GeocodedEvent {
    pub event: UnifiedEvent,
    pub location: GeoLocation,
}
/// Props for EventMap
#[derive(Props, Clone, PartialEq)]
pub struct EventMapProps {
    /// Events to display on map
    pub events: Vec<UnifiedEvent>,
    /// CSS height for the map container
    #[props(default = "400px".to_string())]
    pub height: String,
}
/// Event Map component that displays events on a Leaflet map
#[component]
pub fn EventMap(props: EventMapProps) -> Element {
    let container_id = use_signal(|| {
        format!(
            "event-map-{}-{}",
            crate::platform::timestamp::now_millis(),
            EVENT_MAP_COUNTER.fetch_add(1, Ordering::Relaxed),
        )
    });
    #[allow(unused_mut)]
    let mut leaflet_loaded = use_signal(|| false);
    #[allow(unused_variables, unused_mut)]
    let mut leaflet_loading = use_signal(|| false);
    #[allow(unused_mut)]
    let mut leaflet_error = use_signal(|| None::<String>);
    #[allow(unused_mut)]
    let mut map_initialized = use_signal(|| false);
    #[allow(unused_mut)]
    let mut geocoded_events = use_signal(Vec::<GeocodedEvent>::new);
    #[allow(unused_mut)]
    let mut loading_geo = use_signal(|| false);
    let mut processed_event_ids = use_signal(String::new);
    let mut geocode_cancelled = use_signal(|| false);
    let mut unmounted = use_signal(|| false);
    #[cfg(feature = "web")]
    let mut geocode_gen = use_signal(|| 0u32);
    #[cfg(not(feature = "web"))]
    let _geocode_gen = use_signal(|| 0u32);
    use_drop(move || {
        geocode_cancelled.set(true);
        unmounted.set(true);
    });
    let events_key = use_memo(
        use_reactive(
            (&props.events,),
            |(events,)| {
                let mut hasher = DefaultHasher::new();
                for e in events.iter() {
                    e.coordinate().hash(&mut hasher);
                    e.title().hash(&mut hasher);
                    e.start_timestamp().hash(&mut hasher);
                    e.location().hash(&mut hasher);
                    for loc in e.locations() {
                        loc.hash(&mut hasher);
                    }
                }
                hasher.finish().to_string()
            },
        ),
    );
    let events_for_geocode = props.events.clone();
    let events_count = props.events.len();
    use_effect(move || {
        #[cfg(feature = "web")]
        {
            if *leaflet_loaded.read() || *leaflet_loading.read()
                || leaflet_error.read().is_some()
            {
                return;
            }
            leaflet_loading.set(true);
            spawn(async move {
                if let Err(e) = loadLeaflet().await {
                    if *unmounted.read() {
                        return;
                    }
                    log::error!("Failed to load Leaflet: {:?}", e);
                    leaflet_error
                        .set(
                            Some("Failed to load map. Please refresh the page.".to_string()),
                        );
                    leaflet_loading.set(false);
                    return;
                }
                if *unmounted.read() {
                    return;
                }
                leaflet_loaded.set(true);
                leaflet_loading.set(false);
            });
        }
        #[cfg(not(feature = "web"))]
        {
            leaflet_error.set(Some("Map view is only available in the web version.".to_string()));
        }
    });
    use_effect(move || {
        #[cfg(feature = "web")]
        {
            if !*leaflet_loaded.read() || *map_initialized.read() {
                return;
            }
            let id = container_id.read().clone();
            spawn(async move {
                crate::platform::timer::sleep(std::time::Duration::from_millis(100)).await;
                if *unmounted.read() {
                    return;
                }
                if initMap(&id, 20.0, 0.0, 2) {
                    map_initialized.set(true);
                    log::info!("Map initialized: {}", id);
                } else {
                    log::error!("Failed to initialize map container: {}", id);
                    leaflet_error
                        .set(
                            Some(
                                "Failed to initialize map. Please refresh the page."
                                    .to_string(),
                            ),
                        );
                }
            });
        }
    });
    use_effect({
        let _events_for_geocode = events_for_geocode.clone();
        move || {
            #[cfg(not(feature = "web"))]
            {
                let key = events_key.read().clone();
                processed_event_ids.set(key);
                return;
            }
            #[cfg(feature = "web")]
            {
                let key = events_key.read().clone();
                if key == *processed_event_ids.read() {
                    return;
                }
                if events_for_geocode.is_empty() {
                    processed_event_ids.set(key);
                    geocoded_events.set(Vec::new());
                    return;
                }
                if *loading_geo.read() {
                    return;
                }
                loading_geo.set(true);
                // Increment generation counter to invalidate any pending tasks
                geocode_gen.with_mut(|g| *g = g.wrapping_add(1));
                let this_gen = *geocode_gen.read();
                let key_to_store = key.clone();
                let events_to_process = events_for_geocode.clone();
                spawn(async move {
                    let mut results = Vec::new();
                    const BATCH_SIZE: usize = 5;
                    const BATCH_DELAY_MS: u32 = 200;
                    for (idx, event) in events_to_process.iter().enumerate() {
                        if *geocode_cancelled.read() {
                            log::debug!("Geocoding cancelled, stopping processing");
                            loading_geo.set(false);
                            return;
                        }
                        if idx > 0 && idx % BATCH_SIZE == 0 {
                            crate::platform::timer::sleep_ms(BATCH_DELAY_MS).await;
                        }
                        if let Some(geohash) = event.geohash() {
                            if let Some((lat, lon)) = geohash_to_coords(geohash) {
                                results
                                    .push(GeocodedEvent {
                                        event: event.clone(),
                                        location: GeoLocation {
                                            lat,
                                            lon,
                                            display_name: event.location().unwrap_or("").to_string(),
                                            city: None,
                                            state: None,
                                            country: None,
                                            country_code: None,
                                            place_type: None,
                                        },
                                    });
                                continue;
                            }
                        }
                        if let Some(location_str) = event.location() {
                            if crate::utils::nip52::is_online_location(location_str) {
                                continue;
                            }
                            match geocode(location_str).await {
                                Ok(Some(loc)) => {
                                    results
                                        .push(GeocodedEvent {
                                            event: event.clone(),
                                            location: loc,
                                        });
                                }
                                Ok(None) => {
                                    log::debug!(
                                        "Geocoding returned no results for: {}", location_str
                                    );
                                }
                                Err(e) => {
                                    log::warn!(
                                        "Geocoding failed for '{}': {}", location_str, e
                                    );
                                }
                            }
                        }
                    }
                    if *geocode_cancelled.read() {
                        log::debug!("Geocoding cancelled, not updating signals");
                        loading_geo.set(false);
                        return;
                    }
                    // Verify generation before updating state
                    if *geocode_gen.read() != this_gen {
                        log::debug!("Geocoding generation stale, discarding results");
                        loading_geo.set(false);
                        return;
                    }
                    geocoded_events.set(results);
                    processed_event_ids.set(key_to_store);
                    loading_geo.set(false);
                });
            }
        }
    });
    use_effect(move || {
        #[cfg(feature = "web")]
        {
            if !*map_initialized.read() {
                return;
            }
            let events = geocoded_events.read();
            let id = container_id.read().clone();
            clearMarkers(&id);
            if events.is_empty() {
                return;
            }
            let markers: Vec<MarkerData> = events
                .iter()
                .map(|ge| MarkerData {
                    lat: ge.location.lat,
                    lng: ge.location.lon,
                    popup: format_popup(&ge.event, &ge.location),
                    event_id: ge.event.naddr().to_string(),
                })
                .collect();
            match serde_json::to_string(&markers) {
                Ok(json) => {
                    addMarkersAndFit(&id, &json);
                }
                Err(e) => {
                    log::error!(
                        "Failed to serialize {} map markers for container {}: {}", markers
                        .len(), id, e
                    );
                }
            }
        }
    });
    use_drop(move || {
        #[cfg(feature = "web")]
        {
            let id = container_id.read().clone();
            destroyMap(&id);
        }
    });
    let safe_height = validate_css_dimension(&props.height).unwrap_or("400px");
    let container_style = format!("height: {}; width: 100%;", safe_height);
    rsx! {
        div { class: "event-map-container relative rounded-lg overflow-hidden border border-border",
            div {
                id: "{container_id}",
                style: "{container_style}",
                class: "bg-muted",
            }
            if let Some(ref err) = *leaflet_error.read() {
                div { class: "absolute inset-0 flex items-center justify-center bg-background/80",
                    div { class: "text-center p-4",
                        svg {
                            class: "w-12 h-12 mx-auto text-red-500 mb-2",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "1.5",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z",
                            }
                        }
                        p { class: "text-red-600 dark:text-red-400", "{err}" }
                    }
                }
            } else if !*leaflet_loaded.read() || *loading_geo.read() {
                div { class: "absolute inset-0 flex items-center justify-center bg-background/80",
                    div { class: "flex flex-col items-center gap-2",
                        div { class: "w-8 h-8 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                        span { class: "text-sm text-muted-foreground",
                            if !*leaflet_loaded.read() {
                                "Loading map..."
                            } else {
                                "Geocoding events..."
                            }
                        }
                    }
                }
            }
            if *map_initialized.read() && !*loading_geo.read() && geocoded_events.read().is_empty()
                && events_count > 0
            {
                div { class: "absolute inset-0 flex items-center justify-center bg-background/80",
                    div { class: "text-center p-4",
                        svg {
                            class: "w-12 h-12 mx-auto text-muted-foreground mb-2",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "1.5",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M15 10.5a3 3 0 11-6 0 3 3 0 016 0z",
                            }
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M19.5 10.5c0 7.142-7.5 11.25-7.5 11.25S4.5 17.642 4.5 10.5a7.5 7.5 0 1115 0z",
                            }
                        }
                        p { class: "text-muted-foreground", "No events with physical locations found" }
                    }
                }
            }
            div { class: "absolute bottom-2 left-2 bg-background/90 rounded px-2 py-1 text-xs text-muted-foreground",
                "{geocoded_events.read().len()} events on map"
            }
        }
    }
}
/// Format popup HTML for a marker
#[cfg(feature = "web")]
fn format_popup(event: &UnifiedEvent, location: &GeoLocation) -> String {
    let title = event.title();
    let time = format_popup_time(event);
    let loc = &location.display_name;
    let naddr = event.naddr().to_string();
    let href = if event.is_livestream() {
        format!("/videos/live/{}", naddr)
    } else {
        format!("/calendar/{}", naddr)
    };
    format!(
        r#"<div style="min-width: 200px;">
            <a href="{}" style="font-size: 14px; font-weight: bold; color: #3b82f6; text-decoration: none;">{}</a>
            <div style="color: #666; margin-top: 4px;">{}</div>
            <div style="color: #888; margin-top: 2px; font-size: 12px;">{}</div>
        </div>"#,
        html_escape(&href),
        html_escape(title),
        html_escape(&time),
        html_escape(loc),
    )
}
/// Format time for popup
#[cfg(feature = "web")]
fn format_popup_time(event: &UnifiedEvent) -> String {
    let ts = event.start_timestamp();
    if ts == 0 {
        return "Date TBD".to_string();
    }
    let date = js_sys::Date::new(&(ts as f64 * 1000.0).into());
    let month_names = [
        "Jan",
        "Feb",
        "Mar",
        "Apr",
        "May",
        "Jun",
        "Jul",
        "Aug",
        "Sep",
        "Oct",
        "Nov",
        "Dec",
    ];
    let month = date.get_month() as usize;
    let day = date.get_date();
    let month_str = month_names.get(month).unwrap_or(&"");
    if event.is_all_day() {
        format!("{} {}", month_str, day)
    } else {
        let hours = date.get_hours();
        let minutes = date.get_minutes();
        let am_pm = if hours >= 12 { "PM" } else { "AM" };
        let hour_12 = if hours == 0 {
            12
        } else if hours > 12 {
            hours - 12
        } else {
            hours
        };
        format!("{} {} at {}:{:02} {}", month_str, day, hour_12, minutes, am_pm)
    }
}

/// Escape HTML entities
#[cfg(feature = "web")]
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
/// Skeleton loader for map
#[component]
pub fn EventMapSkeleton() -> Element {
    rsx! {
        div {
            class: "event-map-skeleton rounded-lg overflow-hidden border border-border bg-muted animate-pulse",
            style: "height: 400px;",
            div { class: "h-full flex items-center justify-center",
                span { class: "text-muted-foreground", "Loading map..." }
            }
        }
    }
}
