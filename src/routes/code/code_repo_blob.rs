//! Code Repository Blob Page
//!
//! Displays file content at a specific path and ref.
use crate::components::{
    BranchSelector, CodeFileViewer, CodeFileViewerSkeleton, FilePathBreadcrumb,
};
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repository, git_service};
use crate::stores::nostr_client;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::utils::is_safe_path;
use crate::utils::nip34::Repository;
use dioxus::prelude::*;

fn is_tree_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("is a tree") || lower.contains("is a directory")
}

#[component]
pub fn CodeRepoBlob(naddr: String, git_ref: String, path: Vec<String>) -> Element {
    let path_str = path.join("/");
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut content = use_signal(String::new);
    let mut branches = use_signal(Vec::new);
    let mut repo_signal = use_signal(|| None::<Repository>);
    let mut is_directory = use_signal(|| false);
    let filename = path_str.rsplit('/').next().unwrap_or(&path_str).to_string();
    let mut gen = use_signal(|| 0u32);
    use_effect(use_reactive(
        (&naddr, &git_ref, &path_str),
        move |(naddr, git_ref, path_str)| {
            let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
            if !client_initialized {
                return;
            }
            is_directory.set(false);
            repo_signal.set(None);
            content.set(String::new());
            branches.set(Vec::new());
            let current_gen = gen.peek().wrapping_add(1);
            gen.set(current_gen);
            loading.set(true);
            error.set(None);
            spawn(async move {
                if !is_safe_path(&path_str) {
                    log::warn!("Path traversal attempt blocked: {}", path_str);
                    if *gen.peek() != current_gen {
                        return;
                    }
                    error.set(Some("Invalid path".to_string()));
                    loading.set(false);
                    return;
                }
                let repo = match fetch_repository(&naddr).await {
                    Ok(r) => r,
                    Err(e) => {
                        if *gen.peek() != current_gen {
                            return;
                        }
                        error.set(Some(format!("Failed to load repository: {}", e)));
                        loading.set(false);
                        return;
                    }
                };
                if *gen.peek() != current_gen {
                    return;
                }
                repo_signal.set(Some(repo.clone()));
                if !git_service::GitService::is_initialized() {
                    if let Err(e) = git_service::GitService::init().await {
                        if *gen.peek() != current_gen {
                            return;
                        }
                        error.set(Some(format!("Failed to initialize git: {}", e)));
                        loading.set(false);
                        return;
                    }
                }
                if *gen.peek() != current_gen {
                    return;
                }
                match git_service()
                    .read_file(&repo, &path_str, Some(&git_ref))
                    .await
                {
                    Ok(file_content) => {
                        if *gen.peek() != current_gen {
                            return;
                        }
                        content.set(file_content);
                    }
                    Err(e) => {
                        if *gen.peek() != current_gen {
                            return;
                        }
                        if is_tree_error(&e) {
                            is_directory.set(true);
                        } else {
                            error.set(Some(format!("Failed to load file: {}", e)));
                        }
                    }
                }
                if *gen.peek() != current_gen {
                    return;
                }
                let branch_result = git_service().get_branches(&repo).await;
                if *gen.peek() != current_gen {
                    return;
                }
                if let Ok(branch_list) = branch_result {
                    branches.set(branch_list);
                }
                loading.set(false);
            });
        },
    ));
    use_effect(use_reactive(
        (&naddr, &git_ref, &path, &path_str),
        move |(naddr, git_ref, path, path_str)| {
            if *is_directory.read() && is_safe_path(&path_str) {
                let nav = navigator();
                nav.replace(Route::CodeRepoTree {
                    naddr,
                    git_ref,
                    path,
                });
            }
        },
    ));
    let repo_name = repo_signal()
        .map(|r| r.display_name().to_string())
        .unwrap_or_else(|| "Repository".to_string());
    let parent_path = path_str
        .rsplit_once('/')
        .map(|(p, _)| p.to_string())
        .unwrap_or_default();
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-2",
                    Link {
                        to: Route::AddressViewer {
                            address: naddr.clone(),
                        },
                        class: "text-blue-400 hover:underline font-medium",
                        "{repo_name}"
                    }
                    span { class: "text-muted-foreground", "/" }
                    Link {
                        to: Route::CodeRepoTree {
                            naddr: naddr.clone(),
                            git_ref: git_ref.clone(),
                            path: vec![],
                        },
                        class: "text-blue-400 hover:underline",
                        "Files"
                    }
                }
                div { class: "px-4 py-2 flex items-center gap-4 border-t border-border/50",
                    BranchSelector {
                        branches: branches(),
                        current_ref: git_ref.clone(),
                        naddr: naddr.clone(),
                        path: parent_path.clone(),
                    }
                    FilePathBreadcrumb {
                        naddr: naddr.clone(),
                        git_ref: git_ref.clone(),
                        path: path_str.clone(),
                    }
                }
            }
            div { class: "p-4",
                if loading() {
                    CodeFileViewerSkeleton {}
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
                            to: Route::CodeRepoTree {
                                naddr: naddr.clone(),
                                git_ref: git_ref.clone(),
                                path: vec![],
                            },
                            class: "inline-block mt-4 px-4 py-2 bg-muted hover:bg-accent rounded-lg transition",
                            "Back to Repository Root"
                        }
                    }
                } else {
                    if *HAS_SIGNER.read() {
                        div { class: "flex justify-end mb-2",
                            Link {
                                to: Route::CodeRepoEditFile {
                                    naddr: naddr.clone(),
                                    git_ref: git_ref.clone(),
                                    path: path.clone(),
                                },
                                class: "flex items-center gap-1.5 px-3 py-1.5 text-sm border border-border bg-muted hover:bg-accent rounded-lg transition",
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
                                    path { d: "M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" }
                                    path { d: "M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" }
                                }
                                "Edit"
                            }
                        }
                    }
                    CodeFileViewer {
                        content: content(),
                        filename: filename.clone(),
                        git_ref: git_ref.clone(),
                    }
                }
            }
        }
    }
}
