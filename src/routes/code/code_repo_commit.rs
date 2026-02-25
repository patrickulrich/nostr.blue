//! Commit Detail Page
//!
//! Displays a single commit with its diff.
use super::code_repo_commits::{short_hash, split_commit_message};
use crate::components::code::{DiffViewer, RepoTabNav};
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::{
    fetch_repository,
    github_import::{parse_github_url, GitHubCommitAuthor, GitHubOwner},
};
use crate::stores::nostr_client;
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

/// Single commit detail with diff
#[derive(Debug, Clone, PartialEq)]
struct CommitDetail {
    message: String,
    author_name: String,
    author_avatar: Option<String>,
    date: String,
    diff: String,
    diff_error: Option<String>,
    additions: u32,
    deletions: u32,
    files_changed: u32,
}

/// GitHub single-commit API response
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubCommitResponse {
    sha: String,
    commit: GitHubCommitMeta,
    author: Option<GitHubOwner>,
    stats: Option<GitHubCommitStats>,
    files: Option<Vec<GitHubCommitFile>>,
}

#[derive(Debug, Deserialize)]
struct GitHubCommitMeta {
    message: String,
    author: GitHubCommitAuthor,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubCommitStats {
    additions: u32,
    deletions: u32,
    total: u32,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubCommitFile {
    filename: String,
    status: String,
    additions: u32,
    deletions: u32,
    patch: Option<String>,
}

/// Commit detail page component
#[component]
pub fn CodeRepoCommit(naddr: String, sha: String) -> Element {
    let mut commit_result = use_signal(|| None::<Result<CommitDetail, String>>);

    let mut request_id = use_signal(|| 0u32);

    let client_ready = *nostr_client::CLIENT_INITIALIZED.read();
    use_effect(
        use_reactive(
            (&naddr, &sha, &client_ready),
            move |(n, s, initialized)| {
                if !initialized {
                    return;
                }
                let current_id = request_id.peek().wrapping_add(1);
                request_id.set(current_id);
                commit_result.set(None);
                spawn(async move {
                    let result = fetch_repository(&n).await;
                    if *request_id.peek() != current_id {
                        return;
                    }
                    match &result {
                        Ok(repo) => {
                            let mut found = false;
                            for url in repo.clone.iter().chain(repo.web.iter()) {
                                if let Some((owner, repo_name)) = parse_github_url(url) {
                                    match fetch_commit_detail(&owner, &repo_name, &s).await {
                                        Ok(detail) => {
                                            if *request_id.peek() != current_id {
                                                return;
                                            }
                                            commit_result.set(Some(Ok(detail)));
                                            found = true;
                                            break;
                                        }
                                        Err(_) => continue,
                                    }
                                }
                            }
                            if !found {
                                commit_result.set(Some(Err(
                                    "No GitHub URL found for this repository".to_string(),
                                )));
                            }
                        }
                        Err(e) => {
                            commit_result
                                .set(Some(Err(format!("Failed to fetch repository: {}", e))));
                        }
                    }
                });
            },
        ),
    );

    rsx! {
        div { class: "min-h-screen",
            // Tab navigation
            RepoTabNav { naddr: naddr.clone(), active_tab: "commits" }

            // Header with back link
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-3",
                    Link {
                        to: Route::CodeRepoCommits {
                            naddr: naddr.clone(),
                        },
                        class: "text-muted-foreground hover:text-foreground",
                        aria_label: "Go back",
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
                                line { x1: "3", y1: "12", x2: "9", y2: "12" }
                                line { x1: "15", y1: "12", x2: "21", y2: "12" }
                            }
                            "Commit"
                        }
                        p { class: "text-sm text-muted-foreground font-mono",
                            "{short_hash(&sha)}"
                        }
                    }
                }
            }

            div { class: "p-4",
                match &*commit_result.read() {
                    Some(Ok(detail)) => rsx! {
                        CommitHeader { detail: detail.clone(), sha: sha.clone() }
                        if let Some(ref err) = detail.diff_error {
                            div { class: "mt-4 p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-sm text-destructive",
                                "{err}"
                            }
                        }
                        if !detail.diff.is_empty() {
                            div { class: "mt-4",
                                DiffViewer { content: detail.diff.clone(), is_cover_letter: false }
                            }
                        }
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
                                    line { x1: "12", y1: "8", x2: "12", y2: "12" }
                                    line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                                }
                            }
                            h3 { class: "font-semibold text-lg mb-2", "Failed to load commit" }
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

/// Commit header showing author, message, and stats
#[component]
fn CommitHeader(detail: CommitDetail, sha: String) -> Element {
    let (title_ref, body_ref) = split_commit_message(&detail.message);
    let title = title_ref.to_string();
    let body = body_ref.map(|b| b.to_string());
    let formatted_date = crate::utils::format_commit_date(&detail.date);
    let short_sha = short_hash(&sha);

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 space-y-4",
            h1 { class: "text-xl font-bold text-foreground", "{title}" }
            if let Some(ref b) = body {
                if !b.is_empty() {
                    pre { class: "text-sm text-muted-foreground whitespace-pre-wrap font-sans mt-2",
                        "{b}"
                    }
                }
            }
            div { class: "flex items-center gap-4 text-sm text-muted-foreground",
                div { class: "flex items-center gap-2",
                    if let Some(ref avatar) = detail.author_avatar {
                        img {
                            class: "w-5 h-5 rounded-full",
                            src: "{avatar}",
                            alt: "{detail.author_name}",
                        }
                    } else {
                        div { class: "w-5 h-5 rounded-full bg-muted flex items-center justify-center text-xs",
                            "{detail.author_name.chars().next().unwrap_or('?')}"
                        }
                    }
                    span { class: "font-medium text-foreground", "{detail.author_name}" }
                }
                span { "committed {formatted_date}" }
                code { class: "px-2 py-1 bg-muted rounded text-xs font-mono",
                    "{short_sha}"
                }
            }
            div { class: "flex gap-4 text-sm",
                span { class: "text-green-500", "+{detail.additions}" }
                span { class: "text-red-500", "-{detail.deletions}" }
                span { class: "text-muted-foreground",
                    "{detail.files_changed} files changed"
                }
            }
        }
    }
}

/// Loading skeleton
#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-4 animate-pulse",
            div { class: "bg-card border border-border rounded-lg p-4 space-y-4",
                div { class: "h-6 bg-muted rounded w-3/4" }
                div { class: "flex items-center gap-4",
                    div { class: "h-5 w-5 bg-muted rounded-full" }
                    div { class: "h-4 bg-muted rounded w-32" }
                    div { class: "h-4 bg-muted rounded w-24" }
                }
                div { class: "flex gap-4",
                    div { class: "h-4 bg-muted rounded w-16" }
                    div { class: "h-4 bg-muted rounded w-16" }
                    div { class: "h-4 bg-muted rounded w-24" }
                }
            }
            div { class: "h-64 bg-muted rounded" }
        }
    }
}

const COMMIT_CACHE_MAX: usize = 50;

thread_local! {
    static COMMIT_CACHE: RefCell<HashMap<String, CommitDetail>> = RefCell::new(HashMap::new());
    static COMMIT_CACHE_ORDER: RefCell<VecDeque<String>> = const { RefCell::new(VecDeque::new()) };
}

/// Fetch commit detail from GitHub API
async fn fetch_commit_detail(
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<CommitDetail, String> {
    // Validate SHA: must be 7-64 hex characters, no path traversal
    if sha.len() < 7
        || sha.len() > 64
        || !sha.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(format!("Invalid commit SHA: {}", sha));
    }

    let cache_key = format!("{}/{}/{}", owner, repo, sha);

    // Check cache first (with LRU promotion on hit)
    let cached = COMMIT_CACHE.with(|c| c.borrow().get(&cache_key).cloned());
    if let Some(detail) = cached {
        // Promote to most-recently-used by moving to back of order queue
        COMMIT_CACHE_ORDER.with(|order| {
            let mut order = order.borrow_mut();
            if let Some(pos) = order.iter().position(|k| k == &cache_key) {
                order.remove(pos);
                order.push_back(cache_key.clone());
            }
        });
        return Ok(detail);
    }

    let api_url = format!(
        "https://api.github.com/repos/{}/{}/commits/{}",
        owner, repo, sha,
    );

    // Fetch JSON metadata (includes files[].patch for diff)
    let meta_resp = Request::get(&api_url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "nostr-blue")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !meta_resp.ok() {
        return Err(format!("GitHub API error: {}", meta_resp.status()));
    }

    let json: GitHubCommitResponse = meta_resp
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    // Build diff from files[].patch (avoids a second API request)
    let (diff, diff_error) = if let Some(ref files) = json.files {
        let mut parts = Vec::new();
        for file in files {
            if let Some(ref patch) = file.patch {
                parts.push(format!("diff --git a/{f} b/{f}", f = file.filename));
                match file.status.as_str() {
                    "added" => {
                        parts.push("--- /dev/null".to_string());
                        parts.push(format!("+++ b/{}", file.filename));
                    }
                    "removed" => {
                        parts.push(format!("--- a/{}", file.filename));
                        parts.push("+++ /dev/null".to_string());
                    }
                    _ => {
                        parts.push(format!("--- a/{}", file.filename));
                        parts.push(format!("+++ b/{}", file.filename));
                    }
                }
                parts.push(patch.clone());
            }
        }
        if parts.is_empty() {
            (String::new(), Some("No patch data found in commit".to_string()))
        } else {
            (parts.join("\n"), None)
        }
    } else {
        (String::new(), Some("No file patches available".to_string()))
    };

    let author_avatar = json.author.as_ref().map(|a| a.avatar_url.clone());
    let author_name = json
        .author
        .as_ref()
        .map(|a| a.login.clone())
        .unwrap_or_else(|| json.commit.author.name.clone());

    let stats = json.stats.as_ref();
    let files_changed = json
        .files
        .as_ref()
        .map(|f| f.len() as u32)
        .unwrap_or(0);

    let detail = CommitDetail {
        message: json.commit.message,
        author_name,
        author_avatar,
        date: json.commit.author.date,
        diff,
        diff_error,
        additions: stats.map(|s| s.additions).unwrap_or(0),
        deletions: stats.map(|s| s.deletions).unwrap_or(0),
        files_changed,
    };

    // Store in cache with bounded eviction
    COMMIT_CACHE.with(|c| {
        COMMIT_CACHE_ORDER.with(|order| {
            let mut cache = c.borrow_mut();
            let mut order = order.borrow_mut();
            if !cache.contains_key(&cache_key) {
                if order.len() >= COMMIT_CACHE_MAX {
                    if let Some(oldest) = order.pop_front() {
                        cache.remove(&oldest);
                    }
                }
                order.push_back(cache_key.clone());
            }
            cache.insert(cache_key, detail.clone());
        });
    });

    Ok(detail)
}
