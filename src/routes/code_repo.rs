//! Repository Detail Page
//!
//! View a single NIP-34 Git repository with README, issues, and PRs.
//! Styled to match gittr's layout-client.tsx pattern.
use crate::components::code::{ReadmeViewer, RepoActionBar, RepoHeader, RepoTabNav};
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::{fetch_readme, fetch_repository};
use crate::stores::nostr_client;
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::format_relative_time_or;
use crate::utils::nip34::Repository;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
/// Repository detail page component
#[component]
pub fn CodeRepo(naddr: String) -> Element {
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    let mut loading = use_signal(|| true);
    let naddr_for_effect = naddr.clone();
    let naddr_for_render = naddr.clone();
    use_effect(move || {
        let naddr = naddr_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            log::info!("CodeRepo: Waiting for client initialization...");
            return;
        }
        spawn(async move {
            loading.set(true);
            let result = fetch_repository(&naddr).await;
            repo_result.set(Some(result));
            loading.set(false);
        });
    });
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-3",
                    Link {
                        to: Route::CodeHome {},
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
                            path { d: "M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.28 1.15-.28 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4" }
                            path { d: "M9 18c-4.51 2-5-2-7-2" }
                        }
                        "Repository"
                    }
                }
            }
            div { class: "p-4",
                if !*nostr_client::CLIENT_INITIALIZED.read()
                    || (*loading.read() && repo_result.read().is_none())
                {
                    LoadingSkeleton {}
                } else {
                    match repo_result.read().as_ref() {
                        Some(Ok(r)) => rsx! {
                            RepoContent { repo: r.clone(), naddr: naddr_for_render.clone() }
                        },
                        Some(Err(e)) => rsx! {
                            ErrorState { message: e.clone() }
                        },
                        None => rsx! {
                            LoadingSkeleton {}
                        },
                    }
                }
            }
        }
    }
}
#[component]
fn RepoContent(repo: Repository, naddr: String) -> Element {
    rsx! {
        div { class: "space-y-4",
            div { class: "flex flex-col lg:flex-row lg:items-center justify-between gap-4",
                RepoHeader { repo: repo.clone() }
                RepoActionBar { repo: repo.clone(), naddr: naddr.clone() }
            }
            if let Some(desc) = &repo.description {
                p { class: "text-muted-foreground", "{desc}" }
            }
            RepoTabNav {
                naddr: naddr.clone(),
                active_tab: "overview".to_string(),
                issue_count: Some(repo.issue_count),
                pr_count: Some(repo.pr_count),
            }
            div { class: "pt-4",
                OverviewTab { repo: repo.clone(), naddr: naddr.clone() }
            }
        }
    }
}
#[component]
fn OverviewTab(repo: Repository, naddr: String) -> Element {
    let repo_for_fetch = repo.clone();
    let readme_resource: Resource<Result<String, String>> = use_resource(move || {
        let r = repo_for_fetch.clone();
        async move { fetch_readme(&r, None).await }
    });
    rsx! {
        div { class: "space-y-6",
            div { class: "flex flex-col lg:flex-row gap-4",
                if !repo.clone.is_empty() {
                    div { class: "flex gap-3",
                        Link {
                            to: Route::CodeRepoTree {
                                naddr: naddr.clone(),
                                git_ref: "HEAD".to_string(),
                                path: "".to_string(),
                            },
                            class: "flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition font-medium",
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
                                path { d: "M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" }
                            }
                            "Browse Files"
                        }
                    }
                }
                if !repo.clone.is_empty() {
                    div { class: "flex-1 p-3 bg-muted rounded-lg",
                        p { class: "text-xs text-muted-foreground mb-2", "Clone" }
                        code { class: "text-xs font-mono bg-background px-2 py-1 rounded overflow-x-auto block",
                            "{repo.clone.first().unwrap_or(&String::new())}"
                        }
                    }
                }
            }
            match &*readme_resource.read() {
                Some(Ok(content)) => rsx! {
                    ReadmeViewer { content: Some(content.clone()), loading: false }
                },
                Some(Err(_)) => rsx! {
                    ReadmeViewer { content: None, loading: false }
                },
                None => rsx! {
                    ReadmeViewer { loading: true }
                },
            }
            if !repo.maintainers.is_empty() {
                div {
                    h3 { class: "font-semibold mb-3", "Maintainers" }
                    div { class: "flex flex-wrap gap-2",
                        for pubkey in repo.maintainers.iter() {
                            MaintainerBadge { key: "{pubkey}", pubkey: pubkey.clone() }
                        }
                    }
                }
            }
            if !repo.web.is_empty() {
                div { class: "flex flex-wrap gap-2",
                    for url in repo.web.iter() {
                        a {
                            key: "{url}",
                            href: "{url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            class: "text-sm text-primary hover:underline flex items-center gap-1",
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
                                path { d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" }
                                polyline { points: "15 3 21 3 21 9" }
                                line {
                                    x1: "10",
                                    y1: "14",
                                    x2: "21",
                                    y2: "3",
                                }
                            }
                            "{url}"
                        }
                    }
                }
            }
            div { class: "text-sm text-muted-foreground space-y-1",
                p { "Event ID: {repo.event_id}" }
                p {
                    "Created: "
                    {format_relative_time_or(repo.created_at, "Unknown")}
                }
            }
        }
    }
}
#[component]
fn MaintainerBadge(pubkey: String) -> Element {
    let profile = PROFILE_CACHE.read().peek(&pubkey).cloned();
    let name = profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&pubkey));
    rsx! {
        Link {
            to: Route::Profile {
                pubkey: pubkey.clone(),
            },
            class: "px-3 py-1 bg-muted rounded-full text-sm hover:bg-accent transition",
            "{name}"
        }
    }
}
#[component]
fn ErrorState(message: String) -> Element {
    rsx! {
        div { class: "text-center py-12",
            div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-destructive/10 flex items-center justify-center",
                svg {
                    class: "w-8 h-8 text-destructive",
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
            h3 { class: "font-semibold text-lg mb-2", "Repository Not Found" }
            p { class: "text-muted-foreground text-sm mb-4", "{message}" }
            Link { to: Route::CodeHome {}, class: "text-primary hover:underline", "← Back to Code" }
        }
    }
}
#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-6 animate-pulse",
            div { class: "space-y-3",
                div { class: "h-6 bg-muted rounded w-1/3" }
                div { class: "h-4 bg-muted rounded w-2/3" }
                div { class: "flex gap-4",
                    div { class: "h-4 bg-muted rounded w-16" }
                    div { class: "h-4 bg-muted rounded w-20" }
                    div { class: "h-4 bg-muted rounded w-16" }
                }
            }
            div { class: "h-20 bg-muted rounded-lg" }
            div { class: "flex gap-4 border-b border-border pb-2",
                div { class: "h-6 bg-muted rounded w-20" }
                div { class: "h-6 bg-muted rounded w-16" }
                div { class: "h-6 bg-muted rounded w-24" }
            }
            div { class: "h-32 bg-muted rounded-lg" }
        }
    }
}
