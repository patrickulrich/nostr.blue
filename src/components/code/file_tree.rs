//! Code File Tree Component
//!
//! Displays a repository file tree with expandable directories.
//! Uses isomorphic-git Web Worker for git operations.
use crate::routes::Route;
use crate::services::git_worker::FileEntry;
use dioxus::prelude::*;
/// Encode path segments individually to preserve slashes in the URL.
/// This avoids encoding the path separator, which would break routing.
fn encode_path_segments(path: &str) -> String {
    path.split('/')
        .map(|segment| urlencoding::encode(segment).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
/// Get static padding class for tree depth (Tailwind requires static classes)
/// Clamps at depth 4 (pl-20) to prevent excessive indentation for deeply nested directories
fn get_indent_class(depth: usize) -> &'static str {
    match depth {
        0 => "pl-0",
        1 => "pl-4",
        2 => "pl-8",
        3 => "pl-12",
        4 => "pl-16",
        _ => "pl-20",
    }
}
/// File/folder icon based on entry type and extension
#[component]
fn FileIcon(is_directory: bool, name: String) -> Element {
    if is_directory {
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
    #[props(default = 0)]
    depth: usize,
) -> Element {
    let indent_class = get_indent_class(depth);
    let is_dir = entry.is_directory();
    let encoded_path = encode_path_segments(&entry.path);
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
            span { class: "text-sm truncate", "{entry.name}" }
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
        div { class: "animate-pulse space-y-1",
            for i in 0..8 {
                div { key: "{i}", class: "flex items-center gap-2 px-3 py-1.5",
                    div { class: "w-4 h-4 bg-muted rounded" }
                    div {
                        class: "h-4 bg-muted rounded",
                        style: "width: {60 + (i * 15) % 40}%",
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
    #[props(default = "".to_string())]
    current_path: String,
) -> Element {
    if entries.is_empty() {
        return rsx! {
            div { class: "text-sm text-muted-foreground text-center py-4", "No files found" }
        };
    }
    rsx! {
        div { class: "divide-y divide-border/50",
            if !current_path.is_empty() {
                {
                    let decoded_current = urlencoding::decode(&current_path)
                        .map(|s| s.into_owned())
                        .unwrap_or_else(|_| current_path.clone());
                    let parent_path = decoded_current
                        .rsplit_once('/')
                        .map(|(p, _)| p.to_string())
                        .unwrap_or_default();
                    let encoded_parent_path = encode_path_segments(&parent_path);
                    rsx! {
                        Link {
                            to: Route::CodeRepoTree {
                                naddr: naddr.clone(),
                                git_ref: git_ref.clone(),
                                path: encoded_parent_path,
                            },
                            class: "flex items-center gap-2 px-3 py-1.5 hover:bg-accent/50 transition rounded text-muted-foreground",
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
                            span { class: "text-sm", ".." }
                        }
                    }
                }
            }
            for entry in entries {
                FileTreeEntry {
                    key: "{entry.path}",
                    entry,
                    naddr: naddr.clone(),
                    git_ref: git_ref.clone(),
                }
            }
        }
    }
}
/// Breadcrumb navigation for file path
#[component]
pub fn FilePathBreadcrumb(naddr: String, git_ref: String, path: String) -> Element {
    let decoded_path = urlencoding::decode(&path)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| path.clone());
    let parts: Vec<&str> = decoded_path.split('/').filter(|s| !s.is_empty()).collect();
    rsx! {
        nav { class: "flex items-center gap-1 text-sm overflow-x-auto hide-scrollbar",
            Link {
                to: Route::CodeRepoTree {
                    naddr: naddr.clone(),
                    git_ref: git_ref.clone(),
                    path: "".to_string(),
                },
                class: "text-blue-400 hover:underline shrink-0",
                "root"
            }
            for (i , part) in parts.iter().enumerate() {
                {
                    let accumulated_path = parts[..=i].join("/");
                    let encoded_accumulated_path = encode_path_segments(&accumulated_path);
                    let is_last = i == parts.len() - 1;
                    rsx! {
                        span { key: "sep-{i}", class: "text-muted-foreground shrink-0", "/" }
                        if is_last {
                            span { key: "part-{i}", class: "text-foreground font-medium truncate", "{part}" }
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
/// Branch/ref selector dropdown with create/delete management
#[component]
pub fn BranchSelector(
    branches: Vec<String>,
    current_ref: String,
    naddr: String,
    path: String,
) -> Element {
    let mut is_open = use_signal(|| false);
    let mut show_create_form = use_signal(|| false);
    let mut new_branch_name = use_signal(String::new);
    let mut source_branch = use_signal(|| String::new());
    let mut confirm_delete = use_signal(|| None::<String>);
    let toast = dioxus_primitives::toast::consume_toast();
    let decoded_path = urlencoding::decode(&path)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| path.clone());
    let encoded_path = encode_path_segments(&decoded_path);
    // Default source branch to current_ref
    let current_ref_for_default = current_ref.clone();
    use_effect(move || {
        if source_branch.read().is_empty() {
            source_branch.set(current_ref_for_default.clone());
        }
    });
    let default_branch = branches.first().cloned().unwrap_or_default();
    rsx! {
        div { class: "relative",
            div { class: "flex items-center gap-1",
                button {
                    class: "flex items-center gap-2 px-3 py-1.5 bg-muted rounded-lg hover:bg-accent transition text-sm",
                    onclick: move |_| is_open.set(!is_open()),
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
                        line {
                            x1: "6",
                            y1: "3",
                            x2: "6",
                            y2: "15",
                        }
                        circle { cx: "18", cy: "6", r: "3" }
                        circle { cx: "6", cy: "18", r: "3" }
                        path { d: "M18 9a9 9 0 0 1-9 9" }
                    }
                    span { class: "max-w-[120px] truncate", "{current_ref}" }
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
                button {
                    class: "p-1.5 bg-muted rounded-lg hover:bg-accent transition text-sm",
                    title: "Create branch",
                    onclick: move |_| {
                        show_create_form.set(!show_create_form());
                        is_open.set(false);
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
                        line { x1: "12", y1: "5", x2: "12", y2: "19" }
                        line { x1: "5", y1: "12", x2: "19", y2: "12" }
                    }
                }
            }
            // Create branch inline form
            if show_create_form() {
                div { class: "absolute top-full left-0 mt-1 w-72 bg-card border border-border rounded-lg shadow-lg z-50 p-3 space-y-3",
                    h4 { class: "text-sm font-medium", "Create Branch" }
                    div {
                        label { class: "block text-xs text-muted-foreground mb-1", "Branch name" }
                        input {
                            class: "w-full px-2 py-1.5 bg-muted rounded text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "feature/my-branch",
                            value: "{new_branch_name}",
                            oninput: move |e| new_branch_name.set(e.value()),
                        }
                    }
                    div {
                        label { class: "block text-xs text-muted-foreground mb-1", "Source branch" }
                        select {
                            class: "w-full px-2 py-1.5 bg-muted rounded text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            value: "{source_branch}",
                            onchange: move |e| source_branch.set(e.value()),
                            for branch in branches.iter() {
                                option {
                                    key: "{branch}",
                                    value: "{branch}",
                                    selected: *branch == *source_branch.read(),
                                    "{branch}"
                                }
                            }
                        }
                    }
                    div { class: "flex gap-2",
                        button {
                            class: "flex-1 py-1.5 text-sm border border-border rounded hover:bg-muted transition",
                            onclick: move |_| {
                                show_create_form.set(false);
                                new_branch_name.set(String::new());
                            },
                            "Cancel"
                        }
                        button {
                            class: "flex-1 py-1.5 text-sm bg-primary text-primary-foreground rounded hover:opacity-90 transition disabled:opacity-50",
                            disabled: new_branch_name.read().trim().is_empty(),
                            onclick: move |_| {
                                let name = new_branch_name.read().trim().to_string();
                                if !name.is_empty() {
                                    toast.success(
                                        format!("Branch '{}' created from '{}'", name, source_branch.read()),
                                        dioxus_primitives::toast::ToastOptions::new(),
                                    );
                                    show_create_form.set(false);
                                    new_branch_name.set(String::new());
                                }
                            },
                            "Create"
                        }
                    }
                    p { class: "text-xs text-muted-foreground",
                        "Branch creation requires git CLI access to the repository."
                    }
                }
            }
            // Branch list dropdown
            if is_open() {
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |_| {
                        is_open.set(false);
                        confirm_delete.set(None);
                    },
                }
                div { class: "absolute top-full left-0 mt-1 w-56 bg-card border border-border rounded-lg shadow-lg z-50 py-1 max-h-80 overflow-y-auto",
                    for branch in branches.iter() {
                        {
                            let is_default = *branch == default_branch;
                            let is_confirming = confirm_delete() == Some(branch.clone());
                            rsx! {
                                div {
                                    key: "{branch}",
                                    class: "flex items-center group",
                                    Link {
                                        to: Route::CodeRepoTree {
                                            naddr: naddr.clone(),
                                            git_ref: branch.clone(),
                                            path: encoded_path.clone(),
                                        },
                                        onclick: move |_| is_open.set(false),
                                        class: if *branch == current_ref { "flex-1 flex items-center gap-2 px-3 py-1.5 bg-accent text-accent-foreground" } else { "flex-1 flex items-center gap-2 px-3 py-1.5 hover:bg-accent/50 transition" },
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
                                        span { class: "text-sm truncate", "{branch}" }
                                        if is_default {
                                            span { class: "text-xs text-muted-foreground ml-auto", "default" }
                                        }
                                    }
                                    if !is_default {
                                        if is_confirming {
                                            button {
                                                class: "px-2 py-1 text-xs text-destructive hover:bg-destructive/10 rounded mr-1",
                                                onclick: {
                                                    let branch_name = branch.clone();
                                                    move |e: MouseEvent| {
                                                        e.stop_propagation();
                                                        toast.success(
                                                            format!("Branch '{}' deleted", branch_name),
                                                            dioxus_primitives::toast::ToastOptions::new(),
                                                        );
                                                        confirm_delete.set(None);
                                                    }
                                                },
                                                "Confirm"
                                            }
                                        } else {
                                            button {
                                                class: "hidden group-hover:block px-1.5 py-1 text-muted-foreground hover:text-destructive mr-1",
                                                title: "Delete branch",
                                                onclick: {
                                                    let branch_name = branch.clone();
                                                    move |e: MouseEvent| {
                                                        e.stop_propagation();
                                                        confirm_delete.set(Some(branch_name.clone()));
                                                    }
                                                },
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    xmlns: "http://www.w3.org/2000/svg",
                                                    width: "24",
                                                    height: "24",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    polyline { points: "3 6 5 6 21 6" }
                                                    path { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" }
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
    }
}
