//! Code Home Page
//!
//! Landing page for the /code section featuring:
//! - Decentralized Git hosting discovery
//! - Recent repositories
//! - Code snippets
//! - Navigation to explore, import, etc.

use dioxus::prelude::*;
use crate::components::{
    CodeRepoCard, CodeSnippetCard,
    icons,
};
use crate::routes::Route;
use crate::services::git_hosting::{
    fetch_recent_repositories, fetch_recent_snippets,
};
use crate::stores::nostr_client;
use crate::utils::nip34::{Repository, DisplaySnippet};

/// Code home page component
#[component]
pub fn CodeHome() -> Element {
    let mut active_tab = use_signal(|| CodeTab::Repositories);
    let mut search_query = use_signal(|| String::new());

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "p-4 flex items-center justify-between",
                    h1 {
                        class: "text-xl font-bold flex items-center gap-2",
                        // Code icon
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

                    // Action buttons
                    div {
                        class: "flex items-center gap-2",
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
                                line { x1: "12", y1: "3", x2: "12", y2: "15" }
                            }
                            "Import"
                        }
                    }
                }

                // Search bar
                div {
                    class: "px-4 pb-4",
                    div {
                        class: "relative",
                        input {
                            class: "w-full px-4 py-2 pl-10 bg-muted rounded-full text-sm focus:outline-none focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Search repositories and snippets...",
                            value: "{search_query}",
                            oninput: move |e| search_query.set(e.value())
                        }
                        div {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
                            dangerous_inner_html: icons::SEARCH
                        }
                    }
                }

                // Tab navigation
                div {
                    class: "flex border-b border-border",
                    TabButton {
                        label: "Repositories",
                        active: *active_tab.read() == CodeTab::Repositories,
                        onclick: move |_| active_tab.set(CodeTab::Repositories)
                    }
                    TabButton {
                        label: "Snippets",
                        active: *active_tab.read() == CodeTab::Snippets,
                        onclick: move |_| active_tab.set(CodeTab::Snippets)
                    }
                    TabButton {
                        label: "My Repos",
                        active: *active_tab.read() == CodeTab::MyRepos,
                        onclick: move |_| active_tab.set(CodeTab::MyRepos)
                    }
                }
            }

            // Content
            div {
                class: "p-4",

                // Search results if query is not empty
                if !search_query.read().is_empty() {
                    SearchResults {
                        query: search_query.read().clone()
                    }
                } else {
                    // Tab content
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
                    }
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
        button {
            class: "{class}",
            onclick: move |e| props.onclick.call(e),
            "{props.label}"
        }
    }
}

/// Repositories tab - recent/featured repositories
#[component]
fn RepositoriesTab() -> Element {
    let mut repos = use_signal(|| None::<Result<Vec<Repository>, String>>);

    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        spawn(async move {
            let result = fetch_recent_repositories(20).await;
            repos.set(Some(result));
        });
    });

    rsx! {
        div {
            class: "space-y-6",

            // About section
            div {
                class: "bg-gradient-to-r from-blue-500/10 to-purple-500/10 rounded-lg p-4 border border-border",
                div {
                    class: "flex items-start gap-3",
                    div {
                        class: "w-10 h-10 rounded-lg bg-blue-500/20 flex items-center justify-center flex-shrink-0",
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
                            // Git branch icon
                            line { x1: "6", y1: "3", x2: "6", y2: "15" }
                            circle { cx: "18", cy: "6", r: "3" }
                            circle { cx: "6", cy: "18", r: "3" }
                            path { d: "M18 9a9 9 0 0 1-9 9" }
                        }
                    }
                    div {
                        h2 {
                            class: "font-semibold text-lg",
                            "Decentralized Git Hosting"
                        }
                        p {
                            class: "text-sm text-muted-foreground mt-1",
                            "Host your repositories on Nostr with NIP-34. Issues, pull requests, and collaboration without centralized servers."
                        }
                    }
                }
            }

            // Quick actions
            div {
                class: "grid grid-cols-2 gap-3",
                Link {
                    to: Route::CodeExplore {},
                    class: "p-4 border border-border rounded-lg hover:bg-accent/50 transition flex items-center gap-3",
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
                    to: Route::CodeSnippetNew {},
                    class: "p-4 border border-border rounded-lg hover:bg-accent/50 transition flex items-center gap-3",
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

            // Recent repositories
            div {
                h3 {
                    class: "font-semibold mb-3 flex items-center gap-2",
                    "Recent Repositories"
                    Link {
                        to: Route::CodeExplore {},
                        class: "text-sm text-primary hover:underline ml-auto",
                        "See all"
                    }
                }

                match &*repos.read() {
                    Some(Ok(repositories)) if !repositories.is_empty() => rsx! {
                        div {
                            class: "space-y-3",
                            for repo in repositories.iter().take(10) {
                                CodeRepoCard {
                                    key: "{repo.event_id}",
                                    repo: repo.clone()
                                }
                            }
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        EmptyState {
                            title: "No repositories yet",
                            description: "Be the first to import a repository!"
                        }
                    },
                    Some(Err(e)) => rsx! {
                        div {
                            class: "text-center py-8 text-muted-foreground",
                            "Failed to load repositories: {e}"
                        }
                    },
                    None => rsx! {
                        div {
                            class: "space-y-3",
                            for _ in 0..5 {
                                RepoCardSkeleton {}
                            }
                        }
                    },
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
        div {
            class: "space-y-6",

            // About NIP-C0
            div {
                class: "bg-gradient-to-r from-green-500/10 to-teal-500/10 rounded-lg p-4 border border-border",
                div {
                    class: "flex items-start gap-3",
                    div {
                        class: "w-10 h-10 rounded-lg bg-green-500/20 flex items-center justify-center flex-shrink-0",
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
                        h2 {
                            class: "font-semibold text-lg",
                            "Code Snippets (NIP-C0)"
                        }
                        p {
                            class: "text-sm text-muted-foreground mt-1",
                            "Share reusable code snippets on Nostr. Snippets are Kind 1337 events with language, description, and dependency metadata."
                        }
                    }
                }
            }

            // Create snippet button
            div {
                class: "flex justify-center",
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
                        line { x1: "12", y1: "5", x2: "12", y2: "19" }
                        line { x1: "5", y1: "12", x2: "19", y2: "12" }
                    }
                    "Create Snippet"
                }
            }

            // Recent snippets
            div {
                h3 {
                    class: "font-semibold mb-3",
                    "Recent Snippets"
                }

                match &*snippets.read() {
                    Some(Ok(snippet_list)) if !snippet_list.is_empty() => rsx! {
                        div {
                            class: "space-y-4",
                            for snippet in snippet_list.iter().take(10) {
                                CodeSnippetCard {
                                    key: "{snippet.event_id}",
                                    snippet: snippet.clone()
                                }
                            }
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        EmptyState {
                            title: "No snippets yet",
                            description: "Be the first to share a code snippet!"
                        }
                    },
                    Some(Err(e)) => rsx! {
                        div {
                            class: "text-center py-8 text-muted-foreground",
                            "Failed to load snippets: {e}"
                        }
                    },
                    None => rsx! {
                        div {
                            class: "space-y-4",
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

    let auth = auth_store::AUTH_STATE.read();

    if !auth.is_authenticated {
        return rsx! {
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
                        path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                        circle { cx: "12", cy: "7", r: "4" }
                    }
                }
                h3 {
                    class: "font-semibold text-lg mb-2",
                    "Sign in to view your repositories"
                }
                p {
                    class: "text-muted-foreground text-sm max-w-md mx-auto",
                    "Connect with your Nostr identity to see your repositories and manage your code."
                }
            }
        };
    }

    rsx! {
        div {
            class: "space-y-4",

            // Header with import button
            div {
                class: "flex items-center justify-between",
                h3 {
                    class: "font-semibold",
                    "Your Repositories"
                }
                Link {
                    to: Route::CodeImport {},
                    class: "px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition",
                    "Import Repository"
                }
            }

            // Placeholder for now - will fetch user repos
            EmptyState {
                title: "No repositories yet",
                description: "Import a repository from GitHub, GitLab, or Codeberg to get started."
            }
        }
    }
}

/// Search results component
#[component]
fn SearchResults(query: String) -> Element {
    rsx! {
        div {
            class: "space-y-4",
            h3 {
                class: "font-semibold",
                "Search results for \"{query}\""
            }
            p {
                class: "text-muted-foreground text-sm",
                "Search functionality coming soon..."
            }
        }
    }
}

/// Empty state component
#[component]
fn EmptyState(title: &'static str, description: &'static str) -> Element {
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
                    path { d: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20" }
                    path { d: "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" }
                }
            }
            h3 {
                class: "font-semibold text-lg mb-2",
                "{title}"
            }
            p {
                class: "text-muted-foreground text-sm max-w-md mx-auto",
                "{description}"
            }
        }
    }
}

/// Repository card skeleton for loading state
#[component]
fn RepoCardSkeleton() -> Element {
    rsx! {
        div {
            class: "p-4 border border-border rounded-lg animate-pulse",
            div {
                class: "flex items-start gap-3",
                div {
                    class: "w-10 h-10 rounded-lg bg-muted"
                }
                div {
                    class: "flex-1",
                    div { class: "h-4 bg-muted rounded w-1/3 mb-2" }
                    div { class: "h-3 bg-muted rounded w-1/4" }
                }
            }
            div { class: "h-3 bg-muted rounded w-2/3 mt-3" }
            div {
                class: "flex gap-4 mt-3",
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
        div {
            class: "border border-border rounded-lg overflow-hidden animate-pulse",
            div {
                class: "px-4 py-2 bg-muted/50 border-b border-border flex items-center justify-between",
                div { class: "h-4 bg-muted rounded w-24" }
                div { class: "h-4 bg-muted rounded w-12" }
            }
            div {
                class: "p-4 space-y-2",
                div { class: "h-3 bg-muted rounded w-full" }
                div { class: "h-3 bg-muted rounded w-5/6" }
                div { class: "h-3 bg-muted rounded w-4/6" }
            }
        }
    }
}
