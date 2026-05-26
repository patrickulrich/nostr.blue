use crate::components::{icons, CodeSnippetCard};
use crate::hooks::{use_nostr_resource_public, NostrResourceState};
use crate::routes::Route;
use crate::services::git_hosting::fetch_recent_snippets;
use dioxus::prelude::*;
/// Languages for filtering
const POPULAR_LANGUAGES: &[&str] = &[
    "rust",
    "javascript",
    "typescript",
    "python",
    "go",
    "java",
    "c",
    "cpp",
    "ruby",
    "php",
    "swift",
    "kotlin",
    "bash",
    "sql",
    "html",
    "css",
];
/// Code snippets browse page component
#[component]
pub fn CodeSnippets() -> Element {
    let mut language_filter = use_signal(|| None::<String>);
    let mut search_query = use_signal(String::new);
    let snippets = use_nostr_resource_public(move || async move { fetch_recent_snippets(50).await });
    let snippets_state = snippets.state();
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
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
                                polyline { points: "16 18 22 12 16 6" }
                                polyline { points: "8 6 2 12 8 18" }
                            }
                            "Code Snippets"
                        }
                    }
                    Link {
                        to: Route::CodeSnippetNew {},
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
                        "New"
                    }
                }
                div { class: "px-4 pb-3",
                    div { class: "relative",
                        input {
                            class: "w-full px-4 py-2 pl-10 bg-muted rounded-full text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Search snippets...",
                            value: "{search_query}",
                            oninput: move |e| search_query.set(e.value()),
                        }
                        div {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
                            dangerous_inner_html: icons::SEARCH,
                        }
                    }
                }
                div { class: "px-4 pb-3 flex gap-2 overflow-x-auto scrollbar-hide",
                    button {
                        class: if language_filter.read().is_none() { "px-3 py-1 text-xs rounded-full bg-primary text-primary-foreground whitespace-nowrap" } else { "px-3 py-1 text-xs rounded-full bg-muted text-muted-foreground hover:bg-accent whitespace-nowrap" },
                        onclick: move |_| language_filter.set(None),
                        "All"
                    }
                    for lang in POPULAR_LANGUAGES.iter() {
                        LanguageChip {
                            language: lang.to_string(),
                            active: language_filter.read().as_ref() == Some(&lang.to_string()),
                            onclick: move |_| language_filter.set(Some(lang.to_string())),
                        }
                    }
                }
            }
            div { class: "p-4",
                div { class: "mb-6 p-4 bg-green-500/10 rounded-lg border border-green-500/20",
                    div { class: "flex items-start gap-3",
                        div { class: "w-8 h-8 rounded-lg bg-green-500/20 flex items-center justify-center shrink-0",
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
                                path { d: "M12 16v-4" }
                                path { d: "M12 8h.01" }
                            }
                        }
                        div {
                            p { class: "text-sm",
                                span { class: "font-medium", "NIP-C0 Code Snippets" }
                                span { class: "text-muted-foreground",
                                    " - Kind 1337 events with syntax-highlighted code, metadata, and dependencies."
                                }
                            }
                        }
                    }
                }
                match &*snippets_state.read() {
                    NostrResourceState::Loaded(list) if !list.is_empty() => {
                        let search = search_query.read().to_lowercase();
                        let filtered: Vec<_> = list
                            .iter()
                            .filter(|s| {
                                if search.is_empty() {
                                    true
                                } else {
                                    s.code.to_lowercase().contains(&search)
                                        || s
                                            .name
                                            .as_ref()
                                            .is_some_and(|n| n.to_lowercase().contains(&search))
                                        || s
                                            .description
                                            .as_ref()
                                            .is_some_and(|d| d.to_lowercase().contains(&search))
                                }
                            })
                            .collect();
                        rsx! {
                            if filtered.is_empty() {
                                div { class: "text-center py-12",
                                    p { class: "text-muted-foreground", "No snippets match your search" }
                                }
                            } else {
                                div { class: "space-y-4",
                                    for snippet in filtered {
                                        CodeSnippetCard { key: "{snippet.event_id}", snippet: snippet.clone() }
                                    }
                                }
                            }
                        }
                    }
                    NostrResourceState::Loaded(_) => rsx! {
                        EmptyState {}
                    },
                    NostrResourceState::Error(e) => rsx! {
                        div { class: "text-center py-12 text-destructive", "Error loading snippets: {e}" }
                    },
                    _ => rsx! {
                        LoadingState {}
                    },
                }
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
struct LanguageChipProps {
    language: String,
    active: bool,
    onclick: EventHandler<MouseEvent>,
}
#[component]
fn LanguageChip(props: LanguageChipProps) -> Element {
    let class = if props.active {
        "px-3 py-1 text-xs rounded-full bg-primary text-primary-foreground whitespace-nowrap"
    } else {
        "px-3 py-1 text-xs rounded-full bg-muted text-muted-foreground hover:bg-accent whitespace-nowrap"
    };
    rsx! {
        button { class: "{class}", onclick: move |e| props.onclick.call(e), "{props.language}" }
    }
}
#[component]
fn EmptyState() -> Element {
    rsx! {
        div { class: "text-center py-16",
            div { class: "w-20 h-20 mx-auto mb-6 rounded-full bg-muted flex items-center justify-center",
                svg {
                    class: "w-10 h-10 text-muted-foreground",
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
            h3 { class: "font-semibold text-xl mb-2", "No Snippets Yet" }
            p { class: "text-muted-foreground max-w-md mx-auto mb-6",
                "Be the first to share a code snippet! Share your favorite algorithms, utilities, or code examples with the Nostr community."
            }
            Link {
                to: Route::CodeSnippetNew {},
                class: "inline-flex items-center gap-2 px-6 py-3 bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition font-medium",
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
                "Create Your First Snippet"
            }
        }
    }
}
#[component]
fn LoadingState() -> Element {
    rsx! {
        div { class: "space-y-4",
            for i in 0..5 {
                div {
                    key: "{i}",
                    class: "border border-border rounded-lg overflow-hidden animate-pulse",
                    div { class: "px-4 py-2 bg-muted/50 border-b border-border flex items-center justify-between",
                        div { class: "h-4 bg-muted rounded w-32" }
                        div { class: "h-4 bg-muted rounded w-16" }
                    }
                    div { class: "p-4 space-y-2",
                        div { class: "h-3 bg-muted rounded w-full" }
                        div { class: "h-3 bg-muted rounded w-5/6" }
                        div { class: "h-3 bg-muted rounded w-4/6" }
                        div { class: "h-3 bg-muted rounded w-3/4" }
                    }
                }
            }
        }
    }
}
