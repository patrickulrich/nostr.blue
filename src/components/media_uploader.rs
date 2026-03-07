use crate::stores::blossom_store;
use crate::utils::format::display_server_url;
use dioxus::events::FormData;
use dioxus::prelude::*;

#[cfg(all(feature = "web", feature = "desktop"))]
compile_error!("Cannot enable both 'web' and 'desktop' features");

#[cfg(all(feature = "web", feature = "mobile"))]
compile_error!("Cannot enable both 'web' and 'mobile' features");

#[cfg(all(feature = "desktop", feature = "mobile"))]
compile_error!("Cannot enable both 'desktop' and 'mobile' features");

#[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile")))]
compile_error!("Must enable exactly one of 'web', 'desktop', or 'mobile' feature");

#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
#[cfg(feature = "web")]
use web_sys::HtmlInputElement;

const MEDIA_ACCEPT: &str = "image/*,video/*";
#[derive(Props, Clone, PartialEq)]
pub struct MediaUploaderProps {
    /// Callback when upload completes successfully
    pub on_upload: EventHandler<String>,
    /// Optional label for the upload button
    #[props(default = "Upload Media".to_string())]
    pub button_label: String,
    /// Unique ID for the file input (to avoid conflicts)
    #[props(default = uuid::Uuid::new_v4().to_string())]
    pub input_id: String,
    /// Whether to show server selector (defaults to true)
    #[props(default = true)]
    pub show_server_selector: bool,
}
#[component]
pub fn MediaUploader(props: MediaUploaderProps) -> Element {
    let mut selected_file = use_signal(|| None::<(String, Vec<u8>, String)>);
    let mut quality = use_signal(|| 80u8);
    let mut uploading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let upload_progress = blossom_store::UPLOAD_PROGRESS.read();
    let mut selected_server = use_signal(blossom_store::get_primary_server);
    let show_server_selector = props.show_server_selector;
    let input_id = props.input_id.clone();
    let input_id_for_handler = input_id.clone();
    #[cfg(feature = "mobile")]
    let input_id_for_mobile_handler = input_id.clone();
    let input_id_for_upload = input_id.clone();
    let input_id_for_clear_handler = input_id.clone();
    let handle_file_select = move |evt: Event<FormData>| {
        let input_id = input_id_for_handler.clone();
        spawn(async move {
            error.set(None);
            #[cfg(feature = "mobile")]
            {
                let _ = (evt, input_id);
                // Mobile uses its dedicated picker button path.
            }
            #[cfg(not(feature = "mobile"))]
            {
                let files = evt.files();
                if files.is_empty() {
                    return;
                }
                match read_file_as_bytes(&input_id, MEDIA_ACCEPT).await {
                    Ok((file_name, data, mime_type)) => {
                        log::info!(
                            "File selected: {} ({} bytes)", file_name, data.len()
                        );
                        selected_file.set(Some((file_name, data, mime_type)));
                    }
                    Err(e) => {
                        log::error!("Failed to read file: {}", e);
                        error.set(Some(format!("Failed to read file: {}", e)));
                    }
                }
            }
        });
    };
    #[cfg(feature = "mobile")]
    let handle_mobile_pick = move |_| {
        let input_id = input_id_for_mobile_handler.clone();
        spawn(async move {
            error.set(None);
            match read_file_as_bytes(&input_id, MEDIA_ACCEPT).await {
                Ok((file_name, data, mime_type)) => {
                    log::info!("File selected: {} ({} bytes)", file_name, data.len());
                    selected_file.set(Some((file_name, data, mime_type)));
                }
                Err(e) => {
                    log::error!("Failed to read file: {}", e);
                    error.set(Some(format!("Failed to read file: {}", e)));
                }
            }
        });
    };
    #[cfg(not(feature = "mobile"))]
    let handle_mobile_pick = move |_| {};
    let handle_upload = move |_| {
        if let Some((_filename, data, mime_type)) = selected_file.read().clone() {
            let quality_val = *quality.read();
            let on_upload = props.on_upload;
            let input_id_for_clear = input_id_for_upload.clone();
            let server_url = selected_server.read().clone();
            uploading.set(true);
            error.set(None);
            spawn(async move {
                match blossom_store::upload_image(
                        data,
                        mime_type,
                        quality_val,
                        Some(server_url),
                    )
                    .await
                {
                    Ok(url) => {
                        log::info!("Upload successful: {}", url);
                        on_upload.call(url);
                        selected_file.set(None);
                        uploading.set(false);
                        clear_file_input(&input_id_for_clear);
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
    let handle_clear = move |_| {
        selected_file.set(None);
        error.set(None);
        clear_file_input(&input_id_for_clear_handler);
    };
    let quality_label = match *quality.read() {
        100 => "Original (No Compression)",
        80..=99 => "High Quality",
        50..=79 => "Medium Quality",
        _ => "Low Quality (Smaller Size)",
    };
    rsx! {
        div { class: "space-y-3",
            if selected_file.read().is_none() {
                div { class: "flex items-center justify-center w-full",
                    if cfg!(feature = "mobile") {
                        button {
                            class: "flex flex-col items-center justify-center w-full h-32 border-2 border-gray-300 border-dashed rounded-lg bg-gray-50 dark:bg-gray-700 hover:bg-gray-100 dark:hover:bg-gray-600 dark:border-gray-600",
                            onclick: handle_mobile_pick,
                            div { class: "flex flex-col items-center justify-center pt-5 pb-6",
                                span { class: "text-4xl mb-2", "📎" }
                                p { class: "mb-2 text-sm text-gray-500 dark:text-gray-400",
                                    span { class: "font-semibold", "Tap to upload" }
                                }
                                p { class: "text-xs text-gray-500 dark:text-gray-400",
                                    "Images (PNG, JPG) or Videos (MP4, MOV)"
                                }
                            }
                        }
                    } else {
                        label { class: "flex flex-col items-center justify-center w-full h-32 border-2 border-gray-300 border-dashed rounded-lg cursor-pointer bg-gray-50 dark:bg-gray-700 hover:bg-gray-100 dark:hover:bg-gray-600 dark:border-gray-600",
                            div { class: "flex flex-col items-center justify-center pt-5 pb-6",
                                span { class: "text-4xl mb-2", "📎" }
                                p { class: "mb-2 text-sm text-gray-500 dark:text-gray-400",
                                    span { class: "font-semibold", "Click to upload" }
                                    " or drag and drop"
                                }
                                p { class: "text-xs text-gray-500 dark:text-gray-400",
                                    "Images (PNG, JPG) or Videos (MP4, MOV)"
                                }
                            }
                            input {
                                id: "{props.input_id}",
                                class: "hidden",
                                r#type: "file",
                                accept: "{MEDIA_ACCEPT}",
                                onchange: handle_file_select,
                            }
                        }
                    }
                }
            } else {
                if let Some((filename, data, _)) = selected_file.read().as_ref() {
                    div { class: "p-4 bg-gray-50 dark:bg-gray-700 rounded-lg space-y-3",
                        div { class: "flex items-center justify-between",
                            div {
                                p { class: "text-sm font-medium text-gray-900 dark:text-white",
                                    "{filename}"
                                }
                                p { class: "text-xs text-gray-500 dark:text-gray-400",
                                    "{format_file_size(data.len())}"
                                }
                            }
                            button {
                                class: "px-3 py-1 text-sm text-red-600 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300",
                                onclick: handle_clear,
                                "✕ Remove"
                            }
                        }
                        div { class: "space-y-2",
                            label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                                "Quality: {quality}% ({quality_label})"
                            }
                            input {
                                class: "w-full h-2 bg-gray-200 rounded-lg appearance-none cursor-pointer dark:bg-gray-600",
                                r#type: "range",
                                min: "10",
                                max: "100",
                                step: "10",
                                value: "{quality}",
                                oninput: move |evt| {
                                    if let Ok(val) = evt.value().parse::<u8>() {
                                        quality.set(val);
                                    }
                                },
                            }
                            div { class: "flex justify-between text-xs text-gray-500 dark:text-gray-400",
                                span { "Small" }
                                span { "Original" }
                            }
                        }
                        if show_server_selector {
                            div { class: "space-y-2",
                                label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                                    "Upload to"
                                }
                                select {
                                    class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:outline-hidden focus:ring-2 focus:ring-blue-500 text-sm",
                                    disabled: *uploading.read(),
                                    onchange: move |evt| {
                                        selected_server.set(evt.value());
                                    },
                                    {
                                        let servers = blossom_store::get_servers();
                                        let current_server = selected_server.read().clone();
                                        rsx! {
                                            for server in servers.iter() {
                                                option { value: "{server}", selected: *server == current_server, "{display_server_url(server)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        button {
                            class: "w-full px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white rounded-lg font-medium transition",
                            disabled: *uploading.read(),
                            onclick: handle_upload,
                            if *uploading.read() {
                                if let Some(progress) = *upload_progress {
                                    "Uploading... {progress:.0}%"
                                } else {
                                    "Uploading..."
                                }
                            } else {
                                "{props.button_label}"
                            }
                        }
                    }
                }
            }
            if let Some(err) = error.read().as_ref() {
                div { class: "p-3 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg text-sm",
                    "❌ {err}"
                }
            }
        }
    }
}
/// Helper function to read file as bytes with specific input ID
#[cfg(feature = "web")]
async fn read_file_as_bytes(
    input_id: &str,
    accept: &str,
) -> Result<(String, Vec<u8>, String), String> {
    use js_sys::{ArrayBuffer, Uint8Array};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::window;
    let window = window().ok_or("No window")?;
    let document = window.document().ok_or("No document")?;
    let input = document
        .get_element_by_id(input_id)
        .ok_or("Input not found")?
        .dyn_into::<HtmlInputElement>()
        .map_err(|_| "Not an input element")?;
    let file_list = input.files().ok_or("No files")?;
    let file = file_list.get(0).ok_or("No file selected")?;
    let file_name = file.name();
    let mime_type = file.type_();
    if !matches_accept_filter(accept, &mime_type, &file_name) {
        return Err("Selected file type is not allowed".to_string());
    }
    let promise = file.array_buffer();
    let array_buffer = JsFuture::from(promise).await.map_err(|_| "Failed to read file")?;
    let array_buffer: ArrayBuffer = array_buffer
        .dyn_into()
        .map_err(|_| "Not an ArrayBuffer")?;
    let uint8_array = Uint8Array::new(&array_buffer);
    let bytes = uint8_array.to_vec();
    Ok((file_name, bytes, mime_type))
}

/// Stub for desktop platforms
#[cfg(all(feature = "native", not(feature = "mobile")))]
async fn read_file_as_bytes(
    _input_id: &str,
    accept: &str,
) -> Result<(String, Vec<u8>, String), String> {
    let file_handle = rfd::FileDialog::new()
        .set_title("Select media file")
        .pick_file()
        .ok_or("No file selected")?;
    let file_name = file_handle
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid file name")?
        .to_string();
    let data = std::fs::read(&file_handle)
        .map_err(|e| format!("Failed to read selected file: {}", e))?;
    let mime_type = mime_type_from_filename(&file_name);
    if !matches_accept_filter(accept, &mime_type, &file_name) {
        return Err("Selected file type is not allowed".to_string());
    }
    Ok((file_name, data, mime_type))
}

/// Mobile implementation - uses Android file picker via JNI
#[cfg(feature = "mobile")]
async fn read_file_as_bytes(
    _input_id: &str,
    accept: &str,
) -> Result<(String, Vec<u8>, String), String> {
    let (bytes, mime_type) = crate::platform::mobile::pick_file().await?;
    let extension = mime_type
        .split('/')
        .nth(1)
        .map(|s| s.split(';').next().unwrap_or("bin"))
        .unwrap_or("bin");
    let filename = format!("upload.{}", extension);
    if !matches_accept_filter(accept, &mime_type, &filename) {
        return Err("Selected file type is not allowed".to_string());
    }
    Ok((filename, bytes, mime_type))
}

fn matches_accept_filter(accept: &str, mime_type: &str, filename: &str) -> bool {
    let mime_type = mime_type.trim().to_lowercase();
    let filename = filename.to_lowercase();
    for token in accept.split(',').map(|s| s.trim().to_lowercase()) {
        if token.is_empty() {
            continue;
        }
        if token == "*/*" {
            return true;
        }
        if let Some(prefix) = token.strip_suffix("/*") {
            if mime_type.starts_with(&format!("{}/", prefix)) {
                return true;
            }
            continue;
        }
        if token.starts_with('.') {
            if filename.ends_with(&token) {
                return true;
            }
            continue;
        }
        if mime_type == token {
            return true;
        }
    }
    false
}

#[cfg(all(feature = "native", not(feature = "mobile")))]
fn mime_type_from_filename(filename: &str) -> String {
    match filename
        .rsplit('.')
        .next()
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        _ => "application/octet-stream",
    }
    .to_string()
}
/// Helper function to format file size
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
/// Helper function to clear the file input element value
#[cfg(feature = "web")]
fn clear_file_input(input_id: &str) {
    use web_sys::window;
    if let Some(window) = window() {
        if let Some(document) = window.document() {
            if let Some(element) = document.get_element_by_id(input_id) {
                if let Ok(input) = element.dyn_into::<HtmlInputElement>() {
                    input.set_value("");
                    log::debug!("Cleared file input: {}", input_id);
                }
            }
        }
    }
}

/// Stub for desktop platforms
#[cfg(all(feature = "native", not(feature = "mobile")))]
fn clear_file_input(_input_id: &str) {
    // No-op on desktop
}

/// Mobile: no-op (Android handles this differently)
#[cfg(feature = "mobile")]
fn clear_file_input(_input_id: &str) {
    // No-op on mobile
}
