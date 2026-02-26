//! Code Explore Page
//!
//! Discover repositories and code snippets across Nostr.
use crate::components::{icons, CodeRepoCard, CodeSnippetCard};
use crate::routes::Route;
use crate::services::git_hosting::{fetch_recent_repositories, fetch_recent_snippets};
use crate::stores::nostr_client;
use crate::utils::nip34::{DisplaySnippet, Repository};
use dioxus::prelude::*;
/// Code explore page component
#[component]
pub fn CodeExplore() -> Element {
    let mut filter = use_signal(|| ExploreFilter::All);
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-3",
                    Link {
                        to: Route::CodeHome {},
                        class: "text-muted-foreground hover:text-foreground",
                        dangerous_inner_html: icons::ARROW_LEFT,
                    }
                    h1 { class: "text-xl font-bold", "Explore" }
                }
                div { class: "px-4 pb-2 flex gap-2 overflow-x-auto",
                    FilterChip {
                        label: "All",
                        active: *filter.read() == ExploreFilter::All,
                        onclick: move |_| filter.set(ExploreFilter::All),
                    }
                    FilterChip {
                        label: "Repositories",
                        active: *filter.read() == ExploreFilter::Repositories,
                        onclick: move |_| filter.set(ExploreFilter::Repositories),
                    }
                    FilterChip {
                        label: "Snippets",
                        active: *filter.read() == ExploreFilter::Snippets,
                        onclick: move |_| filter.set(ExploreFilter::Snippets),
                    }
                }
            }
            div { class: "p-4",
                match *filter.read() {
                    ExploreFilter::All => rsx! {
                        AllContent {}
                    },
                    ExploreFilter::Repositories => rsx! {
                        RepositoriesContent {}
                    },
                    ExploreFilter::Snippets => rsx! {
                        SnippetsContent {}
                    },
                }
            }
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum ExploreFilter {
    All,
    Repositories,
    Snippets,
}
#[derive(Props, Clone, PartialEq)]
struct FilterChipProps {
    label: &'static str,
    active: bool,
    onclick: EventHandler<MouseEvent>,
}
#[component]
fn FilterChip(props: FilterChipProps) -> Element {
    let class = if props.active {
        "px-3 py-1.5 text-sm rounded-full bg-primary text-primary-foreground"
    } else {
        "px-3 py-1.5 text-sm rounded-full bg-muted text-muted-foreground hover:bg-accent"
    };
    rsx! {
        button { class: "{class}", onclick: move |e| props.onclick.call(e), "{props.label}" }
    }
}
/// All content - shows both repos and snippets
#[component]
fn AllContent() -> Element {
    let mut repos = use_signal(|| None::<Result<Vec<Repository>, String>>);
    let mut snippets = use_signal(|| None::<Result<Vec<DisplaySnippet>, String>>);
    let mut gen = use_signal(|| 0u32);
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let current_gen = gen.peek().wrapping_add(1);
        gen.set(current_gen);
        spawn(async move {
            let (repos_res, snippets_res) = futures::join!(
                fetch_recent_repositories(10, None), fetch_recent_snippets(10)
            );
            if *gen.peek() != current_gen { return; }
            repos.set(Some(repos_res));
            snippets.set(Some(snippets_res));
        });
    });
    rsx! {
        div { class: "space-y-8",
            div { class: "bg-gradient-to-r from-purple-500/10 to-pink-500/10 rounded-lg p-4 border border-border",
                h2 { class: "font-semibold text-lg mb-2", "🔍 Discover Code on Nostr" }
                p { class: "text-sm text-muted-foreground",
                    "Browse decentralized repositories and code snippets published by developers worldwide."
                }
            }
            div {
                div { class: "flex items-center justify-between mb-3",
                    h3 { class: "font-semibold", "Recent Repositories" }
                    Link {
                        to: Route::CodeRepositories {},
                        class: "text-sm text-primary hover:underline",
                        "View all"
                    }
                }
                match &*repos.read() {
                    Some(Ok(list)) if !list.is_empty() => rsx! {
                        div { class: "space-y-3",
                            for repo in list.iter().take(5) {
                                CodeRepoCard { key: "{repo.event_id}", repo: repo.clone() }
                            }
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        EmptySection { message: "No repositories found" }
                    },
                    Some(Err(e)) => rsx! {
                        ErrorSection { message: "{e}" }
                    },
                    None => rsx! {
                        SkeletonCards { count: 3 }
                    },
                }
            }
            div {
                div { class: "flex items-center justify-between mb-3",
                    h3 { class: "font-semibold", "Recent Snippets" }
                    Link {
                        to: Route::CodeSnippets {},
                        class: "text-sm text-primary hover:underline",
                        "View all"
                    }
                }
                match &*snippets.read() {
                    Some(Ok(list)) if !list.is_empty() => rsx! {
                        div { class: "space-y-4",
                            for snippet in list.iter().take(5) {
                                CodeSnippetCard { key: "{snippet.event_id}", snippet: snippet.clone() }
                            }
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        EmptySection { message: "No snippets found" }
                    },
                    Some(Err(e)) => rsx! {
                        ErrorSection { message: "{e}" }
                    },
                    None => rsx! {
                        SkeletonCards { count: 3 }
                    },
                }
            }
        }
    }
}
/// Repositories only content
#[component]
fn RepositoriesContent() -> Element {
    let mut repos = use_signal(|| None::<Result<Vec<Repository>, String>>);
    let mut gen = use_signal(|| 0u32);
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let current_gen = gen.peek().wrapping_add(1);
        gen.set(current_gen);
        spawn(async move {
            let result = fetch_recent_repositories(50, None).await;
            if *gen.peek() != current_gen { return; }
            repos.set(Some(result));
        });
    });
    rsx! {
        div { class: "space-y-4",
            div { class: "flex items-center gap-2 text-sm text-muted-foreground mb-4",
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
                    circle { cx: "12", cy: "12", r: "10" }
                    path { d: "M12 16v-4" }
                    path { d: "M12 8h.01" }
                }
                span { "Repositories are published as NIP-34 events (Kind 30617)" }
            }
            match &*repos.read() {
                Some(Ok(list)) if !list.is_empty() => rsx! {
                    div { class: "space-y-3",
                        for repo in list.iter() {
                            CodeRepoCard { key: "{repo.event_id}", repo: repo.clone() }
                        }
                    }
                },
                Some(Ok(_)) => rsx! {
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
                        h3 { class: "font-semibold text-lg mb-2", "No repositories yet" }
                        p { class: "text-muted-foreground text-sm mb-4",
                            "Be the first to import a Git repository to Nostr!"
                        }
                        Link {
                            to: Route::CodeImport {},
                            class: "inline-flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition",
                            "Import Repository"
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    ErrorSection { message: "{e}" }
                },
                None => rsx! {
                    SkeletonCards { count: 10 }
                },
            }
        }
    }
}
/// Snippets only content
#[component]
fn SnippetsContent() -> Element {
    let mut snippets = use_signal(|| None::<Result<Vec<DisplaySnippet>, String>>);
    let mut gen = use_signal(|| 0u32);
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let current_gen = gen.peek().wrapping_add(1);
        gen.set(current_gen);
        spawn(async move {
            let result = fetch_recent_snippets(50).await;
            if *gen.peek() != current_gen { return; }
            snippets.set(Some(result));
        });
    });
    rsx! {
        div { class: "space-y-4",
            div { class: "flex items-center gap-2 text-sm text-muted-foreground mb-4",
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
                    circle { cx: "12", cy: "12", r: "10" }
                    path { d: "M12 16v-4" }
                    path { d: "M12 8h.01" }
                }
                span { "Code snippets are NIP-C0 events (Kind 1337)" }
            }
            div { class: "flex justify-center mb-4",
                Link {
                    to: Route::CodeSnippetNew {},
                    class: "inline-flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition",
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
            match &*snippets.read() {
                Some(Ok(list)) if !list.is_empty() => rsx! {
                    div { class: "space-y-4",
                        for snippet in list.iter() {
                            CodeSnippetCard { key: "{snippet.event_id}", snippet: snippet.clone() }
                        }
                    }
                },
                Some(Ok(_)) => rsx! {
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
                                polyline { points: "16 18 22 12 16 6" }
                                polyline { points: "8 6 2 12 8 18" }
                            }
                        }
                        h3 { class: "font-semibold text-lg mb-2", "No snippets yet" }
                        p { class: "text-muted-foreground text-sm", "Be the first to share a code snippet!" }
                    }
                },
                Some(Err(e)) => rsx! {
                    ErrorSection { message: "{e}" }
                },
                None => rsx! {
                    SkeletonCards { count: 10 }
                },
            }
        }
    }
}
#[component]
fn EmptySection(message: &'static str) -> Element {
    rsx! {
        div { class: "text-center py-8 text-muted-foreground", "{message}" }
    }
}
#[component]
fn ErrorSection(message: String) -> Element {
    rsx! {
        div { class: "text-center py-8 text-destructive", "Error: {message}" }
    }
}
#[component]
fn SkeletonCards(count: usize) -> Element {
    rsx! {
        div { class: "space-y-3",
            for i in 0..count {
                div {
                    key: "{i}",
                    class: "p-4 bg-card border border-border rounded-lg animate-pulse",
                    div { class: "flex items-start gap-3",
                        div { class: "w-10 h-10 rounded-lg bg-muted" }
                        div { class: "flex-1",
                            div { class: "h-4 bg-muted rounded w-1/3 mb-2" }
                            div { class: "h-3 bg-muted rounded w-1/4" }
                        }
                    }
                    div { class: "h-3 bg-muted rounded w-2/3 mt-3" }
                }
            }
        }
    }
}
