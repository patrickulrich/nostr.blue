//! Cross-platform opener for payment URIs (NIP-A3 targets).
//!
//! Mirrors the `lightning.rs` dispatcher: web hands the URI to the browser
//! (custom schemes resolve to installed wallet handlers where registered),
//! desktop delegates to the OS default handler, and mobile launches an
//! Android intent with a handler pre-check so a missing wallet surfaces a
//! user-facing error instead of failing silently.
pub async fn open_payment_uri(uri: &str) -> Result<(), String> {
    let uri = uri.trim();
    if uri.is_empty() {
        return Err("Payment URI is empty".to_string());
    }

    #[cfg(feature = "web")]
    {
        let window = web_sys::window().ok_or_else(|| "No window".to_string())?;
        let opened = window
            .open_with_url_and_target_and_features(uri, "_blank", "noopener,noreferrer")
            .map_err(|e| format!("Failed to open payment URI: {e:?}"))?;
        if opened.is_none() {
            return Err("no_handler".to_string());
        }
        Ok(())
    }

    #[cfg(feature = "desktop")]
    {
        webbrowser::open(uri)
            .map(|_| ())
            .map_err(|error| format!("Failed to open payment URI: {error}"))
    }

    #[cfg(feature = "mobile_platform")]
    {
        crate::platform::mobile::open_uri(uri)
    }

    #[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile_platform")))]
    {
        let _ = uri;
        Err("Payment URI opening is not supported on this platform".to_string())
    }
}
