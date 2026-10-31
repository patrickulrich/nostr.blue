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

    // Decode grids in order, returning the first successful payload. This
    // tolerates a blurred/unreadable first grid when a later grid is clean,
    // and handles multi-QR images by yielding the first valid code.
    for grid in &grids {
        if let Ok((_meta, content)) = grid.decode() {
            if !content.trim().is_empty() {
                return Ok(content);
            }
        }
    }
    Err("Unable to decode any QR code in the image".to_string())
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

/// Live camera QR scanner (NUT-16 capable).
///
/// Opens the rear camera, decodes frames continuously via the same pure-Rust
/// `rqrr` pipeline as the still-image path, and feeds `ur:` parts into cdk's
/// fountain decoder until an animated Cashu token reassembles — showing
/// fragment progress while scanning. Non-UR payloads are emitted on first
/// successful decode.
///
/// Platform support: web browsers and the Android WebView (the manifest's
/// CAMERA permission + wry's permission bridge make getUserMedia work there;
/// scanning is the reason Item 0 exists). The Linux desktop has no camera
/// layer under wry, so native builds render the still-image picker instead.
#[derive(Props, Clone, PartialEq)]
pub struct QrLiveScannerProps {
    /// Callback fired with the decoded text (a complete cashu token for
    /// animated `ur:` sequences).
    pub on_scan: EventHandler<String>,
    /// Callback fired when the user closes the scanner without a result.
    pub on_close: EventHandler<()>,
    /// Optional hint rendered under the video.
    #[props(default = "Point the camera at a QR code".to_string())]
    pub hint: String,
}

#[component]
pub fn QrLiveScanner(props: QrLiveScannerProps) -> Element {
    let mut status = use_signal(String::new);
    let mut progress = use_signal(|| Option::<(usize, Option<usize>)>::None);
    let mut live_error = use_signal(|| Option::<String>::None);
    // `true` once a result has been emitted — stops the loop from firing the
    // callback repeatedly while the overlay unmounts.
    let mut done = use_signal(|| false);
    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    let progress_label = use_memo(move || {
        progress()
            .map(|(resolved, total)| {
                let total_label = total.map(|n| format!(" of {n}")).unwrap_or_default();
                format!("{resolved} fragments received{total_label}")
            })
            .unwrap_or_default()
    });

    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    {
        use_effect(move || {
            let on_scan = props.on_scan;
            spawn(async move {
                if let Err(e) = run_live_scan(
                    &mut status,
                    &mut progress,
                    &mut done,
                    on_scan,
                )
                .await
                {
                    live_error.set(Some(e));
                }
            });
        });

        rsx! {
            div {
                class: "fixed inset-0 bg-black/80 z-50 flex flex-col items-center justify-center p-4 gap-3",
                div {
                    class: "bg-card border border-border rounded-lg p-4 w-full max-w-sm",
                    video {
                        id: "qr-live-video",
                        class: "w-full rounded-lg bg-black",
                        autoplay: true,
                        playsinline: true,
                        muted: true,
                    }
                    div { class: "text-center text-sm text-muted-foreground mt-2", "{props.hint}" }
                    if !progress_label.is_empty() {
                        div { class: "text-center text-xs text-muted-foreground",
                            "animated QR: {progress_label}"
                        }
                    }
                    match (&status(), live_error().as_ref()) {
                        (s, None) if !s.is_empty() => rsx! {
                            div { class: "text-center text-xs text-muted-foreground mt-1", "{s}" }
                        },
                        (_, Some(err)) => rsx! {
                            div { class: "text-center text-xs text-red-500 mt-1", "{err}" }
                        },
                        _ => rsx! {},
                    }
                    button {
                        class: "mt-3 w-full px-3 py-2 border border-border rounded-lg text-sm hover:bg-accent transition",
                        onclick: move |_| {
                            stop_camera();
                            props.on_close.call(());
                        },
                        "Close"
                    }
                }
            }
        }
    }
    #[cfg(not(all(feature = "web", target_arch = "wasm32")))]
    {
        // Native desktop: no camera layer under wry — offer the still path.
        // (The &mut borrows keep the `mut` signal bindings meaningful when
        // the live-scan branch is compiled out.)
        let _ = (
            &mut status,
            &mut progress,
            &mut live_error,
            &mut done,
        );
        rsx! {
            div {
                class: "fixed inset-0 bg-black/80 z-50 flex flex-col items-center justify-center p-4 gap-3",
                div {
                    class: "bg-card border border-border rounded-lg p-4 w-full max-w-sm text-center space-y-3",
                    p { class: "text-sm text-muted-foreground",
                        "Live camera scanning is available on web and Android. Use an image file instead:"
                    }
                    QrScanner {
                        label: "Pick QR image".to_string(),
                        on_scan: move |decoded| {
                            props.on_scan.call(decoded);
                        },
                    }
                    button {
                        class: "w-full px-3 py-2 border border-border rounded-lg text-sm hover:bg-accent transition",
                        onclick: move |_| props.on_close.call(()),
                        "Close"
                    }
                }
            }
        }
    }
}

/// Downscaled capture width for per-frame decoding. ~480px gray keeps rqrr's
/// grid detection fast enough for a live loop on mid-range hardware.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
const LIVE_SCAN_WIDTH: u32 = 480;
/// Milliseconds between frame captures.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
const LIVE_SCAN_INTERVAL_MS: u32 = 120;

#[cfg(all(feature = "web", target_arch = "wasm32"))]
async fn run_live_scan(
    status: &mut Signal<String>,
    progress: &mut Signal<Option<(usize, Option<usize>)>>,
    done: &mut Signal<bool>,
    on_scan: EventHandler<String>,
) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;

    // Locate the video element this component just rendered.
    let video = {
        let mut video = None;
        for _ in 0..50 {
            if let Some(el) = document.get_element_by_id("qr-live-video") {
                video = Some(
                    el.dyn_into::<web_sys::HtmlVideoElement>()
                        .map_err(|_| "video element cast failed".to_string())?,
                );
                break;
            }
            crate::platform::timer::sleep_ms(20).await;
        }
        video.ok_or("video element not found")?
    };

    // Rear camera; falls back to any camera the browser offers.
    let media_devices = window
        .navigator()
        .media_devices()
        .map_err(|e| format!("mediaDevices unavailable: {e:?}"))?;
    let constraints = web_sys::MediaStreamConstraints::new();
    let video_constraints = web_sys::MediaTrackConstraints::new();
    video_constraints.set_facing_mode(&wasm_bindgen::JsValue::from_str("environment"));
    constraints.set_video(&video_constraints);
    let stream_request = media_devices
        .get_user_media_with_constraints(&constraints)
        .map_err(|e| format!("getUserMedia request failed: {e:?}"))?;
    let stream = web_sys::MediaStream::from(
        JsFuture::from(stream_request)
            .await
            .map_err(|e| format!("camera permission denied or unavailable: {e:?}"))?,
    );
    video.set_src_object(Some(&stream));
    if let Ok(play_promise) = video.play() {
        // play() resolves once playback starts; a rejection (e.g. autoplay
        // policy edge case) is non-fatal — the frame loop skips until
        // video dimensions are live anyway.
        let _ = JsFuture::from(play_promise).await;
    }

    status.set("Camera active — scanning…".to_string());

    // Offscreen canvas for frame capture.
    let canvas = document
        .create_element("canvas")
        .map_err(|e| format!("canvas creation failed: {e:?}"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| "canvas cast failed".to_string())?;
    let ctx = canvas
        .get_context("2d")
        .map_err(|e| format!("2d context failed: {e:?}"))?
        .and_then(|c| c.dyn_into::<web_sys::CanvasRenderingContext2d>().ok())
        .ok_or("no 2d context")?;

    #[cfg(feature = "cashu")]
    let mut ur_decoder = cdk::nuts::TokenUrDecoder::default();

    let result: Result<Option<String>, String> = 'scan: loop {
        if *done.read() {
            break 'scan Ok(None);
        }
        crate::platform::timer::sleep_ms(LIVE_SCAN_INTERVAL_MS).await;

        let vw = video.video_width();
        let vh = video.video_height();
        if vw == 0 || vh == 0 {
            continue; // first frame not ready yet
        }
        let scale = LIVE_SCAN_WIDTH as f64 / vw as f64;
        let (cw, ch) = (LIVE_SCAN_WIDTH, (vh as f64 * scale).round() as u32);
        canvas.set_width(cw);
        canvas.set_height(ch);
        if ctx
            .draw_image_with_html_video_element_and_dw_and_dh(&video, 0.0, 0.0, cw as f64, ch as f64)
            .is_err()
        {
            continue;
        }
        let Some(image_data) = ctx.get_image_data(0.0, 0.0, cw as f64, ch as f64).ok() else {
            continue;
        };
        let rgba = image_data.data();
        let mut gray = image::GrayImage::new(cw, ch);
        for (x, y, pixel) in gray.enumerate_pixels_mut() {
            let i = ((y * cw + x) * 4) as usize;
            let luma = (0.299 * rgba[i] as f32
                + 0.587 * rgba[i + 1] as f32
                + 0.114 * rgba[i + 2] as f32) as u8;
            *pixel = image::Luma([luma]);
        }

        let mut prepared = rqrr::PreparedImage::prepare(gray);
        for grid in &prepared.detect_grids() {
            let Ok((_meta, content)) = grid.decode() else {
                continue;
            };
            let text = content.trim().to_string();
            if text.is_empty() {
                continue;
            }

            #[cfg(feature = "cashu")]
            {
                if text.to_ascii_lowercase().starts_with("ur:") {
                    if ur_decoder.receive(&text).is_ok() {
                        progress.set(Some((
                            ur_decoder
                                .resolved_fragment_count()
                                .unwrap_or_default(),
                            Some(ur_decoder.fragment_count()),
                        )));
                        if ur_decoder.complete() {
                            if let Ok(Some(token)) = ur_decoder.token() {
                                break 'scan Ok(Some(token.to_string()));
                            }
                        }
                    }
                    continue;
                }
            }

            // Plain payload: emit immediately. (UR parts above `continue`
            // until reassembly completes; anything else is a one-shot code.)
            break 'scan Ok(Some(text));
        }
    };

    // Tear the camera down regardless of outcome.
    for track in stream.get_tracks() {
        if let Ok(track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
            track.stop();
        }
    }
    video.set_src_object(None);

    match result {
        Ok(Some(decoded)) => {
            done.set(true);
            on_scan.call(decoded);
            Ok(())
        }
        Ok(None) => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
fn stop_camera() {
    // The scan loop owns cleanup when it exits; stopping here would race it.
    // Closing sets `on_close` which unmounts the overlay — the loop's next
    // tick observes unmount via signal drop... the spawned task holds signal
    // handles, so it keeps the component alive until `done`. For simplicity
    // the loop is bounded by the overlay's lifetime: clicking Close while a
    // camera runs stops the tracks through this helper as a belt-and-braces.
    use wasm_bindgen::JsCast;
    if let Some(window) = web_sys::window() {
        if let Some(doc) = window.document() {
            if let Some(el) = doc.get_element_by_id("qr-live-video") {
                if let Ok(video) = el.dyn_into::<web_sys::HtmlVideoElement>() {
                    if let Some(stream) = video.src_object() {
                        if let Ok(stream) = stream.dyn_into::<web_sys::MediaStream>() {
                            for track in stream.get_tracks() {
                                if let Ok(track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
                                    track.stop();
                                }
                            }
                        }
                    }
                    video.set_src_object(None);
                }
            }
        }
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

