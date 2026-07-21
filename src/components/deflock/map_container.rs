use crate::services::{deflock, places};
use crate::stores::{deflock_store, places_store};
use crate::utils::leaflet_shared::{LEAFLET_LOAD_JS, POPUP_STYLE_JS};
use crate::components::deflock::filter_bar::DeflockFilterBar;
use dioxus::prelude::*;
use dioxus_core::use_drop;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

static DEFLOCK_MAP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn build_camera_markers_js(id_json: &str, cameras_json: &str) -> String {
    format!(
        r##"(() => {{
            const maps = window.leafletMaps || new Map();
            const map = maps.get({id_json});
            if (!map) return;

            if (window.__deflockCameraLayer) {{
                map.removeLayer(window.__deflockCameraLayer);
            }}
            window.__deflockCameraLayer = L.layerGroup().addTo(map);

            const cameras = {cameras_json};
            const esc = window.__placesEscapeHtml || (function(s) {{
                if (!s) return '';
                return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;').replace(/'/g,'&#39;');
            }});

            cameras.forEach(c => {{
                const dir = c.direction;
                let coneSvg = '';
                if (dir !== null && dir !== undefined) {{
                    coneSvg = `<div style="position:absolute;top:0;left:0;width:32px;height:40px;pointer-events:none;">
                        <svg width="32" height="40" viewBox="0 0 32 40" style="position:absolute;top:0;left:0;transform:rotate(${{dir}}deg);transform-origin:16px 15px;">
                            <path d="M16 15 L2 2 A18 18 0 0 1 30 2 Z" fill="rgba(239,68,68,0.15)" stroke="rgba(239,68,68,0.4)" stroke-width="1"/>
                        </svg>
                    </div>`;
                }}
                const iconHtml = `<div style="position:relative;width:32px;height:40px;">
                    ${{coneSvg}}
                    <svg width="32" height="40" viewBox="0 0 32 40">
                        <path d="M16 0C7.2 0 0 7.2 0 16c0 12 16 24 16 24s16-12 16-24C32 7.2 24.8 0 16 0z" fill="#ef4444" stroke="#fff" stroke-width="1.5"/>
                        <circle cx="16" cy="15" r="6" fill="#1a1a2e"/>
                        <circle cx="16" cy="15" r="3" fill="#ef4444"/>
                    </svg>
                </div>`;

                const icon = L.divIcon({{
                    html: iconHtml,
                    className: 'deflock-marker',
                    iconSize: [32, 40],
                    iconAnchor: [16, 40],
                    popupAnchor: [0, -40]
                }});
                const marker = L.marker([c.lat, c.lon], {{ icon }}).addTo(window.__deflockCameraLayer);

                const s = (v) => v ? esc(String(v)) : '';
                const operator = s(c.operator) || 'Unknown Operator';
                const brand = s(c.brand);
                const zone = s(c.surveillance_zone);
                const mount = s(c.mount_type);
                const direction = c.direction_cardinal ? s(c.direction_cardinal) : (c.direction !== null && c.direction !== undefined ? c.direction + '°' : '');
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
                        <button onclick="window.__placesRequestDirections(${{c.lat}},${{c.lon}},'Camera ${{c.osm_id}}')"
                            style="padding:5px 14px;border-radius:6px;background:#ef4444;color:#fff;border:none;cursor:pointer;font-size:12px;font-weight:500;">
                            Directions
                        </button>
                    </div>
                </div>`;

                marker.bindPopup(popup, {{ maxWidth: 300, className: 'places-popup' }});
            }});
        }})()"##
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
    let mut map_initialized = use_signal(|| false);
    let mut unmounted = use_signal(|| false);
    let mut loc_requested = use_signal(|| false);
    let mut viewport_poll_started = use_signal(|| false);
    let mut route_poll_started = use_signal(|| false);
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
        if !*leaflet_loaded.read() || *map_initialized.read() {
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

                map.on('moveend', () => {{
                    const c = map.getCenter();
                    const b = map.getBounds();
                    const ne = b.getNorthEast();
                    const radiusM = L.latLng(c.lat, c.lng).distanceTo(L.latLng(ne.lat, ne.lng));
                    window.__deflockViewport = {{
                        lat: c.lat,
                        lng: c.lng,
                        radius_km: radiusM / 1000,
                        zoom: map.getZoom()
                    }};
                }});

                if (!window.__placesEscapeHtml) {{
                    window.__placesEscapeHtml = function(str) {{
                        if (!str) return '';
                        return str.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;').replace(/'/g,'&#39;');
                    }};
                }}
                if (!window.__placesRequestDirections) {{
                    window.__placesRequestDirections = async function(toLat, toLng, toName) {{
                        const ul = window.__placesUserLocation;
                        if (!ul) {{ alert('Enable location first'); return; }}
                        try {{
                            const url = 'https://router.project-osrm.org/route/v1/driving/'+ul.lng+','+ul.lat+';'+toLng+','+toLat+'?overview=full&geometries=geojson';
                            const resp = await fetch(url);
                            const data = await resp.json();
                            if (!data.routes || !data.routes.length) {{ alert('No route found'); return; }}
                            const route = data.routes[0];
                            const coords = route.geometry.coordinates.map(c => [c[1], c[0]]);
                            if (window.__placesRouteLayer) map.removeLayer(window.__placesRouteLayer);
                            window.__placesRouteLayer = L.polyline(coords, {{
                                color: '#ef4444', weight: 5, opacity: 0.8, dashArray: '10, 8'
                            }}).addTo(map);
                            map.fitBounds(window.__placesRouteLayer.getBounds(), {{ padding: [60, 60] }});
                            window.__placesRouteInfo = {{
                                distance_km: (route.distance / 1000).toFixed(1),
                                duration_min: (route.duration / 60).toFixed(0),
                                dest_name: toName,
                                dest_lat: toLat,
                                dest_lng: toLng
                            }};
                        }} catch(e) {{
                            console.error('OSRM error:', e);
                        }}
                    }};
                }}

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

                if zoom < 8.0 {
                    continue;
                }

                let precisions = places::geohash_precisions_for_zoom(zoom);
                for precision in &precisions {
                    let prefix = places::geohash_prefix(lat, lng, *precision);
                    if deflock_store::is_geohash_fetched(&prefix) {
                        continue;
                    }
                    deflock_store::mark_geohash_fetched(&prefix);

                    let bbox = deflock::BoundingBox::from_center_radius(lat, lng, radius_km.max(20.0));
                    *deflock_store::CAMERAS_LOADING.write() = true;

                    match deflock::fetch_cameras_in_bbox(bbox).await {
                        Ok(cameras) => {
                            log::info!(
                                "Deflock: {} cameras for geohash '{}' at ({:.3},{:.3}) r={:.0}km",
                                cameras.len(), prefix, lat, lng, radius_km
                            );
                            deflock_store::merge_cameras(cameras);
                            *deflock_store::LAST_ERROR.write() = None;
                        }
                        Err(e) => {
                            log::warn!("Deflock: Overpass fetch failed for '{}': {}", prefix, e);
                            *deflock_store::LAST_ERROR.write() = Some(e);
                            deflock_store::FETCHED_GEOHASHES.write().remove(&prefix);
                        }
                    }
                    *deflock_store::CAMERAS_LOADING.write() = false;
                }
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

        let cameras = deflock_store::get_filtered_cameras();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
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

        let _ = dioxus::document::eval(&build_camera_markers_js(&id_json, &cameras_json));
    });

    let loading = *deflock_store::CAMERAS_LOADING.read();
    let camera_count = deflock_store::CAMERAS.read().len();
    let error = deflock_store::LAST_ERROR.read().clone();
    let directions = places_store::DIRECTIONS.read().clone();

    rsx! {
        div { class: "relative w-full h-screen",
            div {
                id: "{container_id}",
                class: "absolute inset-0 z-0",
                style: "background: #1a1a2e;"
            }

            div { class: "absolute top-0 left-0 right-0 z-[1000] pointer-events-none",
                div { class: "bg-black/60 backdrop-blur-md px-4 py-2 flex items-center justify-between pointer-events-auto",
                    div { class: "flex items-center gap-3",
                        span { class: "text-red-400 text-lg", "📷" }
                        span { class: "text-white text-sm font-medium",
                            "{camera_count} ALPR cameras loaded"
                        }
                        if loading {
                            span { class: "inline-block h-3 w-3 rounded-full border-2 border-white/30 border-t-white animate-spin ml-2" }
                        }
                    }
                    Link {
                        to: crate::routes::Route::DeflockHome {},
                        class: "text-white/60 hover:text-white text-xs",
                        "← Back"
                    }
                }
            }

            if let Some(err) = error.as_ref() {
                div { class: "absolute top-14 left-4 right-4 z-[1000] bg-red-900/80 backdrop-blur rounded-lg px-4 py-2",
                    p { class: "text-red-200 text-xs", "{err}" }
                }
            }

            DeflockFilterBar {}

            if let Some(dir) = directions.as_ref() {
                div { class: "absolute bottom-20 right-4 z-[1000] bg-black/80 backdrop-blur-md rounded-xl p-3 max-w-xs",
                    div { class: "flex items-center justify-between mb-1",
                        span { class: "text-sm font-medium text-white", "Route" }
                        button {
                            class: "text-white/40 hover:text-white text-xs",
                            onclick: move |_| {
                                *places_store::DIRECTIONS.write() = None;
                                let _ = dioxus::document::eval(
                                    "if (window.__placesRouteLayer) { const m = (window.leafletMaps || new Map()).values().next().value; if (m) m.removeLayer(window.__placesRouteLayer); window.__placesRouteLayer = null; }"
                                );
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
