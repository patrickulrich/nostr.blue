use crate::services::places;
use crate::stores::nostr_client;
use crate::stores::notification_dispatcher::DispatcherHandle;
use crate::stores::places_store;
use crate::stores::places_store::MapMode;
use crate::utils::leaflet_shared::{LEAFLET_LOAD_JS, POPUP_STYLE_JS};
use dioxus::prelude::*;
use dioxus_core::use_drop;
use nostr_relay_pool::relay::ReqExitPolicy;
use nostr_relay_pool::SubscribeAutoCloseOptions;
use nostr_sdk::nips::nip01::Coordinate;
use nostr_sdk::prelude::*;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast::error::RecvError;

static PLACES_MAP_COUNTER: AtomicU64 = AtomicU64::new(0);

type OnPlaceEvent = Arc<Mutex<Box<dyn FnMut(&nostr::Event)>>>;

struct PlacesSubState {
    sub_id: SubscriptionId,
    client: Arc<nostr_sdk::Client>,
    handle: Option<DispatcherHandle>,
}

#[allow(clippy::arc_with_non_send_sync)]
fn spawn_places_listener(
    rx: tokio::sync::mpsc::UnboundedReceiver<std::sync::Arc<nostr::Event>>,
    on_event: OnPlaceEvent,
) {
    spawn(async move {
        let mut rx = rx;
        let mut buffer = Vec::new();
        while let Some(event) = rx.recv().await {
            buffer.push(event);
            while let Ok(event) = rx.try_recv() {
                buffer.push(event);
            }
            if let Ok(mut cb) = on_event.lock() {
                for event in &buffer {
                    cb(event);
                }
            }
            buffer.clear();
        }
    });
}

#[allow(clippy::arc_with_non_send_sync)]
fn spawn_places_fallback_listener(
    client: Arc<nostr_sdk::Client>,
    sub_id: SubscriptionId,
    on_event: OnPlaceEvent,
) {
    spawn(async move {
        let mut notifications = client.notifications();
        let mut buffer = Vec::new();
        loop {
            match notifications.recv().await {
                Ok(RelayPoolNotification::Event {
                    subscription_id,
                    event,
                    ..
                }) => {
                    if subscription_id == sub_id {
                        buffer.push(event);
                        while let Ok(notification) = notifications.try_recv() {
                            if let RelayPoolNotification::Event {
                                subscription_id: sid,
                                event,
                                ..
                            } = notification
                            {
                                if sid == sub_id {
                                    buffer.push(event);
                                }
                            }
                        }
                        if let Ok(mut cb) = on_event.lock() {
                            for event in &buffer {
                                cb(event);
                            }
                        }
                        buffer.clear();
                    }
                }
                Ok(RelayPoolNotification::Shutdown) => break,
                // Transient: keep going so places updates don't silently stop.
                Err(RecvError::Lagged(skipped)) => {
                    log::warn!(
                        "places listener: lagged, skipped {} events, continuing",
                        skipped
                    );
                    continue;
                }
                Err(RecvError::Closed) => break,
                Ok(_) => {}
            }
        }
    });
}

// Leaflet loader + popup CSS constants imported from `utils::leaflet_shared`.

fn build_markers_js(id_json: &str, markers_json: &str) -> String {
    format!(
        r##"(() => {{
            const maps = window.leafletMaps || new Map();
            const map = maps.get({id_json});
            if (!map) return;
            const markers = {markers_json};
            const esc = window.__placesEscapeHtml;
            markers.forEach(m => {{
                let iconHtml;
                if (m.type === 'nostr' || m.type === 'nostr_btcmap') {{
                    const dual = m.type === 'nostr_btcmap';
                    iconHtml = `<div style="position:relative;width:32px;height:40px;">
                        <svg width="32" height="40" viewBox="0 0 32 40">
                            ${{dual ? '<path d="M16 0C7.2 0 0 7.2 0 16c0 12 16 24 16 24s16-12 16-24C32 7.2 24.8 0 16 0z" fill="none" stroke="#f7931a" stroke-width="4"/>' : ''}}
                            <path d="M16 0C7.2 0 0 7.2 0 16c0 12 16 24 16 24s16-12 16-24C32 7.2 24.8 0 16 0z" fill="#7c3aed" stroke="#fff" stroke-width="1.5"/>
                            <circle cx="16" cy="15" r="7" fill="#1a1a2e"/>
                        </svg>
                        ${{dual ? '<div style="position:absolute;bottom:4px;right:-4px;width:14px;height:14px;border-radius:50%;background:#f7931a;border:2px solid #fff;display:flex;align-items:center;justify-content:center;font-size:8px;font-weight:bold;color:#fff;">₿</div>' : ''}}
                    </div>`;
                }} else {{
                    iconHtml = `<div style="position:relative;width:32px;height:40px;">
                        <svg width="32" height="40" viewBox="0 0 32 40">
                            <path d="M16 0C7.2 0 0 7.2 0 16c0 12 16 24 16 24s16-12 16-24C32 7.2 24.8 0 16 0z" fill="#f7931a" stroke="#fff" stroke-width="1.5"/>
                            <text x="16" y="19" text-anchor="middle" fill="#fff" font-size="12" font-weight="bold">₿</text>
                        </svg>
                    </div>`;
                }}
                const icon = L.divIcon({{
                    html: iconHtml,
                    className: 'places-marker',
                    iconSize: [32, 40],
                    iconAnchor: [16, 40],
                    popupAnchor: [0, -40]
                }});
                const marker = L.marker([m.lat, m.lng], {{ icon }}).addTo(map);
                const s = (v) => v ? esc(String(v)) : '';
                const name = s(m.name) || 'Unnamed Place';
                const amenity = s(m.amenity);
                const desc = s(m.desc);
                const phone = s(m.phone);
                const website = s(m.website);
                const hours = s(m.hours);
                const street = s(m.street);
                const city = s(m.city);
                const state = s(m.state);
                const addr = s(m.address);
                const pubkey = s(m.pubkey);
                const hasBtcmap = m.btcmap === true;
                const safeWeb = website && /^https?:\/\//i.test(m.website) ? website : null;
                let popup = '';
                if (m.type === 'nostr' || m.type === 'nostr_btcmap') {{
                    const isOpen = hours ? window.__placesIsOpenNow(m.hours) : null;
                    const fmtHours = hours ? window.__placesFormatHours(m.hours) : '';
                    const addrLine = [street, city, state].filter(Boolean).join(', ');
                    popup = `<div style="min-width:220px;max-width:280px;color:#e5e5e5;font-family:-apple-system,BlinkMacSystemFont,sans-serif;font-size:13px;line-height:1.4;">
                        <div style="font-size:15px;font-weight:600;color:#fff;margin-bottom:2px;">${{name}}</div>
                        ${{amenity ? '<div style="color:#a3a3a3;font-size:11px;text-transform:capitalize;margin-bottom:6px;">' + amenity.replace(/_/g,' ') + '</div>' : ''}}
                        ${{hasBtcmap ? '<span style="display:inline-block;background:#f7931a;color:#fff;border-radius:9999px;padding:1px 8px;font-size:10px;font-weight:600;margin-bottom:6px;">₿ Bitcoin</span>' : ''}}
                        <div style="border-top:1px solid rgba(255,255,255,0.1);margin:6px 0;"></div>
                        ${{desc ? '<div style="color:#d4d4d4;font-size:12px;margin-bottom:6px;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden;">' + desc + '</div>' : ''}}
                        ${{hours ? '<div style="margin-bottom:4px;"><span style="color:' + (isOpen === true ? '#4ade80' : isOpen === false ? '#737373' : '#a3a3a3') + ';font-size:12px;">' + (isOpen === true ? '● Open Now' : isOpen === false ? '● Closed' : '') + '</span>' + (fmtHours ? '<div style="color:#737373;font-size:11px;margin-top:2px;">' + fmtHours + '</div>' : '') + '</div>' : ''}}
                        ${{addrLine ? '<div style="color:#a3a3a3;font-size:12px;margin-bottom:3px;">📍 ' + addrLine + '</div>' : ''}}
                        ${{phone ? '<div style="margin-bottom:3px;"><a href="tel:' + phone + '" style="color:#a78bfa;text-decoration:none;font-size:12px;">📞 ' + phone + '</a></div>' : ''}}
                        ${{safeWeb ? '<div style="margin-bottom:3px;"><a href="' + safeWeb + '" target="_blank" rel="noopener" style="color:#a78bfa;text-decoration:none;font-size:12px;word-break:break-all;">🌐 ' + safeWeb.replace(new RegExp("^https?://"), "") + '</a></div>' : ''}}
                        <div style="display:flex;gap:6px;margin-top:8px;">
                            ${{s(m.naddr) ? '<a href="/' + s(m.naddr) + '" style="padding:5px 14px;border-radius:6px;background:transparent;color:#a78bfa;border:1px solid #7c3aed;cursor:pointer;font-size:12px;font-weight:500;text-decoration:none;">View Details</a>' : ''}}
                            <button onclick="window.__placesRequestDirections(${{m.lat}},${{m.lng}},'${{name.replace(/'/g, "\\\\'")}}')"
                                style="padding:5px 14px;border-radius:6px;background:#7c3aed;color:#fff;border:none;cursor:pointer;font-size:12px;font-weight:500;">
                                Directions
                            </button>
                        </div>
                    </div>`;
                }} else {{
                    popup = `<div style="min-width:200px;max-width:260px;color:#e5e5e5;font-family:-apple-system,BlinkMacSystemFont,sans-serif;font-size:13px;line-height:1.4;">
                        <div style="font-size:15px;font-weight:600;color:#fff;margin-bottom:2px;">${{name}}</div>
                        <span style="display:inline-block;background:#f7931a;color:#fff;border-radius:9999px;padding:1px 8px;font-size:10px;font-weight:600;margin-bottom:6px;">₿ Bitcoin Location</span>
                        <div style="border-top:1px solid rgba(255,255,255,0.1);margin:6px 0;"></div>
                        ${{addr ? '<div style="color:#a3a3a3;font-size:12px;margin-bottom:3px;">📍 ' + addr + '</div>' : ''}}
                        ${{phone ? '<div style="margin-bottom:3px;"><a href="tel:' + phone + '" style="color:#a78bfa;text-decoration:none;font-size:12px;">📞 ' + phone + '</a></div>' : ''}}
                        ${{safeWeb ? '<div style="margin-bottom:3px;"><a href="' + safeWeb + '" target="_blank" rel="noopener" style="color:#a78bfa;text-decoration:none;font-size:12px;word-break:break-all;">🌐 ' + safeWeb.replace(new RegExp("^https?://"), "") + '</a></div>' : ''}}
                        ${{hours ? '<div style="color:#737373;font-size:11px;">' + window.__placesFormatHours(m.hours) + '</div>' : ''}}
                        <div style="margin-top:8px;">
                            <button onclick="window.__placesRequestDirections(${{m.lat}},${{m.lng}},'${{name.replace(/'/g, "\\\\'")}}')"
                                style="padding:5px 14px;border-radius:6px;background:#7c3aed;color:#fff;border:none;cursor:pointer;font-size:12px;font-weight:500;">
                                Directions
                            </button>
                        </div>
                    </div>`;
                }}
                marker.bindPopup(popup, {{
                    maxWidth: 300,
                    className: 'places-popup'
                }});
            }});
        }})()"##
    )
}

#[component]
pub fn PlacesMapContainer() -> Element {
    let container_id = use_signal(|| {
        format!(
            "places-map-{}-{}",
            crate::platform::timestamp::now_millis(),
            PLACES_MAP_COUNTER.fetch_add(1, Ordering::Relaxed),
        )
    });

    let mut leaflet_loaded = use_signal(|| false);
    let mut map_initialized = use_signal(|| false);
    let mut unmounted = use_signal(|| false);
    let mut loc_requested = use_signal(|| false);
    let mut viewport_poll_started = use_signal(|| false);
    let mut click_poll_started = use_signal(|| false);
    let mut route_poll_started = use_signal(|| false);
    let mut places_sub_state: Signal<Option<Arc<PlacesSubState>>> = use_signal(|| None);
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
            }})()
            "#
        ));
    });

    use_drop(move || {
        if let Some(state) = places_sub_state() {
            spawn(async move {
                match Arc::try_unwrap(state) {
                    Ok(s) => {
                        if let Some(handle) = s.handle {
                            handle.unregister().await;
                        } else {
                            let _ = s.client.unsubscribe(&s.sub_id).await;
                        }
                    }
                    Err(arc) => {
                        let _ = arc.client.unsubscribe(&arc.sub_id).await;
                    }
                }
                log::info!("Places: subscription cleaned up");
            });
        }
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
                log::error!("Failed to load Leaflet: {}", result);
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
                    center: [40.0, -95.0],
                    zoom: 8,
                    minZoom: 8,
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
                window.__placesRouteLayer = null;
                window.__placesRouteInfo = null;
                window.__placesViewport = null;
                map.on('moveend', () => {{
                    const c = map.getCenter();
                    const b = map.getBounds();
                    const ne = b.getNorthEast();
                    const radiusM = L.latLng(c.lat, c.lng).distanceTo(L.latLng(ne.lat, ne.lng));
                    window.__placesViewport = {{
                        lat: c.lat,
                        lng: c.lng,
                        radius_km: radiusM / 1000,
                        zoom: map.getZoom()
                    }};
                }});
                window.__placesAddMode = false;
                window.__placesTempMarker = null;
                window.__placesClickCoords = null;
                map.on('click', function(e) {{
                    if (!window.__placesAddMode) return;
                    if (window.__placesTempMarker) {{
                        window.__placesTempMarker.setLatLng(e.latlng);
                    }} else {{
                        window.__placesTempMarker = L.marker(e.latlng, {{
                            draggable: true,
                            icon: L.divIcon({{
                                className: '',
                                html: '<div style="width:24px;height:24px;background:#7c3aed;border:3px solid #fff;border-radius:50%;box-shadow:0 2px 8px rgba(0,0,0,0.4);"></div>',
                                iconSize: [24, 24],
                                iconAnchor: [12, 12]
                            }})
                        }}).addTo(map);
                    }}
                    var pos = window.__placesTempMarker.getLatLng();
                    window.__placesClickCoords = {{lat: pos.lat, lng: pos.lng}};
                }});
                window.__placesRequestDirections = async function(toLat, toLng, toName) {{
                    const ul = window.__placesUserLocation;
                    if (!ul) {{
                        alert('Please enable location first (tap the location button)');
                        return;
                    }}
                    try {{
                        const url = 'https://router.project-osrm.org/route/v1/driving/'+ul.lng+','+ul.lat+';'+toLng+','+toLat+'?overview=full&geometries=geojson';
                        const resp = await fetch(url);
                        const data = await resp.json();
                        if (!data.routes || !data.routes.length) {{
                            alert('No route found');
                            return;
                        }}
                        const route = data.routes[0];
                        const coords = route.geometry.coordinates.map(c => [c[1], c[0]]);
                        if (window.__placesRouteLayer) {{
                            map.removeLayer(window.__placesRouteLayer);
                        }}
                        window.__placesRouteLayer = L.polyline(coords, {{
                            color: '#7c3aed', weight: 5, opacity: 0.8, dashArray: '10, 8'
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
                        alert('Route fetch failed');
                    }}
                }};
                window.__placesClearRoute = function() {{
                    if (window.__placesRouteLayer) {{
                        map.removeLayer(window.__placesRouteLayer);
                        window.__placesRouteLayer = null;
                    }}
                    window.__placesRouteInfo = null;
                }};
                window.__placesIsOpenNow = function(hoursStr) {{
                    if (!hoursStr) return null;
                    try {{
                        const dayAbbr = {{ Sunday: 'Su', Monday: 'Mo', Tuesday: 'Tu', Wednesday: 'We', Thursday: 'Th', Friday: 'Fr', Saturday: 'Sa' }};
                        const days = ['Su','Mo','Tu','We','Th','Fr','Sa'];
                        const now = new Date();
                        const dayName = now.toLocaleDateString('en-US', {{ weekday: 'long' }});
                        const today = dayAbbr[dayName] || dayName.slice(0,2);
                        const time = now.getHours() + now.getMinutes() / 60;
                        const groups = hoursStr.split(/;\s*/);
                        for (const g of groups) {{
                            const parts = g.trim().split(/\s+/);
                            if (parts.length < 2) continue;
                            const dayRange = parts[0];
                            const timeRange = parts.slice(1).join('');
                            if (dayRange === today || dayRange.toLowerCase() === '24/7') return true;
                            const dashIdx = dayRange.indexOf('-');
                            let match = false;
                            if (dashIdx !== -1) {{
                                const startD = dayRange.slice(0, dashIdx);
                                const endD = dayRange.slice(dashIdx + 1);
                                const si = days.indexOf(startD);
                                const ei = days.indexOf(endD);
                                const ti = days.indexOf(today);
                                if (si <= ei) match = ti >= si && ti <= ei;
                                else match = ti >= si || ti <= ei;
                            }}
                            if (!match) continue;
                            if (timeRange.toLowerCase() === '24/7' || timeRange === 'off' || timeRange === 'closed') return timeRange !== 'off' && timeRange !== 'closed';
                            const times = timeRange.split('-');
                            if (times.length === 2) {{
                                const o = times[0].split(':');
                                const c = times[1].split(':');
                                const openT = parseInt(o[0]) + (o.length > 1 ? parseInt(o[1]) / 60 : 0);
                                const closeT = parseInt(c[0]) + (c.length > 1 ? parseInt(c[1]) / 60 : 0);
                                return time >= openT && time < closeT;
                            }}
                        }}
                    }} catch(e) {{}}
                    return null;
                }};
                window.__placesFormatHours = function(hoursStr) {{
                    if (!hoursStr) return '';
                    return hoursStr.replace(/\b(Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday)\b/g, (m) => {{
                        return {{Monday:'Mo',Tuesday:'Tu',Wednesday:'We',Thursday:'Th',Friday:'Fr',Saturday:'Sa',Sunday:'Su'}}[m] || m.slice(0,2);
                    }});
                }};
                window.__placesEscapeHtml = function(str) {{
                    if (!str) return '';
                    return str.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;').replace(/'/g,'&#39;');
                }};
                return "true";
                "#,
                popup_style = POPUP_STYLE_JS,
            ))
            .join()
            .await
            .unwrap_or_default();
            if result == "true" {
                map_initialized.set(true);
            } else {
                log::error!("Failed to initialize places map");
            }
        });
    });

    use_effect(move || {
        if *loc_requested.read() || !*map_initialized.read() {
            return;
        }
        loc_requested.set(true);
        *places_store::LOC_LOADING.write() = true;

        spawn(async move {
            match crate::platform::geolocation::get_current_position().await {
                Ok((lat, lon)) => {
                    *places_store::USER_LOCATION.write() = Some((lat, lon));
                    *places_store::LOC_LOADING.write() = false;
                }
                Err(e) => {
                    log::warn!("Geolocation failed: {}", e);
                    *places_store::LOC_LOADING.write() = false;
                }
            }
        });
    });

    use_effect(move || {
        let user_loc = *places_store::USER_LOCATION.read();
        if user_loc.is_none() || !*map_initialized.read() {
            return;
        }
        let (lat, lng) = user_loc.unwrap();
        let id = container_id.read().clone();
        let id_json = serde_json::to_string(&id).unwrap_or_default();
        let loc_json =
            serde_json::to_string(&serde_json::json!({"lat": lat, "lng": lng})).unwrap_or_default();

        spawn(async move {
            let _ = dioxus::document::eval(&format!(
                r#"
                (() => {{
                    const maps = window.leafletMaps || new Map();
                    const map = maps.get({id_json});
                    if (!map) return;
                    map.flyTo([{lat}, {lng}], 13, {{ duration: 1.5 }});
                    window.__placesUserLocation = {loc_json};
                    if (!window.__placesUserDot) {{
                        const pulseIcon = L.divIcon({{
                            html: '<div style="width:20px;height:20px;position:relative;"><div style="position:absolute;inset:0;border-radius:50%;background:rgba(59,130,246,0.3);animation:__pulse 2s infinite;"></div><div style="position:absolute;top:6px;left:6px;width:8px;height:8px;border-radius:50%;background:#3b82f6;border:2px solid #fff;"></div></div><style>@keyframes __pulse{{0%{{transform:scale(1);opacity:1}}100%{{transform:scale(3);opacity:0}}}}</style>',
                            className: '',
                            iconSize: [20, 20],
                            iconAnchor: [10, 10]
                        }});
                        window.__placesUserDot = L.marker([{lat}, {lng}], {{ icon: pulseIcon, zIndexOffset: 1000 }}).addTo(map);
                    }} else {{
                        window.__placesUserDot.setLatLng([{lat}, {lng}]);
                    }}
                }})()
                "#
            ))
            .await;
        });
    });

    #[allow(clippy::arc_with_non_send_sync)]
    use_effect(move || {
        if *viewport_poll_started.read() || !*map_initialized.read() {
            return;
        }
        viewport_poll_started.set(true);

        spawn(async move {
            let client = match nostr_client::get_client() {
                Some(c) => c,
                None => {
                    log::warn!("Places: client not available");
                    return;
                }
            };

            let mut current_sub_prefix: Option<String> = None;

            loop {
                crate::platform::timer::sleep(std::time::Duration::from_millis(1500)).await;
                if *unmounted.read() {
                    return;
                }

                let result: String = dioxus::document::eval(
                    "return window.__placesViewport ? JSON.stringify(window.__placesViewport) : 'null'"
                )
                .join()
                .await
                .unwrap_or_default();
                if result == "null" {
                    continue;
                }
                let Ok(info) = serde_json::from_str::<serde_json::Value>(&result) else {
                    continue;
                };
                let lat = info["lat"].as_f64().unwrap_or(0.0);
                let lng = info["lng"].as_f64().unwrap_or(0.0);
                let radius = info["radius_km"].as_f64().unwrap_or(0.0);
                let zoom = info["zoom"].as_f64().unwrap_or(8.0);
                if lat == 0.0 && lng == 0.0 || radius < 1.0 {
                    continue;
                }
                *places_store::VIEWPORT.write() = Some((lat, lng, radius));
                *places_store::VIEWPORT_ZOOM.write() = Some(zoom);

                // --- Nostr geohash-driven fetch (non-blocking) ---
                let precisions = places::geohash_precisions_for_zoom(zoom);
                for precision in &precisions {
                    let prefix = places::geohash_prefix(lat, lng, *precision);
                    if places_store::is_geohash_fetched(&prefix) {
                        continue;
                    }
                    places_store::mark_geohash_fetched(&prefix);
                    log::info!("Places: spawning fetch for geohash '{}' (precision {})", prefix, precision);
                    spawn(async move {
                        match places::fetch_places_for_geohash(&prefix).await {
                            Ok(places_list) => {
                                for place in places_list {
                                    places_store::merge_place(place);
                                }
                            }
                            Err(e) => {
                                log::warn!("Places: geohash fetch '{}' failed: {}", prefix, e);
                            }
                        }
                    });
                }

                // --- Subscription follows finest geohash prefix (non-blocking) ---
                let finest_prefix = places::geohash_prefix(lat, lng, *precisions.last().unwrap_or(&2));
                if current_sub_prefix.as_ref() != Some(&finest_prefix) {
                    current_sub_prefix = Some(finest_prefix.clone());

                    let old_state = places_sub_state.write().take();
                    let sub_client = client.clone();
                    let mut sub_state = places_sub_state;

                    spawn(async move {
                        if let Some(state) = old_state {
                            match Arc::try_unwrap(state) {
                                Ok(s) => {
                                    if let Some(handle) = s.handle {
                                        handle.unregister().await;
                                    } else {
                                        let _ = s.client.unsubscribe(&s.sub_id).await;
                                    }
                                }
                                Err(arc) => {
                                    let _ = arc.client.unsubscribe(&arc.sub_id).await;
                                }
                            }
                        }

                        let g_tag = SingleLetterTag::lowercase(Alphabet::G);
                        let filter = Filter::new()
                            .kind(Kind::Custom(37515))
                            .custom_tag(g_tag, &finest_prefix)
                            .limit(5000);
                        let auto_close = SubscribeAutoCloseOptions::default()
                            .exit_policy(ReqExitPolicy::WaitDurationAfterEOSE(
                                std::time::Duration::from_secs(600),
                            ));

                        match sub_client.subscribe(filter, Some(auto_close)).await {
                            Ok(output) => {
                                let sub_id = output.val;
                                log::info!(
                                    "Places: subscribed to geohash '{}' ({:?})",
                                    finest_prefix,
                                    sub_id
                                );

                                let on_event: OnPlaceEvent = Arc::new(Mutex::new(Box::new(
                                    |event: &nostr::Event| {
                                        match places::parse_place(event) {
                                            Some(place) => {
                                                log::info!(
                                                    "Places: streaming '{}' from {}",
                                                    place.name,
                                                    event.pubkey.to_hex().chars().take(8).collect::<String>()
                                                );
                                                places_store::merge_place(place);
                                            }
                                            None => {
                                                log::debug!(
                                                    "Places: parse failed for event {} (content len={})",
                                                    event.id.to_hex().chars().take(8).collect::<String>(),
                                                    event.content.len()
                                                );
                                            }
                                        }
                                    },
                                )));

                                let handle =
                                    DispatcherHandle::create(sub_id.clone()).map(|(handle, rx)| {
                                        spawn_places_listener(rx, on_event.clone());
                                        handle
                                    });

                                if handle.is_none() {
                                    spawn_places_fallback_listener(
                                        sub_client.clone(),
                                        sub_id.clone(),
                                        on_event.clone(),
                                    );
                                }

                                sub_state.set(Some(Arc::new(PlacesSubState {
                                    sub_id,
                                    client: sub_client,
                                    handle,
                                })));
                            }
                            Err(e) => {
                                log::error!("Places: subscribe failed: {}", e);
                            }
                        }
                    });
                }

                // --- BTCMap viewport fetch (non-blocking) ---
                if *places_store::SHOW_BTCMAP.read() {
                    let last = *places_store::LAST_BTCMAP_FETCH.read();
                    let needs_fetch = match last {
                        None => true,
                        Some(last_vp) => {
                            places_store::viewport_needs_refetch((lat, lng, radius), last_vp)
                        }
                    };
                    if needs_fetch {
                        *places_store::LAST_BTCMAP_FETCH.write() = Some((lat, lng, radius));
                        *places_store::BTCMAP_LOADING.write() = true;
                        spawn(async move {
                            match places::fetch_btcmap_places_in_viewport(lat, lng, radius).await {
                                Ok(btcmap_list) => {
                                    log::info!(
                                        "BTCMap: {} places within {:.1}km of ({:.4},{:.4})",
                                        btcmap_list.len(),
                                        radius,
                                        lat,
                                        lng
                                    );
                                    *places_store::BTCMAP_PLACES.write() = btcmap_list;
                                    places_store::cross_ref_places_with_btcmap();
                                }
                                Err(e) => {
                                    log::warn!("BTCMap fetch failed: {}", e);
                                }
                            }
                            *places_store::BTCMAP_LOADING.write() = false;
                        });
                    }
                }
            }
        });
    });

    use_effect(move || {
        if *click_poll_started.read() || !*map_initialized.read() {
            return;
        }
        click_poll_started.set(true);

        spawn(async move {
            loop {
                crate::platform::timer::sleep(std::time::Duration::from_millis(300)).await;
                if *unmounted.read() {
                    return;
                }
                let click_result: String = dioxus::document::eval(
                    "var c = window.__placesClickCoords; window.__placesClickCoords = null; return c ? JSON.stringify(c) : 'null'"
                )
                .join()
                .await
                .unwrap_or_default();
                if click_result != "null" {
                    if let Ok(click) = serde_json::from_str::<serde_json::Value>(&click_result) {
                        if let (Some(clat), Some(clng)) = (click["lat"].as_f64(), click["lng"].as_f64()) {
                            *places_store::PENDING_PLACE_COORDS.write() = Some((clat, clng));
                        }
                    }
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
                if result != "null" {
                    if let Ok(info) = serde_json::from_str::<serde_json::Value>(&result) {
                        let dir_info = places_store::DirectionsInfo {
                            distance_km: info["distance_km"]
                                .as_str()
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0.0),
                            duration_min: info["duration_min"]
                                .as_str()
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0.0),
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
        let places_data = places_store::PLACES.read().clone();
        let btcmap_data = places_store::BTCMAP_PLACES.read().clone();
        let show_btcmap = *places_store::SHOW_BTCMAP.read();
        let viewport = *places_store::VIEWPORT.read();

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        places_data.len().hash(&mut hasher);
        btcmap_data.len().hash(&mut hasher);
        show_btcmap.hash(&mut hasher);
        if let Some((lat, lng, rad)) = viewport {
            lat.to_bits().hash(&mut hasher);
            lng.to_bits().hash(&mut hasher);
            rad.to_bits().hash(&mut hasher);
        }
        let current_hash = hasher.finish();

        if *last_marker_hash.read() == Some(current_hash) {
            return;
        }
        last_marker_hash.set(Some(current_hash));

        let id = container_id.read().clone();
        let id_json = serde_json::to_string(&id).unwrap_or_default();

        if places_data.is_empty() && btcmap_data.is_empty() {
            return;
        }

        let nostr_filtered: Vec<_> = match viewport {
            Some((vlat, vlng, vrad)) => places_data
                .into_iter()
                .filter(|p| {
                    places::haversine_km(vlat, vlng, p.coordinates[1], p.coordinates[0]) <= vrad
                })
                .collect(),
            None => places_data,
        };

        if nostr_filtered.is_empty() && btcmap_data.is_empty() {
            return;
        }

        spawn(async move {
            let clear_js = format!(
                r#"
                (() => {{
                    const maps = window.leafletMaps || new Map();
                    const map = maps.get({id_json});
                    if (!map) return;
                    map.eachLayer(layer => {{
                        if (layer instanceof L.Marker && layer !== window.__placesUserDot) map.removeLayer(layer);
                        if (layer instanceof L.Polyline) map.removeLayer(layer);
                    }});
                }})()
                "#
            );
            let _ = dioxus::document::eval(&clear_js).await;

            let mut markers_json = String::from("[");

            for (i, place) in nostr_filtered.iter().enumerate() {
                if i > 0 {
                    markers_json.push(',');
                }
                let has_btcmap = place.btcmap_match.is_some();
                let marker_type = if has_btcmap {
                    "nostr_btcmap"
                } else {
                    "nostr"
                };
                let name = place
                    .name
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"");
                let description = place
                    .description
                    .as_deref()
                    .unwrap_or("")
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"");
                let amenity = place
                    .amenity
                    .as_deref()
                    .unwrap_or("")
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"");
                let phone = place
                    .phone
                    .as_deref()
                    .unwrap_or("")
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"");
                let website = place
                    .website
                    .as_deref()
                    .unwrap_or("")
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"");
                let hours = place
                    .opening_hours
                    .as_deref()
                    .unwrap_or("")
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"");
                let street = place
                    .address
                    .as_ref()
                    .and_then(|a| a.street.as_deref())
                    .unwrap_or("")
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"");
                let city = place
                    .address
                    .as_ref()
                    .and_then(|a| a.city.as_deref())
                    .unwrap_or("")
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"");
                let state = place
                    .address
                    .as_ref()
                    .and_then(|a| a.state.as_deref())
                    .unwrap_or("")
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"");
                let pubkey_short = place.pubkey.chars().take(12).collect::<String>();
                let place_naddr = PublicKey::from_hex(&place.pubkey)
                    .ok()
                    .map(|pk| {
                        Coordinate::new(Kind::Custom(37515), pk)
                            .identifier(&place.d_tag)
                            .to_bech32()
                            .unwrap_or_default()
                    })
                    .unwrap_or_default()
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"");
                markers_json.push_str(&format!(
                    r#"{{"lat":{},"lng":{},"type":"{}","name":"{}","desc":"{}","amenity":"{}","idx":{},"phone":"{}","website":"{}","hours":"{}","street":"{}","city":"{}","state":"{}","btcmap":{},"pubkey":"{}","naddr":"{}"}}"#,
                    place.coordinates[1],
                    place.coordinates[0],
                    marker_type,
                    name,
                    description,
                    amenity,
                    i,
                    phone,
                    website,
                    hours,
                    street,
                    city,
                    state,
                    has_btcmap,
                    pubkey_short,
                    place_naddr,
                ));
            }

            if show_btcmap {
                for bp in &btcmap_data {
                    markers_json.push(',');
                    let name = bp
                        .name
                        .as_deref()
                        .unwrap_or("Bitcoin Location")
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"");
                    let address = bp
                        .address
                        .as_deref()
                        .unwrap_or("")
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"");
                    let phone = bp
                        .phone
                        .as_deref()
                        .unwrap_or("")
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"");
                    let website = bp
                        .website
                        .as_deref()
                        .unwrap_or("")
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"");
                    let hours = bp
                        .opening_hours
                        .as_deref()
                        .unwrap_or("")
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"");
                    markers_json.push_str(&format!(
                        r#"{{"lat":{},"lng":{},"type":"btcmap","name":"{}","btcmap_id":{},"address":"{}","phone":"{}","website":"{}","hours":"{}"}}"#,
                        bp.lat, bp.lon, name, bp.id, address, phone, website, hours,
                    ));
                }
            }

            markers_json.push(']');

            let add_js = build_markers_js(&id_json, &markers_json);
            let _ = dioxus::document::eval(&add_js).await;
        });
    });

    rsx! {
        div { class: "fixed inset-0 bg-[#1a1a2e] z-50",
            div {
                id: "{container_id}",
                style: "position: absolute; inset: 0; z-index: 1;",
            }

            Link {
                to: crate::routes::Route::PlacesHome {},
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

            div { class: "fixed top-4 left-1/2 -translate-x-1/2 z-[60]",
                div { class: "rounded-full border border-white/15 bg-black/60 px-4 py-2 backdrop-blur-md",
                    span { class: "text-sm font-semibold text-white", "Places" }
                }
            }

            div { class: "fixed bottom-6 left-4 z-[60] flex flex-col gap-1",
                if *places_store::SHOW_BTCMAP.read() {
                    div { class: "flex items-center gap-2 rounded-lg bg-black/60 px-3 py-1.5 backdrop-blur-md",
                        div { class: "w-3 h-3 rounded-full bg-purple-500" }
                        span { class: "text-xs text-white/80", "Nostr Places" }
                    }
                    div { class: "flex items-center gap-2 rounded-lg bg-black/60 px-3 py-1.5 backdrop-blur-md",
                        div { class: "w-3 h-3 rounded-full bg-orange-500" }
                        span { class: "text-xs text-white/80", "Bitcoin Locations" }
                    }
                }
            }

            div { class: "fixed right-4 top-1/2 -translate-y-1/2 z-[60] flex flex-col gap-2",
                button {
                    class: if *places_store::SHOW_BTCMAP.read() {
                        "flex items-center justify-center w-10 h-10 rounded-full bg-orange-500 text-white hover:opacity-80 transition"
                    } else {
                        "flex items-center justify-center w-10 h-10 rounded-full bg-black/60 text-white/60 hover:text-white hover:bg-black/80 transition backdrop-blur-md"
                    },
                    onclick: move |_| {
                        let mut show = places_store::SHOW_BTCMAP.write();
                        *show = !*show;
                    },
                    span { class: "text-sm font-bold", "₿" }
                }

                button {
                    class: if places_store::USER_LOCATION.read().is_some() {
                        "flex items-center justify-center w-10 h-10 rounded-full bg-blue-500 text-white hover:opacity-80 transition"
                    } else {
                        "flex items-center justify-center w-10 h-10 rounded-full bg-black/60 text-white/80 hover:text-white hover:bg-black/80 transition backdrop-blur-md"
                    },
                    onclick: move |_| {
                        spawn(async move {
                            *places_store::LOC_LOADING.write() = true;
                            match crate::platform::geolocation::get_current_position().await {
                                Ok((lat, lon)) => {
                                    *places_store::USER_LOCATION.write() = Some((lat, lon));
                                    *places_store::LOC_LOADING.write() = false;
                                }
                                Err(e) => {
                                    log::warn!("Geolocation failed: {}", e);
                                    *places_store::LOC_LOADING.write() = false;
                                }
                            }
                        });
                    },
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
                            d: "M15 10.5a3 3 0 11-6 0 3 3 0 016 0z",
                        }
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M19.5 10.5c0 7.142-7.5 11.25-7.5 11.25S4.5 17.642 4.5 10.5a7.5 7.5 0 1115 0z",
                        }
                    }
                }

                button {
                    class: if matches!(*places_store::MAP_MODE.read(), MapMode::Add) {
                        "flex items-center justify-center w-10 h-10 rounded-full bg-purple-600 text-white hover:opacity-80 transition"
                    } else {
                        "flex items-center justify-center w-10 h-10 rounded-full bg-black/60 text-white/80 hover:text-white hover:bg-black/80 transition backdrop-blur-md"
                    },
                    onclick: move |_| {
                        let mode = places_store::MAP_MODE.read().clone();
                        let new_mode = match mode {
                            MapMode::Add => MapMode::View,
                            _ => MapMode::Add,
                        };
                        *places_store::MAP_MODE.write() = new_mode.clone();
                        let js = match new_mode {
                            MapMode::Add => "window.__placesAddMode = true",
                            _ => "window.__placesAddMode = false; if(window.__placesTempMarker){window.__placesTempMarker.remove();window.__placesTempMarker=null;}",
                        };
                        let _ = dioxus::document::eval(js);
                    },
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
                            d: "M12 4.5v15m7.5-7.5h-15",
                        }
                    }
                }
            }

            if *places_store::LOC_LOADING.read() {
                div { class: "fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 z-[70] flex flex-col items-center gap-3 rounded-2xl border border-white/10 bg-black/80 px-8 py-6 backdrop-blur-md",
                    div { class: "w-8 h-8 border-2 border-purple-400/30 border-t-purple-400 rounded-full animate-spin" }
                    p { class: "text-sm text-white/80", "Finding you..." }
                }
            }

            if !*places_store::LOC_LOADING.read()
                && places_store::USER_LOCATION.read().is_none()
                && *map_initialized.read()
            {
                div { class: "fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 z-[70] flex flex-col items-center gap-3 rounded-2xl border border-white/10 bg-black/80 px-8 py-6 backdrop-blur-md",
                    p { class: "text-sm text-white/80 text-center",
                        "Location unavailable"
                    }
                    p { class: "text-xs text-white/50 text-center",
                        "Tap the location button to try again"
                    }
                }
            }

            if *places_store::BTCMAP_LOADING.read() {
                div { class: "fixed top-16 right-4 z-[60] rounded-lg bg-black/60 px-3 py-2 backdrop-blur-md",
                    div { class: "flex items-center gap-2",
                        div { class: "w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" }
                        span { class: "text-xs text-white/80", "Loading BTCMap..." }
                    }
                }
            }

            if matches!(*places_store::MAP_MODE.read(), MapMode::Add) {
                div { class: "fixed bottom-6 left-1/2 -translate-x-1/2 z-[60] rounded-full border border-purple-500/30 bg-purple-900/80 px-6 py-3 backdrop-blur-md",
                    p { class: "text-sm font-medium text-purple-100", "Tap the map to place a new location" }
                }
            }

            if let Some((lat, lng)) = *places_store::PENDING_PLACE_COORDS.read() {
                PlaceCreateModal {
                    lat,
                    lng,
                    on_close: move |_| {
                        *places_store::PENDING_PLACE_COORDS.write() = None;
                    },
                    on_created: move |_| {
                        *places_store::PENDING_PLACE_COORDS.write() = None;
                        *places_store::MAP_MODE.write() = MapMode::View;
                        let _ = dioxus::document::eval(
                            "window.__placesAddMode = false; if(window.__placesTempMarker){window.__placesTempMarker.remove();window.__placesTempMarker=null;}"
                        );
                    },
                }
            }

            if let Some(dir) = places_store::DIRECTIONS.read().as_ref() {
                {
                    let dir_clone = dir.clone();
                    rsx! {
                        div { class: "fixed bottom-6 left-4 right-4 z-[60] rounded-xl border border-purple-500/30 bg-black/80 px-4 py-3 backdrop-blur-md",
                            div { class: "flex items-center justify-between",
                                div { class: "flex-1 min-w-0",
                                    p { class: "text-sm font-semibold text-white truncate", "{dir_clone.dest_name}" }
                                    div { class: "flex items-center gap-3 mt-1",
                                        span { class: "text-xs text-purple-300",
                                            "{dir_clone.distance_km} km"
                                        }
                                        span { class: "text-xs text-white/50", "·" }
                                        span { class: "text-xs text-purple-300",
                                            "{dir_clone.duration_min} min"
                                        }
                                    }
                                }
                                button {
                                    class: "ml-3 flex items-center justify-center w-8 h-8 rounded-full bg-white/10 hover:bg-white/20 transition",
                                    onclick: move |_| {
                                        *places_store::DIRECTIONS.write() = None;
                                        spawn(async move {
                                            let _ = dioxus::document::eval("window.__placesClearRoute()").await;
                                        });
                                    },
                                    svg {
                                        class: "w-4 h-4 text-white",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M6 18L18 6M6 6l12 12",
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

const AMENITY_OPTIONS: &[(&str, &str)] = &[
    ("", "None"),
    ("restaurant", "Restaurant"),
    ("cafe", "Cafe"),
    ("bar", "Bar"),
    ("pub", "Pub"),
    ("fast_food", "Fast Food"),
    ("bakery", "Bakery"),
    ("hotel", "Hotel"),
    ("hostel", "Hostel"),
    ("shop", "Shop"),
    ("supermarket", "Supermarket"),
    ("pharmacy", "Pharmacy"),
    ("hospital", "Hospital"),
    ("bank", "Bank"),
    ("atm", "ATM"),
    ("library", "Library"),
    ("museum", "Museum"),
    ("cinema", "Cinema"),
    ("theatre", "Theatre"),
    ("park", "Park"),
    ("gym", "Gym"),
    ("school", "School"),
    ("university", "University"),
    ("fuel", "Gas Station"),
    ("parking", "Parking"),
    ("toilets", "Toilets"),
    ("post_office", "Post Office"),
    ("police", "Police"),
    ("fire_station", "Fire Station"),
    ("place_of_worship", "Place of Worship"),
    ("marketplace", "Marketplace"),
    ("coworking_space", "Coworking Space"),
    ("nightclub", "Nightclub"),
    ("campsite", "Campsite"),
    ("tourism", "Tourist Attraction"),
    ("office", "Office"),
];

#[component]
fn PlaceCreateModal(
    lat: f64,
    lng: f64,
    on_close: EventHandler,
    on_created: EventHandler,
) -> Element {
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut amenity = use_signal(String::new);
    let mut phone = use_signal(String::new);
    let mut website = use_signal(String::new);
    let mut street = use_signal(String::new);
    let mut city = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    let can_publish = use_memo(move || {
        !name.read().trim().is_empty() && !*loading.read()
    });

    let handle_submit = move |_| {
        if *loading.peek() {
            return;
        }
        let name_val = name.read().clone();
        if name_val.trim().is_empty() {
            error_msg.set(Some("Name is required".to_string()));
            return;
        }
        loading.set(true);
        error_msg.set(None);

        let desc_val = description.read().clone();
        let amenity_val = amenity.read().clone();
        let phone_val = phone.read().clone();
        let website_val = website.read().clone();
        let street_val = street.read().clone();
        let city_val = city.read().clone();
        let lat = lat;
        let lng = lng;

        spawn(async move {
            let slug = name_val
                .to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect::<String>()
                .trim_matches('-')
                .to_string();
            let geo = places::geohash_prefix(lat, lng, 6);
            let d_tag = format!("{}-{}", slug, geo);

            let address = if street_val.is_empty() && city_val.is_empty() {
                None
            } else {
                Some(places::PlaceAddress {
                    street: if street_val.is_empty() { None } else { Some(street_val) },
                    city: if city_val.is_empty() { None } else { Some(city_val) },
                    state: None,
                    postcode: None,
                    country: None,
                })
            };

            let builder = places::build_place_event_builder(
                &d_tag,
                &name_val,
                if desc_val.is_empty() { None } else { Some(&desc_val) },
                if amenity_val.is_empty() { None } else { Some(&amenity_val) },
                if phone_val.is_empty() { None } else { Some(&phone_val) },
                if website_val.is_empty() { None } else { Some(&website_val) },
                None,
                None,
                None,
                address.as_ref(),
                lat,
                lng,
            );

            match crate::stores::publish_queue::signing::sign_event_builder(builder).await {
                Ok(event) => {
                    if let Some(pl) = places::parse_place(&event) {
                        places_store::merge_place(pl);
                    }
                    crate::stores::publish_queue::enqueue(
                        event,
                        crate::stores::publish_queue::types::QueueEventType::Other("place".to_string()),
                        None,
                        std::collections::HashMap::new(),
                    ).await;
                    loading.set(false);
                    on_created.call(());
                }
                Err(e) => {
                    error_msg.set(Some(format!("Failed to sign: {}", e)));
                    loading.set(false);
                }
            }
        });
    };

    rsx! {
        div {
            class: "fixed inset-0 z-[80] bg-black/50 backdrop-blur-sm flex items-end lg:items-center justify-center",
            onclick: move |_| {
                on_close.call(());
            },

            div {
                class: "w-full max-w-lg bg-background border border-border rounded-t-2xl lg:rounded-xl shadow-xl max-h-[85vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),

                div { class: "flex items-center justify-between p-4 border-b border-border",
                    h2 { class: "text-lg font-semibold", "New Place" }
                    button {
                        class: "p-1 rounded-lg hover:bg-accent transition",
                        onclick: move |_| {
                            on_close.call(());
                        },
                        svg {
                            class: "w-5 h-5 text-muted-foreground",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M6 18L18 6M6 6l12 12",
                            }
                        }
                    }
                }

                div { class: "p-4 space-y-4",
                    if let Some(err) = error_msg.read().as_ref() {
                        div { class: "p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-600 text-sm",
                            "{err}"
                        }
                    }

                    div { class: "flex items-center gap-2 text-sm text-muted-foreground bg-muted rounded-lg px-3 py-2",
                        svg {
                            class: "w-4 h-4 shrink-0",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
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
                        span { "{lat:.4}, {lng:.4}" }
                    }

                    div { class: "space-y-1",
                        label { class: "text-sm font-medium", "Name" }
                        input {
                            r#type: "text",
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                            placeholder: "e.g. Satoshi's Coffee Shop",
                            value: "{name}",
                            oninput: move |e| name.set(e.value()),
                        }
                    }

                    div { class: "space-y-1",
                        label { class: "text-sm font-medium", "Type" }
                        select {
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                            value: "{amenity}",
                            onchange: move |e| amenity.set(e.value()),
                            for (val, label) in AMENITY_OPTIONS {
                                option { value: "{val}", "{label}" }
                            }
                        }
                    }

                    div { class: "space-y-1",
                        label { class: "text-sm font-medium", "Description" }
                        textarea {
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                            rows: "2",
                            placeholder: "What makes this place special?",
                            value: "{description}",
                            oninput: move |e| description.set(e.value()),
                        }
                    }

                    div { class: "grid grid-cols-2 gap-3",
                        div { class: "space-y-1",
                            label { class: "text-sm font-medium", "Phone" }
                            input {
                                r#type: "tel",
                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                placeholder: "+1-555-1234",
                                value: "{phone}",
                                oninput: move |e| phone.set(e.value()),
                            }
                        }
                        div { class: "space-y-1",
                            label { class: "text-sm font-medium", "Website" }
                            input {
                                r#type: "url",
                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                placeholder: "https://...",
                                value: "{website}",
                                oninput: move |e| website.set(e.value()),
                            }
                        }
                    }

                    div { class: "grid grid-cols-2 gap-3",
                        div { class: "space-y-1",
                            label { class: "text-sm font-medium", "Street" }
                            input {
                                r#type: "text",
                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                placeholder: "123 Main St",
                                value: "{street}",
                                oninput: move |e| street.set(e.value()),
                            }
                        }
                        div { class: "space-y-1",
                            label { class: "text-sm font-medium", "City" }
                            input {
                                r#type: "text",
                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                placeholder: "City",
                                value: "{city}",
                                oninput: move |e| city.set(e.value()),
                            }
                        }
                    }
                }

                div { class: "flex items-center justify-end gap-3 p-4 border-t border-border",
                    button {
                        class: "px-4 py-2 rounded-lg border border-border hover:bg-accent transition",
                        onclick: move |_| {
                            on_close.call(());
                        },
                        "Cancel"
                    }
                    button {
                        class: "px-6 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 disabled:opacity-50 transition",
                        disabled: !*can_publish.read(),
                        onclick: handle_submit,
                        if *loading.read() { "Publishing..." } else { "Publish" }
                    }
                }
            }
        }
    }
}
