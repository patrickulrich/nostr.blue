pub async fn get_current_position() -> Result<(f64, f64), String> {
    // The `navigator.geolocation` JS API works on both web (WASM) and mobile
    // (Android WebView). On Android, wry 0.53.5's `RustWebView` enables
    // `setGeolocationEnabled(true)` and `RustWebChromeClient` implements
    // `onGeolocationPermissionsShowPrompt` to handle the runtime permission
    // flow — so all we need here is the JS interop + the manifest permissions.
    #[cfg(any(feature = "web", feature = "mobile_platform"))]
    {
        let mut eval = dioxus::document::eval(r#"
            return await new Promise((resolve) => {
                if (!navigator.geolocation) {
                    dioxus.send(JSON.stringify({error: "Geolocation not supported"}));
                    return;
                }
                navigator.geolocation.getCurrentPosition(
                    (pos) => dioxus.send(JSON.stringify({
                        lat: pos.coords.latitude,
                        lon: pos.coords.longitude
                    })),
                    (err) => dioxus.send(JSON.stringify({error: err.message}))
                );
            });
        "#);
        let result: String = eval.recv().await.map_err(|e| e.to_string())?;
        let val: serde_json::Value =
            serde_json::from_str(&result).map_err(|e| format!("Parse error: {}", e))?;
        if let Some(err) = val.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        let lat = val
            .get("lat")
            .and_then(|v| v.as_f64())
            .ok_or("Missing latitude")?;
        let lon = val
            .get("lon")
            .and_then(|v| v.as_f64())
            .ok_or("Missing longitude")?;
        Ok((lat, lon))
    }
    #[cfg(not(any(feature = "web", feature = "mobile_platform")))]
    {
        Err("Geolocation only available on web".to_string())
    }
}

#[cfg(any(feature = "web", feature = "mobile_platform"))]
pub fn start_watch_position(_callback_id: &str) {
    let _ = dioxus::document::eval(
        r#"
        if (!navigator.geolocation) return;
        if (window._geoWatchId) navigator.geolocation.clearWatch(window._geoWatchId);
        window._geoWatchId = navigator.geolocation.watchPosition(
            (pos) => {
                window._geoWatchPos = { lat: pos.coords.latitude, lon: pos.coords.longitude };
            },
            () => {}
        );
        "#
    );
}

#[cfg(any(feature = "web", feature = "mobile_platform"))]
pub async fn get_watched_position() -> Result<(f64, f64), String> {
    let eval = dioxus::document::eval(
        r#"
        if (window._geoWatchPos) {
            return JSON.stringify(window._geoWatchPos);
        }
        return JSON.stringify({error: "No position yet"});
        "#,
    );
    let result: String = eval.await.map_err(|e| e.to_string())?.to_string();
    let val: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| format!("Parse error: {}", e))?;
    if let Some(err) = val.get("error").and_then(|v| v.as_str()) {
        return Err(err.to_string());
    }
    let lat = val.get("lat").and_then(|v| v.as_f64()).ok_or("Missing lat")?;
    let lon = val.get("lon").and_then(|v| v.as_f64()).ok_or("Missing lon")?;
    Ok((lat, lon))
}

#[cfg(not(any(feature = "web", feature = "mobile_platform")))]
pub fn start_watch_position(_callback_id: &str) {}

#[cfg(not(any(feature = "web", feature = "mobile_platform")))]
pub async fn get_watched_position() -> Result<(f64, f64), String> {
    Err("Geolocation only available on web".to_string())
}
