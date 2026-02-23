//! Code Repository Blob Page
//!
//! Displays file content at a specific path and ref.
use crate::components::{
    BranchSelector, CodeFileViewer, CodeFileViewerSkeleton, FilePathBreadcrumb,
};
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repository, git_service};
use crate::stores::nostr_client;
use crate::utils::is_safe_path;
use crate::utils::nip34::Repository;
use dioxus::prelude::*;
#[component]
pub fn CodeRepoBlob(naddr: String, git_ref: String, path: String) -> Element {
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut content = use_signal(String::new);
    let mut branches = use_signal(Vec::new);
    let mut repo_signal = use_signal(|| None::<Repository>);
    let mut is_directory = use_signal(|| false);
    let filename = path.rsplit('/').next().unwrap_or(&path).to_string();
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
            is_directory.set(false);
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
                if !is_safe_path(&decoded_path) {
                    log::warn!("Path traversal attempt blocked: {}", decoded_path);
                    error.set(Some("Invalid path".to_string()));
                    loading.set(false);
                    return;
                }
                match git_service().read_file(&repo, &decoded_path, Some(&git_ref)).await
                {
                    Ok(file_content) => {
                        content.set(file_content);
                    }
                    Err(e) => {
                        if e.contains("tree") || e.contains("is a tree") {
                            is_directory.set(true);
                        } else {
                            error.set(Some(format!("Failed to load file: {}", e)));
                        }
                    }
                }
                if let Ok(branch_list) = git_service().get_branches(&repo).await {
                    branches.set(branch_list);
                }
                loading.set(false);
            });
        }
    });
    use_effect({
        let naddr = naddr.clone();
        let git_ref = git_ref.clone();
        let path = path.clone();
        move || {
            if *is_directory.read() {
                let nav = navigator();
                nav.replace(Route::CodeRepoTree {
                    naddr: naddr.clone(),
                    git_ref: git_ref.clone(),
                    path: path.clone(),
                });
            }
        }
    });
    let repo_name = repo_signal()
        .map(|r| r.display_name().to_string())
        .unwrap_or_else(|| "Repository".to_string());
    let parent_path = path
        .rsplit_once('/')
        .map(|(p, _)| p.to_string())
        .unwrap_or_default();
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
                    Link {
                        to: Route::CodeRepoTree {
                            naddr: naddr.clone(),
                            git_ref: git_ref.clone(),
                            path: "".to_string(),
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
                        path: path.clone(),
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
                                path: parent_path.clone(),
                            },
                            class: "inline-block mt-4 px-4 py-2 bg-muted hover:bg-accent rounded-lg transition",
                            "Back to Directory"
                        }
                    }
                } else {
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
