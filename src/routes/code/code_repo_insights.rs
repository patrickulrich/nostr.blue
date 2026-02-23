//! Repository Insights Page
//!
//! Shows repository statistics, activity timeline, and contributor overview.
//! Fetches issues, PRs, and (for GitHub repos) commits to build a dashboard.
use crate::components::code::RepoTabNav;
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::git_service::extract_github_info;
use crate::services::git_hosting::{fetch_repo_issues, fetch_repo_prs, fetch_repository, github_import};
use crate::stores::nostr_client;
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::format_relative_time_or;
use crate::utils::nip34::{Issue, IssueStatus, PullRequest, Repository};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;

/// Repository insights page component
#[component]
pub fn CodeRepoInsights(naddr: String) -> Element {
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    let mut loading = use_signal(|| true);
    let naddr_for_render = naddr.clone();

    use_effect(use_reactive(&naddr, move |naddr| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            let result = fetch_repository(&naddr).await;
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
                        // Pie chart icon for insights
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
                            path { d: "M21.21 15.89A10 10 0 1 1 8 2.83" }
                            path { d: "M22 12A10 10 0 0 0 12 2v10z" }
                        }
                        "Insights"
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
                            InsightsContent { repo: r.clone(), naddr: naddr_for_render.clone() }
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

/// Insights content once the repository is loaded
#[component]
fn InsightsContent(repo: Repository, naddr: String) -> Element {
    let mut issues = use_signal(Vec::<Issue>::new);
    let mut prs = use_signal(Vec::<PullRequest>::new);
    let mut commit_count = use_signal(|| None::<usize>);
    let mut data_loading = use_signal(|| true);

    use_effect(use_reactive((&naddr, &repo), move |(n, r)| {
        spawn(async move {
            data_loading.set(true);

            // Fetch issues and PRs in parallel
            let (issues_result, prs_result) = futures::join!(
                fetch_repo_issues(&n),
                fetch_repo_prs(&n)
            );
            if let Ok(fetched) = issues_result {
                issues.set(fetched);
            }
            if let Ok(fetched) = prs_result {
                prs.set(fetched);
            }

            // For GitHub repos, fetch commit count
            if let Some((owner, repo_name)) = extract_github_info(&r) {
                if let Ok(commits) = github_import::fetch_commits(&owner, &repo_name, 100).await {
                    commit_count.set(Some(commits.len()));
                }
            }

            data_loading.set(false);
        });
    }));

    let issues_read = issues.read();
    let prs_read = prs.read();

    let total_issues = issues_read.len();
    let open_issues = issues_read.iter().filter(|i| i.status == IssueStatus::Open).count();
    let closed_issues = issues_read.iter().filter(|i| i.status == IssueStatus::Closed).count();

    let total_prs = prs_read.len();
    let open_prs = prs_read.iter().filter(|p| p.status == IssueStatus::Open).count();
    let merged_prs = prs_read.iter().filter(|p| p.status == IssueStatus::Applied).count();

    let maintainer_count = {
        let mut unique = std::collections::HashSet::new();
        unique.insert(repo.pubkey.clone());
        for m in &repo.maintainers {
            unique.insert(m.clone());
        }
        unique.len()
    };

    rsx! {
        div { class: "space-y-4",
            // Repo header area
            div { class: "flex items-center gap-2",
                span { class: "text-lg font-bold", "{repo.display_name()}" }
                if let Some(desc) = &repo.description {
                    span { class: "text-muted-foreground text-sm", " - {desc}" }
                }
            }

            RepoTabNav {
                naddr: naddr.clone(),
                active_tab: "insights".to_string(),
                issue_count: Some(repo.issue_count),
                pr_count: Some(repo.pr_count),
            }

            if *data_loading.read() {
                div { class: "space-y-4 animate-pulse pt-4",
                    div { class: "grid grid-cols-2 md:grid-cols-4 gap-4",
                        for i in 0..4 {
                            div { key: "{i}", class: "h-24 bg-muted rounded-lg" }
                        }
                    }
                    div { class: "h-48 bg-muted rounded-lg" }
                }
            } else {
                // Stats Cards Grid
                div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-4 pt-4",
                    StatCard {
                        label: "Issues",
                        value: total_issues.to_string(),
                        subtitle: format!("{open_issues} open / {closed_issues} closed"),
                        icon_svg: r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="1"/></svg>"#,
                        color: "text-green-500",
                    }
                    StatCard {
                        label: "Pull Requests",
                        value: total_prs.to_string(),
                        subtitle: format!("{open_prs} open / {merged_prs} merged"),
                        icon_svg: r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="18" cy="18" r="3"/><circle cx="6" cy="6" r="3"/><path d="M13 6h3a2 2 0 0 1 2 2v7"/><line x1="6" y1="9" x2="6" y2="21"/></svg>"#,
                        color: "text-purple-500",
                    }
                    StatCard {
                        label: "Commits",
                        value: match *commit_count.read() {
                            Some(c) => c.to_string(),
                            None => "N/A".to_string(),
                        },
                        subtitle: if commit_count.read().is_some() { "recent commits".to_string() } else { "not available".to_string() },
                        icon_svg: r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><line x1="1.05" y1="12" x2="7" y2="12"/><line x1="17.01" y1="12" x2="22.96" y2="12"/></svg>"#,
                        color: "text-blue-500",
                    }
                    StatCard {
                        label: "Maintainers",
                        value: maintainer_count.to_string(),
                        subtitle: "contributors".to_string(),
                        icon_svg: r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></svg>"#,
                        color: "text-orange-500",
                    }
                    StatCard {
                        label: "Stars",
                        value: repo.star_count.to_string(),
                        subtitle: "star events".to_string(),
                        icon_svg: r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>"#,
                        color: "text-yellow-500",
                    }
                }

                // Recent Activity Timeline
                div { class: "pt-6",
                    h2 { class: "text-lg font-semibold mb-4 flex items-center gap-2",
                        svg {
                            class: "w-5 h-5 text-muted-foreground",
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
                            polyline { points: "12 6 12 12 16 14" }
                        }
                        "Recent Activity"
                    }
                    ActivityTimeline {
                        issues: issues_read.clone(),
                        prs: prs_read.clone(),
                    }
                }

                // Contributors Overview
                div { class: "pt-6",
                    h2 { class: "text-lg font-semibold mb-4 flex items-center gap-2",
                        svg {
                            class: "w-5 h-5 text-muted-foreground",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" }
                            circle { cx: "9", cy: "7", r: "4" }
                            path { d: "M23 21v-2a4 4 0 0 0-3-3.87" }
                            path { d: "M16 3.13a4 4 0 0 1 0 7.75" }
                        }
                        "Maintainers"
                    }
                    ContributorsOverview { repo: repo.clone() }
                }
            }
        }
    }
}

/// Single stat card
#[component]
fn StatCard(
    label: String,
    value: String,
    subtitle: String,
    icon_svg: &'static str,
    color: &'static str,
) -> Element {
    rsx! {
        div { class: "bg-card border border-border rounded-lg p-3",
            div { class: "flex items-center gap-2 mb-2",
                span {
                    class: "w-5 h-5 shrink-0 {color}",
                    dangerous_inner_html: icon_svg,
                }
                span { class: "text-sm text-muted-foreground", "{label}" }
            }
            div { class: "text-2xl font-bold", "{value}" }
            div { class: "text-xs text-muted-foreground mt-1", "{subtitle}" }
        }
    }
}

/// Activity timeline showing recent issues and PRs
#[component]
fn ActivityTimeline(issues: Vec<Issue>, prs: Vec<PullRequest>) -> Element {
    // Build combined timeline entries
    let mut entries: Vec<TimelineEntry> = Vec::new();

    for issue in issues.iter() {
        entries.push(TimelineEntry {
            kind: EntryKind::Issue,
            title: issue.display_title(),
            pubkey: issue.pubkey.clone(),
            created_at: issue.created_at,
            status: issue.status,
        });
    }

    for pr in prs.iter() {
        entries.push(TimelineEntry {
            kind: EntryKind::PullRequest,
            title: pr.display_title(),
            pubkey: pr.pubkey.clone(),
            created_at: pr.created_at,
            status: pr.status,
        });
    }

    // Sort by created_at descending and take top 10
    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    entries.truncate(10);

    if entries.is_empty() {
        return rsx! {
            div { class: "text-center py-8 text-muted-foreground",
                p { "No recent activity found." }
            }
        };
    }

    rsx! {
        div { class: "border border-border rounded-lg divide-y divide-border",
            for entry in entries.iter() {
                TimelineRow { key: "{entry.kind:?}_{entry.pubkey}_{entry.created_at}", entry: entry.clone() }
            }
        }
    }
}

/// Timeline entry types
#[derive(Clone, Debug, PartialEq)]
enum EntryKind {
    Issue,
    PullRequest,
}

/// A single timeline entry (issue or PR)
#[derive(Clone, PartialEq)]
struct TimelineEntry {
    kind: EntryKind,
    title: String,
    pubkey: String,
    created_at: u64,
    status: IssueStatus,
}

/// Single row in the activity timeline
#[component]
fn TimelineRow(entry: TimelineEntry) -> Element {
    let profile = PROFILE_CACHE.read().peek(&entry.pubkey).cloned();
    let author_name = profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&entry.pubkey));

    let (type_icon, type_label) = match entry.kind {
        EntryKind::Issue => (
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="1"/></svg>"#,
            "Issue",
        ),
        EntryKind::PullRequest => (
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="18" cy="18" r="3"/><circle cx="6" cy="6" r="3"/><path d="M13 6h3a2 2 0 0 1 2 2v7"/><line x1="6" y1="9" x2="6" y2="21"/></svg>"#,
            "PR",
        ),
    };

    let status_bg = entry.status.bg_class();
    let time_str = format_relative_time_or(entry.created_at, "Unknown");

    rsx! {
        div { class: "flex items-center gap-3 p-3 hover:bg-accent/50 transition",
            // Type icon
            span {
                class: "w-4 h-4 shrink-0 text-muted-foreground",
                dangerous_inner_html: type_icon,
            }
            // Status dot
            span { class: "w-2 h-2 rounded-full shrink-0 {status_bg}" }
            // Content
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center gap-2",
                    span { class: "text-xs text-muted-foreground font-medium", "{type_label}" }
                    span { class: "text-sm font-medium truncate", "{entry.title}" }
                }
                div { class: "text-xs text-muted-foreground mt-0.5",
                    "by {author_name} - {time_str}"
                }
            }
        }
    }
}

/// Contributors overview with maintainer profiles
#[component]
fn ContributorsOverview(repo: Repository) -> Element {
    // Collect all unique pubkeys: owner + maintainers
    let mut pubkeys = vec![repo.pubkey.clone()];
    for m in &repo.maintainers {
        if !pubkeys.contains(m) {
            pubkeys.push(m.clone());
        }
    }

    if pubkeys.is_empty() {
        return rsx! {
            div { class: "text-center py-8 text-muted-foreground",
                p { "No maintainer information available." }
            }
        };
    }

    rsx! {
        div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3",
            for pubkey in pubkeys.iter() {
                ContributorCard {
                    key: "{pubkey}",
                    pubkey: pubkey.clone(),
                    is_owner: *pubkey == repo.pubkey,
                }
            }
        }
    }
}

/// Individual contributor card
#[component]
fn ContributorCard(pubkey: String, is_owner: bool) -> Element {
    let profile = PROFILE_CACHE.read().peek(&pubkey).cloned();
    let name = profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&pubkey));
    let picture = profile.as_ref().and_then(|p| p.picture.clone());
    let nip05 = profile.as_ref().and_then(|p| p.nip05.clone());

    rsx! {
        Link {
            to: Route::Profile { pubkey: pubkey.clone() },
            class: "flex items-center gap-3 p-3 bg-card border border-border rounded-lg hover:bg-accent/50 transition",
            if let Some(pic) = &picture {
                img {
                    class: "w-10 h-10 rounded-full object-cover shrink-0",
                    src: "{pic}",
                    alt: "{name}",
                }
            } else {
                div { class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center shrink-0 text-muted-foreground font-bold",
                    "{name.chars().next().unwrap_or('?')}"
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center gap-2",
                    span { class: "text-sm font-medium truncate", "{name}" }
                    if is_owner {
                        span { class: "text-xs px-1.5 py-0.5 bg-primary/10 text-primary rounded", "Owner" }
                    } else {
                        span { class: "text-xs px-1.5 py-0.5 bg-muted text-muted-foreground rounded", "Maintainer" }
                    }
                }
                if let Some(nip05_val) = &nip05 {
                    p { class: "text-xs text-muted-foreground truncate", "{nip05_val}" }
                } else {
                    p { class: "text-xs text-muted-foreground truncate", "{truncate_pubkey(&pubkey)}" }
                }
            }
        }
    }
}

/// Error state display
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
                    line { x1: "12", y1: "8", x2: "12", y2: "12" }
                    line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "Failed to load insights" }
            p { class: "text-muted-foreground text-sm mb-4", "{message}" }
            Link { to: Route::CodeHome {}, class: "text-primary hover:underline", "Back to Code" }
        }
    }
}

/// Loading skeleton
#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-6 animate-pulse",
            div { class: "space-y-3",
                div { class: "h-6 bg-muted rounded w-1/3" }
                div { class: "h-4 bg-muted rounded w-2/3" }
            }
            div { class: "flex gap-4 border-b border-border pb-2",
                div { class: "h-6 bg-muted rounded w-20" }
                div { class: "h-6 bg-muted rounded w-16" }
                div { class: "h-6 bg-muted rounded w-24" }
            }
            div { class: "grid grid-cols-2 md:grid-cols-4 gap-4",
                for i in 0..4 {
                    div { key: "{i}", class: "h-24 bg-muted rounded-lg" }
                }
            }
            div { class: "h-48 bg-muted rounded-lg" }
        }
    }
}
