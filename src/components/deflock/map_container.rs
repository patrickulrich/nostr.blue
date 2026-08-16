use crate::services::deflock;
use crate::stores::{deflock_store, places_store};
use crate::utils::leaflet_shared::{
    DIRECTIONS_HELPERS_JS, LEAFLET_LOAD_JS, MARKERCLUSTER_LOAD_JS, POPUP_STYLE_JS,
};
use crate::components::deflock::filter_bar::DeflockFilterBar;
use dioxus::prelude::*;
use dioxus_core::use_drop;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

static DEFLOCK_MAP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Below this zoom, render simple circle markers (cheap, no overlap).
/// At or above, render 60×60 divIcon with DeFlock-style direction cones.
const CAMERA_CONE_ZOOM_THRESHOLD: f64 = 14.0;

/// Caps the Overpass query radius so huge viewports (zoomed out) don't time out.
/// 250km balances coverage with Overpass rate limits.
const MAX_FETCH_RADIUS_KM: f64 = 250.0;

/// DeFlock's exact FOV cone path (from webapp/src/components/LeafletMap.vue).
/// Two paths: an outer translucent fill + an inner ring outline. Drawn pointing "up" (north);
/// rotated per-direction via transform=rotate(${deg}deg) on a wrapping <g>.
const DEFLICK_FOV_CONE_SVG: &str = r#"<path d="M215.248,221.461L99.696,43.732C144.935,16.031 198.536,0 256,0C313.464,0 367.065,16.031 412.304,43.732L296.752,221.461C287.138,209.593 272.448,202.001 256,202.001C239.552,202.001 224.862,209.593 215.248,221.461Z" style="fill:rgb(239,68,68);fill-opacity:0.35;"/>
                    <path d="M215.248,221.461L99.696,43.732C144.935,16.031 198.536,0 256,0C313.464,0 367.065,16.031 412.304,43.732L296.752,221.461C287.138,209.593 272.448,202.001 256,202.001C239.552,202.001 224.862,209.593 215.248,221.461ZM217.92,200.242C228.694,192.652 241.831,188.195 256,188.195C270.169,188.195 283.306,192.652 294.08,200.242C294.08,200.242 392.803,48.4 392.803,48.4C352.363,26.364 305.694,13.806 256,13.806C206.306,13.806 159.637,26.364 119.197,48.4L217.92,200.242Z" style="fill:rgb(239,68,68);fill-opacity:0.55;"/>"#;

fn build_camera_markers_js(id_json: &str, cameras_json: &str, zoom: f64) -> String {
    let use_cones = zoom >= CAMERA_CONE_ZOOM_THRESHOLD;
    let cone_mode_lit = if use_cones { "true" } else { "false" };
    format!(
        r##"(() => {{
            const maps = window.leafletMaps || new Map();
            const map = maps.get({id_json});
            const mapId = {id_json};
            if (!map) return;

            if (window.__deflockCameraLayer) {{
                map.removeLayer(window.__deflockCameraLayer);
            }}
            window.__deflockCameraLayer = L.markerClusterGroup({{
                chunkedLoading: true,
                disableClusteringAtZoom: 16,
                removeOutsideVisibleBounds: true,
                maxClusterRadius: 60,
                spiderfyOnEveryZoom: false,
                spiderfyOnMaxZoom: false,
            }}).addTo(map);

            const cameras = {cameras_json};
            const useCones = {cone_mode_lit};
            const esc = window.__placesEscapeHtml || (function(s) {{
                if (!s) return '';
                return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;').replace(/'/g,'&#39;');
            }});

            const FOV_CONE_SVG = {fov_cone_lit};

            cameras.forEach(c => {{
                const directions = Array.isArray(c.directions) ? c.directions : [];
                const hasDir = directions.length > 0;
                let marker;

                if (useCones && hasDir) {{
                    // Build one rotated <g> per direction value
                    const conesSvg = directions.map(deg =>
                        `<g style="transform:rotate(${{deg}}deg); transform-origin: 256px 256px;">${{FOV_CONE_SVG}}</g>`
                    ).join('');
                    const iconHtml = `<svg width="60" height="60" viewBox="0 0 512 512" xmlns="http://www.w3.org/2000/svg" style="overflow:visible;">
                        ${{conesSvg}}
                        <g transform="matrix(0.906623,0,0,0.906623,23.9045,22.3271)">
                            <circle cx="256" cy="256" r="57.821" style="fill:#ef4444;fill-opacity:0.8;"/>
                            <path d="M256,174.25C301.119,174.25 337.75,210.881 337.75,256C337.75,301.119 301.119,337.75 256,337.75C210.881,337.75 174.25,301.119 174.25,256C174.25,210.881 210.881,174.25 256,174.25ZM256,198.179C224.088,198.179 198.179,224.088 198.179,256C198.179,287.912 224.088,313.821 256,313.821C287.912,313.821 313.821,287.912 313.821,256C313.821,224.088 287.912,198.179 256,198.179Z" style="fill:#ef4444;"/>
                            <circle cx="256" cy="256" r="22" style="fill:#1a1a2e;"/>
                        </g>
                    </svg>`;
                    const icon = L.divIcon({{
                        html: iconHtml,
                        className: 'deflock-marker',
                        iconSize: [60, 60],
                        iconAnchor: [30, 30],
                        popupAnchor: [0, -8]
                    }});
                    marker = L.marker([c.lat, c.lon], {{ icon }}).addTo(window.__deflockCameraLayer);
                }} else {{
                    // Simple circle marker — cheap, no cone
                    marker = L.circleMarker([c.lat, c.lon], {{
                        radius: 8,
                        fillColor: '#ef4444',
                        color: '#ef4444',
                        weight: 2,
                        opacity: 1,
                        fillOpacity: 0.6
                    }}).addTo(window.__deflockCameraLayer);
                }}

                const s = (v) => v ? esc(String(v)) : '';
                const operator = s(c.operator) || 'Unknown Operator';
                const brand = s(c.brand);
                const zone = s(c.surveillance_zone);
                const mount = s(c.mount_type);
                const direction = c.direction_cardinal
                    ? s(c.direction_cardinal)
                    : (directions.length > 0 ? directions.join('°, ') + '°' : '');
                const startDate = s(c.start_date);
                const refId = s(c.ref_id);
                const osmUrl = 'https://www.openstreetmap.org/node/' + c.osm_id;

                let badges = '';
                if (zone) {{
                    const zoneColors = {{traffic: '#f59e0b', town: '#3b82f6', parking: '#8b5cf6', other: '#6b7280'}};
                    const zc = zoneColors[zone] || '#6b7280';
                    badges += `<span style="display:inline-block;background:${{zc}};color:#fff;border-radius:9999px;padding:1px 8px;font-size:10px;font-weight:600;margin-right:4px;">${{zone.charAt(0).toUpperCase() + zone.slice(1)}}</span>`;
                }}
                if (direction) {{
                    badges += `<span style="display:inline-block;background:#374151;color:#d1d5db;border-radius:9999px;padding:1px 8px;font-size:10px;font-weight:600;">→ ${{direction}}</span>`;
                }}

                let details = '';
                if (brand) details += `<div style="color:#a3a3a3;font-size:12px;margin-bottom:2px;">Brand: ${{brand}}</div>`;
                if (mount) details += `<div style="color:#a3a3a3;font-size:12px;margin-bottom:2px;">Mount: ${{mount.replace(/_/g,' ')}}</div>`;
                if (startDate) details += `<div style="color:#a3a3a3;font-size:12px;margin-bottom:2px;">Installed: ${{startDate}}</div>`;
                if (refId) details += `<div style="color:#a3a3a3;font-size:12px;margin-bottom:2px;">Ref: ${{refId}}</div>`;

                const popup = `<div style="min-width:220px;max-width:280px;color:#e5e5e5;font-family:-apple-system,BlinkMacSystemFont,sans-serif;font-size:13px;line-height:1.4;">
                    <div style="font-size:15px;font-weight:600;color:#fff;margin-bottom:2px;">📷 ${{operator}}</div>
                    <div style="margin-bottom:6px;">${{badges}}</div>
                    <div style="border-top:1px solid rgba(255,255,255,0.1);margin:6px 0;"></div>
                    ${{details}}
                    <div style="display:flex;gap:6px;margin-top:8px;">
                        <a href="${{osmUrl}}" target="_blank" rel="noopener" style="padding:5px 14px;border-radius:6px;background:transparent;color:#a78bfa;border:1px solid #7c3aed;cursor:pointer;font-size:12px;font-weight:500;text-decoration:none;">OSM</a>
                        <button onclick="window.__requestDirectionsFor('${{mapId}}',${{c.lat}},${{c.lon}},'Camera ${{c.osm_id}}','#ef4444')"
                            style="padding:5px 14px;border-radius:6px;background:#ef4444;color:#fff;border:none;cursor:pointer;font-size:12px;font-weight:500;">
                            Directions
                        </button>
                    </div>
                </div>`;

                marker.bindPopup(popup, {{ maxWidth: 300, className: 'places-popup' }});
            }});
        }})()"##,
        fov_cone_lit = serde_json::to_string(DEFLICK_FOV_CONE_SVG).unwrap_or_default(),
    )
}

#[component]
pub fn DeflockMapContainer() -> Element {
    let container_id = use_signal(|| {
        format!(
            "deflock-map-{}-{}",
            crate::platform::timestamp::now_millis(),
            DEFLOCK_MAP_COUNTER.fetch_add(1, Ordering::Relaxed),
        )
    });

    let mut leaflet_loaded = use_signal(|| false);
    let mut markercluster_loaded = use_signal(|| false);
    let mut map_initialized = use_signal(|| false);
    let mut unmounted = use_signal(|| false);
    let mut loc_requested = use_signal(|| false);
    let mut viewport_poll_started = use_signal(|| false);
    let mut route_poll_started = use_signal(|| false);
    let mut cache_warmed = use_signal(|| false);
    let mut last_marker_hash: Signal<Option<u64>> = use_signal(|| None);

    use_drop(move || {
        unmounted.set(true);
        let id = container_id.read().clone();
        let id_json = serde_json::to_string(&id).unwrap_or_default();
        let _ = dioxus::document::eval(&format!(
            r#"
            (() => {{
                const maps = window.leafletMaps || new Map();
                if (maps.has({id_json})) {{
                    maps.get({id_json}).remove();
                    maps.delete({id_json});
                }}
                if (window.__deflockCameraLayer) {{
                    window.__deflockCameraLayer = null;
                }}
            }})()
            "#
        ));
    });

    use_effect(move || {
        if *leaflet_loaded.read() {
            return;
        }
        spawn(async move {
            let mut eval = dioxus::document::eval(LEAFLET_LOAD_JS);
            let result: String = eval.recv().await.unwrap_or_default();
            if *unmounted.read() {
                return;
            }
            if result.starts_with("error:") {
                log::error!("Deflock: failed to load Leaflet: {}", result);
            } else {
                leaflet_loaded.set(true);
            }
        });
    });

    use_effect(move || {
        if *markercluster_loaded.read() || !*leaflet_loaded.read() {
            return;
        }
        spawn(async move {
            let mut eval = dioxus::document::eval(MARKERCLUSTER_LOAD_JS);
            let result: String = eval.recv().await.unwrap_or_default();
            if *unmounted.read() {
                return;
            }
            if result.starts_with("error:") {
                log::error!("Deflock: failed to load markercluster: {}", result);
            } else {
                markercluster_loaded.set(true);
            }
        });
    });

    use_effect(move || {
        if !*leaflet_loaded.read() || !*markercluster_loaded.read() || *map_initialized.read() {
            return;
        }
        let id = container_id.read().clone();
        let id_json = serde_json::to_string(&id).unwrap_or_default();
        spawn(async move {
            crate::platform::timer::sleep(std::time::Duration::from_millis(100)).await;
            if *unmounted.read() {
                return;
            }

            let result: String = dioxus::document::eval(&format!(
                r#"
                if (!window.L) {{ return "false"; }}
                {popup_style}
                const maps = window.leafletMaps || new Map();
                if (maps.has({id_json})) {{ maps.get({id_json}).remove(); }}
                const container = document.getElementById({id_json});
                if (!container) {{ return "false"; }}
                const map = L.map({id_json}, {{
                    center: [39.5, -98.35],
                    zoom: 4,
                    minZoom: 3,
                    maxZoom: 19,
                    zoomControl: false,
                    attributionControl: false
                }});
                L.tileLayer('https://{{s}}.basemaps.cartocdn.com/dark_all/{{z}}/{{x}}/{{y}}{{r}}.png', {{
                    attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> &copy; <a href="https://carto.com/">CARTO</a>',
                    subdomains: 'abcd',
                    maxZoom: 19
                }}).addTo(map);
                L.control.attribution({{ position: 'bottomright', prefix: false }}).addTo(map);
                window.leafletMaps = maps;
                maps.set({id_json}, map);
                window.__deflockViewport = null;
                window.__deflockCameraLayer = null;
                window.__deflockCurrentZoom = map.getZoom();

                map.on('moveend', () => {{
                    const c = map.getCenter();
                    const b = map.getBounds();
                    const ne = b.getNorthEast();
                    const radiusM = L.latLng(c.lat, c.lng).distanceTo(L.latLng(ne.lat, ne.lng));
                    const z = map.getZoom();
                    window.__deflockCurrentZoom = z;
                    window.__deflockViewport = {{
                        lat: c.lat,
                        lng: c.lng,
                        radius_km: radiusM / 1000,
                        zoom: z
                    }};
                }});

                if (!window.__placesEscapeHtml) {{
                    window.__placesEscapeHtml = function(str) {{
                        if (!str) return '';
                        return str.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;').replace(/'/g,'&#39;');
                    }};
                }}
                {directions_helpers}

                const c = map.getCenter();
                const b = map.getBounds();
                const ne = b.getNorthEast();
                const radiusM = L.latLng(c.lat, c.lng).distanceTo(L.latLng(ne.lat, ne.lng));
                window.__deflockViewport = {{
                    lat: c.lat, lng: c.lng,
                    radius_km: radiusM / 1000,
                    zoom: map.getZoom()
                }};

                return "true";
                "#,
                popup_style = POPUP_STYLE_JS,
                directions_helpers = DIRECTIONS_HELPERS_JS,
                id_json = id_json,
            ))
            .join()
            .await
            .unwrap_or_default();

            if result == "true" {
                map_initialized.set(true);
            } else {
                log::error!("Deflock: failed to initialize map");
            }
        });
    });

    // Warm the in-memory camera/bbox cache from IndexedDB so revisits are instant.
    // On native targets this is a no-op (stub returns empty vecs).
    use_effect(move || {
        if *cache_warmed.read() || !*map_initialized.read() {
            return;
        }
        cache_warmed.set(true);
        spawn(async move {
            let db = match crate::stores::deflock_cache_db::get_or_open().await {
                Ok(db) => db,
                Err(_) => return,
            };
            let cached_cameras = db.get_all_cameras().await.unwrap_or_default();
            let cached_bboxes = db.get_all_bboxes().await.unwrap_or_default();
            if *unmounted.read() {
                return;
            }
            if !cached_cameras.is_empty() {
                log::info!(
                    "Deflock: warming cache with {} cameras + {} bboxes from IndexedDB",
                    cached_cameras.len(),
                    cached_bboxes.len()
                );
                deflock_store::merge_cameras(cached_cameras);
            }
            for cached in cached_bboxes {
                deflock_store::record_bbox(deflock::BoundingBox {
                    south: cached.south,
                    west: cached.west,
                    north: cached.north,
                    east: cached.east,
                });
            }
        });
    });

    use_effect(move || {
        if *loc_requested.read() || !*map_initialized.read() {
            return;
        }
        loc_requested.set(true);
        spawn(async move {
            match crate::platform::geolocation::get_current_position().await {
                Ok((lat, lon)) => {
                    let id = container_id.read().clone();
                    let id_json = serde_json::to_string(&id).unwrap_or_default();
                    let _ = dioxus::document::eval(&format!(
                        r#"
                        const maps = window.leafletMaps || new Map();
                        const map = maps.get({id_json});
                        if (map) {{ map.setView([{lat}, {lon}], 12); }}
                        window.__placesUserLocation = {{ lat: {lat}, lng: {lon} }};
                        "#,
                        id_json = id_json, lat = lat, lon = lon
                    ));
                }
                Err(e) => {
                    log::warn!("Deflock: geolocation failed: {}", e);
                }
            }
        });
    });

    use_effect(move || {
        if *viewport_poll_started.read() || !*map_initialized.read() {
            return;
        }
        viewport_poll_started.set(true);

        spawn(async move {
            loop {
                crate::platform::timer::sleep(std::time::Duration::from_millis(500)).await;
                if *unmounted.read() {
                    return;
                }

                let viewport_str: String = dioxus::document::eval(
                    "var v = window.__deflockViewport; window.__deflockViewport = null; return v ? JSON.stringify(v) : 'null';"
                )
                .join()
                .await
                .unwrap_or_default();

                if viewport_str == "null" || viewport_str.is_empty() {
                    continue;
                }

                let Ok(viewport) = serde_json::from_str::<serde_json::Value>(&viewport_str) else {
                    continue;
                };
                let Some(lat) = viewport["lat"].as_f64() else { continue; };
                let Some(lng) = viewport["lng"].as_f64() else { continue; };
                let Some(radius_km) = viewport["radius_km"].as_f64() else { continue; };
                let zoom = viewport["zoom"].as_f64().unwrap_or(4.0);

                *deflock_store::VIEWPORT.write() = Some((lat, lng, radius_km, zoom));

                if zoom < 5.0 {
                    continue;
                }

                // Cap the fetch radius so huge viewports (zoomed-out) don't time out Overpass.
                // 250km balances coverage with Overpass rate limits.
                let fetch_radius = radius_km.clamp(20.0, MAX_FETCH_RADIUS_KM);
                // One or two bboxes: near the antimeridian the viewport wraps
                // past ±180° and needs a companion query for the far side.
                let bboxes = deflock::bboxes_for_center_radius(lat, lng, fetch_radius);

                // bbox-coverage dedup: skip fetch when every bbox is fully
                // contained by previously-fetched ones (in-memory or restored
                // from IndexedDB).
                if bboxes.iter().all(deflock_store::is_viewport_covered) {
                    continue;
                }

                *deflock_store::CAMERAS_LOADING.write() = true;

                for bbox in bboxes {
                    if deflock_store::is_viewport_covered(&bbox) {
                        continue;
                    }

                    match deflock::fetch_cameras_in_bbox(bbox).await {
                        Ok(cameras) => {
                            log::info!(
                                "Deflock: {} cameras at ({:.3},{:.3}) r={:.0}km (zoom {})",
                                cameras.len(), lat, lng, fetch_radius, zoom
                            );
                            deflock_store::merge_cameras(cameras.clone());
                            deflock_store::record_bbox(bbox);
                            *deflock_store::LAST_ERROR.write() = None;
                            // Persist to IndexedDB (fire-and-forget). On wasm this is the
                            // user's session-spanning cache; on native it's a no-op stub.
                            // Compacts as it goes: absorbed/stale rows are dropped so the
                            // store mirrors the in-memory containment-merge instead of
                            // accumulating redundant coverage rows forever.
                            let bboxes_for_db = bbox;
                            spawn(async move {
                                let db = match crate::stores::deflock_cache_db::get_or_open().await
                                {
                                    Ok(db) => db,
                                    Err(_) => return,
                                };
                                let _ = db.bulk_insert_cameras(&cameras).await;
                                let new_bbox = bboxes_for_db;
                                let cached_bbox = crate::stores::deflock_cache_db::CachedBbox {
                                    south: new_bbox.south,
                                    west: new_bbox.west,
                                    north: new_bbox.north,
                                    east: new_bbox.east,
                                };
                                match db.get_all_bboxes().await {
                                    Ok(stored) => {
                                        let to_bb = |c: &crate::stores::deflock_cache_db::CachedBbox| {
                                            deflock::BoundingBox {
                                                south: c.south,
                                                west: c.west,
                                                north: c.north,
                                                east: c.east,
                                            }
                                        };
                                        // Any stored row fully contained by the new bbox
                                        // gets absorbed by the rewrite.
                                        let any_absorbed = stored
                                            .iter()
                                            .any(|c| {
                                                deflock_store::contains(&new_bbox, &to_bb(c))
                                            });
                                        // New bbox is redundant (a stored row covers it).
                                        let covered = stored
                                            .iter()
                                            .any(|c| deflock_store::contains(&to_bb(c), &new_bbox));
                                        if !any_absorbed && covered {
                                            return;
                                        }
                                        if !any_absorbed {
                                            let _ = db.insert_bbox(&cached_bbox).await;
                                            return;
                                        }
                                        // Rewrite the compacted set.
                                        if db.clear_bboxes().await.is_err() {
                                            return;
                                        }
                                        let _ = db.insert_bbox(&cached_bbox).await;
                                        for c in stored.iter().filter(|c| {
                                            !deflock_store::contains(&new_bbox, &to_bb(c))
                                        }) {
                                            let _ = db.insert_bbox(c).await;
                                        }
                                    }
                                    Err(_) => {
                                        let _ = db.insert_bbox(&cached_bbox).await;
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            log::warn!("Deflock: Overpass fetch failed: {}", e);
                            *deflock_store::LAST_ERROR.write() = Some(e);
                        }
                    }
                }
                *deflock_store::CAMERAS_LOADING.write() = false;
            }
        });
    });

    use_effect(move || {
        if *route_poll_started.read() || !*map_initialized.read() {
            return;
        }
        route_poll_started.set(true);

        spawn(async move {
            loop {
                crate::platform::timer::sleep(std::time::Duration::from_millis(500)).await;
                if *unmounted.read() {
                    return;
                }
                let result: String = dioxus::document::eval(
                    "return window.__placesRouteInfo ? JSON.stringify(window.__placesRouteInfo) : 'null'"
                )
                .join()
                .await
                .unwrap_or_default();

                if result != "null" && !result.is_empty() {
                    if let Ok(info) = serde_json::from_str::<serde_json::Value>(&result) {
                        let dir_info = places_store::DirectionsInfo {
                            distance_km: info["distance_km"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
                            duration_min: info["duration_min"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
                            dest_name: info["dest_name"].as_str().unwrap_or("").to_string(),
                            dest_lat: info["dest_lat"].as_f64().unwrap_or(0.0),
                            dest_lng: info["dest_lng"].as_f64().unwrap_or(0.0),
                        };
                        *places_store::DIRECTIONS.write() = Some(dir_info);
                        let _ = dioxus::document::eval("window.__placesRouteInfo = null").await;
                    }
                }
            }
        });
    });

    use_effect(move || {
        if !*map_initialized.read() {
            return;
        }

        // Read VIEWPORT to establish a reactive subscription so the effect re-runs
        // when zoom changes (the viewport poll writes VIEWPORT on every moveend,
        // which Leaflet fires for both pan AND zoom).
        let viewport_zoom = deflock_store::VIEWPORT
            .read()
            .as_ref()
            .map(|(_, _, _, zoom)| *zoom)
            .unwrap_or(0.0);
        // Include "is above cone threshold" flag in the hash so markers re-render
        // when crossing the zoom threshold, not on every incremental zoom step.
        let cone_mode = viewport_zoom >= CAMERA_CONE_ZOOM_THRESHOLD;

        let cameras = deflock_store::get_filtered_cameras();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        cone_mode.hash(&mut hasher);
        for c in &cameras {
            c.osm_id.hash(&mut hasher);
            c.lat.to_bits().hash(&mut hasher);
            c.lon.to_bits().hash(&mut hasher);
        }
        let current_hash = hasher.finish();
        if *last_marker_hash.read() == Some(current_hash) {
            return;
        }
        last_marker_hash.set(Some(current_hash));

        let id = container_id.read().clone();
        let id_json = serde_json::to_string(&id).unwrap_or_default();
        let cameras_json = serde_json::to_string(&cameras).unwrap_or_else(|_| "[]".to_string());

        let _ = dioxus::document::eval(&build_camera_markers_js(&id_json, &cameras_json, viewport_zoom));
    });

    let loading = *deflock_store::CAMERAS_LOADING.read();
    let camera_count = deflock_store::CAMERAS.read().len();
    let error = deflock_store::LAST_ERROR.read().clone();
    let directions = places_store::DIRECTIONS.read().clone();

    rsx! {
        div { class: "fixed inset-0 bg-[#1a1a2e] z-50",
            div {
                id: "{container_id}",
                style: "position: absolute; inset: 0; z-index: 1;",
            }

            // Back chevron — top-left, mirrors PlacesMapContainer layout.
            Link {
                to: crate::routes::Route::Explore {},
                class: "fixed top-4 left-4 z-[60] flex items-center justify-center w-10 h-10 rounded-full bg-black/60 backdrop-blur-md text-white hover:bg-black/80 transition",
                svg {
                    class: "w-5 h-5",
                    xmlns: "http://www.w3.org/2000/svg",
                    fill: "none",
                    view_box: "0 0 24 24",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M15 19l-7-7 7-7",
                    }
                }
            }

            // Centered camera-count pill — top-center, mirrors Places' "Places" pill.
            div { class: "fixed top-4 left-1/2 -translate-x-1/2 z-[60]",
                div { class: "rounded-full border border-white/15 bg-black/60 px-4 py-2 backdrop-blur-md flex items-center gap-2",
                    span { class: "text-sm font-semibold text-white",
                        "📷 {camera_count} ALPR"
                    }
                    if loading {
                        span { class: "inline-block h-3 w-3 rounded-full border-2 border-white/30 border-t-white animate-spin" }
                    }
                }
            }

            if let Some(err) = error.as_ref() {
                div { class: "fixed top-20 left-4 right-4 z-[60] bg-red-900/80 backdrop-blur rounded-lg px-4 py-2",
                    p { class: "text-red-200 text-xs", "{err}" }
                }
            }

            DeflockFilterBar {}

            if let Some(dir) = directions.as_ref() {
                div { class: "fixed bottom-20 right-4 z-[60] bg-black/80 backdrop-blur-md rounded-xl p-3 max-w-xs",
                    div { class: "flex items-center justify-between mb-1",
                        span { class: "text-sm font-medium text-white", "Route" }
                        button {
                            class: "text-white/40 hover:text-white text-xs",
                            onclick: move |_| {
                                *places_store::DIRECTIONS.write() = None;
                                let id_json =
                                    serde_json::to_string(&container_id.read().clone())
                                        .unwrap_or_default();
                                let _ = dioxus::document::eval(&format!(
                                    "window.__clearRouteFor({id_json})"
                                ));
                            },
                            "✕"
                        }
                    }
                    div { class: "text-xs text-white/70",
                        "{dir.distance_km} km · {dir.duration_min} min"
                    }
                    div { class: "text-xs text-white/50 mt-1",
                        "To: {dir.dest_name}"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build_camera_markers_js;

    /// The popup Directions button must interpolate the map id as a QUOTED
    /// JS string — see the matching test in places/map_container.rs.
    #[test]
    fn test_directions_onclick_quotes_map_id() {
        let js = build_camera_markers_js(r#""deflock-map-1-0""#, "[]", 5.0);
        assert!(js.contains(r#"window.__requestDirectionsFor('${mapId}'"#));
        assert!(!js.contains(r#"For(${mapId},"#));
    }
}
