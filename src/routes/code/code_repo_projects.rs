//! Project Board
//!
//! Kanban-style view of issues and PRs organized by status.
#![allow(dead_code)]
use crate::components::code::RepoTabNav;
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repo_issues, fetch_repo_prs, fetch_repository};
use crate::stores::nostr_client;
use crate::utils::nip34::{Issue, IssueStatus, PullRequest};
use crate::utils::time::format_time_ago;
use dioxus::prelude::*;

#[derive(Debug, Clone, PartialEq)]
enum BoardItem {
    Issue(Issue),
    PullRequest(PullRequest),
}

impl BoardItem {
    fn status(&self) -> IssueStatus {
        match self {
            Self::Issue(i) => i.status,
            Self::PullRequest(p) => p.status,
        }
    }

    fn title(&self) -> &str {
        match self {
            Self::Issue(i) => i.subject.as_deref().unwrap_or("Untitled"),
            Self::PullRequest(p) => p.content.lines().next().unwrap_or("Untitled"),
        }
    }

    fn event_id(&self) -> &str {
        match self {
            Self::Issue(i) => &i.event_id,
            Self::PullRequest(p) => &p.event_id,
        }
    }

    fn pubkey(&self) -> &str {
        match self {
            Self::Issue(i) => &i.pubkey,
            Self::PullRequest(p) => &p.pubkey,
        }
    }

    fn created_at(&self) -> u64 {
        match self {
            Self::Issue(i) => i.created_at,
            Self::PullRequest(p) => p.created_at,
        }
    }

    fn is_pr(&self) -> bool {
        matches!(self, Self::PullRequest(_))
    }

    fn detail_route(&self) -> Route {
        match self {
            Self::Issue(i) => Route::CodeIssueDetail {
                note_id: i.event_id.clone(),
            },
            Self::PullRequest(p) => Route::CodePullDetail {
                note_id: p.event_id.clone(),
            },
        }
    }

    fn labels(&self) -> &[String] {
        match self {
            Self::Issue(i) => &i.labels,
            Self::PullRequest(p) => &p.labels,
        }
    }
}

#[component]
pub fn CodeRepoProjects(naddr: String) -> Element {
    let mut items = use_signal(Vec::<BoardItem>::new);
    let mut loading = use_signal(|| true);
    let mut repo = use_signal(|| None);

    // Fetch repo + issues + PRs
    let naddr_for_effect = naddr.clone();
    use_effect(move || {
        let n = naddr_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);

            // Fetch repo metadata
            if let Ok(r) = fetch_repository(&n).await {
                repo.set(Some(r));
            }

            // Fetch issues and PRs
            let mut all_items = Vec::new();
            if let Ok(issues) = fetch_repo_issues(&n).await {
                all_items.extend(issues.into_iter().map(BoardItem::Issue));
            }
            if let Ok(prs) = fetch_repo_prs(&n).await {
                all_items.extend(prs.into_iter().map(BoardItem::PullRequest));
            }
            items.set(all_items);
            loading.set(false);
        });
    });

    // Group by status
    let items_read = items.read();
    let draft: Vec<_> = items_read
        .iter()
        .filter(|i| i.status() == IssueStatus::Draft)
        .cloned()
        .collect();
    let open: Vec<_> = items_read
        .iter()
        .filter(|i| i.status() == IssueStatus::Open)
        .cloned()
        .collect();
    let applied: Vec<_> = items_read
        .iter()
        .filter(|i| i.status() == IssueStatus::Applied)
        .cloned()
        .collect();
    let closed: Vec<_> = items_read
        .iter()
        .filter(|i| i.status() == IssueStatus::Closed)
        .cloned()
        .collect();

    rsx! {
        div { class: "min-h-screen",
            if let Some(r) = repo.read().as_ref() {
                RepoTabNav {
                    naddr: naddr.clone(),
                    active_tab: "projects".to_string(),
                    issue_count: Some(r.issue_count),
                    pr_count: Some(r.pr_count),
                }
            }

            div { class: "p-4",
                h2 { class: "text-lg font-bold mb-4 flex items-center gap-2",
                    // Kanban board icon
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
                        rect { x: "3", y: "3", width: "7", height: "18", rx: "1" }
                        rect { x: "14", y: "3", width: "7", height: "10", rx: "1" }
                    }
                    "Project Board"
                }

                if *loading.read() {
                    LoadingSkeleton {}
                } else if items_read.is_empty() {
                    EmptyBoard {}
                } else {
                    // Kanban columns - horizontal scroll
                    div { class: "flex gap-4 overflow-x-auto pb-4",
                        BoardColumn {
                            title: "Draft",
                            color: "bg-muted-foreground",
                            count: draft.len(),
                            items: draft,
                        }
                        BoardColumn {
                            title: "Open",
                            color: "bg-blue-500",
                            count: open.len(),
                            items: open,
                        }
                        BoardColumn {
                            title: "Applied",
                            color: "bg-green-500",
                            count: applied.len(),
                            items: applied,
                        }
                        BoardColumn {
                            title: "Closed",
                            color: "bg-red-500",
                            count: closed.len(),
                            items: closed,
                        }
                    }
                }
            }
        }
    }
}

/// A single kanban column showing items of one status
#[component]
fn BoardColumn(title: &'static str, color: &'static str, count: usize, items: Vec<BoardItem>) -> Element {
    rsx! {
        div { class: "min-w-72 flex-shrink-0 bg-muted/30 rounded-lg",
            // Column header
            div { class: "flex items-center gap-2 p-3 border-b border-border",
                span { class: "w-3 h-3 rounded-full {color}" }
                span { class: "font-semibold text-sm", "{title}" }
                span { class: "ml-auto px-2 py-0.5 text-xs rounded-full bg-muted text-muted-foreground",
                    "{count}"
                }
            }

            // Cards
            div { class: "p-2 space-y-2 max-h-[70vh] overflow-y-auto",
                if items.is_empty() {
                    div { class: "text-center py-8 text-sm text-muted-foreground",
                        "No items"
                    }
                } else {
                    for item in items.iter() {
                        BoardCard { key: "{item.event_id()}", item: item.clone() }
                    }
                }
            }
        }
    }
}

/// A single card within a kanban column
#[component]
fn BoardCard(item: BoardItem) -> Element {
    let time_ago = format_time_ago(item.created_at());
    let truncated_author = {
        let pk = item.pubkey();
        if pk.len() > 12 {
            format!("{}...{}", &pk[..6], &pk[pk.len() - 4..])
        } else {
            pk.to_string()
        }
    };

    rsx! {
        Link {
            to: item.detail_route(),
            class: "block bg-card border border-border rounded-lg p-3 hover:bg-accent/50 transition cursor-pointer",
            // Title
            div { class: "font-medium text-sm mb-2 line-clamp-2", "{item.title()}" }

            // Labels
            if !item.labels().is_empty() {
                div { class: "flex flex-wrap gap-1 mb-2",
                    for label in item.labels().iter().take(3) {
                        span { class: "px-1.5 py-0.5 text-xs rounded-full bg-blue-500/10 text-blue-400 border border-blue-500/20",
                            "{label}"
                        }
                    }
                }
            }

            // Footer: type badge + author + time
            div { class: "flex items-center gap-2 text-xs text-muted-foreground",
                // Type badge
                if item.is_pr() {
                    span { class: "flex items-center gap-1 px-1.5 py-0.5 rounded bg-purple-500/10 text-purple-400",
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
                            circle { cx: "18", cy: "18", r: "3" }
                            circle { cx: "6", cy: "6", r: "3" }
                            path { d: "M13 6h3a2 2 0 0 1 2 2v7" }
                            line { x1: "6", y1: "9", x2: "6", y2: "21" }
                        }
                        "PR"
                    }
                } else {
                    span { class: "flex items-center gap-1 px-1.5 py-0.5 rounded bg-green-500/10 text-green-400",
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
                            circle { cx: "12", cy: "12", r: "10" }
                            circle { cx: "12", cy: "12", r: "1" }
                        }
                        "Issue"
                    }
                }
                span { class: "truncate", "{truncated_author}" }
                span { class: "ml-auto whitespace-nowrap", "{time_ago}" }
            }
        }
    }
}

/// Empty state when no issues or PRs exist
#[component]
fn EmptyBoard() -> Element {
    rsx! {
        div { class: "text-center py-12",
            div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
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
                    rect { x: "3", y: "3", width: "7", height: "18", rx: "1" }
                    rect { x: "14", y: "3", width: "7", height: "10", rx: "1" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "No Items Yet" }
            p { class: "text-muted-foreground text-sm",
                "Create issues or pull requests to see them organized on this board."
            }
        }
    }
}

/// Loading skeleton for the board
#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "flex gap-4 overflow-x-auto pb-4 animate-pulse",
            for i in 0..4 {
                div { key: "{i}", class: "min-w-72 flex-shrink-0 bg-muted/30 rounded-lg",
                    div { class: "p-3 border-b border-border",
                        div { class: "h-4 bg-muted rounded w-20" }
                    }
                    div { class: "p-2 space-y-2",
                        for j in 0..3 {
                            div { key: "{j}", class: "bg-card border border-border rounded-lg p-3",
                                div { class: "h-4 bg-muted rounded w-3/4 mb-2" }
                                div { class: "h-3 bg-muted rounded w-1/2" }
                            }
                        }
                    }
                }
            }
        }
    }
}
