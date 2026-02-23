//! Code Repository Tree Page
//!
//! Displays the file tree for a repository at a specific path and ref.
use crate::components::code::FuzzyFinder;
use crate::components::{
    BranchSelector, CodeFileTree, FilePathBreadcrumb, FileTreeSkeleton,
};
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repository, file_fetcher, git_service};
use crate::stores::nostr_client;
use crate::utils::nip34::Repository;
use dioxus::prelude::*;
use dioxus_core::use_drop;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[component]
pub fn CodeRepoTree(naddr: String, git_ref: String, path: String) -> Element {
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut files = use_signal(Vec::new);
    let mut branches = use_signal(Vec::new);
    let mut repo_signal = use_signal(|| None::<Repository>);
    let mut show_fuzzy_finder = use_signal(|| false);
    let mut all_file_paths = use_signal(Vec::<String>::new);

    // Store cleanup function for "t" key listener removal
    #[allow(unused_variables, unused_mut)]
    let mut t_key_cleanup = use_signal(|| None::<(js_sys::Function, web_sys::Window)>);

    // Keyboard shortcut: press 't' to open fuzzy finder (one-time registration)
    use_hook(move || {
        #[cfg(target_arch = "wasm32")]
        {
            let window = web_sys::window().expect("no global window");
            let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
                // Skip if typing in an input, textarea, or select
                if let Some(target) = event.target() {
                    if let Some(element) = target.dyn_ref::<web_sys::HtmlElement>() {
                        let tag = element.tag_name().to_lowercase();
                        if tag == "input" || tag == "textarea" || tag == "select" {
                            return;
                        }
                        if element.is_content_editable() {
                            return;
                        }
                    }
                }
                if event.key() == "t"
                    && !event.ctrl_key()
                    && !event.meta_key()
                    && !event.alt_key()
                {
                    event.prevent_default();
                    show_fuzzy_finder.set(true);
                }
            }) as Box<dyn FnMut(_)>);

            let js_fn: js_sys::Function = closure.as_ref().unchecked_ref::<js_sys::Function>().clone();
            window.add_event_listener_with_callback("keydown", &js_fn).ok();
            t_key_cleanup.set(Some((js_fn, window.clone())));
            closure.forget();
        }
    });

    use_drop(move || {
        #[cfg(target_arch = "wasm32")]
        if let Some((func, win)) = t_key_cleanup.peek().as_ref() {
            win.remove_event_listener_with_callback("keydown", func).ok();
        }
    });

    // Clear cached file paths when repo or git_ref changes
    let finder_cache_key = use_memo(use_reactive((&naddr, &git_ref), |(n, g)| {
        format!("{}:{}", n, g)
    }));
    let mut prev_finder_key = use_signal(String::new);
    use_effect(move || {
        let current_key = finder_cache_key.read().clone();
        if *prev_finder_key.peek() != current_key {
            prev_finder_key.set(current_key);
            all_file_paths.set(Vec::new());
        }
    });

    // Fetch all file paths when the fuzzy finder is opened
    {
        let git_ref_for_finder = git_ref.clone();
        use_effect(move || {
            let is_open = *show_fuzzy_finder.read();
            let paths_empty = all_file_paths.read().is_empty();
            if is_open && paths_empty {
                if let Some(repo) = repo_signal.read().clone() {
                    let ref_str = git_ref_for_finder.clone();
                    let cache_key = finder_cache_key.read().clone();
                    spawn(async move {
                        // Try REST API first (GitHub/GitLab/Codeberg)
                        if let Ok(paths) =
                            file_fetcher::fetch_all_file_paths(&repo, Some(&ref_str)).await
                        {
                            if *finder_cache_key.peek() == cache_key {
                                all_file_paths.set(paths);
                            }
                            return;
                        }
                        // Fall back to isomorphic-git (works for all sources including GRASP)
                        if let Ok(paths) =
                            git_service::git_service()
                                .list_all_files(&repo, Some(&ref_str))
                                .await
                        {
                            if *finder_cache_key.peek() == cache_key {
                                all_file_paths.set(paths);
                            }
                        } else {
                            log::warn!("Failed to load file paths for fuzzy finder");
                        }
                    });
                }
            }
        });
    }

    let load_key = use_memo({
        let naddr = naddr.clone();
        let git_ref = git_ref.clone();
        let path = path.clone();
        move || format!("{}:{}:{}", naddr, git_ref, path)
    });
    use_effect({
        let naddr = naddr.clone();
        let git_ref = git_ref.clone();
        let path = path.clone();
        move || {
            let _key = load_key.read();
            let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
            if !client_initialized {
                return;
            }
            let naddr = naddr.clone();
            let git_ref = git_ref.clone();
            let path = path.clone();
            spawn(async move {
                loading.set(true);
                error.set(None);
                let repo = match fetch_repository(&naddr).await {
                    Ok(r) => r,
                    Err(e) => {
                        error.set(Some(format!("Repository not found: {}", e)));
                        loading.set(false);
                        return;
                    }
                };
                repo_signal.set(Some(repo.clone()));
                if !git_service::GitService::is_initialized() {
                    if let Err(e) = git_service::GitService::init().await {
                        error.set(Some(format!("Failed to initialize git: {}", e)));
                        loading.set(false);
                        return;
                    }
                }
                let decoded_path = urlencoding::decode(&path)
                    .map(|s| s.into_owned())
                    .unwrap_or_else(|_| path.clone());
                match git_service()
                    .list_files(&repo, &decoded_path, Some(&git_ref))
                    .await
                {
                    Ok(entries) => {
                        files.set(entries);
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to load files: {}", e)));
                    }
                }
                if let Ok(branch_list) = git_service().get_branches(&repo).await {
                    branches.set(branch_list);
                }
                loading.set(false);
            });
        }
    });
    let repo_name = repo_signal()
        .map(|r| r.display_name().to_string())
        .unwrap_or_else(|| "Repository".to_string());
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-2",
                    Link {
                        to: Route::CodeRepo {
                            naddr: naddr.clone(),
                        },
                        class: "text-blue-400 hover:underline font-medium",
                        "{repo_name}"
                    }
                    span { class: "text-muted-foreground", "/" }
                    span { class: "text-muted-foreground", "Files" }
                }
                div { class: "px-4 py-2 flex items-center gap-4 border-t border-border/50",
                    BranchSelector {
                        branches: branches(),
                        current_ref: git_ref.clone(),
                        naddr: naddr.clone(),
                        path: path.clone(),
                    }
                    FilePathBreadcrumb {
                        naddr: naddr.clone(),
                        git_ref: git_ref.clone(),
                        path: path.clone(),
                    }
                    div { class: "ml-auto",
                        button {
                            class: "px-3 py-1.5 text-sm border border-border rounded-lg hover:bg-accent transition flex items-center gap-1.5",
                            onclick: move |_| show_fuzzy_finder.set(true),
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
                                circle { cx: "11", cy: "11", r: "8" }
                                line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
                            }
                            "Find file"
                            span { class: "text-xs text-muted-foreground font-mono ml-1 border border-border rounded px-1", "t" }
                        }
                    }
                }
            }
            div { class: "p-4",
                if loading() {
                    FileTreeSkeleton {}
                } else if let Some(err) = error() {
                    div { class: "text-center py-12",
                        div { class: "text-red-400 mb-4",
                            svg {
                                class: "w-12 h-12 mx-auto",
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
                        }
                        p { class: "text-muted-foreground", "{err}" }
                        Link {
                            to: Route::CodeRepo {
                                naddr: naddr.clone(),
                            },
                            class: "inline-block mt-4 px-4 py-2 bg-muted hover:bg-accent rounded-lg transition",
                            "Back to Repository"
                        }
                    }
                } else {
                    div { class: "border border-border rounded-lg overflow-hidden",
                        CodeFileTree {
                            entries: files(),
                            naddr: naddr.clone(),
                            git_ref: git_ref.clone(),
                            current_path: path.clone(),
                        }
                    }
                }
            }
            if *show_fuzzy_finder.read() {
                FuzzyFinder {
                    files: all_file_paths(),
                    naddr: naddr.clone(),
                    git_ref: git_ref.clone(),
                    on_close: move |_| show_fuzzy_finder.set(false),
                }
            }
        }
    }
}
