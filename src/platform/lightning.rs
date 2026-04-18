pub async fn open_lightning_invoice(invoice: &str) -> Result<(), String> {
    let invoice = invoice.trim();
    if invoice.is_empty() {
        return Err("Lightning invoice is empty".to_string());
    }

    #[cfg(feature = "web")]
    {
        open_lightning_invoice_web(invoice).await
    }

    #[cfg(feature = "desktop")]
    {
        let uri = format!("lightning:{invoice}");
        webbrowser::open(&uri)
            .map(|_| ())
            .map_err(|error| format!("Failed to open Lightning wallet: {error}"))
    }

    #[cfg(feature = "mobile_platform")]
    {
        let uri = format!("lightning:{invoice}");
        crate::platform::mobile::open_lightning_uri(&uri)
    }

    #[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile_platform")))]
    {
        Err("Lightning invoice opening is not supported on this platform".to_string())
    }
}

#[cfg(feature = "web")]
async fn open_lightning_invoice_web(invoice: &str) -> Result<(), String> {
    use js_sys::{Function, Promise, Reflect};
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or_else(|| "No window".to_string())?;
    let window_value = JsValue::from(window.clone());
    let webln = Reflect::get(&window_value, &JsValue::from_str("webln"))
        .map_err(|error| format!("Failed to inspect WebLN: {error:?}"))?;

    if !webln.is_undefined() && !webln.is_null() {
        let enable_result = async {
            let enable_fn = Reflect::get(&webln, &JsValue::from_str("enable"))
                .map_err(|error| format!("Failed to read WebLN enable(): {error:?}"))?
                .dyn_into::<Function>()
                .map_err(|_| "WebLN enable() is not callable".to_string())?;
            let enable_promise = enable_fn
                .call0(&webln)
                .map_err(|error| format!("WebLN enable() failed to start: {error:?}"))?
                .dyn_into::<Promise>()
                .map_err(|_| "WebLN enable() did not return a Promise".to_string())?;
            JsFuture::from(enable_promise)
                .await
                .map_err(|error| format!("WebLN enable() failed: {error:?}"))?;

            let send_payment_fn = Reflect::get(&webln, &JsValue::from_str("sendPayment"))
                .map_err(|error| format!("Failed to read WebLN sendPayment(): {error:?}"))?
                .dyn_into::<Function>()
                .map_err(|_| "WebLN sendPayment() is not callable".to_string())?;
            let send_payment_promise = send_payment_fn
                .call1(&webln, &JsValue::from_str(invoice))
                .map_err(|error| format!("WebLN sendPayment() failed to start: {error:?}"))?
                .dyn_into::<Promise>()
                .map_err(|_| "WebLN sendPayment() did not return a Promise".to_string())?;
            JsFuture::from(send_payment_promise)
                .await
                .map_err(|error| format!("WebLN sendPayment() failed: {error:?}"))?;

            Ok::<(), String>(())
        }
        .await;

        if enable_result.is_ok() {
            return Ok(());
        }
    }

    let uri = format!("lightning:{invoice}");
    window
        .open_with_url(&uri)
        .map_err(|error| format!("Failed to open Lightning wallet: {error:?}"))?;
    Ok(())
}
