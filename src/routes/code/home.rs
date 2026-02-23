//! Code Home Page
//!
//! Landing page for the /code section featuring:
//! - Decentralized Git hosting discovery
//! - Recent repositories
//! - Code snippets
//! - Navigation to explore, import, etc.
use crate::components::{icons, CodeRepoCard, CodeSnippetCard};
use crate::hooks::use_infinite_scroll::use_infinite_scroll;
use crate::routes::Route;
use crate::services::git_hosting::activity::{
    fetch_platform_stats, fetch_recent_global_activity, fetch_top_developers,
    fetch_trending_repositories, Activity, PlatformStats, TopDeveloper,
};
use crate::services::git_hosting::{
    fetch_recent_repositories, fetch_recent_snippets, fetch_user_repositories,
};
use crate::stores::nostr_client;
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::format::format_relative_time_or;
use crate::utils::format::truncate_pubkey;
use crate::utils::nip34::{DisplaySnippet, Repository};
use dioxus::prelude::*;
use nostr_sdk::PublicKey;
use std::collections::HashSet;
/// Code home page component
#[component]
pub fn CodeHome() -> Element {
    let mut active_tab = use_signal(|| CodeTab::Repositories);
    let mut search_query = use_signal(String::new);
    let nav = use_navigator();
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center justify-between",
                    h1 { class: "text-xl font-bold flex items-center gap-2",
                        svg {
                            class: "w-6 h-6",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            polyline { points: "16 18 22 12 16 6" }
                            polyline { points: "8 6 2 12 8 18" }
                        }
                        "Code"
                    }
                    div { class: "flex items-center gap-2",
                        Link {
                            to: Route::CodeNotifications {},
                            class: "p-2 text-muted-foreground hover:text-foreground transition",
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
                                path { d: "M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" }
                                path { d: "M13.73 21a2 2 0 0 1-3.46 0" }
                            }
                        }
                        Link {
                            to: Route::CodeImport {},
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
                                path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                                polyline { points: "17 8 12 3 7 8" }
                                line {
                                    x1: "12",
                                    y1: "3",
                                    x2: "12",
                                    y2: "15",
                                }
                            }
                            "Import"
                        }
                    }
                }
                div { class: "px-4 pb-4",
                    div { class: "relative",
                        input {
                            class: "w-full px-4 py-2 pl-10 bg-muted rounded-full text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Search repositories and snippets...",
                            value: "{search_query}",
                            oninput: move |e| search_query.set(e.value()),
                            onkeypress: move |e: KeyboardEvent| {
                                if e.key() == Key::Enter {
                                    let q = search_query.read().clone();
                                    if !q.is_empty() {
                                        nav.push(Route::CodeSearch { q });
                                    }
                                }
                            },
                        }
                        div {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
                            dangerous_inner_html: icons::SEARCH,
                        }
                    }
                }
                div { class: "flex border-b border-border",
                    TabButton {
                        label: "Repositories",
                        active: *active_tab.read() == CodeTab::Repositories,
                        onclick: move |_| active_tab.set(CodeTab::Repositories),
                    }
                    TabButton {
                        label: "Snippets",
                        active: *active_tab.read() == CodeTab::Snippets,
                        onclick: move |_| active_tab.set(CodeTab::Snippets),
                    }
                    TabButton {
                        label: "My Repos",
                        active: *active_tab.read() == CodeTab::MyRepos,
                        onclick: move |_| active_tab.set(CodeTab::MyRepos),
                    }
                    TabButton {
                        label: "Discover",
                        active: *active_tab.read() == CodeTab::Discover,
                        onclick: move |_| active_tab.set(CodeTab::Discover),
                    }
                }
            }
            div { class: "p-4",
                match *active_tab.read() {
                    CodeTab::Repositories => rsx! {
                        RepositoriesTab {}
                    },
                    CodeTab::Snippets => rsx! {
                        SnippetsTab {}
                    },
                    CodeTab::MyRepos => rsx! {
                        MyReposTab {}
                    },
                    CodeTab::Discover => rsx! {
                        DiscoverTab {}
                    },
                }
            }
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum CodeTab {
    Repositories,
    Snippets,
    MyRepos,
    Discover,
}
#[derive(Props, Clone, PartialEq)]
struct TabButtonProps {
    label: &'static str,
    active: bool,
    onclick: EventHandler<MouseEvent>,
}
#[component]
fn TabButton(props: TabButtonProps) -> Element {
    let class = if props.active {
        "flex-1 py-3 text-sm font-medium text-primary border-b-2 border-primary"
    } else {
        "flex-1 py-3 text-sm font-medium text-muted-foreground hover:text-foreground border-b-2 border-transparent"
    };
    rsx! {
        button { class: "{class}", onclick: move |e| props.onclick.call(e), "{props.label}" }
    }
}
const PAGE_SIZE: usize = 20;
/// Repositories tab - recent/featured repositories with infinite scroll
#[component]
fn RepositoriesTab() -> Element {
    let mut repos = use_signal(Vec::<Repository>::new);
    let mut loading = use_signal(|| true);
    let mut pagination_loading = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut seen_ids = use_signal(HashSet::<String>::new);
    let mut fetch_error = use_signal(|| None::<String>);
    // Initial fetch
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        fetch_error.set(None);
        spawn(async move {
            match fetch_recent_repositories(PAGE_SIZE, None).await {
                Ok(fetched) => {
                    let raw_count = fetched.len();
                    if let Some(oldest) = fetched.iter().min_by_key(|r| r.created_at) {
                        oldest_timestamp.set(Some(oldest.created_at));
                    }
                    has_more.set(raw_count >= PAGE_SIZE);
                    seen_ids.set(fetched.iter().map(|r| r.event_id.clone()).collect());
                    repos.set(fetched);
                }
                Err(e) => {
                    log::error!("Failed to fetch repos: {}", e);
                    fetch_error.set(Some(format!("Failed to fetch repositories: {}", e)));
                }
            }
            loading.set(false);
        });
    });
    // Load more callback for infinite scroll
    let load_more = move || {
        if *pagination_loading.peek() || !*has_more.peek() {
            return;
        }
        // Subtract 1 to make until exclusive (avoid refetching the boundary item)
        let until = (*oldest_timestamp.peek()).map(|ts| ts.saturating_sub(1));
        pagination_loading.set(true);
        spawn(async move {
            match fetch_recent_repositories(PAGE_SIZE, until).await {
                Ok(fetched) => {
                    if fetched.is_empty() {
                        has_more.set(false);
                    } else {
                        let raw_count = fetched.len();
                        if let Some(oldest) = fetched.iter().min_by_key(|r| r.created_at) {
                            oldest_timestamp.set(Some(oldest.created_at));
                        }
                        repos.with_mut(|current| {
                            seen_ids.with_mut(|existing| {
                                for repo in fetched {
                                    if existing.insert(repo.event_id.clone()) {
                                        current.push(repo);
                                    }
                                }
                            });
                        });
                        has_more.set(raw_count >= PAGE_SIZE);
                    }
                }
                Err(e) => log::error!("Failed to load more repos: {}", e),
            }
            pagination_loading.set(false);
        });
    };
    let sentinel_id = use_infinite_scroll(load_more, has_more, pagination_loading);
    rsx! {
        div { class: "space-y-6",
            div { class: "bg-gradient-to-r from-blue-500/10 to-purple-500/10 rounded-lg p-4 border border-border",
                div { class: "flex items-start gap-3",
                    div { class: "w-10 h-10 rounded-lg bg-blue-500/20 flex items-center justify-center shrink-0",
                        svg {
                            class: "w-5 h-5 text-blue-500",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            line {
                                x1: "6",
                                y1: "3",
                                x2: "6",
                                y2: "15",
                            }
                            circle { cx: "18", cy: "6", r: "3" }
                            circle { cx: "6", cy: "18", r: "3" }
                            path { d: "M18 9a9 9 0 0 1-9 9" }
                        }
                    }
                    div {
                        h2 { class: "font-semibold text-lg", "Decentralized Git Hosting" }
                        p { class: "text-sm text-muted-foreground mt-1",
                            "Host your repositories on Nostr with NIP-34. Issues, pull requests, and collaboration without centralized servers."
                        }
                    }
                }
            }
            div { class: "grid grid-cols-2 gap-3",
                Link {
                    to: Route::CodeExplore {},
                    class: "p-4 bg-card border border-border rounded-lg hover:bg-accent/50 transition flex items-center gap-3",
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
                        polygon { points: "16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76" }
                    }
                    div {
                        div { class: "font-medium", "Explore" }
                        div { class: "text-xs text-muted-foreground", "Discover repos" }
                    }
                }
                Link {
                    to: Route::CodeNew {},
                    class: "p-4 bg-card border border-border rounded-lg hover:bg-accent/50 transition flex items-center gap-3",
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
                        line { x1: "12", y1: "5", x2: "12", y2: "19" }
                        line { x1: "5", y1: "12", x2: "19", y2: "12" }
                    }
                    div {
                        div { class: "font-medium", "New Repo" }
                        div { class: "text-xs text-muted-foreground", "Create repository" }
                    }
                }
                Link {
                    to: Route::CodeGlobalIssues {},
                    class: "p-4 bg-card border border-border rounded-lg hover:bg-accent/50 transition flex items-center gap-3",
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
                        circle { cx: "12", cy: "12", r: "1" }
                    }
                    div {
                        div { class: "font-medium", "All Issues" }
                        div { class: "text-xs text-muted-foreground", "Browse issues" }
                    }
                }
                Link {
                    to: Route::CodeGlobalPulls {},
                    class: "p-4 bg-card border border-border rounded-lg hover:bg-accent/50 transition flex items-center gap-3",
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
                        circle { cx: "18", cy: "18", r: "3" }
                        circle { cx: "6", cy: "6", r: "3" }
                        path { d: "M13 6h3a2 2 0 0 1 2 2v7" }
                        line { x1: "6", y1: "9", x2: "6", y2: "21" }
                    }
                    div {
                        div { class: "font-medium", "All Pull Requests" }
                        div { class: "text-xs text-muted-foreground", "Browse PRs" }
                    }
                }
                Link {
                    to: Route::CodeStars {},
                    class: "p-4 bg-card border border-border rounded-lg hover:bg-accent/50 transition flex items-center gap-3",
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
                        polygon { points: "12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" }
                    }
                    div {
                        div { class: "font-medium", "Stars" }
                        div { class: "text-xs text-muted-foreground", "Starred repos" }
                    }
                }
                Link {
                    to: Route::CodeSnippetNew {},
                    class: "p-4 bg-card border border-border rounded-lg hover:bg-accent/50 transition flex items-center gap-3",
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
                        polyline { points: "16 18 22 12 16 6" }
                        polyline { points: "8 6 2 12 8 18" }
                    }
                    div {
                        div { class: "font-medium", "New Snippet" }
                        div { class: "text-xs text-muted-foreground", "Share code" }
                    }
                }
            }
            if let Some(ref err) = *fetch_error.read() {
                div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg flex items-center justify-between",
                    p { class: "text-sm text-destructive", "{err}" }
                    button {
                        class: "px-3 py-1 text-sm rounded-lg bg-destructive text-destructive-foreground hover:opacity-90 transition",
                        onclick: move |_| {
                            fetch_error.set(None);
                            loading.set(true);
                            spawn(async move {
                                match fetch_recent_repositories(PAGE_SIZE, None).await {
                                    Ok(fetched) => {
                                        let raw_count = fetched.len();
                                        if let Some(oldest) = fetched.iter().min_by_key(|r| r.created_at) {
                                            oldest_timestamp.set(Some(oldest.created_at));
                                        }
                                        has_more.set(raw_count >= PAGE_SIZE);
                                        seen_ids.set(fetched.iter().map(|r| r.event_id.clone()).collect());
                                        repos.set(fetched);
                                    }
                                    Err(e) => {
                                        log::error!("Failed to fetch repos: {}", e);
                                        fetch_error.set(Some(format!("Failed to fetch repositories: {}", e)));
                                    }
                                }
                                loading.set(false);
                            });
                        },
                        "Retry"
                    }
                }
            }
            div {
                h3 { class: "font-semibold mb-3 flex items-center gap-2",
                    "Recent Repositories"
                    Link {
                        to: Route::CodeExplore {},
                        class: "text-sm text-primary hover:underline ml-auto",
                        "See all"
                    }
                }
                if *loading.read() {
                    div { class: "space-y-3",
                        for _ in 0..5 {
                            RepoCardSkeleton {}
                        }
                    }
                } else if repos.read().is_empty() {
                    EmptyState {
                        title: "No repositories yet",
                        description: "Be the first to import a repository!",
                    }
                } else {
                    div { class: "space-y-3",
                        for repo in repos.read().iter() {
                            CodeRepoCard { key: "{repo.event_id}", repo: repo.clone() }
                        }
                    }
                    if *has_more.read() {
                        div { id: "{sentinel_id}", class: "h-4" }
                    }
                    if *pagination_loading.read() {
                        div { class: "space-y-3 mt-3",
                            for _ in 0..3 {
                                RepoCardSkeleton {}
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Snippets tab - recent code snippets
#[component]
fn SnippetsTab() -> Element {
    let mut snippets = use_signal(|| None::<Result<Vec<DisplaySnippet>, String>>);
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            let result = fetch_recent_snippets(20).await;
            snippets.set(Some(result));
        });
    });
    rsx! {
        div { class: "space-y-6",
            div { class: "bg-gradient-to-r from-green-500/10 to-teal-500/10 rounded-lg p-4 border border-border",
                div { class: "flex items-start gap-3",
                    div { class: "w-10 h-10 rounded-lg bg-green-500/20 flex items-center justify-center shrink-0",
                        svg {
                            class: "w-5 h-5 text-green-500",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            polyline { points: "16 18 22 12 16 6" }
                            polyline { points: "8 6 2 12 8 18" }
                        }
                    }
                    div {
                        h2 { class: "font-semibold text-lg", "Code Snippets (NIP-C0)" }
                        p { class: "text-sm text-muted-foreground mt-1",
                            "Share reusable code snippets on Nostr. Snippets are Kind 1337 events with language, description, and dependency metadata."
                        }
                    }
                }
            }
            div { class: "flex justify-center",
                Link {
                    to: Route::CodeSnippetNew {},
                    class: "px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition flex items-center gap-2",
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
                        line {
                            x1: "12",
                            y1: "5",
                            x2: "12",
                            y2: "19",
                        }
                        line {
                            x1: "5",
                            y1: "12",
                            x2: "19",
                            y2: "12",
                        }
                    }
                    "Create Snippet"
                }
            }
            div {
                h3 { class: "font-semibold mb-3", "Recent Snippets" }
                match &*snippets.read() {
                    Some(Ok(snippet_list)) if !snippet_list.is_empty() => rsx! {
                        div { class: "space-y-4",
                            for snippet in snippet_list.iter().take(10) {
                                CodeSnippetCard { key: "{snippet.event_id}", snippet: snippet.clone() }
                            }
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        EmptyState {
                            title: "No snippets yet",
                            description: "Be the first to share a code snippet!",
                        }
                    },
                    Some(Err(e)) => rsx! {
                        div { class: "text-center py-8 text-muted-foreground", "Failed to load snippets: {e}" }
                    },
                    None => rsx! {
                        div { class: "space-y-4",
                            for _ in 0..3 {
                                SnippetCardSkeleton {}
                            }
                        }
                    },
                }
            }
        }
    }
}
/// My repos tab - user's repositories
#[component]
fn MyReposTab() -> Element {
    use crate::stores::auth_store;
    let mut my_repos = use_signal(|| None::<Result<Vec<Repository>, String>>);
    let mut loading = use_signal(|| true);
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        // Read auth inside effect so it re-runs on account switch
        let auth = auth_store::AUTH_STATE.read();
        let pubkey_hex = auth.pubkey.clone().unwrap_or_default();
        if pubkey_hex.is_empty() {
            my_repos.set(None);
            loading.set(false);
            return;
        }
        let pubkey = match PublicKey::parse(&pubkey_hex) {
            Ok(pk) => pk,
            Err(e) => {
                my_repos.set(Some(Err(format!("Failed to parse public key: {}", e))));
                loading.set(false);
                return;
            }
        };
        my_repos.set(None);
        loading.set(true);
        spawn(async move {
            match fetch_user_repositories(&pubkey, 50).await {
                Ok(repos) => {
                    if auth_store::get_pubkey().as_deref() != Some(pubkey_hex.as_str()) {
                        return;
                    }
                    my_repos.set(Some(Ok(repos)));
                }
                Err(e) => {
                    if auth_store::get_pubkey().as_deref() != Some(pubkey_hex.as_str()) {
                        return;
                    }
                    my_repos.set(Some(Err(e)));
                }
            }
            loading.set(false);
        });
    });
    let auth = auth_store::AUTH_STATE.read();
    if !auth.is_authenticated {
        return rsx! {
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
                        path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                        circle { cx: "12", cy: "7", r: "4" }
                    }
                }
                h3 { class: "font-semibold text-lg mb-2", "Sign in to view your repositories" }
                p { class: "text-muted-foreground text-sm max-w-md mx-auto",
                    "Connect with your Nostr identity to see your repositories and manage your code."
                }
            }
        };
    }
    rsx! {
        div { class: "space-y-4",
            div { class: "flex items-center justify-between",
                h3 { class: "font-semibold", "Your Repositories" }
                Link {
                    to: Route::CodeImport {},
                    class: "px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition",
                    "Import Repository"
                }
            }
            if *loading.read() {
                div { class: "space-y-3",
                    for _ in 0..3 {
                        RepoCardSkeleton {}
                    }
                }
            } else {
                match &*my_repos.read() {
                    Some(Ok(repos)) if !repos.is_empty() => rsx! {
                        div { class: "space-y-3",
                            for repo in repos.iter() {
                                CodeRepoCard { key: "{repo.event_id}", repo: repo.clone() }
                            }
                        }
                    },
                    Some(Err(e)) => rsx! {
                        div { class: "text-center py-8 text-muted-foreground", "Failed to load repositories: {e}" }
                    },
                    _ => rsx! {
                        EmptyState {
                            title: "No repositories yet",
                            description: "Import a repository from GitHub, GitLab, or Codeberg to get started.",
                        }
                    },
                }
            }
        }
    }
}
/// Discover tab - trending repos, top developers, global activity, platform stats
#[component]
fn DiscoverTab() -> Element {
    let mut trending_repos = use_signal(|| None::<Result<Vec<Repository>, String>>);
    let mut top_devs = use_signal(|| None::<Result<Vec<TopDeveloper>, String>>);
    let mut global_activity = use_signal(|| None::<Result<Vec<Activity>, String>>);
    let mut stats = use_signal(|| None::<Result<PlatformStats, String>>);

    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            let (trending_res, devs_res, activity_res, stats_res) = futures::join!(
                fetch_trending_repositories(10),
                fetch_top_developers(5),
                fetch_recent_global_activity(20),
                fetch_platform_stats(),
            );
            trending_repos.set(Some(trending_res));
            top_devs.set(Some(devs_res));
            global_activity.set(Some(activity_res));
            stats.set(Some(stats_res));
        });
    });

    rsx! {
        div { class: "space-y-6",
            // Section 1: Platform Stats
            div {
                h3 { class: "font-semibold mb-3 flex items-center gap-2",
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
                        path { d: "M21 12c0 1.66-4 3-9 3s-9-1.34-9-3" }
                        path { d: "M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5" }
                        path { d: "M21 5c0 1.66-4 3-9 3S3 6.66 3 5s4-3 9-3 9 1.34 9 3" }
                    }
                    "Platform Stats"
                }
                match &*stats.read() {
                    Some(Ok(s)) => rsx! {
                        div { class: "grid grid-cols-2 gap-3",
                            StatCard { label: "Repositories", value: s.total_repos, icon_type: StatIcon::Repo }
                            StatCard { label: "Issues", value: s.total_issues, icon_type: StatIcon::Issue }
                            StatCard { label: "Pull Requests", value: s.total_prs, icon_type: StatIcon::PullRequest }
                            StatCard { label: "Snippets", value: s.total_snippets, icon_type: StatIcon::Snippet }
                        }
                    },
                    Some(Err(e)) => rsx! {
                        div { class: "text-center py-4 text-muted-foreground text-sm", "Failed to load stats: {e}" }
                    },
                    None => rsx! {
                        div { class: "grid grid-cols-2 gap-3",
                            for _ in 0..4 {
                                div { class: "bg-card p-3 border border-border rounded-lg animate-pulse",
                                    div { class: "h-4 bg-muted rounded w-1/2 mb-2" }
                                    div { class: "h-6 bg-muted rounded w-1/3" }
                                }
                            }
                        }
                    },
                }
            }

            // Section 2: Trending Repositories
            div {
                h3 { class: "font-semibold mb-3 flex items-center gap-2",
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
                        polyline { points: "23 6 13.5 15.5 8.5 10.5 1 18" }
                        polyline { points: "17 6 23 6 23 12" }
                    }
                    "Trending Repositories"
                    span { class: "text-xs text-muted-foreground font-normal", "(last 7 days)" }
                }
                match &*trending_repos.read() {
                    Some(Ok(repos)) if !repos.is_empty() => rsx! {
                        div { class: "space-y-3",
                            for repo in repos.iter() {
                                CodeRepoCard { key: "{repo.event_id}", repo: repo.clone() }
                            }
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        EmptyState {
                            title: "No trending repos yet",
                            description: "Repositories will appear here as they gain stars.",
                        }
                    },
                    Some(Err(e)) => rsx! {
                        div { class: "text-center py-4 text-muted-foreground text-sm", "Failed to load trending repos: {e}" }
                    },
                    None => rsx! {
                        div { class: "space-y-3",
                            for _ in 0..3 {
                                RepoCardSkeleton {}
                            }
                        }
                    },
                }
            }

            // Section 3: Top Developers
            div {
                h3 { class: "font-semibold mb-3 flex items-center gap-2",
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
                    "Top Developers"
                    span { class: "text-xs text-muted-foreground font-normal", "(last 30 days)" }
                }
                match &*top_devs.read() {
                    Some(Ok(devs)) if !devs.is_empty() => rsx! {
                        div { class: "space-y-2",
                            for (i, dev) in devs.iter().enumerate() {
                                DeveloperCard { key: "{dev.pubkey}", rank: i + 1, developer: dev.clone() }
                            }
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        EmptyState {
                            title: "No developer activity yet",
                            description: "Developers will appear here as they contribute.",
                        }
                    },
                    Some(Err(e)) => rsx! {
                        div { class: "text-center py-4 text-muted-foreground text-sm", "Failed to load developers: {e}" }
                    },
                    None => rsx! {
                        div { class: "space-y-2",
                            for _ in 0..3 {
                                div { class: "bg-card p-4 border border-border rounded-lg animate-pulse flex items-center gap-3",
                                    div { class: "w-10 h-10 rounded-full bg-muted" }
                                    div { class: "flex-1",
                                        div { class: "h-4 bg-muted rounded w-1/3 mb-1" }
                                        div { class: "h-3 bg-muted rounded w-1/4" }
                                    }
                                }
                            }
                        }
                    },
                }
            }

            // Section 4: Recent Global Activity
            div {
                h3 { class: "font-semibold mb-3 flex items-center gap-2",
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
                match &*global_activity.read() {
                    Some(Ok(activities)) if !activities.is_empty() => rsx! {
                        div { class: "space-y-1",
                            for activity in activities.iter() {
                                ActivityRow { key: "{activity.event_id}", activity: activity.clone() }
                            }
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        EmptyState {
                            title: "No activity yet",
                            description: "Code activity from all developers will appear here.",
                        }
                    },
                    Some(Err(e)) => rsx! {
                        div { class: "text-center py-4 text-muted-foreground text-sm", "Failed to load activity: {e}" }
                    },
                    None => rsx! {
                        div { class: "space-y-2",
                            for _ in 0..5 {
                                div { class: "p-3 animate-pulse flex items-center gap-3",
                                    div { class: "w-5 h-5 rounded bg-muted" }
                                    div { class: "flex-1",
                                        div { class: "h-3 bg-muted rounded w-2/3" }
                                    }
                                    div { class: "h-3 bg-muted rounded w-16" }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

/// Icon type for stat cards
#[derive(Clone, Copy, PartialEq)]
enum StatIcon {
    Repo,
    Issue,
    PullRequest,
    Snippet,
}

/// Stat card component for platform stats grid
#[component]
fn StatCard(label: &'static str, value: usize, icon_type: StatIcon) -> Element {
    rsx! {
        div { class: "p-3 border border-border rounded-lg bg-card",
            div { class: "flex items-center gap-2 mb-2",
                match icon_type {
                    StatIcon::Repo => rsx! {
                        svg {
                            class: "w-4 h-4 text-blue-500",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            line { x1: "6", y1: "3", x2: "6", y2: "15" }
                            circle { cx: "18", cy: "6", r: "3" }
                            circle { cx: "6", cy: "18", r: "3" }
                            path { d: "M18 9a9 9 0 0 1-9 9" }
                        }
                    },
                    StatIcon::Issue => rsx! {
                        svg {
                            class: "w-4 h-4 text-green-500",
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
                    },
                    StatIcon::PullRequest => rsx! {
                        svg {
                            class: "w-4 h-4 text-purple-500",
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
                    },
                    StatIcon::Snippet => rsx! {
                        svg {
                            class: "w-4 h-4 text-orange-500",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            polyline { points: "16 18 22 12 16 6" }
                            polyline { points: "8 6 2 12 8 18" }
                        }
                    },
                }
                span { class: "text-xs text-muted-foreground", "{label}" }
            }
            div { class: "text-2xl font-bold", "{value}" }
        }
    }
}

/// Developer card for top developers section
#[component]
fn DeveloperCard(rank: usize, developer: TopDeveloper) -> Element {
    let pubkey = developer.pubkey.clone();
    let (display_name, avatar_url) = {
        let cache = PROFILE_CACHE.read();
        let profile = cache.peek(&pubkey);
        let name = profile
            .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
            .unwrap_or_else(|| truncate_pubkey(&pubkey));
        let avatar = profile
            .and_then(|p| p.picture.clone())
            .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/identicon/svg?seed={}", pubkey));
        (name, avatar)
    };

    let suffix = if developer.contribution_count == 1 { "" } else { "s" };

    rsx! {
        Link {
            to: Route::CodeUserProfile { pubkey: pubkey.clone() },
            class: "p-4 bg-card border border-border rounded-lg hover:bg-accent/50 transition flex items-center gap-3",
            div { class: "text-sm font-bold text-muted-foreground w-6 text-center", "#{rank}" }
            img {
                class: "w-10 h-10 rounded-full bg-muted",
                src: "{avatar_url}",
                alt: "Developer avatar",
            }
            div { class: "flex-1 min-w-0",
                div { class: "font-medium truncate", "{display_name}" }
                div { class: "text-xs text-muted-foreground",
                    "{developer.contribution_count} contribution{suffix}"
                }
            }
        }
    }
}

/// Activity row for global activity feed
#[component]
fn ActivityRow(activity: Activity) -> Element {
    let time_str = format_relative_time_or(activity.created_at, "Unknown");
    let author_name = {
        let cache = PROFILE_CACHE.read();
        cache
            .peek(&activity.author)
            .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
            .unwrap_or_else(|| truncate_pubkey(&activity.author))
    };

    rsx! {
        div { class: "p-2 rounded-lg hover:bg-accent/30 transition flex items-center gap-3",
            // Activity type icon
            div { class: "shrink-0",
                match activity.activity_type {
                    crate::services::git_hosting::activity::ActivityType::RepoCreated => rsx! {
                        svg {
                            class: "w-4 h-4 text-blue-500",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            line { x1: "6", y1: "3", x2: "6", y2: "15" }
                            circle { cx: "18", cy: "6", r: "3" }
                            circle { cx: "6", cy: "18", r: "3" }
                            path { d: "M18 9a9 9 0 0 1-9 9" }
                        }
                    },
                    crate::services::git_hosting::activity::ActivityType::IssueOpened => rsx! {
                        svg {
                            class: "w-4 h-4 text-green-500",
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
                    },
                    crate::services::git_hosting::activity::ActivityType::PullRequestOpened => rsx! {
                        svg {
                            class: "w-4 h-4 text-purple-500",
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
                    },
                    crate::services::git_hosting::activity::ActivityType::SnippetCreated => rsx! {
                        svg {
                            class: "w-4 h-4 text-orange-500",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            polyline { points: "16 18 22 12 16 6" }
                            polyline { points: "8 6 2 12 8 18" }
                        }
                    },
                    _ => rsx! {
                        svg {
                            class: "w-4 h-4 text-muted-foreground",
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
                    },
                }
            }
            // Activity text
            div { class: "flex-1 min-w-0",
                div { class: "text-sm flex items-baseline gap-1 min-w-0",
                    span { class: "font-medium shrink-0", "{author_name}" }
                    span { class: "text-muted-foreground shrink-0", "{activity.activity_type}" }
                    span { class: "font-medium truncate min-w-0", "{activity.title}" }
                }
            }
            // Timestamp
            span { class: "text-xs text-muted-foreground whitespace-nowrap shrink-0", "{time_str}" }
        }
    }
}

/// Empty state component
#[component]
fn EmptyState(title: &'static str, description: &'static str) -> Element {
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
                    path { d: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20" }
                    path { d: "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "{title}" }
            p { class: "text-muted-foreground text-sm max-w-md mx-auto", "{description}" }
        }
    }
}
/// Repository card skeleton for loading state
#[component]
fn RepoCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card p-4 border border-border rounded-lg animate-pulse",
            div { class: "flex items-start gap-3",
                div { class: "w-10 h-10 rounded-lg bg-muted" }
                div { class: "flex-1",
                    div { class: "h-4 bg-muted rounded w-1/3 mb-2" }
                    div { class: "h-3 bg-muted rounded w-1/4" }
                }
            }
            div { class: "h-3 bg-muted rounded w-2/3 mt-3" }
            div { class: "flex gap-4 mt-3",
                div { class: "h-3 bg-muted rounded w-12" }
                div { class: "h-3 bg-muted rounded w-12" }
            }
        }
    }
}
/// Snippet card skeleton for loading state
#[component]
fn SnippetCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card border border-border rounded-lg overflow-hidden animate-pulse",
            div { class: "px-4 py-2 bg-muted/50 border-b border-border flex items-center justify-between",
                div { class: "h-4 bg-muted rounded w-24" }
                div { class: "h-4 bg-muted rounded w-12" }
            }
            div { class: "p-4 space-y-2",
                div { class: "h-3 bg-muted rounded w-full" }
                div { class: "h-3 bg-muted rounded w-5/6" }
                div { class: "h-3 bg-muted rounded w-4/6" }
            }
        }
    }
}
