//! In-Repo Code Search Modal
//!
//! Provides full-text code search within a repository.
//! For GitHub-mirrored repos, uses the GitHub Code Search API.
//! Triggered via Ctrl/Cmd+Shift+F keyboard shortcut.
use crate::routes::Route;
use crate::services::git_hosting::git_service::extract_github_info;
use crate::services::git_hosting::github_import::{search_code, CodeSearchResult};
use crate::utils::nip34::Repository;
use dioxus::prelude::*;

/// Modal search component for searching code within a repository
#[component]
pub fn RepoSearchModal(
    repo: Repository,
    naddr: String,
    git_ref: String,
    on_close: EventHandler<()>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut results = use_signal(Vec::<CodeSearchResult>::new);
    let mut searching = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut selected_index = use_signal(|| 0usize);

    let github_info = extract_github_info(&repo);
    let is_github = github_info.is_some();

    let nav = use_navigator();
    let naddr_nav = naddr.clone();
    let git_ref_nav = git_ref.clone();

    let do_search = move |_| {
        let q = query.read().clone();
        if q.trim().is_empty() {
            return;
        }
        let gh = github_info.clone();
        spawn(async move {
            searching.set(true);
            error.set(None);
            results.set(Vec::new());
            selected_index.set(0);

            if let Some((owner, repo_name)) = gh {
                match search_code(&owner, &repo_name, &q, 20).await {
                    Ok(res) => results.set(res),
                    Err(e) => error.set(Some(e)),
                }
            } else {
                error.set(Some("Code search is only available for GitHub-mirrored repositories".to_string()));
            }
            searching.set(false);
        });
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm flex items-start justify-center pt-[10vh]",
            onclick: move |_| on_close.call(()),
            tabindex: 0,
            onkeydown: move |evt: Event<KeyboardData>| {
                match evt.key() {
                    Key::Escape => on_close.call(()),
                    Key::ArrowDown => {
                        let max = results.read().len().saturating_sub(1);
                        let cur = *selected_index.read();
                        if cur < max {
                            selected_index.set(cur + 1);
                        }
                    }
                    Key::ArrowUp => {
                        let cur = *selected_index.read();
                        if cur > 0 {
                            selected_index.set(cur - 1);
                        }
                    }
                    Key::Enter => {
                        let items = results.read();
                        let idx = *selected_index.read();
                        if let Some(result) = items.get(idx) {
                            nav.push(Route::CodeRepoBlob {
                                naddr: naddr_nav.clone(),
                                git_ref: git_ref_nav.clone(),
                                path: result.path.clone(),
                            });
                            on_close.call(());
                        }
                    }
                    _ => {}
                }
            },

            div {
                class: "w-full max-w-2xl bg-card border border-border rounded-xl shadow-2xl mx-4 max-h-[70vh] flex flex-col overflow-hidden",
                onclick: move |e| e.stop_propagation(),

                // Search header
                div { class: "flex items-center gap-3 p-4 border-b border-border",
                    svg {
                        class: "w-5 h-5 text-muted-foreground shrink-0",
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
                    input {
                        class: "flex-1 bg-transparent text-foreground placeholder:text-muted-foreground focus:outline-hidden text-sm",
                        r#type: "text",
                        placeholder: if is_github { "Search code in this repository..." } else { "Code search requires GitHub mirror" },
                        autofocus: true,
                        disabled: !is_github,
                        value: "{query}",
                        oninput: move |e| query.set(e.value()),
                        onkeypress: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter {
                                do_search(());
                            }
                        },
                    }
                    span { class: "text-xs text-muted-foreground bg-muted px-1.5 py-0.5 rounded",
                        "ESC"
                    }
                }

                // Results area
                div { class: "flex-1 overflow-y-auto",
                    if searching() {
                        div { class: "p-6 text-center",
                            div { class: "w-5 h-5 border-2 border-primary border-t-transparent rounded-full animate-spin mx-auto mb-2" }
                            p { class: "text-sm text-muted-foreground", "Searching..." }
                        }
                    } else if let Some(ref err) = *error.read() {
                        div { class: "p-4 text-sm text-destructive", "{err}" }
                    } else if results.read().is_empty() && !query.read().is_empty() {
                        div { class: "p-6 text-center text-sm text-muted-foreground",
                            "No results found"
                        }
                    } else if !results.read().is_empty() {
                        div { class: "divide-y divide-border",
                            for (idx , result) in results.read().iter().enumerate() {
                                {
                                    let is_selected = idx == *selected_index.read();
                                    let path = result.path.clone();
                                    let naddr_click = naddr.clone();
                                    let git_ref_click = git_ref.clone();
                                    let fragment = result.text_matches.first().map(|m| m.fragment.clone()).unwrap_or_default();
                                    rsx! {
                                        div {
                                            key: "{idx}_{path}",
                                            class: if is_selected {
                                                "p-3 cursor-pointer bg-accent/50"
                                            } else {
                                                "p-3 cursor-pointer hover:bg-accent/30"
                                            },
                                            onclick: move |_| {
                                                let p = path.clone();
                                                nav.push(Route::CodeRepoBlob {
                                                    naddr: naddr_click.clone(),
                                                    git_ref: git_ref_click.clone(),
                                                    path: p,
                                                });
                                                on_close.call(());
                                            },
                                            // File path
                                            div { class: "flex items-center gap-2 mb-1",
                                                svg {
                                                    class: "w-3.5 h-3.5 text-muted-foreground shrink-0",
                                                    xmlns: "http://www.w3.org/2000/svg",
                                                    width: "24",
                                                    height: "24",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    path { d: "M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" }
                                                    polyline { points: "13 2 13 9 20 9" }
                                                }
                                                span { class: "text-sm font-mono text-primary truncate",
                                                    "{result.path}"
                                                }
                                            }
                                            // Match fragment
                                            if !fragment.is_empty() {
                                                pre { class: "text-xs text-muted-foreground font-mono bg-muted rounded p-1.5 overflow-x-auto line-clamp-3",
                                                    "{fragment}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "p-6 text-center text-sm text-muted-foreground",
                            if is_github {
                                "Type a search query and press Enter"
                            } else {
                                "Code search is currently available for GitHub-mirrored repositories"
                            }
                        }
                    }
                }

                // Footer
                div { class: "p-2 border-t border-border text-xs text-muted-foreground flex items-center gap-4",
                    span { class: "flex items-center gap-1",
                        kbd { class: "px-1 py-0.5 bg-muted rounded text-[10px]", "↑↓" }
                        "navigate"
                    }
                    span { class: "flex items-center gap-1",
                        kbd { class: "px-1 py-0.5 bg-muted rounded text-[10px]", "↵" }
                        "open"
                    }
                    span { class: "flex items-center gap-1",
                        kbd { class: "px-1 py-0.5 bg-muted rounded text-[10px]", "esc" }
                        "close"
                    }
                }
            }
        }
    }
}
