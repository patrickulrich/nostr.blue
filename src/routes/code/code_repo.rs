//! Repository Detail Page
//!
//! View a single NIP-34 Git repository with README, issues, and PRs.
//! Styled to match gittr's layout-client.tsx pattern.
use crate::components::code::{CloneHelpModal, ContributorsList, ReadmeViewer, RelayDisplay, RepoActionBar, RepoHeader, RepoTabNav};
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::{fetch_readme, fetch_repository};
use crate::stores::nostr_client;
use crate::utils::format_relative_time_or;
use crate::utils::nip34::Repository;
use dioxus::prelude::*;
/// Repository detail page component
#[component]
pub fn CodeRepo(naddr: String) -> Element {
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    let mut loading = use_signal(|| true);
    let mut request_gen = use_signal(|| 0u32);
    let naddr_for_render = naddr.clone();
    use_effect(use_reactive(&naddr, move |naddr_val| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            log::info!("CodeRepo: Waiting for client initialization...");
            return;
        }
        let gen = request_gen.peek().wrapping_add(1);
        request_gen.set(gen);
        repo_result.set(None);
        spawn(async move {
            loading.set(true);
            let result = fetch_repository(&naddr_val).await;
            if *request_gen.peek() != gen { return; }
            repo_result.set(Some(result));
            loading.set(false);
        });
    }));
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
            if let Some(fork_ref) = &repo.fork_of {
                div { class: "flex items-center gap-2 text-sm text-muted-foreground",
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
                        circle { cx: "12", cy: "18", r: "3" }
                        circle { cx: "6", cy: "6", r: "3" }
                        circle { cx: "18", cy: "6", r: "3" }
                        path { d: "M18 9v2c0 .6-.4 1-1 1H7c-.6 0-1-.4-1-1V9" }
                        line { x1: "12", y1: "12", x2: "12", y2: "15" }
                    }
                    span { "Forked from " }
                    code { class: "px-1.5 py-0.5 bg-muted rounded text-xs font-mono",
                        if fork_ref.chars().count() > 12 {
                            "{fork_ref.chars().take(12).collect::<String>()}..."
                        } else {
                            "{fork_ref}"
                        }
                    }
                }
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
    let mut show_clone_modal = use_signal(|| false);
    let repo_for_fetch = repo.clone();
    let readme_resource: Resource<Result<String, String>> = use_resource(move || {
        let r = repo_for_fetch.clone();
        async move { fetch_readme(&r, None).await }
    });
    // Log readme fetch errors once via effect instead of in the render path
    use_effect(move || {
        if let Some(Err(e)) = &*readme_resource.read() {
            log::warn!("Failed to fetch readme: {}", e);
        }
    });
    rsx! {
        div { class: "flex flex-col lg:flex-row gap-6",
            // Main column
            div { class: "flex-1 min-w-0 space-y-6",
                div { class: "flex flex-col lg:flex-row gap-4",
                    if !repo.clone.is_empty() {
                        div { class: "flex gap-3",
                            Link {
                                to: Route::CodeRepoTree {
                                    naddr: naddr.clone(),
                                    git_ref: "HEAD".to_string(),
                                    path: vec![],
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
                            div { class: "flex items-center justify-between mb-2",
                                p { class: "text-xs text-muted-foreground", "Clone" }
                                button {
                                    class: "text-xs text-primary hover:underline",
                                    onclick: move |_| show_clone_modal.set(true),
                                    "More options"
                                }
                            }
                            code { class: "text-xs font-mono bg-background px-2 py-1 rounded overflow-x-auto block",
                                "{repo.clone.first().map(String::as_str).unwrap_or(\"\")}"
                            }
                        }
                    }
                }
                match &*readme_resource.read() {
                    Some(Ok(content)) => rsx! {
                        ReadmeViewer { content: Some(content.clone()), loading: false }
                    },
                    Some(Err(e)) => rsx! {
                        ReadmeViewer { content: None, loading: false, error: Some(e.clone()) }
                    },
                    None => rsx! {
                        ReadmeViewer { loading: true }
                    },
                }
            }
            // Sidebar
            div { class: "lg:w-72 space-y-4",
                // Contributors card
                div { class: "bg-card border border-border rounded-lg p-4",
                    ContributorsList {
                        owner: repo.pubkey.clone(),
                        maintainers: repo.maintainers.clone(),
                        issue_count: Some(repo.issue_count),
                        pr_count: Some(repo.pr_count),
                    }
                }
                // GRASP relay display
                if !repo.relays.is_empty() {
                    RelayDisplay { relays: repo.relays.clone() }
                }
                // Web links card
                if !repo.web.is_empty() {
                    div { class: "bg-card border border-border rounded-lg p-4",
                        h3 { class: "text-sm font-semibold text-foreground mb-3", "Links" }
                        div { class: "space-y-2",
                            for url in repo.web.iter().filter(|u| u.starts_with("http://") || u.starts_with("https://")) {
                                a {
                                    key: "{url}",
                                    href: "{url}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    class: "text-sm text-primary hover:underline flex items-center gap-1 max-w-[14rem] truncate",
                                    title: "{url}",
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
                }
                // Topics card
                if !repo.topics.is_empty() {
                    div { class: "bg-card border border-border rounded-lg p-4",
                        h3 { class: "text-sm font-semibold text-foreground mb-3", "Topics" }
                        div { class: "flex flex-wrap gap-1",
                            for topic in repo.topics.iter() {
                                span {
                                    key: "{topic}",
                                    class: "px-2 py-0.5 rounded-full bg-primary/10 text-primary text-xs",
                                    "{topic}"
                                }
                            }
                        }
                    }
                }
                // Metadata card
                div { class: "bg-card border border-border rounded-lg p-4",
                    h3 { class: "text-sm font-semibold text-foreground mb-3", "About" }
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
        if *show_clone_modal.read() {
            CloneHelpModal {
                clone_urls: repo.clone.clone(),
                naddr: naddr.clone(),
                on_close: move |_| show_clone_modal.set(false),
            }
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
