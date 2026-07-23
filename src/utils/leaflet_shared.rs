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
/// Note: brace-doubled for direct interpolation into Rust `format!()` strings.
pub const POPUP_STYLE_JS: &str = r#"
if (!window.__placesPopupStyleAdded) {{
    const style = document.createElement('style');
    style.textContent = '.places-popup .leaflet-popup-content-wrapper{{background:rgba(20,20,20,0.92);backdrop-filter:blur(10px);border-radius:12px;border:1px solid rgba(255,255,255,0.1);box-shadow:0 8px 32px rgba(0,0,0,0.5);color:#e5e5e5;padding:0;}} .places-popup .leaflet-popup-content{{margin:12px 16px;line-height:1.4;}} .places-popup .leaflet-popup-tip{{background:rgba(20,20,20,0.92);}} .places-popup .leaflet-popup-close-button{{color:#737373 !important;font-size:18px !important;top:6px !important;right:8px !important;}} .places-popup .leaflet-popup-close-button:hover{{color:#fff !important;}}';
    document.head.appendChild(style);
    window.__placesPopupStyleAdded = true;
}}
"#;
