//! Wiki Backlinks Component
//! Shows pages that link to the current wiki page (NIP-54)
use crate::components::icons::{ArrowLeftIcon, Link2Icon};
use crate::routes::Route;
use crate::stores::wiki_store::{fetch_wiki_backlinks, CachedWikiPage};
use dioxus::prelude::*;
/// Backlinks panel for wiki pages
#[component]
pub fn WikiBacklinks(
    /// The wiki page identifier to find backlinks for
    identifier: String,
    /// Pre-loaded backlinks (optional)
    #[props(default = None)]
    backlinks: Option<Vec<CachedWikiPage>>,
    /// Custom CSS classes
    #[props(default = String::new())]
    class: String,
) -> Element {
    let mut loading = use_signal(|| backlinks.is_none());
    let mut pages = use_signal(|| backlinks.unwrap_or_default());
    let mut error = use_signal(|| None::<String>);
    use_effect(move || {
        if pages.read().is_empty() && *loading.read() {
            let id = identifier.clone();
            spawn(async move {
                match fetch_wiki_backlinks(&id, 50).await {
                    Ok(result) => {
                        pages.set(result);
                        loading.set(false);
                    }
                    Err(e) => {
                        error.set(Some(e));
                        loading.set(false);
                    }
                }
            });
        }
    });
    rsx! {
        div { class: "wiki-backlinks {class}",
            div { class: "flex items-center justify-between mb-3",
                h4 { class: "text-sm font-semibold text-foreground flex items-center gap-2",
                    ArrowLeftIcon { class: "w-4 h-4" }
                    "Backlinks"
                }
                if !pages.read().is_empty() {
                    span { class: "text-xs text-muted-foreground", "{pages.read().len()} pages" }
                }
            }
            if *loading.read() {
                BacklinksSkeleton {}
            } else if let Some(ref e) = *error.read() {
                div { class: "text-sm text-destructive", "Failed to load backlinks: {e}" }
            } else if pages.read().is_empty() {
                div { class: "text-sm text-muted-foreground py-4 text-center",
                    "No pages link to this page yet."
                }
            } else {
                ul { class: "space-y-2",
                    for page in pages.read().iter() {
                        BacklinkItem {
                            key: "{page.event.id.to_hex()}",
                            page: page.clone(),
                        }
                    }
                }
            }
        }
    }
}
/// Single backlink item
#[component]
fn BacklinkItem(page: CachedWikiPage) -> Element {
    let nav = use_navigator();
    let identifier = page.article.identifier.clone();
    let item_click = move |_| {
        nav.push(Route::WikiDetail {
            identifier: identifier.clone(),
        });
    };
    rsx! {
        li { class: "group",
            button {
                class: "w-full text-left p-2 rounded-md hover:bg-accent transition-colors",
                onclick: item_click,
                div { class: "flex items-start gap-2",
                    div { class: "shrink-0 mt-0.5",
                        Link2Icon { class: "w-4 h-4 text-muted-foreground group-hover:text-primary transition-colors" }
                    }
                    div { class: "flex-1 min-w-0",
                        h5 { class: "text-sm font-medium text-foreground group-hover:text-primary transition-colors truncate",
                            "{page.article.title}"
                        }
                        if let Some(ref summary) = page.article.summary {
                            p { class: "text-xs text-muted-foreground line-clamp-2 mt-0.5",
                                "{summary}"
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Compact backlinks list (for sidebar)
#[component]
pub fn WikiBacklinksCompact(
    identifier: String,
    #[props(default = 5)] limit: usize,
    #[props(default = String::new())] class: String,
) -> Element {
    let mut loading = use_signal(|| true);
    let mut pages = use_signal(Vec::<CachedWikiPage>::new);
    let mut total = use_signal(|| 0usize);
    use_effect(move || {
        let id = identifier.clone();
        spawn(async move {
            if let Ok(result) = fetch_wiki_backlinks(&id, limit + 1).await {
                total.set(result.len());
                pages.set(result.into_iter().take(limit).collect());
                loading.set(false);
            }
        });
    });
    if *loading.read() || pages.read().is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "wiki-backlinks-compact {class}",
            h5 { class: "text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2",
                "Linked from"
            }
            div { class: "flex flex-wrap gap-1",
                for page in pages.read().iter() {
                    Link {
                        key: "{page.event.id.to_hex()}",
                        class: "inline-flex items-center px-2 py-1 text-xs bg-accent rounded-md hover:bg-accent/80 transition-colors",
                        to: Route::WikiDetail {
                            identifier: page.article.identifier.clone(),
                        },
                        "{page.article.title}"
                    }
                }
                if *total.read() > limit {
                    span { class: "inline-flex items-center px-2 py-1 text-xs text-muted-foreground",
                        "+{*total.read() - limit} more"
                    }
                }
            }
        }
    }
}
/// Backlinks network visualization (graph view)
#[component]
pub fn WikiBacklinksGraph(
    /// Center page identifier
    identifier: String,
    /// Forward links from center page
    forward_links: Vec<String>,
    /// Backward links to center page
    backward_links: Vec<String>,
    #[props(default = String::new())] class: String,
) -> Element {
    rsx! {
        div { class: "wiki-backlinks-graph p-4 bg-card border border-border rounded-lg {class}",
            div { class: "flex items-center gap-4 mb-4 text-xs text-muted-foreground",
                div { class: "flex items-center gap-1",
                    div { class: "w-3 h-3 rounded-full bg-primary" }
                    "Current page"
                }
                div { class: "flex items-center gap-1",
                    div { class: "w-3 h-3 rounded-full bg-green-500" }
                    "Links to ({forward_links.len()})"
                }
                div { class: "flex items-center gap-1",
                    div { class: "w-3 h-3 rounded-full bg-blue-500" }
                    "Links from ({backward_links.len()})"
                }
            }
            div { class: "relative h-48",
                div { class: "absolute left-0 top-0 bottom-0 w-1/3 flex flex-col justify-center gap-1 pr-4",
                    for link in backward_links.iter().take(5) {
                        Link {
                            class: "text-xs px-2 py-1 bg-blue-500/10 text-blue-600 dark:text-blue-400 rounded truncate hover:bg-blue-500/20 transition-colors",
                            to: Route::WikiDetail {
                                identifier: link.clone(),
                            },
                            "{link}"
                        }
                    }
                    if backward_links.len() > 5 {
                        span { class: "text-xs text-muted-foreground", "+{backward_links.len() - 5}" }
                    }
                }
                div { class: "absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2",
                    div { class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg font-medium text-sm shadow-md",
                        "{identifier}"
                    }
                }
                div { class: "absolute right-0 top-0 bottom-0 w-1/3 flex flex-col justify-center gap-1 pl-4",
                    for link in forward_links.iter().take(5) {
                        Link {
                            class: "text-xs px-2 py-1 bg-green-500/10 text-green-600 dark:text-green-400 rounded truncate hover:bg-green-500/20 transition-colors",
                            to: Route::WikiDetail {
                                identifier: link.clone(),
                            },
                            "{link}"
                        }
                    }
                    if forward_links.len() > 5 {
                        span { class: "text-xs text-muted-foreground", "+{forward_links.len() - 5}" }
                    }
                }
            }
        }
    }
}
/// Skeleton loader for backlinks
#[component]
fn BacklinksSkeleton() -> Element {
    rsx! {
        div { class: "space-y-2 animate-pulse",
            for _ in 0..3 {
                div { class: "flex items-start gap-2 p-2",
                    div { class: "w-4 h-4 bg-muted rounded" }
                    div { class: "flex-1",
                        div { class: "h-4 bg-muted rounded w-3/4 mb-1" }
                        div { class: "h-3 bg-muted rounded w-full" }
                    }
                }
            }
        }
    }
}
