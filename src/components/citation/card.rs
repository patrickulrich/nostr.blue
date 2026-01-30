//! Citation Card Component
//! Displays a citation in a card format with type badge, title, author, and metadata
//! Supports Internal (kind 30), External (kind 31), Hardcopy (kind 32), and Prompt (kind 33)
use crate::components::content_menu::{ContentMenu, ContentMenuType};
use crate::components::icons::ExternalLinkIcon;
use crate::stores::citation_store::CachedCitation;
use crate::utils::nkbip03::{Citation, CitationType};
use dioxus::prelude::*;
use url::Url;
/// Styling information for citation types
pub struct CitationStyle {
    pub emoji: &'static str,
    pub label: &'static str,
    pub bg_class: &'static str,
    pub text_class: &'static str,
}
impl CitationStyle {
    /// Combined class for badge display (bg + text)
    pub fn badge_class(&self) -> String {
        format!("{} {}", self.bg_class, self.text_class)
    }
}
/// Get styling for a citation type
pub fn get_citation_style(citation_type: &CitationType) -> CitationStyle {
    match citation_type {
        CitationType::Internal => {
            CitationStyle {
                emoji: "📌",
                label: "Nostr",
                bg_class: "bg-purple-500/20",
                text_class: "text-purple-600 dark:text-purple-400",
            }
        }
        CitationType::ExternalWeb => {
            CitationStyle {
                emoji: "🌐",
                label: "Web",
                bg_class: "bg-blue-500/20",
                text_class: "text-blue-600 dark:text-blue-400",
            }
        }
        CitationType::Hardcopy => {
            CitationStyle {
                emoji: "📖",
                label: "Book",
                bg_class: "bg-amber-500/20",
                text_class: "text-amber-600 dark:text-amber-400",
            }
        }
        CitationType::Prompt => {
            CitationStyle {
                emoji: "🤖",
                label: "AI",
                bg_class: "bg-emerald-500/20",
                text_class: "text-emerald-600 dark:text-emerald-400",
            }
        }
    }
}
/// Citation type badge component
#[component]
pub fn CitationTypeBadge(citation_type: CitationType) -> Element {
    let style = get_citation_style(&citation_type);
    let badge_class = style.badge_class();
    rsx! {
        span { class: "inline-flex items-center gap-1 px-2 py-0.5 text-xs font-medium rounded-full {badge_class}",
            "{style.emoji}"
            "{style.label}"
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
    let preview = if content_preview.chars().count() > 120 {
        let truncated: String = content_preview.chars().take(120).collect();
        format!("{}...", truncated.trim())
    } else {
        content_preview
    };
    let type_specific_info = match &citation.citation {
        Citation::ExternalWeb(c) => Some(("URL", c.url.clone())),
        Citation::Hardcopy(c) => c.doi.clone().map(|d| ("DOI", d)),
        Citation::Prompt(c) => Some(("LLM", c.llm.clone())),
        Citation::Internal(c) => Some(("Ref", c.coordinate.clone())),
    };
    let citation_for_click = citation.clone();
    let citation_for_key = citation.clone();
    let is_clickable = on_click.is_some();
    let cursor_class = if is_clickable { "cursor-pointer" } else { "" };
    let tabindex_val = if is_clickable { "0" } else { "-1" };
    let role_val = if is_clickable { "button" } else { "" };
    rsx! {
        div {
            class: "group relative bg-card rounded-lg border border-border overflow-hidden hover:border-primary/50 transition-all duration-200 hover:shadow-lg {cursor_class}",
            tabindex: "{tabindex_val}",
            role: "{role_val}",
            onclick: move |_| {
                if let Some(handler) = on_click {
                    handler.call(citation_for_click.clone());
                }
            },
            onkeydown: move |evt: KeyboardEvent| {
                let is_activation_key = match evt.key() {
                    Key::Enter => true,
                    Key::Character(ref ch) if ch == " " => true,
                    _ => false,
                };
                if on_click.is_some() && is_activation_key {
                    evt.prevent_default();
                    if let Some(handler) = on_click {
                        handler.call(citation_for_key.clone());
                    }
                }
            },
            div {
                class: "absolute top-2 right-2 z-20 opacity-0 group-hover:opacity-100 transition-opacity",
                onclick: move |e| e.stop_propagation(),
                ContentMenu {
                    content_type: ContentMenuType::Citation,
                    author_pubkey: author_pubkey.clone(),
                    naddr: naddr.clone(),
                    event_id: Some(event_id.clone()),
                }
            }
            div { class: "p-4",
                div { class: "flex items-start justify-between gap-2 mb-2",
                    CitationTypeBadge { citation_type: citation_type.clone() }
                    if let Some(ref date) = accessed_on {
                        span { class: "text-xs text-muted-foreground", "Accessed: {date}" }
                    }
                }
                h3 { class: "text-base font-semibold line-clamp-2 group-hover:text-primary transition-colors mb-1",
                    "{title}"
                }
                if !author.is_empty() {
                    p { class: "text-sm text-muted-foreground mb-2", "by {author}" }
                }
                if !preview.is_empty() {
                    p { class: "text-sm text-muted-foreground/80 line-clamp-3 mb-3 italic",
                        "\"{preview}\""
                    }
                }
                if let Some((label, value)) = type_specific_info {
                    div { class: "flex items-center gap-2 text-xs text-muted-foreground pt-2 border-t border-border",
                        span { class: "font-medium", "{label}:" }
                        span { class: "truncate", "{value}" }
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
    let secondary_info = match &citation.citation {
        Citation::ExternalWeb(c) => {
            Url::parse(&c.url)
                .or_else(|_| Url::parse(&format!("https://{}", c.url)))
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_string()))
        }
        Citation::Hardcopy(c) => c.published_by.clone(),
        Citation::Prompt(c) => Some(c.llm.clone()),
        Citation::Internal(_) => Some("Nostr".to_string()),
    };
    let citation_for_click = citation.clone();
    let citation_for_key = citation.clone();
    let is_clickable = on_click.is_some();
    let cursor_class = if is_clickable { "cursor-pointer" } else { "" };
    let tabindex_val = if is_clickable { "0" } else { "-1" };
    let role_val = if is_clickable { "button" } else { "" };
    rsx! {
        div {
            class: "flex items-center gap-3 p-3 rounded-lg border border-border hover:bg-accent/50 transition-colors {cursor_class}",
            tabindex: "{tabindex_val}",
            role: "{role_val}",
            onclick: move |_| {
                if let Some(handler) = on_click {
                    handler.call(citation_for_click.clone());
                }
            },
            onkeydown: move |evt: KeyboardEvent| {
                let is_activation_key = match evt.key() {
                    Key::Enter => true,
                    Key::Character(ref ch) if ch == " " => true,
                    _ => false,
                };
                if on_click.is_some() && is_activation_key {
                    evt.prevent_default();
                    if let Some(handler) = on_click {
                        handler.call(citation_for_key.clone());
                    }
                }
            },
            {
                let style = get_citation_style(&citation_type);
                rsx! {
                    div { class: "shrink-0 w-8 h-8 rounded-full flex items-center justify-center {style.bg_class}",
                        span { class: "text-sm", "{style.emoji}" }
                    }
                }
            }
            div { class: "flex-1 min-w-0",
                p { class: "text-sm font-medium truncate", "{short_display}" }
                if let Some(ref info) = secondary_info {
                    p { class: "text-xs text-muted-foreground truncate", "{info}" }
                }
            }
            if !is_clickable {
                span { class: "p-1 text-muted-foreground",
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
    let citation_for_click = citation.clone();
    let citation_for_key = citation.clone();
    let is_clickable = on_click.is_some();
    let cursor_class = if is_clickable { "cursor-pointer" } else { "" };
    let tabindex_val = if is_clickable { "0" } else { "-1" };
    let role_val = if is_clickable { "button" } else { "" };
    rsx! {
        span {
            class: "inline-flex items-center gap-1 px-2 py-1 text-xs rounded-full border border-border hover:bg-accent transition-colors {cursor_class}",
            tabindex: "{tabindex_val}",
            role: "{role_val}",
            onclick: move |_| {
                if let Some(handler) = on_click {
                    handler.call(citation_for_click.clone());
                }
            },
            onkeydown: move |evt: KeyboardEvent| {
                let is_activation_key = match evt.key() {
                    Key::Enter => true,
                    Key::Character(ref ch) if ch == " " => true,
                    _ => false,
                };
                if on_click.is_some() && is_activation_key {
                    evt.prevent_default();
                    if let Some(handler) = on_click {
                        handler.call(citation_for_key.clone());
                    }
                }
            },
            span { {get_citation_style(&citation_type).emoji} }
            span { class: "truncate max-w-[150px]", "{short_display}" }
        }
    }
}
/// Skeleton loader for citation cards
#[component]
pub fn CitationCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card rounded-lg border border-border overflow-hidden animate-pulse",
            div { class: "p-4 space-y-3",
                div { class: "flex items-center justify-between",
                    div { class: "h-5 w-16 bg-muted rounded-full" }
                    div { class: "h-4 w-24 bg-muted rounded" }
                }
                div { class: "h-5 bg-muted rounded w-full" }
                div { class: "h-5 bg-muted rounded w-3/4" }
                div { class: "h-4 bg-muted rounded w-1/3" }
                div { class: "space-y-1 pt-2",
                    div { class: "h-3 bg-muted rounded w-full" }
                    div { class: "h-3 bg-muted rounded w-full" }
                    div { class: "h-3 bg-muted rounded w-2/3" }
                }
                div { class: "flex items-center gap-2 pt-3 border-t border-border",
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
        div { class: "flex items-center gap-3 p-3 rounded-lg border border-border animate-pulse",
            div { class: "w-8 h-8 rounded-full bg-muted" }
            div { class: "flex-1 space-y-1",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
            }
            div { class: "w-4 h-4 bg-muted rounded" }
        }
    }
}
