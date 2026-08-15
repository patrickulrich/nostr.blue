//! Shared Leaflet.js loading utilities used by both Places and Deflock map containers.

/// JavaScript that dynamically loads Leaflet CSS + JS from unpkg CDN.
/// Idempotent — safe to call multiple times. Uses a promise singleton.
pub const LEAFLET_LOAD_JS: &str = r#"
return await new Promise((resolve, reject) => {
    if (window.L) { dioxus.send("ok"); return; }
    if (window.leafletLoadingPromise) {
        window.leafletLoadingPromise.then(() => dioxus.send("ok")).catch(e => dioxus.send("error:" + e));
        return;
    }
    window.leafletLoadingPromise = new Promise((res, rej) => {
        let cssLoaded = false, jsLoaded = false, settled = false;
        const done = () => { if (!settled && cssLoaded && jsLoaded) { settled = true; res(); } };
        const fail = (msg) => { if (!settled) { settled = true; window.leafletLoadingPromise = null; rej(msg); } };
        const link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.css';
        link.onload = () => { cssLoaded = true; done(); };
        link.onerror = () => fail('Leaflet CSS failed');
        document.head.appendChild(link);
        const script = document.createElement('script');
        script.src = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.js';
        script.onload = () => { jsLoaded = true; done(); };
        script.onerror = () => fail('Leaflet JS failed');
        document.head.appendChild(script);
    });
    window.leafletLoadingPromise.then(() => dioxus.send("ok")).catch(e => dioxus.send("error:" + e));
});
"#;

/// JavaScript that dynamically loads Leaflet.markercluster CSS + JS from unpkg CDN.
/// Requires Leaflet itself to be loaded first. Idempotent via `window.L.MarkerClusterGroup` check.
pub const MARKERCLUSTER_LOAD_JS: &str = r#"
return await new Promise((resolve, reject) => {
    if (!window.L) { dioxus.send("error:Leaflet not loaded"); return; }
    if (window.L.MarkerClusterGroup) { dioxus.send("ok"); return; }
    if (window.markerClusterLoadingPromise) {
        window.markerClusterLoadingPromise.then(() => dioxus.send("ok")).catch(e => dioxus.send("error:" + e));
        return;
    }
    window.markerClusterLoadingPromise = new Promise((res, rej) => {
        let css1 = false, css2 = false, jsLoaded = false, settled = false;
        const done = () => { if (!settled && css1 && css2 && jsLoaded) { settled = true; res(); } };
        const fail = (msg) => { if (!settled) { settled = true; window.markerClusterLoadingPromise = null; rej(msg); } };
        const link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = 'https://unpkg.com/leaflet.markercluster@1.5.3/dist/MarkerCluster.css';
        link.onload = () => { css1 = true; done(); };
        link.onerror = () => fail('MarkerCluster CSS failed');
        document.head.appendChild(link);
        const link2 = document.createElement('link');
        link2.rel = 'stylesheet';
        link2.href = 'https://unpkg.com/leaflet.markercluster@1.5.3/dist/MarkerCluster.Default.css';
        link2.onload = () => { css2 = true; done(); };
        link2.onerror = () => fail('MarkerCluster Default CSS failed');
        document.head.appendChild(link2);
        const script = document.createElement('script');
        script.src = 'https://unpkg.com/leaflet.markercluster@1.5.3/dist/leaflet.markercluster.js';
        script.onload = () => { jsLoaded = true; done(); };
        script.onerror = () => fail('MarkerCluster JS failed');
        document.head.appendChild(script);
    });
    window.markerClusterLoadingPromise.then(() => dioxus.send("ok")).catch(e => dioxus.send("error:" + e));
});
"#;

/// CSS that styles Leaflet popups with the nostr.blue dark theme.
/// Injects once per page via the `window.__placesPopupStyleAdded` flag.
///
/// NOTE: this constant is passed to `format!()` as a **named argument**
/// (`popup_style = POPUP_STYLE_JS`) in both map containers. `format!()` only
/// unescapes `{{`/`}}` in the *template*, never in substituted *values*, so
/// this text must use SINGLE braces. Do not inline it into a brace-escaped
/// template literal.
pub const POPUP_STYLE_JS: &str = r#"
if (!window.__placesPopupStyleAdded) {
    const style = document.createElement('style');
    style.textContent = '.places-popup .leaflet-popup-content-wrapper{background:rgba(20,20,20,0.92);backdrop-filter:blur(10px);border-radius:12px;border:1px solid rgba(255,255,255,0.1);box-shadow:0 8px 32px rgba(0,0,0,0.5);color:#e5e5e5;padding:0;} .places-popup .leaflet-popup-content{margin:12px 16px;line-height:1.4;} .places-popup .leaflet-popup-tip{background:rgba(20,20,20,0.92);} .places-popup .leaflet-popup-close-button{color:#737373 !important;font-size:18px !important;top:6px !important;right:8px !important;} .places-popup .leaflet-popup-close-button:hover{color:#fff !important;}';
    document.head.appendChild(style);
    window.__placesPopupStyleAdded = true;
}
"#;

/// Global, stateless OSRM directions helpers shared by the Places and Deflock
/// maps. Both are defined guarded and identical in every container, and are
/// safe to define from whichever map initializes first because they resolve
/// ALL state (the Leaflet map instance) at call time:
///
/// - `__requestDirectionsFor(mapId, toLat, toLng, toName, color)` looks the
///   map up in `window.leafletMaps`, so a popup's Directions button always
///   routes on the map it belongs to — even after other maps have been
///   initialized and torn down over the app's lifetime.
/// - The route polyline is stored on the map instance itself
///   (`map.__placesRouteLayer`), never a shared `window` global, so routes
///   on different maps cannot clobber each other.
/// - `__placesRouteInfo` remains a shared global intentionally: it is plain
///   display data that each container's poll loop consumes and nulls.
///
/// NOTE: same brace rule as `POPUP_STYLE_JS` — substituted as a named
/// `format!()` argument (`directions_helpers = DIRECTIONS_HELPERS_JS`), so
/// SINGLE braces. Do not inline into a brace-escaped template literal.
pub const DIRECTIONS_HELPERS_JS: &str = r#"
if (!window.__requestDirectionsFor) {
    window.__requestDirectionsFor = async function(mapId, toLat, toLng, toName, color) {
        const map = (window.leafletMaps || new Map()).get(mapId);
        if (!map) { return; }
        const ul = window.__placesUserLocation;
        if (!ul) { alert('Location unavailable — enable location and try again'); return; }
        try {
            const url = 'https://router.project-osrm.org/route/v1/driving/'+ul.lng+','+ul.lat+';'+toLng+','+toLat+'?overview=full&geometries=geojson';
            const resp = await fetch(url);
            const data = await resp.json();
            if (!data.routes || !data.routes.length) { alert('No route found'); return; }
            const route = data.routes[0];
            const coords = route.geometry.coordinates.map(c => [c[1], c[0]]);
            if (map.__placesRouteLayer) { map.removeLayer(map.__placesRouteLayer); }
            map.__placesRouteLayer = L.polyline(coords, {
                color: color || '#7c3aed', weight: 5, opacity: 0.8, dashArray: '10, 8'
            }).addTo(map);
            map.fitBounds(map.__placesRouteLayer.getBounds(), { padding: [60, 60] });
            window.__placesRouteInfo = {
                distance_km: (route.distance / 1000).toFixed(1),
                duration_min: (route.duration / 60).toFixed(0),
                dest_name: toName,
                dest_lat: toLat,
                dest_lng: toLng
            };
        } catch(e) {
            console.error('OSRM error:', e);
            alert('Route fetch failed');
        }
    };
}
if (!window.__clearRouteFor) {
    window.__clearRouteFor = function(mapId) {
        const map = (window.leafletMaps || new Map()).get(mapId);
        if (map && map.__placesRouteLayer) {
            map.removeLayer(map.__placesRouteLayer);
            map.__placesRouteLayer = null;
        }
        window.__placesRouteInfo = null;
    };
}
"#;

#[cfg(test)]
mod tests {
    use super::{DIRECTIONS_HELPERS_JS, POPUP_STYLE_JS};

    /// Both constants are substituted as named `format!()` arguments, whose
    /// values are NOT brace-unescaped — doubled braces would leak literal
    /// `{{`/`}}` into the evaluated JS (breaking the popup CSS / parsing).
    #[test]
    fn shared_js_constants_have_no_doubled_braces() {
        assert!(!POPUP_STYLE_JS.contains("{{"));
        assert!(!POPUP_STYLE_JS.contains("}}"));
        assert!(!DIRECTIONS_HELPERS_JS.contains("{{"));
        assert!(!DIRECTIONS_HELPERS_JS.contains("}}"));
    }
}
