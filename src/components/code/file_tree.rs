//! Code File Tree Component
//!
//! Displays a repository file tree with expandable directories.
//! Uses isomorphic-git Web Worker for git operations.

use dioxus::prelude::*;
use crate::services::git_worker::FileEntry;
use crate::routes::Route;

/// Get static padding class for tree depth (Tailwind requires static classes)
/// Clamps at depth 4 (pl-20) to prevent excessive indentation for deeply nested directories
fn get_indent_class(depth: usize) -> &'static str {
    match depth {
        0 => "pl-0",
        1 => "pl-4",
        2 => "pl-8",
        3 => "pl-12",
        4 => "pl-16",
        _ => "pl-20", // Clamped at pl-20 for depth > 4
    }
}

/// File/folder icon based on entry type and extension
#[component]
fn FileIcon(is_directory: bool, name: String) -> Element {
    if is_directory {
        // Folder icon
        rsx! {
            svg {
                class: "w-4 h-4 text-blue-400",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "currentColor",
                stroke: "none",
                path { d: "M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" }
            }
        }
    } else {
        // File icon - color based on extension
        let extension = name.rsplit('.').next().unwrap_or("");
        let icon_class = match extension {
            "rs" => "text-orange-400",
            "js" | "jsx" | "mjs" => "text-yellow-400",
            "ts" | "tsx" => "text-blue-400",
            "py" => "text-green-400",
            "go" => "text-cyan-400",
            "rb" => "text-red-400",
            "java" | "kt" => "text-orange-500",
            "c" | "cpp" | "h" | "hpp" => "text-blue-500",
            "json" | "yaml" | "yml" | "toml" => "text-purple-400",
            "md" | "mdx" => "text-gray-400",
            "html" | "htm" => "text-orange-400",
            "css" | "scss" | "sass" => "text-pink-400",
            "svg" | "png" | "jpg" | "jpeg" | "gif" | "webp" => "text-green-400",
            "sh" | "bash" | "zsh" => "text-green-500",
            _ => "text-muted-foreground",
        };

        rsx! {
            svg {
                class: "w-4 h-4 {icon_class}",
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
        }
    }
}

/// Single file/directory entry row
#[component]
pub fn FileTreeEntry(
    entry: FileEntry,
    naddr: String,
    git_ref: String,
    #[props(default = 0)] depth: usize,
) -> Element {
    let indent_class = get_indent_class(depth);
    let is_dir = entry.is_directory();

    // URL-encode the path to handle spaces and special characters
    let encoded_path = urlencoding::encode(&entry.path).into_owned();

    let route = if is_dir {
        Route::CodeRepoTree {
            naddr: naddr.clone(),
            git_ref: git_ref.clone(),
            path: encoded_path,
        }
    } else {
        Route::CodeRepoBlob {
            naddr: naddr.clone(),
            git_ref: git_ref.clone(),
            path: encoded_path,
        }
    };

    rsx! {
        Link {
            to: route,
            class: "flex items-center gap-2 px-3 py-1.5 hover:bg-accent/50 transition rounded {indent_class}",

            FileIcon { is_directory: is_dir, name: entry.name.clone() }

            span {
                class: "text-sm truncate",
                "{entry.name}"
            }

            // Chevron for directories
            if is_dir {
                svg {
                    class: "w-3 h-3 text-muted-foreground ml-auto",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    polyline { points: "9 18 15 12 9 6" }
                }
            }
        }
    }
}

/// File tree loading skeleton
#[component]
pub fn FileTreeSkeleton() -> Element {
    rsx! {
        div {
            class: "animate-pulse space-y-1",
            for i in 0..8 {
                div {
                    key: "{i}",
                    class: "flex items-center gap-2 px-3 py-1.5",
                    div { class: "w-4 h-4 bg-muted rounded" }
                    div {
                        class: "h-4 bg-muted rounded",
                        style: "width: {60 + (i * 15) % 40}%"
                    }
                }
            }
        }
    }
}

/// File tree container component
#[component]
pub fn CodeFileTree(
    entries: Vec<FileEntry>,
    naddr: String,
    git_ref: String,
    #[props(default = "".to_string())] current_path: String,
) -> Element {
    if entries.is_empty() {
        return rsx! {
            div {
                class: "text-sm text-muted-foreground text-center py-4",
                "No files found"
            }
        };
    }

    rsx! {
        div {
            class: "divide-y divide-border/50",

            // Parent directory link if not at root
            if !current_path.is_empty() {
                {
                    let parent_path = current_path
                        .rsplit_once('/')
                        .map(|(p, _)| p.to_string())
                        .unwrap_or_default();
                    let encoded_parent_path = urlencoding::encode(&parent_path).into_owned();

                    rsx! {
                        Link {
                            to: Route::CodeRepoTree {
                                naddr: naddr.clone(),
                                git_ref: git_ref.clone(),
                                path: encoded_parent_path,
                            },
                            class: "flex items-center gap-2 px-3 py-1.5 hover:bg-accent/50 transition rounded text-muted-foreground",

                            // Up arrow icon
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
                                path { d: "m5 12 7-7 7 7" }
                                path { d: "M12 19V5" }
                            }

                            span {
                                class: "text-sm",
                                ".."
                            }
                        }
                    }
                }
            }

            // File entries
            for entry in entries {
                FileTreeEntry {
                    key: "{entry.path}",
                    entry: entry,
                    naddr: naddr.clone(),
                    git_ref: git_ref.clone(),
                }
            }
        }
    }
}

/// Breadcrumb navigation for file path
#[component]
pub fn FilePathBreadcrumb(
    naddr: String,
    git_ref: String,
    path: String,
) -> Element {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    rsx! {
        nav {
            class: "flex items-center gap-1 text-sm overflow-x-auto hide-scrollbar",

            // Root link
            Link {
                to: Route::CodeRepoTree {
                    naddr: naddr.clone(),
                    git_ref: git_ref.clone(),
                    path: "".to_string(),
                },
                class: "text-blue-400 hover:underline shrink-0",
                "root"
            }

            // Path parts
            for (i, part) in parts.iter().enumerate() {
                {
                    let accumulated_path = parts[..=i].join("/");
                    let encoded_accumulated_path = urlencoding::encode(&accumulated_path).into_owned();
                    let is_last = i == parts.len() - 1;

                    rsx! {
                        // Separator
                        span {
                            key: "sep-{i}",
                            class: "text-muted-foreground shrink-0",
                            "/"
                        }

                        // Path segment
                        if is_last {
                            span {
                                key: "part-{i}",
                                class: "text-foreground font-medium truncate",
                                "{part}"
                            }
                        } else {
                            Link {
                                key: "part-{i}",
                                to: Route::CodeRepoTree {
                                    naddr: naddr.clone(),
                                    git_ref: git_ref.clone(),
                                    path: encoded_accumulated_path,
                                },
                                class: "text-blue-400 hover:underline truncate",
                                "{part}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Branch/ref selector dropdown
#[component]
pub fn BranchSelector(
    branches: Vec<String>,
    current_ref: String,
    naddr: String,
    path: String,
) -> Element {
    let mut is_open = use_signal(|| false);

    rsx! {
        div {
            class: "relative",

            // Trigger button
            button {
                class: "flex items-center gap-2 px-3 py-1.5 bg-muted rounded-lg hover:bg-accent transition text-sm",
                onclick: move |_| is_open.set(!is_open()),

                // Branch icon
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
                    line { x1: "6", y1: "3", x2: "6", y2: "15" }
                    circle { cx: "18", cy: "6", r: "3" }
                    circle { cx: "6", cy: "18", r: "3" }
                    path { d: "M18 9a9 9 0 0 1-9 9" }
                }

                span {
                    class: "max-w-[120px] truncate",
                    "{current_ref}"
                }

                // Chevron
                svg {
                    class: "w-3 h-3",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    polyline { points: "6 9 12 15 18 9" }
                }
            }

            // Dropdown menu with click-outside backdrop
            if is_open() {
                // Backdrop to close when clicking outside
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |_| is_open.set(false),
                }
                div {
                    class: "absolute top-full left-0 mt-1 w-48 bg-card border border-border rounded-lg shadow-lg z-50 py-1",

                    for branch in branches.iter() {
                        {
                            // Encode path to handle spaces and special characters
                            let encoded_path = urlencoding::encode(&path).into_owned();
                            rsx! {
                                Link {
                                    key: "{branch}",
                                    to: Route::CodeRepoTree {
                                        naddr: naddr.clone(),
                                        git_ref: branch.clone(),
                                        path: encoded_path.clone(),
                                    },
                                    onclick: move |_| is_open.set(false),
                                    class: if *branch == current_ref {
                                        "flex items-center gap-2 px-3 py-1.5 bg-accent text-accent-foreground"
                                    } else {
                                        "flex items-center gap-2 px-3 py-1.5 hover:bg-accent/50 transition"
                                    },

                                    if *branch == current_ref {
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
                                            polyline { points: "20 6 9 17 4 12" }
                                        }
                                    } else {
                                        div { class: "w-4" }
                                    }

                                    span {
                                        class: "text-sm truncate",
                                        "{branch}"
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
