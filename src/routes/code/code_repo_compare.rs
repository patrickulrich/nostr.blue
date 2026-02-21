//! Repository Compare View
//!
//! Compare two branches and view diff.
use crate::components::code::DiffViewer;
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repository, git_service};
use crate::stores::nostr_client;
use crate::utils::nip34::Repository;
use dioxus::prelude::*;
/// Repository compare page component
#[component]
pub fn CodeRepoCompare(naddr: String) -> Element {
    let mut base_branch = use_signal(|| "main".to_string());
    let mut head_branch = use_signal(String::new);
    let mut diff_content = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut has_compared = use_signal(|| false);
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    use_effect(use_reactive(&naddr, move |naddr| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            let result = fetch_repository(&naddr).await;
            repo_result.set(Some(result));
        });
    }));
    let handle_compare = move |_| {
        diff_content.set(String::new());
        let base = base_branch.read().clone();
        let head = head_branch.read().clone();
        if base.trim().is_empty() || head.trim().is_empty() {
            error.set(Some("Please specify both base and head branches".to_string()));
            return;
        }
        let repo = match &*repo_result.read() {
            Some(Ok(r)) => r.clone(),
            Some(Err(e)) => {
                error.set(Some(format!("Repository error: {}", e)));
                return;
            }
            None => {
                error.set(Some("Repository not loaded yet".to_string()));
                return;
            }
        };
        loading.set(true);
        error.set(None);
        has_compared.set(true);
        spawn(async move {
            // Try universal git_service compare first, falls back to GitHub API internally
            if git_service::GitService::is_initialized()
                || git_service::GitService::init().await.is_ok()
            {
                match git_service::git_service().compare_refs(&repo, &base, &head).await {
                    Ok(diff) => {
                        diff_content.set(diff);
                        loading.set(false);
                        return;
                    }
                    Err(e) => {
                        error.set(Some(e));
                        diff_content.set(String::new());
                    }
                }
            } else {
                error.set(Some("Failed to initialize git service".to_string()));
                diff_content.set(String::new());
            }
            loading.set(false);
        });
    };
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-3",
                    Link {
                        to: Route::CodeRepo {
                            naddr: naddr.clone(),
                        },
                        class: "text-muted-foreground hover:text-foreground",
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
                            line { x1: "18", y1: "20", x2: "18", y2: "10" }
                            line { x1: "12", y1: "20", x2: "12", y2: "4" }
                            line { x1: "6", y1: "20", x2: "6", y2: "14" }
                        }
                        "Compare"
                    }
                }
            }
            div { class: "p-4 space-y-6",
                div { class: "bg-card border border-border rounded-lg p-4 space-y-4",
                    h3 { class: "text-lg font-semibold", "Compare Branches" }
                    div { class: "flex flex-col sm:flex-row items-stretch sm:items-end gap-3",
                        div { class: "flex-1",
                            label { class: "block text-sm font-medium mb-2", "Base" }
                            input {
                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground text-sm",
                                r#type: "text",
                                placeholder: "main",
                                value: "{base_branch}",
                                oninput: move |e| base_branch.set(e.value()),
                            }
                        }
                        span { class: "text-muted-foreground text-sm self-center hidden sm:block pb-2", "..." }
                        div { class: "flex-1",
                            label { class: "block text-sm font-medium mb-2", "Head" }
                            input {
                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground text-sm",
                                r#type: "text",
                                placeholder: "feature-branch",
                                value: "{head_branch}",
                                oninput: move |e| head_branch.set(e.value()),
                            }
                        }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50",
                            disabled: *loading.read(),
                            onclick: handle_compare,
                            if *loading.read() {
                                "Comparing..."
                            } else {
                                "Compare"
                            }
                        }
                    }
                    if let Some(err) = error.read().as_ref() {
                        div { class: "text-sm text-red-400", "{err}" }
                    }
                }
                if *loading.read() {
                    div { class: "text-center py-8",
                        div { class: "inline-block animate-spin rounded-full h-8 w-8 border-b-2 border-primary" }
                        p { class: "text-sm text-muted-foreground mt-3", "Fetching diff..." }
                    }
                } else if *has_compared.read() && !diff_content.read().is_empty() {
                    DiffViewer { content: diff_content.read().clone(), is_cover_letter: false }
                    div { class: "flex justify-end",
                        Link {
                            to: Route::CodePullNew {
                                naddr: naddr.clone(),
                            },
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Create Pull Request"
                        }
                    }
                } else if *has_compared.read() && diff_content.read().is_empty() && error.read().is_none() {
                    div { class: "text-center text-muted-foreground py-8",
                        "No differences found — branches are identical"
                    }
                } else if !*has_compared.read() {
                    div { class: "text-center text-muted-foreground py-8",
                        "Select two branches to compare"
                    }
                }
            }
        }
    }
}
