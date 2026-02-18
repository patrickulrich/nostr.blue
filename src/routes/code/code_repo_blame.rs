//! Repository Blame View
//!
//! Displays file content with commit attribution from GitHub.
//! Shows the most recent commits that touched the file and the file content
//! with line numbers. Links to the full blame view on GitHub.
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::{
    fetch_repository,
    file_fetcher,
    git_service::extract_github_info,
    github_import::{fetch_file_commits, GitHubCommit},
};
use crate::stores::nostr_client;
use crate::utils::is_safe_path;
use crate::utils::nip34::Repository;
use dioxus::prelude::*;
/// Repository blame page component
#[component]
pub fn CodeRepoBlame(naddr: String, git_ref: String, path: String) -> Element {
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut file_content = use_signal(String::new);
    let mut commits = use_signal(Vec::<GitHubCommit>::new);
    let mut repo_signal = use_signal(|| None::<Repository>);
    let mut github_info = use_signal(|| None::<(String, String)>);
    let decoded_path = urlencoding::decode(&path)
        .unwrap_or_else(|_| std::borrow::Cow::Borrowed(&path))
        .to_string();
    let path_parts: Vec<&str> = decoded_path.split('/').filter(|s| !s.is_empty()).collect();
    let filename = decoded_path
        .rsplit('/')
        .next()
        .unwrap_or(&decoded_path)
        .to_string();
    let load_key = use_memo({
        let naddr = naddr.clone();
        let git_ref = git_ref.clone();
        let path = path.clone();
        move || format!("{}:{}:{}", naddr, git_ref, path)
    });
    let mut gen = use_signal(|| 0u32);
    use_effect({
        let naddr = naddr.clone();
        let git_ref = git_ref.clone();
        let path = path.clone();
        move || {
            let _key = load_key.read();
            let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
            if !client_initialized {
                return;
            }
            gen += 1;
            let current_gen = *gen.read();
            let naddr = naddr.clone();
            let git_ref = git_ref.clone();
            let path = path.clone();
            spawn(async move {
                loading.set(true);
                error.set(None);
                let repo = match fetch_repository(&naddr).await {
                    Ok(r) => r,
                    Err(e) => {
                        error.set(Some(format!("Repository not found: {}", e)));
                        loading.set(false);
                        return;
                    }
                };
                if *gen.read() != current_gen { return; }
                repo_signal.set(Some(repo.clone()));
                let gh_info = extract_github_info(&repo);
                github_info.set(gh_info.clone());
                if gh_info.is_none() {
                    error.set(Some(
                        "Blame view is only available for GitHub-hosted repositories."
                            .to_string(),
                    ));
                    loading.set(false);
                    return;
                }
                let decoded = urlencoding::decode(&path)
                    .map(|s| s.into_owned())
                    .unwrap_or_else(|_| path.clone());
                if !is_safe_path(&decoded) {
                    log::warn!("Path traversal attempt blocked in blame: {}", decoded);
                    error.set(Some("Invalid path".to_string()));
                    loading.set(false);
                    return;
                }
                // Fetch file content and commit history in parallel
                let (file_result, commits_result) = {
                    let repo_clone = repo.clone();
                    let decoded_clone = decoded.clone();
                    let git_ref_clone = git_ref.clone();
                    let (owner, repo_name) = gh_info.unwrap();
                    let file_future =
                        file_fetcher::fetch_file(&repo_clone, &decoded_clone, Some(&git_ref_clone));
                    let commits_future =
                        fetch_file_commits(&owner, &repo_name, &decoded, &git_ref, 30);
                    futures::join!(file_future, commits_future)
                };
                if *gen.read() != current_gen { return; }
                match file_result {
                    Ok(content) => file_content.set(content),
                    Err(e) => {
                        error.set(Some(format!("Failed to load file: {}", e)));
                        loading.set(false);
                        return;
                    }
                }
                match commits_result {
                    Ok(c) => commits.set(c),
                    Err(e) => {
                        log::warn!("Failed to load commit history: {}", e);
                        // Non-fatal: we can still show the file content
                    }
                }
                loading.set(false);
            });
        }
    });
    let github_blame_url = {
        let gh = github_info();
        let dp = decoded_path.clone();
        let gr = git_ref.clone();
        gh.map(|(owner, repo_name)| {
            format!(
                "https://github.com/{}/{}/blame/{}/{}",
                urlencoding::encode(&owner),
                urlencoding::encode(&repo_name),
                urlencoding::encode(&gr),
                urlencoding::encode(&dp)
            )
        })
    };
    rsx! {
        div { class: "min-h-screen",
            // Sticky header
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-3",
                    Link {
                        to: Route::CodeRepoBlob {
                            naddr: naddr.clone(),
                            git_ref: git_ref.clone(),
                            path: path.clone(),
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
                            path { d: "M17 3a2.828 2.828 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z" }
                        }
                        "Blame"
                    }
                }
            }
            div { class: "p-4 space-y-4",
                // Breadcrumb navigation
                div { class: "flex items-center gap-1.5 text-sm flex-wrap",
                    Link {
                        to: Route::CodeRepo {
                            naddr: naddr.clone(),
                        },
                        class: "text-primary hover:underline",
                        "Repository"
                    }
                    for (i , part) in path_parts.iter().enumerate() {
                        span { key: "{i}", class: "text-muted-foreground", "/" }
                        if i == path_parts.len() - 1 {
                            span { key: "part-{i}", class: "text-foreground font-medium", "{part}" }
                        } else {
                            span { key: "part-{i}", class: "text-muted-foreground", "{part}" }
                        }
                    }
                }
                // Branch badge
                div { class: "flex items-center gap-2 text-sm text-muted-foreground",
                    span { class: "px-2 py-0.5 text-xs font-mono rounded bg-accent", "{git_ref}" }
                }
                // Main content
                if loading() {
                    BlameLoadingSkeleton {}
                } else if let Some(err) = error() {
                    div { class: "bg-card border border-border rounded-lg p-8 text-center",
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
                        h3 { class: "text-lg font-semibold mb-2", "Unable to Load Blame" }
                        p { class: "text-muted-foreground text-sm mb-4", "{err}" }
                        Link {
                            to: Route::CodeRepoBlob {
                                naddr: naddr.clone(),
                                git_ref: git_ref.clone(),
                                path: path.clone(),
                            },
                            class: "inline-block px-4 py-2 bg-muted hover:bg-accent rounded-lg transition text-sm",
                            "Back to File"
                        }
                    }
                } else {
                    // Commit history header
                    if !commits().is_empty() {
                        BlameCommitHeader {
                            commits: commits(),
                            filename: filename.clone(),
                        }
                    }
                    // View on GitHub link
                    if let Some(url) = &github_blame_url {
                        div { class: "flex justify-end",
                            a {
                                href: "{url}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                class: "inline-flex items-center gap-1.5 px-3 py-1.5 text-sm text-primary hover:text-primary/80 bg-muted hover:bg-accent rounded-lg transition",
                                "View full blame on GitHub"
                                span {
                                    class: "w-3.5 h-3.5",
                                    dangerous_inner_html: icons::EXTERNAL_LINK,
                                }
                            }
                        }
                    }
                    // File content with line numbers
                    BlameFileContent { content: file_content(), filename: filename.clone() }
                }
            }
        }
    }
}
/// Displays the commit history header for the file
#[component]
fn BlameCommitHeader(commits: Vec<GitHubCommit>, filename: String) -> Element {
    let latest = &commits[0];
    let message = &latest.commit.message;
    let (title, _) = match message.find('\n') {
        Some(idx) => (message[..idx].trim(), Some(message[idx..].trim())),
        None => (message.as_str(), None),
    };
    let short_sha = if latest.sha.len() >= 7 {
        &latest.sha[..7]
    } else {
        &latest.sha
    };
    let date = &latest.commit.author.date;
    let formatted_date = format_date(date);
    rsx! {
        div { class: "bg-card border border-border rounded-lg overflow-hidden",
            // Latest commit info
            div { class: "p-4 border-b border-border",
                div { class: "flex items-start justify-between gap-4",
                    div { class: "flex-1 min-w-0",
                        div { class: "flex items-center gap-2 mb-1",
                            svg {
                                class: "w-4 h-4 text-muted-foreground flex-shrink-0",
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
                            a {
                                href: "{latest.html_url}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                class: "font-medium text-sm hover:text-primary truncate",
                                "{title}"
                            }
                        }
                        div { class: "flex items-center gap-3 text-xs text-muted-foreground ml-6",
                            if let Some(author) = &latest.author {
                                div { class: "flex items-center gap-1.5",
                                    img {
                                        class: "w-4 h-4 rounded-full",
                                        src: "{author.avatar_url}",
                                        alt: "{author.login}",
                                    }
                                    span { "{author.login}" }
                                }
                            } else {
                                span { "{latest.commit.author.name}" }
                            }
                            span { "{formatted_date}" }
                        }
                    }
                    a {
                        href: "{latest.html_url}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        class: "flex items-center gap-1.5 px-2 py-1 bg-muted rounded text-xs font-mono hover:bg-accent transition flex-shrink-0",
                        "{short_sha}"
                    }
                }
            }
            // Commit count summary
            div { class: "px-4 py-2 bg-muted/50 text-xs text-muted-foreground flex items-center gap-2",
                svg {
                    class: "w-3.5 h-3.5",
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
                if commits.len() == 1 {
                    "{commits.len()} commit touching {filename}"
                } else if commits.len() >= 30 {
                    "30+ commits touching {filename}"
                } else {
                    "{commits.len()} commits touching {filename}"
                }
            }
        }
    }
}
/// Displays the file content with numbered lines
#[component]
fn BlameFileContent(content: String, filename: String) -> Element {
    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len();
    let gutter_width = if line_count >= 1000 {
        "w-16"
    } else if line_count >= 100 {
        "w-12"
    } else {
        "w-10"
    };
    rsx! {
        div { class: "bg-card border border-border rounded-lg overflow-hidden",
            // File header
            div { class: "px-4 py-2 border-b border-border bg-muted/50 flex items-center justify-between",
                div { class: "flex items-center gap-2 text-sm",
                    span {
                        class: "w-4 h-4 text-muted-foreground",
                        dangerous_inner_html: icons::FILE_TEXT,
                    }
                    span { class: "font-medium", "{filename}" }
                }
                span { class: "text-xs text-muted-foreground",
                    "{line_count} lines"
                }
            }
            // File content
            div { class: "overflow-x-auto",
                table { class: "w-full text-sm font-mono",
                    tbody {
                        for (i , line) in lines.iter().enumerate() {
                            tr { key: "{i}", class: "hover:bg-muted/30 transition-colors",
                                // Line number
                                td {
                                    class: "{gutter_width} px-3 py-0 text-right text-muted-foreground/50 select-none border-r border-border/50 align-top",
                                    "{i + 1}"
                                }
                                // Line content
                                td { class: "px-4 py-0 whitespace-pre text-foreground",
                                    "{line}"
                                }
                            }
                        }
                        if lines.is_empty() {
                            tr {
                                td {
                                    class: "px-4 py-8 text-center text-muted-foreground",
                                    colspan: "2",
                                    "Empty file"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Loading skeleton for the blame view
#[component]
fn BlameLoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-4 animate-pulse",
            // Commit header skeleton
            div { class: "bg-card border border-border rounded-lg p-4",
                div { class: "flex items-start justify-between gap-4",
                    div { class: "flex-1",
                        div { class: "h-4 bg-muted rounded w-3/4 mb-3" }
                        div { class: "flex items-center gap-3",
                            div { class: "h-4 w-4 bg-muted rounded-full" }
                            div { class: "h-3 bg-muted rounded w-24" }
                            div { class: "h-3 bg-muted rounded w-20" }
                        }
                    }
                    div { class: "h-6 bg-muted rounded w-16" }
                }
            }
            // File content skeleton
            div { class: "bg-card border border-border rounded-lg overflow-hidden",
                div { class: "px-4 py-2 border-b border-border bg-muted/50",
                    div { class: "h-4 bg-muted rounded w-32" }
                }
                div { class: "p-4 space-y-2",
                    for i in 0..15 {
                        div { key: "{i}", class: "flex gap-4",
                            div { class: "h-4 bg-muted rounded w-8 flex-shrink-0" }
                            div {
                                class: "h-4 bg-muted rounded",
                                style: "width: {40 + (i * 13) % 50}%",
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Format a date string for display
fn format_date(date_str: &str) -> String {
    if let Some(date_part) = date_str.split('T').next() {
        return date_part.to_string();
    }
    date_str.to_string()
}
