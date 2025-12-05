//! GIF Upload Modal Component
//!
//! Modal dialog for uploading GIFs to Nostr via NIP-96 or Blossom.

use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

use crate::stores::{nip96_store, gif_store, blossom_store};

#[derive(Props, Clone, PartialEq)]
pub struct GifUploadModalProps {
    /// Signal to control modal visibility
    pub show: Signal<bool>,
    /// Optional callback when upload completes successfully
    #[props(default)]
    pub on_upload: Option<EventHandler<gif_store::GifMetadata>>,
}

/// Upload server options
#[derive(Clone, PartialEq)]
enum UploadServer {
    NostrBuild,
    Blossom,
}

#[component]
pub fn GifUploadModal(props: GifUploadModalProps) -> Element {
    let mut show = props.show;

    // State
    // Store: (filename, data, mime, preview_url) - preview_url is an Object URL for efficient preview
    let mut selected_file = use_signal(|| None::<(String, Vec<u8>, String, Option<String>)>);
    let mut caption = use_signal(|| String::new());
    let mut upload_server = use_signal(|| UploadServer::NostrBuild);
    let mut uploading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);

    // Derive progress reactively based on selected server
    // Using use_memo ensures proper re-renders when global signals change
    let progress = use_memo(move || {
        match *upload_server.read() {
            UploadServer::NostrBuild => *nip96_store::NIP96_UPLOAD_PROGRESS.read(),
            UploadServer::Blossom => *blossom_store::UPLOAD_PROGRESS.read(),
        }
    });

    // Generate unique input ID
    let input_id = use_signal(|| format!("gif-upload-{}", uuid::Uuid::new_v4()));

    // Close modal handler
    let close_modal = move |_| {
        show.set(false);
        // Revoke object URL to free memory
        if let Some((_, _, _, Some(url))) = selected_file.read().as_ref() {
            let _ = web_sys::Url::revoke_object_url(url);
        }
        // Reset state
        selected_file.set(None);
        caption.set(String::new());
        error.set(None);
        success.set(false);
        clear_file_input(&input_id.read());
    };

    // File selection handler
    let handle_file_select = {
        let input_id = input_id.clone();
        move |_evt: Event<FormData>| {
            let input_id = input_id.read().clone();
            spawn(async move {
                error.set(None);

                match read_file_as_bytes(&input_id).await {
                    Ok((filename, data, mime_type)) => {
                        // Validate GIF magic bytes (GIF87a or GIF89a)
                        // This is more reliable than MIME type or extension which can be spoofed
                        if data.len() < 6 || (&data[0..6] != b"GIF87a" && &data[0..6] != b"GIF89a") {
                            error.set(Some("Invalid GIF file. Please select a valid GIF.".to_string()));
                            return;
                        }

                        // Also check MIME type or extension as secondary validation
                        if !mime_type.contains("gif") && !filename.to_lowercase().ends_with(".gif") {
                            error.set(Some("Please select a GIF file".to_string()));
                            return;
                        }

                        // Check file size (max 21MB like gifbuddy)
                        if data.len() > 21 * 1024 * 1024 {
                            error.set(Some("File too large. Maximum size is 21MB".to_string()));
                            return;
                        }

                        // Create Object URL for efficient preview (avoids base64 memory overhead)
                        let preview_url = create_object_url(&data, &mime_type);

                        log::info!("Selected GIF: {} ({} bytes)", filename, data.len());
                        selected_file.set(Some((filename, data, mime_type, preview_url)));
                    }
                    Err(e) => {
                        log::error!("Failed to read file: {}", e);
                        error.set(Some(format!("Failed to read file: {}", e)));
                    }
                }
            });
        }
    };

    // Upload handler
    let handle_upload = {
        let on_upload = props.on_upload.clone();
        move |_| {
            let file_data = selected_file.read().clone();
            let caption_text = caption.read().clone();
            let server = upload_server.read().clone();
            let on_upload = on_upload.clone();

            if file_data.is_none() {
                error.set(Some("Please select a file first".to_string()));
                return;
            }

            if caption_text.trim().is_empty() {
                error.set(Some("Please enter a caption/description".to_string()));
                return;
            }

            let (_filename, data, mime_type, _preview_url) = file_data.unwrap();
            let file_size = data.len();

            uploading.set(true);
            error.set(None);

            spawn(async move {
                // Calculate hash for later use
                use sha2::{Sha256, Digest};
                let mut hasher = Sha256::new();
                hasher.update(&data);
                let file_hash = hex::encode(hasher.finalize());

                // Upload based on selected server
                let upload_result = match server {
                    UploadServer::NostrBuild => {
                        nip96_store::upload_to_nip96(
                            data,
                            mime_type.clone(),
                            caption_text.clone(),
                            caption_text.clone(),
                        ).await.map(|metadata| (
                            metadata.url,
                            metadata.original_hash.unwrap_or(file_hash.clone()),
                            metadata.dimensions,
                        ))
                    }
                    UploadServer::Blossom => {
                        blossom_store::upload_image(
                            data,
                            mime_type.clone(),
                            100, // No compression for GIFs
                        ).await.map(|url| (url, file_hash.clone(), None))
                    }
                };

                match upload_result {
                    Ok((url, hash, dimensions)) => {
                        log::info!("File uploaded successfully: {}", url);

                        // Now publish the NIP-94 event
                        let dims = dimensions.map(|(w, h)| (w, h));

                        match gif_store::publish_gif_event(
                            url.clone(),
                            "image/gif".to_string(),
                            hash,
                            caption_text.clone(),
                            Some(file_size),
                            dims,
                        ).await {
                            Ok(event_id) => {
                                log::info!("GIF event published: {}", event_id);
                                success.set(true);
                                uploading.set(false);

                                // Create GifMetadata for callback
                                let gif_metadata = gif_store::GifMetadata {
                                    url: url.clone(),
                                    thumbnail: None,
                                    dimensions: dims.map(|(w, h)| (w as u64, h as u64)),
                                    size: Some(file_size),
                                    blurhash: None,
                                    alt: Some(caption_text.clone()),
                                    summary: Some(caption_text),
                                    created_at: nostr_sdk::Timestamp::now(),
                                };

                                // Add to recent GIFs
                                gif_store::add_recent_gif(gif_metadata.clone());

                                // Call callback if provided
                                if let Some(handler) = on_upload {
                                    handler.call(gif_metadata);
                                }

                                // Auto-close after success
                                spawn(async move {
                                    gloo_timers::future::TimeoutFuture::new(2000).await;
                                    show.set(false);
                                    // Reset state
                                    selected_file.set(None);
                                    caption.set(String::new());
                                    success.set(false);
                                });
                            }
                            Err(e) => {
                                log::error!("Failed to publish GIF event: {}", e);
                                error.set(Some(format!("Upload succeeded but failed to publish: {}", e)));
                                uploading.set(false);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Upload failed: {}", e);
                        error.set(Some(e));
                        uploading.set(false);
                    }
                }
            });
        }
    };

    // Clear file selection
    let handle_clear = {
        let input_id = input_id.clone();
        move |_| {
            // Revoke object URL to free memory
            if let Some((_, _, _, Some(url))) = selected_file.read().as_ref() {
                let _ = web_sys::Url::revoke_object_url(url);
            }
            selected_file.set(None);
            error.set(None);
            clear_file_input(&input_id.read());
        }
    };

    // Don't render if not visible
    if !*show.read() {
        return rsx! { div {} };
    }

    rsx! {
        // Modal overlay (z-70 to appear above GIF picker which is z-60)
        div {
            class: "fixed inset-0 z-[70] flex items-center justify-center bg-black/50",
            onclick: close_modal,

            // Modal content
            div {
                class: "bg-white dark:bg-gray-800 rounded-xl shadow-2xl max-w-lg w-full mx-4",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between p-4 border-b border-gray-200 dark:border-gray-700",
                    h2 {
                        class: "text-lg font-bold text-gray-900 dark:text-white flex items-center gap-2",
                        span { "🎬" }
                        "Upload GIF"
                    }
                    button {
                        class: "text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-full p-1 transition",
                        onclick: close_modal,
                        "✕"
                    }
                }

                // Body
                div {
                    class: "p-4 space-y-4",

                    // Success message
                    if *success.read() {
                        div {
                            class: "p-4 bg-green-100 dark:bg-green-900/30 border border-green-500/30 rounded-lg text-green-700 dark:text-green-300 text-center",
                            span { class: "text-2xl", "✓" }
                            p { class: "font-medium mt-2", "GIF uploaded successfully!" }
                            p { class: "text-sm opacity-75", "Your GIF is now available in the GIF picker." }
                        }
                    }

                    // File selection area
                    if !*success.read() {
                        if selected_file.read().is_none() {
                            // File input
                            div {
                                class: "flex items-center justify-center w-full",
                                label {
                                    class: "flex flex-col items-center justify-center w-full h-40 border-2 border-gray-300 dark:border-gray-600 border-dashed rounded-xl cursor-pointer bg-gray-50 dark:bg-gray-700/50 hover:bg-gray-100 dark:hover:bg-gray-700 transition",
                                    div {
                                        class: "flex flex-col items-center justify-center py-4",
                                        span {
                                            class: "text-5xl mb-3",
                                            "🎬"
                                        }
                                        p {
                                            class: "mb-2 text-sm text-gray-500 dark:text-gray-400",
                                            span { class: "font-semibold", "Click to upload" }
                                            " or drag and drop"
                                        }
                                        p {
                                            class: "text-xs text-gray-500 dark:text-gray-400",
                                            "GIF files only (max 21MB)"
                                        }
                                    }
                                    input {
                                        id: "{input_id}",
                                        class: "hidden",
                                        r#type: "file",
                                        accept: "image/gif,.gif",
                                        onchange: handle_file_select,
                                    }
                                }
                            }
                        } else {
                            // File preview
                            if let Some((filename, data, _, preview_url)) = selected_file.read().as_ref() {
                                div {
                                    class: "p-4 bg-gray-50 dark:bg-gray-700/50 rounded-xl space-y-3",

                                    // File info with preview
                                    div {
                                        class: "flex items-start gap-4",
                                        // GIF preview - use Object URL for efficiency (avoids base64 memory overhead)
                                        div {
                                            class: "w-24 h-24 rounded-lg overflow-hidden bg-gray-200 dark:bg-gray-600 flex-shrink-0",
                                            if let Some(url) = preview_url {
                                                img {
                                                    class: "w-full h-full object-cover",
                                                    src: "{url}",
                                                    alt: "Preview"
                                                }
                                            } else {
                                                // Fallback to base64 if object URL creation failed
                                                img {
                                                    class: "w-full h-full object-cover",
                                                    src: format!("data:image/gif;base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data)),
                                                    alt: "Preview"
                                                }
                                            }
                                        }
                                        div {
                                            class: "flex-1 min-w-0",
                                            p {
                                                class: "text-sm font-medium text-gray-900 dark:text-white truncate",
                                                "{filename}"
                                            }
                                            p {
                                                class: "text-xs text-gray-500 dark:text-gray-400",
                                                "{format_file_size(data.len())}"
                                            }
                                            button {
                                                class: "mt-2 px-3 py-1 text-xs text-red-600 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300 hover:bg-red-100 dark:hover:bg-red-900/30 rounded transition",
                                                onclick: handle_clear,
                                                "Remove"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Caption input
                        div {
                            label {
                                class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                                "Caption / Search Terms"
                            }
                            input {
                                class: "w-full px-4 py-3 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 placeholder-gray-500 dark:placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                                r#type: "text",
                                placeholder: "e.g., funny cat, reaction, celebration...",
                                value: "{caption}",
                                disabled: *uploading.read(),
                                oninput: move |evt| caption.set(evt.value().clone()),
                            }
                            p {
                                class: "mt-1 text-xs text-gray-500 dark:text-gray-400",
                                "This will be used for search and accessibility"
                            }
                        }

                        // Server selection
                        div {
                            label {
                                class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                                "Upload to"
                            }
                            select {
                                class: "w-full px-4 py-3 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                                disabled: *uploading.read(),
                                onchange: move |evt| {
                                    match evt.value().as_str() {
                                        "nostr.build" => upload_server.set(UploadServer::NostrBuild),
                                        "blossom" => upload_server.set(UploadServer::Blossom),
                                        _ => {}
                                    }
                                },
                                option {
                                    value: "nostr.build",
                                    selected: *upload_server.read() == UploadServer::NostrBuild,
                                    "nostr.build (Recommended)"
                                }
                                option {
                                    value: "blossom",
                                    selected: *upload_server.read() == UploadServer::Blossom,
                                    "Blossom ({blossom_store::get_primary_server()})"
                                }
                            }
                        }

                        // Progress bar
                        if *uploading.read() {
                            div {
                                class: "space-y-2",
                                div {
                                    class: "w-full bg-gray-200 dark:bg-gray-600 rounded-full h-2",
                                    div {
                                        class: "bg-blue-600 h-2 rounded-full transition-all duration-300",
                                        style: format!("width: {}%", progress.read().unwrap_or(0.0)),
                                    }
                                }
                                p {
                                    class: "text-xs text-gray-500 dark:text-gray-400 text-center",
                                    {
                                        if let Some(p) = *progress.read() {
                                            format!("Uploading... {:.0}%", p)
                                        } else {
                                            "Uploading...".to_string()
                                        }
                                    }
                                }
                            }
                        }

                        // Error message
                        if let Some(err) = error.read().as_ref() {
                            div {
                                class: "p-3 bg-red-100 dark:bg-red-900/30 border border-red-500/30 rounded-lg text-red-700 dark:text-red-300 text-sm",
                                "{err}"
                            }
                        }
                    }
                }

                // Footer
                if !*success.read() {
                    div {
                        class: "flex justify-end gap-3 p-4 border-t border-gray-200 dark:border-gray-700",
                        button {
                            class: "px-4 py-2 text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition",
                            disabled: *uploading.read(),
                            onclick: close_modal,
                            "Cancel"
                        }
                        button {
                            class: "px-6 py-2 bg-gradient-to-r from-green-500 to-green-600 hover:from-green-600 hover:to-green-700 text-white rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2",
                            disabled: *uploading.read() || selected_file.read().is_none() || caption.read().trim().is_empty(),
                            onclick: handle_upload,
                            if *uploading.read() {
                                span {
                                    class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"
                                }
                                "Uploading..."
                            } else {
                                span { "⬆️" }
                                "Upload GIF"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Read file as bytes from file input
async fn read_file_as_bytes(input_id: &str) -> Result<(String, Vec<u8>, String), String> {
    use wasm_bindgen_futures::JsFuture;
    use web_sys::window;
    use js_sys::{Uint8Array, ArrayBuffer};

    let window = window().ok_or("No window")?;
    let document = window.document().ok_or("No document")?;

    let input = document
        .get_element_by_id(input_id)
        .ok_or("Input not found")?
        .dyn_into::<HtmlInputElement>()
        .map_err(|_| "Not an input element")?;

    let file_list = input.files().ok_or("No files")?;
    let file = file_list.get(0).ok_or("No file selected")?;

    let filename = file.name();
    let mime_type = file.type_();

    // Read file as array buffer
    let promise = file.array_buffer();
    let array_buffer = JsFuture::from(promise)
        .await
        .map_err(|_| "Failed to read file")?;

    let array_buffer: ArrayBuffer = array_buffer.dyn_into().map_err(|_| "Not an ArrayBuffer")?;
    let uint8_array = Uint8Array::new(&array_buffer);
    let bytes = uint8_array.to_vec();

    Ok((filename, bytes, mime_type))
}

/// Clear file input element
fn clear_file_input(input_id: &str) {
    use web_sys::window;

    if let Some(window) = window() {
        if let Some(document) = window.document() {
            if let Some(element) = document.get_element_by_id(input_id) {
                if let Ok(input) = element.dyn_into::<HtmlInputElement>() {
                    input.set_value("");
                }
            }
        }
    }
}

/// Format file size for display
fn format_file_size(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;

    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

/// Create an Object URL from raw bytes (more memory efficient than base64 for large files)
fn create_object_url(data: &[u8], mime_type: &str) -> Option<String> {
    use web_sys::BlobPropertyBag;

    // Create Uint8Array from data
    let uint8_array = js_sys::Uint8Array::from(data);
    let blob_parts = js_sys::Array::new();
    blob_parts.push(&uint8_array);

    // Create Blob with proper MIME type
    let blob_options = BlobPropertyBag::new();
    blob_options.set_type(mime_type);

    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&blob_parts, &blob_options)
        .ok()?;

    // Create Object URL from Blob
    web_sys::Url::create_object_url_with_blob(&blob).ok()
}
