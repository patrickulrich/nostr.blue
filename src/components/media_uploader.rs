use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;
use crate::stores::blossom_store;

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
}

#[component]
pub fn MediaUploader(props: MediaUploaderProps) -> Element {
    let mut selected_file = use_signal(|| None::<(String, Vec<u8>, String)>); // (filename, data, mime)
    let mut quality = use_signal(|| 80u8); // Default to 80% quality
    let mut uploading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let upload_progress = blossom_store::UPLOAD_PROGRESS.read();

    // Clone input_id for use in rsx! and closure
    let input_id = props.input_id.clone();
    let input_id_for_handler = input_id.clone();

    // File input change handler
    let handle_file_select = move |evt: Event<FormData>| {
        let input_id = input_id_for_handler.clone();
        spawn(async move {
            error.set(None);

            if let Some(file_engine) = evt.files() {
                let files = file_engine.files();

                if let Some(file_name) = files.get(0) {
                    // Read file data
                    match read_file_as_bytes(&file_name, &input_id).await {
                        Ok((data, mime_type)) => {
                            log::info!("File selected: {} ({} bytes)", file_name, data.len());
                            selected_file.set(Some((file_name.clone(), data, mime_type)));
                        }
                        Err(e) => {
                            log::error!("Failed to read file: {}", e);
                            error.set(Some(format!("Failed to read file: {}", e)));
                        }
                    }
                }
            }
        });
    };

    // Upload handler
    let handle_upload = move |_| {
        if let Some((_filename, data, mime_type)) = selected_file.read().clone() {
            let quality_val = *quality.read();
            let on_upload = props.on_upload.clone();

            uploading.set(true);
            error.set(None);

            spawn(async move {
                match blossom_store::upload_image(data, mime_type, quality_val).await {
                    Ok(url) => {
                        log::info!("Upload successful: {}", url);
                        on_upload.call(url);
                        selected_file.set(None);
                        uploading.set(false);
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

    // Clear selection
    let handle_clear = move |_| {
        selected_file.set(None);
        error.set(None);
    };

    let quality_label = match *quality.read() {
        100 => "Original (No Compression)",
        80..=99 => "High Quality",
        50..=79 => "Medium Quality",
        _ => "Low Quality (Smaller Size)",
    };

    rsx! {
        div {
            class: "space-y-3",

            // File input
            if selected_file.read().is_none() {
                div {
                    class: "flex items-center justify-center w-full",
                    label {
                        class: "flex flex-col items-center justify-center w-full h-32 border-2 border-gray-300 border-dashed rounded-lg cursor-pointer bg-gray-50 dark:bg-gray-700 hover:bg-gray-100 dark:hover:bg-gray-600 dark:border-gray-600",
                        div {
                            class: "flex flex-col items-center justify-center pt-5 pb-6",
                            span {
                                class: "text-4xl mb-2",
                                "📎"
                            }
                            p {
                                class: "mb-2 text-sm text-gray-500 dark:text-gray-400",
                                span {
                                    class: "font-semibold",
                                    "Click to upload"
                                }
                                " or drag and drop"
                            }
                            p {
                                class: "text-xs text-gray-500 dark:text-gray-400",
                                "Images (PNG, JPG) or Videos (MP4, MOV)"
                            }
                        }
                        input {
                            id: "{props.input_id}",
                            class: "hidden",
                            r#type: "file",
                            accept: "image/*,video/*",
                            onchange: handle_file_select,
                        }
                    }
                }
            } else {
                // File preview
                if let Some((filename, data, _)) = selected_file.read().as_ref() {
                    div {
                        class: "p-4 bg-gray-50 dark:bg-gray-700 rounded-lg space-y-3",

                        // File info
                        div {
                            class: "flex items-center justify-between",
                            div {
                                p {
                                    class: "text-sm font-medium text-gray-900 dark:text-white",
                                    "{filename}"
                                }
                                p {
                                    class: "text-xs text-gray-500 dark:text-gray-400",
                                    "{format_file_size(data.len())}"
                                }
                            }
                            button {
                                class: "px-3 py-1 text-sm text-red-600 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300",
                                onclick: handle_clear,
                                "✕ Remove"
                            }
                        }

                        // Quality slider
                        div {
                            class: "space-y-2",
                            label {
                                class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
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
                                }
                            }
                            div {
                                class: "flex justify-between text-xs text-gray-500 dark:text-gray-400",
                                span { "Small" }
                                span { "Original" }
                            }
                        }

                        // Upload button
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

            // Error message
            if let Some(err) = error.read().as_ref() {
                div {
                    class: "p-3 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg text-sm",
                    "❌ {err}"
                }
            }
        }
    }
}

/// Helper function to read file as bytes with specific input ID
async fn read_file_as_bytes(_file_name: &str, input_id: &str) -> Result<(Vec<u8>, String), String> {
    use wasm_bindgen_futures::JsFuture;
    use web_sys::window;
    use js_sys::{Uint8Array, ArrayBuffer};

    let window = window().ok_or("No window")?;
    let document = window.document().ok_or("No document")?;

    // Get the specific file input element by ID
    let input = document
        .get_element_by_id(input_id)
        .ok_or("Input not found")?
        .dyn_into::<HtmlInputElement>()
        .map_err(|_| "Not an input element")?;

    let file_list = input.files().ok_or("No files")?;
    let file = file_list.get(0).ok_or("No file selected")?;

    let mime_type = file.type_();

    // Read file as array buffer
    let promise = file.array_buffer();
    let array_buffer = JsFuture::from(promise)
        .await
        .map_err(|_| "Failed to read file")?;

    let array_buffer: ArrayBuffer = array_buffer.dyn_into().map_err(|_| "Not an ArrayBuffer")?;
    let uint8_array = Uint8Array::new(&array_buffer);
    let bytes = uint8_array.to_vec();

    Ok((bytes, mime_type))
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
