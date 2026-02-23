//! Code Search Page
//!
//! Search repositories, issues, PRs, and code snippets.
//! Supports advanced query syntax:
//! - `is:open` / `is:closed` - filter by status
//! - `label:bug` - filter by label
//! - `type:issue` / `type:pr` / `type:repo` / `type:snippet` - filter by entity type
//! - `author:npub...` - filter by author
//!
//! Plain text words are used as the search query.
use crate::components::{icons, CodeIssueRow, CodePullRow, CodeRepoCard, CodeSnippetCard};
use crate::routes::Route;
use crate::services::git_hosting::{
    search_issues, search_prs, search_repositories, search_snippets,
};
use crate::stores::nostr_client;
use crate::utils::nip34::{DisplaySnippet, Issue, IssueStatus, PullRequest, Repository};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr_sdk::PublicKey;

/// Parsed search query with structured filters
#[derive(Clone, Debug, Default)]
struct ParsedQuery {
    /// Plain text search terms
    text: String,
    /// Status filter (open/closed)
    status: Option<IssueStatus>,
    /// Label filters
    labels: Vec<String>,
    /// Type filter (issue/pr/repo/snippet)
    entity_type: Option<String>,
    /// Author filter (pubkey)
    author: Option<String>,
}

impl ParsedQuery {
    fn parse(input: &str) -> Self {
        let mut text_parts = Vec::new();
        let mut query = ParsedQuery::default();

        for token in input.split_whitespace() {
            if let Some(value) = token.strip_prefix("is:") {
                match value.to_lowercase().as_str() {
                    "open" => query.status = Some(IssueStatus::Open),
                    "closed" => query.status = Some(IssueStatus::Closed),
                    "merged" | "applied" => query.status = Some(IssueStatus::Applied),
                    "draft" => query.status = Some(IssueStatus::Draft),
                    _ => text_parts.push(token.to_string()),
                }
            } else if let Some(value) = token.strip_prefix("label:") {
                if !value.is_empty() {
                    query.labels.push(value.to_lowercase());
                }
            } else if let Some(value) = token.strip_prefix("type:") {
                match value.to_lowercase().as_str() {
                    "issue" | "issues" => query.entity_type = Some("issue".to_string()),
                    "pr" | "pull" | "prs" => query.entity_type = Some("pr".to_string()),
                    "repo" | "repository" | "repos" => query.entity_type = Some("repo".to_string()),
                    "snippet" | "snippets" | "code" => query.entity_type = Some("snippet".to_string()),
                    _ => text_parts.push(token.to_string()),
                }
            } else if let Some(value) = token.strip_prefix("author:") {
                if !value.is_empty() {
                    // Normalize npub bech32 to hex; fall back to raw string
                    let normalized = PublicKey::parse(value)
                        .map(|pk| pk.to_hex())
                        .unwrap_or_else(|_| value.to_string());
                    query.author = Some(normalized);
                }
            } else {
                text_parts.push(token.to_string());
            }
        }

        query.text = text_parts.join(" ");
        query
    }

    fn has_filters(&self) -> bool {
        self.status.is_some()
            || !self.labels.is_empty()
            || self.entity_type.is_some()
            || self.author.is_some()
    }

    fn matches_issue(&self, issue: &Issue) -> bool {
        if let Some(ref status) = self.status {
            if issue.status != *status {
                return false;
            }
        }
        if !self.labels.is_empty()
            && !self.labels.iter().all(|l| issue.labels.iter().any(|il| il.eq_ignore_ascii_case(l)))
        {
            return false;
        }
        if let Some(ref author) = self.author {
            if issue.pubkey != *author {
                return false;
            }
        }
        true
    }

    fn matches_pr(&self, pr: &PullRequest) -> bool {
        if let Some(ref status) = self.status {
            if pr.status != *status {
                return false;
            }
        }
        if !self.labels.is_empty()
            && !self.labels.iter().all(|l| pr.labels.iter().any(|il| il.eq_ignore_ascii_case(l)))
        {
            return false;
        }
        if let Some(ref author) = self.author {
            if pr.pubkey != *author {
                return false;
            }
        }
        true
    }

    fn matches_repo(&self, repo: &Repository) -> bool {
        if let Some(ref author) = self.author {
            if repo.pubkey != *author {
                return false;
            }
        }
        if !self.labels.is_empty()
            && !self.labels.iter().all(|l| repo.topics.iter().any(|t| t.eq_ignore_ascii_case(l)))
        {
            return false;
        }
        true
    }

    fn matches_snippet(&self, snippet: &DisplaySnippet) -> bool {
        if let Some(ref author) = self.author {
            if snippet.pubkey != *author {
                return false;
            }
        }
        true
    }

    fn show_repos(&self) -> bool {
        self.entity_type.is_none() || self.entity_type.as_deref() == Some("repo")
    }

    fn show_snippets(&self) -> bool {
        self.entity_type.is_none() || self.entity_type.as_deref() == Some("snippet")
    }

    fn show_issues(&self) -> bool {
        self.entity_type.is_none() || self.entity_type.as_deref() == Some("issue")
    }

    fn show_prs(&self) -> bool {
        self.entity_type.is_none() || self.entity_type.as_deref() == Some("pr")
    }
}
/// Code search page component
#[component]
pub fn CodeSearch(q: String) -> Element {
    let query = q.clone();
    let mut search_input = use_signal(|| q.clone());
    // Sync search_input when the route query parameter changes (e.g., navigating back)
    let q_for_sync = q.clone();
    use_effect(use_reactive(&q_for_sync, move |q_val| {
        search_input.set(q_val);
    }));
    let mut active_filter = use_signal(|| SearchFilter::All);
    let nav = use_navigator();
    let mut repos = use_signal(|| None::<Result<Vec<Repository>, String>>);
    let mut snippets = use_signal(|| None::<Result<Vec<DisplaySnippet>, String>>);
    let mut issues = use_signal(|| None::<Result<Vec<Issue>, String>>);
    let mut prs = use_signal(|| None::<Result<Vec<PullRequest>, String>>);
    let parsed = ParsedQuery::parse(&query);
    let parsed_for_filter = parsed.clone();
    let query_for_effect = query.clone();
    let mut request_gen = use_signal(|| 0u32);
    use_effect(use_reactive(&query_for_effect, move |q| {
        let parsed = ParsedQuery::parse(&q);
        // When the user specified only structured filters (e.g. `is:open label:bug`)
        // with no free-text, pass None so the relay returns all events (no .search()
        // filter). Client-side filters in `matches_issue`/`matches_pr` narrow results.
        let search_text: Option<String> = if parsed.text.is_empty() {
            None
        } else {
            Some(parsed.text.clone())
        };
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let gen = request_gen.peek().wrapping_add(1);
        request_gen.set(gen);
        if q.is_empty() {
            repos.set(Some(Ok(vec![])));
            snippets.set(Some(Ok(vec![])));
            issues.set(Some(Ok(vec![])));
            prs.set(Some(Ok(vec![])));
            return;
        }
        spawn(async move {
            {
                // Only fetch entity types that will be displayed (based on type: filter)
                let repos_fut = if parsed.show_repos() {
                    Some(search_repositories(search_text.as_deref(), 20))
                } else { None };
                let snippets_fut = if parsed.show_snippets() {
                    Some(search_snippets(search_text.as_deref(), 20))
                } else { None };
                let issues_fut = if parsed.show_issues() {
                    Some(search_issues(search_text.as_deref(), 20))
                } else { None };
                let prs_fut = if parsed.show_prs() {
                    Some(search_prs(search_text.as_deref(), 20))
                } else { None };

                // Use OptionFuture for conditional parallel execution
                let (repos_res, snippets_res, issues_res, prs_res) = futures::join!(
                    async { match repos_fut { Some(f) => Some(f.await), None => None } },
                    async { match snippets_fut { Some(f) => Some(f.await), None => None } },
                    async { match issues_fut { Some(f) => Some(f.await), None => None } },
                    async { match prs_fut { Some(f) => Some(f.await), None => None } }
                );
                if *request_gen.peek() != gen { return; }
                // Apply client-side filters and set results
                repos.set(Some(repos_res
                    .unwrap_or(Ok(vec![]))
                    .map(|list| list.into_iter().filter(|r| parsed.matches_repo(r)).collect())));
                snippets.set(Some(snippets_res
                    .unwrap_or(Ok(vec![]))
                    .map(|list| list.into_iter().filter(|s| parsed.matches_snippet(s)).collect())));
                issues.set(Some(issues_res
                    .unwrap_or(Ok(vec![]))
                    .map(|list| list.into_iter().filter(|i| parsed.matches_issue(i)).collect())));
                prs.set(Some(prs_res
                    .unwrap_or(Ok(vec![]))
                    .map(|list| list.into_iter().filter(|p| parsed.matches_pr(p)).collect())));
            }
        });
    }));
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
    let repo_count = repos
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|v| v.len())
        .unwrap_or(0);
    let snippet_count = snippets
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|v| v.len())
        .unwrap_or(0);
    let issue_count = issues
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|v| v.len())
        .unwrap_or(0);
    let pr_count = prs
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|v| v.len())
        .unwrap_or(0);
    let total_count = repo_count + snippet_count + issue_count + pr_count;
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
                            circle { cx: "11", cy: "11", r: "8" }
                            line {
                                x1: "21",
                                y1: "21",
                                x2: "16.65",
                                y2: "16.65",
                            }
                        }
                        "Search"
                    }
                }
            }
            div { class: "p-4 space-y-6",
                div { class: "flex gap-2",
                    div { class: "flex-1 relative",
                        input {
                            class: "w-full pl-10 pr-4 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Search... (try is:open label:bug type:issue)",
                            value: "{search_input}",
                            oninput: move |e| search_input.set(e.value()),
                            onkeypress: handle_key_press,
                        }
                        div { class: "absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
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
                                line {
                                    x1: "21",
                                    y1: "21",
                                    x2: "16.65",
                                    y2: "16.65",
                                }
                            }
                        }
                    }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:opacity-90 transition",
                        onclick: handle_search,
                        "Search"
                    }
                }
                if !query.is_empty() {
                    div { class: "text-sm text-muted-foreground",
                        "{total_count} results for \"{query}\""
                    }
                    // Show active search filters
                    if parsed_for_filter.has_filters() {
                        div { class: "flex flex-wrap gap-1.5",
                            if let Some(ref status) = parsed_for_filter.status {
                                {
                                    let status_label = match status {
                                        IssueStatus::Open => "open",
                                        IssueStatus::Closed => "closed",
                                        IssueStatus::Applied => "merged",
                                        IssueStatus::Draft => "draft",
                                    };
                                    rsx! {
                                        span { class: "px-2 py-0.5 rounded-full text-xs bg-blue-500/10 text-blue-500 border border-blue-500/20",
                                            "is:{status_label}"
                                        }
                                    }
                                }
                            }
                            for label in parsed_for_filter.labels.iter() {
                                span {
                                    key: "{label}",
                                    class: "px-2 py-0.5 rounded-full text-xs bg-purple-500/10 text-purple-500 border border-purple-500/20",
                                    "label:{label}"
                                }
                            }
                            if let Some(ref entity_type) = parsed_for_filter.entity_type {
                                span { class: "px-2 py-0.5 rounded-full text-xs bg-green-500/10 text-green-500 border border-green-500/20",
                                    "type:{entity_type}"
                                }
                            }
                            if let Some(ref author) = parsed_for_filter.author {
                                span { class: "px-2 py-0.5 rounded-full text-xs bg-orange-500/10 text-orange-500 border border-orange-500/20",
                                    "author:{truncate_pubkey(author)}"
                                }
                            }
                        }
                    }
                }
                div { class: "flex gap-2 overflow-x-auto pb-2",
                    FilterChip {
                        label: "All",
                        count: total_count,
                        active: *active_filter.read() == SearchFilter::All,
                        onclick: move |_| active_filter.set(SearchFilter::All),
                    }
                    FilterChip {
                        label: "Repositories",
                        count: repo_count,
                        active: *active_filter.read() == SearchFilter::Repositories,
                        onclick: move |_| active_filter.set(SearchFilter::Repositories),
                    }
                    FilterChip {
                        label: "Snippets",
                        count: snippet_count,
                        active: *active_filter.read() == SearchFilter::Snippets,
                        onclick: move |_| active_filter.set(SearchFilter::Snippets),
                    }
                    FilterChip {
                        label: "Issues",
                        count: issue_count,
                        active: *active_filter.read() == SearchFilter::Issues,
                        onclick: move |_| active_filter.set(SearchFilter::Issues),
                    }
                    FilterChip {
                        label: "PRs",
                        count: pr_count,
                        active: *active_filter.read() == SearchFilter::PullRequests,
                        onclick: move |_| active_filter.set(SearchFilter::PullRequests),
                    }
                }
                if query.is_empty() {
                    EmptySearch {}
                } else {
                    div { class: "space-y-6",
                        if *active_filter.read() == SearchFilter::All
                            || *active_filter.read() == SearchFilter::Repositories
                        {
                            if repo_count > 0 {
                                ResultSection { title: "Repositories",
                                    match &*repos.read() {
                                        Some(Ok(list)) => rsx! {
                                            div { class: "grid gap-4",
                                                for repo in list.iter() {
                                                    CodeRepoCard { key: "{repo.event_id}", repo: repo.clone() }
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
                        if *active_filter.read() == SearchFilter::All
                            || *active_filter.read() == SearchFilter::Snippets
                        {
                            if snippet_count > 0 {
                                ResultSection { title: "Code Snippets",
                                    match &*snippets.read() {
                                        Some(Ok(list)) => rsx! {
                                            div { class: "grid gap-4",
                                                for snippet in list.iter() {
                                                    CodeSnippetCard { key: "{snippet.event_id}", snippet: snippet.clone() }
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
                        if *active_filter.read() == SearchFilter::All
                            || *active_filter.read() == SearchFilter::Issues
                        {
                            if issue_count > 0 {
                                ResultSection { title: "Issues",
                                    match &*issues.read() {
                                        Some(Ok(list)) => rsx! {
                                            div { class: "border border-border rounded-lg divide-y divide-border",
                                                for issue in list.iter() {
                                                    CodeIssueRow { key: "{issue.event_id}", issue: issue.clone() }
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
                        if *active_filter.read() == SearchFilter::All
                            || *active_filter.read() == SearchFilter::PullRequests
                        {
                            if pr_count > 0 {
                                ResultSection { title: "Pull Requests",
                                    match &*prs.read() {
                                        Some(Ok(list)) => rsx! {
                                            div { class: "border border-border rounded-lg divide-y divide-border",
                                                for pr in list.iter() {
                                                    CodePullRow { key: "{pr.event_id}", pr: pr.clone() }
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
fn FilterChip(
    label: &'static str,
    count: usize,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if active {
        "px-3 py-1.5 text-sm rounded-full bg-primary text-primary-foreground flex items-center gap-2"
    } else {
        "px-3 py-1.5 text-sm rounded-full bg-muted text-muted-foreground hover:bg-accent transition flex items-center gap-2"
    };
    rsx! {
        button { class: "{class}", onclick: move |e| onclick.call(e),
            "{label}"
            span { class: if active { "px-1.5 py-0.5 text-xs rounded-full bg-primary-foreground/20" } else { "px-1.5 py-0.5 text-xs rounded-full bg-background" },
                "{count}"
            }
        }
    }
}
#[component]
fn ResultSection(title: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "space-y-3",
            h2 { class: "font-semibold text-lg", "{title}" }
            {children}
        }
    }
}
#[component]
fn EmptySearch() -> Element {
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
                    circle { cx: "11", cy: "11", r: "8" }
                    line {
                        x1: "21",
                        y1: "21",
                        x2: "16.65",
                        y2: "16.65",
                    }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "Search Code" }
            p { class: "text-muted-foreground text-sm",
                "Find repositories, code snippets, issues, and pull requests"
            }
        }
    }
}
#[component]
fn NoResults(query: String) -> Element {
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
                    circle { cx: "12", cy: "12", r: "10" }
                    line {
                        x1: "8",
                        y1: "12",
                        x2: "16",
                        y2: "12",
                    }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "No Results Found" }
            p { class: "text-muted-foreground text-sm",
                "No matches for \"{query}\". Try a different search term."
            }
        }
    }
}
#[component]
fn LoadingItems() -> Element {
    rsx! {
        div { class: "space-y-3 animate-pulse",
            for i in 0..3 {
                div { key: "{i}", class: "p-4 border border-border rounded-lg",
                    div { class: "h-4 bg-muted rounded w-2/3 mb-2" }
                    div { class: "h-3 bg-muted rounded w-1/2" }
                }
            }
        }
    }
}
