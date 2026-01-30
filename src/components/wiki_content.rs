//! Wiki Content Component
//! Renders wiki page content with wikilinks (NIP-54 Kind 30818)

use crate::components::icons::{
    AlertTriangleIcon, ArrowLeftIcon, CheckIcon, CopyIcon, ExternalLinkIcon, PenSquareIcon,
    Repeat2Icon,
};
use crate::components::{AsciiDocContent, CitationMetadata};
use crate::routes::Route;
use crate::stores::wiki_store::CachedWikiPage;
use crate::utils::clipboard::copy_formatted_content;
use dioxus::prelude::*;

/// Wiki page content renderer
#[component]
pub fn WikiPageContent(
    /// The wiki page data
    page: CachedWikiPage,
    /// Show page header
    #[props(default = true)]
    show_header: bool,
    /// Show edit button (if user has permission)
    #[props(default = false)]
    show_edit: bool,
    /// Edit callback
    #[props(default = None)]
    on_edit: Option<EventHandler<()>>,
) -> Element {
    // Copy state
    let mut copied = use_signal(|| false);
    // Citation metadata state
    let mut citation_count = use_signal(|| 0usize);

    // Copy handler
    let handle_copy = {
        let content = page.event.content.clone();
        move |_| {
            let content = content.clone();
            spawn(async move {
                match copy_formatted_content(&content).await {
                    Ok(_) => {
                        copied.set(true);
                        spawn(async move {
                            gloo_timers::future::TimeoutFuture::new(2000).await;
                            copied.set(false);
                        });
                    }
                    Err(e) => log::error!("Failed to copy: {:?}", e),
                }
            });
        }
    };

    rsx! {
        article {
            class: "wiki-page",

            // Page header
            if show_header {
                header {
                    class: "mb-6 pb-4 border-b border-border",
                    div {
                        class: "flex items-start justify-between gap-4",
                        div {
                            h1 {
                                class: "text-3xl font-bold text-foreground",
                                "{page.article.title}"
                            }
                            if page.article.identifier != page.article.title.to_lowercase().replace(' ', "-") {
                                p {
                                    class: "text-sm text-muted-foreground mt-1 font-mono",
                                    "{page.article.identifier}"
                                }
                            }
                        }

                        // Action buttons
                        div {
                            class: "flex items-center gap-2",

                            // Copy button
                            button {
                                class: "flex items-center gap-2 px-3 py-1.5 text-sm border border-border rounded-md hover:bg-accent transition-colors",
                                onclick: handle_copy,
                                if *copied.read() {
                                    CheckIcon { class: "w-4 h-4 text-green-500" }
                                    "Copied!"
                                } else {
                                    CopyIcon { class: "w-4 h-4" }
                                    "Copy"
                                }
                            }

                            // Edit button
                            if show_edit {
                                if let Some(handler) = on_edit {
                                    button {
                                        class: "flex items-center gap-2 px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors",
                                        onclick: move |_| handler.call(()),
                                        PenSquareIcon { class: "w-4 h-4" }
                                        "Edit"
                                    }
                                }
                            }
                        }
                    }

                    // Summary and citation badge
                    div {
                        class: "flex items-center gap-3 mt-2",
                        if let Some(ref summary) = page.article.summary {
                            p {
                                class: "text-muted-foreground flex-1",
                                "{summary}"
                            }
                        }
                        // Citation count badge
                        {
                            let count = *citation_count.read();
                            let suffix = if count > 1 { "s" } else { "" };
                            if count > 0 {
                                rsx! {
                                    span {
                                        class: "inline-flex items-center gap-1 px-2 py-0.5 text-xs bg-purple-500/20 text-purple-600 dark:text-purple-400 rounded-full whitespace-nowrap",
                                        "📚 {count} citation{suffix}"
                                    }
                                }
                            } else {
                                rsx! {}
                            }
                        }
                    }
                }
            }

            // Page content with wikilinks
            AsciiDocContent {
                content: page.event.content.clone(),
                enable_wikilinks: true,
                on_citations_loaded: move |metadata: CitationMetadata| {
                    citation_count.set(metadata.count);
                },
            }

            // Fork/defer information
            if page.article.fork_source.is_some() || page.article.defer_to.is_some() {
                div {
                    class: "mt-8 pt-4 border-t border-border",
                    h3 {
                        class: "text-sm font-semibold text-muted-foreground mb-2",
                        "References"
                    }
                    ul {
                        class: "space-y-1 text-sm",
                        if let Some(ref fork) = page.article.fork_source {
                            li {
                                class: "flex items-center gap-2 text-muted-foreground",
                                Repeat2Icon { class: "w-4 h-4" }
                                span { "Forked from: " }
                                {extract_identifier_from_addr(fork)}
                            }
                        }
                        if let Some(ref defer) = page.article.defer_to {
                            li {
                                class: "flex items-center gap-2 text-muted-foreground",
                                ArrowLeftIcon { class: "w-4 h-4 rotate-180" }
                                span { "Defers to: " }
                                {extract_identifier_from_addr(defer)}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Wiki page outline (table of contents)
#[component]
pub fn WikiOutline(content: String, #[props(default = String::new())] class: String) -> Element {
    let headings = use_memo(move || extract_headings(&content));

    if headings.read().is_empty() {
        return rsx! {};
    }

    rsx! {
        nav {
            class: "wiki-outline {class}",
            h4 {
                class: "text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-3",
                "Contents"
            }
            ul {
                class: "space-y-1",
                for (level, text, id) in headings.read().iter() {
                    li {
                        style: "padding-left: {(*level - 1) * 12}px",
                        a {
                            class: "block text-sm text-muted-foreground hover:text-foreground transition-colors py-0.5",
                            href: "#{id}",
                            "{text}"
                        }
                    }
                }
            }
        }
    }
}

/// Wikilinks found in the page
#[component]
pub fn WikiForwardLinks(
    links: Vec<String>,
    #[props(default = String::new())] class: String,
) -> Element {
    if links.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "wiki-forward-links {class}",
            h4 {
                class: "text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-3",
                "Links to"
            }
            div {
                class: "flex flex-wrap gap-2",
                for link in links.iter() {
                    Link {
                        class: "inline-flex items-center gap-1 px-2 py-1 text-sm bg-accent rounded-md hover:bg-accent/80 transition-colors",
                        to: Route::WikiDetail { identifier: link.clone() },
                        ExternalLinkIcon { class: "w-3 h-3" }
                        "{link}"
                    }
                }
            }
        }
    }
}

/// Empty wiki page placeholder
#[component]
pub fn WikiPageNotFound(
    identifier: String,
    #[props(default = false)] can_create: bool,
    #[props(default = None)] on_create: Option<EventHandler<String>>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col items-center justify-center py-16 px-4 text-center",
            div {
                class: "w-16 h-16 rounded-full bg-muted flex items-center justify-center mb-4",
                AlertTriangleIcon { class: "w-8 h-8 text-muted-foreground" }
            }
            h2 {
                class: "text-xl font-semibold text-foreground mb-2",
                "Page Not Found"
            }
            p {
                class: "text-muted-foreground mb-6 max-w-md",
                "The wiki page \"" span { class: "font-mono", "{identifier}" } "\" doesn't exist yet."
            }
            if can_create {
                if let Some(handler) = on_create {
                    button {
                        class: "flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors",
                        onclick: move |_| handler.call(identifier.clone()),
                        span { class: "text-lg font-bold", "+" }
                        "Create this page"
                    }
                }
            }
        }
    }
}

/// Loading skeleton for wiki page
#[component]
pub fn WikiPageSkeleton() -> Element {
    rsx! {
        article {
            class: "wiki-page animate-pulse",
            header {
                class: "mb-6 pb-4 border-b border-border",
                div { class: "h-9 bg-muted rounded w-2/3 mb-2" }
                div { class: "h-4 bg-muted rounded w-1/3" }
            }
            div {
                class: "space-y-4",
                for _ in 0..8 {
                    div { class: "h-4 bg-muted rounded w-full" }
                }
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-4 bg-muted rounded w-5/6" }
            }
        }
    }
}

/// Extract headings from AsciiDoc content
fn extract_headings(content: &str) -> Vec<(usize, String, String)> {
    let heading_pattern = regex::Regex::new(r"(?m)^(=+)\s+(.+)$").unwrap();

    heading_pattern
        .captures_iter(content)
        .map(|cap| {
            let level = cap.get(1).map(|m| m.as_str().len()).unwrap_or(1);
            let text = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let id = slug_from_text(&text);
            (level, text, id)
        })
        .collect()
}

/// Generate a URL-safe slug from heading text
fn slug_from_text(text: &str) -> String {
    text.chars()
        .filter_map(|c| {
            if c.is_alphanumeric() {
                Some(c.to_lowercase().next().unwrap())
            } else if c.is_whitespace() || c == '-' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Extract identifier from address (kind:pubkey:identifier)
fn extract_identifier_from_addr(addr: &str) -> String {
    // Use splitn(3, ':') to handle identifiers containing colons
    addr.splitn(3, ':').nth(2).unwrap_or(addr).to_string()
}
