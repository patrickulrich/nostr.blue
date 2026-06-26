/// Save text content to a file. On web, triggers a browser download.
/// On desktop, opens a save dialog. On mobile (Android), triggers Share Intent.

#[cfg(all(feature = "web", feature = "desktop"))]
compile_error!("Cannot enable both 'web' and 'desktop' features");

#[cfg(all(feature = "web", feature = "mobile_platform"))]
compile_error!("Cannot enable both 'web' and 'mobile' features");

#[cfg(all(feature = "desktop", feature = "mobile_platform"))]
compile_error!("Cannot enable both 'desktop' and 'mobile' features");

#[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile_platform")))]
compile_error!("Must enable exactly one of 'web', 'desktop', or 'mobile' feature");

pub fn save_file(filename: &str, content: &str, _mime_type: &str) -> Result<(), String> {
    #[cfg(feature = "web")]
    {
        crate::utils::download::download_blob(filename, content, _mime_type);
        Ok(())
    }
    #[cfg(feature = "desktop")]
    {
        let path = rfd::FileDialog::new()
            .set_file_name(filename)
            .save_file()
            .ok_or_else(|| "Save cancelled".to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())
    }
    #[cfg(feature = "mobile_platform")]
    {
        crate::platform::download_file(filename, content.as_bytes(), _mime_type)
            .map_err(|e| format!("Download failed: {}", e))
    }
}

/// Phase 5.4 (M16): save raw bytes to a file (for decrypted attachment
/// downloads). Same platform dispatch as `save_file` but takes `&[u8]`
/// instead of `&str` so binary content (images, PDFs) is handled correctly.
///
/// On web: creates a Blob via `js_sys`, triggers an object-URL download.
/// On desktop: opens a save dialog and writes bytes to disk.
/// On mobile: delegates to the platform's `download_file` JNI bridge.
#[allow(dead_code)]
pub fn save_bytes(filename: &str, content: &[u8], mime_type: &str) -> Result<(), String> {
    #[cfg(feature = "web")]
    {
        save_bytes_web(filename, content, mime_type)
    }
    #[cfg(feature = "desktop")]
    {
        let _ = mime_type;
        let path = rfd::FileDialog::new()
            .set_file_name(filename)
            .save_file()
            .ok_or_else(|| "Save cancelled".to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())
    }
    #[cfg(feature = "mobile_platform")]
    {
        crate::platform::download_file(filename, content, mime_type)
            .map_err(|e| format!("Download failed: {}", e))
    }
}

/// Web implementation: create a Blob from raw bytes and trigger download
/// via an object URL anchor click.
#[cfg(feature = "web")]
fn save_bytes_web(filename: &str, content: &[u8], _mime_type: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;

    // Create a Uint8Array view and wrap it in a Blob.
    let uint8 = js_sys::Uint8Array::from(content);
    let parts = js_sys::Array::new();
    parts.push(&uint8);
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)
        .map_err(|e| format!("Blob creation failed: {e:?}"))?;

    // Create an object URL for the blob.
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|e| format!("object URL failed: {e:?}"))?;

    // Create an invisible <a> element and click it to trigger download.
    let anchor = document
        .create_element("a")
        .map_err(|e| format!("create element failed: {e:?}"))?;
    anchor.set_attribute("href", &url).ok();
    anchor.set_attribute("download", filename).ok();
    anchor.set_attribute("style", "display:none;").ok();
    if let Some(body) = document.body() {
        body.append_child(&anchor).ok();
        // Use the generic Element::click (available on all Elements
        // via HTMLElement inheritance) instead of dyn_into which moves.
        if let Ok(el) = wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlElement>(anchor.clone()) {
            el.click();
        }
        let _ = body.remove_child(&anchor);
    }

    // Revoke the object URL to free memory.
    web_sys::Url::revoke_object_url(&url).ok();
    Ok(())
}
