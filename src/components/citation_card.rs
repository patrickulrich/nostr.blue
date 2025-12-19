//! Citation Card Component
//! Displays a citation in a card format with type badge, title, author, and metadata
//! Supports Internal (kind 30), External (kind 31), Hardcopy (kind 32), and Prompt (kind 33)

use dioxus::prelude::*;
use crate::stores::citation_store::CachedCitation;
use crate::utils::nkbip03::{Citation, CitationType};
use crate::components::icons::ExternalLinkIcon;
use crate::components::content_menu::{ContentMenu, ContentMenuType};

/// Citation type badge component
#[component]
pub fn CitationTypeBadge(citation_type: CitationType) -> Element {
    let (icon, label, color_class) = match citation_type {
        CitationType::Internal => ("📌", "Nostr", "bg-purple-500/20 text-purple-600 dark:text-purple-400"),
        CitationType::ExternalWeb => ("🌐", "Web", "bg-blue-500/20 text-blue-600 dark:text-blue-400"),
        CitationType::Hardcopy => ("📖", "Book", "bg-amber-500/20 text-amber-600 dark:text-amber-400"),
        CitationType::Prompt => ("🤖", "AI", "bg-emerald-500/20 text-emerald-600 dark:text-emerald-400"),
    };

    rsx! {
        span {
            class: "inline-flex items-center gap-1 px-2 py-0.5 text-xs font-medium rounded-full {color_class}",
            "{icon}"
            "{label}"
        }
    }
}

/// Citation card for grid/list display
#[component]
pub fn CitationCard(
    citation: CachedCitation,
    /// Optional click handler (when provided, card becomes clickable)
    #[props(default = None)]
    on_click: Option<EventHandler<CachedCitation>>,
) -> Element {
    let base = citation.citation.base();
    let citation_type = citation.citation.citation_type();
    let title = base.title.clone();
    let author = base.author.clone();
    let content_preview = base.content.clone();
    let accessed_on = base.accessed_on.clone();
    let naddr = citation.naddr.clone().unwrap_or_default();
    let event_id = citation.event.id.to_hex();
    let author_pubkey = citation.event.pubkey.to_hex();
    let coordinate = citation.a_tag.clone();

    // Use Citation::short_display() for compact display (USES PREVIOUSLY UNUSED FUNCTION)
    let _short_display = citation.citation.short_display();

    // Truncate content preview
    let preview = if content_preview.chars().count() > 120 {
        let truncated: String = content_preview.chars().take(120).collect();
        format!("{}...", truncated.trim())
    } else {
        content_preview
    };

    // Type-specific metadata
    let type_specific_info = match &citation.citation {
        Citation::ExternalWeb(c) => Some(("URL", c.url.clone())),
        Citation::Hardcopy(c) => c.doi.clone().map(|d| ("DOI", d)),
        Citation::Prompt(c) => Some(("LLM", c.llm.clone())),
        Citation::Internal(c) => Some(("Ref", c.coordinate.clone())),
    };

    let citation_clone = citation.clone();
    let handle_click = move |_| {
        if let Some(ref handler) = on_click {
            handler.call(citation_clone.clone());
        }
    };

    let is_clickable = on_click.is_some();
    let cursor_class = if is_clickable { "cursor-pointer" } else { "" };

    rsx! {
        div {
            class: "group relative bg-card rounded-lg border border-border overflow-hidden hover:border-primary/50 transition-all duration-200 hover:shadow-lg {cursor_class}",
            onclick: handle_click,

            // Menu button (top-right)
            div {
                class: "absolute top-2 right-2 z-20 opacity-0 group-hover:opacity-100 transition-opacity",
                onclick: move |e| e.stop_propagation(),
                ContentMenu {
                    content_type: ContentMenuType::Citation,
                    author_pubkey: author_pubkey.clone(),
                    naddr: naddr.clone(),
                    event_id: Some(event_id.clone()),
                    coordinate: coordinate,
                }
            }

            // Card content
            div {
                class: "p-4",

                // Header row with type badge
                div {
                    class: "flex items-start justify-between gap-2 mb-2",
                    CitationTypeBadge { citation_type: citation_type.clone() }
                    if let Some(ref date) = accessed_on {
                        span {
                            class: "text-xs text-muted-foreground",
                            "Accessed: {date}"
                        }
                    }
                }

                // Title
                h3 {
                    class: "text-base font-semibold line-clamp-2 group-hover:text-primary transition-colors mb-1",
                    "{title}"
                }

                // Author
                if !author.is_empty() {
                    p {
                        class: "text-sm text-muted-foreground mb-2",
                        "by {author}"
                    }
                }

                // Content preview
                if !preview.is_empty() {
                    p {
                        class: "text-sm text-muted-foreground/80 line-clamp-3 mb-3 italic",
                        "\"{preview}\""
                    }
                }

                // Type-specific info footer
                if let Some((label, value)) = type_specific_info {
                    div {
                        class: "flex items-center gap-2 text-xs text-muted-foreground pt-2 border-t border-border",
                        span {
                            class: "font-medium",
                            "{label}:"
                        }
                        span {
                            class: "truncate",
                            "{value}"
                        }
                    }
                }
            }
        }
    }
}

/// Compact citation card for inline lists
#[component]
pub fn CitationCardCompact(
    citation: CachedCitation,
    /// Click handler for selection mode
    #[props(default = None)]
    on_click: Option<EventHandler<CachedCitation>>,
) -> Element {
    let citation_type = citation.citation.citation_type();
    let short_display = citation.citation.short_display();

    // Type-specific secondary info
    let secondary_info = match &citation.citation {
        Citation::ExternalWeb(c) => {
            // Extract domain from URL
            c.url.split("//").nth(1)
                .and_then(|s| s.split('/').next())
                .map(|s| s.to_string())
        },
        Citation::Hardcopy(c) => c.published_by.clone(),
        Citation::Prompt(c) => Some(c.llm.clone()),
        Citation::Internal(_) => Some("Nostr".to_string()),
    };

    let citation_clone = citation.clone();
    let handle_click = move |_| {
        if let Some(ref handler) = on_click {
            handler.call(citation_clone.clone());
        }
    };

    let is_clickable = on_click.is_some();
    let cursor_class = if is_clickable { "cursor-pointer" } else { "" };

    rsx! {
        div {
            class: "flex items-center gap-3 p-3 rounded-lg border border-border hover:bg-accent/50 transition-colors {cursor_class}",
            onclick: handle_click,

            // Type icon
            {
                let bg_class = match citation_type {
                    CitationType::Internal => "bg-purple-500/20",
                    CitationType::ExternalWeb => "bg-blue-500/20",
                    CitationType::Hardcopy => "bg-amber-500/20",
                    CitationType::Prompt => "bg-emerald-500/20",
                };
                let emoji = match citation_type {
                    CitationType::Internal => "📌",
                    CitationType::ExternalWeb => "🌐",
                    CitationType::Hardcopy => "📖",
                    CitationType::Prompt => "🤖",
                };
                rsx! {
                    div {
                        class: "flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center {bg_class}",
                        span {
                            class: "text-sm",
                            "{emoji}"
                        }
                    }
                }
            }

            // Content
            div {
                class: "flex-1 min-w-0",

                // Short display (Author, Title)
                p {
                    class: "text-sm font-medium truncate",
                    "{short_display}"
                }

                // Secondary info (domain, publisher, LLM)
                if let Some(ref info) = secondary_info {
                    p {
                        class: "text-xs text-muted-foreground truncate",
                        "{info}"
                    }
                }
            }

            // Link indicator when not in selection mode
            if !is_clickable {
                span {
                    class: "p-1 text-muted-foreground",
                    ExternalLinkIcon { class: "w-4 h-4".to_string() }
                }
            }
        }
    }
}

/// Mini citation badge for inline display
#[component]
pub fn CitationBadge(
    citation: CachedCitation,
    /// Optional click handler
    #[props(default = None)]
    on_click: Option<EventHandler<CachedCitation>>,
) -> Element {
    let citation_type = citation.citation.citation_type();
    let short_display = citation.citation.short_display();

    let citation_clone = citation.clone();
    let handle_click = move |_| {
        if let Some(ref handler) = on_click {
            handler.call(citation_clone.clone());
        }
    };

    let cursor_class = if on_click.is_some() { "cursor-pointer" } else { "" };

    rsx! {
        span {
            class: "inline-flex items-center gap-1 px-2 py-1 text-xs rounded-full border border-border hover:bg-accent transition-colors {cursor_class}",
            onclick: handle_click,

            span {
                {match citation_type {
                    CitationType::Internal => "📌",
                    CitationType::ExternalWeb => "🌐",
                    CitationType::Hardcopy => "📖",
                    CitationType::Prompt => "🤖",
                }}
            }

            span {
                class: "truncate max-w-[150px]",
                "{short_display}"
            }
        }
    }
}

/// Skeleton loader for citation cards
#[component]
pub fn CitationCardSkeleton() -> Element {
    rsx! {
        div {
            class: "bg-card rounded-lg border border-border overflow-hidden animate-pulse",

            div {
                class: "p-4 space-y-3",

                // Header skeleton (badge + date)
                div {
                    class: "flex items-center justify-between",
                    div { class: "h-5 w-16 bg-muted rounded-full" }
                    div { class: "h-4 w-24 bg-muted rounded" }
                }

                // Title skeleton
                div { class: "h-5 bg-muted rounded w-full" }
                div { class: "h-5 bg-muted rounded w-3/4" }

                // Author skeleton
                div { class: "h-4 bg-muted rounded w-1/3" }

                // Preview skeleton
                div {
                    class: "space-y-1 pt-2",
                    div { class: "h-3 bg-muted rounded w-full" }
                    div { class: "h-3 bg-muted rounded w-full" }
                    div { class: "h-3 bg-muted rounded w-2/3" }
                }

                // Footer skeleton
                div {
                    class: "flex items-center gap-2 pt-3 border-t border-border",
                    div { class: "h-3 w-8 bg-muted rounded" }
                    div { class: "h-3 w-32 bg-muted rounded" }
                }
            }
        }
    }
}

/// Skeleton loader for compact cards
#[component]
pub fn CitationCardCompactSkeleton() -> Element {
    rsx! {
        div {
            class: "flex items-center gap-3 p-3 rounded-lg border border-border animate-pulse",

            // Icon skeleton
            div { class: "w-8 h-8 rounded-full bg-muted" }

            // Content skeleton
            div {
                class: "flex-1 space-y-1",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
            }

            // Action skeleton
            div { class: "w-4 h-4 bg-muted rounded" }
        }
    }
}
