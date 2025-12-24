//! Code File Viewer Component
//!
//! Displays file content with syntax highlighting and line numbers.
//! Supports copy-to-clipboard and shows file metadata.

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

/// Extract file extension from filename
/// Returns empty string for extensionless files or dotfiles (e.g., ".gitignore")
fn extract_extension(filename: &str) -> &str {
    filename
        .rsplit_once('.')
        .filter(|(name, _)| !name.is_empty())
        .map(|(_, ext)| ext)
        .unwrap_or("")
}

/// Language detection from file extension
fn detect_language(filename: &str) -> &'static str {
    // Special case: match full filename for extensionless special files
    // These files don't have extensions, so we check the basename first
    let basename = filename.rsplit('/').next().unwrap_or(filename);
    match basename.to_lowercase().as_str() {
        "dockerfile" => return "dockerfile",
        "makefile" | "gnumakefile" => return "makefile",
        _ => {}
    }

    match extract_extension(filename) {
        "rs" => "rust",
        "js" | "mjs" => "javascript",
        "jsx" => "jsx",
        "ts" => "typescript",
        "tsx" => "tsx",
        "py" => "python",
        "rb" => "ruby",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "c" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "h" => "cpp",
        "cs" => "csharp",
        "php" => "php",
        "sh" | "bash" | "zsh" => "bash",
        "sql" => "sql",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "md" | "mdx" => "markdown",
        "vue" => "vue",
        "svelte" => "svelte",
        _ => "plaintext",
    }
}

/// Check if file is likely binary based on extension
fn is_binary_extension(filename: &str) -> bool {
    matches!(
        extract_extension(filename),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" |
        "pdf" | "doc" | "docx" | "xls" | "xlsx" |
        "zip" | "tar" | "gz" | "rar" | "7z" |
        "exe" | "dll" | "so" | "dylib" |
        "mp3" | "mp4" | "wav" | "avi" | "mov" |
        "woff" | "woff2" | "ttf" | "otf" | "eot" |
        "wasm"
    )
}

/// File viewer loading skeleton
#[component]
pub fn CodeFileViewerSkeleton() -> Element {
    rsx! {
        div {
            class: "animate-pulse",

            // Header skeleton
            div {
                class: "flex items-center justify-between p-3 border-b border-border",
                div { class: "h-5 w-32 bg-muted rounded" }
                div { class: "h-8 w-20 bg-muted rounded" }
            }

            // Content skeleton
            div {
                class: "p-4 space-y-2",
                for i in 0..20 {
                    div {
                        key: "{i}",
                        class: "flex gap-4",
                        div { class: "w-8 h-4 bg-muted rounded" }
                        div {
                            class: "h-4 bg-muted rounded",
                            style: "width: {40 + (i * 7) % 50}%"
                        }
                    }
                }
            }
        }
    }
}

/// Main file viewer component
#[component]
pub fn CodeFileViewer(
    content: String,
    filename: String,
    #[props(default = "".to_string())] git_ref: String,
) -> Element {
    let mut copied = use_signal(|| false);
    let language = detect_language(&filename);
    let is_binary = is_binary_extension(&filename);

    // Handle binary files
    if is_binary {
        return rsx! {
            div {
                class: "border border-border rounded-lg overflow-hidden",

                // Header
                div {
                    class: "flex items-center justify-between px-4 py-3 bg-muted/50 border-b border-border",

                    div {
                        class: "flex items-center gap-2",
                        span {
                            class: "text-sm font-medium",
                            "{filename}"
                        }
                        span {
                            class: "text-xs text-muted-foreground px-2 py-0.5 bg-muted rounded",
                            "Binary"
                        }
                    }
                }

                // Binary file message
                div {
                    class: "flex flex-col items-center justify-center py-12 text-muted-foreground",
                    svg {
                        class: "w-12 h-12 mb-4",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                        polyline { points: "14 2 14 8 20 8" }
                    }
                    p {
                        class: "text-sm",
                        "Binary file not shown"
                    }
                }
            }
        };
    }

    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len();
    let line_number_width = line_count.to_string().len().max(2);

    rsx! {
        div {
            class: "border border-border rounded-lg overflow-hidden",

            // Header with file info and actions
            div {
                class: "flex items-center justify-between px-4 py-3 bg-muted/50 border-b border-border",

                div {
                    class: "flex items-center gap-2",
                    span {
                        class: "text-sm font-medium",
                        "{filename}"
                    }
                    span {
                        class: "text-xs text-muted-foreground px-2 py-0.5 bg-muted rounded",
                        "{language}"
                    }
                    span {
                        class: "text-xs text-muted-foreground",
                        "{line_count} lines"
                    }
                }

                // Copy button
                button {
                    class: "flex items-center gap-1 px-3 py-1.5 text-sm bg-muted hover:bg-accent rounded transition",
                    onclick: {
                        let content = content.clone();
                        move |_| {
                            let content = content.clone();
                            spawn(async move {
                                if let Some(window) = web_sys::window() {
                                    let clipboard = window.navigator().clipboard();
                                    let _ = wasm_bindgen_futures::JsFuture::from(
                                        clipboard.write_text(&content)
                                    ).await;
                                    copied.set(true);
                                    // Reset after 2 seconds
                                    gloo_timers::future::TimeoutFuture::new(2000).await;
                                    copied.set(false);
                                }
                            });
                        }
                    },

                    if copied() {
                        svg {
                            class: "w-4 h-4 text-green-500",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            polyline { points: "20 6 9 17 4 12" }
                        }
                        span { "Copied!" }
                    } else {
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
                            rect { x: "9", y: "9", width: "13", height: "13", rx: "2", ry: "2" }
                            path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
                        }
                        span { "Copy" }
                    }
                }
            }

            // Code content with line numbers
            div {
                class: "overflow-x-auto",

                pre {
                    class: "text-sm font-mono leading-relaxed",

                    code {
                        class: "block",

                        table {
                            class: "w-full border-collapse",

                            tbody {
                                for (i, line) in lines.iter().enumerate() {
                                    tr {
                                        key: "{i}",
                                        class: "hover:bg-accent/30 transition-colors",

                                        // Line number
                                        td {
                                            class: "select-none text-right text-muted-foreground px-3 py-0 border-r border-border/50 bg-muted/30 sticky left-0",
                                            style: "width: {line_number_width + 2}ch; min-width: {line_number_width + 2}ch",
                                            "{i + 1}"
                                        }

                                        // Code line
                                        td {
                                            class: "px-4 py-0 whitespace-pre",
                                            "{line}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Compact code viewer for inline display (e.g., README preview)
#[component]
pub fn CodeFileViewerCompact(
    content: String,
    #[props(default = 10)] max_lines: usize,
) -> Element {
    // Collect once to avoid iterating the content twice
    let all_lines: Vec<&str> = content.lines().collect();
    let total_lines = all_lines.len();
    let truncated = total_lines > max_lines;
    let lines: Vec<&str> = all_lines.into_iter().take(max_lines).collect();

    rsx! {
        div {
            class: "border border-border rounded-lg overflow-hidden",

            pre {
                class: "text-sm font-mono leading-relaxed p-4 overflow-x-auto bg-muted/20",

                code {
                    for (i, line) in lines.iter().enumerate() {
                        div {
                            key: "{i}",
                            class: "whitespace-pre",
                            "{line}"
                        }
                    }

                    if truncated {
                        div {
                            class: "text-muted-foreground mt-2",
                            "... {total_lines - max_lines} more lines"
                        }
                    }
                }
            }
        }
    }
}

/// Raw file download button
#[component]
pub fn RawFileButton(
    content: String,
    filename: String,
) -> Element {
    rsx! {
        button {
            class: "flex items-center gap-1 px-3 py-1.5 text-sm bg-muted hover:bg-accent rounded transition",
            onclick: {
                let content = content.clone();
                let filename = filename.clone();
                move |_| {
                    let content = content.clone();
                    let filename = filename.clone();
                    // Create blob and download
                    if let Some(window) = web_sys::window() {
                        if let Some(document) = window.document() {
                            let blob = web_sys::Blob::new_with_str_sequence(
                                &js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(&content))
                            ).ok();

                            if let Some(blob) = blob {
                                let url = web_sys::Url::create_object_url_with_blob(&blob).ok();
                                if let Some(url) = url {
                                    if let Ok(a) = document.create_element("a") {
                                        let _ = a.set_attribute("href", &url);
                                        let _ = a.set_attribute("download", &filename);
                                        if let Some(body) = document.body() {
                                            let _ = body.append_child(&a);
                                            if let Some(html_a) = a.dyn_ref::<web_sys::HtmlElement>() {
                                                html_a.click();
                                            }
                                            let _ = body.remove_child(&a);
                                        }
                                        // Defer URL revocation to allow browser to start download
                                        let url_to_revoke = url.clone();
                                        spawn(async move {
                                            gloo_timers::future::TimeoutFuture::new(100).await;
                                            let _ = web_sys::Url::revoke_object_url(&url_to_revoke);
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            },

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
                polyline { points: "7 10 12 15 17 10" }
                line { x1: "12", y1: "15", x2: "12", y2: "3" }
            }
            span { "Raw" }
        }
    }
}
