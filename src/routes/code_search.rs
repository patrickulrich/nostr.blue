//! Code Search Page
//!
//! Search repositories, issues, PRs, and code snippets.

use dioxus::prelude::*;
use crate::components::{icons, CodeRepoCard, CodeSnippetCard, CodeIssueRow, CodePullRow};
use crate::routes::Route;
use crate::services::git_hosting::{search_repositories, search_snippets, search_issues, search_prs};
use crate::stores::nostr_client;
use crate::utils::nip34::{Repository, DisplaySnippet, Issue, PullRequest};

/// Code search page component
#[component]
pub fn CodeSearch(q: String) -> Element {
    let query = q.clone();
    let mut search_input = use_signal(|| q.clone());
    let mut active_filter = use_signal(|| SearchFilter::All);
    let nav = use_navigator();

    // Search result signals
    let mut repos = use_signal(|| None::<Result<Vec<Repository>, String>>);
    let mut snippets = use_signal(|| None::<Result<Vec<DisplaySnippet>, String>>);
    let mut issues = use_signal(|| None::<Result<Vec<Issue>, String>>);
    let mut prs = use_signal(|| None::<Result<Vec<PullRequest>, String>>);

    // Clone for effect
    let query_for_effect = query.clone();

    // Search - wait for client initialization
    use_effect(move || {
        let q = query_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        spawn(async move {
            if q.is_empty() {
                repos.set(Some(Ok(vec![])));
                snippets.set(Some(Ok(vec![])));
                issues.set(Some(Ok(vec![])));
                prs.set(Some(Ok(vec![])));
            } else {
                // Run searches in parallel using join
                let (repos_res, snippets_res, issues_res, prs_res) = futures::join!(
                    search_repositories(&q, 20),
                    search_snippets(&q, 20),
                    search_issues(&q, 20),
                    search_prs(&q, 20)
                );
                repos.set(Some(repos_res));
                snippets.set(Some(snippets_res));
                issues.set(Some(issues_res));
                prs.set(Some(prs_res));
            }
        });
    });

    let handle_search = move |_| {
        let new_query = search_input.read().clone();
        if !new_query.is_empty() {
            nav.push(Route::CodeSearch { q: new_query });
        }
    };

    let handle_key_press = move |e: KeyboardEvent| {
        if e.key() == Key::Enter {
            let new_query = search_input.read().clone();
            if !new_query.is_empty() {
                nav.push(Route::CodeSearch { q: new_query });
            }
        }
    };

    // Count results
    let repo_count = repos.read().as_ref().and_then(|r| r.as_ref().ok()).map(|v| v.len()).unwrap_or(0);
    let snippet_count = snippets.read().as_ref().and_then(|r| r.as_ref().ok()).map(|v| v.len()).unwrap_or(0);
    let issue_count = issues.read().as_ref().and_then(|r| r.as_ref().ok()).map(|v| v.len()).unwrap_or(0);
    let pr_count = prs.read().as_ref().and_then(|r| r.as_ref().ok()).map(|v| v.len()).unwrap_or(0);
    let total_count = repo_count + snippet_count + issue_count + pr_count;

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
                            circle { cx: "11", cy: "11", r: "8" }
                            line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
                        }
                        "Search"
                    }
                }
            }

            // Content
            div {
                class: "p-4 space-y-6",

                // Search input
                div {
                    class: "flex gap-2",
                    div {
                        class: "flex-1 relative",
                        input {
                            class: "w-full pl-10 pr-4 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Search repositories, snippets, issues...",
                            value: "{search_input}",
                            oninput: move |e| search_input.set(e.value()),
                            onkeypress: handle_key_press
                        }
                        div {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
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
                                circle { cx: "11", cy: "11", r: "8" }
                                line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
                            }
                        }
                    }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:opacity-90 transition",
                        onclick: handle_search,
                        "Search"
                    }
                }

                // Results info
                if !query.is_empty() {
                    div {
                        class: "text-sm text-muted-foreground",
                        "{total_count} results for \"{query}\""
                    }
                }

                // Filter tabs
                div {
                    class: "flex gap-2 overflow-x-auto pb-2",
                    FilterChip {
                        label: "All",
                        count: total_count,
                        active: *active_filter.read() == SearchFilter::All,
                        onclick: move |_| active_filter.set(SearchFilter::All)
                    }
                    FilterChip {
                        label: "Repositories",
                        count: repo_count,
                        active: *active_filter.read() == SearchFilter::Repositories,
                        onclick: move |_| active_filter.set(SearchFilter::Repositories)
                    }
                    FilterChip {
                        label: "Snippets",
                        count: snippet_count,
                        active: *active_filter.read() == SearchFilter::Snippets,
                        onclick: move |_| active_filter.set(SearchFilter::Snippets)
                    }
                    FilterChip {
                        label: "Issues",
                        count: issue_count,
                        active: *active_filter.read() == SearchFilter::Issues,
                        onclick: move |_| active_filter.set(SearchFilter::Issues)
                    }
                    FilterChip {
                        label: "PRs",
                        count: pr_count,
                        active: *active_filter.read() == SearchFilter::PullRequests,
                        onclick: move |_| active_filter.set(SearchFilter::PullRequests)
                    }
                }

                // Results
                if query.is_empty() {
                    EmptySearch {}
                } else {
                    div {
                        class: "space-y-6",

                        // Repositories
                        if *active_filter.read() == SearchFilter::All || *active_filter.read() == SearchFilter::Repositories {
                            if repo_count > 0 {
                                ResultSection {
                                    title: "Repositories",
                                    match &*repos.read() {
                                        Some(Ok(list)) => rsx! {
                                            div {
                                                class: "grid gap-4",
                                                for repo in list.iter() {
                                                    CodeRepoCard {
                                                        key: "{repo.event_id}",
                                                        repo: repo.clone()
                                                    }
                                                }
                                            }
                                        },
                                        Some(Err(e)) => rsx! {
                                            p { class: "text-sm text-destructive", "{e}" }
                                        },
                                        None => rsx! {
                                            LoadingItems {}
                                        },
                                    }
                                }
                            }
                        }

                        // Snippets
                        if *active_filter.read() == SearchFilter::All || *active_filter.read() == SearchFilter::Snippets {
                            if snippet_count > 0 {
                                ResultSection {
                                    title: "Code Snippets",
                                    match &*snippets.read() {
                                        Some(Ok(list)) => rsx! {
                                            div {
                                                class: "grid gap-4",
                                                for snippet in list.iter() {
                                                    CodeSnippetCard {
                                                        key: "{snippet.event_id}",
                                                        snippet: snippet.clone()
                                                    }
                                                }
                                            }
                                        },
                                        Some(Err(e)) => rsx! {
                                            p { class: "text-sm text-destructive", "{e}" }
                                        },
                                        None => rsx! {
                                            LoadingItems {}
                                        },
                                    }
                                }
                            }
                        }

                        // Issues
                        if *active_filter.read() == SearchFilter::All || *active_filter.read() == SearchFilter::Issues {
                            if issue_count > 0 {
                                ResultSection {
                                    title: "Issues",
                                    match &*issues.read() {
                                        Some(Ok(list)) => rsx! {
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
                                        Some(Err(e)) => rsx! {
                                            p { class: "text-sm text-destructive", "{e}" }
                                        },
                                        None => rsx! {
                                            LoadingItems {}
                                        },
                                    }
                                }
                            }
                        }

                        // PRs
                        if *active_filter.read() == SearchFilter::All || *active_filter.read() == SearchFilter::PullRequests {
                            if pr_count > 0 {
                                ResultSection {
                                    title: "Pull Requests",
                                    match &*prs.read() {
                                        Some(Ok(list)) => rsx! {
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
                                        Some(Err(e)) => rsx! {
                                            p { class: "text-sm text-destructive", "{e}" }
                                        },
                                        None => rsx! {
                                            LoadingItems {}
                                        },
                                    }
                                }
                            }
                        }

                        // No results
                        if total_count == 0 && repos.read().is_some() {
                            NoResults { query: query.clone() }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SearchFilter {
    All,
    Repositories,
    Snippets,
    Issues,
    PullRequests,
}

#[component]
fn FilterChip(label: &'static str, count: usize, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let class = if active {
        "px-3 py-1.5 text-sm rounded-full bg-primary text-primary-foreground flex items-center gap-2"
    } else {
        "px-3 py-1.5 text-sm rounded-full bg-muted text-muted-foreground hover:bg-accent transition flex items-center gap-2"
    };

    rsx! {
        button {
            class: "{class}",
            onclick: move |e| onclick.call(e),
            "{label}"
            span {
                class: if active { "px-1.5 py-0.5 text-xs rounded-full bg-primary-foreground/20" } else { "px-1.5 py-0.5 text-xs rounded-full bg-background" },
                "{count}"
            }
        }
    }
}

#[component]
fn ResultSection(title: &'static str, children: Element) -> Element {
    rsx! {
        div {
            class: "space-y-3",
            h2 {
                class: "font-semibold text-lg",
                "{title}"
            }
            {children}
        }
    }
}

#[component]
fn EmptySearch() -> Element {
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
                    circle { cx: "11", cy: "11", r: "8" }
                    line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "Search Code" }
            p { class: "text-muted-foreground text-sm", "Find repositories, code snippets, issues, and pull requests" }
        }
    }
}

#[component]
fn NoResults(query: String) -> Element {
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
                    line { x1: "8", y1: "12", x2: "16", y2: "12" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "No Results Found" }
            p { class: "text-muted-foreground text-sm", "No matches for \"{query}\". Try a different search term." }
        }
    }
}

#[component]
fn LoadingItems() -> Element {
    rsx! {
        div {
            class: "space-y-3 animate-pulse",
            for i in 0..3 {
                div {
                    key: "{i}",
                    class: "p-4 border border-border rounded-lg",
                    div { class: "h-4 bg-muted rounded w-2/3 mb-2" }
                    div { class: "h-3 bg-muted rounded w-1/2" }
                }
            }
        }
    }
}
