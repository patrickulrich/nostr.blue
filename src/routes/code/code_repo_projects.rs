//! Project Board
//!
//! Kanban-style view of issues and PRs organized by status.
#![allow(dead_code)]
use crate::components::code::RepoTabNav;
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repo_issues, fetch_repo_prs, fetch_repository};
use crate::stores::nostr_client;
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::nip34::{Issue, IssueStatus, PullRequest};
use crate::utils::time::format_time_ago;
use crate::utils::truncate_pubkey;
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

    fn title(&self) -> String {
        match self {
            Self::Issue(i) => i.subject.clone().unwrap_or_else(|| "Untitled".to_string()),
            Self::PullRequest(p) => p.display_title(),
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
    let mut fetch_error = use_signal(|| None::<String>);
    let mut gen = use_signal(|| 0u32);

    // Fetch repo + issues + PRs
    use_effect(use_reactive(&naddr, move |n| {
        let captured_gen = gen.peek().wrapping_add(1);
        gen.set(captured_gen);
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            repo.set(None);
            items.set(Vec::new());
            fetch_error.set(None);
            loading.set(true);
            return;
        }
        repo.set(None);
        items.set(Vec::new());
        fetch_error.set(None);
        loading.set(true);
        spawn(async move {
            let mut errors = Vec::new();

            // Fetch repo, issues, and PRs concurrently
            let (repo_result, issues_result, prs_result) = futures::join!(
                fetch_repository(&n),
                fetch_repo_issues(&n),
                fetch_repo_prs(&n)
            );
            if *gen.peek() != captured_gen { return; }

            match repo_result {
                Ok(r) => repo.set(Some(r)),
                Err(e) => errors.push(format!("Repository: {}", e)),
            }

            let mut all_items = Vec::new();
            match issues_result {
                Ok(issues) => all_items.extend(issues.into_iter().map(BoardItem::Issue)),
                Err(e) => errors.push(format!("Issues: {}", e)),
            }
            match prs_result {
                Ok(prs) => all_items.extend(prs.into_iter().map(BoardItem::PullRequest)),
                Err(e) => errors.push(format!("PRs: {}", e)),
            }
            if !errors.is_empty() {
                fetch_error.set(Some(errors.join("; ")));
            }
            items.set(all_items);
            loading.set(false);
        });
    }));

    // Group by status (memoized to avoid recomputing on every render)
    let grouped = use_memo(move || {
        let items_read = items.read();
        let mut draft = Vec::new();
        let mut open = Vec::new();
        let mut applied = Vec::new();
        let mut closed = Vec::new();
        for item in items_read.iter() {
            match item.status() {
                IssueStatus::Draft => draft.push(item.clone()),
                IssueStatus::Open => open.push(item.clone()),
                IssueStatus::Applied => applied.push(item.clone()),
                IssueStatus::Closed => closed.push(item.clone()),
            }
        }
        (draft, open, applied, closed)
    });
    let grouped_read = grouped.read();
    let (ref draft, ref open, ref applied, ref closed) = *grouped_read;
    let all_empty = draft.is_empty() && open.is_empty() && applied.is_empty() && closed.is_empty();

    rsx! {
        div { class: "min-h-screen",
            if let Some(r) = repo.read().as_ref() {
                // TODO(#218): add "projects" tab to RepoTabNav; active_tab left empty until tab exists
                RepoTabNav {
                    naddr: naddr.clone(),
                    active_tab: "".to_string(),
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
                } else if let Some(ref err) = *fetch_error.read() {
                    if all_empty {
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
                            h3 { class: "font-semibold text-lg mb-2", "Failed to load project board" }
                            p { class: "text-muted-foreground text-sm", "{err}" }
                        }
                    }
                } else if all_empty {
                    EmptyBoard {}
                }
                // Show error banner + Kanban board when partial data loaded
                if !*loading.read() && !all_empty {
                    if let Some(ref err) = *fetch_error.read() {
                        div { class: "mb-4 p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-sm text-destructive",
                            "{err}"
                        }
                    }
                    // Kanban columns - horizontal scroll
                    div { class: "flex gap-4 overflow-x-auto pb-4",
                        BoardColumn {
                            title: "Draft",
                            color: "bg-yellow-500",
                            count: draft.len(),
                            items: draft.clone(),
                        }
                        BoardColumn {
                            title: "Open",
                            color: "bg-green-500",
                            count: open.len(),
                            items: open.clone(),
                        }
                        BoardColumn {
                            title: "Applied",
                            color: "bg-purple-500",
                            count: applied.len(),
                            items: applied.clone(),
                        }
                        BoardColumn {
                            title: "Closed",
                            color: "bg-red-500",
                            count: closed.len(),
                            items: closed.clone(),
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
    let author_display = {
        let pk = item.pubkey();
        let profile = PROFILE_CACHE.read().peek(pk).cloned();
        profile
            .as_ref()
            .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
            .unwrap_or_else(|| truncate_pubkey(pk))
    };

    rsx! {
        Link {
            to: item.detail_route(),
            class: "block bg-card border border-border rounded-lg p-4 hover:bg-accent/50 transition cursor-pointer",
            // Title
            div { class: "font-medium text-sm mb-2 line-clamp-2", "{item.title()}" }

            // Labels
            if !item.labels().is_empty() {
                div { class: "flex flex-wrap gap-1 mb-2",
                    for label in item.labels().iter().take(3) {
                        span { key: "{label}", class: "px-1.5 py-0.5 text-xs rounded-full bg-blue-500/10 text-blue-400 border border-blue-500/20",
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
                span { class: "truncate", "{author_display}" }
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
