//! Repository Commits Page
//!
//! View commit history for a repository.
//! Tries isomorphic-git (universal) first, falls back to GitHub API.
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::{
    fetch_repository, git_service,
    github_import::{fetch_commits, parse_github_url, GitHubCommit},
};
use crate::services::git_worker::CommitEntry;
use crate::stores::nostr_client;
use crate::utils::{format_commit_date, format_time_ago};
use crate::utils::nip34::Repository;
use dioxus::prelude::*;

/// Split a commit message into its first-line title and optional body.
pub(super) fn split_commit_message(message: &str) -> (&str, Option<&str>) {
    match message.find('\n') {
        Some(idx) => (message[..idx].trim(), Some(message[idx..].trim())),
        None => (message.trim(), None),
    }
}

/// Return the first 7 characters of a hash, or the full string if shorter.
pub(super) fn short_hash(hash: &str) -> &str {
    hash.get(..7).unwrap_or(hash)
}

/// Unified commit data from either source
#[derive(Debug, Clone)]
enum CommitData {
    GitHub(Vec<GitHubCommit>),
    Git(Vec<CommitEntry>),
}

/// Repository commits page component
#[component]
pub fn CodeRepoCommits(naddr: String) -> Element {
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    let mut commits_result = use_signal(|| None::<Result<CommitData, String>>);
    let mut request_gen = use_signal(|| 0u32);
    use_effect(use_reactive(&naddr, move |n| {
        let gen = request_gen.peek().wrapping_add(1);
        request_gen.set(gen);
        commits_result.set(None);
        repo_result.set(None);
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            let result = fetch_repository(&n).await;
            if *request_gen.peek() != gen { return; }
            if let Ok(ref repo) = result {
                // Try isomorphic-git first (works for ALL repo sources)
                if git_service::GitService::is_initialized()
                    || git_service::GitService::init().await.is_ok()
                {
                    match git_service().get_log(repo, None, 50).await {
                        Ok(entries) if !entries.is_empty() => {
                            if *request_gen.peek() != gen { return; }
                            commits_result.set(Some(Ok(CommitData::Git(entries))));
                            repo_result.set(Some(result));
                            return;
                        }
                        Ok(_) => {
                            log::warn!("git_service get_log returned empty, falling back to GitHub API");
                        }
                        Err(e) => {
                            log::warn!("git_service get_log failed, falling back to GitHub API: {}", e);
                        }
                    }
                }
                // Fall back to GitHub API for GitHub-hosted repos
                let mut last_github_err: Option<String> = None;
                for url in repo.clone.iter().chain(repo.web.iter()) {
                    if *request_gen.peek() != gen { return; }
                    if let Some((owner, repo_name)) = parse_github_url(url) {
                        match fetch_commits(&owner, &repo_name, 30).await {
                            Ok(commits) => {
                                if *request_gen.peek() != gen { return; }
                                commits_result.set(Some(Ok(CommitData::GitHub(commits))));
                                repo_result.set(Some(result));
                                return;
                            }
                            Err(e) => {
                                log::warn!("fetch_commits failed for {}/{}: {}", owner, repo_name, e);
                                last_github_err = Some(e);
                                continue;
                            }
                        }
                    }
                }
                if *request_gen.peek() != gen { return; }
                let msg = if let Some(err) = last_github_err {
                    format!("GitHub API error: {}. Try cloning the repository first by browsing its files.", err)
                } else {
                    "Could not load commits. Try cloning the repository first by browsing its files.".to_string()
                };
                commits_result.set(Some(Err(msg)));
            } else if let Err(ref e) = result {
                if *request_gen.peek() != gen { return; }
                commits_result.set(Some(Err(format!("Failed to load repository: {}", e))));
            }
            if *request_gen.peek() != gen { return; }
            repo_result.set(Some(result));
        });
    }));
    let repo_name = match &*repo_result.read() {
        Some(Ok(r)) => r.name.clone().unwrap_or_else(|| r.id.clone()),
        _ => "Repository".to_string(),
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
                    div {
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
                                circle { cx: "12", cy: "12", r: "3" }
                                line {
                                    x1: "3",
                                    y1: "12",
                                    x2: "9",
                                    y2: "12",
                                }
                                line {
                                    x1: "15",
                                    y1: "12",
                                    x2: "21",
                                    y2: "12",
                                }
                            }
                            "Commits"
                        }
                        p { class: "text-sm text-muted-foreground", "{repo_name}" }
                    }
                }
            }
            div { class: "p-4",
                match &*commits_result.read() {
                    Some(Ok(CommitData::GitHub(list))) if !list.is_empty() => rsx! {
                        div { class: "space-y-3",
                            p { class: "text-sm text-muted-foreground mb-4", "{list.len()} commits" }
                            div { class: "border border-border rounded-lg divide-y divide-border",
                                for commit in list.iter() {
                                    GitHubCommitRow { key: "{commit.sha}", commit: commit.clone() }
                                }
                            }
                        }
                    },
                    Some(Ok(CommitData::Git(list))) if !list.is_empty() => rsx! {
                        div { class: "space-y-3",
                            p { class: "text-sm text-muted-foreground mb-4", "{list.len()} commits" }
                            div { class: "border border-border rounded-lg divide-y divide-border",
                                for commit in list.iter() {
                                    GitCommitRow { key: "{commit.oid}", commit: commit.clone() }
                                }
                            }
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        EmptyCommits {}
                    },
                    Some(Err(e)) => rsx! {
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
                            h3 { class: "font-semibold text-lg mb-2", "Failed to load commits" }
                            p { class: "text-muted-foreground text-sm", "{e}" }
                        }
                    },
                    None => rsx! {
                        LoadingSkeleton {}
                    },
                }
            }
        }
    }
}

/// Commit row for GitHub API data
#[component]
fn GitHubCommitRow(commit: GitHubCommit) -> Element {
    let message = &commit.commit.message;
    let (title, body) = split_commit_message(message);
    let date = &commit.commit.author.date;
    let formatted_date = format_commit_date(date);
    let short_sha = short_hash(&commit.sha);
    rsx! {
        div { class: "p-4 hover:bg-muted/50 transition",
            div { class: "flex items-start justify-between gap-4",
                div { class: "flex-1 min-w-0",
                    if commit.html_url.starts_with("https://") || commit.html_url.starts_with("http://") {
                        a {
                            href: "{commit.html_url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            class: "font-medium hover:text-primary truncate block",
                            "{title}"
                        }
                    } else {
                        span {
                            class: "font-medium truncate block",
                            "{title}"
                        }
                    }
                    if let Some(b) = body {
                        if !b.is_empty() {
                            p { class: "text-sm text-muted-foreground mt-1 line-clamp-2",
                                "{b}"
                            }
                        }
                    }
                    div { class: "flex items-center gap-3 mt-2 text-sm text-muted-foreground",
                        if let Some(author) = &commit.author {
                            div { class: "flex items-center gap-2",
                                img {
                                    class: "w-5 h-5 rounded-full",
                                    src: "{author.avatar_url}",
                                    alt: "{author.login}",
                                }
                                span { "{author.login}" }
                            }
                        } else {
                            div { class: "flex items-center gap-2",
                                div { class: "w-5 h-5 rounded-full bg-muted flex items-center justify-center text-xs",
                                    "{commit.commit.author.name.chars().next().unwrap_or('?')}"
                                }
                                span { "{commit.commit.author.name}" }
                            }
                        }
                        span { "committed {formatted_date}" }
                    }
                }
                if commit.html_url.starts_with("https://") || commit.html_url.starts_with("http://") {
                    a {
                        href: "{commit.html_url}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        class: "flex items-center gap-2 px-2 py-1 bg-muted rounded text-xs font-mono hover:bg-accent transition",
                        svg {
                            class: "w-3 h-3 text-muted-foreground",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "12", cy: "12", r: "3" }
                        }
                        "{short_sha}"
                    }
                } else {
                    span {
                        class: "flex items-center gap-2 px-2 py-1 bg-muted rounded text-xs font-mono",
                        svg {
                            class: "w-3 h-3 text-muted-foreground",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "12", cy: "12", r: "3" }
                        }
                        "{short_sha}"
                    }
                }
            }
        }
    }
}

/// Commit row for isomorphic-git data
#[component]
fn GitCommitRow(commit: CommitEntry) -> Element {
    let message = &commit.message;
    let (title, body) = split_commit_message(message);
    let formatted_date = format_time_ago(commit.timestamp);
    let short_oid = short_hash(&commit.oid);
    let initial = commit.author.chars().next().unwrap_or('?');
    rsx! {
        div { class: "p-4 hover:bg-muted/50 transition",
            div { class: "flex items-start justify-between gap-4",
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium truncate", "{title}" }
                    if let Some(b) = body {
                        if !b.is_empty() {
                            p { class: "text-sm text-muted-foreground mt-1 line-clamp-2",
                                "{b}"
                            }
                        }
                    }
                    div { class: "flex items-center gap-3 mt-2 text-sm text-muted-foreground",
                        div { class: "flex items-center gap-2",
                            div { class: "w-5 h-5 rounded-full bg-muted flex items-center justify-center text-xs",
                                "{initial}"
                            }
                            span { "{commit.author}" }
                        }
                        span { "committed {formatted_date}" }
                    }
                }
                span { class: "flex items-center gap-2 px-2 py-1 bg-muted rounded text-xs font-mono",
                    svg {
                        class: "w-3 h-3 text-muted-foreground",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        circle { cx: "12", cy: "12", r: "3" }
                    }
                    "{short_oid}"
                }
            }
        }
    }
}

#[component]
fn EmptyCommits() -> Element {
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
                    circle { cx: "12", cy: "12", r: "3" }
                    line {
                        x1: "3",
                        y1: "12",
                        x2: "9",
                        y2: "12",
                    }
                    line {
                        x1: "15",
                        y1: "12",
                        x2: "21",
                        y2: "12",
                    }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "No Commits" }
            p { class: "text-muted-foreground text-sm", "This repository has no commits yet." }
        }
    }
}
#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-3 animate-pulse",
            for i in 0..5 {
                div { key: "{i}", class: "bg-card p-4 border border-border rounded-lg",
                    div { class: "h-4 bg-muted rounded w-3/4 mb-3" }
                    div { class: "flex items-center gap-3",
                        div { class: "h-5 w-5 bg-muted rounded-full" }
                        div { class: "h-3 bg-muted rounded w-32" }
                    }
                }
            }
        }
    }
}
