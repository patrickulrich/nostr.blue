//! Code File Viewer Component
//!
//! Displays file content with language detection, line numbers, and copy-to-clipboard.
//! Shows file metadata including detected language label.
use dioxus::prelude::*;
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
    let basename = filename.rsplit('/').next().unwrap_or(filename);
    match basename.to_lowercase().as_str() {
        "dockerfile" => return "dockerfile",
        "makefile" | "gnumakefile" => return "makefile",
        _ => {}
    }
    let ext = extract_extension(filename).to_lowercase();
    match ext.as_str() {
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
    let ext = extract_extension(filename).to_lowercase();
    matches!(
        ext.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "ico"
            | "pdf"
            | "doc"
            | "docx"
            | "xls"
            | "xlsx"
            | "zip"
            | "tar"
            | "gz"
            | "rar"
            | "7z"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "mp3"
            | "mp4"
            | "wav"
            | "avi"
            | "mov"
            | "woff"
            | "woff2"
            | "ttf"
            | "otf"
            | "eot"
            | "wasm"
    )
}
/// File viewer loading skeleton
#[component]
pub fn CodeFileViewerSkeleton() -> Element {
    rsx! {
        div { class: "animate-pulse",
            div { class: "flex items-center justify-between p-3 border-b border-border",
                div { class: "h-5 w-32 bg-muted rounded" }
                div { class: "h-8 w-20 bg-muted rounded" }
            }
            div { class: "p-4 space-y-2",
                for i in 0..20 {
                    div { key: "{i}", class: "flex gap-4",
                        div { class: "w-8 h-4 bg-muted rounded" }
                        div {
                            class: "h-4 bg-muted rounded",
                            style: "width: {40 + (i * 7) % 50}%",
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
    let mut copy_error = use_signal(|| false);
    let language = detect_language(&filename);
    let is_binary = is_binary_extension(&filename);
    if is_binary {
        return rsx! {
            div { class: "border border-border rounded-lg overflow-hidden",
                div { class: "flex items-center justify-between px-4 py-3 bg-muted/50 border-b border-border",
                    div { class: "flex items-center gap-2",
                        span { class: "text-sm font-medium", "{filename}" }
                        span { class: "text-xs text-muted-foreground px-2 py-0.5 bg-muted rounded",
                            "Binary"
                        }
                    }
                }
                div { class: "flex flex-col items-center justify-center py-12 text-muted-foreground",
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
                    p { class: "text-sm", "Binary file not shown" }
                }
            }
        };
    }
    const MAX_RENDER_LINES: usize = 2000;
    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len();
    let line_number_width = line_count.to_string().len().max(2);
    let is_truncated = line_count > MAX_RENDER_LINES;
    let displayed_lines = if is_truncated {
        &lines[..MAX_RENDER_LINES]
    } else {
        &lines[..]
    };
    rsx! {
        div { class: "border border-border rounded-lg overflow-hidden",
            div { class: "flex items-center justify-between px-4 py-3 bg-muted/50 border-b border-border",
                div { class: "flex items-center gap-2",
                    span { class: "text-sm font-medium", "{filename}" }
                    span { class: "text-xs text-muted-foreground px-2 py-0.5 bg-muted rounded",
                        "{language}"
                    }
                    if !git_ref.is_empty() {
                        span { class: "text-xs text-muted-foreground px-2 py-0.5 bg-muted rounded",
                            "{git_ref}"
                        }
                    }
                    span { class: "text-xs text-muted-foreground", "{line_count} lines" }
                }
                button {
                    class: "flex items-center gap-1 px-3 py-1.5 text-sm bg-muted hover:bg-accent rounded transition",
                    onclick: {
                        let content = content.clone();
                        move |_| {
                            let content = content.clone();
                            spawn(async move {
                                match crate::platform::clipboard::copy_to_clipboard(&content).await {
                                    Ok(_) => {
                                        copy_error.set(false);
                                        copied.set(true);
                                        crate::platform::timer::sleep_ms(2000).await;
                                        copied.set(false);
                                    }
                                    Err(e) => {
                                        log::error!("Clipboard write failed: {}", e);
                                        copied.set(false);
                                        copy_error.set(true);
                                        crate::platform::timer::sleep_ms(2000).await;
                                        copy_error.set(false);
                                    }
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
                    } else if copy_error() {
                        svg {
                            class: "w-4 h-4 text-red-500",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle {
                                cx: "12",
                                cy: "12",
                                r: "10",
                            }
                            line {
                                x1: "12",
                                y1: "8",
                                x2: "12",
                                y2: "12",
                            }
                            line {
                                x1: "12",
                                y1: "16",
                                x2: "12.01",
                                y2: "16",
                            }
                        }
                        span { "Copy failed" }
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
                            rect {
                                x: "9",
                                y: "9",
                                width: "13",
                                height: "13",
                                rx: "2",
                                ry: "2",
                            }
                            path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
                        }
                        span { "Copy" }
                    }
                }
            }
            div { class: "overflow-x-auto",
                pre { class: "text-sm font-mono leading-relaxed",
                    code { class: "block",
                        table { class: "w-full border-collapse",
                            tbody {
                                for (i , line) in displayed_lines.iter().enumerate() {
                                    tr {
                                        key: "{i}",
                                        class: "hover:bg-accent/30 transition-colors",
                                        td {
                                            class: "select-none text-right text-muted-foreground px-3 py-0 border-r border-border/50 bg-muted/30 sticky left-0",
                                            style: "width: {line_number_width + 2}ch; min-width: {line_number_width + 2}ch",
                                            "{i + 1}"
                                        }
                                        td { class: "px-4 py-0 whitespace-pre", "{line}" }
                                    }
                                }
                            }
                        }
                    }
                }
                if is_truncated {
                    div { class: "px-4 py-3 bg-amber-500/10 border-t border-amber-500/20 text-amber-600 dark:text-amber-400 text-sm",
                        "⚠️ File truncated: showing first {MAX_RENDER_LINES} of {line_count} lines. Copy the content to view the full file."
                    }
                }
            }
        }
    }
}
/// Compact code viewer for inline display (e.g., README preview)
#[component]
pub fn CodeFileViewerCompact(content: String, #[props(default = 10)] max_lines: usize) -> Element {
    let all_lines: Vec<&str> = content.lines().collect();
    let total_lines = all_lines.len();
    let truncated = total_lines > max_lines;
    let lines: Vec<&str> = all_lines.into_iter().take(max_lines).collect();
    rsx! {
        div { class: "border border-border rounded-lg overflow-hidden",
            pre { class: "text-sm font-mono leading-relaxed p-4 overflow-x-auto bg-muted/20",
                code {
                    for (i , line) in lines.iter().enumerate() {
                        div { key: "{i}", class: "whitespace-pre", "{line}" }
                    }
                    if truncated {
                        div { class: "text-muted-foreground mt-2",
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
pub fn RawFileButton(content: String, filename: String) -> Element {
    let mut save_error = use_signal(|| false);
    rsx! {
        button {
            class: if save_error() {
                "flex items-center gap-1 px-3 py-1.5 text-sm bg-red-500/20 text-red-500 rounded transition"
            } else {
                "flex items-center gap-1 px-3 py-1.5 text-sm bg-muted hover:bg-accent rounded transition"
            },
            title: "",
            onclick: {
                let content = content.clone();
                let filename = filename.clone();
                move |_| {
                    save_error.set(false);
                    let _content = content.clone();
                    let _filename = filename.clone();
                    match crate::platform::download::save_file(
                        &_filename,
                        &_content,
                        "text/plain;charset=utf-8",
                    ) {
                        Ok(()) => save_error.set(false),
                        Err(e) if e.contains("Save cancelled") => {
                            save_error.set(false);
                        }
                        Err(e) => {
                            log::error!("Download failed for '{}': {}", _filename, e);
                            save_error.set(true);
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
                line {
                    x1: "12",
                    y1: "15",
                    x2: "12",
                    y2: "3",
                }
            }
            span { if save_error() { "Save failed" } else { "Raw" } }
        }
    }
}
