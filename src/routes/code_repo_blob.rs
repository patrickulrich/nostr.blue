//! Code Repository Blob Page
//!
//! Displays file content at a specific path and ref.

use dioxus::prelude::*;
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repository, git_service};
use crate::stores::nostr_client;
use crate::components::{CodeFileViewer, CodeFileViewerSkeleton, FilePathBreadcrumb, BranchSelector};
use crate::utils::nip34::Repository;

#[component]
pub fn CodeRepoBlob(naddr: String, git_ref: String, path: String) -> Element {
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut content = use_signal(String::new);
    let mut branches = use_signal(Vec::new);
    let mut repo_signal = use_signal(|| None::<Repository>);
    let mut is_directory = use_signal(|| false);

    // Extract filename from path
    let filename = path.rsplit('/').next().unwrap_or(&path).to_string();

    // Create a load key that changes when route params change - this ensures the effect re-runs
    let load_key = use_memo({
        let naddr = naddr.clone();
        let git_ref = git_ref.clone();
        let path = path.clone();
        move || format!("{}:{}:{}", naddr, git_ref, path)
    });

    // Load file content when key changes
    use_effect({
        let naddr = naddr.clone();
        let git_ref = git_ref.clone();
        let path = path.clone();
        move || {
            // Read the key to register as dependency
            let _key = load_key.read();
            let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

            if !client_initialized {
                return;
            }

            // Reset state for new load
            is_directory.set(false);

            let naddr = naddr.clone();
            let git_ref = git_ref.clone();
            let path = path.clone();

            spawn(async move {
                loading.set(true);
                error.set(None);

                // Fetch repository
                let repo = match fetch_repository(&naddr).await {
                    Ok(r) => r,
                    Err(e) => {
                        error.set(Some(format!("Repository not found: {}", e)));
                        loading.set(false);
                        return;
                    }
                };
                repo_signal.set(Some(repo.clone()));

                // Initialize git worker if needed
                if !git_service::GitService::is_initialized() {
                    if let Err(e) = git_service::GitService::init().await {
                        error.set(Some(format!("Failed to initialize git: {}", e)));
                        loading.set(false);
                        return;
                    }
                }

                // Read file content
                match git_service().read_file(&repo, &path, Some(&git_ref)).await {
                    Ok(file_content) => {
                        content.set(file_content);
                    }
                    Err(e) => {
                        // Check if the error indicates this is a directory (tree instead of blob)
                        if e.contains("tree") || e.contains("is a tree") {
                            // Signal to redirect to tree view
                            is_directory.set(true);
                        } else {
                            error.set(Some(format!("Failed to load file: {}", e)));
                        }
                    }
                }

                // Get branches
                if let Ok(branch_list) = git_service().get_branches(&repo).await {
                    branches.set(branch_list);
                }

                loading.set(false);
            });
        }
    });

    // Redirect to tree view if path is a directory
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

    // Get parent directory for the "back" link
    let parent_path = path
        .rsplit_once('/')
        .map(|(p, _)| p.to_string())
        .unwrap_or_default();

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",

                // Repository name and navigation
                div {
                    class: "px-4 py-3 flex items-center gap-2",

                    Link {
                        to: Route::CodeRepo { naddr: naddr.clone() },
                        class: "text-blue-400 hover:underline font-medium",
                        "{repo_name}"
                    }

                    span {
                        class: "text-muted-foreground",
                        "/"
                    }

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

                // Branch selector and breadcrumb
                div {
                    class: "px-4 py-2 flex items-center gap-4 border-t border-border/50",

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

            // Content
            div {
                class: "p-4",

                if loading() {
                    CodeFileViewerSkeleton {}
                } else if let Some(err) = error() {
                    div {
                        class: "text-center py-12",
                        div {
                            class: "text-red-400 mb-4",
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
                                line { x1: "12", y1: "8", x2: "12", y2: "12" }
                                line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                            }
                        }
                        p {
                            class: "text-muted-foreground",
                            "{err}"
                        }
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
