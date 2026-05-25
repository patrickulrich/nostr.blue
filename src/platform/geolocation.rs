pub async fn get_current_position() -> Result<(f64, f64), String> {
    #[cfg(feature = "web")]
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
    #[cfg(not(feature = "web"))]
    {
        Err("Geolocation only available on web".to_string())
    }
}
