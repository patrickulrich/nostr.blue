//! Repository File Upload
//!
//! Upload files to Blossom servers for use with a repository.
//! Supports drag-and-drop, multiple files, and any file type.
use crate::components::icons;
use crate::routes::Route;
use crate::stores::{auth_store, blossom_store, nostr_client};
use crate::services::git_hosting::fetch_repository;
use crate::utils::clipboard::copy_to_clipboard;
use crate::utils::format::display_server_url;
use crate::utils::nip34::Repository;
use dioxus::html::HasFileData;
use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use js_sys::{ArrayBuffer, Uint8Array};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::{HtmlElement, HtmlInputElement};

/// A file selected for upload (name, bytes, mime type)
#[derive(Clone, Debug)]
struct SelectedFile {
    id: u64,
    name: String,
    data: Vec<u8>,
    mime_type: String,
}

/// Result of a completed upload
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct UploadResult {
    id: u64,
    name: String,
    url: String,
    size: usize,
}

/// Repository upload page component
#[component]
pub fn CodeRepoUpload(naddr: String) -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut selected_files = use_signal(Vec::<SelectedFile>::new);
    let mut upload_results = use_signal(Vec::<UploadResult>::new);
    let mut uploading = use_signal(|| false);
    let mut current_file_index = use_signal(|| 0usize);
    let mut error_message = use_signal(|| None::<String>);
    let mut is_dragging = use_signal(|| false);
    let mut selected_server = use_signal(blossom_store::get_primary_server);
    let upload_progress = blossom_store::UPLOAD_PROGRESS.read();
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    let mut copied_index = use_signal(|| None::<usize>);
    let mut next_file_id = use_signal(|| 0u64);

    let input_id = "code-repo-upload-file-input";

    // Fetch repository data
    let mut request_gen = use_signal(|| 0u64);
    use_effect(use_reactive(&naddr, move |naddr| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let gen = request_gen.peek().wrapping_add(1);
        request_gen.set(gen);
        repo_result.set(None);
        let n = naddr.clone();
        spawn(async move {
            let result = fetch_repository(&n).await;
            if *request_gen.peek() != gen { return; }
            repo_result.set(Some(result));
        });
    }));

    // Auth gate
    if !auth.is_authenticated {
        return rsx! {
            NotAuthenticatedState { naddr: naddr.clone() }
        };
    }

    let repo_name = match &*repo_result.read() {
        Some(Ok(r)) => r.name.clone().unwrap_or_else(|| r.id.clone()),
        _ => "Repository".to_string(),
    };

    // Handle file selection via input
    let handle_file_select = move |_evt: Event<FormData>| {
        spawn(async move {
            error_message.set(None);
            match read_files_from_input(input_id).await {
                Ok(files) => {
                    if !files.is_empty() {
                        let mut current = selected_files.write();
                        for mut file in files {
                            let id = *next_file_id.peek();
                            file.id = id;
                            next_file_id.set(id + 1);
                            current.push(file);
                        }
                    }
                    clear_file_input(input_id);
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to read files: {}", e)));
                    clear_file_input(input_id);
                }
            }
        });
    };

    // Handle removing a single file by its stable ID
    let mut handle_remove_file = move |file_id: u64| {
        selected_files.write().retain(|f| f.id != file_id);
    };

    // Handle clearing all files
    let handle_clear_all = move |_| {
        selected_files.write().clear();
        error_message.set(None);
        clear_file_input(input_id);
    };

    // Handle upload
    let handle_upload = move |_| {
        let files_to_upload: Vec<SelectedFile> = selected_files.read().clone();
        if files_to_upload.is_empty() {
            return;
        }
        let server_url = selected_server.read().clone();
        uploading.set(true);
        current_file_index.set(0);
        error_message.set(None);

        spawn(async move {
            let mut results = upload_results.write().clone();
            let total = files_to_upload.len();
            let mut all_succeeded = true;

            for (i, file) in files_to_upload.into_iter().enumerate() {
                current_file_index.set(i);
                let quality = 100u8; // quality=100 bypasses image compression, safe for all file types
                let file_size = file.data.len();
                match blossom_store::upload_image(
                    file.data,
                    file.mime_type,
                    quality,
                    Some(server_url.clone()),
                )
                .await
                {
                    Ok(url) => {
                        log::info!("Uploaded file {}/{}: {} -> {}", i + 1, total, file.name, url);
                        results.push(UploadResult {
                            id: file.id,
                            name: file.name.clone(),
                            url,
                            size: file_size,
                        });
                    }
                    Err(e) => {
                        log::error!("Failed to upload {}: {}", file.name, e);
                        error_message.set(Some(format!("Failed to upload '{}': {}", file.name, e)));
                        all_succeeded = false;
                        break;
                    }
                }
            }

            *upload_results.write() = results;
            if all_succeeded {
                selected_files.write().clear();
                clear_file_input(input_id);
            }
            uploading.set(false);
        });
    };

    // Handle copy URL
    let handle_copy = move |index: usize| {
        let results = upload_results.read();
        if let Some(result) = results.get(index) {
            let url = result.url.clone();
            spawn(async move {
                if copy_to_clipboard(&url).await.is_ok() {
                    copied_index.set(Some(index));
                    // Reset copied state after 2 seconds
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    if *copied_index.read() == Some(index) {
                        copied_index.set(None);
                    }
                }
            });
        }
    };

    let files_count = selected_files.read().len();
    let total_size: usize = selected_files.read().iter().map(|f| f.data.len()).sum();
    let has_results = !upload_results.read().is_empty();
    let files_plural = if files_count != 1 { "s" } else { "" };
    let total_size_display = format_size(total_size);
    let current_file_display = *current_file_index.read() + 1;

    rsx! {
        div { class: "min-h-screen",
            // Header
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-3",
                    Link {
                        to: Route::CodeRepo {
                            naddr: naddr.clone(),
                        },
                        class: "text-muted-foreground hover:text-foreground transition",
                        dangerous_inner_html: icons::ARROW_LEFT,
                    }
                    h1 { class: "text-xl font-bold flex items-center gap-2",
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                            polyline { points: "17 8 12 3 7 8" }
                            line { x1: "12", y1: "3", x2: "12", y2: "15" }
                        }
                        "Upload Files"
                    }
                }
            }

            div { class: "p-4 space-y-6 max-w-3xl mx-auto",
                // Repository context
                if repo_result.read().is_none() {
                    div { class: "p-4 bg-muted rounded-lg animate-pulse",
                        div { class: "h-4 bg-muted-foreground/20 rounded w-48" }
                    }
                } else {
                    div { class: "p-4 bg-muted rounded-lg",
                        p { class: "text-sm text-muted-foreground",
                            "Uploading files for "
                            span { class: "font-medium text-foreground", "{repo_name}" }
                        }
                    }
                }

                // Error message
                if let Some(error) = error_message.read().as_ref() {
                    div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{error}"
                    }
                }

                // Drop zone / file selector (only show when not uploading)
                if !*uploading.read() {
                    div {
                        class: {
                            let base = "border-2 border-dashed rounded-lg p-12 text-center transition cursor-pointer";
                            if *is_dragging.read() {
                                format!("{} border-primary bg-primary/5", base)
                            } else {
                                format!("{} border-border hover:border-muted-foreground", base)
                            }
                        },
                        ondragover: move |e| {
                            e.prevent_default();
                            is_dragging.set(true);
                        },
                        ondragenter: move |e| {
                            e.prevent_default();
                            is_dragging.set(true);
                        },
                        ondragleave: move |_| {
                            is_dragging.set(false);
                        },
                        ondrop: move |evt: Event<DragData>| async move {
                            evt.prevent_default();
                            is_dragging.set(false);
                            let files = evt.files();
                            if files.is_empty() {
                                return;
                            }
                            let mut new_files = Vec::new();
                            let mut failed_names: Vec<String> = Vec::new();
                            for file in files {
                                let name = file.name();
                                let mime_type = file.content_type()
                                    .unwrap_or_else(|| "application/octet-stream".to_string());
                                match file.read_bytes().await {
                                    Ok(bytes) => {
                                        let id = *next_file_id.peek();
                                        next_file_id.set(id + 1);
                                        new_files.push(SelectedFile {
                                            id,
                                            name,
                                            data: bytes.to_vec(),
                                            mime_type,
                                        });
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to read dropped file {}: {:?}", name, e);
                                        failed_names.push(name);
                                    }
                                }
                            }
                            if !failed_names.is_empty() {
                                error_message.set(Some(format!("Failed to read: {}", failed_names.join(", "))));
                            }
                            if !new_files.is_empty() {
                                selected_files.write().extend(new_files);
                                if failed_names.is_empty() {
                                    error_message.set(None);
                                }
                            }
                        },
                        onclick: move |_| {
                            // Programmatically click the hidden file input
                            trigger_file_input(input_id);
                        },
                        div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
                            svg {
                                class: "w-8 h-8 text-muted-foreground",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                                polyline { points: "17 8 12 3 7 8" }
                                line { x1: "12", y1: "3", x2: "12", y2: "15" }
                            }
                        }
                        if *is_dragging.read() {
                            h3 { class: "text-lg font-semibold mb-2 text-primary", "Drop files here" }
                        } else {
                            h3 { class: "text-lg font-semibold mb-2", "Drag and drop files here" }
                        }
                        p { class: "text-muted-foreground text-sm mb-4", "Or click to browse files from your computer" }
                        p { class: "text-muted-foreground/60 text-xs", "All file types supported" }
                        // Hidden file input
                        input {
                            id: input_id,
                            class: "hidden",
                            r#type: "file",
                            accept: "*/*",
                            multiple: true,
                            onchange: handle_file_select,
                            // Stop click propagation so the div onclick doesn't re-trigger
                            onclick: move |e| e.stop_propagation(),
                        }
                    }
                }

                // Selected files list
                if files_count > 0 && !*uploading.read() {
                    div { class: "bg-card border border-border rounded-lg",
                        div { class: "p-3 border-b border-border flex items-center justify-between",
                            p { class: "text-sm font-medium",
                                "{files_count} file{files_plural} selected"
                                span { class: "text-muted-foreground ml-2",
                                    "({total_size_display})"
                                }
                            }
                            button {
                                class: "text-xs text-destructive hover:text-destructive/80 transition",
                                onclick: handle_clear_all,
                                "Clear all"
                            }
                        }
                        div { class: "divide-y divide-border",
                            for file in selected_files.read().iter() {
                                {
                                    let file_id = file.id;
                                    let file_name = file.name.clone();
                                    let file_size_display = format_size(file.data.len());
                                    let file_ext = get_file_extension(&file_name);
                                    let file_type = file.mime_type.clone();
                                    rsx! {
                                        div { key: "{file_id}", class: "p-3 flex items-center gap-3",
                                            // File icon
                                            div { class: "w-8 h-8 rounded bg-muted flex items-center justify-center shrink-0",
                                                span { class: "text-xs text-muted-foreground font-mono",
                                                    "{file_ext}"
                                                }
                                            }
                                            div { class: "flex-1 min-w-0",
                                                p { class: "text-sm font-medium truncate", "{file_name}" }
                                                p { class: "text-xs text-muted-foreground",
                                                    "{file_size_display}"
                                                    if !file_type.is_empty() {
                                                        " \u{00B7} {file_type}"
                                                    }
                                                }
                                            }
                                            button {
                                                class: "p-1 text-muted-foreground hover:text-destructive hover:bg-accent rounded transition",
                                                onclick: move |_| handle_remove_file(file_id),
                                                svg {
                                                    class: "w-4 h-4",
                                                    xmlns: "http://www.w3.org/2000/svg",
                                                    width: "24",
                                                    height: "24",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    line { x1: "18", y1: "6", x2: "6", y2: "18" }
                                                    line { x1: "6", y1: "6", x2: "18", y2: "18" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Server selector
                if files_count > 0 && !*uploading.read() {
                    div { class: "space-y-2",
                        label { class: "block text-sm font-medium", "Upload to" }
                        select {
                            class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            onchange: move |evt| {
                                selected_server.set(evt.value());
                            },
                            {
                                let servers = blossom_store::get_servers();
                                let current_server = selected_server.read().clone();
                                rsx! {
                                    for server in servers.iter() {
                                        option {
                                            value: "{server}",
                                            selected: *server == current_server,
                                            "{display_server_url(server)}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Upload button / progress
                if files_count > 0 || *uploading.read() {
                    if *uploading.read() {
                        div { class: "bg-card border border-border rounded-lg p-4 space-y-3",
                            div { class: "flex items-center gap-3",
                                svg {
                                    class: "w-5 h-5 animate-spin text-primary shrink-0",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    circle {
                                        class: "opacity-25",
                                        cx: "12",
                                        cy: "12",
                                        r: "10",
                                        stroke: "currentColor",
                                        stroke_width: "4",
                                    }
                                    path {
                                        class: "opacity-75",
                                        fill: "currentColor",
                                        d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z",
                                    }
                                }
                                p { class: "text-sm font-medium",
                                    "Uploading file {current_file_display} of {files_count}..."
                                }
                            }
                            if let Some(progress) = *upload_progress {
                                div { class: "w-full bg-muted rounded-full h-2 overflow-hidden",
                                    div {
                                        class: "bg-primary h-2 rounded-full transition-all duration-300",
                                        style: "width: {progress}%",
                                    }
                                }
                                p { class: "text-xs text-muted-foreground text-right", "{progress:.0}%" }
                            }
                        }
                    } else {
                        button {
                            class: "w-full px-4 py-2.5 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition font-medium flex items-center justify-center gap-2",
                            onclick: handle_upload,
                            svg {
                                class: "w-4 h-4",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                                polyline { points: "17 8 12 3 7 8" }
                                line { x1: "12", y1: "3", x2: "12", y2: "15" }
                            }
                            "Upload {files_count} File{files_plural}"
                        }
                    }
                }

                // Upload results
                if has_results {
                    div { class: "space-y-3",
                        h3 { class: "text-sm font-medium flex items-center gap-2",
                            svg {
                                class: "w-4 h-4 text-green-400",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M22 11.08V12a10 10 0 1 1-5.93-9.14" }
                                polyline { points: "22 4 12 14.01 9 11.01" }
                            }
                            "Uploaded Files"
                        }
                        div { class: "bg-green-500/10 border border-green-500/20 rounded-lg divide-y divide-green-500/10",
                            for (i, result) in upload_results.read().iter().enumerate() {
                                {
                                    let result_name = result.name.clone();
                                    let result_url = result.url.clone();
                                    let result_size = result.size;
                                    let is_copied = *copied_index.read() == Some(i);
                                    let copy_btn_class = if is_copied {
                                        "px-3 py-1 text-xs rounded-md transition shrink-0 bg-green-500/20 text-green-400"
                                    } else {
                                        "px-3 py-1 text-xs rounded-md transition shrink-0 bg-muted hover:bg-accent text-muted-foreground"
                                    };
                                    let size_display = format_size(result_size);
                                    rsx! {
                                        div { class: "p-3 space-y-2",
                                            div { class: "flex items-center justify-between",
                                                div {
                                                    p { class: "text-sm font-medium text-green-400", "{result_name}" }
                                                    p { class: "text-xs text-muted-foreground", "{size_display}" }
                                                }
                                                button {
                                                    class: "{copy_btn_class}",
                                                    onclick: move |_| handle_copy(i),
                                                    if is_copied {
                                                        "Copied!"
                                                    } else {
                                                        "Copy URL"
                                                    }
                                                }
                                            }
                                            p { class: "text-xs text-muted-foreground font-mono break-all select-all",
                                                "{result_url}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Info note about Blossom
                div { class: "p-4 bg-blue-500/10 rounded-lg border border-blue-500/20",
                    div { class: "flex items-start gap-3",
                        div { class: "w-8 h-8 rounded-lg bg-blue-500/20 flex items-center justify-center shrink-0",
                            svg {
                                class: "w-4 h-4 text-blue-500",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                circle { cx: "12", cy: "12", r: "10" }
                                path { d: "M12 16v-4" }
                                path { d: "M12 8h.01" }
                            }
                        }
                        div {
                            p { class: "text-sm",
                                span { class: "font-medium", "Blossom File Storage" }
                                span { class: "text-muted-foreground",
                                    " - Files are uploaded to Blossom servers and are not stored in the git repository. You can reference uploaded files using their URLs in issues, pull requests, and README files."
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Read multiple files from a file input element
#[cfg(target_arch = "wasm32")]
async fn read_files_from_input(input_id: &str) -> Result<Vec<SelectedFile>, String> {
    let window = web_sys::window().ok_or("No window")?;
    let document = window.document().ok_or("No document")?;
    let input = document
        .get_element_by_id(input_id)
        .ok_or("Input not found")?
        .dyn_into::<HtmlInputElement>()
        .map_err(|_| "Not an input element")?;
    let file_list = input.files().ok_or("No files")?;
    let count = file_list.length();

    let mut files = Vec::new();
    for i in 0..count {
        let file = file_list.get(i).ok_or("Failed to get file")?;
        let name = file.name();
        let mime_type = file.type_();
        let promise = file.array_buffer();
        let array_buffer = JsFuture::from(promise)
            .await
            .map_err(|_| format!("Failed to read file: {}", name))?;
        let array_buffer: ArrayBuffer = array_buffer
            .dyn_into()
            .map_err(|_| "Not an ArrayBuffer")?;
        let uint8_array = Uint8Array::new(&array_buffer);
        let bytes = uint8_array.to_vec();

        // Default to application/octet-stream if browser reports empty mime type
        let final_mime = if mime_type.is_empty() {
            "application/octet-stream".to_string()
        } else {
            mime_type
        };

        files.push(SelectedFile {
            id: 0, // Assigned by caller
            name,
            data: bytes,
            mime_type: final_mime,
        });
    }

    Ok(files)
}

#[cfg(not(target_arch = "wasm32"))]
async fn read_files_from_input(_input_id: &str) -> Result<Vec<SelectedFile>, String> {
    Err("File reading is only supported in WASM".to_string())
}

/// Programmatically click a file input element
#[cfg(target_arch = "wasm32")]
fn trigger_file_input(input_id: &str) {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(element) = document.get_element_by_id(input_id) {
                if let Ok(html_element) = element.dyn_into::<HtmlElement>() {
                    html_element.click();
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn trigger_file_input(_input_id: &str) {}

/// Clear a file input element's value
#[cfg(target_arch = "wasm32")]
fn clear_file_input(input_id: &str) {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(element) = document.get_element_by_id(input_id) {
                if let Ok(input) = element.dyn_into::<HtmlInputElement>() {
                    input.set_value("");
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn clear_file_input(_input_id: &str) {}

/// Format bytes as human-readable string
fn format_size(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;
    const GB: usize = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Get file extension for display in the file icon
fn get_file_extension(name: &str) -> String {
    match name.rfind('.') {
        Some(pos) if pos > 0 && pos < name.len() - 1 => {
            let ext = &name[pos + 1..];
            if ext.len() <= 4 {
                ext.to_uppercase()
            } else {
                ext.chars().take(4).collect::<String>().to_uppercase()
            }
        }
        _ => "FILE".to_string(),
    }
}

#[component]
fn NotAuthenticatedState(naddr: String) -> Element {
    rsx! {
        div { class: "min-h-screen flex items-center justify-center p-4",
            div { class: "text-center max-w-md",
                div { class: "w-20 h-20 mx-auto mb-6 rounded-full bg-muted flex items-center justify-center",
                    svg {
                        class: "w-10 h-10 text-muted-foreground",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                        circle { cx: "12", cy: "7", r: "4" }
                    }
                }
                h2 { class: "font-semibold text-xl mb-2", "Sign In Required" }
                p { class: "text-muted-foreground mb-6",
                    "Connect with your Nostr identity to upload files."
                }
                Link {
                    to: Route::CodeRepo { naddr },
                    class: "text-primary hover:underline",
                    "Back to Repository"
                }
            }
        }
    }
}
