//! QR code scanner component for invoice/payment input.
//!
//! Phase 4.3 (F5): provides a cross-platform QR scanner that accepts an
//! image file (from camera capture on mobile, file picker on desktop).
//! On web, the `capture="environment"` attribute opens the rear camera
//! directly on mobile browsers.
//!
//! Decoding is done in pure Rust via `rqrr`, which takes an
//! `image::GrayImage` and finds+decodes QR codes in it. No JS dependency
//! needed — works on WASM and native.

use dioxus::prelude::*;

/// Props for the QR scanner button.
#[derive(Props, Clone, PartialEq)]
pub struct QrScannerProps {
    /// Label for the scan button.
    #[props(default = "Scan QR".to_string())]
    pub label: String,
    /// Callback fired when a QR code is successfully decoded.
    pub on_scan: EventHandler<String>,
    /// Optional callback fired when scanning fails (e.g., no QR in image).
    pub on_error: Option<EventHandler<String>>,
}

/// A compact QR scanner button. Renders a file input that accepts images
/// (with camera capture on mobile). When a QR code is found in the image,
/// calls `on_scan` with the decoded text.
///
/// Usage:
/// ```rust,ignore
/// QrScanner {
///     label: "Scan".to_string(),
///     on_scan: move |decoded| {
///         invoice_input.set(decoded);
///     },
/// }
/// ```
#[component]
pub fn QrScanner(props: QrScannerProps) -> Element {
    let file_input_id = use_signal(|| {
        format!(
            "qr-input-{}",
            crate::platform::timestamp::now_millis() % 100_000
        )
    });
    let mut scanning = use_signal(|| false);

    rsx! {
        button {
            class: "px-3 py-2 border border-border rounded-lg text-sm hover:bg-accent transition flex items-center gap-1.5",
            disabled: *scanning.read(),
            onclick: move |_| {
                // Trigger the hidden file input.
                let id = file_input_id.read().clone();
                #[cfg(feature = "web")]
                {
                    use wasm_bindgen::JsCast;
                    if let Some(window) = web_sys::window() {
                        if let Some(doc) = window.document() {
                            if let Some(el) = doc.get_element_by_id(&id) {
                                if let Ok(input) = el.dyn_into::<web_sys::HtmlInputElement>() {
                                    input.click();
                                }
                            }
                        }
                    }
                }
                #[cfg(not(feature = "web"))]
                {
                    let _ = id; // suppress unused warning on non-web
                    log::debug!("QR file-input trigger not yet implemented on native; use file picker");
                }
            },
            if *scanning.read() {
                "Decoding…"
            } else {
                span { class: "text-base leading-none", "📷" }
                "{props.label}"
            }
        }
        input {
            id: "{file_input_id}",
            class: "hidden",
            r#type: "file",
            accept: "image/*",
            capture: "environment",
            onchange: move |e| {
                let on_scan = props.on_scan;
                let on_error = props.on_error;
                scanning.set(true);

                spawn(async move {
                    match decode_qr_from_file_event(&e).await {
                        Ok(decoded) => {
                            on_scan.call(decoded);
                        }
                        Err(err) => {
                            log::warn!("QR scan failed: {err}");
                            if let Some(handler) = on_error {
                                handler.call(err);
                            }
                        }
                    }
                    scanning.set(false);
                });
            },
        }
    }
}

/// Decode a QR code from a Dioxus file input event.
///
/// Reads the file bytes, loads them as an image, converts to grayscale,
/// and runs `rqrr` to find and decode QR codes.
async fn decode_qr_from_file_event(
    e: &Event<FormData>,
) -> Result<String, String> {
    let file_bytes = read_file_bytes_from_event(e).await?;

    // Load the image from bytes.
    let img = image::load_from_memory(&file_bytes)
        .map_err(|e| format!("Failed to load image: {e}"))?;

    // Convert to grayscale for QR detection.
    let gray = img.to_luma8();

    // Find and decode QR codes using rqrr's PreparedImage API.
    let mut prepared = rqrr::PreparedImage::prepare(gray);
    let grids = prepared.detect_grids();

    if grids.is_empty() {
        return Err("No QR code found in the image".to_string());
    }

    // Decode the first grid found.
    let (_meta, content) = grids[0]
        .decode()
        .map_err(|e| format!("QR decode failed: {e}"))?;

    Ok(content)
}

/// Read file bytes from a Dioxus form data event.
async fn read_file_bytes_from_event(
    e: &Event<FormData>,
) -> Result<Vec<u8>, String> {
    let value = e.value();
    if value.is_empty() {
        return Err("No file selected".to_string());
    }

    #[cfg(feature = "web")]
    {
        read_file_bytes_web(&value).await
    }
    #[cfg(not(feature = "web"))]
    {
        // On native, `value` is the file path. Try reading from disk.
        std::fs::read(&value).map_err(|e| format!("Failed to read file: {e}"))
    }
}

#[cfg(feature = "web")]
async fn read_file_bytes_web(filename: &str) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;

    let inputs = document
        .query_selector_all("input[type='file']")
        .map_err(|e| format!("querySelectorAll failed: {e:?}"))?;
    for i in 0..inputs.length() {
        if let Some(input) = inputs.get(i) {
            if let Ok(input) = input.dyn_into::<web_sys::HtmlInputElement>() {
                if let Some(files) = input.files() {
                    if let Some(file) = files.get(0) {
                        if file.name() == filename {
                            let array_buffer = JsFuture::from(file.array_buffer())
                                .await
                                .map_err(|e| format!("arrayBuffer failed: {e:?}"))?;
                            let uint8 = js_sys::Uint8Array::new(&array_buffer);
                            let mut bytes = vec![0u8; uint8.length() as usize];
                            uint8.copy_to(&mut bytes);
                            return Ok(bytes);
                        }
                    }
                }
            }
        }
    }
    Err(format!("Could not find file {filename:?} in any file input"))
}

