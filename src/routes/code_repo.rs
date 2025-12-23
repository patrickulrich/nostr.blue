//! Repository Detail Page
//!
//! View a single NIP-34 Git repository with README, issues, and PRs.
//! Styled to match gittr's layout-client.tsx pattern.

use dioxus::prelude::*;
use crate::components::{icons, CodeIssueRow, CodePullRow};
use crate::components::code::{RepoHeader, RepoActionBar, RepoTabNav, ReadmeViewer};
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repository, fetch_repo_issues, fetch_repo_prs, fetch_readme};
use crate::utils::nip34::Repository;
use crate::utils::format_relative_time_or;
use crate::stores::profiles::PROFILE_CACHE;
use crate::stores::nostr_client;

/// Repository detail page component
#[component]
pub fn CodeRepo(naddr: String) -> Element {
    let active_tab = use_signal(|| RepoTab::Overview);
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    let mut loading = use_signal(|| true);

    // Clone naddr for use in the effect and for passing to child components
    let naddr_for_effect = naddr.clone();
    let naddr_for_render = naddr.clone();

    // Wait for client initialization before fetching
    // This prevents the "Client not initialized" error when navigating directly to this page
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
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "p-4 flex items-center gap-3",
                    Link {
                        to: Route::CodeHome {},
                        class: "text-muted-foreground hover:text-foreground",
                        dangerous_inner_html: icons::ARROW_LEFT
                    }
                    h1 {
                        class: "text-xl font-bold flex items-center gap-2",
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

            // Content
            div {
                class: "p-4",
                // Show loading while client is initializing or data is being fetched
                if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && repo_result.read().is_none()) {
                    LoadingSkeleton {}
                } else {
                    match repo_result.read().as_ref() {
                        Some(Ok(r)) => rsx! {
                            RepoContent {
                                repo: r.clone(),
                                naddr: naddr_for_render.clone(),
                                active_tab: active_tab,
                            }
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

#[derive(Clone, Copy, PartialEq)]
enum RepoTab {
    Overview,
    Issues,
    PullRequests,
}

#[component]
fn RepoContent(repo: Repository, naddr: String, active_tab: Signal<RepoTab>) -> Element {
    rsx! {
        div {
            class: "space-y-4",

            // Row 1: Header + Action Bar
            div {
                class: "flex flex-col lg:flex-row lg:items-center justify-between gap-4",

                // Repository header (owner/repo breadcrumb + Public badge)
                RepoHeader { repo: repo.clone() }

                // Action bar (Watch/Star/Fork/Zap/Share)
                RepoActionBar { repo: repo.clone(), naddr: naddr.clone() }
            }

            // Description
            if let Some(desc) = &repo.description {
                p {
                    class: "text-muted-foreground",
                    "{desc}"
                }
            }

            // Tab navigation
            RepoTabNav {
                repo: repo.clone(),
                naddr: naddr.clone(),
                active_tab: match *active_tab.read() {
                    RepoTab::Overview => "overview".to_string(),
                    RepoTab::Issues => "issues".to_string(),
                    RepoTab::PullRequests => "pulls".to_string(),
                },
                issue_count: Some(repo.issue_count),
                pr_count: Some(repo.pr_count),
            }

            // Tab content
            div {
                class: "pt-4",
                match *active_tab.read() {
                    RepoTab::Overview => rsx! {
                        OverviewTab { repo: repo.clone(), naddr: naddr.clone() }
                    },
                    RepoTab::Issues => rsx! {
                        IssuesTab { naddr: naddr.clone() }
                    },
                    RepoTab::PullRequests => rsx! {
                        PullRequestsTab { naddr: naddr.clone() }
                    },
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TabButtonProps {
    label: &'static str,
    #[props(default = 0)]
    count: u32,
    active: bool,
    onclick: EventHandler<MouseEvent>,
}

#[component]
fn TabButton(props: TabButtonProps) -> Element {
    let class = if props.active {
        "px-4 py-2 text-sm font-medium text-primary border-b-2 border-primary flex items-center gap-2"
    } else {
        "px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground border-b-2 border-transparent flex items-center gap-2"
    };

    rsx! {
        button {
            class: "{class}",
            onclick: move |e| props.onclick.call(e),
            "{props.label}"
            if props.count > 0 {
                span {
                    class: "px-1.5 py-0.5 text-xs rounded-full bg-muted",
                    "{props.count}"
                }
            }
        }
    }
}

#[component]
fn OverviewTab(repo: Repository, naddr: String) -> Element {
    // Fetch README content
    let repo_for_fetch = repo.clone();
    let readme_resource: Resource<Result<String, String>> = use_resource(move || {
        let r = repo_for_fetch.clone();
        async move {
            fetch_readme(&r, None).await
        }
    });

    rsx! {
        div {
            class: "space-y-6",

            // Browse Files and Clone URL section
            div {
                class: "flex flex-col lg:flex-row gap-4",

                // Left side: Browse Files button
                if !repo.clone.is_empty() {
                    div {
                        class: "flex gap-3",
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

                // Right side: Clone URL
                if !repo.clone.is_empty() {
                    div {
                        class: "flex-1 p-3 bg-muted rounded-lg",
                        p { class: "text-xs text-muted-foreground mb-2", "Clone" }
                        code {
                            class: "text-xs font-mono bg-background px-2 py-1 rounded overflow-x-auto block",
                            "{repo.clone.first().unwrap_or(&String::new())}"
                        }
                    }
                }
            }

            // README section
            match &*readme_resource.read() {
                Some(Ok(content)) => rsx! {
                    ReadmeViewer {
                        content: Some(content.clone()),
                        loading: false,
                    }
                },
                Some(Err(_)) => rsx! {
                    ReadmeViewer {
                        content: None,
                        loading: false,
                    }
                },
                None => rsx! {
                    ReadmeViewer {
                        loading: true,
                    }
                },
            }

            // Maintainers
            if !repo.maintainers.is_empty() {
                div {
                    h3 {
                        class: "font-semibold mb-3",
                        "Maintainers"
                    }
                    div {
                        class: "flex flex-wrap gap-2",
                        for pubkey in repo.maintainers.iter() {
                            MaintainerBadge {
                                key: "{pubkey}",
                                pubkey: pubkey.clone()
                            }
                        }
                    }
                }
            }

            // Web URLs
            if !repo.web.is_empty() {
                div {
                    class: "flex flex-wrap gap-2",
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
                                line { x1: "10", y1: "14", x2: "21", y2: "3" }
                            }
                            "{url}"
                        }
                    }
                }
            }

            // Metadata
            div {
                class: "text-sm text-muted-foreground space-y-1",
                p { "Event ID: {repo.event_id}" }
                p { "Created: " {format_relative_time_or(repo.created_at, "Unknown")} }
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
        .unwrap_or_else(|| format!("{}...", &pubkey[..8]));

    rsx! {
        Link {
            to: Route::Profile { pubkey: pubkey.clone() },
            class: "px-3 py-1 bg-muted rounded-full text-sm hover:bg-accent transition",
            "{name}"
        }
    }
}

#[component]
fn IssuesTab(naddr: String) -> Element {
    let naddr_clone = naddr.clone();
    let issues = use_resource(move || {
        let n = naddr_clone.clone();
        async move {
            fetch_repo_issues(&n).await
        }
    });

    rsx! {
        div {
            class: "space-y-4",

            // Create issue button
            div {
                class: "flex justify-end",
                Link {
                    to: Route::CodeIssueNew { naddr: naddr.clone() },
                    class: "px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition flex items-center gap-1",
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
                    "New Issue"
                }
            }

            // Issues list
            match &*issues.read() {
                Some(Ok(list)) if !list.is_empty() => rsx! {
                    div {
                        class: "border border-border rounded-lg divide-y divide-border",
                        for issue in list.iter() {
                            CodeIssueRow {
                                key: "{issue.event_id}",
                                issue: issue.clone()
                            }
                        }
                    }
                },
                Some(Ok(_)) => rsx! {
                    EmptyIssues {}
                },
                Some(Err(e)) => rsx! {
                    div {
                        class: "text-center py-8 text-destructive",
                        "Failed to load issues: {e}"
                    }
                },
                None => rsx! {
                    div {
                        class: "space-y-2",
                        for i in 0..3 {
                            div {
                                key: "{i}",
                                class: "p-3 border border-border rounded animate-pulse",
                                div { class: "h-4 bg-muted rounded w-2/3 mb-2" }
                                div { class: "h-3 bg-muted rounded w-1/3" }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn EmptyIssues() -> Element {
    rsx! {
        div {
            class: "text-center py-12",
            div {
                class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
                svg {
                    class: "w-8 h-8 text-muted-foreground",
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
            h3 { class: "font-semibold text-lg mb-2", "No Issues" }
            p { class: "text-muted-foreground text-sm", "This repository has no open issues." }
        }
    }
}

#[component]
fn PullRequestsTab(naddr: String) -> Element {
    let naddr_clone = naddr.clone();
    let prs = use_resource(move || {
        let n = naddr_clone.clone();
        async move {
            fetch_repo_prs(&n).await
        }
    });

    rsx! {
        div {
            class: "space-y-4",

            // Create PR button
            div {
                class: "flex justify-end",
                Link {
                    to: Route::CodePullNew { naddr: naddr.clone() },
                    class: "px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition flex items-center gap-1",
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
                    "New PR"
                }
            }

            // PRs list
            match &*prs.read() {
                Some(Ok(list)) if !list.is_empty() => rsx! {
                    div {
                        class: "border border-border rounded-lg divide-y divide-border",
                        for pr in list.iter() {
                            CodePullRow {
                                key: "{pr.event_id}",
                                pr: pr.clone()
                            }
                        }
                    }
                },
                Some(Ok(_)) => rsx! {
                    EmptyPRs {}
                },
                Some(Err(e)) => rsx! {
                    div {
                        class: "text-center py-8 text-destructive",
                        "Failed to load PRs: {e}"
                    }
                },
                None => rsx! {
                    div {
                        class: "space-y-2",
                        for i in 0..3 {
                            div {
                                key: "{i}",
                                class: "p-3 border border-border rounded animate-pulse",
                                div { class: "h-4 bg-muted rounded w-2/3 mb-2" }
                                div { class: "h-3 bg-muted rounded w-1/3" }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn EmptyPRs() -> Element {
    rsx! {
        div {
            class: "text-center py-12",
            div {
                class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
                svg {
                    class: "w-8 h-8 text-muted-foreground",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    circle { cx: "18", cy: "18", r: "3" }
                    circle { cx: "6", cy: "6", r: "3" }
                    path { d: "M13 6h3a2 2 0 0 1 2 2v7" }
                    line { x1: "6", y1: "9", x2: "6", y2: "21" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "No Pull Requests" }
            p { class: "text-muted-foreground text-sm", "This repository has no open pull requests." }
        }
    }
}

#[component]
fn ErrorState(message: String) -> Element {
    rsx! {
        div {
            class: "text-center py-12",
            div {
                class: "w-16 h-16 mx-auto mb-4 rounded-full bg-destructive/10 flex items-center justify-center",
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
                    line { x1: "12", y1: "8", x2: "12", y2: "12" }
                    line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "Repository Not Found" }
            p { class: "text-muted-foreground text-sm mb-4", "{message}" }
            Link {
                to: Route::CodeHome {},
                class: "text-primary hover:underline",
                "← Back to Code"
            }
        }
    }
}

#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div {
            class: "space-y-6 animate-pulse",

            // Header skeleton
            div {
                class: "space-y-3",
                div { class: "h-6 bg-muted rounded w-1/3" }
                div { class: "h-4 bg-muted rounded w-2/3" }
                div {
                    class: "flex gap-4",
                    div { class: "h-4 bg-muted rounded w-16" }
                    div { class: "h-4 bg-muted rounded w-20" }
                    div { class: "h-4 bg-muted rounded w-16" }
                }
            }

            // Clone box skeleton
            div { class: "h-20 bg-muted rounded-lg" }

            // Tabs skeleton
            div {
                class: "flex gap-4 border-b border-border pb-2",
                div { class: "h-6 bg-muted rounded w-20" }
                div { class: "h-6 bg-muted rounded w-16" }
                div { class: "h-6 bg-muted rounded w-24" }
            }

            // Content skeleton
            div { class: "h-32 bg-muted rounded-lg" }
        }
    }
}

